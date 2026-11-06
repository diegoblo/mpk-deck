import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createRoot } from "react-dom/client";
import type { Config, Edit, Fault, Run, Snapshot } from "./types";
import { Icon } from "./ui";
import type { IconName } from "./ui";
import { Board, Connections, Mixer } from "./audio-pages";
import { Midi, Macros, Profiles } from "./profile-pages";
import "./style.css";

const pages = [
  { id: "board", icon: "board", label: "Soundboard", detail: "Biblioteca y reproducción" },
  { id: "mix", icon: "mix", label: "Mezclador", detail: "Volumen por aplicación" },
  { id: "midi", icon: "midi", label: "Asignaciones MIDI", detail: "Aprendizaje y diagnóstico" },
  { id: "macros", icon: "macro", label: "Macros", detail: "Secuencias y contexto" },
  { id: "profiles", icon: "profiles", label: "Perfiles", detail: "Modos y automatización" },
  { id: "connections", icon: "link", label: "Conexiones", detail: "Dispositivo y rutas de audio" },
] as const satisfies ReadonlyArray<{ id: string; icon: IconName; label: string; detail: string }>;
type Page = typeof pages[number]["id"];

function fault(error: unknown): string {
  if (typeof error === "object" && error && "message" in error) return (error as Fault).message;
  return typeof error === "string" ? error : "La operación no se pudo completar.";
}

function Deck() {
  const [view, setView] = useState<Snapshot>();
  const [error, setError] = useState<string>();
  const [draft, setDraft] = useState<Config>();
  const [page, setPage] = useState<Page>("board");
  const [selected, setSelected] = useState("");
  const [busy, setBusy] = useState(false);
  const locked = useRef(false);
  const accept = (next: Snapshot) => setView(previous => !previous || next.epoch >= previous.epoch ? next : previous);

  useEffect(() => {
    let alive = true;
    const unlisten = listen<Snapshot>("deck-state", event => { if (alive) accept(event.payload); });
    unlisten.then(() => invoke<Snapshot>("deck", { command: { kind: "snapshot" } }))
      .then(snapshot => { if (alive) accept(snapshot); })
      .catch(error => { if (alive) setError(fault(error)); });
    return () => { alive = false; void unlisten.then(stop => stop()).catch(() => {}); };
  }, []);

  const run: Run = async command => {
    const emergency = command.kind === "stop" || command.kind === "cancel_macro" || (command.kind === "voice" && !command.enabled);
    if (locked.current && !emergency) return;
    if (!emergency) { locked.current = true; setBusy(true); }
    setError(undefined);
    try {
      const snapshot = await invoke<Snapshot>("deck", { command });
      accept(snapshot);
      return snapshot;
    } catch (error) {
      setError(fault(error));
    } finally {
      if (!emergency) { locked.current = false; setBusy(false); }
    }
  };

  if (!view) return <main className="boot"><span className="brand-mark"><Icon name="board" size={25} /></span><h1>MPK Deck</h1>
    {error ? <><p role="alert">{error}</p><button onClick={() => void run({ kind: "snapshot" })}>Reintentar</button></> : <p role="status">Conectando con el motor…</p>}
  </main>;

  const config = draft ?? view.config;
  const profile = config.profiles.find(profile => profile.id === selected) ?? config.profiles[0];
  const active = view.config.profiles.find(profile => profile.id === view.active_profile);
  const edit: Edit = update => { if (!locked.current) setDraft(draft => update(draft ?? view.config)); };
  const update = (value: typeof profile) => edit(config => ({ ...config, profiles: config.profiles.map(profile => profile.id === value.id ? value : profile) }));
  const save = async (config: Config) => { if (await run({ kind: "save", config })) setDraft(undefined); };
  const section = pages.find(item => item.id === page)!;
  const shared = { view, config, run, edit, busy };
  const profileProps = { ...shared, profile, update, select: setSelected };
  const notice = error ?? view.error?.message;
  const connectedName = view.ports.find(port => port.id === view.config.midi_port)?.name;
  return <div className="shell">
    <aside className="sidebar">
      <div className="brand"><span className="brand-mark"><Icon name="board" size={22} /></span><div><strong>MPK Deck</strong><small>CONTROL DE ESCRITORIO</small></div></div>
      <nav aria-label="Secciones">
        {pages.map(item => <button key={item.id} className={page === item.id ? "nav active" : "nav"} aria-current={page === item.id ? "page" : undefined} onClick={() => setPage(item.id)}>
          <Icon name={item.icon} />{item.label}
        </button>)}
      </nav>
      <div className="sidebar-foot">
        <span className={view.midi_connected ? "dot live" : "dot"} />
        <div><strong>{view.midi_connected ? connectedName ?? "MIDI conectado" : "MIDI sin conexión"}</strong>
          <small>{view.midi_connected ? "Entrada preparada" : "Configura el puerto en Conexiones"}</small></div>
      </div>
    </aside>
    <main className="main">
      <header className="topbar">
        <div><div className="eyebrow">{section.detail}</div><h1>{section.label}</h1></div>
        <div className="header-tools">
          <div className="active-profile" title={view.profile_reason}><Icon name="profiles" size={17} /><span>{active?.name ?? "Perfil activo"}<small>{view.profile_reason}</small></span></div>
          <button className="danger stop" onClick={() => void run({ kind: "stop" })}><Icon name="stop" size={16} /> Detener clips</button>
          {view.voice_active && <button className="danger" onClick={() => void run({ kind: "voice", enabled: false })}>Desactivar micrófono</button>}
        </div>
      </header>
      {draft && <div className="save-bar"><span>{draft.revision === view.config.revision ? "Cambios pendientes de guardar" : "La configuración guardada cambió. Descarta el borrador y vuelve a editar."}</span>
        <div className="toolbar"><button disabled={busy} onClick={() => setDraft(undefined)}>Descartar</button><button className="primary" disabled={busy || draft.revision !== view.config.revision} onClick={() => void save(draft)}>Guardar cambios</button></div>
      </div>}
      {notice && <div role="alert" className="notice"><Icon name="alert" size={18} />{notice}{error && <button className="icon-button" aria-label="Cerrar aviso" onClick={() => setError(undefined)}><Icon name="close" size={16} /></button>}</div>}
      <div className="workspace">
        {page === "board" && <Board {...shared} pending={!!draft} connect={() => setPage("connections")} />}
        {page === "connections" && <Connections {...shared} />}
        {page === "mix" && <Mixer {...shared} />}
        {page === "midi" && <Midi {...profileProps} />}
        {page === "macros" && <Macros {...profileProps} />}
        {page === "profiles" && <Profiles {...profileProps} pending={!!draft} />}
      </div>
      <footer className="status" role="status"><span className="dot live" />{view.activity ?? "Motor preparado"}</footer>
    </main>
  </div>;
}

createRoot(document.getElementById("root")!).render(<Deck />);
