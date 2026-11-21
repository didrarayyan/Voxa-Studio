use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Static catalog entry for an official KittenTTS model.
#[derive(Debug, Clone)]
pub struct ModelCatalogEntry {
    pub repo_id: &'static str,
    pub display_name: &'static str,
    pub category: ModelCategory,
    pub params: &'static str,
    pub size_summary: &'static str,
    pub languages_summary: &'static str,
    pub badge: &'static str,
    pub description: &'static str,
    pub default_weights: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCategory {
    TinyCpu,
    StudioLm,
    LegacyOnnx,
}

pub const MODEL_CATALOG: &[ModelCatalogEntry] = &[
    ModelCatalogEntry {
        repo_id: "KittenML/kitten-tts-2-nano",
        display_name: "Kitten-TTS-2 Nano",
        category: ModelCategory::TinyCpu,
        params: "17M / lang",
        size_summary: "8.8 MB (int4) - 17 MB (int8) · ~200 MB RAM",
        languages_summary: "8 languages (en, de, es, it, pt, ru, hi, fr)",
        badge: "Recommended",
        description: "Great everyday balance of natural voices, speed, and tiny CPU memory.",
        default_weights: Some("int8"),
    },
    ModelCatalogEntry {
        repo_id: "KittenML/kitten-tts-2-pico",
        display_name: "Kitten-TTS-2 Pico",
        category: ModelCategory::TinyCpu,
        params: "7M / lang",
        size_summary: "4 MB (int4) - 8 MB (int8) · ~160 MB RAM",
        languages_summary: "8 languages (en, de, es, it, pt, ru, hi, fr)",
        badge: "Ultra-Fast",
        description: "Smallest and fastest model — ideal for instant read-aloud on any CPU.",
        default_weights: Some("int4"),
    },
    ModelCatalogEntry {
        repo_id: "KittenML/kitten-tts-2-mini",
        display_name: "Kitten-TTS-2 Mini",
        category: ModelCategory::TinyCpu,
        params: "82M",
        size_summary: "47 MB (int4) - 83 MB (int8) · ~350 MB RAM",
        languages_summary: "16 languages including Japanese, Korean, Swedish, Turkish",
        badge: "16 Languages",
        description: "Single rich multilingual model supporting 16 languages and expressive prosody.",
        default_weights: Some("int8"),
    },
    ModelCatalogEntry {
        repo_id: "KittenML/kitten-tts-2",
        display_name: "Kitten-TTS-2 (1.7B Studio)",
        category: ModelCategory::StudioLm,
        params: "1.7B",
        size_summary: "492 MB (emb4) - 954 MB (packed) · ~2.5-3.8 GB RAM",
        languages_summary: "20+ languages, 115 voices + Voice Cloning",
        badge: "Cloning + Emotions",
        description: "Full speech language model (unpacks 1.7B bf16 + S3Codec in RAM) with zero-shot cloning, [emotion] tags, <events>, and (((emphasis))).",
        default_weights: Some("emb4"),
    },
    ModelCatalogEntry {
        repo_id: "KittenML/kitten-tts-mini-0.8",
        display_name: "Kitten-TTS Mini 0.8",
        category: ModelCategory::LegacyOnnx,
        params: "80M",
        size_summary: "80 MB",
        languages_summary: "English (8 classic voices)",
        badge: "Classic 0.8",
        description: "Original high-quality 0.8 ONNX model with speed control.",
        default_weights: None,
    },
    ModelCatalogEntry {
        repo_id: "KittenML/kitten-tts-micro-0.8",
        display_name: "Kitten-TTS Micro 0.8",
        category: ModelCategory::LegacyOnnx,
        params: "40M",
        size_summary: "41 MB",
        languages_summary: "English (8 classic voices)",
        badge: "Classic 0.8",
        description: "Balanced classic 0.8 ONNX model for CPU.",
        default_weights: None,
    },
    ModelCatalogEntry {
        repo_id: "KittenML/kitten-tts-nano-0.8-fp32",
        display_name: "Kitten-TTS Nano 0.8 (FP32)",
        category: ModelCategory::LegacyOnnx,
        params: "15M",
        size_summary: "56 MB",
        languages_summary: "English (8 classic voices)",
        badge: "Classic 0.8",
        description: "Compact full-precision 0.8 ONNX model.",
        default_weights: None,
    },
    ModelCatalogEntry {
        repo_id: "KittenML/kitten-tts-nano-0.8-int8",
        display_name: "Kitten-TTS Nano 0.8 (INT8)",
        category: ModelCategory::LegacyOnnx,
        params: "15M",
        size_summary: "25 MB",
        languages_summary: "English (8 classic voices)",
        badge: "Classic 0.8",
        description: "Quantized 0.8 ONNX model for low disk footprint.",
        default_weights: None,
    },
];

pub const EXPRESSION_TAGS: &[(&str, &str)] = &[
    ("joyful", "Happy and bright"),
    ("excited", "Upbeat and energetic"),
    ("tender", "Warm and gentle"),
    ("contemplative", "Thoughtful and calm"),
    ("surprised", "Astonished reaction"),
    ("nervous", "Hesitant and tense"),
    ("sad", "Soft and sorrowful"),
    ("stern", "Firm and serious"),
    ("angry", "Fierce and intense"),
    ("mundane", "Casual everyday tone"),
];

pub const VOCAL_EVENT_TAGS: &[(&str, &str)] = &[
    ("laugh", "Hearty laugh"),
    ("giggle", "Light giggle"),
    ("sigh", "Audible sigh"),
    ("gasp", "Sudden gasp"),
    ("pause", "Brief natural beat"),
    ("um", "Conversational filler"),
    ("scoff", "Playful scoff"),
    ("sob", "Emotional sob"),
    ("gulp", "Nervous gulp"),
    ("growl", "Playful growl"),
];

pub fn language_display_name(code: &str) -> String {
    match code {
        "en" => "English (en)".to_string(),
        "de" => "German (de)".to_string(),
        "es" => "Spanish (es)".to_string(),
        "fr" => "French (fr)".to_string(),
        "it" => "Italian (it)".to_string(),
        "pt" => "Portuguese (pt)".to_string(),
        "ru" => "Russian (ru)".to_string(),
        "hi" => "Hindi (hi)".to_string(),
        "da" => "Danish (da)".to_string(),
        "fi" => "Finnish (fi)".to_string(),
        "ja" => "Japanese (ja)".to_string(),
        "ko" => "Korean (ko)".to_string(),
        "nl" => "Dutch (nl)".to_string(),
        "sv" => "Swedish (sv)".to_string(),
        "sw" => "Swahili (sw)".to_string(),
        "tr" => "Turkish (tr)".to_string(),
        other => other.to_string(),
    }
}

pub fn sample_text_for_language(lang: &str) -> &'static str {
    match lang {
        "en" | "English" => "Hello! Welcome to Voxa Studio. Let's bring your script to life.",
        "de" | "German" => "Guten Morgen! Ich wünsche dir einen wunderschönen Tag.",
        "es" | "Spanish" => "Buenos días. Espero que tengas un día maravilloso.",
        "fr" | "French" => "Bonjour. Je te souhaite une excellente journée.",
        "it" | "Italian" => "Buongiorno. Ti auguro una splendida giornata.",
        "pt" | "Portuguese" => "Bom dia. Desejo-lhe um dia maravilhoso.",
        "ru" | "Russian" => "Доброе утро. Желаю тебе прекрасного дня.",
        "hi" | "Hindi" => "सुप्रभात। आपका दिन शुभ हो।",
        "ar" | "Arabic" => "صباح الخير. أتمنى لك يوماً رائعاً.",
        "zh" | "Chinese" => "早上好。祝你度过美好的一天。",
        "nl" | "Dutch" => "Goedemorgen. Ik wens je een fijne dag.",
        "id" | "Indonesian" => "Selamat pagi. Semoga harimu menyenangkan.",
        "ja" | "Japanese" => "おはようございます。素敵な一日をお過ごしください。",
        "ko" | "Korean" => "좋은 아침입니다. 오늘도 좋은 하루 보내세요.",
        "pl" | "Polish" => "Dzień dobry. Życzę ci wspaniałego dnia.",
        "tr" | "Turkish" => "Günaydın. Sana harika bir gün dilerim.",
        "da" => "Godmorgen. Jeg ønsker dig en dejlig dag.",
        "fi" => "Hyvää huomenta. Toivon sinulle ihanaa päivää.",
        "sv" => "God morgon. Jag önskar dig en underbar dag.",
        "sw" => "Habari za asubuhi. Nakutakia siku njema.",
        _ => "Hello! Welcome to Voxa Studio.",
    }
}

pub fn builtin_voice_description(name: &str) -> Option<&'static str> {
    match name {
        "Bella" => Some("Warm classic voice, female"),
        "Jasper" => Some("Clear classic voice, male"),
        "Luna" => Some("Bright classic voice, female"),
        "Bruno" => Some("Steady classic voice, male"),
        "Rosie" => Some("Friendly classic voice, female"),
        "Hugo" => Some("Rich classic voice, male"),
        "Kiki" => Some("Lively classic voice, female"),
        "Leo" => Some("Smooth classic voice, male"),
        "Matthew" => Some("Exhausted mechanic, male"),
        "Elliot" => Some("Grieving young, male"),
        "Willow" => Some("Hushed young, female"),
        "Dolores" => Some("Southern elderly, female"),
        "Victor" => Some("Controlled fury older, male"),
        "Dante" => Some("Playful smooth young, male"),
        "Alfred" => Some("Tender older, male"),
        "Saoirse" => Some("Joyful Irish, female"),
        "Claire" => Some("Playful professional, female"),
        "Raven" => Some("Controlled fury young, female"),
        "Marcus" => Some("Smooth radio DJ, male"),
        "Herbert" => Some("Wise elderly professor, male"),
        "Diana" => Some("Stern military officer, female"),
        "Laurence" => Some("Dramatic stage actor, male"),
        "Maeve" => Some("Cozy warm storyteller, female"),
        "Walter" => Some("Warm grandfather, male"),
        "Edith" => Some("Soft elderly grandmother, female"),
        "Miles" => Some("Gentle librarian, male"),
        "Grace" => Some("Soothing nurse, female"),
        "Reginald" => Some("Distinguished elderly diplomat, male"),
        "Iris" => Some("Hushed museum guide, female"),
        "Frank" => Some("Gravelly war veteran, male"),
        "Serena" => Some("Gentle yoga instructor, female"),
        "Julian" => Some("Dreamy poet soft, male"),
        "Eleanor" => Some("Somber historian, female"),
        "Otis" => Some("Rumbling blues musician, male"),
        "Vincent" => Some("Hushed museum guide, male"),
        "Martha" => Some("Weathered radio journalist, female"),
        "Sable" => Some("Hushed conspirator, female"),
        "Victoria" => Some("Regal narrator, female"),
        _ => None,
    }
}

/// Segment produced when highlighting KittenTTS 2 expression markup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkupKind {
    Plain,
    Emotion,
    VocalEvent,
    Emphasis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkupSegment {
    pub text: String,
    pub kind: MarkupKind,
}

pub fn is_recognized_emotion(tag: &str) -> bool {
    let key = tag.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    EXPRESSION_TAGS.iter().any(|(k, _)| *k == key)
        || matches!(
            key.as_str(),
            "happy"
                | "cheerful"
                | "upbeat"
                | "calm"
                | "thoughtful"
                | "whisper"
                | "gentle"
                | "soft"
                | "serious"
                | "firm"
                | "furious"
                | "mad"
                | "fearful"
                | "scared"
                | "anxious"
                | "shocked"
                | "astonished"
                | "neutral"
        )
}

pub fn is_recognized_event(tag: &str) -> bool {
    let key = tag.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    VOCAL_EVENT_TAGS.iter().any(|(k, _)| *k == key)
        || matches!(
            key.as_str(),
            "chuckle"
                | "laughter"
                | "haha"
                | "hehe"
                | "breath"
                | "breathe"
                | "exhale"
                | "inhale"
                | "cry"
                | "crying"
                | "weep"
                | "hmm"
                | "uh"
                | "er"
                | "cough"
                | "clear_throat"
                | "swallow"
                | "wait"
                | "beat"
        )
}

/// Detect `[emotion]`, `<event>`, and `(((emphasis)))` spans for live UI highlighting.
pub fn parse_expression_markup(text: &str) -> Vec<MarkupSegment> {
    let bytes = text.as_bytes();
    let len = text.len();
    let mut out = Vec::new();
    let mut plain_start = 0usize;
    let mut i = 0usize;

    while i < len {
        if !text.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let rest = &text[i..];

        // Check (((emphasis)))
        if rest.starts_with("(((") {
            if let Some(rel_end) = rest[3..].find(")))") {
                let inner = &rest[3..3 + rel_end];
                if !inner.is_empty() && inner.len() <= 80 && !inner.contains('\n') && !inner.contains('(') && !inner.contains(')') {
                    if i > plain_start {
                        out.push(MarkupSegment {
                            text: text[plain_start..i].to_string(),
                            kind: MarkupKind::Plain,
                        });
                    }
                    let end = i + 3 + rel_end + 3;
                    out.push(MarkupSegment {
                        text: text[i..end].to_string(),
                        kind: MarkupKind::Emphasis,
                    });
                    i = end;
                    plain_start = i;
                    continue;
                }
            }
        }

        // Check [emotion]
        if bytes[i] == b'[' {
            if let Some(rel_end) = rest[1..].find(']') {
                let tag = &rest[1..1 + rel_end];
                if is_recognized_emotion(tag) {
                    if i > plain_start {
                        out.push(MarkupSegment {
                            text: text[plain_start..i].to_string(),
                            kind: MarkupKind::Plain,
                        });
                    }
                    let end = i + 1 + rel_end + 1;
                    out.push(MarkupSegment {
                        text: text[i..end].to_string(),
                        kind: MarkupKind::Emotion,
                    });
                    i = end;
                    plain_start = i;
                    continue;
                }
            }
        }

        // Check <event>
        if bytes[i] == b'<' {
            if let Some(rel_end) = rest[1..].find('>') {
                let tag = &rest[1..1 + rel_end];
                if is_recognized_event(tag) {
                    if i > plain_start {
                        out.push(MarkupSegment {
                            text: text[plain_start..i].to_string(),
                            kind: MarkupKind::Plain,
                        });
                    }
                    let end = i + 1 + rel_end + 1;
                    out.push(MarkupSegment {
                        text: text[i..end].to_string(),
                        kind: MarkupKind::VocalEvent,
                    });
                    i = end;
                    plain_start = i;
                    continue;
                }
            }
        }

        let ch_len = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        i += ch_len;
    }

    if plain_start < len {
        out.push(MarkupSegment {
            text: text[plain_start..].to_string(),
            kind: MarkupKind::Plain,
        });
    }
    out
}

pub fn has_any_expression_markup(text: &str) -> bool {
    parse_expression_markup(text)
        .iter()
        .any(|seg| seg.kind != MarkupKind::Plain)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkupSpan {
    pub segment: MarkupSegment,
    pub start_char: usize,
    pub end_char: usize,
}

#[derive(Debug, Clone)]
pub struct TagTooltipInfo {
    pub badge: String,
    pub category: &'static str,
    pub description: String,
    pub placement_tip: &'static str,
}

pub fn parse_expression_spans(text: &str) -> Vec<MarkupSpan> {
    let mut out = Vec::new();
    let mut char_cursor = 0usize;
    for seg in parse_expression_markup(text) {
        let len = seg.text.chars().count();
        out.push(MarkupSpan {
            segment: seg,
            start_char: char_cursor,
            end_char: char_cursor + len,
        });
        char_cursor += len;
    }
    out
}

pub fn describe_markup_segment(seg: &MarkupSegment) -> Option<TagTooltipInfo> {
    match seg.kind {
        MarkupKind::Plain => None,
        MarkupKind::Emotion => {
            let inner = seg
                .text
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim();
            let norm = inner.to_ascii_lowercase().replace([' ', '-'], "_");
            let canonical = match norm.as_str() {
                "happy" | "cheerful" => "joyful",
                "upbeat" => "excited",
                "calm" | "thoughtful" => "contemplative",
                "whisper" | "gentle" | "soft" => "tender",
                "serious" | "firm" => "stern",
                "furious" | "mad" => "angry",
                "fearful" | "scared" | "anxious" => "nervous",
                "shocked" | "astonished" => "surprised",
                "neutral" => "mundane",
                other => other,
            };
            let meaning = match canonical {
                "joyful" => "Happy, bright, and cheerful tone — lifts pitch and smiling energy.",
                "excited" => "Upbeat, fast-paced, and enthusiastic delivery.",
                "tender" => "Warm, gentle, softer speaking voice with calm pacing.",
                "contemplative" => "Thoughtful, reflective, slower conversational tone.",
                "surprised" => "Astonished reaction with an animated, higher pitch inflection.",
                "nervous" => "Hesitant, tense, slightly trembling delivery.",
                "sad" => "Soft, slower, sorrowful voice with subdued warmth.",
                "stern" => "Firm, serious, grounded, authoritative cadence.",
                "angry" => "Fierce, intense, forceful delivery.",
                "mundane" => "Balanced, neutral, everyday conversational voice.",
                _ => "Adjusts speaker emotion & tone for the following speech.",
            };
            let alias_note = if canonical != norm {
                format!(" (alias for [{canonical}])")
            } else {
                String::new()
            };
            Some(TagTooltipInfo {
                badge: seg.text.clone(),
                category: "Emotion Tone Tag",
                description: format!("{meaning}{alias_note}"),
                placement_tip: "Place at the start of the script or before a sentence. Never spoken aloud as a word.",
            })
        }
        MarkupKind::VocalEvent => {
            let inner = seg
                .text
                .trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .trim();
            let norm = inner.to_ascii_lowercase().replace([' ', '-'], "_");
            let canonical = match norm.as_str() {
                "chuckle" | "laughter" | "haha" => "laugh",
                "hehe" => "giggle",
                "breath" | "breathe" | "exhale" => "sigh",
                "inhale" => "gasp",
                "cry" | "crying" | "weep" => "sob",
                "hmm" | "uh" | "er" => "um",
                "cough" | "clear_throat" => "scoff",
                "swallow" => "gulp",
                "wait" | "beat" => "pause",
                other => other,
            };
            let meaning = match canonical {
                "laugh" => "Inserts a natural hearty laugh sound without saying the word 'laugh'.",
                "giggle" => "Inserts a light, playful giggle.",
                "sigh" => "Inserts an audible exhale / emotional sigh.",
                "gasp" => "Inserts a sudden intake breath / surprised gasp.",
                "pause" => "Adds a clean natural beat of silence (~0.4s) between phrases.",
                "um" => "Inserts a relaxed conversational filler sound ('uhm').",
                "scoff" => "Inserts a brief dismissive scoff / throat clear.",
                "sob" => "Inserts an emotional crying / sobbing breath cue.",
                "gulp" => "Inserts a nervous swallow sound cue.",
                "growl" => "Inserts a low playful / fierce growl cue.",
                _ => "Inserts a non-verbal vocal cue without reading the tag name aloud.",
            };
            let alias_note = if canonical != norm {
                format!(" (alias for <{canonical}>)")
            } else {
                String::new()
            };
            Some(TagTooltipInfo {
                badge: seg.text.clone(),
                category: "Vocal Event Cue",
                description: format!("{meaning}{alias_note}"),
                placement_tip: "Place inline between words or clauses to insert this non-verbal vocal cue.",
            })
        }
        MarkupKind::Emphasis => {
            let inner = seg
                .text
                .trim()
                .strip_prefix("(((")
                .and_then(|s| s.strip_suffix(")))"))
                .unwrap_or(&seg.text)
                .trim();
            Some(TagTooltipInfo {
                badge: format!("((({inner})))"),
                category: "Word Stress & Emphasis Tag",
                description: format!(
                    "Speaks \"{inner}\" aloud with extra vocal stress, pitch accent, and emphasis while hiding the ((( ))) brackets."
                ),
                placement_tip: "Wrap 1-4 words in (((triple parentheses))) to make the speaker stress them.",
            })
        }
    }
}

// ── IPC Request Types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum WorkerRequest {
    Ping,
    LoadModel {
        model_name: String,
        backend: Option<String>,
        device: Option<String>,
        weights: Option<String>,
        decoder: Option<String>,
        cache_dir: Option<String>,
        hf_token: Option<String>,
        threads: Option<u32>,
    },
    Generate(GenerateRequest),
    Transcribe {
        reference: String,
    },
    VoiceSample {
        voice: String,
        output_wav: String,
    },
    Normalize {
        text: String,
    },
    Phonemize {
        text: String,
        language: Option<String>,
    },
    ExportAudio {
        input_wav: String,
        output_path: String,
        format: String,
    },
    AudioInfo {
        path: String,
        output_wav: Option<String>,
    },
    FreeMemory {
        unload_model: bool,
    },
    Shutdown,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerateRequest {
    pub text: String,
    pub voice: Option<String>,
    pub language: Option<String>,
    pub reference: Option<String>,
    pub reference_text: Option<String>,
    pub speed: Option<f32>,
    pub preset: Option<String>,
    pub temperature: Option<f32>,
    pub top_k: Option<u32>,
    pub top_p: Option<f32>,
    pub min_p: Option<f32>,
    pub max_new_tokens: Option<u32>,
    pub normalize: Option<bool>,
    pub use_reference_prompt: Option<bool>,
    pub advanced: Option<BTreeMap<String, serde_json::Value>>,
    pub stream: bool,
    pub output_wav: String,
}

// ── IPC Response / Event Types ─────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct VoiceEntry {
    pub name: String,
    pub language: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub has_reference: bool,
    #[serde(default)]
    pub transcript: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DecodePresetValues {
    pub temperature: f32,
    pub top_k: u32,
    pub top_p: f32,
    #[serde(default)]
    pub min_p: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ModelInfo {
    pub model_name: String,
    pub family: String, // "kitten2" | "kitten2_tiny" | "onnx_legacy"
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub device: Option<String>,
    #[serde(default)]
    pub weights: Option<String>,
    #[serde(default)]
    pub decoder: Option<String>,
    pub sample_rate: u32,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub default_language: Option<String>,
    #[serde(default)]
    pub default_voice: Option<String>,
    #[serde(default)]
    pub voices: Vec<VoiceEntry>,
    #[serde(default)]
    pub supports_cloning: bool,
    #[serde(default)]
    pub supports_expression: bool,
    #[serde(default)]
    pub supports_speed: bool,
    #[serde(default)]
    pub supports_phonemize: bool,
    #[serde(default)]
    pub supports_normalize: bool,
    #[serde(default)]
    pub supports_voice_sample: bool,
    #[serde(default)]
    pub available_decoders: Vec<String>,
    #[serde(default)]
    pub available_weights: Vec<String>,
    #[serde(default)]
    pub decode_presets: BTreeMap<String, DecodePresetValues>,
    #[serde(default)]
    pub default_preset: Option<String>,
    #[serde(default)]
    pub advanced_options: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub use_reference_prompt_default: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct NormalizeSpan {
    #[serde(default)]
    pub original: String,
    #[serde(default)]
    pub replacement: String,
    #[serde(default)]
    pub orig_start: Option<usize>,
    #[serde(default)]
    pub orig_end: Option<usize>,
    #[serde(default)]
    pub norm_start: Option<usize>,
    #[serde(default)]
    pub norm_end: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
#[allow(dead_code)]
pub enum WorkerEvent {
    Pong,
    Status {
        message: String,
    },
    Notice {
        message: String,
    },
    DownloadProgress {
        file_name: String,
        downloaded_bytes: u64,
        total_bytes: u64,
        speed_bps: f64,
        #[serde(default)]
        eta_s: Option<f64>,
    },
    ModelLoaded {
        info: ModelInfo,
    },
    Chunk {
        index: usize,
        wav_path: String,
        duration_s: f32,
    },
    Generated {
        wav_path: String,
        sample_rate: u32,
        duration_s: f32,
        elapsed_s: f32,
        chunks: usize,
    },
    Transcribed {
        text: String,
    },
    VoiceSampleReady {
        voice: String,
        wav_path: String,
        transcript: String,
    },
    Normalized {
        original: String,
        normalized: String,
        #[serde(default)]
        spans: Vec<NormalizeSpan>,
    },
    Phonemized {
        chunks: Vec<String>,
        #[serde(default)]
        note: Option<String>,
    },
    Exported {
        output_path: String,
        format: String,
    },
    AudioInfo {
        path: String,
        duration_s: f32,
        clipped: bool,
        playback_wav: Option<String>,
    },
    MemoryFreed {
        unloaded: bool,
        ram_mb: f32,
    },
    Error {
        message: String,
        #[serde(default)]
        detail: Option<String>,
    },
    #[serde(skip)]
    Log(String),
    #[serde(skip)]
    Exited(String),
}
