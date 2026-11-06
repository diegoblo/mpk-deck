export type Fault = { code: string; message: string };
export type Input = { kind: "note" | "cc"; channel: number; number: number };
export type Mode = "press" | "momentary" | "toggle" | "continuous";
export type Action =
  | { kind: "play"; sound: string; repeat: boolean; preview?: boolean }
  | { kind: "stop" }
  | { kind: "volume"; app: string; min: number; max: number }
  | { kind: "master_volume"; min: number; max: number }
  | { kind: "mute"; app: string }
  | { kind: "profile"; profile: string };
export type Target = { kind: "action"; action: Action } | { kind: "macro"; id: string };
export type SurfaceControl =
  | { kind: "pad_a" | "pad_b" | "knob" | "key"; index: number }
  | { kind: "joystick_x" | "joystick_y" | "pedal" };
export type Binding = { input: Input; mode: Mode; target: Target; surface?: SurfaceControl };
export type Step = { delay_ms: number; action: Action };
export type Macro = { id: string; name: string; enabled: boolean; foreground: string | null; steps: Step[] };
export type Rule = { app: string; priority: number; requires_midi: boolean };
export type Profile = { id: string; name: string; bindings: Binding[]; macros: Macro[]; rule: Rule | null };
export type Sound = { id: string; name: string; level: number };
export type Config = {
  version: number; revision: number; midi_port: string | null;
  preview: string | null; destination: string | null; sounds: Sound[]; profiles: Profile[];
  voice: { microphone: string | null; level: number };
  manual_profile: string; automatic: boolean; macros_enabled: boolean; delay_ms: number;
};
export type SessionApp = {
  id: string; name: string; volume: number; muted: boolean; mixed: boolean; sessions: number; active: number;
};
export type Snapshot = {
  epoch: number; config: Config;
  ports: { id: string; name: string }[];
  midi_connected: boolean; midi_error: Fault | null; outputs: string[];
  microphones: string[]; virtual_output: string | null; voice_active: boolean;
  apps: SessionApp[]; mixer_error: Fault | null; active_profile: string;
  profile_reason: string; monitoring: boolean;
  log: { elapsed_ms: number; message: { input: Input; value: number; release: boolean }; outcome: string }[];
  dropped: number; learning: boolean; learned: Input | null; capturing: boolean;
  captured_app: string | null; macro_running: boolean; activity: string | null; error: Fault | null;
};
export type Command =
  | { kind: "voice"; enabled: boolean }
  | { kind: "snapshot" | "import_sound" | "import_profile" | "stop" | "clear" | "learn" | "cancel_learn" | "capture_app" | "cancel_macro" }
  | { kind: "save"; config: Config }
  | { kind: "export_profile"; id: string }
  | { kind: "play"; id: string; preview: boolean; repeat: boolean }
  | { kind: "mix"; app: string; volume: number } | { kind: "mix"; app: string; mute: boolean }
  | { kind: "monitor"; enabled: boolean }
  | { kind: "run_macro"; profile: string; id: string };
export type Run = (command: Command) => Promise<Snapshot | undefined>;
export type Edit = (update: (config: Config) => Config) => void;
export type Shared = { view: Snapshot; config: Config; run: Run; edit: Edit; busy: boolean };
export type ProfileProps = Shared & { profile: Profile; update: (value: Profile) => void; select: (id: string) => void };
