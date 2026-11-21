use rodio::buffer::SamplesBuffer;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const WAVEFORM_BARS: usize = 140;

#[derive(Debug, Clone)]
pub struct PlaybackSnapshot {
    pub loaded_path: Option<PathBuf>,
    pub duration_secs: f32,
    pub position_secs: f32,
    pub is_playing: bool,
    pub is_paused: bool,
    pub is_streaming: bool,
    pub volume: f32,
    pub peaks: Vec<f32>,
    pub last_error: Option<String>,
}

impl Default for PlaybackSnapshot {
    fn default() -> Self {
        Self {
            loaded_path: None,
            duration_secs: 0.0,
            position_secs: 0.0,
            is_playing: false,
            is_paused: false,
            is_streaming: false,
            volume: 0.9,
            peaks: Vec::new(),
            last_error: None,
        }
    }
}

enum AudioCmd {
    LoadAndPlay { path: PathBuf, auto_play: bool },
    StartStream,
    AppendStreamChunk { path: PathBuf },
    FinishStream { final_path: PathBuf },
    TogglePause,
    Stop,
    Replay,
    Seek { fraction: f32 },
    SetVolume(f32),
}

pub struct AudioPlayer {
    tx: Sender<AudioCmd>,
    snapshot: Arc<Mutex<PlaybackSnapshot>>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<AudioCmd>();
        let snapshot = Arc::new(Mutex::new(PlaybackSnapshot::default()));
        let snap_bg = Arc::clone(&snapshot);

        thread::spawn(move || {
            let stream_res = OutputStream::try_default();
            let (_stream, handle): (Option<OutputStream>, Option<OutputStreamHandle>) =
                match stream_res {
                    Ok((s, h)) => (Some(s), Some(h)),
                    Err(e) => {
                        if let Ok(mut guard) = snap_bg.lock() {
                            guard.last_error = Some(format!("Audio output unavailable: {e}"));
                        }
                        (None, None)
                    }
                };

            let mut sink: Option<Sink> = handle.as_ref().and_then(|h| Sink::try_new(h).ok());
            let mut samples: Vec<f32> = Vec::new();
            let mut sample_rate: u32 = 24_000;
            let mut volume: f32 = 0.9;
            let mut play_started_at: Option<Instant> = None;
            let mut base_offset_secs: f32 = 0.0;
            let mut paused: bool = false;
            let mut streaming: bool = false;

            if let Some(s) = &sink {
                s.set_volume(volume);
            }

            loop {
                // Poll commands with short timeout so position stays fresh during playback
                match rx.recv_timeout(Duration::from_millis(40)) {
                    Ok(cmd) => match cmd {
                        AudioCmd::LoadAndPlay { path, auto_play } => {
                            streaming = false;
                            match decode_mono_f32(&path) {
                                Ok((decoded, sr)) => {
                                    samples = decoded;
                                    sample_rate = sr.max(1);
                                    base_offset_secs = 0.0;
                                    if let Some(s) = &sink {
                                        s.stop();
                                    }
                                    sink = handle.as_ref().and_then(|h| Sink::try_new(h).ok());
                                    if let Some(s) = &sink {
                                        s.set_volume(volume);
                                        if auto_play && !samples.is_empty() {
                                            s.append(SamplesBuffer::new(1, sample_rate, samples.clone()));
                                            s.play();
                                            paused = false;
                                            play_started_at = Some(Instant::now());
                                        } else {
                                            paused = false;
                                            play_started_at = None;
                                        }
                                    }
                                    let peaks = compute_peaks(&samples, WAVEFORM_BARS);
                                    let dur = samples.len() as f32 / sample_rate as f32;
                                    if let Ok(mut g) = snap_bg.lock() {
                                        g.loaded_path = Some(path);
                                        g.duration_secs = dur;
                                        g.position_secs = 0.0;
                                        g.peaks = peaks;
                                        g.is_streaming = false;
                                        g.last_error = None;
                                    }
                                }
                                Err(err) => {
                                    if let Ok(mut g) = snap_bg.lock() {
                                        g.last_error = Some(err);
                                    }
                                }
                            }
                        }
                        AudioCmd::StartStream => {
                            streaming = true;
                            samples.clear();
                            sample_rate = 24_000;
                            base_offset_secs = 0.0;
                            paused = false;
                            if let Some(s) = &sink {
                                s.stop();
                            }
                            sink = handle.as_ref().and_then(|h| Sink::try_new(h).ok());
                            if let Some(s) = &sink {
                                s.set_volume(volume);
                                s.play();
                            }
                            play_started_at = Some(Instant::now());
                            if let Ok(mut g) = snap_bg.lock() {
                                g.loaded_path = None;
                                g.duration_secs = 0.0;
                                g.position_secs = 0.0;
                                g.peaks.clear();
                                g.is_streaming = true;
                                g.is_playing = true;
                                g.is_paused = false;
                                g.last_error = None;
                            }
                        }
                        AudioCmd::AppendStreamChunk { path } => {
                            if let Ok((chunk, sr)) = decode_mono_f32(&path) {
                                sample_rate = sr.max(1);
                                samples.extend_from_slice(&chunk);
                                if let Some(s) = &sink {
                                    let was_empty = s.empty();
                                    s.append(SamplesBuffer::new(1, sample_rate, chunk));
                                    if was_empty && !paused {
                                        s.play();
                                        if play_started_at.is_none() {
                                            play_started_at = Some(Instant::now());
                                        }
                                    }
                                }
                                let peaks = compute_peaks(&samples, WAVEFORM_BARS);
                                let dur = samples.len() as f32 / sample_rate as f32;
                                if let Ok(mut g) = snap_bg.lock() {
                                    g.duration_secs = dur;
                                    g.peaks = peaks;
                                }
                            }
                        }
                        AudioCmd::FinishStream { final_path } => {
                            streaming = false;
                            if let Ok((decoded, sr)) = decode_mono_f32(&final_path) {
                                samples = decoded;
                                sample_rate = sr.max(1);
                                let peaks = compute_peaks(&samples, WAVEFORM_BARS);
                                let dur = samples.len() as f32 / sample_rate as f32;
                                if let Ok(mut g) = snap_bg.lock() {
                                    g.loaded_path = Some(final_path);
                                    g.duration_secs = dur;
                                    g.peaks = peaks;
                                    g.is_streaming = false;
                                }
                            } else if let Ok(mut g) = snap_bg.lock() {
                                g.is_streaming = false;
                            }
                        }
                        AudioCmd::TogglePause => {
                            if let Some(s) = &sink {
                                if s.empty() && !samples.is_empty() {
                                    // Restart from beginning
                                    base_offset_secs = 0.0;
                                    s.append(SamplesBuffer::new(1, sample_rate, samples.clone()));
                                    s.play();
                                    paused = false;
                                    play_started_at = Some(Instant::now());
                                } else if paused {
                                    s.play();
                                    paused = false;
                                    play_started_at = Some(Instant::now());
                                } else {
                                    if let Some(t0) = play_started_at.take() {
                                        base_offset_secs += t0.elapsed().as_secs_f32();
                                    }
                                    s.pause();
                                    paused = true;
                                }
                            }
                        }
                        AudioCmd::Stop => {
                            streaming = false;
                            paused = false;
                            base_offset_secs = 0.0;
                            play_started_at = None;
                            if let Some(s) = &sink {
                                s.stop();
                            }
                            sink = handle.as_ref().and_then(|h| Sink::try_new(h).ok());
                            if let Some(s) = &sink {
                                s.set_volume(volume);
                            }
                        }
                        AudioCmd::Replay => {
                            if !samples.is_empty() {
                                streaming = false;
                                base_offset_secs = 0.0;
                                paused = false;
                                if let Some(s) = &sink {
                                    s.stop();
                                }
                                sink = handle.as_ref().and_then(|h| Sink::try_new(h).ok());
                                if let Some(s) = &sink {
                                    s.set_volume(volume);
                                    s.append(SamplesBuffer::new(1, sample_rate, samples.clone()));
                                    s.play();
                                    play_started_at = Some(Instant::now());
                                }
                            }
                        }
                        AudioCmd::Seek { fraction } => {
                            if !samples.is_empty() {
                                let clamped = fraction.clamp(0.0, 0.999);
                                let total_dur = samples.len() as f32 / sample_rate as f32;
                                let target_secs = clamped * total_dur;
                                let start_idx = ((clamped * samples.len() as f32) as usize)
                                    .min(samples.len().saturating_sub(1));
                                let was_paused = paused;
                                let was_playing = sink.as_ref().map(|s| !s.empty()).unwrap_or(false);
                                base_offset_secs = target_secs;

                                if let Some(s) = &sink {
                                    s.stop();
                                }
                                sink = handle.as_ref().and_then(|h| Sink::try_new(h).ok());
                                if let Some(s) = &sink {
                                    s.set_volume(volume);
                                    s.append(SamplesBuffer::new(
                                        1,
                                        sample_rate,
                                        samples[start_idx..].to_vec(),
                                    ));
                                    if was_paused && was_playing {
                                        s.pause();
                                        paused = true;
                                        play_started_at = None;
                                    } else {
                                        s.play();
                                        paused = false;
                                        play_started_at = Some(Instant::now());
                                    }
                                }
                            }
                        }
                        AudioCmd::SetVolume(v) => {
                            volume = v.clamp(0.0, 1.5);
                            if let Some(s) = &sink {
                                s.set_volume(volume);
                            }
                            if let Ok(mut g) = snap_bg.lock() {
                                g.volume = volume;
                            }
                        }
                    },
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }

                // Update live snapshot
                let dur = if samples.is_empty() {
                    0.0
                } else {
                    samples.len() as f32 / sample_rate as f32
                };
                let sink_empty = sink.as_ref().map(|s| s.empty()).unwrap_or(true);
                let is_playing = !sink_empty && !paused;
                let pos = if sink_empty && !streaming {
                    play_started_at = None;
                    paused = false;
                    if base_offset_secs >= dur {
                        base_offset_secs = 0.0;
                    }
                    base_offset_secs
                } else if paused {
                    base_offset_secs.min(dur)
                } else if let Some(t0) = play_started_at {
                    (base_offset_secs + t0.elapsed().as_secs_f32()).min(dur)
                } else {
                    base_offset_secs.min(dur)
                };

                if let Ok(mut g) = snap_bg.lock() {
                    g.duration_secs = dur;
                    g.position_secs = pos;
                    g.is_playing = is_playing;
                    g.is_paused = paused && !sink_empty;
                    g.is_streaming = streaming;
                }
            }
        });

        Self { tx, snapshot }
    }

    pub fn snapshot(&self) -> PlaybackSnapshot {
        self.snapshot.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn load_and_play(&self, path: impl AsRef<Path>, auto_play: bool) {
        let _ = self.tx.send(AudioCmd::LoadAndPlay {
            path: path.as_ref().to_path_buf(),
            auto_play,
        });
    }

    pub fn start_stream(&self) {
        let _ = self.tx.send(AudioCmd::StartStream);
    }

    pub fn append_stream_chunk(&self, path: impl AsRef<Path>) {
        let _ = self.tx.send(AudioCmd::AppendStreamChunk {
            path: path.as_ref().to_path_buf(),
        });
    }

    pub fn finish_stream(&self, final_path: impl AsRef<Path>) {
        let _ = self.tx.send(AudioCmd::FinishStream {
            final_path: final_path.as_ref().to_path_buf(),
        });
    }

    pub fn toggle_pause(&self) {
        let _ = self.tx.send(AudioCmd::TogglePause);
    }

    pub fn stop(&self) {
        let _ = self.tx.send(AudioCmd::Stop);
    }

    pub fn replay(&self) {
        let _ = self.tx.send(AudioCmd::Replay);
    }

    pub fn seek(&self, fraction: f32) {
        let _ = self.tx.send(AudioCmd::Seek { fraction });
    }

    pub fn set_volume(&self, volume: f32) {
        let _ = self.tx.send(AudioCmd::SetVolume(volume));
    }
}

pub fn decode_mono_f32(path: &Path) -> Result<(Vec<f32>, u32), String> {
    let file = File::open(path).map_err(|e| format!("Cannot open audio file: {e}"))?;
    let decoder = Decoder::new(BufReader::new(file))
        .map_err(|e| format!("Cannot decode audio file ({}): {e}", path.display()))?;
    let channels = decoder.channels().max(1) as usize;
    let sample_rate = decoder.sample_rate();
    let interleaved: Vec<f32> = decoder.convert_samples::<f32>().collect();

    if channels == 1 {
        return Ok((interleaved, sample_rate));
    }

    let frames = interleaved.len() / channels;
    let mut mono = Vec::with_capacity(frames);
    for f in 0..frames {
        let mut sum = 0.0f32;
        for c in 0..channels {
            sum += interleaved[f * channels + c];
        }
        mono.push(sum / channels as f32);
    }
    Ok((mono, sample_rate))
}

pub fn compute_peaks(samples: &[f32], num_bars: usize) -> Vec<f32> {
    if samples.is_empty() || num_bars == 0 {
        return Vec::new();
    }
    let mut peaks = Vec::with_capacity(num_bars);
    let step = (samples.len() as f32 / num_bars as f32).max(1.0);
    for i in 0..num_bars {
        let start = ((i as f32 * step) as usize).min(samples.len() - 1);
        let end = (((i + 1) as f32 * step) as usize)
            .max(start + 1)
            .min(samples.len());
        let mut peak = 0.0f32;
        for &s in &samples[start..end] {
            let a = s.abs();
            if a > peak {
                peak = a;
            }
        }
        peaks.push(peak.clamp(0.03, 1.0));
    }
    peaks
}
