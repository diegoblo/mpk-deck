import type { ReactNode } from "react";
import type { Action, Config, Input, Profile, ProfileProps, Shared, Snapshot } from "./types";

const paths = {
  board: <><rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/></>,
  link: <><path d="M10 13a4 4 0 0 0 6 .5l3-3a4 4 0 0 0-5.7-5.7l-1.7 1.7"/><path d="M14 11a4 4 0 0 0-6-.5l-3 3a4 4 0 0 0 5.7 5.7l1.7-1.7"/></>,
  midi: <><rect x="3" y="5" width="18" height="14" rx="2"/><path d="M9 5v14m6-14v14M7 5v7m4-7v7m6-7v7"/></>,
  mix: <path d="M5 3v6m0 4v8m7-18v12m0 4v2m7-18v2m0 4v12M2 9h6m1 10h6m1-14h6"/>,
  macro: <path d="m13 2-9 12h7l-1 8 10-12h-7l1-8Z"/>,
  profiles: <path d="m12 3 9 5-9 5-9-5 9-5Zm-9 9 9 5 9-5m-18 4 9 5 9-5"/>,
  play: <path d="m8 5 12 7-12 7V5Z"/>,
  stop: <rect x="6" y="6" width="12" height="12" rx="1"/>,
  sound: <path d="M3 9v6h4l5 4V5L7 9H3Zm13 0a4 4 0 0 1 0 6m2-9a8 8 0 0 1 0 12"/>,
  add: <path d="M12 5v14M5 12h14"/>,
  close: <path d="m6 6 12 12M18 6 6 18"/>,
  loop: <path d="m16 3 4 4-4 4m4-4H8a4 4 0 0 0-4 4m4 10-4-4 4-4m-4 4h12a4 4 0 0 0 4-4"/>,
  headphones: <path d="M3 14v-2a9 9 0 0 1 18 0v2M3 14v4a2 2 0 0 0 2 2h2v-6H3Zm18 0v4a2 2 0 0 1-2 2h-2v-6h4Z"/>,
  alert: <><path d="M12 8v5m0 3h.01"/><circle cx="12" cy="12" r="9"/></>,
} satisfies Record<string, ReactNode>;

export type IconName = keyof typeof paths;

export function Icon({ name, size = 19 }: { name: IconName; size?: number }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name]}</svg>;
}
export function Field({ label, children }: { label: string; children: ReactNode }) {
  return <label className="field"><span>{label}</span>{children}</label>;
}
export function Panel({ title, subtitle, children }: { title: string; subtitle?: string; children: ReactNode }) {
  return <section className="panel"><h2>{title}</h2>{subtitle && <p className="muted panel-intro">{subtitle}</p>}{children}</section>;
}
export function Empty({ icon, title, children }: { icon: IconName; title: string; children: ReactNode }) {
  return <div className="empty"><span className="empty-icon"><Icon name={icon} size={27} /></span><h2>{title}</h2>{children}</div>;
}

export const appName = (path: string) => path.split(/[\\/]/).at(-1) ?? path;
export const inputName = (input: Input) => `${input.kind === "note" ? "Nota" : "CC"} ${input.number} · Canal ${input.channel + 1}`;
export const percent = (level: number) => Math.round(level * 100);
export const makeProfile = (name: string): Profile => ({ id: crypto.randomUUID(), name, rule: null, bindings: [], macros: [] });

export function copyProfile(profile: Profile): Profile {
  const copy = structuredClone(profile);
  copy.id = crypto.randomUUID();
  copy.name = `${profile.name.slice(0, 42)} · copia`;
  const ids = new Map<string, string>();
  for (const macro of copy.macros) {
    const previous = macro.id;
    macro.id = crypto.randomUUID();
    ids.set(previous, macro.id);
  }
  const retarget = (action: Action) => {
    if (action.kind === "profile" && action.profile === profile.id) action.profile = copy.id;
  };
  for (const binding of copy.bindings) {
    if (binding.target.kind === "macro") binding.target.id = ids.get(binding.target.id) ?? binding.target.id;
    else retarget(binding.target.action);
  }
  for (const macro of copy.macros) for (const step of macro.steps) retarget(step.action);
  return copy;
}

export function AppSelect({ view, value, onChange, empty = "Selecciona una aplicación" }: {
  view: Snapshot; value: string; onChange: (app: string) => void; empty?: string;
}) {
  const options = new Set([...(value ? [value] : []), ...(view.captured_app ? [view.captured_app] : []), ...view.apps.map(app => app.id)]);
  const names = [...options].map(appName);
  return <select value={value} onChange={event => onChange(event.target.value)}>
    <option value="">{empty}</option>
    {[...options].map(app => <option key={app} value={app}>{names.filter(name => name === appName(app)).length > 1 ? app : appName(app)}</option>)}
  </select>;
}

export function ProfilePicker({ config, profile, select }: ProfileProps) {
  return <Field label="Perfil en edición"><select value={profile.id} onChange={event => select(event.target.value)}>
    {config.profiles.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
  </select></Field>;
}

export function Capture({ view, busy, run }: Shared) {
  return <div className="toolbar wrap">
    <button disabled={busy || view.capturing} onClick={() => void run({ kind: "capture_app" })}>{view.capturing ? "Cambia de ventana ahora…" : "Capturar otra aplicación"}</button>
    <span className="muted">{view.captured_app ? `Capturada · ${appName(view.captured_app)}` : "Dispones de 4 segundos para cambiar de ventana."}</span>
  </div>;
}

const actionNames: Record<Action["kind"], string> = {
  play: "Reproducir sonido", stop: "Detener audio", volume: "Ajustar volumen",
  master_volume: "Ajustar volumen general",
  mute: "Alternar silencio", profile: "Fijar perfil",
};

export function ActionEditor({ action, config, view, onChange }: {
  action: Action; config: Config; view: Snapshot; onChange: (action: Action) => void;
}) {
  function choose(kind: Action["kind"]) {
    switch (kind) {
      case "stop": onChange({ kind }); break;
      case "play": onChange({ kind, sound: config.sounds[0]?.id ?? "", repeat: false, preview: !config.destination }); break;
      case "volume": onChange({ kind, app: view.apps[0]?.id ?? "", min: 0, max: 1 }); break;
      case "master_volume": onChange({ kind, min: 0, max: 1 }); break;
      case "mute": onChange({ kind, app: view.apps[0]?.id ?? "" }); break;
      case "profile": onChange({ kind, profile: config.manual_profile }); break;
    }
  }
  return <div className="form-grid">
    <Field label="Acción"><select value={action.kind} onChange={event => choose(event.target.value as Action["kind"])}>
      {Object.entries(actionNames).map(([kind, name]) => <option key={kind} value={kind}>{name}</option>)}
    </select></Field>
    {action.kind === "play" && <>
      <Field label="Sonido"><select value={action.sound} onChange={event => onChange({ ...action, sound: event.target.value })}>
        <option value="">Selecciona un sonido</option>
        {config.sounds.map(sound => <option value={sound.id} key={sound.id}>{sound.name}</option>)}
      </select></Field>
      <Field label="Salida"><select value={action.preview ? "preview" : "destination"} onChange={event => onChange({ ...action, preview: event.target.value === "preview" })}>
        <option value="preview">Preescucha local</option>
        <option value="destination">Salida para Discord</option>
      </select></Field>
      <label className="check"><input type="checkbox" checked={action.repeat} onChange={event => onChange({ ...action, repeat: event.target.checked })} /> Repetir hasta detener</label>
    </>}
    {(action.kind === "volume" || action.kind === "mute") && <Field label="Aplicación">
      <AppSelect view={view} value={action.app} onChange={app => onChange({ ...action, app })} />
    </Field>}
    {(action.kind === "volume" || action.kind === "master_volume") && <>
      <Field label="Mínimo (%)"><input type="number" min={0} max={100} value={percent(action.min)} onChange={event => onChange({ ...action, min: Number(event.target.value) / 100 })} /></Field>
      <Field label="Máximo (%)"><input type="number" min={0} max={100} value={percent(action.max)} onChange={event => onChange({ ...action, max: Number(event.target.value) / 100 })} /></Field>
    </>}
    {action.kind === "master_volume" && <p className="hint">Controla el volumen de la salida predeterminada de Windows.</p>}
    {action.kind === "profile" && <Field label="Perfil"><select value={action.profile} onChange={event => onChange({ ...action, profile: event.target.value })}>
      {config.profiles.map(profile => <option value={profile.id} key={profile.id}>{profile.name}</option>)}
    </select></Field>}
  </div>;
}
