use crate::{
    microphone::{self, Microphone},
    model::{Fault, Input, Result, Sound},
    store::{Store, read_bounded},
};
use rodio::{
    Decoder, OutputStream, OutputStreamBuilder, Sink, Source,
    cpal::{
        self,
        traits::{DeviceTrait, HostTrait},
    },
};
use std::{
    collections::VecDeque,
    io::Cursor,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const MAX_BYTES: u64 = 24 * 1024 * 1024;
const MAX_SECONDS: usize = 30;
const MAX_VOICES: usize = 4;
const CACHE_SIZE: usize = 4;

#[derive(Clone)]
pub struct Clip {
    samples: Arc<[f32]>,
    rate: u32,
    channels: u16,
}

pub fn load_file(path: &Path) -> Result<(Vec<u8>, Clip)> {
    let bytes = read_bounded(path, MAX_BYTES)?;
    let clip = decode(&bytes)?;
    Ok((bytes, clip))
}

fn decode(bytes: &[u8]) -> Result<Clip> {
    let decoder = Decoder::try_from(Cursor::new(bytes.to_vec())).map_err(|_| {
        Fault::new(
            "decode",
            "El archivo WAV o MP3 es ilegible o usa un formato incompatible.",
        )
    })?;
    let channels = decoder.channels();
    let rate = decoder.sample_rate();
    if !(1..=2).contains(&channels) || !(8000..=96000).contains(&rate) {
        return Err(Fault::new(
            "format",
            "Usa audio mono o estéreo de 8 a 96 kHz.",
        ));
    }
    let limit = MAX_SECONDS * rate as usize * channels as usize;
    let samples: Vec<_> = decoder
        .take(limit + 1)
        .map(|sample| {
            if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        })
        .collect();
    if samples.is_empty()
        || samples.len() > limit
        || !samples.len().is_multiple_of(channels as usize)
    {
        return Err(Fault::new(
            "duration",
            "El clip debe contener audio válido y durar hasta 30 segundos.",
        ));
    }
    Ok(Clip {
        samples: samples.into(),
        rate,
        channels,
    })
}

struct Playback {
    clip: Clip,
    position: usize,
    repeat: bool,
}

impl Iterator for Playback {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.repeat && self.position == self.clip.samples.len() {
            self.position = 0;
        }
        let sample = *self.clip.samples.get(self.position)?;
        self.position += 1;
        Some(sample)
    }
}

impl Source for Playback {
    fn current_span_len(&self) -> Option<usize> {
        if self.repeat {
            None
        } else {
            Some(self.clip.samples.len() - self.position)
        }
    }
    fn channels(&self) -> u16 {
        self.clip.channels
    }
    fn sample_rate(&self) -> u32 {
        self.clip.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        if self.repeat {
            None
        } else {
            Some(Duration::from_secs_f64(
                self.clip.samples.len() as f64
                    / f64::from(self.clip.rate)
                    / f64::from(self.clip.channels),
            ))
        }
    }
}

pub fn outputs() -> Result<Vec<String>> {
    let host = cpal::default_host();
    let devices = host.output_devices().map_err(|_| {
        Fault::new(
            "audio_devices",
            "No se pudieron enumerar las salidas de audio.",
        )
    })?;
    let mut names: Vec<_> = devices.filter_map(|device| device.name().ok()).collect();
    names.sort_unstable();
    Ok(names)
}

struct Voice {
    sink: Sink,
    owner: Option<Input>,
}

struct Route {
    stream: OutputStream,
    failed: Arc<AtomicBool>,
    voices: Vec<Voice>,
}

impl Route {
    fn open(name: &str) -> Result<Self> {
        let host = cpal::default_host();
        let mut matches = host
            .output_devices()
            .map_err(|_| {
                Fault::new(
                    "audio_devices",
                    "No se pudieron abrir las salidas de audio.",
                )
            })?
            .filter(|device| device.name().is_ok_and(|label| label == name));
        let device = matches.next().ok_or_else(|| {
            Fault::new(
                "audio_missing",
                "La salida seleccionada no está disponible. Comprueba su conexión.",
            )
        })?;
        if matches.next().is_some() {
            return Err(Fault::new(
                "audio_ambiguous",
                "Hay salidas con el mismo nombre. Renómbralas en Windows para distinguirlas.",
            ));
        }
        let failed = Arc::new(AtomicBool::new(false));
        let callback = Arc::clone(&failed);
        let stream = OutputStreamBuilder::from_device(device)
            .and_then(|builder| builder.with_error_callback(move |_| callback.store(true, Ordering::Relaxed)).open_stream())
            .map_err(|_| Fault::new("audio_open", "No se pudo abrir la salida seleccionada. Comprueba el dispositivo y su formato en Windows."))?;
        Ok(Self {
            stream,
            failed,
            voices: vec![],
        })
    }
}

#[derive(Default)]
pub struct Audio {
    preview: Option<Route>,
    destination: Option<Route>,
    microphone: Option<(Microphone, Sink)>,
    cache: VecDeque<(String, Clip)>,
}

impl Audio {
    pub fn start_voice(&mut self, input: &str, output: &str, level: f32) -> Result<()> {
        self.stop_voice();
        if !microphone::is_virtual(output) {
            return Err(Fault::new(
                "mic_route",
                "Elige la salida virtual de MPK Deck para mezclar el micrófono.",
            ));
        }
        if self
            .destination
            .as_ref()
            .is_some_and(|route| route.failed.load(Ordering::Relaxed))
        {
            self.destination = None;
        }
        if self.destination.is_none() {
            self.destination = Some(Route::open(output)?);
        }
        let (capture, source) = Microphone::open(input)?;
        let route = self.destination.as_ref().expect("voice output");
        let sink = Sink::connect_new(route.stream.mixer());
        // Capture and all clips share a full-scale peak budget.
        sink.set_volume(level * 0.5);
        sink.append(source);
        self.microphone = Some((capture, sink));
        Ok(())
    }

    pub fn stop_voice(&mut self) {
        if let Some((_, sink)) = self.microphone.take() {
            sink.stop();
        }
    }

    pub fn voice_active(&self) -> bool {
        self.microphone.is_some()
    }

    pub fn remember(&mut self, id: String, clip: Clip) {
        self.cache.retain(|(key, _)| *key != id);
        self.cache.push_front((id, clip));
        self.cache.truncate(CACHE_SIZE);
    }

    pub fn play(
        &mut self,
        store: &Store,
        sound: &Sound,
        name: Option<&str>,
        preview: bool,
        repeat: bool,
        owner: Option<Input>,
    ) -> Result<()> {
        let name = name.ok_or_else(|| {
            Fault::new(
                "audio_unset",
                if preview {
                    "Elige una salida de preescucha en Conexiones."
                } else {
                    "Elige una salida para Discord en Conexiones. Puedes usar la preescucha local."
                },
            )
        })?;
        if preview && microphone::is_virtual(name) {
            return Err(Fault::new(
                "preview_route",
                "Elige auriculares o altavoces para la preescucha.",
            ));
        }
        if !preview
            && self
                .destination
                .as_ref()
                .is_some_and(|route| route.failed.load(Ordering::Relaxed))
        {
            self.stop_voice();
        }
        let path = store.sound(&sound.id);
        if !path.is_file() {
            return Err(Fault::new(
                "sound_missing",
                "Falta el archivo del sonido. Vuelve a importarlo.",
            ));
        }
        let clip = match self.cache.iter().position(|(key, _)| *key == sound.id) {
            Some(index) => {
                let entry = self.cache.remove(index).expect("cache index");
                let clip = entry.1.clone();
                self.cache.push_front(entry);
                clip
            }
            None => {
                let (_, clip) = load_file(&path)?;
                self.remember(sound.id.clone(), clip.clone());
                clip
            }
        };
        let route = if preview {
            &mut self.preview
        } else {
            &mut self.destination
        };
        if route
            .as_ref()
            .is_some_and(|route| route.failed.load(Ordering::Relaxed))
        {
            *route = None;
        }
        if route.is_none() {
            *route = Some(Route::open(name)?);
        }
        let route = route.as_mut().expect("audio route");
        route.voices.retain(|voice| !voice.sink.empty());
        if route.voices.len() == MAX_VOICES {
            route.voices.remove(0).sink.stop();
        }
        let sink = Sink::connect_new(route.stream.mixer());
        let clip_budget = if microphone::is_virtual(name) {
            0.5
        } else {
            1.0
        };
        sink.set_volume(sound.level * clip_budget / MAX_VOICES as f32);
        sink.append(Playback {
            clip,
            position: 0,
            repeat,
        });
        route.voices.push(Voice { sink, owner });
        Ok(())
    }

    pub fn stop_owner(&mut self, owner: Input) -> bool {
        let mut stopped = false;
        for route in [&mut self.preview, &mut self.destination]
            .into_iter()
            .flatten()
        {
            route.voices.retain(|voice| {
                let stop = voice.owner == Some(owner) && !voice.sink.empty();
                if stop {
                    voice.sink.stop();
                    stopped = true;
                }
                !stop && !voice.sink.empty()
            });
        }
        stopped
    }

    pub fn stop_bound(&mut self) {
        for route in [&mut self.preview, &mut self.destination]
            .into_iter()
            .flatten()
        {
            route.voices.retain(|voice| {
                if voice.owner.is_some() {
                    voice.sink.stop();
                    false
                } else {
                    true
                }
            });
        }
    }

    pub fn stop(&mut self) {
        for route in [&mut self.preview, &mut self.destination]
            .into_iter()
            .flatten()
        {
            for voice in route.voices.drain(..) {
                voice.sink.stop();
            }
        }
    }

    pub fn reset_outputs(&mut self) {
        self.stop_voice();
        self.stop();
        self.preview = None;
        self.destination = None;
    }

    pub fn check(&mut self) -> Option<Fault> {
        if self
            .microphone
            .as_ref()
            .is_some_and(|(capture, _)| capture.failed())
        {
            self.stop_voice();
            return Some(Fault::new(
                "mic_disconnected",
                "La captura se interrumpió. Comprueba el micrófono y actívalo de nuevo.",
            ));
        }
        if self
            .destination
            .as_ref()
            .is_some_and(|route| route.failed.load(Ordering::Relaxed))
        {
            self.stop_voice();
        }
        let mut failure = false;
        for route in [&mut self.preview, &mut self.destination] {
            if route
                .as_ref()
                .is_some_and(|route| route.failed.load(Ordering::Relaxed))
            {
                *route = None;
                failure = true;
            }
        }
        failure.then(|| Fault::new("audio_disconnected", "Se interrumpió la salida de audio. Comprueba el dispositivo y vuelve a reproducir."))
    }
}
