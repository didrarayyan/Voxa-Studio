use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use hound::{SampleFormat as HoundSampleFormat, WavSpec, WavWriter};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub const MAX_RECORD_SECS: f32 = 30.0;

pub struct MicRecorder {
    stream: Option<Stream>,
    writer: Option<Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>>,
    stop_flag: Arc<AtomicBool>,
    level_bits: Arc<AtomicU32>,
    started_at: Option<Instant>,
    output_path: Option<PathBuf>,
}

impl MicRecorder {
    pub fn new() -> Self {
        Self {
            stream: None,
            writer: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
            level_bits: Arc::new(AtomicU32::new(0)),
            started_at: None,
            output_path: None,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.stream.is_some()
    }

    pub fn elapsed_secs(&self) -> f32 {
        self.started_at
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0)
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.level_bits.load(Ordering::Relaxed)).clamp(0.0, 1.0)
    }

    pub fn start(&mut self, output_path: PathBuf) -> Result<(), String> {
        self.stop();

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to prepare recording folder: {e}"))?;
        }

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "No microphone input device was found.".to_string())?;
        let supported = device
            .default_input_config()
            .map_err(|e| format!("Could not read microphone configuration: {e}"))?;

        let sample_rate = supported.sample_rate().0;
        let channels = supported.channels().max(1) as usize;
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();

        let spec = WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: HoundSampleFormat::Int,
        };
        let wav_writer = WavWriter::create(&output_path, spec)
            .map_err(|e| format!("Could not create recording file: {e}"))?;
        let writer_arc = Arc::new(Mutex::new(Some(wav_writer)));
        let writer_cb = Arc::clone(&writer_arc);

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_cb = Arc::clone(&stop_flag);
        let level_bits = Arc::new(AtomicU32::new(0));
        let level_cb = Arc::clone(&level_bits);

        let err_fn = |err| {
            eprintln!("Microphone stream error: {err}");
        };

        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                &config,
                move |data: &[f32], _| {
                    if stop_cb.load(Ordering::Relaxed) {
                        return;
                    }
                    write_mono_samples(
                        data.chunks(channels).map(|frame| {
                            frame.iter().copied().sum::<f32>() / channels as f32
                        }),
                        &writer_cb,
                        &level_cb,
                    );
                },
                err_fn,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                &config,
                move |data: &[i16], _| {
                    if stop_cb.load(Ordering::Relaxed) {
                        return;
                    }
                    write_mono_samples(
                        data.chunks(channels).map(|frame| {
                            let sum: f32 = frame.iter().map(|&s| s as f32 / 32768.0).sum();
                            sum / channels as f32
                        }),
                        &writer_cb,
                        &level_cb,
                    );
                },
                err_fn,
                None,
            ),
            SampleFormat::U16 => device.build_input_stream(
                &config,
                move |data: &[u16], _| {
                    if stop_cb.load(Ordering::Relaxed) {
                        return;
                    }
                    write_mono_samples(
                        data.chunks(channels).map(|frame| {
                            let sum: f32 = frame
                                .iter()
                                .map(|&s| (s as f32 - 32768.0) / 32768.0)
                                .sum();
                            sum / channels as f32
                        }),
                        &writer_cb,
                        &level_cb,
                    );
                },
                err_fn,
                None,
            ),
            other => {
                return Err(format!("Unsupported microphone sample format: {other:?}"));
            }
        }
        .map_err(|e| format!("Could not open microphone stream: {e}"))?;

        stream
            .play()
            .map_err(|e| format!("Could not start recording: {e}"))?;

        self.stream = Some(stream);
        self.writer = Some(writer_arc);
        self.stop_flag = stop_flag;
        self.level_bits = level_bits;
        self.started_at = Some(Instant::now());
        self.output_path = Some(output_path);
        Ok(())
    }

    pub fn stop(&mut self) -> Option<PathBuf> {
        self.stop_flag.store(true, Ordering::Relaxed);
        self.stream.take();
        if let Some(writer_arc) = self.writer.take() {
            if let Ok(mut guard) = writer_arc.lock() {
                if let Some(w) = guard.take() {
                    let _ = w.finalize();
                }
            }
        }
        self.level_bits.store(0, Ordering::Relaxed);
        self.started_at = None;
        self.output_path.take()
    }
}

impl Drop for MicRecorder {
    fn drop(&mut self) {
        self.stop();
    }
}

fn write_mono_samples<I>(
    iter: I,
    writer_arc: &Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>,
    level_bits: &Arc<AtomicU32>,
) where
    I: Iterator<Item = f32>,
{
    let mut peak = 0.0f32;
    if let Ok(mut guard) = writer_arc.lock() {
        if let Some(writer) = guard.as_mut() {
            for sample in iter {
                let clamped = sample.clamp(-1.0, 1.0);
                let abs = clamped.abs();
                if abs > peak {
                    peak = abs;
                }
                let pcm = (clamped * 32767.0) as i16;
                let _ = writer.write_sample(pcm);
            }
        }
    }
    level_bits.store(peak.to_bits(), Ordering::Relaxed);
}
