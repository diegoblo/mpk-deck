use crate::{
    audio::{self, Audio},
    microphone,
    midi::Midi,
    model::{Sound, new_id},
    store::Store,
    windows_audio::{Com, Mixer},
};
use rodio::cpal::{
    self, FromSample, Sample, SampleFormat, SizedSample,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requiere el MPK Mini conectado a este Windows"]
fn opens_physical_mpk() {
    let _com = Com::init().unwrap();
    let (tx, rx) = mpsc::sync_channel(256);
    let mut midi = Midi::new(tx, Arc::new(AtomicU32::new(0)));
    let (ports, _) = midi.refresh(None).unwrap();
    let port = ports
        .iter()
        .find(|port| port.name.to_ascii_lowercase().contains("mpk mini 3"))
        .expect("Conecta un MPK mini 3 por USB");
    let (_, error) = midi.refresh(Some(&port.id)).unwrap();
    assert!(midi.connected(), "{error:?}");
    println!("Puerto físico abierto: {}", port.name);

    if let Ok(seconds) = std::env::var("MPK_DECK_MIDI_RECONNECT_SECONDS") {
        let deadline = Instant::now() + Duration::from_secs(seconds.parse().unwrap());
        let mut removed = false;
        let mut reopened = false;
        while Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(400)) {
                Ok(crate::engine::Event::MidiRemoved(id)) if midi.removed(&id) => {
                    println!("WinRT notificó la retirada del puerto.");
                    removed = true;
                }
                Ok(crate::engine::Event::Midi(generation, message))
                    if reopened && generation == midi.generation =>
                {
                    println!("Mensaje después de reconectar: {message:?}");
                    break;
                }
                _ => {}
            }
            if removed && !reopened {
                midi.refresh(Some(&port.id)).unwrap();
                if midi.connected() {
                    println!("Puerto reabierto con el identificador guardado.");
                    reopened = true;
                }
            }
        }
        assert!(
            removed && reopened,
            "No se observó un ciclo de retirada y reconexión."
        );
    }

    if let Ok(seconds) = std::env::var("MPK_DECK_MIDI_CAPTURE_SECONDS") {
        let limit = Instant::now() + Duration::from_secs(seconds.parse().unwrap());
        let mut captured = 0;
        while let Ok(event) = rx.recv_timeout(limit.saturating_duration_since(Instant::now())) {
            if let crate::engine::Event::Midi(_, message) = event {
                println!("{message:?}");
                captured += 1;
            }
        }
        assert!(captured > 0, "No se tocaron controles durante la prueba");
    }
    midi.disconnect();
    assert!(!midi.connected());
}

fn test_wav() -> Vec<u8> {
    let frames: u32 = 96_000;
    let bytes = frames * 2;
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + bytes).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(48_000u32.to_le_bytes());
    wav.extend(96_000u32.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(bytes.to_le_bytes());
    for index in 0..frames {
        let sample =
            ((index as f32 * std::f32::consts::TAU * 440.0 / 48_000.0).sin() * 2300.0) as i16;
        wav.extend(sample.to_le_bytes());
    }
    wav
}

fn listen_for_tone<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    heard: Arc<AtomicBool>,
) -> cpal::Stream
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device
        .build_input_stream(
            config,
            move |samples: &[T], _| {
                if samples
                    .iter()
                    .any(|&sample| f32::from_sample(sample).abs() > 0.002)
                {
                    heard.store(true, Ordering::Relaxed);
                }
            },
            |error| eprintln!("WASAPI: {error}"),
            None,
        )
        .unwrap()
}

#[test]
#[ignore = "requiere el controlador MPK Deck firmado e instalado"]
fn transfers_tone_through_virtual_microphone() {
    let _com = Com::init().unwrap();
    let output =
        microphone::virtual_output(&audio::outputs().unwrap(), &microphone::inputs().unwrap())
            .expect("Windows debe detectar la pareja de endpoints de MPK Deck");
    let input = cpal::default_host()
        .input_devices()
        .unwrap()
        .find(|device| {
            device
                .name()
                .is_ok_and(|name| microphone::is_virtual(&name))
        })
        .unwrap();
    let config = input.default_input_config().unwrap();
    let heard = Arc::new(AtomicBool::new(false));
    let capture = match config.sample_format() {
        SampleFormat::F32 => listen_for_tone::<f32>(&input, &config.config(), Arc::clone(&heard)),
        SampleFormat::I16 => listen_for_tone::<i16>(&input, &config.config(), Arc::clone(&heard)),
        other => panic!("Formato de prueba inesperado: {other:?}"),
    };
    capture.play().unwrap();
    thread::sleep(Duration::from_millis(200));
    assert!(
        !heard.load(Ordering::Relaxed),
        "La entrada virtual debería estar en silencio."
    );

    let folder = tempfile::tempdir().unwrap();
    let store = Store::new(folder.path().to_owned()).unwrap();
    let sound = Sound {
        id: new_id(),
        name: "QA tone".into(),
        level: 1.0,
    };
    store.save_sound(&sound.id, &test_wav()).unwrap();
    let mut audio = Audio::default();
    audio
        .play(&store, &sound, Some(&output), false, false, None)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !heard.load(Ordering::Relaxed) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        heard.load(Ordering::Relaxed),
        "El tono enviado a la salida no llegó al micrófono virtual."
    );
}

#[test]
#[ignore = "reproduce brevemente un tono de prueba en la salida predeterminada de Windows"]
fn plays_local_audio_and_restores_own_session() {
    let _com = Com::init().unwrap();
    let host = cpal::default_host();
    let output = host
        .default_output_device()
        .expect("Selecciona una salida de audio en Windows");
    let name = output.name().unwrap();
    let config = output.default_output_config().unwrap();
    assert_eq!(
        config.sample_format(),
        SampleFormat::F32,
        "Se requiere loopback WASAPI compartido en f32 para esta comprobación."
    );
    let heard = Arc::new(AtomicBool::new(false));
    let captured = Arc::clone(&heard);
    let loopback = output
        .build_input_stream(
            &config.config(),
            move |samples: &[f32], _| {
                if samples.iter().any(|sample| sample.abs() > 0.002) {
                    captured.store(true, Ordering::Relaxed);
                }
            },
            |error| eprintln!("Loopback: {error}"),
            None,
        )
        .unwrap();
    loopback.play().unwrap();
    thread::sleep(Duration::from_millis(200));
    assert!(
        !heard.load(Ordering::Relaxed),
        "Hay otro audio en la salida. Pausa la reproducción y repite."
    );

    let folder = tempfile::tempdir().unwrap();
    let store = Store::new(folder.path().to_owned()).unwrap();
    let sound = Sound {
        id: new_id(),
        name: "QA tone".into(),
        level: 1.0,
    };
    store.save_sound(&sound.id, &test_wav()).unwrap();
    let mut audio = Audio::default();
    audio
        .play(&store, &sound, Some(&name), true, false, None)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !heard.load(Ordering::Relaxed) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        heard.load(Ordering::Relaxed),
        "No se capturó señal en el loopback de la salida real."
    );
    let mut mixer = Mixer::default();
    mixer.refresh().unwrap();
    let id = std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .to_lowercase();
    let before = mixer
        .snapshot()
        .into_iter()
        .find(|app| app.id == id)
        .expect("Debe existir la sesión WASAPI del proceso de prueba");
    let changed = if before.volume > 0.2 {
        before.volume - 0.1
    } else {
        before.volume + 0.1
    };
    mixer.set(&id, Some(changed), None).unwrap();
    let after = mixer.snapshot().into_iter().find(|app| app.id == id);
    mixer.set(&id, Some(before.volume), None).unwrap();
    assert!((after.unwrap().volume - changed).abs() < 0.01);
    audio.stop();
    println!("Tono capturado por WASAPI loopback y nivel de la sesión de prueba restaurado.");
}
