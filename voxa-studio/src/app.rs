use crate::audio::{AudioPlayer, PlaybackSnapshot};
use crate::protocol::{
    builtin_voice_description, describe_markup_segment, has_any_expression_markup,
    language_display_name, parse_expression_markup, parse_expression_spans,
    sample_text_for_language, DocumentChapter, GenerateRequest, MarkupKind, ModelCategory,
    ModelInfo, NormalizeSpan, TagTooltipInfo, VoiceBlend, VoiceEntry, WorkerEvent, WorkerRequest,
    EXPRESSION_TAGS, MODEL_CATALOG, VOCAL_EVENT_TAGS,
};
use crate::recorder::{MicRecorder, MAX_RECORD_SECS};
use crate::state::{
    AppConfig, HistoryItem, SavedVoiceProfile, StudioStorage, ThemeMode, STORY_STARTERS,
};
use crate::theme::{
    apply_theme, card_header_row, draw_close_icon, draw_mic_icon, draw_pause_icon, draw_play_icon,
    draw_replay_icon, draw_search_icon, draw_sparkle_icon, draw_star_icon, draw_stop_icon,
    draw_theme_icon, draw_voice_avatar, draw_voxa_logo, format_mmss, pill_badge, styled_card,
    vector_icon_button, waveform_widget, StudioPalette,
};
use crate::worker::{locate_workspace_root, WorkerHandle};
use chrono::Local;
use eframe::egui::{self, Color32, CornerRadius, Key, Modifiers, RichText, Stroke, Vec2};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveTab {
    Speak,
    CloneVoice,
    ModelsSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelFilterTab {
    All,
    FastCpu,
    Kokoro,
    StudioLm,
    Classic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloneSourceMode {
    UploadFile,
    RecordMic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecodingMode {
    Stable,
    Expressive,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmotionOverride {
    Auto,
    ForceOn,
    ForceOff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportFormat {
    Wav,
    Mp3,
    Flac,
    Ogg,
}

impl ExportFormat {
    fn ext(&self) -> &'static str {
        match self {
            ExportFormat::Wav => "wav",
            ExportFormat::Mp3 => "mp3",
            ExportFormat::Flac => "flac",
            ExportFormat::Ogg => "ogg",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            ExportFormat::Wav => "WAV (Uncompressed)",
            ExportFormat::Mp3 => "MP3 (Small file)",
            ExportFormat::Flac => "FLAC (Lossless)",
            ExportFormat::Ogg => "OGG Opus",
        }
    }
}

#[derive(Debug, Clone)]
enum BusyTask {
    LoadingModel(String),
    Generating { started: Instant, chunks_done: usize },
    Transcribing,
    SamplingVoice(String),
    Exporting,
}

#[derive(Debug, Clone)]
enum BannerKind {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
struct BannerMessage {
    kind: BannerKind,
    text: String,
    detail: Option<String>,
}

#[derive(Debug, Clone)]
enum InspectorContent {
    Normalized {
        original: String,
        normalized: String,
        spans: Vec<NormalizeSpan>,
    },
    Phonemes {
        text: String,
        chunks: Vec<String>,
    },
}

struct PendingGenMeta {
    text: String,
    voice_label: String,
    model_name: String,
}

#[derive(Debug, Clone)]
struct DownloadProgressState {
    file_name: String,
    downloaded_bytes: u64,
    total_bytes: u64,
    speed_bps: f64,
    eta_s: Option<f64>,
}

#[derive(Debug, Clone)]
struct PendingModelConfig {
    model_name: String,
    backend: Option<String>,
    device: Option<String>,
    weights: Option<String>,
    decoder: Option<String>,
}

pub struct VoxaStudioApp {
    storage: StudioStorage,
    config: AppConfig,
    worker: Option<WorkerHandle>,
    worker_error: Option<String>,

    audio: AudioPlayer,
    recorder: MicRecorder,

    active_tab: ActiveTab,
    model_info: Option<ModelInfo>,
    busy: Option<BusyTask>,
    download_progress: Option<DownloadProgressState>,
    pending_model_config: Option<PendingModelConfig>,
    banner: Option<BannerMessage>,
    logs: VecDeque<String>,
    force_open_logs: bool,
    show_quick_guide: bool,
    show_tags_guide: bool,
    inspector: Option<InspectorContent>,

    // Speak tab state
    selected_language: String,
    selected_voice: String,
    voice_search: String,
    favorites_only: bool,
    speak_text: String,
    last_auto_sample: String,
    loaded_doc_title: Option<String>,
    loaded_chapters: Vec<DocumentChapter>,
    loaded_chapter_idx: usize,
    blend_enabled: bool,
    blend_second_voice: String,
    blend_primary_pct: u32,

    // Clone tab state
    clone_source: CloneSourceMode,
    clone_ref_path: Option<PathBuf>,
    clone_ref_duration: Option<f32>,
    clone_transcript: String,
    clone_text: String,
    clone_use_ref_prompt: bool,
    new_profile_name: String,
    selected_profile_id: Option<String>,
    saved_voices: Vec<SavedVoiceProfile>,

    // Synthesis controls
    speed: f32,
    decoding_mode: DecodingMode,
    temperature: f32,
    top_k: u32,
    top_p: f32,
    min_p: f32,
    normalize_text: bool,
    max_new_tokens: u32,
    repetition_penalty: f32,
    repetition_window: u32,
    token_run_penalty: f32,
    token_run_grace: u32,
    chunk_chars: u32,
    chunk_min_chars: u32,
    chunk_gap_s: f32,
    emotion_override: EmotionOverride,
    use_seed: bool,
    seed: u32,

    // Player & history state
    export_format: ExportFormat,
    last_gen_summary: String,
    pending_gen: Option<PendingGenMeta>,
    stream_started_for_gen: bool,
    pending_export_target: Option<PathBuf>,
    history: Vec<HistoryItem>,

    // Models & Settings tab draft configuration
    model_filter: ModelFilterTab,
    draft_model_name: String,
    draft_custom_repo: String,
    draft_tiny_weights: String,
    draft_tiny_backend: String,
    draft_kokoro_weights: String,
    draft_k2_weights: String,
    draft_k2_decoder: String,
    draft_device: String,
    draft_threads: u32,
    reload_after_cancel: bool,
}

impl VoxaStudioApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let root = locate_workspace_root();
        let storage = StudioStorage::init(root.as_deref());
        let config = storage.load_config();
        let history = storage.load_history();
        let saved_voices = storage.load_saved_voices();

        let audio = AudioPlayer::new();
        audio.set_volume(config.volume);

        let (worker, worker_error) = match WorkerHandle::spawn(cc.egui_ctx.clone()) {
            Ok(w) => (Some(w), None),
            Err(e) => (None, Some(e)),
        };

        let mut app = Self {
            storage,
            model_filter: ModelFilterTab::All,
            draft_model_name: config.last_model_name.clone(),
            draft_custom_repo: String::new(),
            draft_tiny_weights: config
                .last_weights
                .clone()
                .filter(|w| w == "int8" || w == "int4")
                .unwrap_or_else(|| "int8".to_string()),
            draft_tiny_backend: config
                .last_backend
                .clone()
                .filter(|b| b == "onnx" || b == "torch")
                .unwrap_or_else(|| "onnx".to_string()),
            draft_kokoro_weights: config
                .last_weights
                .clone()
                .filter(|w| w == "int8" || w == "fp16" || w == "fp32")
                .unwrap_or_else(|| "int8".to_string()),
            draft_k2_weights: config
                .last_weights
                .clone()
                .filter(|w| w == "emb4" || w == "packed" || w == "full")
                .unwrap_or_else(|| "emb4".to_string()),
            draft_k2_decoder: config
                .last_decoder
                .clone()
                .filter(|d| d == "default" || d == "student_w4")
                .unwrap_or_else(|| "student_w4".to_string()),
            draft_device: config
                .last_device
                .clone()
                .unwrap_or_else(|| "auto".to_string()),
            draft_threads: config.threads.max(1),
            config,
            worker,
            worker_error,
            audio,
            recorder: MicRecorder::new(),
            active_tab: ActiveTab::Speak,
            model_info: None,
            busy: None,
            download_progress: None,
            pending_model_config: None,
            banner: None,
            logs: VecDeque::with_capacity(200),
            force_open_logs: false,
            show_quick_guide: false,
            show_tags_guide: false,
            inspector: None,

            selected_language: "All".to_string(),
            selected_voice: String::new(),
            voice_search: String::new(),
            favorites_only: false,
            speak_text: STORY_STARTERS[1].text.to_string(),
            last_auto_sample: STORY_STARTERS[1].text.to_string(),
            loaded_doc_title: None,
            loaded_chapters: Vec::new(),
            loaded_chapter_idx: 0,
            blend_enabled: false,
            blend_second_voice: String::new(),
            blend_primary_pct: 60,

            clone_source: CloneSourceMode::UploadFile,
            clone_ref_path: None,
            clone_ref_duration: None,
            clone_transcript: String::new(),
            clone_text: "Hello! This is a friendly test of my cloned voice.".to_string(),
            clone_use_ref_prompt: true,
            new_profile_name: String::new(),
            selected_profile_id: None,
            saved_voices,

            speed: 1.0,
            decoding_mode: DecodingMode::Expressive,
            temperature: 0.75,
            top_k: 40,
            top_p: 0.88,
            min_p: 0.0,
            normalize_text: true,
            max_new_tokens: 1000,
            repetition_penalty: 1.1,
            repetition_window: 50,
            token_run_penalty: 1.3,
            token_run_grace: 10,
            chunk_chars: 380,
            chunk_min_chars: 130,
            chunk_gap_s: 0.16,
            emotion_override: EmotionOverride::Auto,
            use_seed: false,
            seed: 17,

            export_format: ExportFormat::Wav,
            last_gen_summary: String::new(),
            pending_gen: None,
            stream_started_for_gen: false,
            pending_export_target: None,
            history,
            reload_after_cancel: false,
        };

        if let Some(err) = app.worker_error.clone() {
            app.banner = Some(BannerMessage {
                kind: BannerKind::Error,
                text: err,
                detail: None,
            });
        } else {
            app.trigger_load_from_config();
        }

        app
    }

    fn trigger_load_from_config(&mut self) {
        let model_name = self.config.last_model_name.clone();
        self.load_model_with_params(
            model_name,
            self.config.last_backend.clone(),
            self.config.last_device.clone(),
            self.config.last_weights.clone(),
            self.config.last_decoder.clone(),
            Some(self.config.threads),
        );
    }

    fn load_model_with_params(
        &mut self,
        model_name: String,
        backend: Option<String>,
        device: Option<String>,
        weights: Option<String>,
        decoder: Option<String>,
        threads: Option<u32>,
    ) {
        let Some(worker) = self.worker.as_mut() else {
            self.banner = Some(BannerMessage {
                kind: BannerKind::Error,
                text: "Voice engine is not running.".to_string(),
                detail: self.worker_error.clone(),
            });
            return;
        };

        self.download_progress = None;
        self.pending_model_config = Some(PendingModelConfig {
            model_name: model_name.clone(),
            backend: backend.clone(),
            device: device.clone(),
            weights: weights.clone(),
            decoder: decoder.clone(),
        });

        // Only persist immediately on startup when no engine is loaded yet; during switches
        // preserve the previous working engine until ModelLoaded succeeds so Cancel Download restores it.
        if self.model_info.is_none() {
            self.config.last_model_name = model_name.clone();
            self.config.last_backend = backend.clone();
            self.config.last_device = device.clone();
            self.config.last_weights = weights.clone();
            self.config.last_decoder = decoder.clone();
            self.storage.save_config(&self.config);
        }

        let cache_dir = if self.config.cache_dir.trim().is_empty() {
            None
        } else {
            Some(self.config.cache_dir.trim().to_string())
        };
        let hf_token = if self.config.hf_token.trim().is_empty() {
            None
        } else {
            Some(self.config.hf_token.trim().to_string())
        };

        let req = WorkerRequest::LoadModel {
            model_name: model_name.clone(),
            backend,
            device,
            weights,
            decoder,
            cache_dir,
            hf_token,
            threads,
        };
        if let Err(err) = worker.send(&req) {
            self.banner = Some(BannerMessage {
                kind: BannerKind::Error,
                text: err,
                detail: None,
            });
        } else {
            self.busy = Some(BusyTask::LoadingModel(model_name));
            self.banner = None;
        }
    }

    fn load_from_draft_settings(&mut self) {
        let target = if !self.draft_custom_repo.trim().is_empty() {
            self.draft_custom_repo.trim().to_string()
        } else {
            self.draft_model_name.clone()
        };

        let cat = MODEL_CATALOG
            .iter()
            .find(|m| m.repo_id == target)
            .map(|m| m.category);

        let device = if self.draft_device == "auto" {
            None
        } else {
            Some(self.draft_device.clone())
        };

        let (backend, weights, decoder, threads) = match cat {
            Some(ModelCategory::TinyCpu) => (
                Some(self.draft_tiny_backend.clone()),
                if self.draft_tiny_backend == "torch" {
                    None
                } else {
                    Some(self.draft_tiny_weights.clone())
                },
                None,
                Some(self.draft_threads),
            ),
            Some(ModelCategory::KokoroOnnx) => (
                Some("onnx".to_string()),
                Some(self.draft_kokoro_weights.clone()),
                None,
                Some(self.draft_threads),
            ),
            Some(ModelCategory::StudioLm) => (
                None,
                Some(self.draft_k2_weights.clone()),
                Some(self.draft_k2_decoder.clone()),
                None,
            ),
            Some(ModelCategory::LegacyOnnx) => (device.clone(), None, None, None),
            None => (None, None, None, Some(self.draft_threads)),
        };

        self.config.threads = self.draft_threads;
        self.load_model_with_params(target, backend, device, weights, decoder, threads);
    }

    fn switch_or_reload_from_draft(&mut self, ctx: &egui::Context) {
        if self.busy.is_some() {
            if let Some(worker) = self.worker.as_mut() {
                if let Err(err) = worker.restart(ctx.clone()) {
                    self.busy = None;
                    self.banner = Some(BannerMessage {
                        kind: BannerKind::Error,
                        text: format!("Failed to switch engine cleanly: {err}"),
                        detail: None,
                    });
                    return;
                }
            }
            self.busy = None;
            self.download_progress = None;
            self.pending_model_config = None;
            self.pending_gen = None;
            self.stream_started_for_gen = false;
            self.reload_after_cancel = false;
            self.audio.stop();
        }
        self.load_from_draft_settings();
    }

    fn load_kitten2_for_cloning(&mut self) {
        self.draft_model_name = "KittenML/kitten-tts-2".to_string();
        self.draft_custom_repo.clear();
        self.draft_k2_weights = "emb4".to_string();
        self.draft_k2_decoder = "student_w4".to_string();
        self.load_model_with_params(
            "KittenML/kitten-tts-2".to_string(),
            None,
            if self.draft_device == "auto" {
                None
            } else {
                Some(self.draft_device.clone())
            },
            Some("emb4".to_string()),
            Some("student_w4".to_string()),
            None,
        );
    }

    fn cancel_current_task(&mut self, ctx: &egui::Context) {
        if let Some(worker) = self.worker.as_mut() {
            let had_model = self.model_info.is_some();
            let was_loading = matches!(&self.busy, Some(BusyTask::LoadingModel(_)));
            match worker.restart(ctx.clone()) {
                Ok(()) => {
                    self.busy = None;
                    self.download_progress = None;
                    self.pending_model_config = None;
                    self.pending_gen = None;
                    self.stream_started_for_gen = false;
                    self.audio.stop();

                    if !had_model {
                        self.config.last_model_name = "KittenML/kitten-tts-2-nano".to_string();
                        self.config.last_backend = None;
                        self.config.last_device = None;
                        self.config.last_weights = Some("int8".to_string());
                        self.config.last_decoder = None;
                        self.storage.save_config(&self.config);
                    }
                    self.draft_model_name = self.config.last_model_name.clone();
                    self.draft_custom_repo.clear();
                    if let Some(w) = &self.config.last_weights {
                        if w == "int8" || w == "int4" {
                            self.draft_tiny_weights = w.clone();
                        }
                        if w == "int8" || w == "fp16" || w == "fp32" {
                            self.draft_kokoro_weights = w.clone();
                        }
                        if w == "emb4" || w == "packed" || w == "full" {
                            self.draft_k2_weights = w.clone();
                        }
                    }
                    if let Some(d) = &self.config.last_decoder {
                        self.draft_k2_decoder = d.clone();
                    }

                    self.banner = Some(BannerMessage {
                        kind: BannerKind::Info,
                        text: if was_loading && had_model {
                            format!(
                                "Download cancelled. Restoring previous engine ({})...",
                                short_model_name(&self.config.last_model_name)
                            )
                        } else if was_loading {
                            "Download cancelled. Loading fast Kitten-TTS-2 Nano instead..."
                                .to_string()
                        } else {
                            "Cancelled. Reloading your active voice model...".to_string()
                        },
                        detail: None,
                    });
                    self.reload_after_cancel = true;
                }
                Err(e) => {
                    self.busy = None;
                    self.download_progress = None;
                    self.banner = Some(BannerMessage {
                        kind: BannerKind::Error,
                        text: e,
                        detail: None,
                    });
                }
            }
        }
    }

    fn poll_worker_events(&mut self) {
        let events = match &self.worker {
            Some(w) => w.try_recv_all(),
            None => return,
        };

        for evt in events {
            match evt {
                WorkerEvent::Pong => {
                    if self.reload_after_cancel {
                        self.reload_after_cancel = false;
                        self.trigger_load_from_config();
                    }
                }
                WorkerEvent::Status { message } => {
                    self.push_log(&message);
                }
                WorkerEvent::Notice { message } => {
                    self.banner = Some(BannerMessage {
                        kind: BannerKind::Warning,
                        text: message,
                        detail: None,
                    });
                }
                WorkerEvent::DownloadProgress {
                    file_name,
                    downloaded_bytes,
                    total_bytes,
                    speed_bps,
                    eta_s,
                } => {
                    self.download_progress = Some(DownloadProgressState {
                        file_name,
                        downloaded_bytes,
                        total_bytes,
                        speed_bps,
                        eta_s,
                    });
                }
                WorkerEvent::ModelLoaded { info } => {
                    self.busy = None;
                    self.download_progress = None;
                    self.apply_loaded_model_info(info);
                }
                WorkerEvent::Chunk {
                    index,
                    wav_path,
                    duration_s: _,
                } => {
                    if let Some(BusyTask::Generating { chunks_done, .. }) = &mut self.busy {
                        *chunks_done = index + 1;
                    }
                    if self.config.stream_playback && self.config.auto_play {
                        if index == 0 {
                            self.stream_started_for_gen = true;
                            self.audio.start_stream();
                        }
                        self.audio.append_stream_chunk(&wav_path);
                    }
                }
                WorkerEvent::Generated {
                    wav_path,
                    sample_rate: _,
                    duration_s,
                    elapsed_s,
                    chunks,
                } => {
                    let prev_busy = self.busy.take();
                    if let Some(BusyTask::SamplingVoice(v)) = prev_busy {
                        self.stream_started_for_gen = false;
                        self.audio.load_and_play(&wav_path, true);
                        self.last_gen_summary =
                            format!("Sample: {v} ({duration_s:.1}s)");
                        continue;
                    }

                    self.last_gen_summary = if chunks > 1 {
                        format!("{duration_s:.1}s in {elapsed_s:.1}s · {chunks} chunks")
                    } else {
                        format!("{duration_s:.1}s in {elapsed_s:.1}s")
                    };

                    if self.stream_started_for_gen {
                        self.stream_started_for_gen = false;
                        self.audio.finish_stream(&wav_path);
                    } else {
                        self.audio.load_and_play(&wav_path, self.config.auto_play);
                    }

                    if let Some(meta) = self.pending_gen.take() {
                        let snippet = meta.text.trim().replace('\n', " ");
                        let title = if snippet.chars().count() > 46 {
                            let s: String = snippet.chars().take(46).collect();
                            format!("{s}...")
                        } else {
                            snippet
                        };
                        let item = HistoryItem {
                            id: Local::now().format("%Y%m%d_%H%M%S_%3f").to_string(),
                            title,
                            text: meta.text,
                            voice_label: meta.voice_label,
                            model_name: meta.model_name,
                            wav_path,
                            duration_secs: duration_s,
                            elapsed_secs: elapsed_s,
                            created_at: Local::now().format("%b %d, %I:%M %p").to_string(),
                        };
                        self.history.insert(0, item);
                        self.history.truncate(25);
                        self.storage.save_history(&self.history);
                    }
                }
                WorkerEvent::Transcribed { text } => {
                    self.busy = None;
                    self.clone_transcript = text;
                    self.banner = Some(BannerMessage {
                        kind: BannerKind::Success,
                        text: "Transcript filled in by Whisper. Review or edit before generating.".to_string(),
                        detail: None,
                    });
                }
                WorkerEvent::VoiceSampleReady {
                    voice,
                    wav_path,
                    transcript,
                } => {
                    self.busy = None;
                    self.audio.load_and_play(&wav_path, true);
                    self.last_gen_summary = if transcript.is_empty() {
                        format!("Sample: {voice}")
                    } else {
                        format!("Sample {voice}: \"{}\"", truncate_str(&transcript, 50))
                    };
                }
                WorkerEvent::Normalized {
                    original,
                    normalized,
                    spans,
                } => {
                    self.inspector = Some(InspectorContent::Normalized {
                        original,
                        normalized,
                        spans,
                    });
                }
                WorkerEvent::Phonemized { chunks, note: _ } => {
                    let text = if self.active_tab == ActiveTab::CloneVoice {
                        self.clone_text.clone()
                    } else {
                        self.speak_text.clone()
                    };
                    self.inspector = Some(InspectorContent::Phonemes { text, chunks });
                }
                WorkerEvent::Exported {
                    output_path,
                    format,
                } => {
                    self.busy = None;
                    self.pending_export_target = None;
                    self.banner = Some(BannerMessage {
                        kind: BannerKind::Success,
                        text: format!("Exported {} to {}", format.to_uppercase(), output_path),
                        detail: None,
                    });
                }
                WorkerEvent::AudioInfo {
                    path,
                    duration_s,
                    clipped,
                    playback_wav,
                } => {
                    let usable = playback_wav
                        .map(PathBuf::from)
                        .unwrap_or_else(|| PathBuf::from(&path));
                    self.clone_ref_path = Some(usable);
                    self.clone_ref_duration = Some(duration_s);
                    if clipped {
                        self.banner = Some(BannerMessage {
                            kind: BannerKind::Warning,
                            text: format!(
                                "Clipped long clip to {duration_s:.1}s (optimal cloning window is 5-30s)."
                            ),
                            detail: None,
                        });
                    }
                }
                WorkerEvent::MemoryFreed { unloaded, ram_mb } => {
                    if unloaded {
                        self.model_info = None;
                    }
                    self.banner = Some(BannerMessage {
                        kind: BannerKind::Success,
                        text: format!(
                            "{} (worker RAM now {ram_mb:.0} MB).",
                            if unloaded {
                                "Unloaded engine from memory"
                            } else {
                                "Freed unused engine memory"
                            }
                        ),
                        detail: None,
                    });
                }
                WorkerEvent::DocumentExtracted {
                    title,
                    path: _,
                    chapters,
                } => {
                    self.busy = None;
                    let count = chapters.len();
                    if let Some(first) = chapters.first() {
                        if self.active_tab == ActiveTab::CloneVoice {
                            self.clone_text = first.text.clone();
                        } else {
                            self.speak_text = first.text.clone();
                        }
                    }
                    self.loaded_doc_title = Some(title.clone());
                    self.loaded_chapters = chapters;
                    self.loaded_chapter_idx = 0;
                    self.banner = Some(BannerMessage {
                        kind: BannerKind::Success,
                        text: format!(
                            "Loaded \"{title}\" ({count} {}). Select any chapter above the script box.",
                            if count == 1 { "section" } else { "sections" }
                        ),
                        detail: None,
                    });
                }
                WorkerEvent::Error { message, detail } => {
                    self.busy = None;
                    self.download_progress = None;
                    self.pending_model_config = None;
                    self.pending_gen = None;
                    self.stream_started_for_gen = false;
                    self.pending_export_target = None;
                    self.banner = Some(BannerMessage {
                        kind: BannerKind::Error,
                        text: message,
                        detail,
                    });
                }
                WorkerEvent::Log(line) => {
                    self.push_log(&line);
                }
                WorkerEvent::Exited(msg) => {
                    self.busy = None;
                    self.download_progress = None;
                    self.push_log(&msg);
                }
            }
        }
    }

    fn push_log(&mut self, line: &str) {
        if self.logs.len() >= 250 {
            self.logs.pop_front();
        }
        self.logs.push_back(line.to_string());
    }

    fn apply_loaded_model_info(&mut self, info: ModelInfo) {
        if let Some(pending) = self.pending_model_config.take() {
            self.config.last_model_name = pending.model_name;
            self.config.last_backend = pending.backend;
            self.config.last_device = pending.device;
            self.config.last_weights = pending.weights;
            self.config.last_decoder = pending.decoder;
            self.storage.save_config(&self.config);
        }

        if let Some(dv) = &info.default_voice {
            self.selected_voice = dv.clone();
        } else if let Some(first) = info.voices.first() {
            self.selected_voice = first.name.clone();
        }

        if info.family == "kitten2_tiny" {
            self.selected_language = info
                .default_language
                .clone()
                .unwrap_or_else(|| "All".to_string());
        } else if info.family == "kokoro" {
            self.selected_language = "All".to_string();
        } else if info.languages.contains(&"English".to_string()) {
            self.selected_language = "English".to_string();
        } else {
            self.selected_language = "All".to_string();
        }

        if info.supports_voice_blending {
            let valid_second = !self.blend_second_voice.is_empty()
                && info.voices.iter().any(|v| v.name == self.blend_second_voice);
            if !valid_second {
                self.blend_second_voice = info
                    .voices
                    .iter()
                    .find(|v| v.name == "am_adam" && v.name != self.selected_voice)
                    .or_else(|| info.voices.iter().find(|v| v.name != self.selected_voice))
                    .map(|v| v.name.clone())
                    .unwrap_or_default();
            }
        } else {
            self.blend_enabled = false;
        }

        if let Some(preset_name) = &info.default_preset {
            self.decoding_mode = if preset_name == "expressive" {
                DecodingMode::Expressive
            } else {
                DecodingMode::Stable
            };
        }
        let target_preset = match self.decoding_mode {
            DecodingMode::Stable => "stable",
            _ => "expressive",
        };
        if let Some(p) = info.decode_presets.get(target_preset) {
            self.temperature = p.temperature;
            self.top_k = p.top_k;
            self.top_p = p.top_p;
            self.min_p = p.min_p;
        }
        if let Some(v) = info.advanced_options.get("repetition_penalty").and_then(|x| x.as_f64()) {
            self.repetition_penalty = v as f32;
        }
        if let Some(v) = info.advanced_options.get("repetition_window").and_then(|x| x.as_u64()) {
            self.repetition_window = v as u32;
        }
        if let Some(v) = info.advanced_options.get("token_run_penalty").and_then(|x| x.as_f64()) {
            self.token_run_penalty = v as f32;
        }
        if let Some(v) = info.advanced_options.get("token_run_grace").and_then(|x| x.as_u64()) {
            self.token_run_grace = v as u32;
        }
        if let Some(v) = info.advanced_options.get("chunk_chars").and_then(|x| x.as_u64()) {
            self.chunk_chars = v as u32;
        }
        if let Some(v) = info.advanced_options.get("chunk_min_chars").and_then(|x| x.as_u64()) {
            self.chunk_min_chars = v as u32;
        }
        if let Some(v) = info.advanced_options.get("chunk_gap_s").and_then(|x| x.as_f64()) {
            self.chunk_gap_s = v as f32;
        }
        self.clone_use_ref_prompt = info.use_reference_prompt_default;

        let loaded_msg = format!(
            "Engine ready: {} ({} voices)",
            short_model_name(&info.model_name),
            info.voices.len()
        );
        self.model_info = Some(info);
        self.banner = Some(BannerMessage {
            kind: BannerKind::Success,
            text: loaded_msg,
            detail: None,
        });
    }

    fn send_generate(&mut self, use_clone: bool) {
        let Some(info) = self.model_info.clone() else {
            self.banner = Some(BannerMessage {
                kind: BannerKind::Warning,
                text: "Please wait for a model to finish loading (or pick one in Models & Engine).".to_string(),
                detail: None,
            });
            return;
        };
        if self.busy.is_some() {
            return;
        }
        if use_clone && !info.supports_cloning {
            self.banner = Some(BannerMessage {
                kind: BannerKind::Warning,
                text: "Load Kitten-TTS-2 first to generate with a cloned voice.".to_string(),
                detail: None,
            });
            return;
        }

        let text = if use_clone {
            self.clone_text.clone()
        } else {
            self.speak_text.clone()
        };
        if text.trim().is_empty() {
            self.banner = Some(BannerMessage {
                kind: BannerKind::Warning,
                text: "Type some text first so Voxa Studio knows what to speak.".to_string(),
                detail: None,
            });
            return;
        }

        let reference = if use_clone {
            if let Some(path) = &self.clone_ref_path {
                Some(path.to_string_lossy().to_string())
            } else {
                self.banner = Some(BannerMessage {
                    kind: BannerKind::Warning,
                    text: "Pick an audio clip or record your voice first.".to_string(),
                    detail: None,
                });
                return;
            }
        } else {
            None
        };

        let timestamp = Local::now().format("%Y%m%d_%H%M%S_%3f").to_string();
        let output_wav = self
            .storage
            .outputs_dir
            .join(format!("speech_{timestamp}.wav"))
            .to_string_lossy()
            .to_string();

        let voice_opt = if use_clone {
            None
        } else {
            Some(self.selected_voice.clone()).filter(|v| !v.is_empty())
        };

        let lang_opt = if !use_clone
            && (info.family == "kitten2_tiny" || info.family == "kokoro")
            && self.selected_language != "All"
        {
            Some(self.selected_language.clone())
        } else {
            None
        };

        let voice_blend_opt = if !use_clone
            && info.supports_voice_blending
            && self.blend_enabled
            && !self.blend_second_voice.is_empty()
            && self.blend_second_voice != self.selected_voice
        {
            Some(VoiceBlend {
                second_voice: self.blend_second_voice.clone(),
                primary_weight: (self.blend_primary_pct as f32 / 100.0).clamp(0.05, 0.95),
            })
        } else {
            None
        };

        let mut advanced: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        if info.family == "kitten2" {
            advanced.insert("repetition_penalty".into(), serde_json::json!(self.repetition_penalty));
            if self.repetition_window > 0 {
                advanced.insert("repetition_window".into(), serde_json::json!(self.repetition_window));
            }
            advanced.insert("token_run_penalty".into(), serde_json::json!(self.token_run_penalty));
            advanced.insert("token_run_grace".into(), serde_json::json!(self.token_run_grace));
            advanced.insert("chunk_chars".into(), serde_json::json!(self.chunk_chars));
            advanced.insert("chunk_min_chars".into(), serde_json::json!(self.chunk_min_chars));
            advanced.insert("chunk_gap_s".into(), serde_json::json!(self.chunk_gap_s));
            match self.emotion_override {
                EmotionOverride::Auto => {}
                EmotionOverride::ForceOn => {
                    advanced.insert("use_emotion".into(), serde_json::json!(true));
                }
                EmotionOverride::ForceOff => {
                    advanced.insert("use_emotion".into(), serde_json::json!(false));
                }
            }
            if self.use_seed {
                advanced.insert("seed".into(), serde_json::json!(self.seed));
            }
        }

        let (preset, temp, top_k, top_p, min_p) = if info.family == "kitten2" {
            match self.decoding_mode {
                DecodingMode::Stable => (Some("stable".to_string()), None, None, None, None),
                DecodingMode::Expressive => {
                    (Some("expressive".to_string()), None, None, None, None)
                }
                DecodingMode::Custom => (
                    Some("expressive".to_string()),
                    Some(self.temperature),
                    Some(self.top_k),
                    Some(self.top_p),
                    Some(self.min_p),
                ),
            }
        } else {
            (None, None, None, None, None)
        };

        let voice_label = if use_clone {
            if let Some(pid) = &self.selected_profile_id {
                self.saved_voices
                    .iter()
                    .find(|p| &p.id == pid)
                    .map(|p| format!("Clone: {}", p.name))
                    .unwrap_or_else(|| "Custom Clone".to_string())
            } else {
                "Custom Clone".to_string()
            }
        } else if let Some(ref b) = voice_blend_opt {
            format!(
                "{} {}% + {} {}%",
                self.selected_voice,
                self.blend_primary_pct,
                b.second_voice,
                100 - self.blend_primary_pct
            )
        } else {
            self.selected_voice.clone()
        };

        let req = WorkerRequest::Generate(GenerateRequest {
            text: text.clone(),
            voice: voice_opt,
            voice_blend: voice_blend_opt,
            language: lang_opt,
            reference,
            reference_text: if use_clone && !self.clone_transcript.trim().is_empty() {
                Some(self.clone_transcript.trim().to_string())
            } else {
                None
            },
            speed: if info.supports_speed {
                Some(self.speed)
            } else {
                None
            },
            preset,
            temperature: temp,
            top_k,
            top_p,
            min_p,
            max_new_tokens: if info.family == "kitten2" {
                Some(self.max_new_tokens)
            } else {
                None
            },
            normalize: if info.supports_normalize {
                Some(self.normalize_text)
            } else {
                None
            },
            use_reference_prompt: if use_clone {
                Some(self.clone_use_ref_prompt)
            } else {
                None
            },
            advanced: if advanced.is_empty() {
                None
            } else {
                Some(advanced)
            },
            stream: self.config.stream_playback,
            output_wav,
        });

        if let Some(worker) = self.worker.as_mut() {
            if let Err(err) = worker.send(&req) {
                self.banner = Some(BannerMessage {
                    kind: BannerKind::Error,
                    text: err,
                    detail: None,
                });
            } else {
                self.pending_gen = Some(PendingGenMeta {
                    text,
                    voice_label,
                    model_name: short_model_name(&info.model_name).to_string(),
                });
                self.stream_started_for_gen = false;
                self.busy = Some(BusyTask::Generating {
                    started: Instant::now(),
                    chunks_done: 0,
                });
                self.banner = None;
            }
        }
    }

    fn sample_voice(&mut self, voice: &VoiceEntry) {
        let Some(info) = &self.model_info else { return };
        if self.busy.is_some() {
            return;
        }
        self.stream_started_for_gen = false;
        if info.supports_voice_sample {
            let out = self
                .storage
                .temp_dir
                .join(format!("sample_{}.wav", sanitize_filename(&voice.name)));
            if let Some(worker) = self.worker.as_mut() {
                let req = WorkerRequest::VoiceSample {
                    voice: voice.name.clone(),
                    output_wav: out.to_string_lossy().to_string(),
                };
                if worker.send(&req).is_ok() {
                    self.busy = Some(BusyTask::SamplingVoice(voice.name.clone()));
                }
            }
        } else {
            self.selected_voice = voice.name.clone();
            let greeting = sample_text_for_language(&voice.language).to_string();
            let out = self
                .storage
                .temp_dir
                .join(format!("sample_{}.wav", sanitize_filename(&voice.name)));
            let req = WorkerRequest::Generate(GenerateRequest {
                text: greeting,
                voice: Some(voice.name.clone()),
                voice_blend: None,
                language: if info.family == "kitten2_tiny" || info.family == "kokoro" {
                    Some(voice.language.clone())
                } else {
                    None
                },
                reference: None,
                reference_text: None,
                speed: Some(self.speed),
                preset: None,
                temperature: None,
                top_k: None,
                top_p: None,
                min_p: None,
                max_new_tokens: None,
                normalize: Some(true),
                use_reference_prompt: None,
                advanced: None,
                stream: false,
                output_wav: out.to_string_lossy().to_string(),
            });
            if let Some(worker) = self.worker.as_mut() {
                if worker.send(&req).is_ok() {
                    self.busy = Some(BusyTask::SamplingVoice(voice.name.clone()));
                }
            }
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });

        for path in dropped {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            match ext.as_str() {
                "txt" | "md" => {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        self.loaded_doc_title = None;
                        self.loaded_chapters.clear();
                        self.loaded_chapter_idx = 0;
                        if self.active_tab == ActiveTab::CloneVoice {
                            self.clone_text = content;
                        } else {
                            self.speak_text = content;
                        }
                        self.banner = Some(BannerMessage {
                            kind: BannerKind::Success,
                            text: format!(
                                "Imported script from {}",
                                path.file_name().unwrap_or_default().to_string_lossy()
                            ),
                            detail: None,
                        });
                    }
                }
                "epub" | "pdf" => {
                    self.open_book_document(&path);
                }
                "wav" | "mp3" | "flac" | "ogg" | "m4a" | "aac" => {
                    self.active_tab = ActiveTab::CloneVoice;
                    self.clone_source = CloneSourceMode::UploadFile;
                    self.set_clone_reference_file(path);
                }
                _ => {}
            }
        }
    }

    fn open_book_document(&mut self, path: &Path) {
        if let Some(worker) = self.worker.as_mut() {
            let req = WorkerRequest::ExtractDocument {
                path: path.to_string_lossy().to_string(),
            };
            match worker.send(&req) {
                Ok(()) => {
                    self.banner = Some(BannerMessage {
                        kind: BannerKind::Info,
                        text: format!(
                            "Reading chapters from {}...",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ),
                        detail: None,
                    });
                }
                Err(e) => {
                    self.banner = Some(BannerMessage {
                        kind: BannerKind::Error,
                        text: e,
                        detail: None,
                    });
                }
            }
        }
    }

    fn set_clone_reference_file(&mut self, path: PathBuf) {
        self.selected_profile_id = None;
        self.clone_ref_path = Some(path.clone());
        self.clone_ref_duration = None;
        self.clone_transcript.clear();

        let out_wav = self
            .storage
            .temp_dir
            .join(format!("ref_{}.wav", Local::now().format("%Y%m%d_%H%M%S")));
        if let Some(worker) = self.worker.as_mut() {
            let _ = worker.send(&WorkerRequest::AudioInfo {
                path: path.to_string_lossy().to_string(),
                output_wav: Some(out_wav.to_string_lossy().to_string()),
            });
        }
    }

    fn save_current_voice_profile(&mut self) {
        let Some(src) = self.clone_ref_path.clone() else {
            return;
        };
        let name = self.new_profile_name.trim().to_string();
        if name.is_empty() {
            self.banner = Some(BannerMessage {
                kind: BannerKind::Warning,
                text: "Enter a profile name first (e.g. Mom's Story Voice).".to_string(),
                detail: None,
            });
            return;
        }
        let id = Local::now().format("%Y%m%d_%H%M%S_%3f").to_string();
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("wav");
        let dest = self
            .storage
            .voices_dir
            .join(format!("{}_{}.{}", sanitize_filename(&name), id, ext));
        if let Err(err) = std::fs::copy(&src, &dest) {
            self.banner = Some(BannerMessage {
                kind: BannerKind::Error,
                text: format!("Could not save voice file: {err}"),
                detail: None,
            });
            return;
        }
        let profile = SavedVoiceProfile {
            id: id.clone(),
            name: name.clone(),
            wav_path: dest.to_string_lossy().to_string(),
            transcript: self.clone_transcript.trim().to_string(),
            duration_secs: self.clone_ref_duration.unwrap_or(0.0),
            created_at: Local::now().format("%b %d, %Y").to_string(),
        };
        self.saved_voices.insert(0, profile);
        self.storage.save_saved_voices(&self.saved_voices);
        self.selected_profile_id = Some(id);
        self.new_profile_name.clear();
        self.banner = Some(BannerMessage {
            kind: BannerKind::Success,
            text: format!("Saved voice profile \"{name}\"."),
            detail: None,
        });
    }

    fn export_audio_with_dialog(&mut self, source_wav: &Path) {
        let ext = self.export_format.ext();
        let default_name = format!(
            "voxa_{}.{}",
            Local::now().format("%Y%m%d_%H%M%S"),
            ext
        );
        if let Some(target) = rfd::FileDialog::new()
            .set_file_name(&default_name)
            .add_filter(self.export_format.label(), &[ext])
            .save_file()
        {
            if self.export_format == ExportFormat::Wav {
                match std::fs::copy(source_wav, &target) {
                    Ok(_) => {
                        self.banner = Some(BannerMessage {
                            kind: BannerKind::Success,
                            text: format!("Exported WAV to {}", target.display()),
                            detail: None,
                        });
                    }
                    Err(e) => {
                        self.banner = Some(BannerMessage {
                            kind: BannerKind::Error,
                            text: format!("Failed to save WAV: {e}"),
                            detail: None,
                        });
                    }
                }
            } else if let Some(worker) = self.worker.as_mut() {
                self.pending_export_target = Some(target.clone());
                self.busy = Some(BusyTask::Exporting);
                let _ = worker.send(&WorkerRequest::ExportAudio {
                    input_wav: source_wav.to_string_lossy().to_string(),
                    output_path: target.to_string_lossy().to_string(),
                    format: ext.to_string(),
                });
            }
        }
    }

    fn open_folder_for_path(path: &Path) {
        #[cfg(target_os = "windows")]
        {
            let target = if path.is_file() {
                format!("/select,{}", path.display())
            } else {
                path.display().to_string()
            };
            let _ = std::process::Command::new("explorer.exe").arg(target).spawn();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let dir = if path.is_file() {
                path.parent().unwrap_or(path)
            } else {
                path
            };
            let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
        }
    }
}

impl eframe::App for VoxaStudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker_events();
        self.handle_dropped_files(ctx);

        if self.recorder.is_recording() {
            ctx.request_repaint();
            if self.recorder.elapsed_secs() >= MAX_RECORD_SECS {
                if let Some(path) = self.recorder.stop() {
                    self.set_clone_reference_file(path);
                    self.banner = Some(BannerMessage {
                        kind: BannerKind::Info,
                        text: "Reached 30-second max clip length — recording stopped automatically.".to_string(),
                        detail: None,
                    });
                }
            }
        }

        let snap = self.audio.snapshot();
        if snap.is_playing || self.busy.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }

        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Enter)) && self.busy.is_none() {
            match self.active_tab {
                ActiveTab::Speak => self.send_generate(false),
                ActiveTab::CloneVoice => self.send_generate(true),
                ActiveTab::ModelsSettings => {}
            }
        }

        let has_audio = snap.loaded_path.is_some() || snap.duration_secs > 0.0;
        if !ctx.wants_keyboard_input()
            && has_audio
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Space))
        {
            self.audio.toggle_pause();
        }

        let pal = apply_theme(ctx, self.config.theme);

        // Zone 1: Top Studio Header Bar
        egui::TopBottomPanel::top("header_bar")
            .frame(
                egui::Frame::new()
                    .fill(pal.panel_bg)
                    .stroke(Stroke::new(1.0_f32, pal.border))
                    .inner_margin(egui::Margin::symmetric(14, 8)),
            )
            .show(ctx, |ui| {
                self.draw_header(ui, &pal, &snap);
            });

        // Zone 2: Bottom Timeline & Audio Track Dock
        egui::TopBottomPanel::bottom("timeline_dock")
            .frame(
                egui::Frame::new()
                    .fill(pal.panel_bg)
                    .stroke(Stroke::new(1.0_f32, pal.border))
                    .inner_margin(egui::Margin::symmetric(14, 10)),
            )
            .show(ctx, |ui| {
                self.draw_timeline_dock(ui, &pal, &snap);
            });

        // Zone 3: Left Media & Voice Pool (on Speak & Clone Voice tabs)
        if self.active_tab == ActiveTab::Speak || self.active_tab == ActiveTab::CloneVoice {
            egui::SidePanel::left("voice_sidebar")
                .default_width(300.0)
                .width_range(260.0..=360.0)
                .resizable(true)
                .frame(
                    egui::Frame::new()
                        .fill(pal.panel_bg)
                        .stroke(Stroke::new(1.0_f32, pal.border))
                        .inner_margin(egui::Margin::same(12)),
                )
                .show(ctx, |ui| {
                    if self.active_tab == ActiveTab::Speak {
                        self.draw_voice_pool_sidebar(ui, &pal);
                    } else {
                        self.draw_saved_profiles_sidebar(ui, &pal);
                    }
                });

            // Zone 4: Right Parameter Inspector & Recent Takes
            egui::SidePanel::right("inspector_panel")
                .default_width(295.0)
                .width_range(255.0..=360.0)
                .resizable(true)
                .frame(
                    egui::Frame::new()
                        .fill(pal.panel_bg)
                        .stroke(Stroke::new(1.0_f32, pal.border))
                        .inner_margin(egui::Margin::same(12)),
                )
                .show(ctx, |ui| {
                    self.draw_inspector_sidebar(
                        ui,
                        &pal,
                        self.active_tab == ActiveTab::CloneVoice,
                    );
                });
        }

        // Zone 5: Central Script & Studio Canvas
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(pal.bg).inner_margin(egui::Margin::same(14)))
            .show(ctx, |ui| {
                if let Some(banner) = self.banner.clone() {
                    self.draw_banner(ui, &pal, &banner);
                    ui.add_space(10.0);
                }

                if self.active_tab != ActiveTab::ModelsSettings {
                    if let Some(BusyTask::LoadingModel(loading_name)) = self.busy.clone() {
                        self.draw_loading_progress_ui(ui, &pal, &loading_name);
                        ui.add_space(10.0);
                    }
                }

                match self.active_tab {
                    ActiveTab::Speak => self.draw_speak_canvas(ui, &pal),
                    ActiveTab::CloneVoice => self.draw_clone_canvas(ui, &pal),
                    ActiveTab::ModelsSettings => self.draw_models_tab(ui, &pal),
                }
            });

        self.draw_quick_guide_window(ctx, &pal);
        self.draw_tags_guide_window(ctx, &pal);
        self.draw_inspector_window(ctx, &pal);
    }
}

impl VoxaStudioApp {
    fn draw_header(
        &mut self,
        ui: &mut egui::Ui,
        pal: &StudioPalette,
        snap: &PlaybackSnapshot,
    ) {
        let mut next_tab: Option<ActiveTab> = None;
        let mut open_models_from_pill = false;
        let mut export_path_opt: Option<PathBuf> = None;
        let mut toggle_guide = false;
        let mut toggle_theme = false;
        let mut free_ram_clicked = false;

        card_header_row(
            ui,
            |ui| {
                draw_voxa_logo(ui, pal, 34.0);
                ui.label(
                    RichText::new("Voxa")
                        .size(17.5)
                        .strong()
                        .color(pal.ink),
                );
                pill_badge(
                    ui,
                    "STUDIO",
                    pal.primary.gamma_multiply(0.16),
                    pal.primary,
                );

                ui.add_space(12.0);

                for (tab, label) in [
                    (ActiveTab::Speak, "Speak Studio"),
                    (ActiveTab::CloneVoice, "Voice Clone"),
                    (ActiveTab::ModelsSettings, "Models & Engine"),
                ] {
                    let selected = self.active_tab == tab;
                    let btn = egui::Button::new(
                        RichText::new(label)
                            .size(13.5)
                            .strong()
                            .color(if selected { pal.on_primary } else { pal.ink }),
                    )
                    .fill(if selected { pal.primary } else { pal.card_bg })
                    .stroke(Stroke::new(
                        1.0_f32,
                        if selected { pal.primary } else { pal.border },
                    ))
                    .min_size(Vec2::new(0.0, 32.0))
                    .corner_radius(CornerRadius::same(8));
                    if ui.add(btn).clicked() {
                        next_tab = Some(tab);
                    }
                }
            },
            |ui| {
                let can_export = snap.loaded_path.is_some() && self.busy.is_none();
                let export_btn = egui::Button::new(
                    RichText::new("Export")
                        .size(13.0)
                        .strong()
                        .color(if can_export {
                            pal.on_primary
                        } else {
                            pal.ink_muted
                        }),
                )
                .fill(if can_export {
                    pal.primary
                } else {
                    pal.card_alt
                })
                .stroke(Stroke::new(
                    1.0_f32,
                    if can_export { pal.primary } else { pal.border },
                ))
                .min_size(Vec2::new(72.0, 30.0))
                .corner_radius(CornerRadius::same(8));
                if ui
                    .add_enabled(can_export, export_btn)
                    .on_hover_text("Export audio file (WAV / MP3 / FLAC / OGG)")
                    .clicked()
                {
                    export_path_opt = snap.loaded_path.clone();
                }

                if ui
                    .add(
                        egui::Button::new(RichText::new("Guide").size(12.5))
                            .min_size(Vec2::new(0.0, 30.0))
                            .corner_radius(CornerRadius::same(8)),
                    )
                    .clicked()
                {
                    toggle_guide = true;
                }

                if ui
                    .add_enabled(
                        self.model_info.is_some() && self.busy.is_none(),
                        egui::Button::new(RichText::new("Free RAM").size(12.0))
                            .min_size(Vec2::new(0.0, 30.0))
                            .corner_radius(CornerRadius::same(8)),
                    )
                    .on_hover_text("Trim caches, unload Whisper/encoder helpers, and release unused RAM back to Windows")
                    .clicked()
                {
                    free_ram_clicked = true;
                }

                let is_dark = self.config.theme == ThemeMode::Dark;
                let theme_btn_w = 68.0;
                let (t_rect, t_resp) =
                    ui.allocate_exact_size(Vec2::new(theme_btn_w, 30.0), egui::Sense::click());
                if ui.is_rect_visible(t_rect) {
                    let p = ui.painter_at(t_rect);
                    p.rect_filled(
                        t_rect,
                        CornerRadius::same(8),
                        if t_resp.hovered() {
                            pal.card_alt
                        } else {
                            pal.card_bg
                        },
                    );
                    p.rect_stroke(
                        t_rect,
                        CornerRadius::same(8),
                        Stroke::new(1.0_f32, pal.border_strong),
                        egui::StrokeKind::Inside,
                    );
                    let icon_r = egui::Rect::from_min_size(
                        egui::Pos2::new(t_rect.left() + 8.0, t_rect.center().y - 7.5),
                        Vec2::splat(15.0),
                    );
                    draw_theme_icon(&p, icon_r, is_dark, pal.ink);
                    p.text(
                        egui::Pos2::new(icon_r.right() + 6.0, t_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        if is_dark { "Light" } else { "Dark" },
                        egui::FontId::proportional(12.5),
                        pal.ink,
                    );
                }
                if t_resp
                    .on_hover_text(if is_dark {
                        "Switch to Light Mode"
                    } else {
                        "Switch to Dark Studio Mode"
                    })
                    .clicked()
                {
                    toggle_theme = true;
                }

                let (pill_text, pill_bg, pill_fg) =
                    if let Some(BusyTask::LoadingModel(m)) = &self.busy {
                        (
                            format!("Loading {}...", short_model_name(m)),
                            pal.warning_soft,
                            pal.warning,
                        )
                    } else if let Some(info) = &self.model_info {
                        let wt = info
                            .weights
                            .as_deref()
                            .map(|w| format!(" · {w}"))
                            .unwrap_or_default();
                        (
                            format!("{}{}", short_model_name(&info.model_name), wt),
                            pal.success_soft,
                            pal.success,
                        )
                    } else {
                        (
                            "No model loaded".to_string(),
                            pal.danger_soft,
                            pal.danger,
                        )
                    };

                let pill_resp = ui.add(
                    egui::Button::new(
                        RichText::new(pill_text).size(12.0).strong().color(pill_fg),
                    )
                    .fill(pill_bg)
                    .stroke(Stroke::new(1.0_f32, pill_fg.gamma_multiply(0.35)))
                    .min_size(Vec2::new(0.0, 30.0))
                    .corner_radius(CornerRadius::same(15)),
                );
                if pill_resp
                    .on_hover_text("Click to switch models or open Engine settings")
                    .clicked()
                {
                    open_models_from_pill = true;
                }
            },
        );

        if let Some(t) = next_tab {
            self.active_tab = t;
        }
        if open_models_from_pill {
            self.active_tab = ActiveTab::ModelsSettings;
        }
        if toggle_guide {
            self.show_quick_guide = !self.show_quick_guide;
        }
        if free_ram_clicked {
            if let Some(worker) = self.worker.as_mut() {
                let _ = worker.send(&WorkerRequest::FreeMemory {
                    unload_model: false,
                });
            }
        }
        if toggle_theme {
            self.config.theme = match self.config.theme {
                ThemeMode::Dark => ThemeMode::Light,
                ThemeMode::Light => ThemeMode::Dark,
            };
            self.storage.save_config(&self.config);
        }
        if let Some(p) = export_path_opt {
            self.export_audio_with_dialog(&p);
        }
    }

    fn draw_timeline_dock(
        &mut self,
        ui: &mut egui::Ui,
        pal: &StudioPalette,
        snap: &PlaybackSnapshot,
    ) {
        let has_audio = snap.loaded_path.is_some() || snap.duration_secs > 0.0;
        let mut do_toggle_pause = false;
        let mut do_stop = false;
        let mut do_replay = false;
        let mut export_path_opt: Option<PathBuf> = None;

        // Row 1: Transport & Timecode (left) + Stats, Volume, Export Format & Actions (right)
        card_header_row(
            ui,
            |ui| {
                if vector_icon_button(
                    ui,
                    Vec2::splat(34.0),
                    pal.primary,
                    Stroke::NONE,
                    17,
                    has_audio,
                    |p, r, col| {
                        if snap.is_playing {
                            draw_pause_icon(p, r, col);
                        } else {
                            draw_play_icon(p, r, col);
                        }
                    },
                    pal.on_primary,
                )
                .on_hover_text(if snap.is_playing {
                    "Pause (Space)"
                } else {
                    "Play (Space)"
                })
                .clicked()
                {
                    do_toggle_pause = true;
                }

                if vector_icon_button(
                    ui,
                    Vec2::splat(30.0),
                    pal.card_bg,
                    Stroke::new(1.0_f32, pal.border_strong),
                    8,
                    has_audio,
                    draw_stop_icon,
                    pal.ink,
                )
                .on_hover_text("Stop")
                .clicked()
                {
                    do_stop = true;
                }

                if vector_icon_button(
                    ui,
                    Vec2::splat(30.0),
                    pal.card_bg,
                    Stroke::new(1.0_f32, pal.border_strong),
                    8,
                    has_audio,
                    draw_replay_icon,
                    pal.ink,
                )
                .on_hover_text("Replay from start")
                .clicked()
                {
                    do_replay = true;
                }

                ui.label(
                    RichText::new(format!(
                        "{} / {}",
                        format_mmss(snap.position_secs),
                        format_mmss(snap.duration_secs)
                    ))
                    .monospace()
                    .strong()
                    .size(13.0)
                    .color(pal.ink),
                );

                if !self.last_gen_summary.is_empty() {
                    pill_badge(ui, &self.last_gen_summary, pal.success_soft, pal.success);
                }
            },
            |ui| {
                if let Some(path) = snap.loaded_path.clone() {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Folder").size(12.0))
                                .min_size(Vec2::new(0.0, 28.0)),
                        )
                        .on_hover_text("Reveal audio file in Explorer")
                        .clicked()
                    {
                        Self::open_folder_for_path(&path);
                    }

                    if ui
                        .add_enabled(
                            self.busy.is_none(),
                            egui::Button::new(RichText::new("Save As...").strong().size(12.0))
                                .min_size(Vec2::new(0.0, 28.0)),
                        )
                        .clicked()
                    {
                        export_path_opt = Some(path);
                    }

                    egui::ComboBox::from_id_salt("dock_export_fmt")
                        .selected_text(self.export_format.ext().to_uppercase())
                        .width(64.0)
                        .show_ui(ui, |ui| {
                            for fmt in [
                                ExportFormat::Wav,
                                ExportFormat::Mp3,
                                ExportFormat::Flac,
                                ExportFormat::Ogg,
                            ] {
                                ui.selectable_value(&mut self.export_format, fmt, fmt.label());
                            }
                        });
                }

                ui.spacing_mut().slider_width = 80.0;
                let mut vol = self.config.volume;
                if ui
                    .add(egui::Slider::new(&mut vol, 0.0..=1.2).show_value(false))
                    .on_hover_text(format!("Volume: {:.0}%", vol * 100.0))
                    .changed()
                {
                    self.config.volume = vol;
                    self.audio.set_volume(vol);
                    self.storage.save_config(&self.config);
                }
                ui.label(RichText::new("Vol").size(12.0).color(pal.ink_muted));
            },
        );

        if do_toggle_pause {
            self.audio.toggle_pause();
        }
        if do_stop {
            self.audio.stop();
        }
        if do_replay {
            self.audio.replay();
        }
        if let Some(p) = export_path_opt {
            self.export_audio_with_dialog(&p);
        }

        ui.add_space(6.0);

        // Row 2: Full-width Studio Audio Timeline Track
        let progress = if snap.duration_secs > 0.0 {
            snap.position_secs / snap.duration_secs
        } else {
            0.0
        };
        if let Some(seek_frac) = waveform_widget(
            ui,
            pal,
            &snap.peaks,
            progress,
            snap.duration_secs,
            60.0,
        ) {
            self.audio.seek(seek_frac);
        }
    }

    fn draw_banner(&mut self, ui: &mut egui::Ui, pal: &StudioPalette, banner: &BannerMessage) {
        let (bg, fg) = match banner.kind {
            BannerKind::Info => (pal.accent_blue_soft, pal.accent_blue),
            BannerKind::Success => (pal.success_soft, pal.success),
            BannerKind::Warning => (pal.warning_soft, pal.warning),
            BannerKind::Error => (pal.danger_soft, pal.danger),
        };
        egui::Frame::new()
            .fill(bg)
            .stroke(Stroke::new(1.0_f32, fg.gamma_multiply(0.45)))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(12, 7))
            .show(ui, |ui| {
                card_header_row(
                    ui,
                    |ui| {
                        ui.label(RichText::new(&banner.text).color(fg).strong().size(13.0));
                    },
                    |ui| {
                        if ui.small_button("Dismiss").clicked() {
                            self.banner = None;
                        }
                        if banner.detail.is_some() && ui.small_button("View Log").clicked() {
                            self.active_tab = ActiveTab::ModelsSettings;
                            self.force_open_logs = true;
                        }
                    },
                );
            });
    }

    fn draw_voice_pool_sidebar(&mut self, ui: &mut egui::Ui, pal: &StudioPalette) {
        card_header_row(
            ui,
            |ui| {
                ui.label(RichText::new("Voice Pool").size(15.0).strong().color(pal.ink));
            },
            |ui| {
                let fav_bg = if self.favorites_only {
                    pal.primary
                } else {
                    pal.card_bg
                };
                let fav_fg = if self.favorites_only {
                    pal.on_primary
                } else {
                    pal.ink
                };
                let resp = vector_icon_button(
                    ui,
                    Vec2::new(86.0, 26.0),
                    fav_bg,
                    Stroke::new(
                        1.0_f32,
                        if self.favorites_only {
                            pal.primary
                        } else {
                            pal.border_strong
                        },
                    ),
                    13,
                    true,
                    |p, r, col| {
                        let star_r = egui::Rect::from_min_size(r.min, Vec2::splat(r.height()));
                        draw_star_icon(p, star_r, self.favorites_only, col);
                        p.text(
                            egui::Pos2::new(star_r.right() + 4.0, r.center().y),
                            egui::Align2::LEFT_CENTER,
                            "Favorites",
                            egui::FontId::proportional(11.5),
                            col,
                        );
                    },
                    fav_fg,
                );
                if resp.clicked() {
                    self.favorites_only = !self.favorites_only;
                }
            },
        );

        ui.add_space(6.0);

        let Some(info) = self.model_info.clone() else {
            ui.add_space(12.0);
            ui.label(RichText::new("Loading voice catalog...").color(pal.ink_muted));
            return;
        };

        if info.languages.len() > 1 {
            let current_label = if self.selected_language == "All" {
                "All languages".to_string()
            } else {
                language_display_name(&self.selected_language)
            };
            let full_w = ui.available_width();
            egui::ComboBox::from_id_salt("voice_lang_filter")
                .selected_text(current_label)
                .width(full_w)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(self.selected_language == "All", "All languages")
                        .clicked()
                    {
                        self.selected_language = "All".to_string();
                    }
                    for lang in &info.languages {
                        let label = language_display_name(lang);
                        if ui
                            .selectable_label(&self.selected_language == lang, label)
                            .clicked()
                        {
                            self.selected_language = lang.clone();
                            if let Some(first) = info.voices.iter().find(|v| &v.language == lang) {
                                self.on_voice_selected(&info, first);
                            }
                        }
                    }
                });
            ui.add_space(4.0);
        }

        // Search box
        let search_w = ui.available_width();
        egui::Frame::new()
            .fill(pal.input_bg)
            .stroke(Stroke::new(1.0_f32, pal.border_strong))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(8, 5))
            .show(ui, |ui| {
                let inner_w = (search_w - 18.0).max(100.0);
                ui.set_min_width(inner_w);
                ui.set_max_width(inner_w);
                ui.horizontal(|ui| {
                    let (icon_rect, _) =
                        ui.allocate_exact_size(Vec2::splat(15.0), egui::Sense::hover());
                    draw_search_icon(&ui.painter_at(icon_rect), icon_rect, pal.ink_muted);

                    let has_q = !self.voice_search.is_empty();
                    let edit_w = (inner_w - if has_q { 46.0 } else { 22.0 }).max(60.0);
                    ui.add_sized(
                        Vec2::new(edit_w, 19.0),
                        egui::TextEdit::singleline(&mut self.voice_search)
                            .frame(false)
                            .hint_text("Search voices or style..."),
                    );
                    if has_q
                        && vector_icon_button(
                            ui,
                            Vec2::splat(18.0),
                            Color32::TRANSPARENT,
                            Stroke::NONE,
                            6,
                            true,
                            draw_close_icon,
                            pal.ink_muted,
                        )
                        .clicked()
                    {
                        self.voice_search.clear();
                    }
                });
            });

        ui.add_space(4.0);

        let query = self.voice_search.trim().to_ascii_lowercase();
        let filtered: Vec<VoiceEntry> = info
            .voices
            .iter()
            .filter(|v| {
                if self.selected_language != "All" && v.language != self.selected_language {
                    return false;
                }
                if self.favorites_only && !self.config.favorites.contains(&v.name) {
                    return false;
                }
                if !query.is_empty() {
                    let desc = if v.description.is_empty() {
                        builtin_voice_description(&v.name).unwrap_or("")
                    } else {
                        v.description.as_str()
                    };
                    return v.name.to_ascii_lowercase().contains(&query)
                        || desc.to_ascii_lowercase().contains(&query)
                        || v.language.to_ascii_lowercase().contains(&query);
                }
                true
            })
            .cloned()
            .collect();

        ui.label(
            RichText::new(format!("{} voices", filtered.len()))
                .size(11.5)
                .color(pal.ink_muted),
        );
        ui.add_space(4.0);

        let mut to_sample: Option<VoiceEntry> = None;
        let mut to_select: Option<VoiceEntry> = None;
        let mut toggle_fav: Option<String> = None;

        egui::ScrollArea::vertical()
            .id_salt("voice_list_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let list_w = ui.available_width();
                for voice in &filtered {
                    let is_selected = self.selected_voice == voice.name;
                    let is_fav = self.config.favorites.contains(&voice.name);
                    let desc = if !voice.description.is_empty() {
                        voice.description.as_str()
                    } else {
                        builtin_voice_description(&voice.name).unwrap_or("Built-in voice")
                    };

                    let card_fill = if is_selected {
                        pal.card_bg
                    } else {
                        pal.card_alt
                    };
                    let border_col = if is_selected {
                        pal.primary
                    } else {
                        pal.border
                    };

                    egui::Frame::new()
                        .fill(card_fill)
                        .stroke(Stroke::new(
                            if is_selected { 1.5_f32 } else { 1.0_f32 },
                            border_col,
                        ))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(egui::Margin::symmetric(9, 7))
                        .show(ui, |ui| {
                            let inner_w = (list_w - 22.0).max(160.0);
                            ui.set_min_width(inner_w);
                            ui.set_max_width(inner_w);

                            card_header_row(
                                ui,
                                |ui| {
                                    draw_voice_avatar(ui, pal, &voice.name, is_selected);
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 1.0;
                                        ui.horizontal(|ui| {
                                            let name_btn = egui::Button::new(
                                                RichText::new(&voice.name)
                                                    .strong()
                                                    .size(13.5)
                                                    .color(if is_selected {
                                                        pal.primary
                                                    } else {
                                                        pal.ink
                                                    }),
                                            )
                                            .frame(false);
                                            if ui.add(name_btn).clicked() {
                                                to_select = Some(voice.clone());
                                            }
                                            if self.selected_language == "All"
                                                && info.languages.len() > 1
                                            {
                                                pill_badge(
                                                    ui,
                                                    &voice.language,
                                                    pal.accent_blue_soft,
                                                    pal.accent_blue,
                                                );
                                            }
                                        });
                                        if ui
                                            .add(
                                                egui::Label::new(
                                                    RichText::new(desc)
                                                        .size(11.5)
                                                        .color(pal.ink_muted),
                                                )
                                                .sense(egui::Sense::click()),
                                            )
                                            .clicked()
                                        {
                                            to_select = Some(voice.clone());
                                        }
                                    });
                                },
                                |ui| {
                                    if vector_icon_button(
                                        ui,
                                        Vec2::splat(24.0),
                                        Color32::TRANSPARENT,
                                        Stroke::NONE,
                                        6,
                                        true,
                                        |p, r, col| draw_star_icon(p, r, is_fav, col),
                                        if is_fav { pal.primary } else { pal.ink_muted },
                                    )
                                    .on_hover_text("Pin to Favorites")
                                    .clicked()
                                    {
                                        toggle_fav = Some(voice.name.clone());
                                    }

                                    if ui
                                        .add_enabled(
                                            self.busy.is_none(),
                                            egui::Button::new(
                                                RichText::new("Sample").size(11.5),
                                            )
                                            .min_size(Vec2::new(52.0, 24.0))
                                            .corner_radius(CornerRadius::same(7)),
                                        )
                                        .on_hover_text("Hear a quick voice preview")
                                        .clicked()
                                    {
                                        to_sample = Some(voice.clone());
                                    }
                                },
                            );
                        });
                    ui.add_space(5.0);
                }
            });

        if let Some(name) = toggle_fav {
            if !self.config.favorites.remove(&name) {
                self.config.favorites.insert(name);
            }
            self.storage.save_config(&self.config);
        }
        if let Some(v) = to_select {
            self.on_voice_selected(&info, &v);
        }
        if let Some(v) = to_sample {
            self.sample_voice(&v);
        }
    }

    fn draw_saved_profiles_sidebar(&mut self, ui: &mut egui::Ui, pal: &StudioPalette) {
        ui.label(
            RichText::new("Voice Profiles")
                .size(15.0)
                .strong()
                .color(pal.ink),
        );
        ui.label(
            RichText::new("Save uploaded or recorded clips here to reuse anytime.")
                .size(12.0)
                .color(pal.ink_muted),
        );
        ui.add_space(6.0);

        // Save current clip as profile
        let box_w = ui.available_width();
        egui::Frame::new()
            .fill(pal.card_alt)
            .stroke(Stroke::new(1.0_f32, pal.border))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                let inner_w = (box_w - 22.0).max(120.0);
                ui.set_min_width(inner_w);
                ui.set_max_width(inner_w);
                ui.add_sized(
                    Vec2::new(inner_w, 26.0),
                    egui::TextEdit::singleline(&mut self.new_profile_name)
                        .hint_text("Profile name (e.g. Mom)"),
                );
                ui.add_space(6.0);
                if ui
                    .add_enabled(
                        self.clone_ref_path.is_some(),
                        egui::Button::new("Save Current Clip as Profile")
                            .min_size(Vec2::new(inner_w, 28.0)),
                    )
                    .clicked()
                {
                    self.save_current_voice_profile();
                }
            });

        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("Saved ({})", self.saved_voices.len()))
                .size(12.0)
                .color(pal.ink_muted),
        );
        ui.add_space(4.0);

        egui::ScrollArea::vertical()
            .id_salt("saved_profiles_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.saved_voices.is_empty() {
                    ui.label(
                        RichText::new("No saved profiles yet.")
                            .size(12.5)
                            .color(pal.ink_muted),
                    );
                    return;
                }

                let list_w = ui.available_width();
                let mut select_prof: Option<SavedVoiceProfile> = None;
                let mut play_prof: Option<PathBuf> = None;
                let mut del_prof: Option<String> = None;

                for prof in &self.saved_voices {
                    let selected = self.selected_profile_id.as_deref() == Some(&prof.id);
                    egui::Frame::new()
                        .fill(if selected { pal.card_bg } else { pal.card_alt })
                        .stroke(Stroke::new(
                            if selected { 1.5_f32 } else { 1.0_f32 },
                            if selected { pal.primary } else { pal.border },
                        ))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(egui::Margin::symmetric(9, 7))
                        .show(ui, |ui| {
                            let inner_w = (list_w - 22.0).max(140.0);
                            ui.set_min_width(inner_w);
                            ui.set_max_width(inner_w);
                            card_header_row(
                                ui,
                                |ui| {
                                    draw_voice_avatar(ui, pal, &prof.name, selected);
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 1.0;
                                        if ui
                                            .selectable_label(
                                                selected,
                                                RichText::new(&prof.name).strong().size(13.5),
                                            )
                                            .clicked()
                                        {
                                            select_prof = Some(prof.clone());
                                        }
                                        ui.label(
                                            RichText::new(format!(
                                                "{:.1}s · {}",
                                                prof.duration_secs, prof.created_at
                                            ))
                                            .size(11.5)
                                            .color(pal.ink_muted),
                                        );
                                    });
                                },
                                |ui| {
                                    if vector_icon_button(
                                        ui,
                                        Vec2::splat(24.0),
                                        pal.accent_blue_soft,
                                        Stroke::NONE,
                                        7,
                                        true,
                                        draw_play_icon,
                                        pal.accent_blue,
                                    )
                                    .on_hover_text("Play clip")
                                    .clicked()
                                    {
                                        play_prof = Some(PathBuf::from(&prof.wav_path));
                                    }
                                    if vector_icon_button(
                                        ui,
                                        Vec2::splat(22.0),
                                        Color32::TRANSPARENT,
                                        Stroke::NONE,
                                        6,
                                        true,
                                        draw_close_icon,
                                        pal.ink_muted,
                                    )
                                    .on_hover_text("Delete profile")
                                    .clicked()
                                    {
                                        del_prof = Some(prof.id.clone());
                                    }
                                },
                            );
                        });
                    ui.add_space(5.0);
                }

                if let Some(p) = select_prof {
                    self.selected_profile_id = Some(p.id);
                    self.clone_ref_path = Some(PathBuf::from(p.wav_path));
                    self.clone_ref_duration = Some(p.duration_secs);
                    self.clone_transcript = p.transcript;
                }
                if let Some(path) = play_prof {
                    self.audio.load_and_play(path, true);
                }
                if let Some(id) = del_prof {
                    if let Some(pos) = self.saved_voices.iter().position(|v| v.id == id) {
                        let removed = self.saved_voices.remove(pos);
                        let _ = std::fs::remove_file(&removed.wav_path);
                        self.storage.save_saved_voices(&self.saved_voices);
                    }
                }
            });
    }

    fn draw_inspector_sidebar(
        &mut self,
        ui: &mut egui::Ui,
        pal: &StudioPalette,
        use_clone: bool,
    ) {
        ui.label(
            RichText::new("Inspector")
                .size(15.0)
                .strong()
                .color(pal.ink),
        );
        ui.add_space(4.0);

        egui::ScrollArea::vertical()
            .id_salt("right_inspector_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let panel_w = ui.available_width();
                // Voice Settings Card
                egui::Frame::new()
                    .fill(pal.card_alt)
                    .stroke(Stroke::new(1.0_f32, pal.border))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        let inner_w = (panel_w - 22.0).max(160.0);
                        ui.set_min_width(inner_w);
                        ui.set_max_width(inner_w);
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            let Some(info) = self.model_info.clone() else {
                                ui.label(
                                    RichText::new("Settings appear once model loads.")
                                        .size(12.5)
                                        .color(pal.ink_muted),
                                );
                                return;
                            };

                            if info.supports_speed {
                                ui.label(
                                    RichText::new("Speech Speed")
                                        .strong()
                                        .size(13.0)
                                        .color(pal.ink),
                                );
                                ui.spacing_mut().slider_width = (inner_w - 60.0).max(110.0);
                                ui.add(egui::Slider::new(&mut self.speed, 0.5..=2.0).suffix("x"));
                                ui.horizontal(|ui| {
                                    for (val, label) in [
                                        (0.85, "0.85x"),
                                        (1.0, "1.0x"),
                                        (1.2, "1.2x"),
                                    ] {
                                        if ui
                                            .selectable_label(
                                                (self.speed - val).abs() < 0.02,
                                                label,
                                            )
                                            .clicked()
                                        {
                                            self.speed = val;
                                        }
                                    }
                                });
                                ui.add_space(6.0);
                            }

                            if !use_clone && info.supports_voice_blending && info.voices.len() >= 2 {
                                ui.checkbox(&mut self.blend_enabled, "Blend two voices (Kokoro)");
                                if self.blend_enabled {
                                    if self.blend_second_voice.is_empty()
                                        || self.blend_second_voice == self.selected_voice
                                    {
                                        self.blend_second_voice = info
                                            .voices
                                            .iter()
                                            .find(|v| v.name != self.selected_voice)
                                            .map(|v| v.name.clone())
                                            .unwrap_or_default();
                                    }
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new("2nd:")
                                                .size(12.0)
                                                .color(pal.ink_muted),
                                        );
                                        egui::ComboBox::from_id_salt("kokoro_blend_second_voice")
                                            .selected_text(&self.blend_second_voice)
                                            .width((inner_w - 42.0).max(110.0))
                                            .show_ui(ui, |ui| {
                                                for v in &info.voices {
                                                    if v.name == self.selected_voice {
                                                        continue;
                                                    }
                                                    ui.selectable_value(
                                                        &mut self.blend_second_voice,
                                                        v.name.clone(),
                                                        format!("{} ({})", v.name, v.language),
                                                    );
                                                }
                                            });
                                    });
                                    ui.spacing_mut().slider_width = (inner_w - 75.0).max(95.0);
                                    ui.add(
                                        egui::Slider::new(&mut self.blend_primary_pct, 10..=90)
                                            .suffix("%"),
                                    );
                                    ui.label(
                                        RichText::new(format!(
                                            "{}% {} + {}% {}",
                                            self.blend_primary_pct,
                                            self.selected_voice,
                                            100 - self.blend_primary_pct,
                                            self.blend_second_voice
                                        ))
                                        .size(11.0)
                                        .color(pal.primary),
                                    );
                                    ui.horizontal(|ui| {
                                        for (pct, lbl) in [(70, "70/30"), (50, "50/50"), (30, "30/70")] {
                                            if ui
                                                .selectable_label(self.blend_primary_pct == pct, lbl)
                                                .clicked()
                                            {
                                                self.blend_primary_pct = pct;
                                            }
                                        }
                                    });
                                }
                                ui.add_space(6.0);
                            }

                            if info.family == "kitten2" {
                                ui.label(
                                    RichText::new("Delivery Mode")
                                        .strong()
                                        .size(13.0)
                                        .color(pal.ink),
                                );
                                ui.radio_value(
                                    &mut self.decoding_mode,
                                    DecodingMode::Stable,
                                    "Stable (Steady)",
                                );
                                ui.radio_value(
                                    &mut self.decoding_mode,
                                    DecodingMode::Expressive,
                                    "Expressive (Lively)",
                                );
                                ui.radio_value(
                                    &mut self.decoding_mode,
                                    DecodingMode::Custom,
                                    "Custom Sampling",
                                );

                                if self.decoding_mode == DecodingMode::Custom {
                                    ui.add_space(4.0);
                                    ui.spacing_mut().slider_width = (inner_w - 95.0).max(90.0);
                                    ui.add(
                                        egui::Slider::new(&mut self.temperature, 0.3..=2.0)
                                            .text("Temp"),
                                    );
                                    ui.add(
                                        egui::Slider::new(&mut self.top_k, 0..=200)
                                            .text("top_k"),
                                    );
                                    ui.add(
                                        egui::Slider::new(&mut self.top_p, 0.5..=1.0)
                                            .text("top_p"),
                                    );
                                    ui.add(
                                        egui::Slider::new(&mut self.min_p, 0.0..=0.5)
                                            .text("min_p"),
                                    );
                                }
                                ui.add_space(6.0);
                            }

                            ui.label(
                                RichText::new("Playback & Text")
                                    .strong()
                                    .size(13.0)
                                    .color(pal.ink),
                            );
                            if info.family == "kitten2" || info.family == "onnx_legacy" {
                                ui.checkbox(
                                    &mut self.normalize_text,
                                    "Expand numbers & dates",
                                );
                            }
                            if ui
                                .checkbox(&mut self.config.stream_playback, "Stream chunks live")
                                .changed()
                            {
                                self.storage.save_config(&self.config);
                            }
                            if ui
                                .checkbox(&mut self.config.auto_play, "Auto-play generated")
                                .changed()
                            {
                                self.storage.save_config(&self.config);
                            }

                            if info.supports_normalize || info.supports_phonemize {
                                ui.add_space(4.0);
                                ui.horizontal_wrapped(|ui| {
                                    if info.supports_normalize
                                        && ui.small_button("Preview Spoken Text").clicked()
                                    {
                                        let txt = if use_clone {
                                            self.clone_text.clone()
                                        } else {
                                            self.speak_text.clone()
                                        };
                                        if let Some(worker) = self.worker.as_mut() {
                                            let _ = worker
                                                .send(&WorkerRequest::Normalize { text: txt });
                                        }
                                    }
                                    if info.supports_phonemize
                                        && ui.small_button("Preview Phonemes").clicked()
                                    {
                                        let txt = if use_clone {
                                            self.clone_text.clone()
                                        } else {
                                            self.speak_text.clone()
                                        };
                                        let lang = if self.selected_language != "All" {
                                            Some(self.selected_language.clone())
                                        } else {
                                            None
                                        };
                                        if let Some(worker) = self.worker.as_mut() {
                                            let _ = worker.send(&WorkerRequest::Phonemize {
                                                text: txt,
                                                language: lang,
                                            });
                                        }
                                    }
                                });
                            }

                            if info.family == "kitten2" {
                                ui.add_space(6.0);
                                egui::CollapsingHeader::new(
                                    RichText::new("Advanced LM Knobs")
                                        .size(12.5)
                                        .color(pal.ink_muted),
                                )
                                .id_salt(if use_clone { "adv_clone" } else { "adv_speak" })
                                .default_open(false)
                                .show(ui, |ui| {
                                    ui.spacing_mut().slider_width = (inner_w - 110.0).max(80.0);
                                    ui.add(
                                        egui::Slider::new(&mut self.max_new_tokens, 250..=3000)
                                            .step_by(250.0)
                                            .text("Max tokens"),
                                    );
                                    ui.add(
                                        egui::Slider::new(&mut self.repetition_penalty, 1.0..=2.0)
                                            .text("Rep penalty"),
                                    );
                                    ui.add(
                                        egui::Slider::new(&mut self.repetition_window, 0..=250)
                                            .text("Rep window"),
                                    );
                                    ui.add(
                                        egui::Slider::new(&mut self.token_run_penalty, 1.0..=2.5)
                                            .text("Run penalty"),
                                    );
                                    ui.add(
                                        egui::Slider::new(&mut self.token_run_grace, 2..=30)
                                            .text("Run grace"),
                                    );
                                    ui.add(
                                        egui::Slider::new(&mut self.chunk_chars, 150..=800)
                                            .text("Chunk chars"),
                                    );
                                    ui.add(
                                        egui::Slider::new(&mut self.chunk_min_chars, 40..=300)
                                            .text("Min chars"),
                                    );
                                    ui.add(
                                        egui::Slider::new(&mut self.chunk_gap_s, 0.0..=1.0)
                                            .suffix("s")
                                            .text("Pause"),
                                    );

                                    ui.label(
                                        RichText::new("Emotion conditioning:")
                                            .size(12.0)
                                            .color(pal.ink_muted),
                                    );
                                    ui.horizontal(|ui| {
                                        ui.radio_value(
                                            &mut self.emotion_override,
                                            EmotionOverride::Auto,
                                            "Auto",
                                        );
                                        ui.radio_value(
                                            &mut self.emotion_override,
                                            EmotionOverride::ForceOn,
                                            "On",
                                        );
                                        ui.radio_value(
                                            &mut self.emotion_override,
                                            EmotionOverride::ForceOff,
                                            "Off",
                                        );
                                    });

                                    ui.horizontal(|ui| {
                                        ui.checkbox(&mut self.use_seed, "Seed:");
                                        ui.add_enabled(
                                            self.use_seed,
                                            egui::DragValue::new(&mut self.seed)
                                                .range(0..=999_999),
                                        );
                                        if ui.small_button("Rand").clicked() {
                                            self.use_seed = true;
                                            self.seed =
                                                (Local::now().timestamp_subsec_millis() % 99_999)
                                                    + 1;
                                        }
                                    });
                                });
                            }
                        });
                    });

                ui.add_space(12.0);

                // Recent Takes Section
                card_header_row(
                    ui,
                    |ui| {
                        ui.label(
                            RichText::new(format!("Recent Takes ({})", self.history.len()))
                                .size(14.5)
                                .strong()
                                .color(pal.ink),
                        );
                    },
                    |_| {},
                );
                ui.add_space(4.0);

                if self.history.is_empty() {
                    ui.label(
                        RichText::new("Generated clips appear here for instant replay.")
                            .size(12.0)
                            .color(pal.ink_muted),
                    );
                } else {
                    let mut play_path: Option<PathBuf> = None;
                    let mut export_path: Option<PathBuf> = None;
                    let mut reuse_text: Option<String> = None;
                    let mut remove_id: Option<String> = None;

                    for item in &self.history {
                        egui::Frame::new()
                            .fill(pal.card_alt)
                            .stroke(Stroke::new(1.0_f32, pal.border))
                            .corner_radius(CornerRadius::same(9))
                            .inner_margin(egui::Margin::symmetric(9, 7))
                            .show(ui, |ui| {
                                let inner_w = (panel_w - 20.0).max(160.0);
                                ui.set_min_width(inner_w);
                                ui.set_max_width(inner_w);
                                ui.label(
                                    RichText::new(&item.title)
                                        .strong()
                                        .size(12.5)
                                        .color(pal.ink),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {:.1}s · {}",
                                        item.voice_label, item.duration_secs, item.created_at
                                    ))
                                    .size(11.0)
                                    .color(pal.ink_muted),
                                );
                                ui.add_space(3.0);
                                ui.horizontal(|ui| {
                                    if ui.small_button("Play").clicked() {
                                        play_path = Some(PathBuf::from(&item.wav_path));
                                    }
                                    if ui.small_button("Save").clicked() {
                                        export_path = Some(PathBuf::from(&item.wav_path));
                                    }
                                    if ui.small_button("Reuse").clicked() {
                                        reuse_text = Some(item.text.clone());
                                    }
                                    if ui.small_button("Delete").clicked() {
                                        remove_id = Some(item.id.clone());
                                    }
                                });
                            });
                        ui.add_space(5.0);
                    }

                    if let Some(p) = play_path {
                        self.audio.load_and_play(p, true);
                    }
                    if let Some(p) = export_path {
                        self.export_audio_with_dialog(&p);
                    }
                    if let Some(txt) = reuse_text {
                        if self.active_tab == ActiveTab::CloneVoice {
                            self.clone_text = txt;
                        } else {
                            self.speak_text = txt;
                        }
                    }
                    if let Some(id) = remove_id {
                        if let Some(pos) = self.history.iter().position(|h| h.id == id) {
                            let removed = self.history.remove(pos);
                            let _ = std::fs::remove_file(&removed.wav_path);
                            self.storage.save_history(&self.history);
                        }
                    }
                }
            });
    }

    fn on_voice_selected(&mut self, info: &ModelInfo, voice: &VoiceEntry) {
        self.selected_voice = voice.name.clone();
        if info.family == "kitten2" {
            let is_english = voice.language == "English";
            self.normalize_text = is_english;
            if self.speak_text.trim() == self.last_auto_sample.trim()
                || self.speak_text.trim().is_empty()
            {
                let sample = sample_text_for_language(&voice.language).to_string();
                self.last_auto_sample = sample.clone();
                self.speak_text = sample;
            }
        } else if (info.family == "kitten2_tiny" || info.family == "kokoro")
            && (self.speak_text.trim() == self.last_auto_sample.trim()
                || self.speak_text.trim().is_empty())
        {
            let sample = sample_text_for_language(&voice.language).to_string();
            self.last_auto_sample = sample.clone();
            self.speak_text = sample;
        }
    }

    fn draw_speak_canvas(&mut self, ui: &mut egui::Ui, pal: &StudioPalette) {
        egui::ScrollArea::vertical()
            .id_salt("speak_center_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.draw_script_and_generate_card(ui, pal, false);
            });
    }

    fn draw_script_and_generate_card(
        &mut self,
        ui: &mut egui::Ui,
        pal: &StudioPalette,
        use_clone: bool,
    ) {
        styled_card(ui, pal, |ui| {
            let supports_expression = self
                .model_info
                .as_ref()
                .map(|m| m.supports_expression)
                .unwrap_or(false);

            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(if use_clone { "2. Script" } else { "Script" })
                        .size(15.5)
                        .strong()
                        .color(pal.ink),
                );
                if !use_clone && !self.selected_voice.is_empty() {
                    let lang = self
                        .model_info
                        .as_ref()
                        .and_then(|m| {
                            m.voices
                                .iter()
                                .find(|v| v.name == self.selected_voice)
                                .map(|v| v.language.as_str())
                        })
                        .unwrap_or("");
                    let badge = if lang.is_empty() {
                        self.selected_voice.clone()
                    } else {
                        format!("{} · {lang}", self.selected_voice)
                    };
                    pill_badge(ui, &badge, pal.accent_blue_soft, pal.accent_blue);
                    if self.blend_enabled
                        && self
                            .model_info
                            .as_ref()
                            .is_some_and(|m| m.supports_voice_blending)
                        && !self.blend_second_voice.is_empty()
                    {
                        pill_badge(
                            ui,
                            &format!(
                                "Mix {}% + {}% {}",
                                self.blend_primary_pct,
                                100 - self.blend_primary_pct,
                                self.blend_second_voice
                            ),
                            pal.primary.gamma_multiply(0.18),
                            pal.primary,
                        );
                    }
                }
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt(if use_clone {
                    "clone_starters"
                } else {
                    "speak_starters"
                })
                .selected_text("Story Starters")
                .show_ui(ui, |ui| {
                    for starter in STORY_STARTERS {
                        let label = if starter.needs_kitten2 && !supports_expression {
                            format!("{} (Best on Kitten-TTS-2)", starter.title)
                        } else {
                            starter.title.to_string()
                        };
                        if ui.selectable_label(false, label).clicked() {
                            if use_clone {
                                self.clone_text = starter.text.to_string();
                            } else {
                                self.speak_text = starter.text.to_string();
                                self.last_auto_sample = starter.text.to_string();
                            }
                        }
                    }
                });
                if ui
                    .button("Import File / Book")
                    .on_hover_text("Import .txt, .md, .epub book, or .pdf document")
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Scripts & Books", &["txt", "md", "epub", "pdf"])
                        .pick_file()
                    {
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|s| s.to_ascii_lowercase())
                            .unwrap_or_default();
                        if ext == "epub" || ext == "pdf" {
                            self.open_book_document(&path);
                        } else if let Ok(content) = std::fs::read_to_string(&path) {
                            self.loaded_doc_title = None;
                            self.loaded_chapters.clear();
                            self.loaded_chapter_idx = 0;
                            if use_clone {
                                self.clone_text = content;
                            } else {
                                self.speak_text = content;
                            }
                        }
                    }
                }
                if ui.button("Clear").clicked() {
                    if use_clone {
                        self.clone_text.clear();
                    } else {
                        self.speak_text.clear();
                    }
                }
                if ui
                    .button("Tags Guide")
                    .on_hover_text("Open interactive guide for [emotion], <event>, and (((emphasis))) tags")
                    .clicked()
                {
                    self.show_tags_guide = true;
                }
            });

            ui.add_space(8.0);

            if !use_clone && !self.loaded_chapters.is_empty() {
                let doc_title = self
                    .loaded_doc_title
                    .as_deref()
                    .unwrap_or("Imported Document");
                let total_ch = self.loaded_chapters.len();
                let mut select_idx: Option<usize> = None;
                let mut merge_all = false;
                let mut close_doc = false;

                egui::Frame::new()
                    .fill(pal.card_alt)
                    .stroke(Stroke::new(1.0_f32, pal.border))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            pill_badge(
                                ui,
                                &format!("Book · {}", truncate_str(doc_title, 18)),
                                pal.accent_blue_soft,
                                pal.accent_blue,
                            );
                            if ui
                                .add_enabled(self.loaded_chapter_idx > 0, egui::Button::new("Prev").small())
                                .clicked()
                            {
                                select_idx = Some(self.loaded_chapter_idx.saturating_sub(1));
                            }
                            let cur_label = self
                                .loaded_chapters
                                .get(self.loaded_chapter_idx)
                                .map(|c| {
                                    format!(
                                        "{}/{}: {}",
                                        self.loaded_chapter_idx + 1,
                                        total_ch,
                                        truncate_str(&c.title, 20)
                                    )
                                })
                                .unwrap_or_default();
                            egui::ComboBox::from_id_salt("book_chapter_combo")
                                .selected_text(cur_label)
                                .width(175.0)
                                .show_ui(ui, |ui| {
                                    for (idx, ch) in self.loaded_chapters.iter().enumerate() {
                                        if ui
                                            .selectable_label(
                                                idx == self.loaded_chapter_idx,
                                                format!(
                                                    "{}. {} ({} chars)",
                                                    idx + 1,
                                                    truncate_str(&ch.title, 34),
                                                    ch.char_count
                                                ),
                                            )
                                            .clicked()
                                        {
                                            select_idx = Some(idx);
                                        }
                                    }
                                });
                            if ui
                                .add_enabled(
                                    self.loaded_chapter_idx + 1 < total_ch,
                                    egui::Button::new("Next").small(),
                                )
                                .clicked()
                            {
                                select_idx = Some((self.loaded_chapter_idx + 1).min(total_ch - 1));
                            }
                            if total_ch > 1 && ui.small_button("All").on_hover_text("Load all chapters combined").clicked() {
                                merge_all = true;
                            }
                            if ui.small_button("Close").clicked() {
                                close_doc = true;
                            }
                        });
                    });

                if let Some(idx) = select_idx {
                    if let Some(ch) = self.loaded_chapters.get(idx) {
                        self.loaded_chapter_idx = idx;
                        self.speak_text = ch.text.clone();
                    }
                } else if merge_all {
                    self.speak_text = self
                        .loaded_chapters
                        .iter()
                        .map(|c| c.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n\n");
                } else if close_doc {
                    self.loaded_doc_title = None;
                    self.loaded_chapters.clear();
                    self.loaded_chapter_idx = 0;
                }
                ui.add_space(6.0);
            }

            // Strictly bounded multiline script field
            let box_w = ui.available_width();
            let target = if use_clone {
                &mut self.clone_text
            } else {
                &mut self.speak_text
            };

            egui::Frame::new()
                .fill(pal.input_bg)
                .stroke(Stroke::new(1.5_f32, pal.border_strong))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    let inner_w = (box_w - 26.0).max(100.0);
                    ui.set_min_width(inner_w);
                    ui.set_max_width(inner_w);
                    egui::ScrollArea::vertical()
                        .id_salt(if use_clone {
                            "clone_txt_scroll"
                        } else {
                            "speak_txt_scroll"
                        })
                        .max_height(260.0)
                        .show(ui, |ui| {
                            let ink_col = pal.ink;
                            let emo_col = pal.emotion_chip;
                            let ev_col = pal.event_chip;
                            let emph_col = pal.emphasis_chip;
                            let mut layouter =
                                |ui: &egui::Ui, text: &str, wrap_width: f32| {
                                    let mut job = egui::text::LayoutJob::default();
                                    job.wrap.max_width = wrap_width;
                                    let font_id = egui::FontId::proportional(15.0);
                                    for seg in parse_expression_markup(text) {
                                        let fmt = match seg.kind {
                                            MarkupKind::Plain => egui::TextFormat {
                                                font_id: font_id.clone(),
                                                color: ink_col,
                                                ..Default::default()
                                            },
                                            MarkupKind::Emotion => egui::TextFormat {
                                                font_id: font_id.clone(),
                                                color: emo_col,
                                                background: emo_col.gamma_multiply(0.28),
                                                ..Default::default()
                                            },
                                            MarkupKind::VocalEvent => egui::TextFormat {
                                                font_id: font_id.clone(),
                                                color: ev_col,
                                                background: ev_col.gamma_multiply(0.28),
                                                ..Default::default()
                                            },
                                            MarkupKind::Emphasis => egui::TextFormat {
                                                font_id: font_id.clone(),
                                                color: emph_col,
                                                background: emph_col.gamma_multiply(0.30),
                                                underline: Stroke::new(1.5_f32, emph_col),
                                                ..Default::default()
                                            },
                                        };
                                        job.append(&seg.text, 0.0, fmt);
                                    }
                                    ui.fonts(|f| f.layout_job(job))
                                };
                            let text_out = egui::TextEdit::multiline(target)
                                .frame(false)
                                .font(egui::FontId::proportional(15.0))
                                .desired_rows(if use_clone || !self.loaded_chapters.is_empty() { 6 } else { 8 })
                                .desired_width(inner_w)
                                .layouter(&mut layouter)
                                .hint_text("Type or paste your story, dialogue, or script here... (Or drop a .txt, .md, .epub, or .pdf anywhere!)")
                                .show(ui);

                            if text_out.response.hovered() {
                                if let Some(pointer_pos) = ui.ctx().pointer_hover_pos() {
                                    let rel_pos = pointer_pos - text_out.galley_pos;
                                    let cursor = text_out.galley.cursor_from_pos(rel_pos);
                                    let caret_rect = text_out.galley.pos_from_cursor(&cursor);
                                    let within_row = rel_pos.y >= caret_rect.top() - 4.0
                                        && rel_pos.y <= caret_rect.bottom() + 4.0;
                                    if within_row {
                                        let char_idx = cursor.ccursor.index;
                                        let spans = parse_expression_spans(target);
                                        let hovered_span = spans.into_iter().find(|sp| {
                                            if sp.segment.kind == MarkupKind::Plain {
                                                return false;
                                            }
                                            let inside = sp.start_char <= char_idx
                                                && char_idx < sp.end_char
                                                && (rel_pos.x - caret_rect.left()).abs() <= 28.0;
                                            let on_closing_edge = char_idx == sp.end_char
                                                && rel_pos.x <= caret_rect.left() + 1.0
                                                && rel_pos.x >= caret_rect.left() - 10.0;
                                            inside || on_closing_edge
                                        });
                                        if let Some(sp) = hovered_span {
                                            if let Some(tip) = describe_markup_segment(&sp.segment) {
                                                let kind = sp.segment.kind;
                                                egui::show_tooltip_at_pointer(
                                                    ui.ctx(),
                                                    ui.layer_id(),
                                                    egui::Id::new("prompt_tag_hover_tip"),
                                                    |ui| draw_tag_tooltip_card(ui, pal, &tip, kind),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        });
                });

            ui.add_space(8.0);

            // Expression & Emotion Controls (always visible across all models)
            egui::CollapsingHeader::new(
                RichText::new("Expression & Emotion Markup (Tags Not Spoken Aloud)")
                    .size(13.0)
                    .strong()
                    .color(pal.ink),
            )
            .id_salt(if use_clone { "expr_clone" } else { "expr_speak" })
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if supports_expression {
                        pill_badge(
                            ui,
                            "Native Neural Mode (Kitten-TTS-2)",
                            pal.success_soft,
                            pal.success,
                        );
                    } else {
                        pill_badge(
                            ui,
                            "Studio Tone & SFX Mode (tags stripped from speech + pitch/tone/SFX applied)",
                            pal.accent_blue_soft,
                            pal.accent_blue,
                        );
                    }
                    if ui.small_button("Open Full Tags Guide").clicked() {
                        self.show_tags_guide = true;
                    }
                });
                ui.add_space(3.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("Emotion:")
                            .size(12.0)
                            .color(pal.emotion_chip)
                            .strong(),
                    );
                    for &(tag, tip) in EXPRESSION_TAGS {
                        let btn = egui::Button::new(
                            RichText::new(format!("[{tag}]"))
                                .size(11.5)
                                .color(pal.emotion_chip),
                        )
                        .fill(pal.emotion_chip.gamma_multiply(0.12))
                        .corner_radius(CornerRadius::same(12));
                        if ui.add(btn).on_hover_text(tip).clicked() {
                            apply_leading_emotion(target, tag);
                        }
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("Events:")
                            .size(12.0)
                            .color(pal.event_chip)
                            .strong(),
                    );
                    for &(ev, tip) in VOCAL_EVENT_TAGS {
                        let btn = egui::Button::new(
                            RichText::new(format!("<{ev}>"))
                                .size(11.5)
                                .color(pal.event_chip),
                        )
                        .fill(pal.event_chip.gamma_multiply(0.12))
                        .corner_radius(CornerRadius::same(12));
                        if ui.add(btn).on_hover_text(tip).clicked() {
                            append_inline_token(target, &format!("<{ev}>"));
                        }
                    }
                    let emph_btn = egui::Button::new(
                        RichText::new("(((emphasis)))")
                            .size(11.5)
                            .color(pal.emphasis_chip),
                    )
                    .fill(pal.emphasis_chip.gamma_multiply(0.14))
                    .corner_radius(CornerRadius::same(12));
                    if ui
                        .add(emph_btn)
                        .on_hover_text(
                            "Wrap the last word in (((triple parentheses))) to stress it",
                        )
                        .clicked()
                    {
                        emphasize_last_word(target);
                    }
                });

                if has_any_expression_markup(target) {
                    ui.add_space(3.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new("Detected Tags (hover any tag to see what it does):")
                                .size(11.5)
                                .color(pal.ink_muted),
                        );
                        for seg in parse_expression_markup(target) {
                            match seg.kind {
                                MarkupKind::Plain => {
                                    ui.label(RichText::new(seg.text).size(12.0).color(pal.ink));
                                }
                                MarkupKind::Emotion
                                | MarkupKind::VocalEvent
                                | MarkupKind::Emphasis => {
                                    let chip_color = match seg.kind {
                                        MarkupKind::Emotion => pal.emotion_chip,
                                        MarkupKind::VocalEvent => pal.event_chip,
                                        MarkupKind::Emphasis => pal.emphasis_chip,
                                        MarkupKind::Plain => pal.ink,
                                    };
                                    let resp = ui
                                        .add(
                                            egui::Label::new(
                                                RichText::new(format!(" {} ", seg.text))
                                                    .size(11.5)
                                                    .strong()
                                                    .color(chip_color)
                                                    .background_color(
                                                        chip_color.gamma_multiply(0.20),
                                                    ),
                                            )
                                            .sense(egui::Sense::hover()),
                                        );
                                    if let Some(tip) = describe_markup_segment(&seg) {
                                        let kind = seg.kind;
                                        resp.on_hover_ui(|ui| {
                                            draw_tag_tooltip_card(ui, pal, &tip, kind);
                                        });
                                    }
                                }
                            }
                        }
                    });
                }
            });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);

            // Bottom Action Row: Stats (left) + Generate CTA & Cancel (right)
            let chars = target.chars().count();
            let words = target.split_whitespace().count();
            let est_secs = ((words as f32) / 2.6).max(1.0).round() as u32;

            let supports_cloning = self
                .model_info
                .as_ref()
                .map(|m| m.supports_cloning)
                .unwrap_or(false);
            let can_generate = self.busy.is_none()
                && self.model_info.is_some()
                && (!use_clone || supports_cloning);

            card_header_row(
                ui,
                |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{words} words · {chars} chars · ~{est_secs}s · Ctrl+Enter"
                        ))
                        .size(12.0)
                        .color(pal.ink_muted),
                    );
                },
                |ui| {
                    if let Some(busy) = &self.busy {
                        ui.spinner();
                        let status_text = match busy {
                            BusyTask::LoadingModel(m) => {
                                format!("Loading {}...", short_model_name(m))
                            }
                            BusyTask::Generating {
                                started,
                                chunks_done,
                            } => {
                                let secs = started.elapsed().as_secs_f32();
                                if *chunks_done > 0 {
                                    format!("Generating {secs:.1}s ({chunks_done} chunk)")
                                } else {
                                    format!("Generating {secs:.1}s...")
                                }
                            }
                            BusyTask::Transcribing => "Transcribing...".to_string(),
                            BusyTask::SamplingVoice(v) => format!("Previewing {v}..."),
                            BusyTask::Exporting => "Exporting...".to_string(),
                        };
                        ui.label(RichText::new(status_text).strong().color(pal.primary));

                        if matches!(
                            busy,
                            BusyTask::LoadingModel(_)
                                | BusyTask::Generating { .. }
                                | BusyTask::Transcribing
                        ) {
                            let cancel_btn = egui::Button::new(
                                RichText::new("Cancel").color(pal.danger).strong(),
                            )
                            .fill(pal.danger_soft)
                            .corner_radius(CornerRadius::same(8));
                            if ui.add(cancel_btn).clicked() {
                                let ctx = ui.ctx().clone();
                                self.cancel_current_task(&ctx);
                            }
                        }
                    }

                    let btn_w = 198.0;
                    let btn_h = 42.0;
                    let (btn_rect, resp) = ui.allocate_exact_size(
                        Vec2::new(btn_w, btn_h),
                        if can_generate {
                            egui::Sense::click()
                        } else {
                            egui::Sense::hover()
                        },
                    );
                    if ui.is_rect_visible(btn_rect) {
                        let p = ui.painter_at(btn_rect);
                        let fill = if !can_generate {
                            pal.ink_muted.gamma_multiply(0.35)
                        } else if resp.is_pointer_button_down_on() || resp.hovered() {
                            pal.primary_hover
                        } else {
                            pal.primary
                        };
                        p.rect_filled(btn_rect, CornerRadius::same(10), fill);
                        let icon_rect = egui::Rect::from_min_size(
                            egui::Pos2::new(btn_rect.left() + 14.0, btn_rect.center().y - 9.0),
                            Vec2::splat(18.0),
                        );
                        draw_sparkle_icon(&p, icon_rect, pal.on_primary);
                        p.text(
                            egui::Pos2::new(icon_rect.right() + 8.0, btn_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            "Generate Speech",
                            egui::FontId::proportional(15.0),
                            pal.on_primary,
                        );
                    }
                    if resp.clicked() && can_generate {
                        self.send_generate(use_clone);
                    }
                },
            );
        });
    }

    fn draw_clone_canvas(&mut self, ui: &mut egui::Ui, pal: &StudioPalette) {
        egui::ScrollArea::vertical()
            .id_salt("clone_center_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let canvas_w = ui.available_width();
                ui.set_max_width(canvas_w);

                let supports_cloning = self
                    .model_info
                    .as_ref()
                    .map(|m| m.supports_cloning)
                    .unwrap_or(false);

                if !supports_cloning {
                    egui::Frame::new()
                        .fill(pal.warning_soft)
                        .stroke(Stroke::new(1.0_f32, pal.warning.gamma_multiply(0.5)))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(egui::Margin::same(14))
                        .show(ui, |ui| {
                            let inner_w = (canvas_w - 30.0).max(160.0);
                            ui.set_min_width(inner_w);
                            ui.set_max_width(inner_w);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("Voice Cloning uses Kitten-TTS-2 (1.7B)")
                                        .size(15.0)
                                        .strong()
                                        .color(pal.warning),
                                );
                                ui.label(
                                    RichText::new("Prepare your 5-30s clip below, then load Kitten-TTS-2 (492 MB emb4 + 39 MB decoder).")
                                        .size(12.5)
                                        .color(pal.ink),
                                );
                                ui.add_space(6.0);
                                let btn = egui::Button::new(
                                    RichText::new("Load Kitten-TTS-2 Now")
                                        .strong()
                                        .color(pal.on_primary),
                                )
                                .fill(pal.primary)
                                .min_size(Vec2::new(0.0, 32.0))
                                .corner_radius(CornerRadius::same(8));
                                if ui.add_enabled(self.busy.is_none(), btn).clicked() {
                                    self.load_kitten2_for_cloning();
                                }
                            });
                        });
                    ui.add_space(10.0);
                }

                // Step 1: Reference Clip & Mic Recorder Card
                styled_card(ui, pal, |ui| {
                    card_header_row(
                        ui,
                        |ui| {
                            ui.label(
                                RichText::new("1. Reference Clip (5-30s)")
                                    .size(15.0)
                                    .strong()
                                    .color(pal.ink),
                            );
                        },
                        |ui| {
                            ui.radio_value(
                                &mut self.clone_source,
                                CloneSourceMode::UploadFile,
                                "Upload File",
                            );
                            ui.radio_value(
                                &mut self.clone_source,
                                CloneSourceMode::RecordMic,
                                "Record Mic",
                            );
                        },
                    );

                    ui.add_space(6.0);

                    match self.clone_source {
                        CloneSourceMode::UploadFile => {
                            ui.horizontal_wrapped(|ui| {
                                if ui.button("Choose Audio File...").clicked() {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter(
                                            "Audio files",
                                            &["wav", "mp3", "flac", "ogg", "m4a"],
                                        )
                                        .pick_file()
                                    {
                                        self.set_clone_reference_file(path);
                                    }
                                }
                                if let Some(path) = self.clone_ref_path.clone() {
                                    let name = path
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string();
                                    let dur_txt = self
                                        .clone_ref_duration
                                        .map(|d| format!(" ({d:.1}s)"))
                                        .unwrap_or_default();
                                    pill_badge(
                                        ui,
                                        &format!("{name}{dur_txt}"),
                                        pal.accent_blue_soft,
                                        pal.accent_blue,
                                    );
                                    if ui.button("Listen to Clip").clicked() {
                                        self.audio.load_and_play(path, true);
                                    }
                                } else {
                                    ui.label(
                                        RichText::new("Or drag & drop a .wav, .mp3, .flac, or .ogg file anywhere.")
                                            .size(12.5)
                                            .color(pal.ink_muted),
                                    );
                                }
                            });
                        }
                        CloneSourceMode::RecordMic => {
                            ui.horizontal_wrapped(|ui| {
                                if self.recorder.is_recording() {
                                    let stop_btn = egui::Button::new(
                                        RichText::new("Stop Recording")
                                            .strong()
                                            .color(Color32::WHITE),
                                    )
                                    .fill(pal.danger)
                                    .corner_radius(CornerRadius::same(8));
                                    if ui.add(stop_btn).clicked() {
                                        if let Some(path) = self.recorder.stop() {
                                            self.set_clone_reference_file(path);
                                        }
                                    }

                                    let elapsed = self.recorder.elapsed_secs();
                                    ui.label(
                                        RichText::new(format!(
                                            "Recording {} / 00:30",
                                            format_mmss(elapsed)
                                        ))
                                        .strong()
                                        .color(pal.danger),
                                    );

                                    let level = self.recorder.level();
                                    let (bar_rect, _) = ui.allocate_exact_size(
                                        Vec2::new(150.0, 12.0),
                                        egui::Sense::hover(),
                                    );
                                    let p = ui.painter_at(bar_rect);
                                    p.rect_filled(bar_rect, CornerRadius::same(6), pal.input_bg);
                                    let fill_w = (bar_rect.width() * (level * 1.8).clamp(0.04, 1.0))
                                        .min(bar_rect.width());
                                    let fill_rect = egui::Rect::from_min_size(
                                        bar_rect.min,
                                        Vec2::new(fill_w, bar_rect.height()),
                                    );
                                    p.rect_filled(fill_rect, CornerRadius::same(6), pal.success);
                                } else {
                                    let (rec_rect, rec_resp) = ui.allocate_exact_size(
                                        Vec2::new(152.0, 32.0),
                                        egui::Sense::click(),
                                    );
                                    if ui.is_rect_visible(rec_rect) {
                                        let p = ui.painter_at(rec_rect);
                                        p.rect_filled(
                                            rec_rect,
                                            CornerRadius::same(8),
                                            if rec_resp.hovered() {
                                                pal.primary_hover
                                            } else {
                                                pal.primary
                                            },
                                        );
                                        let mic_r = egui::Rect::from_min_size(
                                            egui::Pos2::new(
                                                rec_rect.left() + 10.0,
                                                rec_rect.center().y - 7.5,
                                            ),
                                            Vec2::splat(15.0),
                                        );
                                        draw_mic_icon(&p, mic_r, pal.on_primary);
                                        p.text(
                                            egui::Pos2::new(
                                                mic_r.right() + 6.0,
                                                rec_rect.center().y,
                                            ),
                                            egui::Align2::LEFT_CENTER,
                                            "Start Recording",
                                            egui::FontId::proportional(13.5),
                                            pal.on_primary,
                                        );
                                    }
                                    if rec_resp.clicked() {
                                        let rec_path = self.storage.recordings_dir.join(format!(
                                            "mic_{}.wav",
                                            Local::now().format("%Y%m%d_%H%M%S")
                                        ));
                                        if let Err(err) = self.recorder.start(rec_path) {
                                            self.banner = Some(BannerMessage {
                                                kind: BannerKind::Error,
                                                text: err,
                                                detail: None,
                                            });
                                        }
                                    }
                                    if let Some(path) = self.clone_ref_path.clone() {
                                        if ui.button("Listen to Recording").clicked() {
                                            self.audio.load_and_play(path, true);
                                        }
                                    }
                                }
                            });
                        }
                    }

                    ui.add_space(8.0);

                    card_header_row(
                        ui,
                        |ui| {
                            ui.label(
                                RichText::new("Reference Transcript")
                                    .strong()
                                    .size(13.0)
                                    .color(pal.ink),
                            );
                        },
                        |ui| {
                            let can_transcribe = supports_cloning
                                && self.clone_ref_path.is_some()
                                && self.busy.is_none();
                            if ui
                                .add_enabled(
                                    can_transcribe,
                                    egui::Button::new("Auto-Transcribe (Whisper)"),
                                )
                                .on_hover_text("Optional: typing the 1-2 sentences yourself is much faster on CPU.")
                                .clicked()
                            {
                                if let Some(path) = &self.clone_ref_path {
                                    if let Some(worker) = self.worker.as_mut() {
                                        let _ = worker.send(&WorkerRequest::Transcribe {
                                            reference: path.to_string_lossy().to_string(),
                                        });
                                        self.busy = Some(BusyTask::Transcribing);
                                    }
                                }
                            }
                        },
                    );

                    let tr_w = ui.available_width();
                    egui::Frame::new()
                        .fill(pal.input_bg)
                        .stroke(Stroke::new(1.2_f32, pal.border_strong))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            let inner_w = (tr_w - 18.0).max(100.0);
                            ui.set_min_width(inner_w);
                            ui.set_max_width(inner_w);
                            ui.add(
                                egui::TextEdit::multiline(&mut self.clone_transcript)
                                    .frame(false)
                                    .desired_rows(2)
                                    .desired_width(inner_w)
                                    .hint_text("Type what is said in the reference clip (leave empty to auto-transcribe with Whisper)."),
                            );
                        });

                    ui.add_space(4.0);
                    ui.checkbox(
                        &mut self.clone_use_ref_prompt,
                        "Include clip audio tokens in prompt (improves cloning fidelity)",
                    );
                });

                ui.add_space(10.0);
                self.draw_script_and_generate_card(ui, pal, true);
            });
    }

    fn draw_models_tab(&mut self, ui: &mut egui::Ui, pal: &StudioPalette) {
        let ctx = ui.ctx().clone();
        let mut switch_requested = false;
        let loaded_model_name = self.model_info.as_ref().map(|i| i.model_name.clone());
        let loading_model_name = match &self.busy {
            Some(BusyTask::LoadingModel(m)) => Some(m.clone()),
            _ => None,
        };

        egui::ScrollArea::vertical()
            .id_salt("models_tab_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let canvas_w = ui.available_width();
                ui.set_max_width(canvas_w);

                if let Some(ref loading_name) = loading_model_name {
                    self.draw_loading_progress_ui(ui, pal, loading_name);
                    ui.add_space(8.0);
                }

                if let Some(info) = &self.model_info {
                    let friendly_name = MODEL_CATALOG
                        .iter()
                        .find(|m| m.repo_id == info.model_name)
                        .map(|m| m.display_name)
                        .unwrap_or_else(|| short_model_name(&info.model_name));
                    styled_card(ui, pal, |ui| {
                        card_header_row(
                            ui,
                            |ui| {
                                ui.label(
                                    RichText::new("Active Engine:")
                                        .size(14.0)
                                        .strong()
                                        .color(pal.ink),
                                );
                                pill_badge(ui, friendly_name, pal.success_soft, pal.success);
                                pill_badge(
                                    ui,
                                    &format!("{} voices · {} Hz", info.voices.len(), info.sample_rate),
                                    pal.accent_blue_soft,
                                    pal.accent_blue,
                                );
                                if info.supports_voice_blending {
                                    pill_badge(
                                        ui,
                                        "Voice Blending",
                                        pal.primary.gamma_multiply(0.16),
                                        pal.primary,
                                    );
                                }
                                if info.supports_cloning {
                                    pill_badge(
                                        ui,
                                        "Voice Cloning",
                                        pal.primary.gamma_multiply(0.16),
                                        pal.primary,
                                    );
                                }
                            },
                            |ui| {
                                if ui
                                    .add_enabled(
                                        self.busy.is_none(),
                                        egui::Button::new("Free RAM").small(),
                                    )
                                    .on_hover_text("Release transient caches and trim unused RAM back to Windows")
                                    .clicked()
                                {
                                    if let Some(worker) = self.worker.as_mut() {
                                        let _ = worker.send(&WorkerRequest::FreeMemory {
                                            unload_model: false,
                                        });
                                    }
                                }
                                if ui
                                    .add_enabled(
                                        self.busy.is_none(),
                                        egui::Button::new("Unload").small(),
                                    )
                                    .on_hover_text("Unload the active engine to free maximum host memory")
                                    .clicked()
                                {
                                    if let Some(worker) = self.worker.as_mut() {
                                        let _ = worker.send(&WorkerRequest::FreeMemory {
                                            unload_model: true,
                                        });
                                    }
                                }
                            },
                        );
                    });
                    ui.add_space(10.0);
                }

                // Quick Pick Goal Cards
                styled_card(ui, pal, |ui| {
                    ui.label(
                        RichText::new("Quick Pick — Choose by What You Need")
                            .size(15.5)
                            .strong()
                            .color(pal.ink),
                    );
                    ui.label(
                        RichText::new("One click switches and downloads (on first use) the best engine for your task.")
                            .size(12.0)
                            .color(pal.ink_muted),
                    );
                    ui.add_space(8.0);

                    let mut picked_repo: Option<&'static str> = None;
                    let presets = [
                        (
                            "Fast Everyday CPU",
                            "KittenML/kitten-tts-2-nano",
                            "~200 MB RAM · Instant",
                            "Lightweight 8-language speech that runs faster than real time on any CPU.",
                        ),
                        (
                            "Books & Voice Blend",
                            "kokoro-tts/kokoro-v1.0",
                            "~300 MB RAM · 54 Voices",
                            "Kokoro-82M crisp voices with dual-voice blending & EPUB/PDF chapter reader.",
                        ),
                        (
                            "Cloning & Emotions",
                            "KittenML/kitten-tts-2",
                            "1.7B · 115 Voices",
                            "Zero-shot voice cloning from 5-30s audio plus native [emotion] & <event> tags.",
                        ),
                    ];
                    ui.columns(3, |cols| {
                        for (idx, (title, repo_id, tag, summary)) in presets.into_iter().enumerate() {
                            let is_active = loaded_model_name.as_deref() == Some(repo_id);
                            egui::Frame::new()
                                .fill(if is_active { pal.card_bg } else { pal.card_alt })
                                .stroke(Stroke::new(
                                    if is_active { 1.6_f32 } else { 1.0_f32 },
                                    if is_active { pal.success } else { pal.border_strong },
                                ))
                                .corner_radius(CornerRadius::same(10))
                                .inner_margin(egui::Margin::same(11))
                                .show(&mut cols[idx], |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.vertical(|ui| {
                                        ui.label(
                                            RichText::new(title)
                                                .strong()
                                                .size(13.5)
                                                .color(if is_active { pal.success } else { pal.ink }),
                                        );
                                        ui.add_space(4.0);
                                        ui.horizontal(|ui| {
                                            pill_badge(
                                                ui,
                                                tag,
                                                pal.accent_blue_soft,
                                                pal.accent_blue,
                                            );
                                        });
                                        ui.add_space(5.0);
                                        ui.allocate_ui_with_layout(
                                            Vec2::new(ui.available_width(), 38.0),
                                            egui::Layout::top_down(egui::Align::LEFT),
                                            |ui| {
                                                ui.label(
                                                    RichText::new(summary)
                                                        .size(11.5)
                                                        .color(pal.ink_muted),
                                                );
                                            },
                                        );
                                        ui.add_space(6.0);
                                        let footer_w = ui.available_width().max(100.0);
                                        if is_active {
                                            egui::Frame::new()
                                                .fill(pal.success_soft)
                                                .stroke(Stroke::new(
                                                    1.0_f32,
                                                    pal.success.gamma_multiply(0.5),
                                                ))
                                                .corner_radius(CornerRadius::same(7))
                                                .show(ui, |ui| {
                                                    ui.allocate_ui_with_layout(
                                                        Vec2::new(footer_w - 2.0, 26.0),
                                                        egui::Layout::centered_and_justified(
                                                            egui::Direction::LeftToRight,
                                                        ),
                                                        |ui| {
                                                            ui.label(
                                                                RichText::new("Active Engine")
                                                                    .size(12.0)
                                                                    .strong()
                                                                    .color(pal.success),
                                                            );
                                                        },
                                                    );
                                                });
                                        } else {
                                            let btn = egui::Button::new(
                                                RichText::new("Use This Engine")
                                                    .size(12.0)
                                                    .strong()
                                                    .color(pal.on_primary),
                                            )
                                            .fill(pal.primary)
                                            .min_size(Vec2::new(footer_w, 28.0))
                                            .corner_radius(CornerRadius::same(7));
                                            if ui.add(btn).clicked() {
                                                picked_repo = Some(repo_id);
                                            }
                                        }
                                    });
                                });
                        }
                    });
                    if let Some(repo_id) = picked_repo {
                        self.draft_model_name = repo_id.to_string();
                        self.draft_custom_repo.clear();
                        if let Some(entry) = MODEL_CATALOG.iter().find(|e| e.repo_id == repo_id) {
                            if let Some(dw) = entry.default_weights {
                                match entry.category {
                                    ModelCategory::TinyCpu => self.draft_tiny_weights = dw.to_string(),
                                    ModelCategory::KokoroOnnx => self.draft_kokoro_weights = dw.to_string(),
                                    ModelCategory::StudioLm => self.draft_k2_weights = dw.to_string(),
                                    ModelCategory::LegacyOnnx => {}
                                }
                            }
                        }
                        switch_requested = true;
                    }
                });

                ui.add_space(10.0);

                // Filterable All-Models Browser + Simple Quality Selector
                styled_card(ui, pal, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new("Model Catalog")
                                .size(15.5)
                                .strong()
                                .color(pal.ink),
                        );
                        ui.add_space(8.0);
                        for (tab, lbl) in [
                            (ModelFilterTab::All, "All Models"),
                            (ModelFilterTab::FastCpu, "Fast CPU"),
                            (ModelFilterTab::Kokoro, "Kokoro (Blend)"),
                            (ModelFilterTab::StudioLm, "1.7B Studio"),
                            (ModelFilterTab::Classic, "Classic 0.8"),
                        ] {
                            let selected = self.model_filter == tab;
                            let chip = egui::Button::new(
                                RichText::new(lbl)
                                    .size(12.0)
                                    .strong()
                                    .color(if selected { pal.on_primary } else { pal.ink }),
                            )
                            .fill(if selected { pal.primary } else { pal.card_alt })
                            .corner_radius(CornerRadius::same(14));
                            if ui.add(chip).clicked() {
                                self.model_filter = tab;
                            }
                        }
                    });
                    ui.add_space(8.0);

                    let row_w = (ui.available_width() - 26.0).max(200.0);
                    for entry in MODEL_CATALOG.iter().filter(|e| match self.model_filter {
                        ModelFilterTab::All => true,
                        ModelFilterTab::FastCpu => e.category == ModelCategory::TinyCpu,
                        ModelFilterTab::Kokoro => e.category == ModelCategory::KokoroOnnx,
                        ModelFilterTab::StudioLm => e.category == ModelCategory::StudioLm,
                        ModelFilterTab::Classic => e.category == ModelCategory::LegacyOnnx,
                    }) {
                        let selected = self.draft_model_name == entry.repo_id
                            && self.draft_custom_repo.trim().is_empty();
                        let is_active_engine = loaded_model_name.as_deref() == Some(entry.repo_id)
                            && loading_model_name.is_none();
                        let is_loading_this = loading_model_name.as_deref() == Some(entry.repo_id);
                        let mut activate_this = false;

                        egui::Frame::new()
                            .fill(if selected || is_active_engine {
                                pal.card_bg
                            } else {
                                pal.card_alt
                            })
                            .stroke(Stroke::new(
                                if is_active_engine || selected { 1.6_f32 } else { 1.0_f32 },
                                if is_active_engine {
                                    pal.success
                                } else if selected {
                                    pal.primary
                                } else {
                                    pal.border
                                },
                            ))
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(egui::Margin::symmetric(12, 8))
                            .show(ui, |ui| {
                                ui.set_min_width(row_w);
                                ui.set_max_width(row_w);
                                ui.horizontal(|ui| {
                                    if ui
                                        .radio(
                                            selected || is_active_engine,
                                            RichText::new(entry.display_name)
                                                .strong()
                                                .size(14.0)
                                                .color(if is_active_engine {
                                                    pal.success
                                                } else if selected {
                                                    pal.primary
                                                } else {
                                                    pal.ink
                                                }),
                                        )
                                        .clicked()
                                    {
                                        activate_this = true;
                                    }
                                    pill_badge(
                                        ui,
                                        entry.badge,
                                        pal.primary.gamma_multiply(0.16),
                                        pal.primary,
                                    );
                                    pill_badge(
                                        ui,
                                        entry.params,
                                        pal.accent_blue_soft,
                                        pal.accent_blue,
                                    );
                                    ui.label(
                                        RichText::new(entry.size_summary)
                                            .size(12.0)
                                            .color(pal.ink_muted),
                                    );

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if is_active_engine {
                                                pill_badge(
                                                    ui,
                                                    "Active",
                                                    pal.success_soft,
                                                    pal.success,
                                                );
                                            } else if is_loading_this {
                                                pill_badge(
                                                    ui,
                                                    "Loading...",
                                                    pal.warning_soft,
                                                    pal.warning,
                                                );
                                                ui.spinner();
                                            } else {
                                                let act_btn = egui::Button::new(
                                                    RichText::new("Activate")
                                                        .strong()
                                                        .size(12.0)
                                                        .color(pal.on_primary),
                                                )
                                                .fill(pal.primary)
                                                .corner_radius(CornerRadius::same(7))
                                                .min_size(Vec2::new(78.0, 26.0));
                                                if ui.add(act_btn).clicked() {
                                                    activate_this = true;
                                                }
                                            }
                                        },
                                    );
                                });
                                ui.label(
                                    RichText::new(format!(
                                        "{} — {}",
                                        entry.languages_summary, entry.description
                                    ))
                                    .size(12.0)
                                    .color(pal.ink_muted),
                                );

                                // Inline plain-English quality selector for the active/selected card
                                if (is_active_engine || selected) && entry.category != ModelCategory::LegacyOnnx {
                                    ui.add_space(4.0);
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            RichText::new("Quality / Size:")
                                                .size(11.5)
                                                .strong()
                                                .color(pal.ink_muted),
                                        );
                                        match entry.category {
                                            ModelCategory::TinyCpu => {
                                                for (val, lbl) in [("int8", "Balanced (int8)"), ("int4", "Ultra-Compact (int4)")] {
                                                    if ui
                                                        .radio_value(
                                                            &mut self.draft_tiny_weights,
                                                            val.into(),
                                                            lbl,
                                                        )
                                                        .changed()
                                                    {
                                                        self.draft_tiny_backend = "onnx".into();
                                                        self.draft_model_name = entry.repo_id.into();
                                                        switch_requested = true;
                                                    }
                                                }
                                            }
                                            ModelCategory::KokoroOnnx => {
                                                for (val, lbl) in [
                                                    ("int8", "Compact (88 MB · int8)"),
                                                    ("fp16", "Balanced (169 MB · fp16)"),
                                                    ("fp32", "Full Studio (310 MB · fp32)"),
                                                ] {
                                                    if ui
                                                        .radio_value(
                                                            &mut self.draft_kokoro_weights,
                                                            val.into(),
                                                            lbl,
                                                        )
                                                        .changed()
                                                    {
                                                        self.draft_model_name = entry.repo_id.into();
                                                        switch_requested = true;
                                                    }
                                                }
                                            }
                                            ModelCategory::StudioLm => {
                                                for (val, lbl) in [
                                                    ("emb4", "Compact (492 MB · emb4)"),
                                                    ("packed", "Lossless (954 MB · packed)"),
                                                    ("full", "Full (3.47 GB · bf16)"),
                                                ] {
                                                    if ui
                                                        .radio_value(
                                                            &mut self.draft_k2_weights,
                                                            val.into(),
                                                            lbl,
                                                        )
                                                        .changed()
                                                    {
                                                        self.draft_model_name = entry.repo_id.into();
                                                        switch_requested = true;
                                                    }
                                                }
                                            }
                                            ModelCategory::LegacyOnnx => {}
                                        }
                                    });
                                }
                            });

                        if activate_this {
                            self.draft_model_name = entry.repo_id.to_string();
                            self.draft_custom_repo.clear();
                            if let Some(dw) = entry.default_weights {
                                match entry.category {
                                    ModelCategory::TinyCpu => {
                                        self.draft_tiny_weights = dw.to_string();
                                    }
                                    ModelCategory::KokoroOnnx => {
                                        self.draft_kokoro_weights = dw.to_string();
                                    }
                                    ModelCategory::StudioLm => {
                                        self.draft_k2_weights = dw.to_string();
                                    }
                                    ModelCategory::LegacyOnnx => {}
                                }
                            }
                            switch_requested = true;
                        }
                        ui.add_space(4.0);
                    }
                });

                ui.add_space(10.0);

                // Collapsible Advanced Hardware & Custom Model Settings
                styled_card(ui, pal, |ui| {
                    egui::CollapsingHeader::new(
                        RichText::new("Advanced Hardware, Custom Models & Cache")
                            .size(14.5)
                            .strong()
                            .color(pal.ink),
                    )
                    .id_salt("advanced_engine_settings")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        let selected_cat = MODEL_CATALOG
                            .iter()
                            .find(|m| m.repo_id == self.draft_model_name)
                            .map(|m| m.category);

                        match selected_cat {
                            Some(ModelCategory::TinyCpu) => {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Runtime:");
                                    if ui
                                        .radio_value(
                                            &mut self.draft_tiny_backend,
                                            "onnx".into(),
                                            "ONNX (Fastest CPU)",
                                        )
                                        .changed()
                                    {
                                        switch_requested = true;
                                    }
                                    if ui
                                        .radio_value(
                                            &mut self.draft_tiny_backend,
                                            "torch".into(),
                                            "PyTorch (FP32)",
                                        )
                                        .changed()
                                    {
                                        switch_requested = true;
                                    }
                                });
                            }
                            Some(ModelCategory::StudioLm) => {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Audio Decoder:");
                                    if ui
                                        .radio_value(
                                            &mut self.draft_k2_decoder,
                                            "student_w4".into(),
                                            "student_w4 (39 MB compact)",
                                        )
                                        .changed()
                                    {
                                        switch_requested = true;
                                    }
                                    if ui
                                        .radio_value(
                                            &mut self.draft_k2_decoder,
                                            "default".into(),
                                            "default (459 MB best quality)",
                                        )
                                        .changed()
                                    {
                                        switch_requested = true;
                                    }
                                });
                            }
                            _ => {}
                        }

                        ui.horizontal_wrapped(|ui| {
                            ui.add(
                                egui::Slider::new(&mut self.draft_threads, 1..=16)
                                    .text("CPU Threads"),
                            );
                            ui.label("Device:");
                            for (val, lbl) in [
                                ("auto", "Auto"),
                                ("cpu", "CPU"),
                                ("cuda", "CUDA GPU"),
                                ("amd_gpu", "DirectML GPU"),
                            ] {
                                if ui.radio_value(&mut self.draft_device, val.into(), lbl).changed() {
                                    switch_requested = true;
                                }
                            }
                        });

                        ui.separator();

                        ui.horizontal(|ui| {
                            ui.label("Custom Repo or Local Folder:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.draft_custom_repo)
                                    .hint_text("Optional: folder path (KittenTTS or Kokoro .onnx+.bin) or HF repo ID")
                                    .desired_width(300.0),
                            );
                            if ui.button("Browse Folder...").clicked() {
                                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                                    self.draft_custom_repo = dir.to_string_lossy().to_string();
                                }
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Custom Model Cache Dir:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.config.cache_dir)
                                    .hint_text("Default (~/.cache/huggingface & ~/.cache/voxa-studio)")
                                    .desired_width(280.0),
                            );
                            if ui.button("Browse...").clicked() {
                                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                                    self.config.cache_dir = dir.to_string_lossy().to_string();
                                    self.storage.save_config(&self.config);
                                }
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("HF Token (private repos only):");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.config.hf_token)
                                    .password(true)
                                    .hint_text("Optional")
                                    .desired_width(240.0),
                            );
                            if ui.button("Save Preferences").clicked() {
                                self.storage.save_config(&self.config);
                                self.banner = Some(BannerMessage {
                                    kind: BannerKind::Success,
                                    text: "Saved preferences.".to_string(),
                                    detail: None,
                                });
                            }
                        });

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let load_btn = egui::Button::new(
                                RichText::new("Reload / Load Custom Model")
                                    .size(13.5)
                                    .strong()
                                    .color(pal.on_primary),
                            )
                            .fill(pal.primary)
                            .min_size(Vec2::new(205.0, 34.0))
                            .corner_radius(CornerRadius::same(9));
                            if ui.add(load_btn).clicked() {
                                switch_requested = true;
                            }
                        });
                    });
                });

                ui.add_space(10.0);

                styled_card(ui, pal, |ui| {
                    egui::CollapsingHeader::new(
                        RichText::new(format!("Engine Activity Log ({} lines)", self.logs.len()))
                            .size(14.0)
                            .strong()
                            .color(pal.ink),
                    )
                    .id_salt("engine_log_collapse")
                    .open(if self.force_open_logs { Some(true) } else { None })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(!self.logs.is_empty(), egui::Button::new("Copy Log").small())
                                .on_hover_text("Copy all engine log lines to clipboard")
                                .clicked()
                            {
                                let full_log =
                                    self.logs.iter().cloned().collect::<Vec<_>>().join("\n");
                                ui.ctx().copy_text(full_log);
                                self.banner = Some(BannerMessage {
                                    kind: BannerKind::Success,
                                    text: format!(
                                        "Copied {} log lines to clipboard.",
                                        self.logs.len()
                                    ),
                                    detail: None,
                                });
                            }
                            if ui.small_button("Clear Log").clicked() {
                                self.logs.clear();
                            }
                            if let Some(w) = &self.worker {
                                ui.label(
                                    RichText::new(format!("Python: {}", w.python_path().display()))
                                        .size(11.5)
                                        .color(pal.ink_muted),
                                );
                            }
                        });
                        egui::ScrollArea::vertical()
                            .id_salt("log_scroll")
                            .max_height(180.0)
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                for line in &self.logs {
                                    ui.label(RichText::new(line).monospace().size(11.5));
                                }
                            });
                    });
                    self.force_open_logs = false;
                });
            });

        if switch_requested {
            self.switch_or_reload_from_draft(&ctx);
        }
    }

    fn draw_loading_progress_ui(
        &mut self,
        ui: &mut egui::Ui,
        pal: &StudioPalette,
        loading_name: &str,
    ) {
        let avail_w = ui.available_width();
        let mut cancel_clicked = false;
        let dl_opt = self.download_progress.clone();

        egui::Frame::new()
            .fill(pal.warning_soft)
            .stroke(Stroke::new(1.2_f32, pal.warning))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                let inner_w = (avail_w - 28.0).max(200.0);
                ui.set_min_width(inner_w);
                ui.set_max_width(inner_w);

                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        RichText::new(format!(
                            "Loading engine: {}...",
                            short_model_name(loading_name)
                        ))
                        .strong()
                        .size(13.5)
                        .color(pal.warning),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let cancel_btn = egui::Button::new(
                            RichText::new("Cancel Download")
                                .strong()
                                .size(12.0)
                                .color(pal.danger),
                        )
                        .fill(pal.danger_soft)
                        .stroke(Stroke::new(1.0_f32, pal.danger.gamma_multiply(0.5)))
                        .corner_radius(CornerRadius::same(8));
                        if ui.add(cancel_btn).clicked() {
                            cancel_clicked = true;
                        }
                    });
                });

                if let Some(dl) = dl_opt {
                    ui.add_space(6.0);
                    let frac = if dl.total_bytes > 0 {
                        (dl.downloaded_bytes as f32 / dl.total_bytes as f32).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let speed_str = format_transfer_speed(dl.speed_bps);
                    let eta_str = dl
                        .eta_s
                        .filter(|s| *s >= 1.0 && s.is_finite())
                        .map(|s| format!(" · ETA {:.0}s", s))
                        .unwrap_or_default();
                    let label = if dl.total_bytes > 0 {
                        let pct = (frac * 100.0).round() as u32;
                        format!(
                            "{} — {} / {} ({pct}%) · {speed_str}{eta_str}",
                            dl.file_name,
                            format_bytes(dl.downloaded_bytes),
                            format_bytes(dl.total_bytes),
                        )
                    } else {
                        format!(
                            "{} — {} · {speed_str}",
                            dl.file_name,
                            format_bytes(dl.downloaded_bytes),
                        )
                    };
                    ui.add(
                        egui::ProgressBar::new(frac)
                            .desired_width(inner_w)
                            .text(label)
                            .animate(true),
                    );
                }
            });

        if cancel_clicked {
            let ctx = ui.ctx().clone();
            self.cancel_current_task(&ctx);
        }
    }

    fn draw_quick_guide_window(&mut self, ctx: &egui::Context, pal: &StudioPalette) {
        if !self.show_quick_guide {
            return;
        }
        let mut open = self.show_quick_guide;
        egui::Window::new("Quick Guide — Voxa Studio")
            .open(&mut open)
            .resizable(true)
            .default_width(500.0)
            .show(ctx, |ui| {
                ui.label(RichText::new("1. Speak Studio").strong().color(pal.primary));
                ui.label("Pick any voice on the left, click Sample to preview, type or drop a .txt file in the center script editor, tweak Speed/Delivery on the right, and click Generate Speech (Ctrl+Enter).");
                ui.add_space(6.0);

                ui.label(RichText::new("2. Bottom Audio Timeline").strong().color(pal.primary));
                ui.label("Press Space to Play/Pause, click or drag anywhere on the waveform ruler to scrub, and export as WAV, MP3, FLAC, or OGG.");
                ui.add_space(6.0);

                ui.label(RichText::new("3. Voice Cloning & Expressions").strong().color(pal.primary));
                ui.label("Switch to Voice Clone to upload or record a 5-30s clip (typing the transcript yourself is much faster on CPU than Whisper), and use Kitten-TTS-2 for [joyful] emotions, <laugh> events, and (((emphasis))).");
            });
        self.show_quick_guide = open;
    }

    fn draw_tags_guide_window(&mut self, ctx: &egui::Context, pal: &StudioPalette) {
        if !self.show_tags_guide {
            return;
        }
        let mut open = self.show_tags_guide;
        let use_clone = self.active_tab == ActiveTab::CloneVoice;

        egui::Window::new("Expression & Voice Tags Guide")
            .open(&mut open)
            .resizable(true)
            .default_width(600.0)
            .default_height(520.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("tags_guide_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(
                                "Tags steer tone, emotion, pauses, and vocal sounds — they are never read aloud as words. Hover over any colored tag right inside the script editor to inspect what it does.",
                            )
                            .size(12.5)
                            .color(pal.ink_muted),
                        );
                        ui.add_space(8.0);

                        // 1. Emotion Tags
                        styled_card(ui, pal, |ui| {
                            ui.label(
                                RichText::new("1. Emotion Tags [tag] — Changes Speaker Tone & Mood")
                                    .size(14.5)
                                    .strong()
                                    .color(pal.emotion_chip),
                            );
                            ui.label(
                                RichText::new("Place at the start of your script or before a sentence. Common aliases also work: [happy], [cheerful], [upbeat], [calm], [whisper], [serious], [mad], [scared], [shocked], [neutral].")
                                    .size(12.0)
                                    .color(pal.ink_muted),
                            );
                            ui.add_space(6.0);
                            for &(tag, tip) in EXPRESSION_TAGS {
                                ui.horizontal(|ui| {
                                    pill_badge(
                                        ui,
                                        &format!("[{tag}]"),
                                        pal.emotion_chip.gamma_multiply(0.18),
                                        pal.emotion_chip,
                                    );
                                    ui.label(RichText::new(tip).size(12.5).color(pal.ink));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.small_button("Use in Script").clicked() {
                                                let target = if use_clone {
                                                    &mut self.clone_text
                                                } else {
                                                    &mut self.speak_text
                                                };
                                                apply_leading_emotion(target, tag);
                                            }
                                        },
                                    );
                                });
                            }
                        });

                        ui.add_space(8.0);

                        // 2. Vocal Event Tags
                        styled_card(ui, pal, |ui| {
                            ui.label(
                                RichText::new("2. Vocal Event Tags <tag> — Inserts Laughs, Sighs, Gasps & Pauses")
                                    .size(14.5)
                                    .strong()
                                    .color(pal.event_chip),
                            );
                            ui.label(
                                RichText::new("Place inline anywhere between sentences or clauses. Common aliases also work: <chuckle>, <haha>, <hehe>, <breath>, <inhale>, <cry>, <hmm>, <cough>, <wait>, <beat>.")
                                    .size(12.0)
                                    .color(pal.ink_muted),
                            );
                            ui.add_space(6.0);
                            for &(ev, tip) in VOCAL_EVENT_TAGS {
                                ui.horizontal(|ui| {
                                    pill_badge(
                                        ui,
                                        &format!("<{ev}>"),
                                        pal.event_chip.gamma_multiply(0.18),
                                        pal.event_chip,
                                    );
                                    ui.label(RichText::new(tip).size(12.5).color(pal.ink));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.small_button("Insert").clicked() {
                                                let target = if use_clone {
                                                    &mut self.clone_text
                                                } else {
                                                    &mut self.speak_text
                                                };
                                                append_inline_token(target, &format!("<{ev}>"));
                                            }
                                        },
                                    );
                                });
                            }
                        });

                        ui.add_space(8.0);

                        // 3. Emphasis
                        styled_card(ui, pal, |ui| {
                            ui.label(
                                RichText::new("3. Word Emphasis (((word))) — Stresses Key Words")
                                    .size(14.5)
                                    .strong()
                                    .color(pal.emphasis_chip),
                            );
                            ui.label(
                                RichText::new("Wrap 1 to 4 important words in triple parentheses, like: We finished before dinner, and it looks (((amazing)))! Only the words inside are spoken, with extra vocal stress and pitch accent.")
                                    .size(12.5)
                                    .color(pal.ink),
                            );
                            ui.add_space(6.0);
                            if ui
                                .button("Load Expressive Demo Script")
                                .clicked()
                            {
                                let demo = STORY_STARTERS[1].text.to_string();
                                if use_clone {
                                    self.clone_text = demo;
                                } else {
                                    self.speak_text = demo.clone();
                                    self.last_auto_sample = demo;
                                }
                            }
                        });
                    });
            });
        self.show_tags_guide = open;
    }

    fn draw_inspector_window(&mut self, ctx: &egui::Context, pal: &StudioPalette) {
        let Some(content) = self.inspector.clone() else {
            return;
        };
        let mut open = true;
        let title = match &content {
            InspectorContent::Normalized { .. } => "Spoken Text Preview (Normalization)",
            InspectorContent::Phonemes { .. } => "Phoneme Preview",
        };
        egui::Window::new(title)
            .open(&mut open)
            .resizable(true)
            .default_width(540.0)
            .show(ctx, |ui| match &content {
                InspectorContent::Normalized {
                    original,
                    normalized,
                    spans,
                } => {
                    ui.label(RichText::new("Original text:").strong().color(pal.ink_muted));
                    ui.label(original);
                    ui.add_space(6.0);
                    ui.label(RichText::new("What the model hears:").strong().color(pal.primary));
                    egui::Frame::new()
                        .fill(pal.input_bg)
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(egui::Margin::same(10))
                        .show(ui, |ui| {
                            ui.label(RichText::new(normalized).size(14.5).color(pal.ink));
                        });
                    if !spans.is_empty() {
                        ui.add_space(8.0);
                        ui.label(RichText::new("Changed parts:").strong().color(pal.ink_muted));
                        for sp in spans {
                            ui.horizontal(|ui| {
                                pill_badge(ui, &sp.original, pal.warning_soft, pal.warning);
                                ui.label("->");
                                pill_badge(ui, &sp.replacement, pal.success_soft, pal.success);
                            });
                        }
                    }
                }
                InspectorContent::Phonemes { text, chunks } => {
                    ui.label(RichText::new("Input:").strong().color(pal.ink_muted));
                    ui.label(text);
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!("Phoneme chunks ({}):", chunks.len()))
                            .strong()
                            .color(pal.primary),
                    );
                    egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                        for (i, ch) in chunks.iter().enumerate() {
                            ui.label(RichText::new(format!("Chunk {}: {}", i + 1, ch)).monospace());
                        }
                    });
                }
            });
        if !open {
            self.inspector = None;
        }
    }
}

fn apply_leading_emotion(text: &mut String, tag: &str) {
    let trimmed = text.trim_start();
    let prefix = format!("[{tag}]");
    if trimmed.starts_with('[') {
        if let Some(end) = trimmed.find(']') {
            let inside = trimmed[1..end].to_ascii_lowercase();
            if EXPRESSION_TAGS.iter().any(|(k, _)| *k == inside) {
                let rest = trimmed[end + 1..].trim_start();
                *text = if rest.is_empty() {
                    format!("{prefix} ")
                } else {
                    format!("{prefix} {rest}")
                };
                return;
            }
        }
    }
    if trimmed.is_empty() {
        *text = format!("{prefix} ");
    } else {
        *text = format!("{prefix} {trimmed}");
    }
}

fn append_inline_token(text: &mut String, token: &str) {
    if text.is_empty() || text.ends_with(char::is_whitespace) {
        text.push_str(token);
    } else {
        text.push(' ');
        text.push_str(token);
    }
    text.push(' ');
}

fn emphasize_last_word(text: &mut String) {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        text.push_str("(((word))) ");
        return;
    }
    if let Some(pos) = trimmed.rfind(char::is_whitespace) {
        let head = &trimmed[..=pos];
        let last = &trimmed[pos + 1..];
        if last.starts_with("(((") && last.ends_with(")))") {
            return;
        }
        *text = format!("{head}((({last}))) ");
    } else if !(trimmed.starts_with("(((") && trimmed.ends_with(")))")) {
        *text = format!("((({trimmed}))) ");
    }
}

fn short_model_name(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let sub: String = s.chars().take(max_chars).collect();
        format!("{sub}...")
    }
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn format_transfer_speed(bps: f64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    if !bps.is_finite() || bps <= 0.0 {
        return "calculating...".to_string();
    }
    if bps >= MB {
        format!("{:.1} MB/s", bps / MB)
    } else if bps >= KB {
        format!("{:.0} KB/s", bps / KB)
    } else {
        format!("{bps:.0} B/s")
    }
}

fn draw_tag_tooltip_card(
    ui: &mut egui::Ui,
    pal: &StudioPalette,
    info: &TagTooltipInfo,
    kind: MarkupKind,
) {
    let accent = match kind {
        MarkupKind::Emotion => pal.emotion_chip,
        MarkupKind::VocalEvent => pal.event_chip,
        MarkupKind::Emphasis => pal.emphasis_chip,
        MarkupKind::Plain => pal.primary,
    };
    ui.set_max_width(320.0);
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            pill_badge(ui, &info.badge, accent.gamma_multiply(0.22), accent);
            ui.label(
                RichText::new(info.category)
                    .size(12.0)
                    .strong()
                    .color(pal.ink),
            );
        });
        ui.add_space(4.0);
        ui.label(RichText::new(&info.description).size(12.5).color(pal.ink));
        ui.add_space(3.0);
        ui.label(
            RichText::new(info.placement_tip)
                .size(11.5)
                .color(pal.ink_muted),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::compute_peaks;
    use crate::protocol::{parse_expression_markup, MarkupKind, WorkerEvent, WorkerRequest};

    #[test]
    fn test_protocol_roundtrip() {
        let req = WorkerRequest::Generate(GenerateRequest {
            text: "Hello world".into(),
            voice: Some("Eleanor".into()),
            voice_blend: Some(VoiceBlend {
                second_voice: "am_adam".into(),
                primary_weight: 0.6,
            }),
            language: Some("en".into()),
            reference: None,
            reference_text: None,
            speed: Some(1.1),
            preset: None,
            temperature: None,
            top_k: None,
            top_p: None,
            min_p: None,
            max_new_tokens: None,
            normalize: Some(true),
            use_reference_prompt: None,
            advanced: None,
            stream: true,
            output_wav: "out.wav".into(),
        });
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"cmd\":\"generate\""));
        assert!(json.contains("\"voice\":\"Eleanor\""));
        assert!(json.contains("\"second_voice\":\"am_adam\""));

        let ext_req = serde_json::to_string(&WorkerRequest::ExtractDocument {
            path: "book.epub".into(),
        })
        .unwrap();
        assert!(ext_req.contains("\"cmd\":\"extract_document\""));

        let doc_evt: WorkerEvent = serde_json::from_str(
            r#"{"event":"document_extracted","title":"Sample Book","path":"book.epub","chapters":[{"title":"Ch 1","text":"Once upon a time.","char_count":17}]}"#,
        )
        .unwrap();
        match doc_evt {
            WorkerEvent::DocumentExtracted {
                title, chapters, ..
            } => {
                assert_eq!(title, "Sample Book");
                assert_eq!(chapters.len(), 1);
            }
            _ => panic!("unexpected event variant"),
        }

        assert!(MODEL_CATALOG
            .iter()
            .any(|m| m.repo_id == "kokoro-tts/kokoro-v1.0" && m.category == ModelCategory::KokoroOnnx));
        assert_eq!(language_display_name("en-us"), "English (US)");

        let evt_json = r#"{"event":"generated","wav_path":"out.wav","sample_rate":24000,"duration_s":1.5,"elapsed_s":0.3,"chunks":1}"#;
        let evt: WorkerEvent = serde_json::from_str(evt_json).unwrap();
        match evt {
            WorkerEvent::Generated { sample_rate, chunks, .. } => {
                assert_eq!(sample_rate, 24000);
                assert_eq!(chunks, 1);
            }
            _ => panic!("unexpected event variant"),
        }
    }

    #[test]
    fn test_expression_markup_parsing() {
        let input = "[joyful] We won <laugh> I (((hardly))) believe [3]!";
        let segs = parse_expression_markup(input);
        assert!(segs.iter().any(|s| s.kind == MarkupKind::Emotion && s.text == "[joyful]"));
        assert!(segs.iter().any(|s| s.kind == MarkupKind::VocalEvent && s.text == "<laugh>"));
        assert!(segs.iter().any(|s| s.kind == MarkupKind::Emphasis && s.text == "(((hardly)))"));
        assert!(!segs.iter().any(|s| s.kind == MarkupKind::Emotion && s.text == "[3]"));

        let aliased = "[happy] Hi <chuckle>!";
        let a_segs = parse_expression_markup(aliased);
        assert!(a_segs.iter().any(|s| s.kind == MarkupKind::Emotion && s.text == "[happy]"));
        assert!(a_segs.iter().any(|s| s.kind == MarkupKind::VocalEvent && s.text == "<chuckle>"));
    }

    #[test]
    fn test_emotion_helper_replaces_existing_tag() {
        let mut s = "[sad] Hello there".to_string();
        apply_leading_emotion(&mut s, "joyful");
        assert_eq!(s, "[joyful] Hello there");

        let mut em = "Hello world".to_string();
        emphasize_last_word(&mut em);
        assert_eq!(em, "Hello (((world))) ");
    }

    #[test]
    fn test_waveform_peaks() {
        let samples: Vec<f32> = (0..1000).map(|i| (i as f32 / 1000.0).sin()).collect();
        let peaks = compute_peaks(&samples, 100);
        assert_eq!(peaks.len(), 100);
        assert!(peaks.iter().all(|&p| (0.03..=1.0).contains(&p)));
    }

    #[test]
    fn test_expression_spans_and_tooltips() {
        let input = "[joyful] Guess <chuckle> (((amazing)))!";
        let spans = parse_expression_spans(input);
        assert_eq!(spans[0].segment.text, "[joyful]");
        assert_eq!(spans[0].start_char, 0);
        assert_eq!(spans[0].end_char, 8);

        let emo_tip = describe_markup_segment(&spans[0].segment).unwrap();
        assert_eq!(emo_tip.category, "Emotion Tone Tag");
        assert!(emo_tip.description.contains("Happy"));

        let ev_span = spans
            .iter()
            .find(|s| s.segment.kind == MarkupKind::VocalEvent)
            .unwrap();
        let ev_tip = describe_markup_segment(&ev_span.segment).unwrap();
        assert!(ev_tip.description.contains("<laugh>"));

        let emph_span = spans
            .iter()
            .find(|s| s.segment.kind == MarkupKind::Emphasis)
            .unwrap();
        let emph_tip = describe_markup_segment(&emph_span.segment).unwrap();
        assert!(emph_tip.description.contains("amazing"));
    }

    #[test]
    fn test_download_progress_and_log_helpers() {
        let dl_json = r#"{"event":"download_progress","file_name":"model.safetensors","downloaded_bytes":10485760,"total_bytes":52428800,"speed_bps":5242880.0,"eta_s":8.0}"#;
        let evt: WorkerEvent = serde_json::from_str(dl_json).unwrap();
        match evt {
            WorkerEvent::DownloadProgress {
                file_name,
                downloaded_bytes,
                total_bytes,
                speed_bps,
                eta_s,
            } => {
                assert_eq!(file_name, "model.safetensors");
                assert_eq!(downloaded_bytes, 10_485_760);
                assert_eq!(total_bytes, 52_428_800);
                assert!((speed_bps - 5_242_880.0).abs() < 1.0);
                assert_eq!(eta_s, Some(8.0));
            }
            _ => panic!("unexpected event variant"),
        }

        assert_eq!(format_bytes(10_485_760), "10.0 MB");
        assert_eq!(format_transfer_speed(5_242_880.0), "5.0 MB/s");
        assert!(crate::worker::is_benign_hf_warning(
            "Warning: You are sending unauthenticated requests to the HF Hub. Please set a HF_TOKEN"
        ));
        assert!(crate::worker::is_benign_hf_warning(
            "HF_HUB_DISABLE_SYMLINKS_WARNING environment variable"
        ));
        assert!(!crate::worker::is_benign_hf_warning(
            "RuntimeError: Out of memory"
        ));

        let free_req = serde_json::to_string(&WorkerRequest::FreeMemory {
            unload_model: false,
        })
        .unwrap();
        assert!(free_req.contains("\"cmd\":\"free_memory\""));

        let free_evt: WorkerEvent =
            serde_json::from_str(r#"{"event":"memory_freed","unloaded":false,"ram_mb":312.5}"#)
                .unwrap();
        match free_evt {
            WorkerEvent::MemoryFreed { unloaded, ram_mb } => {
                assert!(!unloaded);
                assert!((ram_mb - 312.5).abs() < 0.1);
            }
            _ => panic!("unexpected event variant"),
        }
    }

    #[test]
    fn test_embedded_app_icon_decodes() {
        let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/voxa-icon-256.png"))
            .expect("embedded Voxa Studio icon PNG must decode cleanly");
        assert_eq!(icon.width, 256);
        assert_eq!(icon.height, 256);
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
    }
}
