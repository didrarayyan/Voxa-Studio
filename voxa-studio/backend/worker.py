"""Persistent JSON-lines voice synthesis sidecar for Voxa Studio."""
from __future__ import annotations

import ctypes
import gc
import logging
import os
import sys
import tempfile
import threading
import time
import traceback
import warnings
from pathlib import Path

os.environ.setdefault("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")
os.environ.setdefault("HF_HUB_VERBOSITY", "error")
os.environ.setdefault("TQDM_MININTERVAL", "0.1")
warnings.filterwarnings("ignore", message=".*symlinks.*", category=UserWarning)
warnings.filterwarnings("ignore", message=".*unauthenticated.*", category=UserWarning)
logging.getLogger("huggingface_hub").setLevel(logging.ERROR)

# Redirect stdout to stderr BEFORE importing kittenml/torch/hf so stray print()
# statements or progress output never corrupt the JSON-lines protocol on stdout.
_PROTOCOL_OUT = sys.stdout
sys.stdout = sys.stderr
_WRITE_LOCK = threading.Lock()


def emit(payload: dict) -> None:
    import json
    line = json.dumps(payload, ensure_ascii=False)
    with _WRITE_LOCK:
        _PROTOCOL_OUT.write(line + "\n")
        _PROTOCOL_OUT.flush()


import re
import numpy as np
import soundfile as sf

from kittenml import KittenTTS
from kittenml.repo import KITTEN2_TINY_TYPE, KITTEN2_TYPE, ONNX_TYPES, resolve_repo

try:
    from kittenml.kittentts2.text import trim_silence as _k2_trim_silence
except Exception:
    _k2_trim_silence = None


class _NullTqdmStream:
    def write(self, _s: str) -> int:
        return 0

    def flush(self) -> None:
        pass

    def isatty(self) -> bool:
        return False


_NULL_TQDM_STREAM = _NullTqdmStream()
_DOWNLOAD_HOOK_INSTALLED = False


def _install_hf_download_hook() -> None:
    """Capture Hugging Face / tqdm byte downloads into clean download_progress JSON events."""
    global _DOWNLOAD_HOOK_INSTALLED
    if _DOWNLOAD_HOOK_INSTALLED:
        return
    try:
        import tqdm.std as tqdm_std
    except Exception:
        return

    orig_init = tqdm_std.tqdm.__init__
    orig_update = tqdm_std.tqdm.update
    lock = threading.Lock()
    last_global_emit = {"t": 0.0}

    def _wrapped_init(self, *args, **kwargs):
        unit = kwargs.get("unit", "")
        is_byte_bar = unit in ("B", "bytes") or bool(kwargs.get("unit_scale", False))
        if is_byte_bar:
            kwargs["disable"] = False
            kwargs.setdefault("mininterval", 0.1)
            if "file" not in kwargs or kwargs["file"] is None:
                kwargs["file"] = _NULL_TQDM_STREAM
        orig_init(self, *args, **kwargs)
        actual_unit = getattr(self, "unit", unit)
        self._kitten_is_byte_bar = bool(
            is_byte_bar or actual_unit in ("B", "bytes") or getattr(self, "unit_scale", False)
        )
        self._kitten_start_t = time.monotonic()
        self._kitten_last_emit_t = 0.0
        self._kitten_last_n = int(getattr(self, "n", 0) or 0)
        self._kitten_speed_bps = 0.0

    def _wrapped_update(self, n=1):
        res = orig_update(self, n)
        if not getattr(self, "_kitten_is_byte_bar", False):
            return res
        now = time.monotonic()
        cur_n = int(getattr(self, "n", 0) or 0)
        total = int(getattr(self, "total", 0) or 0)
        done = total > 0 and cur_n >= total
        prev_t = getattr(self, "_kitten_last_emit_t", 0.0)
        if not done and (now - prev_t) < 0.10:
            return res
        with lock:
            if not done and (now - last_global_emit["t"]) < 0.08:
                return res
            dt = max(1e-3, now - (prev_t or getattr(self, "_kitten_start_t", now)))
            dn = max(0, cur_n - int(getattr(self, "_kitten_last_n", 0) or 0))
            inst_speed = dn / dt
            prev_speed = float(getattr(self, "_kitten_speed_bps", 0.0) or 0.0)
            ema_speed = inst_speed if prev_speed <= 0.0 else (0.35 * inst_speed + 0.65 * prev_speed)
            self._kitten_speed_bps = ema_speed
            self._kitten_last_emit_t = now
            self._kitten_last_n = cur_n
            last_global_emit["t"] = now

            eta_s = ((total - cur_n) / ema_speed) if (total > cur_n and ema_speed > 1.0) else None
            raw_desc = str(getattr(self, "desc", "") or "Model weights").strip().rstrip(":")
            file_name = raw_desc.split("/")[-1].split("\\")[-1] if ("/" in raw_desc or "\\" in raw_desc) else raw_desc
            emit({
                "event": "download_progress",
                "file_name": file_name or "Model weights",
                "downloaded_bytes": cur_n,
                "total_bytes": total,
                "speed_bps": float(ema_speed),
                "eta_s": float(eta_s) if eta_s is not None else None,
            })
        return res

    tqdm_std.tqdm.__init__ = _wrapped_init
    tqdm_std.tqdm.update = _wrapped_update
    _DOWNLOAD_HOOK_INSTALLED = True


_install_hf_download_hook()


LANGUAGE_PREFIXES = (
    "Arabic",
    "Chinese",
    "Dutch",
    "French",
    "German",
    "Hindi",
    "Indonesian",
    "Italian",
    "Japanese",
    "Korean",
    "Polish",
    "Portuguese",
    "Russian",
    "Spanish",
    "Turkish",
)

MAX_REFERENCE_S = 30.0
CLIPPED_REFERENCE_S = 20.0

CURRENT_MODEL = None
CURRENT_FAMILY = None
CURRENT_MODEL_NAME = None
CURRENT_WEIGHTS = None
CURRENT_DECODER = None


class _PROCESS_MEMORY_COUNTERS(ctypes.Structure):
    _fields_ = [
        ("cb", ctypes.c_uint32),
        ("PageFaultCount", ctypes.c_uint32),
        ("PeakWorkingSetSize", ctypes.c_size_t),
        ("WorkingSetSize", ctypes.c_size_t),
        ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
        ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
        ("PagefileUsage", ctypes.c_size_t),
        ("PeakPagefileUsage", ctypes.c_size_t),
    ]


def _win32_k32():
    k32 = ctypes.windll.kernel32
    k32.GetCurrentProcess.restype = ctypes.c_void_p
    k32.K32GetProcessMemoryInfo.argtypes = [
        ctypes.c_void_p,
        ctypes.POINTER(_PROCESS_MEMORY_COUNTERS),
        ctypes.c_uint32,
    ]
    k32.K32GetProcessMemoryInfo.restype = ctypes.c_bool
    k32.SetProcessWorkingSetSize.argtypes = [
        ctypes.c_void_p,
        ctypes.c_size_t,
        ctypes.c_size_t,
    ]
    k32.SetProcessWorkingSetSize.restype = ctypes.c_bool
    return k32


def get_process_ram_mb() -> float:
    """Return current committed private memory in MiB on Windows."""
    if sys.platform == "win32":
        try:
            k32 = _win32_k32()
            counters = _PROCESS_MEMORY_COUNTERS()
            counters.cb = ctypes.sizeof(counters)
            proc = k32.GetCurrentProcess()
            if k32.K32GetProcessMemoryInfo(proc, ctypes.byref(counters), counters.cb):
                val = counters.PagefileUsage or counters.WorkingSetSize
                return float(val) / (1024.0 * 1024.0)
        except Exception:
            pass
    return 0.0


def _trim_cache_dict(cache: object, keep_last: int = 1) -> None:
    if not isinstance(cache, dict):
        return
    if keep_last <= 0:
        cache.clear()
        return
    while len(cache) > keep_last:
        try:
            oldest = next(iter(cache))
            cache.pop(oldest, None)
        except Exception:
            break


def _unload_transient_helpers(model: object) -> None:
    if model is None:
        return
    transcriber = getattr(model, "transcriber", None)
    if transcriber is not None:
        inner = getattr(transcriber, "_inner", transcriber)
        if hasattr(inner, "_pipeline"):
            inner._pipeline = None
    encoder = getattr(model, "speaker_encoder", None)
    if encoder is not None:
        inner = getattr(encoder, "_inner", encoder)
        if hasattr(inner, "_model"):
            inner._model = None


def _release_memory(trim_working_set: bool = True, clear_all_caches: bool = False) -> None:
    """Release transient Whisper/encoder submodels, bound caches, and trim Windows heap."""
    if CURRENT_MODEL is not None and CURRENT_FAMILY == "kitten2":
        _unload_transient_helpers(CURRENT_MODEL)
        keep = 0 if clear_all_caches else 1
        _trim_cache_dict(getattr(CURRENT_MODEL, "_reference_cache", None), keep_last=keep)
        codec = getattr(CURRENT_MODEL, "codec", None)
        if codec is not None:
            _trim_cache_dict(getattr(codec, "_ref_cache", None), keep_last=keep)

    gc.collect()
    try:
        import torch
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
    except Exception:
        pass

    if sys.platform == "win32":
        for crt_name in ("ucrtbase", "msvcrt"):
            try:
                ctypes.CDLL(crt_name)._heapmin()
            except Exception:
                pass
        if trim_working_set:
            try:
                k32 = _win32_k32()
                proc = k32.GetCurrentProcess()
                k32.SetProcessWorkingSetSize(proc, ctypes.c_size_t(-1), ctypes.c_size_t(-1))
            except Exception:
                pass


class _AutoUnloadTranscriber:
    """Proxies ReferenceTranscriber so Whisper weights unload right after transcribing."""

    def __init__(self, inner) -> None:
        self._inner = inner

    def __call__(self, wav_path):
        try:
            return self._inner(wav_path)
        finally:
            self._inner._pipeline = None
            _release_memory(trim_working_set=True)

    def __getattr__(self, name):
        return getattr(self._inner, name)


class _AutoUnloadSpeakerEncoder:
    """Proxies SpeakerEncoder so its embedding model unloads right after encoding."""

    def __init__(self, inner) -> None:
        self._inner = inner

    def __call__(self, wav_path):
        try:
            return self._inner(wav_path)
        finally:
            self._inner._model = None
            _release_memory(trim_working_set=True)

    def __getattr__(self, name):
        return getattr(self._inner, name)


_K2_LOW_RAM_PATCHED = False


def _iter_k2_lm_tensors(repo_dir: Path, config: dict, variant: str, lm_dir: Path):
    """Yield (name, tensor) one tensor at a time to avoid duplicating 1.7B weights in RAM."""
    if variant == "full":
        from safetensors import safe_open
        from kittenml.kittentts2.weights import plain_shards

        resolved_dir, shards = plain_shards(repo_dir, config, lm_dir)
        if not shards:
            raise FileNotFoundError(f"no plain model weights in {resolved_dir}")
        for shard in shards:
            with safe_open(str(shard), framework="pt") as handle:
                for key in handle.keys():
                    yield key, handle.get_tensor(key)
        return

    from kittenml.kittentts2.weights import iter_lm_weights

    yield from iter_lm_weights(repo_dir, config, variant)


def _install_low_ram_kittentts2_loader() -> None:
    """Patch KittenTTS2._load_language_model to build on 'meta' and stream weights once."""
    global _K2_LOW_RAM_PATCHED
    if _K2_LOW_RAM_PATCHED:
        return

    from kittenml.kittentts2 import model as k2_model_mod

    orig_loader = k2_model_mod.KittenTTS2._load_language_model

    def _streaming_load_language_model(self) -> None:
        # Only stream when using Torch/CPU or CUDA directly; fall back on unexpected setups.
        model = None
        try:
            import torch
            from transformers import AutoConfig, AutoModelForCausalLM, AutoTokenizer

            lm_dir = self.repo_dir / self.config.get("lm_dir", "lm")
            if not lm_dir.is_dir():
                raise FileNotFoundError(f"language model directory not found: {lm_dir}")

            self.dtype = torch.bfloat16 if self.config.get("dtype", "bf16") == "bf16" else torch.float32
            self.tokenizer = AutoTokenizer.from_pretrained(str(lm_dir))
            lm_config = AutoConfig.from_pretrained(str(lm_dir))

            for attention in ("flash_attention_2", "sdpa"):
                try:
                    with torch.device("meta"):
                        model = AutoModelForCausalLM.from_config(
                            lm_config, dtype=self.dtype, attn_implementation=attention
                        )
                    break
                except Exception:
                    if attention == "sdpa":
                        raise

            inner_model = getattr(model, "model", None)
            rotary = getattr(inner_model, "rotary_emb", None)
            if rotary is not None:
                with torch.device(self.device):
                    inner_model.rotary_emb = type(rotary)(config=lm_config)

            expected_keys = {n for n, _ in model.named_parameters()} | {
                n for n, _ in model.named_buffers()
            }
            loaded_keys: set[str] = set()
            spk_state: dict = {}

            with k2_model_mod._quiet_unexpected_keys():
                for name, tensor in _iter_k2_lm_tensors(
                    self.repo_dir, self.config, self.weights, lm_dir
                ):
                    if name.startswith("spk_proj."):
                        spk_state[name] = tensor
                        continue
                    if name in expected_keys:
                        model.load_state_dict(
                            {name: tensor.to(device=self.device)},
                            strict=False,
                            assign=True,
                        )
                        loaded_keys.add(name)
                    del tensor

                model.tie_weights()

            unfilled = [
                n for n, _ in model.named_parameters()
                if n not in loaded_keys and n != "lm_head.weight"
            ]
            if unfilled:
                raise ValueError(f"weights missing from the checkpoint: {unfilled[:5]}")

            meta_left = [
                n for n, t in list(model.named_parameters()) + list(model.named_buffers())
                if t.device.type == "meta"
            ]
            if meta_left:
                raise RuntimeError(f"meta tensors left after streaming load: {meta_left[:5]}")

            k2_model_mod.attach_speaker_head(
                model,
                str(lm_dir),
                self.device,
                self.dtype,
                spk_dim=self.config.get(
                    "speaker_embedding_dim", k2_model_mod.SPEAKER_EMBEDDING_DIM
                ),
                state=spk_state,
            )
            spk_state.clear()
            model.eval()
            self.model = model
        except Exception as exc:
            # Only fall back if failure happened before model assignment and wasn't missing weights/files.
            if isinstance(exc, (FileNotFoundError, ValueError)):
                raise
            model = None
            _release_memory(trim_working_set=False)
            orig_loader(self)

    k2_model_mod.KittenTTS2._load_language_model = _streaming_load_language_model
    _K2_LOW_RAM_PATCHED = True


# ── Expression markup, tone presets & DSP shaping ────────────────────────────────────────────
EXPRESSION_TAGS = (
    "mundane", "nervous", "tender", "angry", "excited",
    "stern", "sad", "contemplative", "surprised", "joyful",
)
VOCAL_EVENT_TAGS = (
    "pause", "sigh", "gasp", "laugh", "giggle",
    "sob", "scoff", "growl", "um", "gulp",
)

EMOTION_ALIASES = {t: t for t in EXPRESSION_TAGS}
EMOTION_ALIASES.update({
    "happy": "joyful",
    "cheerful": "joyful",
    "upbeat": "excited",
    "calm": "contemplative",
    "thoughtful": "contemplative",
    "whisper": "tender",
    "gentle": "tender",
    "soft": "tender",
    "serious": "stern",
    "firm": "stern",
    "furious": "angry",
    "mad": "angry",
    "fearful": "nervous",
    "scared": "nervous",
    "anxious": "nervous",
    "shocked": "surprised",
    "astonished": "surprised",
    "neutral": "mundane",
})

EVENT_ALIASES = {t: t for t in VOCAL_EVENT_TAGS}
EVENT_ALIASES.update({
    "chuckle": "laugh",
    "laughter": "laugh",
    "haha": "laugh",
    "hehe": "giggle",
    "breath": "sigh",
    "breathe": "sigh",
    "exhale": "sigh",
    "inhale": "gasp",
    "cry": "sob",
    "crying": "sob",
    "weep": "sob",
    "hmm": "um",
    "uh": "um",
    "er": "um",
    "cough": "scoff",
    "clear_throat": "scoff",
    "swallow": "gulp",
    "wait": "pause",
    "beat": "pause",
})

# Emotion acoustic profile: (speed_mult, pitch_semitones, tilt_coeff, gain, extra_pause_s)
_EMOTION_PROFILES = {
    "joyful": (1.08, 1.3, -0.35, 1.10, 0.0),
    "excited": (1.15, 2.0, -0.45, 1.15, 0.0),
    "tender": (0.90, -0.6, 0.35, 0.92, 0.08),
    "contemplative": (0.91, -0.8, 0.25, 0.95, 0.14),
    "surprised": (1.13, 2.2, -0.40, 1.12, 0.05),
    "nervous": (1.10, 1.4, -0.15, 0.96, 0.04),
    "sad": (0.85, -1.6, 0.50, 0.86, 0.18),
    "stern": (0.93, -1.4, -0.20, 1.12, 0.08),
    "angry": (1.06, -0.7, -0.55, 1.24, 0.04),
    "mundane": (1.00, 0.0, 0.0, 1.00, 0.0),
}

_BRACKET_TAG_RE = re.compile(r"\[([A-Za-z][A-Za-z0-9 _-]{0,22})\]")
_ANGLE_TAG_RE = re.compile(r"<([A-Za-z][A-Za-z0-9 _-]{0,22})>")
_MARKUP_TOKEN_RE = re.compile(
    r"\[([A-Za-z][A-Za-z0-9 _-]{0,22})\]"
    r"|<([A-Za-z][A-Za-z0-9 _-]{0,22})>"
    r"|\(\(\(([^()\n]{1,80})\)\)\)([,.!?;:]*)"
)


def _norm_key(raw: str) -> str:
    return re.sub(r"[\s-]+", "_", raw.strip().lower())


def preprocess_text_for_kitten2(text: str) -> tuple[str, bool]:
    """Canonicalize emotion/event aliases for KittenTTS 2 and carry active emotion across chunks."""
    has_tags = False

    def _sub_bracket(m: re.Match) -> str:
        nonlocal has_tags
        canon = EMOTION_ALIASES.get(_norm_key(m.group(1)))
        if canon:
            has_tags = True
            return f"[{canon}]"
        return m.group(0)

    def _sub_angle(m: re.Match) -> str:
        nonlocal has_tags
        canon = EVENT_ALIASES.get(_norm_key(m.group(1)))
        if canon:
            has_tags = True
            return f"<{canon}>"
        return m.group(0)

    cleaned = _BRACKET_TAG_RE.sub(_sub_bracket, str(text or ""))
    cleaned = _ANGLE_TAG_RE.sub(_sub_angle, cleaned)
    if re.search(r"\(\(\([^()\n]{1,80}\)\)\)", cleaned):
        has_tags = True
    return cleaned, has_tags


def split_kitten2_with_carried_emotion(chunks: list[str], initial_emotion: str | None = None) -> list[str]:
    """Ensure every KittenTTS 2 chunk carries the active leading [emotion] tag."""
    active = initial_emotion
    out = []
    for chunk in chunks:
        s = chunk.strip()
        if not s:
            continue
        m = _BRACKET_TAG_RE.match(s)
        first_canon = EMOTION_ALIASES.get(_norm_key(m.group(1))) if m else None
        if not first_canon and active:
            s = f"[{active}] {s}"
        for bm in _BRACKET_TAG_RE.finditer(s):
            c = EMOTION_ALIASES.get(_norm_key(bm.group(1)))
            if c:
                active = c
        out.append(s)
    return out


def parse_expressive_script(text: str) -> list[dict]:
    """Split script into speakable text clauses and vocal event items with active emotion state."""
    items: list[dict] = []
    current_emotion: str | None = None
    pos = 0
    s = str(text or "")

    def _push_speech(raw_chunk: str, emphasized: bool) -> None:
        cleaned = raw_chunk.strip()
        if cleaned and any(ch.isalnum() for ch in cleaned):
            items.append({
                "type": "speech",
                "text": cleaned,
                "emotion": current_emotion,
                "emphasis": emphasized,
            })

    for m in _MARKUP_TOKEN_RE.finditer(s):
        start, end = m.span()
        before = s[pos:start]
        if before:
            _push_speech(before, False)

        bracket_raw, angle_raw, emph_inner, emph_punct = m.groups()
        if bracket_raw is not None:
            key = _norm_key(bracket_raw)
            canon = EMOTION_ALIASES.get(key)
            if canon:
                current_emotion = canon
            else:
                # Strip unknown clause-start [bracket] stage notes instead of reading them aloud;
                # keep mid-clause inline words if preceded directly by an alphanumeric word.
                prev_char = s[:start].rstrip()[-1:] if s[:start].strip() else ""
                at_clause_start = not prev_char or prev_char in ".!?;:\n"
                if not at_clause_start:
                    _push_speech(bracket_raw, False)
        elif angle_raw is not None:
            key = _norm_key(angle_raw)
            canon = EVENT_ALIASES.get(key)
            if canon:
                items.append({
                    "type": "event",
                    "event": canon,
                    "emotion": current_emotion,
                })
            else:
                # Treat unknown <stage_tag> cleanly as a brief pause rather than pronouncing tag names
                items.append({
                    "type": "event",
                    "event": "pause",
                    "emotion": current_emotion,
                })
        elif emph_inner is not None:
            phrase = f"{emph_inner.strip()}{emph_punct or ''}"
            _push_speech(phrase, True)

        pos = end

    tail = s[pos:]
    if tail:
        _push_speech(tail, False)
    return items


def _apply_spectral_tilt(wav: np.ndarray, tilt: float) -> np.ndarray:
    """Fast 1st-order spectral tilt: positive = warm/soft highs, negative = bright/present highs."""
    if wav.size == 0 or abs(tilt) < 1e-3:
        return wav
    from scipy.signal import lfilter
    if tilt > 0.0:
        # Soft low-pass warm blend (sad/tender/contemplative)
        alpha = min(0.82, 0.25 + tilt * 0.95)
        warm = lfilter([1.0 - alpha], [1.0, -alpha], wav)
        mix = min(0.85, tilt * 1.35)
        return (1.0 - mix) * wav + mix * warm
    # Gentle presence lift without harsh Nyquist spike (joyful/excited/angry/emphasis)
    alpha = 0.55
    low_band = lfilter([1.0 - alpha], [1.0, -alpha], wav)
    high_band = wav - low_band
    return wav + min(0.75, -tilt * 1.15) * high_band


def _trim_edges(wav: np.ndarray, sr: int, keep_s: float = 0.015) -> np.ndarray:
    if _k2_trim_silence is not None:
        return _k2_trim_silence(wav, sr, thresh_db=-44.0, keep_s=keep_s)
    arr = np.asarray(wav, dtype=np.float32).reshape(-1)
    if arr.size < int(sr * 0.04):
        return arr
    hop = max(1, int(sr * 0.01))
    frames = arr[: arr.size // hop * hop].reshape(-1, hop)
    rms = np.sqrt((frames ** 2).mean(axis=1) + 1e-12)
    loud = np.nonzero(rms > 0.008)[0]
    if not loud.size:
        return arr
    keep = int(sr * keep_s)
    return arr[max(0, loud[0] * hop - keep) : min(arr.size, (loud[-1] + 1) * hop + keep)]


def _resample_pitch(wav: np.ndarray, pitch_steps: float) -> np.ndarray:
    """Shift pitch smoothly in ~2 ms via sinc resampling (zero JIT warm-up)."""
    arr = np.asarray(wav, dtype=np.float32).reshape(-1)
    if arr.size < 64 or abs(pitch_steps) < 0.08:
        return arr
    from scipy.signal import resample
    factor = 2.0 ** (float(pitch_steps) / 12.0)
    new_len = max(32, int(round(arr.size / factor)))
    return np.ascontiguousarray(resample(arr, new_len), dtype=np.float32)


def apply_emotion_and_emphasis_dsp(
    wav: np.ndarray,
    sr: int,
    emotion: str | None,
    emphasis: bool = False,
) -> tuple[np.ndarray, float]:
    """Shape pitch, spectral brightness, vibrato/drive, and gain for Tiny/ONNX expressive speech."""
    arr = np.asarray(wav, dtype=np.float32).reshape(-1)
    if arr.size == 0:
        return arr, 0.0

    _, pitch_steps, tilt, gain, extra_pause_s = _EMOTION_PROFILES.get(
        emotion or "mundane", (1.0, 0.0, 0.0, 1.0, 0.0)
    )
    if emphasis:
        pitch_steps += 1.05
        tilt -= 0.28
        gain *= 1.16

    arr = _resample_pitch(arr, pitch_steps)
    arr = _apply_spectral_tilt(arr, tilt).astype(np.float32)

    if emotion == "nervous" and arr.size > 0:
        t = np.arange(arr.size, dtype=np.float32) / float(sr)
        arr = arr * (0.94 + 0.07 * np.sin(2.0 * np.pi * 5.8 * t))
    elif emotion == "angry" and arr.size > 0:
        arr = np.tanh(arr * 1.42) * 0.88

    arr = arr * float(gain)
    peak = float(np.max(np.abs(arr))) if arr.size else 0.0
    if peak > 0.94:
        arr = np.tanh(arr / max(peak, 1e-5) * 1.25) * 0.94
    return np.ascontiguousarray(arr, dtype=np.float32), float(extra_pause_s)


def _synth_shaped_noise(duration_s: float, sr: int, cutoff_hz: float, amp: float, env_pow: float = 1.0) -> np.ndarray:
    from scipy.signal import butter, filtfilt
    n = max(1, int(duration_s * sr))
    rng = np.random.default_rng(17)
    noise = rng.normal(0.0, 1.0, n).astype(np.float32)
    b, a = butter(2, min(0.95, cutoff_hz / (0.5 * sr)), btype="low")
    filtered = filtfilt(b, a, noise).astype(np.float32) if n > 12 else noise
    t = np.linspace(0.0, 1.0, n, dtype=np.float32)
    env = np.sin(np.pi * t) ** env_pow
    peak = float(np.max(np.abs(filtered))) or 1.0
    return np.ascontiguousarray(filtered / peak * env * amp, dtype=np.float32)


def synthesize_event_audio(event: str, emotion: str | None, base_synth_fn, sr: int) -> np.ndarray:
    """Render a non-verbal event cue so <laugh>, <sigh>, <pause>, etc. sound like real events."""
    ev = (event or "pause").lower()
    if ev == "pause":
        return np.zeros(int(0.38 * sr), dtype=np.float32)
    if ev == "gasp":
        breath = _synth_shaped_noise(0.20, sr, cutoff_hz=2600.0, amp=0.16, env_pow=0.8)
        return np.concatenate([breath, np.zeros(int(0.12 * sr), dtype=np.float32)])
    if ev == "gulp":
        pulse = _synth_shaped_noise(0.09, sr, cutoff_hz=650.0, amp=0.12, env_pow=1.5)
        return np.concatenate([np.zeros(int(0.06 * sr), dtype=np.float32), pulse, np.zeros(int(0.14 * sr), dtype=np.float32)])

    presets = {
        "um": ("uhm,", 0.85, 0.0, 0.25, 0.85, 0.14),
        "laugh": ("Ha ha ha!", 1.18, 2.0, -0.35, 1.05, 0.12),
        "giggle": ("Hee hee!", 1.24, 2.8, -0.40, 0.96, 0.12),
        "sigh": ("Haah.", 0.76, -1.8, 0.55, 0.75, 0.18),
        "scoff": ("Hmph,", 0.92, -1.0, -0.15, 0.92, 0.10),
        "sob": ("Ah, huh.", 0.80, -1.4, 0.45, 0.80, 0.18),
        "growl": ("Grrr,", 0.84, -2.8, -0.30, 1.05, 0.10),
    }
    cue_text, spd, pitch_st, tilt, gain, post_pause_s = presets.get(
        ev, ("uhm,", 0.95, 0.0, 0.0, 0.9, 0.14)
    )
    pitch_factor = 2.0 ** (pitch_st / 12.0)
    neural_spd = float(np.clip(spd / pitch_factor, 0.5, 2.0))
    raw = _trim_edges(base_synth_fn(cue_text, neural_spd), sr, keep_s=0.015)
    raw = _resample_pitch(raw, pitch_st)
    raw = _apply_spectral_tilt(raw, tilt).astype(np.float32) * gain
    if ev == "sigh" and raw.size > 0:
        breath = _synth_shaped_noise(raw.size / float(sr), sr, cutoff_hz=1800.0, amp=0.10, env_pow=1.2)
        raw = raw * 0.75 + breath[: raw.size]
    elif ev == "sob" and raw.size > 0:
        t = np.arange(raw.size, dtype=np.float32) / float(sr)
        raw = raw * (0.82 + 0.18 * np.sin(2.0 * np.pi * 6.0 * t))
    fade = min(int(0.012 * sr), raw.size // 4)
    if fade > 0:
        raw[:fade] *= np.linspace(0.0, 1.0, fade, dtype=np.float32)
        raw[-fade:] *= np.linspace(1.0, 0.0, fade, dtype=np.float32)
    raw = np.clip(raw, -0.95, 0.95)
    return np.concatenate([raw, np.zeros(int(post_pause_s * sr), dtype=np.float32)])


def join_expressive_segments(segments: list[tuple[np.ndarray, float, bool]], sr: int) -> np.ndarray:
    """Join expressive clause segments smoothly, keeping tight seams on emphasized words."""
    clean = [(np.asarray(w, dtype=np.float32).reshape(-1), gap_s, trim_tight) for w, gap_s, trim_tight in segments if w is not None and len(w)]
    if not clean:
        return np.zeros(0, dtype=np.float32)
    out = []
    fade = max(1, int(0.008 * sr))
    for i, (seg, gap_s, trim_tight) in enumerate(clean):
        piece = _trim_edges(seg, sr, keep_s=0.008 if trim_tight else 0.018).copy()
        if piece.size > 2 * fade:
            piece[:fade] *= np.linspace(0.0, 1.0, fade, dtype=np.float32)
            piece[-fade:] *= np.linspace(1.0, 0.0, fade, dtype=np.float32)
        out.append(piece)
        if i < len(clean) - 1 and gap_s > 0.004:
            out.append(np.zeros(int(gap_s * sr), dtype=np.float32))
    return np.ascontiguousarray(np.concatenate(out), dtype=np.float32)


def strip_tags_for_preview(text: str) -> tuple[str, list[dict]]:
    """Return spoken text without tags plus span summaries for Preview Spoken Text."""
    items = parse_expressive_script(text)
    spoken_parts = []
    spans = []
    seen = set()
    for m in _MARKUP_TOKEN_RE.finditer(str(text or "")):
        raw = m.group(0)
        if raw in seen:
            continue
        seen.add(raw)
        if m.group(1) is not None:
            canon = EMOTION_ALIASES.get(_norm_key(m.group(1)))
            repl = f"(tone: {canon}, not spoken)" if canon else "(tag stripped)"
            spans.append({"original": raw, "replacement": repl})
        elif m.group(2) is not None:
            canon = EVENT_ALIASES.get(_norm_key(m.group(2))) or "pause"
            spans.append({"original": raw, "replacement": f"(event sound: <{canon}>)"})
        elif m.group(3) is not None:
            spans.append({"original": raw, "replacement": f"{m.group(3).strip()} (emphasized tone)"})
    for it in items:
        if it["type"] == "speech":
            spoken_parts.append(it["text"])
    return " ".join(spoken_parts).strip(), spans


def infer_kitten2_voice_language(voice_name: str) -> str:
    for prefix in LANGUAGE_PREFIXES:
        if voice_name.startswith(prefix):
            return prefix
    return "English"


def detect_family(model) -> str:
    cls_name = type(model).__name__
    if cls_name == "KittenTTS2":
        return "kitten2"
    if cls_name == "KittenTTS2Tiny":
        return "kitten2_tiny"
    return "onnx_legacy"


def build_model_info(model, model_name: str, weights: str | None, decoder: str | None) -> dict:
    family = detect_family(model)
    if family == "kitten2":
        voices = []
        langs_seen = set()
        for vname in model.available_voices:
            spec = model.voices.get(vname, {})
            lang = infer_kitten2_voice_language(vname)
            langs_seen.add(lang)
            voices.append({
                "name": vname,
                "language": lang,
                "description": "",
                "key": vname,
                "has_reference": bool(spec.get("reference") and os.path.isfile(spec["reference"])),
                "transcript": spec.get("transcript", ""),
            })
        ordered_langs = ["English"] + sorted(l for l in langs_seen if l != "English")
        return {
            "model_name": model_name,
            "family": family,
            "backend": getattr(model, "backend", None),
            "device": str(getattr(model, "device", "cpu")),
            "weights": getattr(model, "weights", weights),
            "decoder": getattr(model, "decoder", decoder),
            "sample_rate": int(model.sample_rate),
            "languages": ordered_langs,
            "default_language": "English",
            "default_voice": model.config.get("default_voice") or (voices[0]["name"] if voices else None),
            "voices": voices,
            "supports_cloning": True,
            "supports_expression": True,
            "supports_speed": False,
            "supports_phonemize": False,
            "supports_normalize": True,
            "supports_voice_sample": True,
            "available_decoders": list(model.available_decoders),
            "available_weights": list(model.available_weights),
            "decode_presets": dict(model.decode_presets),
            "default_preset": getattr(model, "default_preset", "stable"),
            "advanced_options": dict(model.advanced_options),
            "use_reference_prompt_default": bool(getattr(model, "use_reference_prompt", True)),
        }

    if family == "kitten2_tiny":
        voices = []
        for entry in model.voices():
            voices.append({
                "name": entry.get("name", entry.get("key", "")),
                "language": entry.get("language", "en"),
                "description": entry.get("description", ""),
                "key": entry.get("key", ""),
                "has_reference": False,
                "transcript": "",
            })
        default_lang = getattr(model, "default_language", "en")
        default_voice = next(
            (v["name"] for v in voices if v["language"] == default_lang),
            voices[0]["name"] if voices else None,
        )
        return {
            "model_name": model_name,
            "family": family,
            "backend": getattr(model, "runtime", "onnx"),
            "device": str(getattr(model, "device", None) or "cpu"),
            "weights": getattr(model, "weights", weights),
            "decoder": None,
            "sample_rate": int(model.sample_rate),
            "languages": list(model.languages),
            "default_language": default_lang,
            "default_voice": default_voice,
            "voices": voices,
            "supports_cloning": False,
            "supports_expression": False,
            "supports_speed": True,
            "supports_phonemize": True,
            "supports_normalize": True,
            "supports_voice_sample": False,
            "available_decoders": [],
            "available_weights": list(getattr(model, "onnx_weights", ["int8", "int4"])),
            "decode_presets": {},
            "default_preset": None,
            "advanced_options": {},
            "use_reference_prompt_default": False,
        }

    # onnx_legacy
    voice_names = list(model.available_voices)
    voices = [
        {
            "name": v,
            "language": "English",
            "description": "Classic KittenTTS 0.8 voice",
            "key": v,
            "has_reference": False,
            "transcript": "",
        }
        for v in voice_names
    ]
    return {
        "model_name": model_name,
        "family": family,
        "backend": "onnx",
        "device": "cpu",
        "weights": None,
        "decoder": None,
        "sample_rate": 24000,
        "languages": ["English"],
        "default_language": "English",
        "default_voice": voice_names[0] if voice_names else None,
        "voices": voices,
        "supports_cloning": False,
        "supports_expression": False,
        "supports_speed": True,
        "supports_phonemize": False,
        "supports_normalize": True,
        "supports_voice_sample": False,
        "available_decoders": [],
        "available_weights": [],
        "decode_presets": {},
        "default_preset": None,
        "advanced_options": {},
        "use_reference_prompt_default": False,
    }


def prepare_reference_audio(path: str) -> tuple[str, float, bool]:
    """Read audio duration and clip >30s recordings to 20s WAV, converting non-WAV if needed."""
    try:
        info = sf.info(path)
        duration = float(info.duration)
        if duration <= MAX_REFERENCE_S:
            return path, duration, False
        audio, sr = sf.read(path, frames=int(CLIPPED_REFERENCE_S * info.samplerate), dtype="float32")
        if audio.ndim > 1:
            audio = audio.mean(axis=1)
        clipped = tempfile.NamedTemporaryFile(suffix=".wav", delete=False).name
        sf.write(clipped, audio, sr)
        emit({
            "event": "notice",
            "message": f"Clipped reference from {duration:.1f}s to the first {CLIPPED_REFERENCE_S:.0f}s (optimal cloning window is 5-30s).",
        })
        return clipped, float(len(audio) / sr), True
    except Exception:
        # Fallback through librosa for formats libsndfile cannot open directly
        import librosa
        audio, sr = librosa.load(path, sr=None, mono=True)
        duration = float(len(audio) / sr)
        clipped_flag = duration > MAX_REFERENCE_S
        if clipped_flag:
            audio = audio[: int(CLIPPED_REFERENCE_S * sr)]
            emit({
                "event": "notice",
                "message": f"Clipped reference from {duration:.1f}s to the first {CLIPPED_REFERENCE_S:.0f}s.",
            })
        out_wav = tempfile.NamedTemporaryFile(suffix=".wav", delete=False).name
        sf.write(out_wav, audio, sr)
        return out_wav, float(len(audio) / sr), clipped_flag


def write_wav_pcm16(path: str, audio: np.ndarray, sample_rate: int) -> None:
    arr = np.asarray(audio, dtype=np.float32).reshape(-1)
    arr = np.clip(arr, -1.0, 1.0)
    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
    sf.write(path, arr, int(sample_rate), subtype="PCM_16")


def handle_load_model(req: dict) -> None:
    global CURRENT_MODEL, CURRENT_FAMILY, CURRENT_MODEL_NAME, CURRENT_WEIGHTS, CURRENT_DECODER

    model_name = (req.get("model_name") or "").strip()
    if not model_name:
        raise ValueError("Model name or local path cannot be empty.")

    cache_dir = req.get("cache_dir") or None
    hf_token = req.get("hf_token") or None
    backend = req.get("backend") or None
    device = req.get("device") or None
    weights = req.get("weights") or None
    decoder = req.get("decoder") or None
    threads = req.get("threads")

    emit({"event": "status", "message": f"Checking {model_name}..."})
    _, _, config = resolve_repo(model_name, cache_dir=cache_dir)
    model_type = config.get("type")

    # Free previously loaded model cleanly before allocating the next one.
    if CURRENT_MODEL is not None:
        _unload_transient_helpers(CURRENT_MODEL)
        CURRENT_MODEL = None
        CURRENT_FAMILY = None
        CURRENT_MODEL_NAME = None
        CURRENT_WEIGHTS = None
        CURRENT_DECODER = None
        _release_memory(trim_working_set=True, clear_all_caches=True)

    try:
        import torch
        torch.set_num_threads(max(1, min(4, int(threads) if threads else (os.cpu_count() or 4))))
    except Exception:
        pass

    emit({"event": "status", "message": f"Downloading & loading {model_name}..."})
    if model_type == KITTEN2_TYPE:
        _install_low_ram_kittentts2_loader()
        model = KittenTTS(
            model_name,
            cache_dir=cache_dir,
            backend=backend,
            device=device,
            hf_token=hf_token,
            decoder=decoder,
            weights=weights,
        )
        if str(getattr(model, "device", "cpu")) == "cpu":
            # Avoid loading 1.55B whisper-large-v3 (4-8 GB float32 RAM) on CPU auto-transcription.
            model.transcriber.model_name = "openai/whisper-base"
        model.transcriber = _AutoUnloadTranscriber(model.transcriber)
        model.speaker_encoder = _AutoUnloadSpeakerEncoder(model.speaker_encoder)
    elif model_type == KITTEN2_TINY_TYPE:
        tiny_weights = weights if backend != "torch" else None
        model = KittenTTS(
            model_name,
            cache_dir=cache_dir,
            backend=backend,
            device=device,
            hf_token=hf_token,
            weights=tiny_weights,
        )
        if threads:
            model.threads = int(threads)
    elif model_type in ONNX_TYPES:
        onnx_backend = backend or device
        if onnx_backend in ("auto", "onnx"):
            onnx_backend = None
        model = KittenTTS(
            model_name,
            cache_dir=cache_dir,
            backend=onnx_backend,
            hf_token=hf_token,
        )
    else:
        raise ValueError(f"Unsupported model type {model_type!r} in {model_name}")

    CURRENT_MODEL = model
    CURRENT_FAMILY = detect_family(model)
    CURRENT_MODEL_NAME = model_name
    CURRENT_WEIGHTS = weights
    CURRENT_DECODER = decoder

    # Release temporary tl2_emb4 / safetensors unpacking buffers back to Windows.
    _release_memory(trim_working_set=True)

    info = build_model_info(model, model_name, weights, decoder)
    emit({"event": "model_loaded", "info": info})


def _generate_expressive_sequence(
    items: list[dict],
    base_speed: float,
    base_synth_fn,
    sample_rate: int,
    output_wav: str,
    stream: bool,
) -> tuple[np.ndarray, int]:
    """Synthesize parsed speech clauses + vocal events with emotion DSP and optional streaming."""
    stem = str(Path(output_wav).with_suffix(""))
    joined_segments: list[tuple[np.ndarray, float, bool]] = []
    emitted_chunks = 0

    for idx, item in enumerate(items):
        emo = item.get("emotion")
        is_last = idx == len(items) - 1
        next_is_emph = not is_last and items[idx + 1].get("emphasis", False)

        if item["type"] == "event":
            seg = synthesize_event_audio(item["event"], emo, base_synth_fn, sample_rate)
            gap_s = 0.02 if next_is_emph else 0.04
            trim_tight = False
        else:
            is_emph = bool(item.get("emphasis"))
            emo_spd, pitch_steps, _, _, _ = _EMOTION_PROFILES.get(
                emo or "mundane", (1.0, 0.0, 0.0, 1.0, 0.0)
            )
            if is_emph:
                emo_spd *= 0.94
                pitch_steps += 1.05
            pitch_factor = 2.0 ** (pitch_steps / 12.0)
            neural_speed = float(np.clip((base_speed * emo_spd) / pitch_factor, 0.5, 2.0))
            raw = base_synth_fn(item["text"], neural_speed)
            seg, extra_pause = apply_emotion_and_emphasis_dsp(
                raw, sample_rate, emo, emphasis=is_emph
            )
            txt = item["text"].rstrip()
            if next_is_emph:
                gap_s = 0.012
                trim_tight = True
            elif txt.endswith((".", "!", "?")):
                gap_s = 0.14 + extra_pause
                trim_tight = False
            elif txt.endswith((",", ";", ":")):
                gap_s = 0.06 + extra_pause * 0.5
                trim_tight = bool(item.get("emphasis"))
            else:
                gap_s = 0.02 + extra_pause * 0.4
                trim_tight = bool(item.get("emphasis"))

        flat = np.asarray(seg, dtype=np.float32).reshape(-1)
        if flat.size == 0:
            continue
        joined_segments.append((flat, gap_s, trim_tight))

        if stream:
            chunk_path = f"{stem}_chunk_{emitted_chunks}.wav"
            write_wav_pcm16(chunk_path, flat, sample_rate)
            emit({
                "event": "chunk",
                "index": emitted_chunks,
                "wav_path": chunk_path,
                "duration_s": float(flat.size / sample_rate),
            })
            emitted_chunks += 1

    if not joined_segments:
        raise RuntimeError("No audio was generated for the given text.")
    audio = join_expressive_segments(joined_segments, sample_rate)
    return audio, max(1, emitted_chunks if stream else len(joined_segments))


def handle_generate(req: dict) -> None:
    if CURRENT_MODEL is None:
        raise RuntimeError("No model is loaded yet. Please load a model first.")

    text = str(req.get("text") or "")
    if not text.strip():
        raise ValueError("Type some text to speak first.")

    output_wav = req["output_wav"]
    stream = bool(req.get("stream", False))
    family = CURRENT_FAMILY
    sample_rate = int(getattr(CURRENT_MODEL, "sample_rate", 24000))
    started = time.time()

    try:
        if family == "kitten2":
            voice = req.get("voice")
            reference = req.get("reference")
            reference_text = req.get("reference_text")
            if reference:
                reference, _, _ = prepare_reference_audio(reference)
                voice = None
            if reference_text is not None:
                reference_text = reference_text.strip() or None

            text, has_tags = preprocess_text_for_kitten2(text)
            adv = dict(req.get("advanced") or {})
            preset = req.get("preset") or CURRENT_MODEL.default_preset
            if has_tags:
                if "use_emotion" not in adv:
                    adv["use_emotion"] = True
                if preset == "stable" and req.get("temperature") is None:
                    preset = "expressive"

            # On CPU, cap chunk length so S3Gen.mel2wav Conv1d upsampling stays low-RAM.
            if str(getattr(CURRENT_MODEL, "device", "cpu")) == "cpu":
                adv["chunk_chars"] = min(int(adv.get("chunk_chars") or 220), 240)
                adv["chunk_min_chars"] = min(int(adv.get("chunk_min_chars") or 90), 100)

            # Carry active [emotion] onto later chunks if script exceeds chunk budget
            chunk_max = int(adv.get("chunk_chars") or getattr(CURRENT_MODEL, "chunk_chars", 380) or 380)
            if has_tags and len(text) > chunk_max:
                from kittenml.kittentts2 import text as k2_text
                raw_chunks = k2_text.split_for_synthesis(
                    k2_text.normalize_text(text, normalize=bool(req.get("normalize", True))),
                    max_chars=chunk_max,
                    min_chars=int(adv.get("chunk_min_chars") or getattr(CURRENT_MODEL, "chunk_min_chars", 130) or 130),
                )
                carried = split_kitten2_with_carried_emotion(raw_chunks)
                text = " ".join(carried) if len(carried) == 1 else "\n".join(carried)

            kwargs = {
                "voice": voice,
                "reference": reference,
                "reference_text": reference_text,
                "preset": preset,
                "max_new_tokens": int(req.get("max_new_tokens") or 1000),
                "normalize": bool(req.get("normalize", True)),
            }
            if req.get("use_reference_prompt") is not None:
                kwargs["use_reference_prompt"] = bool(req["use_reference_prompt"])
            for param in ("temperature", "top_k", "top_p", "min_p"):
                if req.get(param) is not None:
                    kwargs[param] = req[param]
            if adv:
                kwargs["advanced"] = adv

            if stream:
                wavs = []
                stem = str(Path(output_wav).with_suffix(""))
                for idx, chunk in enumerate(CURRENT_MODEL.generate_stream(text, **kwargs)):
                    flat_chunk = np.asarray(chunk, dtype=np.float32).reshape(-1)
                    chunk_path = f"{stem}_chunk_{idx}.wav"
                    write_wav_pcm16(chunk_path, flat_chunk, sample_rate)
                    wavs.append(flat_chunk)
                    emit({
                        "event": "chunk",
                        "index": idx,
                        "wav_path": chunk_path,
                        "duration_s": float(flat_chunk.size / sample_rate),
                    })
                gap_s = float(adv.get("chunk_gap_s", 0.16))
                audio = CURRENT_MODEL._join(wavs, gap_s=gap_s)
                chunks_count = len(wavs)
            else:
                audio = CURRENT_MODEL.generate(text, **kwargs)
                chunks_count = 1

        elif family == "kitten2_tiny":
            voice = req.get("voice") or None
            language = req.get("language") or None
            speed = float(req.get("speed") or 1.0)
            items = parse_expressive_script(text)
            has_expressive = (
                len(items) > 1
                or (len(items) == 1 and (items[0]["type"] != "speech" or items[0].get("emotion") is not None or items[0].get("emphasis")))
            )
            if has_expressive:
                audio, chunks_count = _generate_expressive_sequence(
                    items,
                    speed,
                    lambda txt, spd: CURRENT_MODEL.generate(txt, voice=voice, language=language, speed=spd),
                    sample_rate,
                    output_wav,
                    stream,
                )
            elif stream:
                wavs = []
                stem = str(Path(output_wav).with_suffix(""))
                for idx, chunk in enumerate(CURRENT_MODEL.generate_stream(text, voice=voice, language=language, speed=speed)):
                    flat_chunk = np.asarray(chunk, dtype=np.float32).reshape(-1)
                    chunk_path = f"{stem}_chunk_{idx}.wav"
                    write_wav_pcm16(chunk_path, flat_chunk, sample_rate)
                    wavs.append(flat_chunk)
                    emit({
                        "event": "chunk",
                        "index": idx,
                        "wav_path": chunk_path,
                        "duration_s": float(flat_chunk.size / sample_rate),
                    })
                if not wavs:
                    raise RuntimeError("No audio was generated for the given text.")
                gap = np.zeros(int(0.15 * sample_rate), dtype=np.float32)
                parts = []
                for w in wavs:
                    parts.extend([w, gap])
                audio = np.concatenate(parts[:-1])
                chunks_count = len(wavs)
            else:
                audio = CURRENT_MODEL.generate(text, voice=voice, language=language, speed=speed)
                chunks_count = 1

        else:
            # onnx_legacy
            voice = req.get("voice") or "Bella"
            speed = float(req.get("speed") or 1.0)
            clean_text = bool(req.get("normalize", True))
            items = parse_expressive_script(text)
            has_expressive = (
                len(items) > 1
                or (len(items) == 1 and (items[0]["type"] != "speech" or items[0].get("emotion") is not None or items[0].get("emphasis")))
            )
            if has_expressive:
                audio, chunks_count = _generate_expressive_sequence(
                    items,
                    speed,
                    lambda txt, spd: CURRENT_MODEL.generate(txt, voice=voice, speed=spd, clean_text=clean_text),
                    sample_rate,
                    output_wav,
                    stream,
                )
            elif stream:
                wavs = []
                stem = str(Path(output_wav).with_suffix(""))
                for idx, chunk in enumerate(CURRENT_MODEL.generate_stream(text, voice=voice, speed=speed, clean_text=clean_text)):
                    flat_chunk = np.asarray(chunk, dtype=np.float32).reshape(-1)
                    chunk_path = f"{stem}_chunk_{idx}.wav"
                    write_wav_pcm16(chunk_path, flat_chunk, sample_rate)
                    wavs.append(flat_chunk)
                    emit({
                        "event": "chunk",
                        "index": idx,
                        "wav_path": chunk_path,
                        "duration_s": float(flat_chunk.size / sample_rate),
                    })
                if not wavs:
                    raise RuntimeError("No audio was generated for the given text.")
                audio = np.concatenate(wavs, axis=-1)
                chunks_count = len(wavs)
            else:
                audio = CURRENT_MODEL.generate(text, voice=voice, speed=speed, clean_text=clean_text)
                chunks_count = 1

        if audio is None:
            raise RuntimeError("No audio samples were generated. Try rewording the text.")

        audio_arr = np.asarray(audio, dtype=np.float32).reshape(-1)
        if audio_arr.size == 0:
            raise RuntimeError("No audio samples were generated. Try rewording the text.")

        write_wav_pcm16(output_wav, audio_arr, sample_rate)
        elapsed_s = time.time() - started
        duration_s = float(audio_arr.size / sample_rate)
        emit({
            "event": "generated",
            "wav_path": output_wav,
            "sample_rate": sample_rate,
            "duration_s": duration_s,
            "elapsed_s": elapsed_s,
            "chunks": chunks_count,
        })
    finally:
        _release_memory(trim_working_set=True)


def handle_transcribe(req: dict) -> None:
    if CURRENT_MODEL is None or CURRENT_FAMILY != "kitten2":
        raise RuntimeError("Auto-transcription requires KittenTTS 2 to be loaded.")
    ref_path = req.get("reference")
    if not ref_path:
        raise ValueError("Please pick or record a reference audio clip first.")
    try:
        prepared, _, _ = prepare_reference_audio(ref_path)
        emit({"event": "status", "message": "Transcribing reference clip with Whisper..."})
        transcript = CURRENT_MODEL.transcriber(prepared)
        emit({"event": "transcribed", "text": transcript})
    finally:
        _release_memory(trim_working_set=True)


def handle_voice_sample(req: dict) -> None:
    if CURRENT_MODEL is None or CURRENT_FAMILY != "kitten2":
        raise RuntimeError("Built-in reference sample playback requires KittenTTS 2.")
    voice = req["voice"]
    output_wav = req["output_wav"]
    spec = CURRENT_MODEL.voices.get(voice)
    if not spec:
        raise ValueError(f"Unknown voice {voice!r}")
    try:
        audio, sr = sf.read(spec["reference"], dtype="float32", always_2d=True)
        mono = audio.mean(axis=1)
        write_wav_pcm16(output_wav, mono, sr)
        emit({
            "event": "voice_sample_ready",
            "voice": voice,
            "wav_path": output_wav,
            "transcript": spec.get("transcript", ""),
        })
    finally:
        _release_memory(trim_working_set=True)


def handle_free_memory(req: dict) -> None:
    global CURRENT_MODEL, CURRENT_FAMILY, CURRENT_MODEL_NAME, CURRENT_WEIGHTS, CURRENT_DECODER
    unload = bool(req.get("unload_model", False))
    if unload and CURRENT_MODEL is not None:
        _unload_transient_helpers(CURRENT_MODEL)
        CURRENT_MODEL = None
        CURRENT_FAMILY = None
        CURRENT_MODEL_NAME = None
        CURRENT_WEIGHTS = None
        CURRENT_DECODER = None
    _release_memory(trim_working_set=True, clear_all_caches=True)
    emit({
        "event": "memory_freed",
        "unloaded": unload,
        "ram_mb": round(get_process_ram_mb(), 1),
    })


def handle_normalize(req: dict) -> None:
    text = str(req.get("text") or "")
    if CURRENT_MODEL is None:
        raise RuntimeError("Load a model first to inspect text normalization.")

    if CURRENT_FAMILY == "kitten2":
        cleaned, _ = preprocess_text_for_kitten2(text)
        normalized = CURRENT_MODEL.normalize_text(cleaned)
        _, tag_spans = strip_tags_for_preview(cleaned)
        emit({
            "event": "normalized",
            "original": text,
            "normalized": normalized,
            "spans": tag_spans,
        })
    elif CURRENT_FAMILY == "kitten2_tiny":
        from kittenml.kittentts2_tiny.frontend.tts_norm import normalize as tiny_normalize
        spoken_preview, tag_spans = strip_tags_for_preview(text)
        normalized = tiny_normalize(spoken_preview) if spoken_preview else ""
        emit({
            "event": "normalized",
            "original": text,
            "normalized": normalized,
            "spans": tag_spans,
        })
    else:
        spoken_preview, tag_spans = strip_tags_for_preview(text)
        res = CURRENT_MODEL.normalize_text(spoken_preview, return_spans=True)
        spans = list(tag_spans)
        for sp in getattr(res, "spans", []):
            spans.append({
                "original": spoken_preview[sp.originalStartChar:sp.originalEndChar],
                "replacement": res.text[sp.normalizedStartChar:sp.normalizedEndChar],
                "orig_start": sp.originalStartChar,
                "orig_end": sp.originalEndChar,
                "norm_start": sp.normalizedStartChar,
                "norm_end": sp.normalizedEndChar,
            })
        emit({
            "event": "normalized",
            "original": text,
            "normalized": res.text,
            "spans": spans,
        })


def handle_phonemize(req: dict) -> None:
    if CURRENT_MODEL is None or CURRENT_FAMILY != "kitten2_tiny":
        raise RuntimeError("Phoneme preview is supported for KittenTTS 2 Pico, Nano, and Mini.")
    text = str(req.get("text") or "")
    language = req.get("language") or None
    spoken_preview, tag_spans = strip_tags_for_preview(text)
    chunks = CURRENT_MODEL.phonemize(spoken_preview, language=language) if spoken_preview else []
    note = (
        "Expression/event tags ([...], <...>) were stripped from speech phonemes and routed to tone & sound-effect synthesis."
        if tag_spans
        else None
    )
    emit({"event": "phonemized", "chunks": list(chunks), "note": note})


def handle_export_audio(req: dict) -> None:
    input_wav = req["input_wav"]
    output_path = req["output_path"]
    fmt = (req.get("format") or "wav").lower()
    format_map = {
        "wav": ("WAV", "PCM_16"),
        "mp3": ("MP3", "MPEG_LAYER_III"),
        "flac": ("FLAC", "PCM_16"),
        "ogg": ("OGG", "OPUS"),
    }
    if fmt not in format_map:
        raise ValueError(f"Unsupported export format: {fmt}")
    sf_fmt, subtype = format_map[fmt]
    audio, sr = sf.read(input_wav, dtype="float32")
    os.makedirs(os.path.dirname(os.path.abspath(output_path)), exist_ok=True)
    sf.write(output_path, audio, sr, format=sf_fmt, subtype=subtype)
    emit({"event": "exported", "output_path": output_path, "format": fmt})


def handle_audio_info(req: dict) -> None:
    path = req["path"]
    out_wav = req.get("output_wav")
    try:
        info = sf.info(path)
        duration = float(info.duration)
        clipped = duration > MAX_REFERENCE_S
        playback_wav = None
        if clipped or not path.lower().endswith(".wav"):
            frames = int(CLIPPED_REFERENCE_S * info.samplerate) if clipped else -1
            audio, sr = sf.read(path, frames=frames, dtype="float32")
            if audio.ndim > 1:
                audio = audio.mean(axis=1)
            target = out_wav or tempfile.NamedTemporaryFile(suffix=".wav", delete=False).name
            write_wav_pcm16(target, audio, sr)
            playback_wav = target
            if clipped:
                duration = float(len(audio) / sr)
        emit({
            "event": "audio_info",
            "path": path,
            "duration_s": duration,
            "clipped": clipped,
            "playback_wav": playback_wav,
        })
    except Exception:
        import librosa
        audio, sr = librosa.load(path, sr=None, mono=True)
        raw_dur = float(len(audio) / sr)
        clipped = raw_dur > MAX_REFERENCE_S
        if clipped:
            audio = audio[: int(CLIPPED_REFERENCE_S * sr)]
        target = out_wav or tempfile.NamedTemporaryFile(suffix=".wav", delete=False).name
        write_wav_pcm16(target, audio, sr)
        emit({
            "event": "audio_info",
            "path": path,
            "duration_s": float(len(audio) / sr),
            "clipped": clipped,
            "playback_wav": target,
        })


def main() -> None:
    import json
    for raw_line in sys.stdin:
        line = raw_line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            cmd = req.get("cmd")
            if cmd == "ping":
                emit({"event": "pong"})
            elif cmd == "load_model":
                handle_load_model(req)
            elif cmd == "generate":
                handle_generate(req)
            elif cmd == "transcribe":
                handle_transcribe(req)
            elif cmd == "voice_sample":
                handle_voice_sample(req)
            elif cmd == "normalize":
                handle_normalize(req)
            elif cmd == "phonemize":
                handle_phonemize(req)
            elif cmd == "export_audio":
                handle_export_audio(req)
            elif cmd == "audio_info":
                handle_audio_info(req)
            elif cmd == "free_memory":
                handle_free_memory(req)
            elif cmd == "shutdown":
                break
            else:
                emit({"event": "error", "message": f"Unknown command: {cmd}"})
        except Exception as exc:
            emit({
                "event": "error",
                "message": str(exc),
                "detail": traceback.format_exc(),
            })


if __name__ == "__main__":
    main()
