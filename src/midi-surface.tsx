import { useLayoutEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import type { Binding, SurfaceControl } from "./types";
import { Icon, inputName } from "./ui";

export const surfaceId = (control: SurfaceControl) =>
  "index" in control ? `${control.kind}-${control.index}` : control.kind;

export function surfaceName(control: SurfaceControl): string {
  switch (control.kind) {
    case "pad_a": return `Pad ${control.index} · Banco A`;
    case "pad_b": return `Pad ${control.index} · Banco B`;
    case "knob": return `Mando K${control.index}`;
    case "key": return `Tecla ${control.index}`;
    case "joystick_x": return "Joystick · Eje X";
    case "joystick_y": return "Joystick · Eje Y";
    case "pedal": return "Pedal de sustain";
  }
}

export type SurfaceSelection = { control: SurfaceControl; anchor: SVGGElement };
const whiteKeys = [1, 3, 5, 6, 8, 10, 12, 13, 15, 17, 18, 20, 22, 24, 25];
const blackKeys = [2, 4, 7, 9, 11, 14, 16, 19, 21, 23];
const keyWidth = 44;

export function MidiSurface({ bindings, selected, onSelect }: {
  bindings: Binding[]; selected?: SurfaceControl; onSelect: (selection: SurfaceSelection) => void;
}) {
  const [bank, setBank] = useState<"pad_a" | "pad_b">("pad_a");
  const mapped = new Map(bindings.flatMap(binding =>
    binding.surface ? [[surfaceId(binding.surface), binding] as const] : []));

  function hotspot(control: SurfaceControl, artwork: ReactNode, markX: number, markY: number) {
    const id = surfaceId(control);
    const binding = mapped.get(id);
    const label = surfaceName(control);
    const active = selected && id === surfaceId(selected);
    const open = (anchor: SVGGElement) => onSelect({ control, anchor });
    return <g key={id} role="button" tabIndex={0} aria-haspopup="dialog" aria-expanded={!!active}
      aria-label={binding ? `${label}, asignado a ${inputName(binding.input)}` : `${label}, sin asignar`}
      className={`surface-control${binding ? " mapped" : ""}${active ? " selected" : ""}`}
      onClick={event => open(event.currentTarget)}
      onKeyDown={event => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          if (!event.repeat) open(event.currentTarget);
        }
      }}>
      <title>{binding ? `${label} · ${inputName(binding.input)}` : label}</title>
      {artwork}
      {binding && <circle className="surface-mark" cx={markX} cy={markY} r="3.2" />}
    </g>;
  }

  return <>
    <div className="surface-toolbar">
      <div className="surface-bank" role="group" aria-label="Banco de pads">
        <span>Banco de pads</span>
        <button type="button" aria-pressed={bank === "pad_a"} onClick={() => setBank("pad_a")}>A</button>
        <button type="button" aria-pressed={bank === "pad_b"} onClick={() => setBank("pad_b")}>B</button>
      </div>
      <span className="surface-count"><span aria-hidden="true" /> {mapped.size} en el mapa</span>
    </div>
    <div className="surface-sheet">
      <svg className="surface-drawing" viewBox="0 0 760 407" role="group" aria-label="Dibujo interactivo del MPK Mini MK3">
        <g className="surface-construction" aria-hidden="true">
          <path d="M31 45V23m698 22V23M31 32h698M20 55H9m11 298H9m7-298v298" />
          <path d="m27 36 8-8m690 8 8-8M12 59l8-8m-8 306 8-8" />
          <text x="380" y="24" textAnchor="middle">317,5 mm</text>
        </g>
        <rect className="surface-case" x="31" y="55" width="698" height="298" rx="15" />
        <path className="surface-guide" d="M43 211h674M247 71v124m257-124v124" />
        <g className="surface-print" aria-hidden="true">
          <text x="55" y="78">AKAI MPK MINI MK3</text>
          <text x="264" y="78">PADS / BANCO {bank === "pad_a" ? "A" : "B"}</text>
          <text x="521" y="78">MANDOS</text>
          <rect x="150" y="90" width="74" height="31" rx="3" />
          <text x="187" y="109" textAnchor="middle">OLED</text>
          <path d="M56 184h168" />
          <rect x="57" y="157" width="46" height="15" rx="3" />
          <rect x="117" y="157" width="46" height="15" rx="3" />
          <rect x="177" y="157" width="46" height="15" rx="3" />
          <text x="80" y="168" textAnchor="middle">OCT</text>
          <text x="140" y="168" textAnchor="middle">ARP</text>
          <text x="200" y="168" textAnchor="middle">PROG</text>
          <text x="56" y="201">TECLADO · 25 TECLAS</text>
        </g>
        <g className="surface-print" aria-hidden="true">
          <circle cx="81" cy="114" r="23" />
          <circle cx="81" cy="114" r="10" />
          <path d="M81 96v7m0 22v7m-18-18h7m22 0h7" />
        </g>
        {hotspot({ kind: "joystick_x" }, <><rect className="surface-shape" x="109" y="98" width="29" height="27" rx="5" /><text x="123.5" y="115">X</text></>, 134, 102)}
        {hotspot({ kind: "joystick_y" }, <><rect className="surface-shape" x="109" y="126" width="29" height="27" rx="5" /><text x="123.5" y="143">Y</text></>, 134, 130)}
        {[5, 6, 7, 8, 1, 2, 3, 4].map((index, position) => {
          const x = 263 + position % 4 * 59;
          const y = 89 + Math.floor(position / 4) * 56;
          return hotspot({ kind: bank, index }, <>
            <rect className="surface-shape" x={x} y={y} width="50" height="48" rx="5" />
            <rect className="surface-detail" x={x + 5} y={y + 5} width="40" height="38" rx="3" />
            <text x={x + 25} y={y + 28}>{index}</text>
          </>, x + 43, y + 7);
        })}
        {Array.from({ length: 8 }, (_, position) => {
          const x = 542 + position % 4 * 48;
          const y = 112 + Math.floor(position / 4) * 58;
          return hotspot({ kind: "knob", index: position + 1 }, <>
            <circle className="surface-shape" cx={x} cy={y} r="18" />
            <circle className="surface-detail" cx={x} cy={y} r="12" />
            <path className="surface-detail" d={`M${x} ${y - 18}v10`} />
            <text x={x} y={y + 4}>K{position + 1}</text>
          </>, x + 13, y - 13);
        })}
        {whiteKeys.map((index, position) => {
          const x = 50 + position * keyWidth;
          return hotspot({ kind: "key", index },
            <rect className="surface-shape surface-key" x={x} y="219" width={keyWidth} height="120" rx="2" />,
            x + keyWidth / 2, 326);
        })}
        {blackKeys.map(index => {
          const x = 50 + (whiteKeys.findIndex(key => key > index)) * keyWidth;
          return hotspot({ kind: "key", index },
            <rect className="surface-shape surface-sharp" x={x - 13} y="219" width="26" height="70" rx="2" />,
            x, 280);
        })}
        <g className="surface-construction" aria-hidden="true">
          <path d="M31 372h127m50 0h521m-698-6v12m698-12v12" />
          <text x="31" y="394">VISTA SUPERIOR</text>
          <text x="729" y="394" textAnchor="end">SUPERFICIE MIDI · 01</text>
        </g>
        {hotspot({ kind: "pedal" }, <><rect className="surface-shape" x="160" y="359" width="77" height="26" rx="5" /><text x="198.5" y="375">PEDAL</text></>, 229, 366)}
      </svg>
    </div>
    <p className="hint surface-help">Elige el mismo banco en el MPK y en el dibujo. El joystick se puede asignar cuando envía CC.</p>
  </>;
}

export function ControlPopover({ selection, onDismiss, children }: {
  selection: SurfaceSelection; onDismiss: () => void; children: ReactNode;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const { anchor, control } = selection;
  useLayoutEffect(() => {
    const node = dialog.current!;
    node.showModal();
    const margin = 12;
    const gap = 13;
    const fit = (value: number, size: number, viewport: number) =>
      Math.max(margin, Math.min(value, viewport - size - margin));
    const position = () => {
      const rect = anchor.getBoundingClientRect();
      const { offsetWidth: width, offsetHeight: height } = node;
      const centerX = rect.left + rect.width / 2;
      const centerY = rect.top + rect.height / 2;
      const side = rect.right + gap + width + margin <= window.innerWidth ? "right"
        : rect.left - gap - width >= margin ? "left"
        : rect.bottom + gap + height + margin <= window.innerHeight ? "below" : "above";
      const horizontal = side === "left" || side === "right";
      const left = fit(side === "right" ? rect.right + gap : side === "left" ? rect.left - width - gap : centerX - width / 2, width, window.innerWidth);
      const top = fit(side === "below" ? rect.bottom + gap : side === "above" ? rect.top - height - gap : centerY - height / 2, height, window.innerHeight);
      const pin = Math.max(14, Math.min(horizontal ? height - 14 : width - 14, horizontal ? centerY - top : centerX - left));
      node.style.left = `${left}px`;
      node.style.top = `${top}px`;
      node.style.setProperty("--surface-pin", `${pin}px`);
      node.dataset.side = side;
    };
    const observer = new ResizeObserver(position);
    observer.observe(node);
    if (anchor.ownerSVGElement) observer.observe(anchor.ownerSVGElement);
    window.addEventListener("resize", position);
    position();
    return () => {
      window.removeEventListener("resize", position);
      observer.disconnect();
      node.close();
      if (anchor.isConnected) anchor.focus({ preventScroll: true });
    };
  }, [anchor]);

  return <dialog ref={dialog} className="surface-dialog" aria-labelledby="surface-control-title"
    onCancel={event => { event.preventDefault(); onDismiss(); }}
    onClick={event => { if (event.target === event.currentTarget) onDismiss(); }}>
    <div className="surface-dialog-content">
      <div className="editor-head">
        <div><p className="eyebrow">Asignación MIDI</p><h2 id="surface-control-title">{surfaceName(control)}</h2></div>
        <button type="button" className="icon-button" aria-label="Cerrar asignación" onClick={onDismiss}><Icon name="close" size={17} /></button>
      </div>
      {children}
    </div>
  </dialog>;
}
