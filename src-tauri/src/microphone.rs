use crate::model::{Fault, Result};
use rodio::{
    Source,
    cpal::{
        self, FromSample, Sample, SampleFormat, SizedSample,
        traits::{DeviceTrait, HostTrait, StreamTrait},
    },
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::Duration,
};

// Keep the microphone in memory only. A stalled consumer cannot grow this queue.
const BLOCK: usize = 128;
const QUEUE_BLOCKS: usize = 12;
const VIRTUAL_ADAPTER: &str = "MPK Deck Virtual Audio";

pub fn is_virtual(name: &str) -> bool {
    name == VIRTUAL_ADAPTER
        || name.ends_with(" (MPK Deck Virtual Audio)")
        || matches!(name, "MPK Deck Voice Send" | "MPK Deck Microphone")
}

pub fn inputs() -> Result<Vec<String>> {
    let mut inputs: Vec<_> = cpal::default_host()
        .input_devices()
        .map_err(|_| {
            Fault::new(
                "mic_devices",
                "No se pudieron consultar las entradas de Windows.",
            )
        })?
        .filter_map(|device| device.name().ok())
        .collect();
    inputs.sort_unstable();
    Ok(inputs)
}

pub fn virtual_output(outputs: &[String], inputs: &[String]) -> Option<String> {
    let mut playback = outputs.iter().filter(|name| is_virtual(name));
    let output = playback.next()?;
    (playback.next().is_none() && inputs.iter().filter(|name| is_virtual(name)).count() == 1)
        .then(|| output.clone())
}

struct Producer {
    tx: SyncSender<[f32; BLOCK]>,
    block: [f32; BLOCK],
    len: usize,
    channels: usize,
}

impl Producer {
    fn capture<T>(&mut self, samples: &[T])
    where
        T: Sample,
        f32: FromSample<T>,
    {
        for frame in samples.chunks_exact(self.channels) {
            let sum: f32 = frame
                .iter()
                .map(|&sample| {
                    let value = f32::from_sample(sample);
                    if value.is_finite() {
                        value.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    }
                })
                .sum();
            self.block[self.len] = sum / self.channels as f32;
            self.len += 1;
            if self.len == BLOCK {
                let _ = self.tx.try_send(self.block);
                self.len = 0;
            }
        }
    }
}

pub struct MicSource {
    rx: Receiver<[f32; BLOCK]>,
    block: [f32; BLOCK],
    index: usize,
    rate: u32,
}

impl Iterator for MicSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.index == BLOCK {
            self.block = self.rx.try_recv().unwrap_or([0.0; BLOCK]);
            self.index = 0;
        }
        let sample = self.block[self.index];
        self.index += 1;
        Some(sample)
    }
}

impl Source for MicSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        1
    }
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

pub struct Microphone {
    _stream: cpal::Stream,
    failed: Arc<AtomicBool>,
}

impl Microphone {
    pub fn open(name: &str) -> Result<(Self, MicSource)> {
        if is_virtual(name) {
            return Err(Fault::new(
                "mic_loop",
                "Selecciona tu micrófono físico para evitar realimentación.",
            ));
        }
        let host = cpal::default_host();
        let mut matches = host
            .input_devices()
            .map_err(|_| {
                Fault::new(
                    "mic_devices",
                    "No se pudieron abrir las entradas de Windows.",
                )
            })?
            .filter(|device| device.name().is_ok_and(|label| label == name));
        let device = matches.next().ok_or_else(|| {
            Fault::new("mic_missing", "No se encuentra el micrófono seleccionado.")
        })?;
        if matches.next().is_some() {
            return Err(Fault::new(
                "mic_ambiguous",
                "Hay entradas con el mismo nombre. Renómbralas en Windows para distinguirlas.",
            ));
        }
        let config = device.default_input_config().map_err(|_| {
            Fault::new(
                "mic_format",
                "Windows no indicó un formato compatible para ese micrófono.",
            )
        })?;
        let channels = usize::from(config.channels());
        let rate = config.sample_rate().0;
        if !(1..=32).contains(&channels) || !(8000..=192000).contains(&rate) {
            return Err(Fault::new(
                "mic_format",
                "El formato de captura del micrófono no es compatible.",
            ));
        }
        let (tx, rx) = mpsc::sync_channel(QUEUE_BLOCKS);
        let producer = Producer {
            tx,
            block: [0.0; BLOCK],
            len: 0,
            channels,
        };
        let failed = Arc::new(AtomicBool::new(false));
        let stream = match config.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, &config.config(), producer, Arc::clone(&failed)),
            SampleFormat::I16 => build::<i16>(&device, &config.config(), producer, Arc::clone(&failed)),
            SampleFormat::I32 => build::<i32>(&device, &config.config(), producer, Arc::clone(&failed)),
            SampleFormat::U16 => build::<u16>(&device, &config.config(), producer, Arc::clone(&failed)),
            _ => return Err(Fault::new("mic_format", "El formato de captura del micrófono no es compatible.")),
        }.map_err(|_| Fault::new("mic_open", "No se pudo abrir el micrófono. Revisa la conexión y los permisos de micrófono de Windows."))?;
        stream
            .play()
            .map_err(|_| Fault::new("mic_open", "No se pudo iniciar la captura del micrófono."))?;
        Ok((
            Self {
                _stream: stream,
                failed,
            },
            MicSource {
                rx,
                block: [0.0; BLOCK],
                index: BLOCK,
                rate,
            },
        ))
    }

    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut producer: Producer,
    failed: Arc<AtomicBool>,
) -> std::result::Result<cpal::Stream, cpal::BuildStreamError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device.build_input_stream(
        config,
        move |samples: &[T], _| producer.capture(samples),
        move |_| failed.store(true, Ordering::Relaxed),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mic_downmixes_and_supplies_silence_on_underrun() {
        let (tx, rx) = mpsc::sync_channel(1);
        let mut producer = Producer {
            tx,
            block: [0.0; BLOCK],
            len: 0,
            channels: 2,
        };
        producer.capture(&[0.25, 0.75].repeat(BLOCK));
        producer.capture(&[1.0, 1.0].repeat(BLOCK));
        let mut source = MicSource {
            rx,
            block: [0.0; BLOCK],
            index: BLOCK,
            rate: 48_000,
        };
        assert!(source.by_ref().take(BLOCK).all(|value| value == 0.5));
        assert!(source.take(BLOCK).all(|value| value == 0.0));
    }

    #[test]
    fn requires_both_unique_virtual_endpoints() {
        let output = "Altavoces (MPK Deck Virtual Audio)".to_owned();
        let input = "Micrófono (MPK Deck Virtual Audio)".to_owned();
        assert_eq!(
            virtual_output(std::slice::from_ref(&output), std::slice::from_ref(&input)),
            Some(output.clone())
        );
        assert!(virtual_output(&[output.clone(), output], &[input]).is_none());
        assert!(virtual_output(&["MPK Deck Voice Send".into()], &["Headset".into()]).is_none());
    }

    #[test]
    #[ignore = "abre el micrófono físico de Windows sin grabar ni reproducir"]
    fn opens_real_microphone_and_receives_pcm() {
        let _com = crate::windows_audio::Com::init().unwrap();
        let device = cpal::default_host()
            .default_input_device()
            .expect("Configura un micrófono físico en Windows");
        let name = device.name().unwrap();
        let (capture, source) = Microphone::open(&name).unwrap();
        assert!(source.rx.recv_timeout(Duration::from_secs(3)).is_ok());
        assert!(!capture.failed());
        println!("WASAPI entregó muestras en memoria desde el micrófono físico.");
    }
}
