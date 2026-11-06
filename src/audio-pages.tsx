import { useState } from "react";
import type { Run, SessionApp, Shared } from "./types";
import { Empty, Field, Icon, Panel, percent } from "./ui";

export function Board({ view, config, run, edit, busy, pending, connect }: Shared & { pending: boolean; connect: () => void }) {
  const preview = view.config.preview && view.outputs.includes(view.config.preview);
  const destination = view.config.destination && view.outputs.includes(view.config.destination);
  const importSound = () => void run({ kind: "import_sound" });
  return <>
    <div className="section-head"><div><h2>Biblioteca de sonidos</h2><p className="muted">Clips WAV o MP3 de hasta 30 segundos.</p></div>
      <button className="primary" disabled={busy || pending} onClick={importSound}><Icon name="add" /> Importar sonido</button></div>
    <div className="route-strip">
      <button className="route" onClick={connect}><Icon name="headphones" /><span><strong>Preescucha</strong><small>{preview ? view.config.preview : "Selecciona una salida local"}</small></span><span className={preview ? "dot live" : "dot"} /></button>
      <button className="route" onClick={connect}><Icon name="sound" /><span><strong>Salida Discord</strong><small>{destination ? view.config.destination : "Selecciona una salida virtual"}</small></span><span className={destination ? "dot live" : "dot"} /></button>
    </div>
    {config.sounds.length === 0 ? <Empty icon="sound" title="Tu biblioteca está vacía">
      <p>Importa un sonido para preescucharlo o asignarlo a un pad del MPK Mini.</p>
      <button className="primary" disabled={busy || pending} onClick={importSound}><Icon name="add" /> Importar primer sonido</button>
    </Empty> : <div className="sound-grid">{config.sounds.map((sound, index) => <article className="sound-card" key={sound.id}>
      <div className="card-kicker"><Icon name="sound" /><span>CLIP {String(index + 1).padStart(2, "0")}</span></div>
      <h3>{sound.name}</h3>
      <Field label={`Nivel · ${percent(sound.level)} %`}><input type="range" min={0} max={100} value={percent(sound.level)} onChange={event => {
        const level = Number(event.target.value) / 100;
        edit(config => ({ ...config, sounds: config.sounds.map(item => item.id === sound.id ? { ...item, level } : item) }));
      }} /></Field>
      <div className="toolbar wrap">
        <button disabled={busy || !preview} onClick={() => void run({ kind: "play", id: sound.id, preview: true, repeat: false })}><Icon name="headphones" size={16} /> Probar</button>
        <button disabled={busy || !destination} onClick={() => void run({ kind: "play", id: sound.id, preview: false, repeat: false })}><Icon name="play" size={16} /> Enviar</button>
        <button className="icon-button" disabled={busy || !destination} aria-label={`Repetir ${sound.name} en Discord`} title="Repetir en Discord" onClick={() => void run({ kind: "play", id: sound.id, preview: false, repeat: true })}><Icon name="loop" size={16} /></button>
      </div>
      <details><summary>Editar clip</summary><Field label="Nombre"><input value={sound.name} maxLength={60} onChange={event => {
        const name = event.target.value;
        edit(config => ({ ...config, sounds: config.sounds.map(item => item.id === sound.id ? { ...item, name } : item) }));
      }} /></Field>
        <button className="danger" onClick={() => edit(config => ({ ...config, sounds: config.sounds.filter(item => item.id !== sound.id) }))}>Quitar de la biblioteca</button>
      </details>
    </article>)}</div>}
  </>;
}

export function Connections({ view, config, edit, run, busy }: Shared) {
  const microphone = config.voice.microphone;
  const voicePending = config.destination !== view.config.destination || config.preview !== view.config.preview
    || microphone !== view.config.voice.microphone || config.voice.level !== view.config.voice.level;
  return <>
    <Panel title="Entrada MIDI" subtitle="Elige el puerto que envía los pads, teclas y mandos. Guarda para abrir la conexión.">
      <Field label="Dispositivo MIDI"><select value={config.midi_port ?? ""} onChange={event => edit(config => ({ ...config, midi_port: event.target.value || null }))}>
        <option value="">Sin seleccionar</option>
        {config.midi_port && !view.ports.some(port => port.id === config.midi_port) && <option value={config.midi_port}>Puerto guardado · ausente</option>}
        {view.ports.map(port => <option key={port.id} value={port.id}>{port.name}</option>)}
      </select></Field>
      <p className="hint">{view.midi_error?.message ?? (view.midi_connected ? "Puerto abierto. Puedes aprender controles en Asignaciones MIDI." : "Conecta el MPK Mini por USB para que aparezca en la lista.")}</p>
    </Panel>
    <Panel title="Rutas de audio" subtitle="Selecciona dispositivos distintos para separar lo que escuchas de lo que envías.">
      <div className="form-grid">{(["preview", "destination"] as const).map(route =>
        <Field key={route} label={route === "preview" ? "Salida de preescucha" : "Salida para Discord"}>
          <select value={config[route] ?? ""} onChange={event => edit(config => ({ ...config, [route]: event.target.value || null }))}>
            <option value="">Sin seleccionar</option>
            {config[route] && !view.outputs.includes(config[route]) && <option value={config[route]}>{config[route]} · ausente</option>}
            {[...new Set(view.outputs)].filter(output => route !== "preview" || output !== view.virtual_output).map(output => <option key={output} value={output}>{output}</option>)}
          </select>
        </Field>)}</div>
      <p className="hint">Usa tus auriculares como preescucha y una entrada de audio virtual para enviar los clips a Discord.</p>
    </Panel>
    <Panel title="Micrófono MPK Deck" subtitle="Combina tu voz y los clips de la salida para Discord. Activa la captura manualmente en cada sesión.">
      {view.virtual_output ? <>
        <div className="form-grid">
          <Field label="Micrófono físico"><select value={microphone ?? ""} onChange={event => {
            const microphone = event.target.value || null;
            edit(config => ({ ...config, voice: { ...config.voice, microphone } }));
          }}>
            <option value="">Sin seleccionar</option>
            {microphone && !view.microphones.includes(microphone) && <option value={microphone}>{microphone} · ausente</option>}
            {[...new Set(view.microphones)].map(name => <option key={name} value={name}>{name}</option>)}
          </select></Field>
          <Field label={`Nivel de micrófono · ${percent(config.voice.level)} %`}><input type="range" min={0} max={100} value={percent(config.voice.level)} onChange={event => {
            const level = Number(event.target.value) / 100;
            edit(config => ({ ...config, voice: { ...config.voice, level } }));
          }} /></Field>
        </div>
        <div className="toolbar wrap">
          {config.destination !== view.virtual_output && <button disabled={busy} onClick={() => {
            const destination = view.virtual_output;
            edit(config => ({ ...config, destination, preview: config.preview === destination ? null : config.preview }));
          }}>Usar salida de MPK Deck</button>}
          <button className={view.voice_active ? "danger" : "primary"} disabled={!view.voice_active && (busy || voicePending || !microphone || config.destination !== view.virtual_output)}
            onClick={() => void run({ kind: "voice", enabled: !view.voice_active })}>
            {view.voice_active ? "Desactivar micrófono" : "Activar micrófono"}
          </button>
          <span role="status">{view.voice_active ? "Micrófono activo" : voicePending ? "Guarda las rutas y el nivel para activarlo." : "Micrófono desactivado"}</span>
        </div>
        <p className="hint">En Discord, selecciona la entrada asociada a <strong>MPK Deck Virtual Audio</strong>. Mantén los auriculares como salida.</p>
      </> : <p className="hint">Windows todavía no detecta el controlador de audio virtual de MPK Deck. Puedes seguir usando la preescucha local.</p>}
    </Panel>
  </>;
}

export function Mixer({ view, busy, run }: Shared) {
  return <>
    <div className="section-head"><div><h2>Sesiones de Windows</h2><p className="muted">Los cambios se aplican a las sesiones de audio de cada aplicación.</p></div></div>
    {view.mixer_error && <p role="alert" className="notice">{view.mixer_error.message}</p>}
    {view.apps.length === 0 ? <Empty icon="mix" title="No hay sesiones de audio"><p>Reproduce audio en una aplicación para que aparezca en el mezclador.</p></Empty>
      : <div className="mix-list">{view.apps.map(app => <MixRow key={app.id} app={app} busy={busy} run={run} />)}</div>}
  </>;
}

function MixRow({ app, busy, run }: { app: SessionApp; busy: boolean; run: Run }) {
  const [draft, setDraft] = useState<number | null>(null);
  const value = draft ?? percent(app.volume);
  return <article className="mix-row">
    <span className="mix-icon"><Icon name="sound" /></span>
    <div className="mix-name"><h3>{app.name}</h3><small>{app.active} activas · {app.sessions} sesiones{app.mixed ? " · niveles distintos" : ""}</small></div>
    <Field label={`Volumen de ${app.name} · ${value} %`}><input type="range" min={0} max={100} value={value} onChange={event => setDraft(Number(event.target.value))} /></Field>
    <div className="toolbar"><button disabled={busy || draft === null} onClick={async () => { if (await run({ kind: "mix", app: app.id, volume: value / 100 })) setDraft(null); }}>Aplicar</button>
      <button disabled={busy} aria-pressed={app.muted} onClick={() => void run({ kind: "mix", app: app.id, mute: !app.muted })}>{app.muted ? "Activar audio" : "Silenciar"}</button></div>
  </article>;
}
