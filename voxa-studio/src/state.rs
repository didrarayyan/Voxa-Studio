use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedVoiceProfile {
    pub id: String,
    pub name: String,
    pub wav_path: String,
    pub transcript: String,
    pub duration_secs: f32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryItem {
    pub id: String,
    pub title: String,
    pub text: String,
    pub voice_label: String,
    pub model_name: String,
    pub wav_path: String,
    pub duration_secs: f32,
    pub elapsed_secs: f32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub theme: ThemeMode,
    pub last_model_name: String,
    pub last_backend: Option<String>,
    pub last_device: Option<String>,
    pub last_weights: Option<String>,
    pub last_decoder: Option<String>,
    pub threads: u32,
    pub cache_dir: String,
    pub hf_token: String,
    pub auto_play: bool,
    pub stream_playback: bool,
    pub volume: f32,
    pub favorites: BTreeSet<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Dark,
            last_model_name: "KittenML/kitten-tts-2".to_string(),
            last_backend: None,
            last_device: None,
            last_weights: Some("emb4".to_string()),
            last_decoder: Some("student_w4".to_string()),
            threads: 4,
            cache_dir: String::new(),
            hf_token: String::new(),
            auto_play: true,
            stream_playback: true,
            volume: 0.9,
            favorites: ["Eleanor", "Paul", "Bruno", "Luna", "Maeve", "Walter"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
}

pub struct StoryStarter {
    pub title: &'static str,
    pub text: &'static str,
    pub needs_kitten2: bool,
}

pub const STORY_STARTERS: &[StoryStarter] = &[
    StoryStarter {
        title: "Cozy Bedtime Story",
        text: "Once upon a time, in a quiet little cottage by the woods, a curious red fox discovered a glowing lantern on the porch. Every evening at sunset, the lantern hummed a gentle lullaby that helped all the woodland animals drift peacefully to sleep.",
        needs_kitten2: false,
    },
    StoryStarter {
        title: "Happy Celebration (with expressions)",
        text: "[joyful] Guess what happened today? <laugh> We actually finished the treehouse before dinner, and it looks (((amazing)))!",
        needs_kitten2: true,
    },
    StoryStarter {
        title: "Friendly Morning Greeting",
        text: "Good morning! Hope everyone has a bright, cheerful day ahead. Remember to take a deep breath, smile, and enjoy the little moments.",
        needs_kitten2: false,
    },
    StoryStarter {
        title: "Numbers, Prices & Dates Check",
        text: "Dr. Rivera arrived at 3:05 p.m. on March 14th, 2026 with 12 boxes of cookies costing $4.50 each — a 25% discount for the neighborhood picnic!",
        needs_kitten2: false,
    },
    StoryStarter {
        title: "Gentle Mystery (with pause & whisper feel)",
        text: "[contemplative] Wait a second... <pause> Did you hear those soft footsteps upstairs? <gasp> It was only the evening breeze tapping the window shutters!",
        needs_kitten2: true,
    },
];

pub struct StudioStorage {
    pub data_dir: PathBuf,
    pub outputs_dir: PathBuf,
    pub voices_dir: PathBuf,
    pub recordings_dir: PathBuf,
    pub temp_dir: PathBuf,
}

impl StudioStorage {
    pub fn init(workspace_root: Option<&Path>) -> Self {
        let base = workspace_root
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let data_dir = base.join("studio_data");
        let outputs_dir = data_dir.join("outputs");
        let voices_dir = data_dir.join("saved_voices");
        let recordings_dir = voices_dir.join("_recordings");
        let temp_dir = data_dir.join("_temp");

        for dir in [&data_dir, &outputs_dir, &voices_dir, &recordings_dir, &temp_dir] {
            let _ = std::fs::create_dir_all(dir);
        }

        Self {
            data_dir,
            outputs_dir,
            voices_dir,
            recordings_dir,
            temp_dir,
        }
    }

    pub fn load_config(&self) -> AppConfig {
        let path = self.data_dir.join("config.json");
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_config(&self, cfg: &AppConfig) {
        let path = self.data_dir.join("config.json");
        if let Ok(json) = serde_json::to_string_pretty(cfg) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn load_history(&self) -> Vec<HistoryItem> {
        let path = self.data_dir.join("history.json");
        let mut items: Vec<HistoryItem> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        items.retain(|it| Path::new(&it.wav_path).is_file());
        items
    }

    pub fn save_history(&self, items: &[HistoryItem]) {
        let path = self.data_dir.join("history.json");
        let trimmed: Vec<&HistoryItem> = items.iter().take(25).collect();
        if let Ok(json) = serde_json::to_string_pretty(&trimmed) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn load_saved_voices(&self) -> Vec<SavedVoiceProfile> {
        let path = self.voices_dir.join("profiles.json");
        let mut items: Vec<SavedVoiceProfile> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        items.retain(|v| Path::new(&v.wav_path).is_file());
        items
    }

    pub fn save_saved_voices(&self, profiles: &[SavedVoiceProfile]) {
        let path = self.voices_dir.join("profiles.json");
        if let Ok(json) = serde_json::to_string_pretty(profiles) {
            let _ = std::fs::write(path, json);
        }
    }
}
