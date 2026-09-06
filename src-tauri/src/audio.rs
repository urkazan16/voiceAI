//! Microphone capture. The input stream exists only while recording: `start` on hotkey
//! down, `stop` drops the CPAL stream after the utterance, a short hold, or cancel.
//! `list_input_devices` enumerates Bluetooth and virtual sources without opening a stream.

use crate::error::{LfError, LfResult};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

static LAST_STREAM_ERROR: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn stream_error_slot() -> &'static Mutex<Option<String>> {
    LAST_STREAM_ERROR.get_or_init(|| Mutex::new(None))
}

pub fn take_stream_error() -> Option<String> {
    stream_error_slot().lock().ok().and_then(|mut g| g.take())
}

fn remember_stream_error(message: String) {
    if let Ok(mut slot) = stream_error_slot().lock() {
        *slot = Some(message);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioDevice {
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone)]
pub struct CapturedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

pub struct LiveCapture {
    stream: cpal::Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
}

pub type SharedCapture = Arc<CaptureHub>;

enum CaptureCommand {
    Start {
        name: Option<String>,
        reply: std::sync::mpsc::Sender<LfResult<()>>,
    },
    Stop {
        reply: std::sync::mpsc::Sender<Option<CapturedAudio>>,
    },
    Peek {
        reply: std::sync::mpsc::Sender<Option<CapturedAudio>>,
    },
    IsRecording {
        reply: std::sync::mpsc::Sender<bool>,
    },
}

pub struct CaptureHub {
    tx: std::sync::mpsc::Sender<CaptureCommand>,
}

impl CaptureHub {
    pub fn spawn() -> SharedCapture {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("localflow-audio".into())
            .spawn(move || {
                let mut live: Option<LiveCapture> = None;
                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        CaptureCommand::Start { name, reply } => {
                            if live.is_some() {
                                drop(live.take().map(LiveCapture::finish));
                                std::thread::sleep(Duration::from_millis(40));
                            }
                            let mut result = start_capture(name.as_deref());
                            for delay_ms in [80_u64, 160, 320] {
                                if result.is_ok() {
                                    break;
                                }
                                std::thread::sleep(Duration::from_millis(delay_ms));
                                result = start_capture(name.as_deref());
                            }
                            let result = result.map(|capture| {
                                live = Some(capture);
                            });
                            let _ = reply.send(result);
                        }
                        CaptureCommand::Stop { reply } => {
                            let audio = live.take().map(LiveCapture::finish);
                            let _ = reply.send(audio);
                        }
                        CaptureCommand::Peek { reply } => {
                            let audio = live.as_ref().map(|cap| cap.peek());
                            let _ = reply.send(audio);
                        }
                        CaptureCommand::IsRecording { reply } => {
                            let _ = reply.send(live.is_some());
                        }
                    }
                }
            })
            .expect("start audio thread");
        Arc::new(Self { tx })
    }

    pub fn start(&self, name: Option<String>) -> LfResult<()> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(CaptureCommand::Start { name, reply })
            .map_err(|_| LfError::DeviceUnavailable("audio thread stopped".into()))?;
        rx.recv()
            .map_err(|_| LfError::DeviceUnavailable("audio thread stopped".into()))?
    }

    pub fn stop(&self) -> Option<CapturedAudio> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx.send(CaptureCommand::Stop { reply }).ok()?;
        rx.recv().ok().flatten()
    }

    pub fn peek(&self) -> Option<CapturedAudio> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx.send(CaptureCommand::Peek { reply }).ok()?;
        rx.recv().ok().flatten()
    }

    pub fn is_recording(&self) -> bool {
        let (reply, rx) = std::sync::mpsc::channel();
        if self.tx.send(CaptureCommand::IsRecording { reply }).is_err() {
            return false;
        }
        rx.recv().unwrap_or(false)
    }
}

pub fn list_input_devices() -> LfResult<Vec<AudioDevice>> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();
    let mut devices = Vec::new();
    match host.input_devices() {
        Ok(iter) => {
            for device in iter {
                let name = device.name().unwrap_or_else(|_| "Unknown".into());
                let is_default = name == default_name;
                devices.push(AudioDevice { name, is_default });
            }
        }
        Err(err) => return Err(LfError::DeviceUnavailable(err.to_string())),
    }
    Ok(devices)
}

pub fn start_capture(preferred_name: Option<&str>) -> LfResult<LiveCapture> {
    use cpal::traits::{DeviceTrait, StreamTrait};
    let host = cpal::default_host();
    let device = select_input_device(&host, preferred_name)?;
    let config = device
        .default_input_config()
        .map_err(|e| map_capture_error(e.to_string()))?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    let samples = Arc::new(Mutex::new(Vec::new()));
    let writer = samples.clone();
    let err_fn = |err: cpal::StreamError| {
        let message = err.to_string();
        remember_stream_error(message.clone());
        eprintln!("audio stream error: {message}");
    };
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device
            .build_input_stream(
                &config.into(),
                move |data: &[f32], _| {
                    if let Ok(mut buf) = writer.lock() {
                        buf.extend_from_slice(data);
                    }
                },
                err_fn,
                None,
            )
            .map_err(|e| map_capture_error(e.to_string()))?,
        cpal::SampleFormat::I16 => device
            .build_input_stream(
                &config.into(),
                move |data: &[i16], _| {
                    if let Ok(mut buf) = writer.lock() {
                        buf.extend(data.iter().map(|s| *s as f32 / 32768.0));
                    }
                },
                err_fn,
                None,
            )
            .map_err(|e| map_capture_error(e.to_string()))?,
        other => {
            return Err(LfError::DeviceUnavailable(format!(
                "unsupported sample format {other:?}"
            )))
        }
    };
    stream
        .play()
        .map_err(|e| map_capture_error(e.to_string()))?;
    Ok(LiveCapture {
        stream,
        samples,
        sample_rate,
        channels,
    })
}

impl LiveCapture {
    pub fn peek(&self) -> CapturedAudio {
        let samples = self.samples.lock().map(|g| g.clone()).unwrap_or_default();
        CapturedAudio {
            samples,
            sample_rate: self.sample_rate,
            channels: self.channels,
        }
    }

    pub fn finish(self) -> CapturedAudio {
        drop(self.stream);
        std::thread::sleep(Duration::from_millis(20));
        let samples = self.samples.lock().map(|g| g.clone()).unwrap_or_default();
        CapturedAudio {
            samples,
            sample_rate: self.sample_rate,
            channels: self.channels,
        }
    }
}

pub fn to_whisper_pcm(audio: &CapturedAudio) -> Vec<f32> {
    let mono = downmix_mono(&audio.samples, audio.channels);
    resample_linear(&mono, audio.sample_rate, 16_000)
}

pub fn duration_ms(audio: &CapturedAudio) -> u64 {
    if audio.sample_rate == 0 || audio.channels == 0 {
        return 0;
    }
    let frames = audio.samples.len() as u64 / u64::from(audio.channels.max(1));
    frames * 1000 / u64::from(audio.sample_rate)
}

pub fn downmix_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    if ch == 1 {
        return samples.to_vec();
    }
    samples
        .chunks(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

pub fn resample_linear(input: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if input.is_empty() || from_hz == 0 {
        return Vec::new();
    }
    if from_hz == to_hz {
        return input.to_vec();
    }
    let ratio = f64::from(from_hz) / f64::from(to_hz);
    let out_len = ((input.len() as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 * ratio;
        let idx = src.floor() as usize;
        let frac = src - idx as f64;
        let a = input[idx];
        let b = input.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac as f32);
    }
    out
}

fn map_capture_error(message: String) -> LfError {
    let lower = message.to_lowercase();
    if lower.contains("permission")
        || lower.contains("denied")
        || lower.contains("not authorized")
        || lower.contains("errorkisdenied")
        || lower.contains("-54")
        || lower.contains("busy")
        || lower.contains("in use")
        || lower.contains("occupied")
    {
        if lower.contains("busy") || lower.contains("in use") || lower.contains("occupied") {
            LfError::DeviceUnavailable(format!("busy: {message}"))
        } else {
            LfError::PermissionDenied("Microphone permission is required for dictation".into())
        }
    } else if lower.contains("unplug")
        || lower.contains("disconnect")
        || lower.contains("not connected")
        || lower.contains("removed")
    {
        LfError::DeviceUnavailable(format!("disconnected: {message}"))
    } else {
        LfError::DeviceUnavailable(message)
    }
}

fn select_input_device(host: &cpal::Host, preferred_name: Option<&str>) -> LfResult<cpal::Device> {
    use cpal::traits::{DeviceTrait, HostTrait};
    if let Some(name) = preferred_name {
        if let Ok(mut devices) = host.input_devices() {
            if let Some(device) = devices.find(|d| d.name().ok().as_deref() == Some(name)) {
                return Ok(device);
            }
        }
    }
    host.default_input_device()
        .ok_or_else(|| LfError::DeviceUnavailable("no input device".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_downmix_averages_channels() {
        let mono = downmix_mono(&[0.0, 1.0, 0.5, 0.5], 2);
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 0.5).abs() < f32::EPSILON);
        assert!((mono[1] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn resample_halves_length_when_rate_doubles() {
        let input: Vec<f32> = (0..16).map(|i| i as f32).collect();
        let out = resample_linear(&input, 32_000, 16_000);
        assert_eq!(out.len(), 8);
    }

    #[test]
    fn permission_errors_are_not_generic_device_unavailable() {
        let err = map_capture_error("PermissionDenied".into());
        assert_eq!(err.code(), "PERMISSION_DENIED");
    }

    #[test]
    fn busy_and_disconnect_errors_keep_device_unavailable_code() {
        let busy = map_capture_error("Device is busy".into());
        assert_eq!(busy.code(), "DEVICE_UNAVAILABLE");
        assert!(busy.to_string().to_lowercase().contains("busy"));
        let gone = map_capture_error("device disconnected".into());
        assert_eq!(gone.code(), "DEVICE_UNAVAILABLE");
        assert!(gone.to_string().to_lowercase().contains("disconnect"));
    }
}
