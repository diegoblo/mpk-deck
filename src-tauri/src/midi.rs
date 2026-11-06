use crate::{
    engine::Event,
    model::{Fault, Input, Kind, Result},
};
use serde::Serialize;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
    mpsc::SyncSender,
};
use windows::{
    Devices::{
        Enumeration::{DeviceInformation, DeviceInformationUpdate, DeviceWatcher},
        Midi::{MidiInPort, MidiMessageReceivedEventArgs},
    },
    Foundation::TypedEventHandler,
    Storage::Streams::DataReader,
    core::HSTRING,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Port {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Message {
    pub input: Input,
    pub value: u8,
    pub release: bool,
}

pub fn parse(data: &[u8]) -> Option<Message> {
    let [status, number, value] = *data else {
        return None;
    };
    if number > 127 || value > 127 {
        return None;
    }
    let (kind, release) = match status & 0xf0 {
        0x80 => (Kind::Note, true),
        0x90 => (Kind::Note, value == 0),
        0xb0 => (Kind::Cc, false),
        _ => return None,
    };
    Some(Message {
        input: Input {
            kind,
            channel: status & 15,
            number,
        },
        value,
        release,
    })
}

struct Connection {
    id: String,
    port: MidiInPort,
    token: i64,
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.port.RemoveMessageReceived(self.token);
        let _ = self.port.Close();
    }
}

struct Watch {
    device: DeviceWatcher,
    added: Option<i64>,
    updated: Option<i64>,
    removed: Option<i64>,
}

impl Watch {
    fn start(selector: &HSTRING, tx: SyncSender<Event>) -> windows::core::Result<Self> {
        let mut watch = Self {
            device: DeviceInformation::CreateWatcherAqsFilter(selector)?,
            added: None,
            updated: None,
            removed: None,
        };
        // Windows requires all three subscriptions. Periodic discovery handles additions and renames.
        watch.added =
            Some(watch.device.Added(
                &TypedEventHandler::<DeviceWatcher, DeviceInformation>::new(|_, _| Ok(())),
            )?);
        watch.updated =
            Some(watch.device.Updated(&TypedEventHandler::<
                DeviceWatcher,
                DeviceInformationUpdate,
            >::new(|_, _| Ok(())))?);
        watch.removed = Some(watch.device.Removed(&TypedEventHandler::<
            DeviceWatcher,
            DeviceInformationUpdate,
        >::new(move |_, update| {
            if let Some(update) = update.as_ref() {
                let _ = tx.try_send(Event::MidiRemoved(update.Id()?.to_string()));
            }
            Ok(())
        }))?);
        watch.device.Start()?;
        Ok(watch)
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.device.Stop();
        if let Some(token) = self.added {
            let _ = self.device.RemoveAdded(token);
        }
        if let Some(token) = self.updated {
            let _ = self.device.RemoveUpdated(token);
        }
        if let Some(token) = self.removed {
            let _ = self.device.RemoveRemoved(token);
        }
    }
}

pub struct Midi {
    connection: Option<Connection>,
    watch: Option<Watch>,
    tx: SyncSender<Event>,
    dropped: Arc<AtomicU32>,
    pub generation: u64,
}

impl Midi {
    pub fn new(tx: SyncSender<Event>, dropped: Arc<AtomicU32>) -> Self {
        Self {
            connection: None,
            watch: None,
            tx,
            dropped,
            generation: 0,
        }
    }

    pub fn disconnect(&mut self) {
        if self.connection.take().is_some() {
            self.generation += 1;
        }
    }

    pub fn removed(&mut self, id: &str) -> bool {
        if self
            .connection
            .as_ref()
            .is_some_and(|connection| connection.id == id)
        {
            self.disconnect();
            true
        } else {
            false
        }
    }

    fn available(&mut self) -> windows::core::Result<Vec<Port>> {
        let selector = MidiInPort::GetDeviceSelector()?;
        if self.watch.is_none() {
            self.watch = Watch::start(&selector, self.tx.clone()).ok();
        }
        let devices = DeviceInformation::FindAllAsyncAqsFilter(&selector)?.join()?;
        let mut ports = Vec::new();
        for index in 0..devices.Size()? {
            let device = devices.GetAt(index)?;
            ports.push(Port {
                id: device.Id()?.to_string(),
                name: device.Name()?.to_string(),
            });
        }
        ports.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        Ok(ports)
    }

    fn connect(&mut self, id: &str) -> windows::core::Result<()> {
        self.generation += 1;
        let generation = self.generation;
        let port = MidiInPort::FromIdAsync(&HSTRING::from(id))?.join()?;
        let tx = self.tx.clone();
        let dropped = Arc::clone(&self.dropped);
        let handler =
            TypedEventHandler::<MidiInPort, MidiMessageReceivedEventArgs>::new(move |_, args| {
                if let Some(args) = args.as_ref() {
                    let data = args.Message()?.RawData()?;
                    if data.Length()? == 3 {
                        let mut bytes = [0; 3];
                        DataReader::FromBuffer(&data)?.ReadBytes(&mut bytes)?;
                        if let Some(message) = parse(&bytes)
                            && tx.try_send(Event::Midi(generation, message)).is_err()
                        {
                            dropped.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                Ok(())
            });
        let token = match port.MessageReceived(&handler) {
            Ok(token) => token,
            Err(error) => {
                let _ = port.Close();
                return Err(error);
            }
        };
        self.connection = Some(Connection {
            id: id.into(),
            port,
            token,
        });
        Ok(())
    }

    pub fn refresh(&mut self, desired: Option<&str>) -> Result<(Vec<Port>, Option<Fault>)> {
        let ports = self
            .available()
            .map_err(|_| Fault::new("midi", "Windows no pudo consultar los puertos MIDI."))?;
        let selected = desired.filter(|id| ports.iter().any(|port| port.id == *id));
        if self
            .connection
            .as_ref()
            .is_some_and(|connection| selected != Some(connection.id.as_str()))
        {
            self.disconnect();
        }
        let mut fault = None;
        if self.connection.is_none() {
            if let Some(id) = selected {
                if self.connect(id).is_err() {
                    fault = Some(Fault::new(
                        "midi_busy",
                        "No se pudo abrir el puerto. Cierra otras apps MIDI o vuelve a conectar el dispositivo.",
                    ));
                }
            } else if desired.is_some() {
                fault = Some(Fault::new(
                    "midi_missing",
                    "Puerto MIDI desconectado. Se reintentará automáticamente.",
                ));
            }
        }
        Ok((ports, fault))
    }

    pub fn connected(&self) -> bool {
        self.connection.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_release_velocity_and_control_changes() {
        let note = parse(&[0x99, 36, 73]).unwrap();
        assert_eq!(note.input.channel, 9);
        assert_eq!(note.value, 73);
        assert!(!note.release);
        assert!(parse(&[0x99, 36, 0]).unwrap().release);
        assert!(parse(&[0x89, 36, 64]).unwrap().release);
        assert_eq!(parse(&[0xb0, 12, 0]).unwrap().input.kind, Kind::Cc);
        assert!(parse(&[0xf8]).is_none());
        assert!(parse(&[0x90, 128, 2]).is_none());
    }
}
