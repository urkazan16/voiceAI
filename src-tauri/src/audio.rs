use crate::error::{LfError, LfResult};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioDevice {
    pub name: String,
    pub is_default: bool,
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

pub fn record_until_stop(stop: Arc<AtomicBool>, collected: Arc<Mutex<Vec<f32>>>) -> LfResult<()> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| LfError::DeviceUnavailable("no input device".into()))?;
    let config = device
        .default_input_config()
        .map_err(|e| LfError::DeviceUnavailable(e.to_string()))?;
    let err_fn = |err| eprintln!("audio stream error: {err}");
    let writer = collected.clone();
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
            .map_err(|e| LfError::DeviceUnavailable(e.to_string()))?,
        _ => {
            return Err(LfError::DeviceUnavailable(
                "unsupported sample format".into(),
            ))
        }
    };
    stream
        .play()
        .map_err(|e| LfError::DeviceUnavailable(e.to_string()))?;
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    drop(stream);
    Ok(())
}
