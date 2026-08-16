use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use std::sync::mpsc;

pub const AUDIO_QUEUE_CAPACITY: usize = 2_048;

pub struct SimpleAudio {
    _stream: cpal::Stream,
}

impl SimpleAudio {
    fn build_stream<T>(
        device: &cpal::Device,
        config: &StreamConfig,
        channels: usize,
        sample_receiver: mpsc::Receiver<(f32, f32)>,
    ) -> Result<cpal::Stream, String>
    where
        T: SizedSample + FromSample<f32>,
    {
        device
            .build_output_stream(
                config,
                move |data: &mut [T], _| {
                    for frame in data.chunks_mut(channels) {
                        let (left, right) = sample_receiver.try_recv().unwrap_or((0.0, 0.0));
                        let center = (left + right) * 0.5;
                        if frame.len() == 1 {
                            frame[0] = T::from_sample(center);
                        } else {
                            frame[0] = T::from_sample(left);
                            frame[1] = T::from_sample(right);
                            for sample in &mut frame[2..] {
                                *sample = T::from_sample(center);
                            }
                        }
                    }
                },
                |error| eprintln!("audio: output stream error: {error}"),
                None,
            )
            .map_err(|error| format!("could not create output stream: {error}"))
    }

    pub fn new() -> Result<(Self, mpsc::SyncSender<(f32, f32)>, u32), String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "no default output device is available".to_owned())?;
        let supported_config = device
            .default_output_config()
            .map_err(|error| format!("could not query the default output format: {error}"))?;
        let sample_rate = supported_config.sample_rate().0;
        let channels = usize::from(supported_config.channels());
        let sample_format = supported_config.sample_format();
        let config: StreamConfig = supported_config.into();
        let (sample_sender, sample_receiver) = mpsc::sync_channel(AUDIO_QUEUE_CAPACITY);

        let stream = match sample_format {
            SampleFormat::F32 => {
                Self::build_stream::<f32>(&device, &config, channels, sample_receiver)
            }
            SampleFormat::I16 => {
                Self::build_stream::<i16>(&device, &config, channels, sample_receiver)
            }
            SampleFormat::U16 => {
                Self::build_stream::<u16>(&device, &config, channels, sample_receiver)
            }
            SampleFormat::I32 => {
                Self::build_stream::<i32>(&device, &config, channels, sample_receiver)
            }
            SampleFormat::U32 => {
                Self::build_stream::<u32>(&device, &config, channels, sample_receiver)
            }
            SampleFormat::F64 => {
                Self::build_stream::<f64>(&device, &config, channels, sample_receiver)
            }
            format => return Err(format!("unsupported output sample format: {format}")),
        }?;

        stream
            .play()
            .map_err(|error| format!("could not start output stream: {error}"))?;
        Ok((Self { _stream: stream }, sample_sender, sample_rate))
    }
}
