#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod engine;
mod microphone;
mod midi;
mod model;
mod store;
mod windows_audio;
#[cfg(test)]
mod windows_smoke;

use engine::{Engine, Request, Snapshot};
use model::{Config, Fault, Result};
use serde::Deserialize;
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Snapshot,
    Save {
        config: Config,
    },
    ImportSound,
    ImportProfile,
    ExportProfile {
        id: String,
    },
    Play {
        id: String,
        preview: bool,
        repeat: bool,
    },
    Stop,
    Voice {
        enabled: bool,
    },
    Mix {
        app: String,
        volume: Option<f32>,
        mute: Option<bool>,
    },
    Monitor {
        enabled: bool,
    },
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

fn file_error() -> Fault {
    Fault::new("file_path", "Selecciona un archivo local de Windows.")
}

async fn dialog(app: AppHandle, window: WebviewWindow, command: Command) -> Result<Request> {
    tauri::async_runtime::spawn_blocking(move || {
        let picker = app.dialog().file().set_parent(&window);
        match command {
            Command::ImportSound => {
                let Some(file) = picker
                    .add_filter("Audio WAV o MP3", &["wav", "mp3"])
                    .blocking_pick_file()
                else {
                    return Ok(Request::Snapshot);
                };
                let path = file.into_path().map_err(|_| file_error())?;
                let extension = path
                    .extension()
                    .and_then(|part| part.to_str())
                    .map(str::to_ascii_lowercase);
                if !matches!(extension.as_deref(), Some("wav" | "mp3")) {
                    return Err(Fault::new("file_type", "Selecciona un archivo WAV o MP3."));
                }
                let name = path
                    .file_stem()
                    .and_then(|part| part.to_str())
                    .ok_or_else(file_error)?
                    .to_owned();
                let (bytes, clip) = audio::load_file(&path)?;
                Ok(Request::AddSound { name, bytes, clip })
            }
            Command::ImportProfile => {
                match picker
                    .add_filter("Perfil JSON", &["json"])
                    .blocking_pick_file()
                {
                    Some(file) => Ok(Request::ImportProfile(
                        file.into_path().map_err(|_| file_error())?,
                    )),
                    None => Ok(Request::Snapshot),
                }
            }
            Command::ExportProfile { id } => {
                match picker
                    .add_filter("Perfil JSON", &["json"])
                    .set_file_name("perfil-mpk-deck.json")
                    .blocking_save_file()
                {
                    Some(file) => Ok(Request::ExportProfile {
                        path: file.into_path().map_err(|_| file_error())?,
                        id,
                    }),
                    None => Ok(Request::Snapshot),
                }
            }
            _ => unreachable!("file dialog command"),
        }
    })
    .await
    .map_err(|_| {
        Fault::new(
            "file",
            "Se interrumpió la selección o la lectura del archivo.",
        )
    })?
}

#[tauri::command]
async fn deck(
    app: AppHandle,
    window: WebviewWindow,
    engine: State<'_, Engine>,
    command: Command,
) -> Result<Snapshot> {
    if window.label() != "main" {
        return Err(Fault::new("window", "Ventana no autorizada."));
    }
    let request = match command {
        Command::Snapshot => Request::Snapshot,
        Command::Save { config } => Request::Save(config),
        Command::ImportSound | Command::ImportProfile | Command::ExportProfile { .. } => {
            dialog(app, window, command).await?
        }
        Command::Play {
            id,
            preview,
            repeat,
        } => Request::Play {
            id,
            preview,
            repeat,
        },
        Command::Stop => Request::Stop,
        Command::Voice { enabled } => Request::Voice(enabled),
        Command::Mix { app, volume, mute } => Request::Mix { app, volume, mute },
        Command::Monitor { enabled } => Request::Monitor(enabled),
        Command::Clear => Request::Clear,
        Command::Learn => Request::Learn,
        Command::CancelLearn => Request::CancelLearn,
        Command::CaptureApp => Request::CaptureApp,
        Command::RunMacro { profile, id } => Request::RunMacro { profile, id },
        Command::CancelMacro => Request::CancelMacro,
    };
    engine.request(request).await
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let store = store::Store::for_app(app.path().app_local_data_dir()?)?;
            app.manage(Engine::start(app.handle().clone(), store)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![deck])
        .build(tauri::generate_context!());
    let app = match app {
        Ok(app) => app,
        Err(error) => {
            use windows::{
                Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
                core::{HSTRING, PCWSTR},
            };
            let title = HSTRING::from("MPK Deck");
            let message = HSTRING::from(format!("No se pudo iniciar MPK Deck.\n\n{error}"));
            unsafe {
                MessageBoxW(
                    None,
                    PCWSTR(message.as_ptr()),
                    PCWSTR(title.as_ptr()),
                    MB_OK | MB_ICONERROR,
                )
            };
            return;
        }
    };

    app.run(|app, event| {
        if matches!(event, tauri::RunEvent::Exit)
            && let Some(engine) = app.try_state::<Engine>()
        {
            engine.close();
        }
    });
}
