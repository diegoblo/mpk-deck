use crate::model::{Config, Fault, MAX_CONFIG, Profile, Result, SCHEMA};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileFile {
    pub version: u32,
    pub profile: Profile,
}

pub struct Store {
    root: PathBuf,
    _instance: Option<File>,
}

fn io_error(operation: &str, error: std::io::Error) -> Fault {
    Fault::new("storage", format!("{operation} ({:?}).", error.kind()))
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = read_bounded(path, MAX_CONFIG)?;
    serde_json::from_slice(&bytes).map_err(|_| {
        Fault::new(
            "json",
            "El archivo JSON no coincide con el formato esperado.",
        )
    })
}

pub fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = File::open(path).map_err(|error| io_error("No se pudo abrir el archivo", error))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("No se pudo leer el archivo", error))?;
    if bytes.len() as u64 > limit {
        return Err(Fault::new("size", "El archivo supera el tamaño permitido."));
    }
    Ok(bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Fault::new("storage", "Falta la carpeta de destino."))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| io_error("No se pudo crear el archivo temporal", error))?;
    temp.write_all(bytes)
        .and_then(|()| temp.as_file().sync_all())
        .map_err(|error| io_error("No se pudo guardar el archivo", error))?;
    temp.persist(path)
        .map_err(|error| io_error("No se pudo reemplazar el archivo", error.error))?;
    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| Fault::new("json", "No se pudo serializar la configuración."))?;
    if bytes.len() as u64 > MAX_CONFIG {
        return Err(Fault::new("size", "La configuración supera 1 MiB."));
    }
    atomic_write(path, &bytes)
}

impl Store {
    pub fn new(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(root.join("sounds"))
            .map_err(|error| io_error("No se pudo preparar la carpeta de datos", error))?;
        Ok(Self {
            root,
            _instance: None,
        })
    }

    pub fn for_app(root: PathBuf) -> Result<Self> {
        let mut store = Self::new(root)?;
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(store.root.join("instance.lock"))
            .map_err(|error| io_error("No se pudo abrir el bloqueo de configuración", error))?;
        lock.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => {
                Fault::new("instance", "MPK Deck ya está abierto. Vuelve a su ventana.")
            }
            std::fs::TryLockError::Error(error) => {
                io_error("No se pudo bloquear la configuración", error)
            }
        })?;
        store._instance = Some(lock);
        Ok(store)
    }

    pub fn load(&self) -> (Config, Option<Fault>) {
        let path = self.root.join("settings.json");
        if !path.exists() && !self.root.join("settings.backup.json").exists() {
            return (Config::default(), None);
        }
        let load = |path: &Path| -> Result<Config> {
            let config: Config = read_json(path)?;
            config.validate()?;
            Ok(config)
        };
        match load(&path) {
            Ok(config) => (config, None),
            Err(error) => match load(&self.root.join("settings.backup.json")) {
                Ok(config) => (
                    config,
                    Some(Fault::new(
                        "recovery",
                        "Se abrió la copia de respaldo. Guarda para reparar la configuración.",
                    )),
                ),
                Err(_) => (
                    Config::default(),
                    Some(Fault::new(
                        "recovery",
                        format!(
                            "{} Se abrió un perfil nuevo. El archivo anterior se conservará al guardar.",
                            error.message
                        ),
                    )),
                ),
            },
        }
    }

    pub fn save(&self, config: &Config) -> Result<()> {
        let path = self.root.join("settings.json");
        if path
            .try_exists()
            .map_err(|error| io_error("No se pudo comprobar la configuración", error))?
        {
            if read_json::<Config>(&path)
                .and_then(|old| old.validate())
                .is_ok()
            {
                atomic_write(
                    &self.root.join("settings.backup.json"),
                    &read_bounded(&path, MAX_CONFIG)?,
                )?;
            } else {
                let backup = self.root.join(format!(
                    "settings-recovered-{}.json",
                    crate::model::new_id()
                ));
                fs::copy(&path, backup)
                    .map_err(|error| io_error("No se pudo crear una copia recuperable", error))?;
            }
        }
        write_json(&path, config)
    }

    pub fn sound(&self, id: &str) -> PathBuf {
        self.root.join("sounds").join(id)
    }

    pub fn save_sound(&self, id: &str, bytes: &[u8]) -> Result<()> {
        atomic_write(&self.sound(id), bytes)
    }
}

pub fn export_profile(path: &Path, profile: Profile) -> Result<()> {
    write_json(
        path,
        &ProfileFile {
            version: SCHEMA,
            profile,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_settings_recover_backup_and_preserve_original_on_save() {
        let folder = tempfile::tempdir().unwrap();
        let store = Store::new(folder.path().to_owned()).unwrap();
        let mut config = Config::default();
        store.save(&config).unwrap();
        config.revision += 1;
        store.save(&config).unwrap();
        let settings = folder.path().join("settings.json");
        fs::write(&settings, b"{broken").unwrap();

        let (backup, notice) = store.load();
        assert_eq!(backup.revision, 0);
        assert_eq!(notice.unwrap().code, "recovery");
        store.save(&config).unwrap();
        assert_eq!(store.load(), (config, None));
        assert!(fs::read_dir(folder.path()).unwrap().any(|file| {
            let file = file.unwrap();
            file.file_name()
                .to_string_lossy()
                .starts_with("settings-recovered-")
                && fs::read(file.path()).unwrap() == b"{broken"
        }));
    }
}
