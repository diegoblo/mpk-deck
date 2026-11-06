use crate::{
    audio::{self, Audio, Clip},
    microphone,
    midi::{Message, Midi, Port},
    model::{self, Action, Config, Fault, Input, Kind, Mode, Result, Sound, Step, Target},
    store::{self, ProfileFile, Store},
    windows_audio::{self, App, Com, Mixer},
};
use serde::Serialize;
use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

const DEVICES_INTERVAL: Duration = Duration::from_millis(1500);
const CONTEXT_INTERVAL: Duration = Duration::from_millis(250);
const PUBLISH_INTERVAL: Duration = Duration::from_millis(100);
const LEARN_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Serialize)]
pub struct Snapshot {
    epoch: u64,
    config: Config,
    ports: Vec<Port>,
    midi_connected: bool,
    midi_error: Option<Fault>,
    outputs: Vec<String>,
    microphones: Vec<String>,
    virtual_output: Option<String>,
    voice_active: bool,
    apps: Vec<App>,
    mixer_error: Option<Fault>,
    active_profile: String,
    profile_reason: String,
    monitoring: bool,
    log: VecDeque<Log>,
    dropped: u32,
    learning: bool,
    learned: Option<Input>,
    capturing: bool,
    captured_app: Option<String>,
    macro_running: bool,
    activity: Option<String>,
    error: Option<Fault>,
}

#[derive(Clone, Serialize)]
struct Log {
    elapsed_ms: u64,
    message: Message,
    outcome: String,
}

pub enum Request {
    Snapshot,
    Save(Config),
    AddSound {
        name: String,
        bytes: Vec<u8>,
        clip: Clip,
    },
    ImportProfile(PathBuf),
    ExportProfile {
        path: PathBuf,
        id: String,
    },
    Play {
        id: String,
        preview: bool,
        repeat: bool,
    },
    Stop,
    Voice(bool),
    Mix {
        app: String,
        volume: Option<f32>,
        mute: Option<bool>,
    },
    Monitor(bool),
    Clear,
    Learn,
    CancelLearn,
    CaptureApp,
    RunMacro {
        profile: String,
        id: String,
    },
    CancelMacro,
}

pub enum Event {
    Midi(u64, Message),
    MidiRemoved(String),
    Command(Box<Request>, oneshot::Sender<Result<Snapshot>>),
    Wake,
}

pub struct Engine {
    tx: SyncSender<Event>,
    quit: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Engine {
    pub fn start(app: AppHandle, store: Store) -> Result<Self> {
        let (tx, rx) = mpsc::sync_channel(256);
        let midi_tx = tx.clone();
        let quit = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&quit);
        let thread = thread::Builder::new()
            .name("mpk-deck-engine".into())
            .spawn(move || {
                let com = Com::init();
                let dropped = Arc::new(AtomicU32::new(0));
                let mut worker = Worker::new(
                    store,
                    Midi::new(midi_tx, Arc::clone(&dropped)),
                    com.as_ref().err().cloned(),
                );
                worker.run(app, rx, &stop, &dropped);
                // Release device and session handles before COM is uninitialized.
                drop(worker);
                drop(com);
            })
            .map_err(|_| Fault::new("startup", "No se pudo iniciar el motor de MPK Deck."))?;
        Ok(Self {
            tx,
            quit,
            thread: Mutex::new(Some(thread)),
        })
    }

    pub async fn request(&self, command: Request) -> Result<Snapshot> {
        let (reply, receive) = oneshot::channel();
        self.tx
            .try_send(Event::Command(Box::new(command), reply))
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => {
                    Fault::new("busy", "El motor está ocupado. Vuelve a intentarlo.")
                }
                mpsc::TrySendError::Disconnected(_) => {
                    Fault::new("engine", "El motor se ha detenido. Reinicia MPK Deck.")
                }
            })?;
        receive
            .await
            .map_err(|_| Fault::new("engine", "El motor se ha detenido. Reinicia MPK Deck."))?
    }

    pub fn close(&self) {
        self.quit.store(true, Ordering::Release);
        let _ = self.tx.try_send(Event::Wake);
        if let Ok(mut thread) = self.thread.lock()
            && let Some(thread) = thread.take()
        {
            let _ = thread.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.close();
    }
}

struct PendingMacro {
    name: String,
    foreground: Option<String>,
    steps: VecDeque<Step>,
    next_at: Instant,
}

struct Worker {
    store: Store,
    view: Snapshot,
    midi: Midi,
    audio: Audio,
    mixer: Mixer,
    com_error: Option<Fault>,
    held: HashSet<Input>,
    foreground: Option<String>,
    pending_profile: Option<(String, Instant)>,
    sequence: Option<PendingMacro>,
    learn_until: Option<Instant>,
    capture_at: Option<Instant>,
    started: Instant,
    dirty: bool,
}

impl Worker {
    fn new(store: Store, midi: Midi, com_error: Option<Fault>) -> Self {
        let (config, error) = store.load();
        let active_profile = config.manual_profile.clone();
        Self {
            store,
            midi,
            com_error: com_error.clone(),
            audio: Audio::default(),
            mixer: Mixer::default(),
            held: HashSet::new(),
            foreground: None,
            pending_profile: None,
            sequence: None,
            learn_until: None,
            capture_at: None,
            started: Instant::now(),
            dirty: true,
            view: Snapshot {
                epoch: 0,
                config,
                active_profile,
                profile_reason: "Perfil de inicio".into(),
                error,
                ports: vec![],
                midi_connected: false,
                midi_error: None,
                outputs: vec![],
                microphones: vec![],
                virtual_output: None,
                voice_active: false,
                apps: vec![],
                mixer_error: com_error,
                monitoring: false,
                log: VecDeque::new(),
                dropped: 0,
                learning: false,
                learned: None,
                capturing: false,
                captured_app: None,
                macro_running: false,
                activity: None,
            },
        }
    }

    fn run(&mut self, app: AppHandle, rx: Receiver<Event>, quit: &AtomicBool, dropped: &AtomicU32) {
        let mut devices_at = Instant::now();
        let mut context_at = devices_at;
        let mut publish_at = devices_at;
        while !quit.load(Ordering::Acquire) {
            let now = Instant::now();
            if now >= devices_at {
                devices_at = now + DEVICES_INTERVAL;
                self.refresh();
                if let Some(error) = self.audio.check() {
                    self.error(error);
                }
                self.update_voice_status();
            }
            if now >= context_at {
                context_at = now + CONTEXT_INTERVAL;
                if self.view.config.automatic || self.sequence.is_some() {
                    self.foreground = windows_audio::foreground();
                }
                self.update_profile(now);
            }
            if self
                .sequence
                .as_ref()
                .is_some_and(|sequence| sequence.next_at <= now)
            {
                self.step_macro(now);
            }
            if self.learn_until.is_some_and(|end| end <= now) {
                self.learn_until = None;
                self.view.learning = false;
                self.error(Fault::new(
                    "learn_timeout",
                    "No se recibió un control MIDI. Comprueba la conexión y reintenta.",
                ));
            }
            if self.capture_at.is_some_and(|at| at <= now) {
                self.capture_at = None;
                self.view.capturing = false;
                self.view.captured_app = windows_audio::foreground();
                self.view.activity = self
                    .view
                    .captured_app
                    .as_deref()
                    .map(|app| format!("Aplicación capturada · {}", model::app_name(app)));
                if self.view.captured_app.is_none() {
                    self.error(Fault::new("foreground", "No se identificó otra aplicación. Reintenta y cambia de ventana antes de cuatro segundos."));
                }
                self.dirty = true;
            }
            if now >= publish_at {
                publish_at = now + PUBLISH_INTERVAL;
                let count = dropped.load(Ordering::Relaxed);
                if count != self.view.dropped {
                    self.view.dropped = count;
                    self.held.clear();
                    self.audio.stop_bound();
                    self.error(Fault::new(
                        "midi_overflow",
                        "Se descartaron eventos MIDI. Se soltaron los controles retenidos.",
                    ));
                }
                if self.dirty {
                    self.view.epoch += 1;
                    let _ = app.emit_to("main", "deck-state", &self.view);
                    self.dirty = false;
                }
            }
            let mut wake = devices_at.min(context_at);
            if self.dirty {
                wake = wake.min(publish_at);
            }
            for deadline in [
                self.learn_until,
                self.capture_at,
                self.sequence.as_ref().map(|sequence| sequence.next_at),
            ]
            .into_iter()
            .flatten()
            {
                wake = wake.min(deadline);
            }
            match rx.recv_timeout(wake.saturating_duration_since(Instant::now())) {
                Ok(Event::Midi(generation, message)) if generation == self.midi.generation => {
                    self.handle_midi(message)
                }
                Ok(Event::MidiRemoved(id)) if self.midi.removed(&id) => {
                    self.held.clear();
                    self.audio.stop_bound();
                    self.cancel_macro();
                    self.refresh_midi();
                }
                Ok(Event::Command(command, reply)) => {
                    let result = self.command(*command).map(|()| {
                        self.view.epoch += 1;
                        self.view.clone()
                    });
                    let _ = reply.send(result);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                _ => {}
            }
        }
    }

    fn error(&mut self, error: Fault) {
        self.view.error = Some(error);
        self.dirty = true;
    }

    fn refresh_midi(&mut self) {
        let previous = self.midi.generation;
        match self.midi.refresh(self.view.config.midi_port.as_deref()) {
            Ok((ports, error)) => {
                self.dirty |= self.view.ports != ports || self.view.midi_error != error;
                self.view.ports = ports;
                self.view.midi_error = error;
            }
            Err(error) => {
                self.midi.disconnect();
                self.dirty |= self.view.midi_error.as_ref() != Some(&error);
                self.view.midi_error = Some(error);
            }
        }
        let connected = self.midi.connected();
        self.dirty |= self.view.midi_connected != connected;
        self.view.midi_connected = connected;
        if previous != self.midi.generation || !connected {
            self.held.clear();
            self.audio.stop_bound();
            if previous != self.midi.generation && !connected {
                self.cancel_macro();
            }
        }
    }

    fn refresh(&mut self) {
        self.refresh_midi();
        match audio::outputs() {
            Ok(outputs) => {
                self.dirty |= self.view.outputs != outputs;
                self.view.outputs = outputs;
            }
            Err(error) => self.error(error),
        }
        let inputs = match microphone::inputs() {
            Ok(inputs) => inputs,
            Err(error) => {
                self.error(error);
                vec![]
            }
        };
        let output = microphone::virtual_output(&self.view.outputs, &inputs);
        self.dirty |= self.view.virtual_output != output;
        self.view.virtual_output = output;
        let physical: Vec<_> = inputs
            .into_iter()
            .filter(|name| !microphone::is_virtual(name))
            .collect();
        if self.audio.voice_active()
            && (self.view.virtual_output.as_ref() != self.view.config.destination.as_ref()
                || self
                    .view
                    .config
                    .voice
                    .microphone
                    .as_ref()
                    .is_none_or(|name| physical.iter().filter(|input| *input == name).count() != 1))
        {
            self.audio.stop_voice();
            self.error(Fault::new("mic_disconnected", "Un dispositivo de la mezcla se desconectó. Comprueba la conexión y activa el micrófono de nuevo."));
        }
        self.dirty |= self.view.microphones != physical;
        self.view.microphones = physical;
        if self.com_error.is_none() {
            let error = self.mixer.refresh().err();
            self.dirty |= self.view.mixer_error != error;
            self.view.mixer_error = error;
            self.sync_mix();
        }
    }

    fn update_voice_status(&mut self) {
        let active = self.audio.voice_active();
        self.dirty |= self.view.voice_active != active;
        self.view.voice_active = active;
    }

    fn set_voice(&mut self, enabled: bool) -> Result<()> {
        if enabled && !self.audio.voice_active() {
            let config = &self.view.config;
            let output = self.view.virtual_output.as_deref().ok_or_else(|| {
                Fault::new(
                    "virtual_audio_missing",
                    "Windows aún no detecta el controlador de audio virtual de MPK Deck.",
                )
            })?;
            if config.destination.as_deref() != Some(output) {
                return Err(model::invalid(
                    "Guarda la salida virtual de MPK Deck como salida para Discord.",
                ));
            }
            let input = config.voice.microphone.as_deref().ok_or_else(|| {
                model::invalid("Elige y guarda tu micrófono físico en Conexiones.")
            })?;
            self.audio.start_voice(input, output, config.voice.level)?;
            self.view.error = None;
            self.view.activity = Some("Micrófono enviado a la mezcla de Discord".into());
        } else if !enabled && self.audio.voice_active() {
            self.audio.stop_voice();
            self.view.activity = Some("Micrófono desactivado".into());
        }
        self.update_voice_status();
        Ok(())
    }

    fn sync_mix(&mut self) {
        let apps = self.mixer.snapshot();
        self.dirty |= self.view.apps != apps;
        self.view.apps = apps;
    }

    fn update_profile(&mut self, now: Instant) {
        let (id, reason) = self
            .view
            .config
            .preferred_profile(self.foreground.as_deref(), self.view.midi_connected);
        if id == self.view.active_profile {
            self.pending_profile = None;
            self.dirty |= self.view.profile_reason != reason;
            self.view.profile_reason = reason;
            return;
        }
        let ready = !self.view.config.automatic
            || !self
                .view
                .config
                .profiles
                .iter()
                .any(|profile| profile.id == self.view.active_profile)
            || self
                .pending_profile
                .as_ref()
                .is_some_and(|(pending, since)| {
                    pending == id
                        && now.duration_since(*since)
                            >= Duration::from_millis(u64::from(self.view.config.delay_ms))
                });
        if ready {
            self.view.active_profile = id.into();
            self.view.profile_reason = reason;
            self.pending_profile = None;
            self.audio.stop_bound();
            self.cancel_macro();
            self.dirty = true;
        } else if self
            .pending_profile
            .as_ref()
            .is_none_or(|(pending, _)| pending != id)
        {
            self.pending_profile = Some((id.into(), now));
        }
    }

    fn commit(&mut self, mut config: Config) -> Result<()> {
        if config.revision != self.view.config.revision {
            return Err(Fault::new(
                "conflict",
                "La configuración cambió mientras editabas. Recarga los cambios guardados.",
            ));
        }
        config.validate()?;
        if config
            .preview
            .as_deref()
            .is_some_and(microphone::is_virtual)
        {
            return Err(model::invalid("Usa una salida local para la preescucha."));
        }
        config.revision = config
            .revision
            .checked_add(1)
            .ok_or_else(|| model::invalid("Revisión fuera de rango."))?;
        self.store.save(&config)?;
        let previous = &self.view.config;
        let midi_changed = previous.midi_port != config.midi_port;
        let route_changed =
            previous.preview != config.preview || previous.destination != config.destination;
        let voice_changed = previous.voice != config.voice;
        let mappings_changed = previous.profiles != config.profiles;
        let removed_sounds = previous
            .sounds
            .iter()
            .any(|sound| !config.sounds.iter().any(|item| item.id == sound.id));
        let cancel = !config.macros_enabled || mappings_changed;
        self.view.config = config;
        self.view.error = None;
        if route_changed {
            self.audio.reset_outputs();
        } else if voice_changed {
            self.audio.stop_voice();
        }
        self.update_voice_status();
        if mappings_changed || midi_changed {
            self.audio.stop_bound();
        }
        if removed_sounds {
            self.audio.stop();
        }
        if cancel {
            self.cancel_macro();
        }
        if midi_changed {
            self.refresh_midi();
        }
        self.pending_profile = None;
        if self.view.config.automatic {
            self.foreground = windows_audio::foreground();
        }
        self.update_profile(Instant::now());
        self.dirty = true;
        Ok(())
    }

    fn play(&mut self, id: &str, preview: bool, repeat: bool, owner: Option<Input>) -> Result<()> {
        let config = &self.view.config;
        let sound = config
            .sounds
            .iter()
            .find(|sound| sound.id == id)
            .ok_or_else(|| model::invalid("Selecciona un sonido existente."))?;
        let route = if preview {
            &config.preview
        } else {
            &config.destination
        };
        self.audio
            .play(&self.store, sound, route.as_deref(), preview, repeat, owner)?;
        self.view.activity = Some(format!(
            "{} · {}",
            if preview {
                "Preescucha"
            } else {
                "Salida Discord"
            },
            sound.name
        ));
        self.dirty = true;
        Ok(())
    }

    fn action(&mut self, action: Action, value: u8, owner: Option<Input>) -> Result<String> {
        let result = match action {
            Action::Play {
                sound,
                repeat,
                preview,
            } => {
                self.play(&sound, preview, repeat, owner)?;
                self.view.activity.clone().unwrap_or_default()
            }
            Action::Stop => {
                self.audio.stop();
                "Audio detenido".into()
            }
            Action::Volume { app, min, max } => {
                if let Some(error) = &self.com_error {
                    return Err(error.clone());
                }
                let level = min + (max - min) * f32::from(value) / 127.0;
                self.mixer.set(&app, Some(level), None)?;
                self.sync_mix();
                format!(
                    "Volumen · {} · {:.0} %",
                    model::app_name(&app),
                    level * 100.0
                )
            }
            Action::MasterVolume { min, max } => {
                if let Some(error) = &self.com_error {
                    return Err(error.clone());
                }
                let level = min + (max - min) * f32::from(value) / 127.0;
                windows_audio::set_master_volume(level)?;
                format!("Volumen general · {:.0} %", level * 100.0)
            }
            Action::Mute { app } => {
                if let Some(error) = &self.com_error {
                    return Err(error.clone());
                }
                self.mixer.toggle(&app)?;
                self.sync_mix();
                format!("Silencio alternado · {}", model::app_name(&app))
            }
            Action::Profile { profile } => {
                let mut config = self.view.config.clone();
                config.manual_profile = profile;
                config.automatic = false;
                self.commit(config)?;
                "Perfil fijado por una acción".into()
            }
        };
        self.view.activity = Some(result.clone());
        self.dirty = true;
        Ok(result)
    }

    fn handle_midi(&mut self, message: Message) {
        let input = message.input;
        let released = message.release || input.kind == Kind::Cc && message.value == 0;
        let rising = if released {
            self.held.remove(&input);
            false
        } else {
            self.held.insert(input)
        };
        let outcome = if self.view.learning {
            if !released {
                self.view.learned = Some(input);
                self.view.learning = false;
                self.learn_until = None;
                self.dirty = true;
                "Control aprendido".into()
            } else {
                "Esperando pulsación o giro".into()
            }
        } else {
            match self.dispatch(message, rising, released) {
                Ok(outcome) => outcome,
                Err(error) => {
                    let outcome = error.message.clone();
                    self.error(error);
                    outcome
                }
            }
        };
        if self.view.monitoring {
            self.view.log.push_front(Log {
                elapsed_ms: self.started.elapsed().as_millis() as u64,
                message,
                outcome,
            });
            self.view.log.truncate(80);
            self.dirty = true;
        }
    }

    fn dispatch(&mut self, message: Message, rising: bool, released: bool) -> Result<String> {
        let input = message.input;
        let profile = self.view.active_profile.clone();
        let binding = self
            .view
            .config
            .profiles
            .iter()
            .find(|item| item.id == profile)
            .and_then(|item| item.bindings.iter().find(|binding| binding.input == input))
            .cloned();
        let Some(binding) = binding else {
            return Ok("Sin asignación".into());
        };
        if binding.mode == Mode::Momentary && released {
            self.audio.stop_owner(input);
            return Ok("Fin de pulsación · sonido detenido".into());
        }
        if binding.mode != Mode::Continuous && !rising {
            return Ok("Sin disparo · liberación o repetición".into());
        }
        match binding.target {
            Target::Macro { id } => self.start_macro(&profile, &id),
            Target::Action { action } => {
                if binding.mode == Mode::Toggle && self.audio.stop_owner(input) {
                    return Ok("Sonido alternante detenido".into());
                }
                let owner = matches!(binding.mode, Mode::Toggle | Mode::Momentary).then_some(input);
                let value = if binding.mode == Mode::Continuous {
                    message.value
                } else {
                    127
                };
                self.action(action, value, owner)
            }
        }
    }

    fn start_macro(&mut self, profile: &str, id: &str) -> Result<String> {
        if !self.view.config.macros_enabled {
            return Err(Fault::new("macro_paused", "Las macros están en pausa."));
        }
        if self.sequence.is_some() {
            return Err(Fault::new(
                "macro_busy",
                "Ya hay una macro en ejecución. Cancélala o espera a que termine.",
            ));
        }
        if profile != self.view.active_profile {
            return Err(Fault::new(
                "macro_profile",
                "Activa el perfil de esta macro antes de ejecutarla.",
            ));
        }
        let sequence = self
            .view
            .config
            .profiles
            .iter()
            .find(|item| item.id == profile)
            .and_then(|item| item.macros.iter().find(|sequence| sequence.id == id))
            .ok_or_else(|| model::invalid("Selecciona una macro existente."))?;
        if !sequence.enabled {
            return Err(Fault::new("macro_disabled", "Esta macro está desactivada."));
        }
        self.foreground = windows_audio::foreground();
        if sequence
            .foreground
            .as_deref()
            .is_some_and(|expected| Some(expected) != self.foreground.as_deref())
        {
            return Err(Fault::new(
                "macro_context",
                "La aplicación requerida para esta macro no está en primer plano.",
            ));
        }
        let summary = format!(
            "Macro iniciada · {}{}",
            sequence.name,
            sequence
                .foreground
                .as_deref()
                .map(|app| format!(" · {}", model::app_name(app)))
                .unwrap_or_default()
        );
        let steps: VecDeque<_> = sequence.steps.clone().into();
        let next_at = Instant::now() + Duration::from_millis(u64::from(steps[0].delay_ms));
        self.sequence = Some(PendingMacro {
            name: sequence.name.clone(),
            foreground: sequence.foreground.clone(),
            steps,
            next_at,
        });
        self.view.macro_running = true;
        self.view.activity = Some(summary.clone());
        self.dirty = true;
        Ok(summary)
    }

    fn step_macro(&mut self, now: Instant) {
        let Some(mut sequence) = self.sequence.take() else {
            return;
        };
        if sequence
            .foreground
            .as_deref()
            .is_some_and(|expected| Some(expected) != windows_audio::foreground().as_deref())
        {
            self.cancel_macro();
            self.error(Fault::new(
                "macro_context",
                "Macro cancelada porque cambió la ventana en primer plano.",
            ));
            return;
        }
        let step = sequence.steps.pop_front().expect("pending macro step");
        let switches_profile = matches!(step.action, Action::Profile { .. });
        if let Err(error) = self.action(step.action, 127, None) {
            self.cancel_macro();
            self.error(error);
        } else if !switches_profile && let Some(next) = sequence.steps.front() {
            sequence.next_at = now + Duration::from_millis(u64::from(next.delay_ms));
            self.sequence = Some(sequence);
        } else {
            self.view.macro_running = false;
            self.view.activity = Some(format!("Macro finalizada · {}", sequence.name));
            self.dirty = true;
        }
    }

    fn cancel_macro(&mut self) {
        self.sequence = None;
        if self.view.macro_running {
            self.view.macro_running = false;
            self.view.activity = Some("Macro cancelada".into());
            self.dirty = true;
        }
    }

    fn command(&mut self, request: Request) -> Result<()> {
        match request {
            Request::Snapshot => return Ok(()),
            Request::Save(config) => self.commit(config)?,
            Request::AddSound { name, bytes, clip } => {
                let mut config = self.view.config.clone();
                let id = model::new_id();
                config.sounds.push(Sound {
                    id: id.clone(),
                    name,
                    level: 0.85,
                });
                config.validate()?;
                self.store.save_sound(&id, &bytes)?;
                if let Err(error) = self.commit(config) {
                    let _ = std::fs::remove_file(self.store.sound(&id));
                    return Err(error);
                }
                self.audio.remember(id, clip);
            }
            Request::ImportProfile(path) => {
                let file: ProfileFile = store::read_json(&path)?;
                if file.version != model::SCHEMA {
                    return Err(Fault::new(
                        "schema",
                        "La versión del perfil es incompatible.",
                    ));
                }
                let mut config = self.view.config.clone();
                let mut profile = file.profile;
                let old = std::mem::replace(&mut profile.id, model::new_id());
                let new = profile.id.clone();
                let mut ids = std::collections::HashMap::new();
                for sequence in &mut profile.macros {
                    let old = std::mem::replace(&mut sequence.id, model::new_id());
                    if ids.insert(old, sequence.id.clone()).is_some() {
                        return Err(model::invalid("El perfil contiene macros duplicadas."));
                    }
                }
                for binding in &mut profile.bindings {
                    if let Target::Macro { id } = &mut binding.target
                        && let Some(new_id) = ids.get(id)
                    {
                        *id = new_id.clone();
                    }
                }
                let actions = profile
                    .bindings
                    .iter_mut()
                    .filter_map(|binding| match &mut binding.target {
                        Target::Action { action } => Some(action),
                        Target::Macro { .. } => None,
                    })
                    .chain(profile.macros.iter_mut().flat_map(|sequence| {
                        sequence.steps.iter_mut().map(|step| &mut step.action)
                    }));
                for action in actions {
                    match action {
                        Action::Profile { profile } if *profile == old => *profile = new.clone(),
                        Action::Play { sound, .. }
                            if config.sounds.iter().any(|item| item.id == *sound)
                                && !self.store.sound(sound).is_file() =>
                        {
                            return Err(Fault::new(
                                "sound_missing",
                                "Falta un archivo de sonido referenciado en el perfil.",
                            ));
                        }
                        _ => {}
                    }
                }
                config.profiles.push(profile);
                self.commit(config)?;
                self.view.activity = Some("Perfil importado".into());
            }
            Request::ExportProfile { path, id } => {
                let profile = self
                    .view
                    .config
                    .profiles
                    .iter()
                    .find(|profile| profile.id == id)
                    .ok_or_else(|| model::invalid("Guarda el perfil antes de exportarlo."))?;
                store::export_profile(&path, profile.clone())?;
                self.view.activity = Some("Perfil exportado".into());
            }
            Request::Play {
                id,
                preview,
                repeat,
            } => self.play(&id, preview, repeat, None)?,
            Request::Stop => {
                self.audio.stop();
                self.cancel_macro();
                self.view.activity = Some("Audio detenido".into());
            }
            Request::Voice(enabled) => self.set_voice(enabled)?,
            Request::Mix { app, volume, mute } => {
                if !model::valid_app(&app)
                    || volume.is_some_and(|level| !model::valid_level(level))
                    || volume.is_some() == mute.is_some()
                {
                    return Err(model::invalid(
                        "Selecciona una aplicación y cambia el volumen o el silencio.",
                    ));
                }
                if let Some(error) = &self.com_error {
                    return Err(error.clone());
                }
                self.mixer.set(&app, volume, mute)?;
                self.sync_mix();
            }
            Request::Monitor(enabled) => {
                self.view.monitoring = enabled;
                if !enabled {
                    self.view.log.clear();
                }
            }
            Request::Clear => self.view.log.clear(),
            Request::Learn => {
                if !self.midi.connected() {
                    return Err(Fault::new(
                        "midi_missing",
                        "Conecta y guarda primero un puerto MIDI.",
                    ));
                }
                self.learn_until = Some(Instant::now() + LEARN_TIMEOUT);
                self.audio.stop_bound();
                self.view.learned = None;
                self.view.learning = true;
            }
            Request::CancelLearn => {
                self.learn_until = None;
                self.view.learning = false;
                self.view.learned = None;
            }
            Request::CaptureApp => {
                self.view.captured_app = None;
                self.view.capturing = true;
                self.capture_at = Some(Instant::now() + Duration::from_secs(4));
            }
            Request::RunMacro { profile, id } => {
                self.start_macro(&profile, &id)?;
            }
            Request::CancelMacro => self.cancel_macro(),
        }
        self.dirty = true;
        Ok(())
    }
}
