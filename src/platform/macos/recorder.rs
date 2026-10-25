//! Microphone capture with cpal.

use crate::audio;
use crate::facade::{Audio, FacadeError, Result};
use crate::platform::bridge::Signal;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
use std::sync::mpsc;

/// An open microphone stream. The audio callback sends chunks over a channel, so no
/// buffer is shared between threads. Dropping it closes the microphone, so it is
/// released on every exit path, including a panic.
pub struct Recording {
    stream: Stream,
    chunks: mpsc::Receiver<Vec<f32>>,
    channels: u16,
    sample_rate: u32,
}

impl Recording {
    /// Opens the default input device and starts capturing. If the device goes away
    /// mid-recording, [`Signal::MicrophoneLost`] is sent on `signals`.
    pub fn start(signals: &mpsc::Sender<Signal>) -> Result<Self> {
        let device = cpal::default_host()
            .default_input_device()
            .ok_or_else(|| FacadeError::Microphone("no microphone was found".into()))?;
        let supported = device.default_input_config().map_err(microphone)?;
        let format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let (sender, chunks) = mpsc::channel();
        let stream = match format {
            SampleFormat::F32 => open::<f32>(&device, &config, sender, signals.clone()),
            SampleFormat::I16 => open::<i16>(&device, &config, sender, signals.clone()),
            SampleFormat::I32 => open::<i32>(&device, &config, sender, signals.clone()),
            SampleFormat::U16 => open::<u16>(&device, &config, sender, signals.clone()),
            other => Err(FacadeError::Microphone(format!(
                "unsupported sample format {other}"
            ))),
        }?;
        stream.play().map_err(microphone)?;
        Ok(Self {
            stream,
            chunks,
            channels: config.channels,
            sample_rate: config.sample_rate,
        })
    }

    /// Closes the microphone and returns everything captured, ready for Whisper.
    pub fn stop(self) -> Audio {
        drop(self.stream);
        let samples: Vec<f32> = self.chunks.try_iter().flatten().collect();
        audio::prepare(&samples, self.channels, self.sample_rate)
    }
}

fn open<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    sender: mpsc::Sender<Vec<f32>>,
    signals: mpsc::Sender<Signal>,
) -> Result<Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device
        .build_input_stream::<T, _, _>(
            *config,
            move |data: &[T], _| {
                let _ = sender.send(
                    data.iter()
                        .map(|sample| sample.to_sample::<f32>())
                        .collect(),
                );
            },
            move |error| {
                tracing::warn!(kind = ?error.kind(), "microphone error");
                if error.kind() == ErrorKind::DeviceNotAvailable {
                    let _ = signals.send(Signal::MicrophoneLost);
                }
            },
            None,
        )
        .map_err(microphone)
}

fn microphone(error: impl std::fmt::Display) -> FacadeError {
    FacadeError::Microphone(error.to_string())
}
