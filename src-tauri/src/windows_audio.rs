use crate::model::{Fault, Result, app_name, valid_app};
use serde::Serialize;
use std::{collections::BTreeMap, ptr::null};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Media::Audio::{
            AudioSessionStateActive, AudioSessionStateExpired, DEVICE_STATE_ACTIVE,
            Endpoints::IAudioEndpointVolume, IAudioSessionControl2, IAudioSessionManager2,
            IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator, eMultimedia, eRender,
        },
        System::{
            Com::{
                CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
            },
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    },
    core::{Interface, PWSTR},
};

pub struct Com;
impl Com {
    pub fn init() -> Result<Self> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }.map_err(|_| {
            Fault::new(
                "com",
                "No se pudo iniciar COM para el mezclador de Windows.",
            )
        })?;
        Ok(Self)
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn default_render_volume() -> windows::core::Result<IAudioEndpointVolume> {
    unsafe {
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let endpoint = devices.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
        endpoint.Activate(CLSCTX_ALL, None)
    }
}

pub fn set_master_volume(level: f32) -> Result<()> {
    let volume = default_render_volume().map_err(|_| {
        Fault::new(
            "master_volume_endpoint",
            "No se pudo abrir la salida predeterminada. Comprueba la salida de sonido en Windows.",
        )
    })?;
    unsafe { volume.SetMasterVolumeLevelScalar(level, null()) }.map_err(|_| {
        Fault::new(
            "master_volume_set",
            "No se pudo cambiar el volumen general. Comprueba la salida de sonido en Windows.",
        )
    })
}

#[cfg(test)]
#[test]
#[ignore = "cambia brevemente el volumen de la salida predeterminada de Windows"]
fn changes_and_restores_real_master_volume() {
    let _com = Com::init().unwrap();
    let volume = default_render_volume().unwrap();
    let original = unsafe { volume.GetMasterVolumeLevelScalar() }.unwrap();
    let muted = unsafe { volume.GetMute() }.unwrap();
    let target = if original >= 0.02 {
        original - 0.02
    } else {
        original + 0.02
    };
    set_master_volume(target).unwrap();
    let changed = unsafe { volume.GetMasterVolumeLevelScalar() };
    let current_mute = unsafe { volume.GetMute() };
    unsafe { volume.SetMasterVolumeLevelScalar(original, null()) }.unwrap();
    assert!((changed.unwrap() - target).abs() < 0.01);
    assert!((unsafe { volume.GetMasterVolumeLevelScalar() }.unwrap() - original).abs() < 0.01);
    assert_eq!(current_mute.unwrap(), muted);
}

struct Process(HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn process_path(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let process =
        Process(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?);
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    }
    .ok()?;
    let path = String::from_utf16(&buf[..len as usize])
        .ok()?
        .to_lowercase();
    valid_app(&path).then_some(path)
}

pub fn foreground() -> Option<String> {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid)) };
    (pid != std::process::id())
        .then(|| process_path(pid))
        .flatten()
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct App {
    pub id: String,
    pub name: String,
    pub volume: f32,
    pub muted: bool,
    pub mixed: bool,
    pub sessions: usize,
    pub active: usize,
}

struct Session {
    volume: ISimpleAudioVolume,
    active: bool,
}

#[derive(Default)]
pub struct Mixer {
    sessions: BTreeMap<String, Vec<Session>>,
}

impl Mixer {
    pub fn refresh(&mut self) -> Result<()> {
        self.sessions.clear();
        self.enumerate().map_err(|_| Fault::new("sessions", "No se pudieron consultar las sesiones de audio. Comprueba el servicio de audio de Windows."))
    }

    fn enumerate(&mut self) -> windows::core::Result<()> {
        unsafe {
            let devices: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let endpoints = devices.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
            let mut paths = BTreeMap::new();
            for index in 0..endpoints.GetCount()? {
                let endpoint = endpoints.Item(index)?;
                let manager: IAudioSessionManager2 = endpoint.Activate(CLSCTX_ALL, None)?;
                let sessions = manager.GetSessionEnumerator()?;
                for index in 0..sessions.GetCount()? {
                    let Ok(session) = sessions.GetSession(index) else {
                        continue;
                    };
                    let Ok(state) = session.GetState() else {
                        continue;
                    };
                    if state == AudioSessionStateExpired {
                        continue;
                    }
                    let Ok(control) = session.cast::<IAudioSessionControl2>() else {
                        continue;
                    };
                    let Ok(pid) = control.GetProcessId() else {
                        continue;
                    };
                    let Some(path) = paths.entry(pid).or_insert_with(|| process_path(pid)) else {
                        continue;
                    };
                    let Ok(volume) = session.cast::<ISimpleAudioVolume>() else {
                        continue;
                    };
                    self.sessions
                        .entry(path.clone())
                        .or_default()
                        .push(Session {
                            volume,
                            active: state == AudioSessionStateActive,
                        });
                }
            }
        }
        Ok(())
    }

    pub fn snapshot(&self) -> Vec<App> {
        self.sessions
            .iter()
            .filter_map(|(id, sessions)| {
                let mut levels = sessions.iter().filter_map(|session| unsafe {
                    Some((
                        session.volume.GetMasterVolume().ok()?,
                        session.volume.GetMute().ok()?.as_bool(),
                    ))
                });
                let (volume, first_muted) = levels.next()?;
                let mut muted = first_muted;
                let mut mixed = false;
                for (level, is_muted) in levels {
                    mixed |= (volume - level).abs() > 0.001 || first_muted != is_muted;
                    muted &= is_muted;
                }
                Some(App {
                    id: id.clone(),
                    name: app_name(id).into(),
                    volume,
                    muted,
                    mixed,
                    sessions: sessions.len(),
                    active: sessions.iter().filter(|session| session.active).count(),
                })
            })
            .collect()
    }

    pub fn set(&mut self, app: &str, volume: Option<f32>, mute: Option<bool>) -> Result<()> {
        if !self.sessions.contains_key(app) {
            self.refresh()?;
        }
        let sessions = self.sessions.get(app).ok_or_else(|| {
            Fault::new(
                "session_missing",
                "La aplicación no tiene sesiones de audio disponibles.",
            )
        })?;
        let mut failures = 0;
        for session in sessions {
            let result = unsafe {
                if let Some(level) = volume {
                    session.volume.SetMasterVolume(level, null())
                } else if let Some(mute) = mute {
                    session.volume.SetMute(mute, null())
                } else {
                    Ok(())
                }
            };
            failures += usize::from(result.is_err());
        }
        if failures > 0 {
            Err(Fault::new(
                "session_changed",
                format!(
                    "No se pudieron modificar {failures} de {} sesiones. Actualiza el mezclador y vuelve a intentarlo.",
                    sessions.len()
                ),
            ))
        } else {
            Ok(())
        }
    }

    pub fn toggle(&mut self, app: &str) -> Result<()> {
        if !self.sessions.contains_key(app) {
            self.refresh()?;
        }
        let current = self
            .snapshot()
            .into_iter()
            .find(|item| item.id == app)
            .ok_or_else(|| {
                Fault::new(
                    "session_missing",
                    "La aplicación no tiene sesiones de audio disponibles.",
                )
            })?;
        self.set(app, None, Some(!current.muted))
    }
}
