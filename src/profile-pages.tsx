import { useState } from "react";
import type { Action, Binding, Input, Macro, Mode, Profile, ProfileProps } from "./types";
import { ActionEditor, AppSelect, Capture, Empty, Field, Icon, Panel, ProfilePicker, copyProfile, inputName, makeProfile } from "./ui";

const modes: Record<Mode, string> = { press: "Pulsación", momentary: "Mientras mantienes", toggle: "Alternante", continuous: "Continuo (CC)" };
const sameInput = (a: Input, b: Input) => a.kind === b.kind && a.number === b.number && a.channel === b.channel;

export function Midi(props: ProfileProps) {
  const { view, config, profile, run, busy, update } = props;
  const [input, setInput] = useState<Input>({ kind: "note", channel: 0, number: 0 });
  const exists = (control: Input) => profile.bindings.some(binding => sameInput(binding.input, control));
  function add(control: Input) {
    if (exists(control)) return;
    const action: Action = config.sounds.length ? { kind: "play", sound: config.sounds[0].id, repeat: false, preview: !config.destination } : { kind: "stop" };
    update({ ...profile, bindings: [...profile.bindings, { input: control, mode: "press", target: { kind: "action", action } }] });
  }
  return <>
    <Panel title="Aprender control" subtitle="Inicia el aprendizaje y toca el pad, tecla o mando que quieras asignar.">
      <ProfilePicker {...props} />
      <div className="toolbar wrap">
        <button className="primary" disabled={busy || !view.midi_connected} onClick={() => void run({ kind: view.learning ? "cancel_learn" : "learn" })}><Icon name="midi" /> {view.learning ? "Cancelar aprendizaje" : "Aprender desde el MPK"}</button>
        {view.learning && <span role="status" className="muted">Esperando un control durante 15 segundos…</span>}
        {view.learned && <button disabled={busy || exists(view.learned)} onClick={() => add(view.learned!)}>
          <Icon name="add" /> Asignar {inputName(view.learned)}
        </button>}
      </div>
      <details><summary>Introducir control manualmente</summary>
        <div className="form-grid">
          <Field label="Tipo"><select value={input.kind} onChange={event => setInput({ ...input, kind: event.target.value as Input["kind"] })}><option value="note">Nota</option><option value="cc">CC</option></select></Field>
          <Field label="Canal (1–16)"><input type="number" min={1} max={16} value={input.channel + 1} onChange={event => setInput({ ...input, channel: Number(event.target.value) - 1 })} /></Field>
          <Field label="Número (0–127)"><input type="number" min={0} max={127} value={input.number} onChange={event => setInput({ ...input, number: Number(event.target.value) })} /></Field>
        </div>
        <button disabled={busy || exists(input) || !Number.isInteger(input.channel) || input.channel < 0 || input.channel > 15 || !Number.isInteger(input.number) || input.number < 0 || input.number > 127} onClick={() => add(input)}>Añadir control</button>
      </details>
    </Panel>
    <Panel title="Controles asignados" subtitle="Los cambios entran en vigor al guardar. Los modos momentáneo y alternante controlan sonidos.">
      {!profile.bindings.length && <p className="muted">Todavía no hay controles asignados en este perfil.</p>}
      {profile.bindings.map((binding, index) => <BindingEditor key={inputName(binding.input)} {...props} binding={binding}
        change={value => update({ ...profile, bindings: profile.bindings.map((item, i) => i === index ? value : item) })}
        remove={() => update({ ...profile, bindings: profile.bindings.filter((_, i) => i !== index) })} />)}
    </Panel>
    <Panel title="Diagnóstico MIDI" subtitle="El registro es temporal y se borra al ocultarlo. Muestra hasta 80 eventos.">
      <div className="toolbar"><button disabled={busy} onClick={() => void run({ kind: "monitor", enabled: !view.monitoring })}>{view.monitoring ? "Ocultar y borrar" : "Mostrar eventos"}</button>
        {view.monitoring && <button disabled={busy} onClick={() => void run({ kind: "clear" })}>Limpiar registro</button>}
        {view.dropped > 0 && <span className="muted">{view.dropped} eventos descartados</span>}
      </div>
      {view.monitoring && <div className="midi-log">{view.log.length ? <table><thead><tr><th>Tiempo</th><th>Control</th><th>Valor</th><th>Resultado</th></tr></thead><tbody>
        {view.log.map((item, i) => <tr key={`${item.elapsed_ms}-${i}`}><td>{item.elapsed_ms} ms</td><td>{inputName(item.message.input)}</td><td>{item.message.value}{item.message.release ? " · soltar" : ""}</td><td>{item.outcome}</td></tr>)}
      </tbody></table> : <p className="muted">Mueve un control del dispositivo para ver su mensaje.</p>}</div>}
    </Panel>
  </>;
}

function BindingEditor({ binding, profile, config, view, change, remove }: ProfileProps & {
  binding: Binding; change: (value: Binding) => void; remove: () => void;
}) {
  return <section className="editor">
    <div className="editor-head"><h3>{inputName(binding.input)}</h3><button className="icon-button danger" aria-label={`Quitar ${inputName(binding.input)}`} onClick={remove}><Icon name="close" size={16} /></button></div>
    <div className="form-grid">
      <Field label="Tipo de asignación"><select value={binding.target.kind} onChange={event => change(event.target.value === "macro"
        ? { ...binding, mode: "press", target: { kind: "macro", id: profile.macros[0]?.id ?? "" } }
        : { ...binding, target: { kind: "action", action: { kind: "stop" } } })}>
        <option value="action">Acción directa</option><option value="macro" disabled={!profile.macros.length}>Macro</option>
      </select></Field>
      <Field label="Modo"><select value={binding.mode} disabled={binding.target.kind === "macro"} onChange={event => change({ ...binding, mode: event.target.value as Mode })}>
        {Object.entries(modes).map(([key, name]) => <option key={key} value={key} disabled={key === "continuous" && binding.input.kind !== "cc"}>{name}</option>)}
      </select></Field>
    </div>
    {binding.target.kind === "action"
      ? <ActionEditor action={binding.target.action} config={config} view={view} onChange={action => change({ ...binding, target: { kind: "action", action } })} />
      : <Field label="Macro"><select value={binding.target.id} onChange={event => change({ ...binding, target: { kind: "macro", id: event.target.value } })}>
        <option value="">Selecciona una macro</option>{profile.macros.map(sequence => <option key={sequence.id} value={sequence.id}>{sequence.name}</option>)}
      </select></Field>}
  </section>;
}

export function Macros(props: ProfileProps) {
  const { config, view, profile, run, edit, busy, update } = props;
  const change = (sequence: Macro) => update({ ...profile, macros: profile.macros.map(item => item.id === sequence.id ? sequence : item) });
  return <>
    <Panel title="Secuencias del perfil" subtitle="Ejecuta acciones en orden y, si quieres, limítalas a una aplicación en primer plano.">
      <ProfilePicker {...props} />
      <div className="toolbar wrap">
        <button className="primary" disabled={busy || profile.macros.length >= 24} onClick={() => update({ ...profile, macros: [...profile.macros,
          { id: crypto.randomUUID(), name: "Nueva macro", enabled: true, foreground: null, steps: [{ delay_ms: 0, action: { kind: "stop" } }] }] })}><Icon name="add" /> Crear macro</button>
        <button disabled={!view.macro_running} className="danger" onClick={() => void run({ kind: "cancel_macro" })}>Cancelar macro en curso</button>
      </div>
      <label className="check"><input type="checkbox" checked={!config.macros_enabled} onChange={event => edit(config => ({ ...config, macros_enabled: !event.target.checked }))} /> Pausar todas las macros al guardar</label>
      <Capture {...props} />
    </Panel>
    {profile.macros.length === 0 && <Empty icon="macro" title="Todavía no hay macros"><p>Crea una secuencia y asígnala a un control MIDI.</p></Empty>}
    {profile.macros.map(sequence => <Panel title={sequence.name || "Macro sin nombre"} key={sequence.id}>
      <div className="form-grid">
        <Field label="Nombre de macro"><input maxLength={60} value={sequence.name} onChange={event => change({ ...sequence, name: event.target.value })} /></Field>
        <Field label="Aplicación requerida"><AppSelect view={view} value={sequence.foreground ?? ""} empty="Cualquier aplicación" onChange={app => change({ ...sequence, foreground: app || null })} /></Field>
      </div>
      <label className="check"><input type="checkbox" checked={sequence.enabled} onChange={event => change({ ...sequence, enabled: event.target.checked })} /> Macro habilitada</label>
      {sequence.steps.map((step, index) => <section className="editor" key={index}>
        <div className="editor-head"><h3>Paso {index + 1}</h3><div className="toolbar">
          <button disabled={index === 0} aria-label={`Subir paso ${index + 1}`} onClick={() => {
            const steps = [...sequence.steps]; [steps[index - 1], steps[index]] = [steps[index], steps[index - 1]];
            change({ ...sequence, steps });
          }}>↑</button>
          <button className="icon-button danger" aria-label={`Quitar paso ${index + 1}`} disabled={sequence.steps.length <= 1} onClick={() => change({ ...sequence, steps: sequence.steps.filter((_, i) => i !== index) })}><Icon name="close" size={16} /></button>
        </div></div>
        <Field label="Espera previa (ms)"><input type="number" min={0} max={10000} value={step.delay_ms} onChange={event => {
          const delay_ms = Number(event.target.value);
          change({ ...sequence, steps: sequence.steps.map((item, i) => i === index ? { ...item, delay_ms } : item) });
        }} /></Field>
        <ActionEditor config={config} view={view} action={step.action} onChange={action => change({ ...sequence, steps: sequence.steps.map((item, i) => i === index ? { ...item, action } : item) })} />
      </section>)}
      <div className="toolbar wrap">
        <button disabled={sequence.steps.length >= 16} onClick={() => change({ ...sequence, steps: [...sequence.steps, { delay_ms: 0, action: { kind: "stop" } }] })}>Añadir paso</button>
        <button disabled={busy || view.active_profile !== profile.id || !view.config.profiles.find(item => item.id === profile.id)?.macros.some(item => item.id === sequence.id)} onClick={() => void run({ kind: "run_macro", profile: profile.id, id: sequence.id })}><Icon name="play" size={16} /> Ejecutar versión guardada</button>
        <button className="danger" onClick={() => update({ ...profile, macros: profile.macros.filter(item => item.id !== sequence.id),
          bindings: profile.bindings.filter(binding => binding.target.kind !== "macro" || binding.target.id !== sequence.id) })}>Quitar macro</button>
      </div>
    </Panel>)}
  </>;
}

export function Profiles(props: ProfileProps & { pending: boolean }) {
  const { view, config, profile, update, run, edit, select, pending, busy } = props;
  function add(value: Profile) {
    edit(config => ({ ...config, profiles: [...config.profiles, value] }));
    select(value.id);
  }
  return <>
    <Panel title="Administrar perfiles" subtitle="Edita asignaciones y macros por perfil. Exporta el perfil guardado como JSON.">
      <ProfilePicker {...props} />
      <div className="toolbar wrap">
        <button className="primary" disabled={busy || config.profiles.length >= 32} onClick={() => add(makeProfile("Nuevo perfil"))}><Icon name="add" /> Nuevo</button>
        <button disabled={busy || config.profiles.length >= 32} onClick={() => add(copyProfile(profile))}>Duplicar</button>
        <button disabled={busy || pending || config.profiles.length >= 32} onClick={() => void run({ kind: "import_profile" })}>Importar JSON</button>
        <button disabled={busy || pending} onClick={() => void run({ kind: "export_profile", id: profile.id })}>Exportar JSON</button>
      </div>
      <div className="form-grid">
        <Field label="Nombre del perfil"><input maxLength={60} value={profile.name} onChange={event => update({ ...profile, name: event.target.value })} /></Field>
        <Field label="Perfil manual o de respaldo"><select value={config.manual_profile} onChange={event => edit(config => ({ ...config, manual_profile: event.target.value }))}>
          {config.profiles.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
        </select></Field>
      </div>
      <div className="toolbar wrap">
        <button disabled={busy} onClick={() => edit(config => ({ ...config, manual_profile: profile.id, automatic: false }))}>Fijar este perfil al guardar</button>
        <button className="danger" disabled={busy || config.profiles.length <= 1} onClick={() => edit(config => {
          const profiles = config.profiles.filter(item => item.id !== profile.id);
          return { ...config, profiles, manual_profile: config.manual_profile === profile.id ? profiles[0].id : config.manual_profile };
        })}>Quitar perfil</button>
      </div>
    </Panel>
    <Panel title="Cambio automático" subtitle="Gana la regla de mayor prioridad. En un empate se usa el orden de la lista de perfiles.">
      <label className="check"><input type="checkbox" checked={config.automatic} onChange={event => edit(config => ({ ...config, automatic: event.target.checked }))} /> Activar cambio automático</label>
      <Field label="Demora antes de cambiar (ms)"><input type="number" min={200} max={5000} step={50} value={config.delay_ms} onChange={event => {
        const delay_ms = Number(event.target.value); edit(config => ({ ...config, delay_ms }));
      }} /></Field>
      <h3>Regla de {profile.name || "este perfil"}</h3>
      <Capture {...props} />
      <div className="form-grid">
        <Field label="Aplicación en primer plano"><AppSelect view={view} value={profile.rule?.app ?? ""} empty="Sin regla" onChange={app =>
          update({ ...profile, rule: app ? { priority: profile.rule?.priority ?? 0, requires_midi: profile.rule?.requires_midi ?? false, app } : null })} /></Field>
        {profile.rule && <Field label="Prioridad (−100 a 100)"><input type="number" min={-100} max={100} value={profile.rule.priority} onChange={event => update({ ...profile, rule: { ...profile.rule!, priority: Number(event.target.value) } })} /></Field>}
      </div>
      {profile.rule && <label className="check"><input type="checkbox" checked={profile.rule.requires_midi} onChange={event => update({ ...profile, rule: { ...profile.rule!, requires_midi: event.target.checked } })} /> Requiere conexión MIDI</label>}
      <p className="hint">Estado guardado · {view.profile_reason}</p>
    </Panel>
  </>;
}
