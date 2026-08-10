import { useEffect, useRef, useState } from "react";

const CANVAS_W = 560;
const CANVAS_H = 220;
const COLORS: { value: string; label: string }[] = [
  { value: "#111111", label: "Negro" },
  { value: "#1a4fd6", label: "Azul" },
];

/**
 * Modal para dibujar una firma a mano alzada sobre un canvas transparente.
 * Al guardar recorta al contenido y devuelve el PNG en base64 (sin prefijo
 * data:) junto con el nombre elegido.
 */
export default function DibujarFirma({
  onSave,
  onClose,
}: {
  onSave: (name: string, pngBase64: string) => void;
  onClose: () => void;
}) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const drawingRef = useRef(false);
  const lastRef = useRef<{ x: number; y: number } | null>(null);
  const [name, setName] = useState("Mi firma");
  const [color, setColor] = useState(COLORS[0].value);
  const [empty, setEmpty] = useState(true);
  const colorRef = useRef(color);
  colorRef.current = color;

  // canvas nítido en pantallas retina
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = CANVAS_W * dpr;
    canvas.height = CANVAS_H * dpr;
    const ctx = canvas.getContext("2d");
    if (ctx) ctx.scale(dpr, dpr);
  }, []);

  function pos(e: React.PointerEvent<HTMLCanvasElement>) {
    const rect = e.currentTarget.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  }

  function onDown(e: React.PointerEvent<HTMLCanvasElement>) {
    e.currentTarget.setPointerCapture(e.pointerId);
    drawingRef.current = true;
    lastRef.current = pos(e);
  }

  function onMove(e: React.PointerEvent<HTMLCanvasElement>) {
    if (!drawingRef.current || !lastRef.current) return;
    const ctx = canvasRef.current?.getContext("2d");
    if (!ctx) return;
    const p = pos(e);
    const last = lastRef.current;
    // trazo suavizado: curva cuadrática hacia el punto medio
    const mid = { x: (last.x + p.x) / 2, y: (last.y + p.y) / 2 };
    ctx.strokeStyle = colorRef.current;
    ctx.lineWidth = 2.4;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.beginPath();
    ctx.moveTo(last.x, last.y);
    ctx.quadraticCurveTo(last.x, last.y, mid.x, mid.y);
    ctx.lineTo(p.x, p.y);
    ctx.stroke();
    lastRef.current = p;
    setEmpty(false);
  }

  function onUp() {
    drawingRef.current = false;
    lastRef.current = null;
  }

  function clear() {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    setEmpty(true);
  }

  /** Recorta el canvas al contenido dibujado (más margen) y exporta PNG. */
  function exportPng(): string | null {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return null;
    const { width, height } = canvas;
    const data = ctx.getImageData(0, 0, width, height).data;
    let minX = width;
    let minY = height;
    let maxX = -1;
    let maxY = -1;
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        if (data[(y * width + x) * 4 + 3] > 0) {
          if (x < minX) minX = x;
          if (x > maxX) maxX = x;
          if (y < minY) minY = y;
          if (y > maxY) maxY = y;
        }
      }
    }
    if (maxX < 0) return null;
    const pad = 10;
    minX = Math.max(0, minX - pad);
    minY = Math.max(0, minY - pad);
    maxX = Math.min(width - 1, maxX + pad);
    maxY = Math.min(height - 1, maxY + pad);
    const out = document.createElement("canvas");
    out.width = maxX - minX + 1;
    out.height = maxY - minY + 1;
    out
      .getContext("2d")
      ?.drawImage(
        canvas,
        minX,
        minY,
        out.width,
        out.height,
        0,
        0,
        out.width,
        out.height,
      );
    return out.toDataURL("image/png").replace(/^data:image\/png;base64,/, "");
  }

  function save() {
    const png = exportPng();
    if (!png) return;
    onSave(name.trim() || "Mi firma", png);
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal modal-firma"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
        }}
      >
        <h3>Dibujar firma</h3>
        <canvas
          ref={canvasRef}
          className="firma-canvas"
          style={{ width: CANVAS_W, height: CANVAS_H, touchAction: "none" }}
          onPointerDown={onDown}
          onPointerMove={onMove}
          onPointerUp={onUp}
          onPointerLeave={onUp}
        />
        <div className="card-row">
          <input
            type="text"
            className="firma-name"
            placeholder="Nombre de la firma"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <select
            className="size-select"
            title="Color del trazo"
            value={color}
            onChange={(e) => setColor(e.target.value)}
          >
            {COLORS.map((c) => (
              <option key={c.value} value={c.value}>
                {c.label}
              </option>
            ))}
          </select>
          <button className="btn" disabled={empty} onClick={clear}>
            Borrar
          </button>
        </div>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn btn-primary" disabled={empty} onClick={save}>
            Guardar firma
          </button>
        </div>
      </div>
    </div>
  );
}
