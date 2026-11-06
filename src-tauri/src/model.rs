use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt};

pub const SCHEMA: u32 = 1;
pub const MAX_CONFIG: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fault {
    pub code: &'static str,
    pub message: String,
}

impl Fault {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Fault {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(out)
    }
}
impl std::error::Error for Fault {}

pub type Result<T> = std::result::Result<T, Fault>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub revision: u64,
    pub midi_port: Option<String>,
    pub preview: Option<String>,
    pub destination: Option<String>,
    #[serde(default)]
    pub voice: VoiceSettings,
    pub sounds: Vec<Sound>,
    pub profiles: Vec<Profile>,
    pub manual_profile: String,
    pub automatic: bool,
    pub macros_enabled: bool,
    pub delay_ms: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceSettings {
    pub microphone: Option<String>,
    pub level: f32,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            microphone: None,
            level: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sound {
    pub id: String,
    pub name: String,
    pub level: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub bindings: Vec<Binding>,
    pub macros: Vec<Macro>,
    pub rule: Option<Rule>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub app: String,
    pub priority: i32,
    pub requires_midi: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub input: Input,
    pub mode: Mode,
    pub target: Target,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<SurfaceControl>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SurfaceControl {
    PadA { index: u8 },
    PadB { index: u8 },
    Knob { index: u8 },
    Key { index: u8 },
    JoystickX,
    JoystickY,
    Pedal,
}

impl SurfaceControl {
    fn accepts(self, kind: Kind) -> bool {
        match self {
            Self::PadA { index } | Self::PadB { index } => (1..=8).contains(&index),
            Self::Knob { index } => (1..=8).contains(&index) && kind == Kind::Cc,
            Self::Key { index } => (1..=25).contains(&index) && kind == Kind::Note,
            Self::JoystickX | Self::JoystickY | Self::Pedal => kind == Kind::Cc,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Target {
    Action { action: Action },
    Macro { id: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Play {
        sound: String,
        repeat: bool,
        #[serde(default)]
        preview: bool,
    },
    Stop,
    Volume {
        app: String,
        min: f32,
        max: f32,
    },
    MasterVolume {
        min: f32,
        max: f32,
    },
    Mute {
        app: String,
    },
    Profile {
        profile: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Macro {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub foreground: Option<String>,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub delay_ms: u32,
    pub action: Action,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub kind: Kind,
    pub channel: u8,
    pub number: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Note,
    Cc,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Press,
    Momentary,
    Toggle,
    Continuous,
}

impl Default for Config {
    fn default() -> Self {
        let id = new_id();
        Self {
            version: SCHEMA,
            revision: 0,
            midi_port: None,
            preview: None,
            destination: None,
            voice: VoiceSettings::default(),
            sounds: vec![],
            profiles: vec![Profile {
                id: id.clone(),
                name: "Principal".into(),
                bindings: vec![],
                macros: vec![],
                rule: None,
            }],
            manual_profile: id,
            automatic: false,
            macros_enabled: true,
            delay_ms: 700,
        }
    }
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn invalid(message: impl Into<String>) -> Fault {
    Fault::new("validation", message)
}

fn bounded(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

pub fn valid_app(value: &str) -> bool {
    bounded(value, 1024)
        && std::path::Path::new(value).is_absolute()
        && value.to_ascii_lowercase().ends_with(".exe")
}

fn valid_id(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}

pub fn valid_level(value: f32) -> bool {
    (0.0..=1.0).contains(&value)
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.version != SCHEMA {
            return Err(Fault::new(
                "schema",
                "Versión de configuración incompatible.",
            ));
        }
        if !valid_level(self.voice.level)
            || self
                .voice
                .microphone
                .as_deref()
                .is_some_and(|name| !bounded(name, 1024))
        {
            return Err(invalid(
                "Selecciona una entrada válida y un nivel de micrófono entre 0 y 100 %.",
            ));
        }
        if !(200..=5000).contains(&self.delay_ms)
            || self.profiles.is_empty()
            || self.profiles.len() > 32
            || self.sounds.len() > 64
            || [&self.midi_port, &self.preview, &self.destination]
                .into_iter()
                .flatten()
                .any(|value| !bounded(value, 1024))
            || self.preview.is_some() && self.preview == self.destination
        {
            return Err(invalid(
                "Comprueba los límites de perfiles, sonidos, demora y dispositivos. Usa salidas distintas.",
            ));
        }
        let mut ids = HashSet::new();
        for sound in &self.sounds {
            if !valid_id(&sound.id)
                || !ids.insert(&sound.id)
                || !bounded(&sound.name, 120)
                || !valid_level(sound.level)
            {
                return Err(invalid("Hay un sonido inválido o duplicado."));
            }
        }
        if !self
            .profiles
            .iter()
            .any(|profile| profile.id == self.manual_profile)
        {
            return Err(invalid("Selecciona un perfil manual existente."));
        }
        for profile in &self.profiles {
            if !valid_id(&profile.id)
                || !ids.insert(&profile.id)
                || !bounded(&profile.name, 120)
                || profile.bindings.len() > 128
                || profile.macros.len() > 24
            {
                return Err(invalid("Hay un perfil inválido o duplicado."));
            }
            if let Some(rule) = &profile.rule
                && (!valid_app(&rule.app) || !(-100..=100).contains(&rule.priority))
            {
                return Err(invalid(
                    "Selecciona una aplicación para la regla y una prioridad entre −100 y 100.",
                ));
            }
            for sequence in &profile.macros {
                if !valid_id(&sequence.id)
                    || !ids.insert(&sequence.id)
                    || !bounded(&sequence.name, 120)
                    || sequence.steps.is_empty()
                    || sequence.steps.len() > 16
                    || sequence
                        .foreground
                        .as_deref()
                        .is_some_and(|app| !valid_app(app))
                    || sequence
                        .steps
                        .iter()
                        .map(|step| u64::from(step.delay_ms))
                        .sum::<u64>()
                        > 10_000
                {
                    return Err(invalid(
                        "Cada macro debe tener 1–16 pasos y como máximo 10 segundos de espera.",
                    ));
                }
                for (index, step) in sequence.steps.iter().enumerate() {
                    if matches!(step.action, Action::Profile { .. })
                        && index + 1 != sequence.steps.len()
                    {
                        return Err(invalid(
                            "Fijar un perfil debe ser el último paso de la macro.",
                        ));
                    }
                    self.validate_action(&step.action)?;
                }
            }
            let mut inputs = HashSet::new();
            let mut surface = HashSet::new();
            for binding in &profile.bindings {
                if binding.input.channel > 15
                    || binding.input.number > 127
                    || !inputs.insert(binding.input)
                {
                    return Err(invalid(
                        "Hay controles MIDI inválidos o asignados dos veces en el perfil.",
                    ));
                }
                if let Some(control) = binding.surface
                    && (!control.accepts(binding.input.kind) || !surface.insert(control))
                {
                    return Err(invalid(
                        "Hay posiciones del mapa visual inválidas o asignadas dos veces.",
                    ));
                }
                match &binding.target {
                    Target::Macro { id }
                        if binding.mode == Mode::Press
                            && profile.macros.iter().any(|sequence| sequence.id == *id) => {}
                    Target::Macro { .. } => {
                        return Err(invalid(
                            "Selecciona una macro existente y usa el modo Pulsación.",
                        ));
                    }
                    Target::Action { action } => {
                        self.validate_action(action)?;
                        match (binding.mode, action) {
                            (Mode::Press, _) => {}
                            (
                                Mode::Continuous,
                                Action::Volume { .. } | Action::MasterVolume { .. },
                            ) if binding.input.kind == Kind::Cc => {}
                            (Mode::Momentary | Mode::Toggle, Action::Play { .. }) => {}
                            _ => {
                                return Err(invalid(
                                    "Usa un mando CC para volumen continuo, o un sonido para modos momentáneo y alternante.",
                                ));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn validate_action(&self, action: &Action) -> Result<()> {
        let valid = match action {
            Action::Play { sound, .. } => self.sounds.iter().any(|item| item.id == *sound),
            Action::Stop => true,
            Action::Volume { app, min, max } => {
                valid_app(app) && valid_level(*min) && valid_level(*max) && min <= max
            }
            Action::MasterVolume { min, max } => {
                valid_level(*min) && valid_level(*max) && min <= max
            }
            Action::Mute { app } => valid_app(app),
            Action::Profile { profile } => self.profiles.iter().any(|item| item.id == *profile),
        };
        if valid {
            Ok(())
        } else {
            Err(invalid(
                "La acción apunta a un sonido, perfil o aplicación inválidos, o tiene niveles fuera de rango.",
            ))
        }
    }

    pub fn preferred_profile(&self, foreground: Option<&str>, connected: bool) -> (&str, String) {
        if self.automatic
            && let Some(foreground) = foreground
        {
            let mut best: Option<(&Profile, &Rule)> = None;
            for profile in &self.profiles {
                if let Some(rule) = &profile.rule
                    && (!rule.requires_midi || connected)
                    && rule.app == foreground
                    && best.is_none_or(|(_, selected)| selected.priority < rule.priority)
                {
                    best = Some((profile, rule));
                }
            }
            if let Some((profile, rule)) = best {
                return (
                    &profile.id,
                    format!(
                        "Regla de {} · prioridad {}",
                        app_name(&rule.app),
                        rule.priority
                    ),
                );
            }
        }
        let reason = if self.automatic {
            "Perfil de respaldo"
        } else {
            "Perfil fijado manualmente"
        };
        (&self.manual_profile, reason.into())
    }
}

pub fn app_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_settings_saved_before_voice_capture() {
        let old = Config::default();
        let mut data = serde_json::to_value(&old).unwrap();
        data.as_object_mut().unwrap().remove("voice");
        let restored: Config = serde_json::from_value(data).unwrap();
        assert_eq!(restored, old);
        assert!(restored.validate().is_ok());
    }

    #[test]
    fn automatic_profiles_respect_priority_order_and_connection() {
        let mut config = Config::default();
        let app = "c:\\apps\\discord.exe";
        let rule = Rule {
            app: app.into(),
            priority: 10,
            requires_midi: true,
        };
        for name in ["Discord", "Segundo"] {
            config.profiles.push(Profile {
                id: new_id(),
                name: name.into(),
                rule: Some(rule.clone()),
                bindings: vec![],
                macros: vec![],
            });
        }
        config.automatic = true;
        assert_eq!(
            config.preferred_profile(Some(app), true).0,
            config.profiles[1].id
        );
        assert_eq!(
            config.preferred_profile(Some(app), false).0,
            config.manual_profile
        );
        config.profiles[2].rule.as_mut().unwrap().priority += 1;
        assert_eq!(
            config.preferred_profile(Some(app), true).0,
            config.profiles[2].id
        );
        config.automatic = false;
        assert_eq!(
            config.preferred_profile(Some(app), true).0,
            config.manual_profile
        );
    }

    #[test]
    fn rejects_continuous_note_and_duplicate_controls() {
        let mut config = Config::default();
        let binding = Binding {
            input: Input {
                kind: Kind::Note,
                channel: 0,
                number: 60,
            },
            mode: Mode::Continuous,
            surface: None,
            target: Target::Action {
                action: Action::Stop,
            },
        };
        config.profiles[0].bindings.push(binding.clone());
        assert!(config.validate().is_err());
        config.profiles[0].bindings[0].mode = Mode::Press;
        assert!(config.validate().is_ok());
        config.profiles[0].bindings.push(binding);
        assert!(config.validate().is_err());
    }

    #[test]
    fn validates_continuous_master_volume_binding() {
        let mut config = Config::default();
        config.profiles[0].bindings.push(Binding {
            input: Input {
                kind: Kind::Cc,
                channel: 0,
                number: 70,
            },
            mode: Mode::Continuous,
            surface: None,
            target: Target::Action {
                action: Action::MasterVolume { min: 0.0, max: 0.7 },
            },
        });
        let serialized = serde_json::to_string(&config).unwrap();
        let mut loaded: Config = serde_json::from_str(&serialized).unwrap();
        assert_eq!(loaded, config);
        assert!(loaded.validate().is_ok());
        loaded.profiles[0].bindings[0].input.kind = Kind::Note;
        assert!(loaded.validate().is_err());
        for (min, max) in [(0.8, 0.2), (-0.1, 1.0), (0.0, 1.1)] {
            assert!(
                config
                    .validate_action(&Action::MasterVolume { min, max })
                    .is_err()
            );
        }
    }

    #[test]
    fn visual_positions_are_optional_unique_and_match_the_midi_kind() {
        let mut config = Config::default();
        let mut binding: Binding = serde_json::from_str(
            r#"{"input":{"kind":"cc","channel":0,"number":70},"mode":"press","target":{"kind":"action","action":{"kind":"stop"}}}"#,
        )
        .unwrap();
        assert_eq!(binding.surface, None);
        binding.surface = Some(SurfaceControl::Knob { index: 1 });
        config.profiles[0].bindings.push(binding.clone());
        let loaded: Config =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(loaded, config);
        assert!(loaded.validate().is_ok());

        binding.input.number = 71;
        config.profiles[0].bindings.push(binding);
        assert!(config.validate().is_err());
        let other = &mut config.profiles[0].bindings[1];
        other.surface = Some(SurfaceControl::Knob { index: 9 });
        assert!(config.validate().is_err());
        config.profiles[0].bindings[1].surface = Some(SurfaceControl::Key { index: 2 });
        assert!(config.validate().is_err());
    }
}
