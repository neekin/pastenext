import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useI18n } from "../i18n";
import { pins, type PinInfo, type PinAction } from "./api";
import { imagePoint, renderAnnotatedImage, type NumberMark } from "./annotations";
import "./pins.css";

export default function PinWindow() {
  const { t } = useI18n();
  const [info, setInfo] = useState<PinInfo | null>(null);
  const [image, setImage] = useState("");
  const [marking, setMarking] = useState(false);
  const marks = info?.marks ?? [];
  const [dimensions, setDimensions] = useState({ width: 0, height: 0 });
  const [copying, setCopying] = useState(false);
  const [copied, setCopied] = useState(false);
  const imageRef = useRef<HTMLImageElement>(null);
  const [error, setError] = useState("");
  const surface = useRef<HTMLDivElement>(null);
  const chain = useRef(Promise.resolve());
  const label = getCurrentWindow().label;
  // Serialize input so rapid wheel/slider events cannot overwrite newer native state.
  const enqueue = useCallback((operation: () => Promise<void>) => {
    chain.current = chain.current.then(operation)
      .then(() => setError("")).catch((e) => setError(String(e)));
  }, []);
  const act = useCallback((action: PinAction, value?: number) => {
    enqueue(() => pins.action(label, action, value));
  }, [enqueue, label]);
  const annotate = (action: "add" | "undo" | "clear", mark?: NumberMark) => {
    setCopied(false);
    enqueue(() => pins.mark(label, action, mark));
  };

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    let stopErrors: (() => void) | undefined;
    void (async () => {
      const stop = await listen<PinInfo>("pin-changed", (event) => {
        if (alive && event.payload.label === label) setInfo(event.payload);
      });
      if (!alive) { stop(); return; }
      unlisten = stop;
      const stopSaveErrors = await listen<string>("pin-save-error", (event) => {
        if (alive) setError(String(event.payload));
      });
      if (!alive) { stopSaveErrors(); return; }
      stopErrors = stopSaveErrors;
      const content = await pins.content();
      if (alive) { setInfo(content.info); setImage(content.image); }
    })().catch((e) => {
      if (alive) {
        setError(String(e));
        // Show a recoverable error rather than leaving an invisible pin behind.
        void pins.action(label, "ready").catch(() => {});
      }
    });
    return () => { alive = false; unlisten?.(); stopErrors?.(); };
  }, [label]);

  useEffect(() => {
    const node = surface.current;
    if (!node) return;
    let delta = 0;
    let frame = 0;
    const wheel = (event: WheelEvent) => {
      if ((event.target as HTMLElement).closest("button, input")) return;
      event.preventDefault();
      delta += event.deltaY * (event.deltaMode === 1 ? 16 : 1);
      if (!frame) frame = requestAnimationFrame(() => {
        act("zoom", Math.exp(-Math.max(-100, Math.min(100, delta)) * 0.003));
        delta = 0; frame = 0;
      });
    };
    node.addEventListener("wheel", wheel, { passive: false });
    return () => { node.removeEventListener("wheel", wheel); cancelAnimationFrame(frame); };
  }, [act]);

  const ready = () => {
    const img = imageRef.current;
    if (img) setDimensions({ width: img.naturalWidth, height: img.naturalHeight });
    act("ready");
  };
  const copy = async () => {
    if (!imageRef.current || copying) return;
    setCopying(true);
    setCopied(false);
    try {
      await chain.current;
      const content = await pins.content();
      await pins.copy(renderAnnotatedImage(imageRef.current, content.info.marks));
      setError(""); setCopied(true);
    } catch (e) { setError(String(e)); }
    finally { setCopying(false); }
  };
  return (
    <div ref={surface} className={`pin-window ${info?.clickThrough ? "pin-through" : ""}`} tabIndex={0}
      onContextMenu={(e) => e.preventDefault()}
      onKeyDown={(e) => {
        if (e.target instanceof HTMLInputElement) return;
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z") {
          e.preventDefault(); annotate("undo"); return;
        }
        if (e.key === "Escape" && marking) { e.preventDefault(); setMarking(false); return; }
        if (e.key === "Escape") { e.preventDefault(); act("hide"); }
        if (e.key === "+" || e.key === "=") { e.preventDefault(); act("zoom", 1.1); }
        if (e.key === "-") { e.preventDefault(); act("zoom", 1 / 1.1); }
      }}>
      <div className={`pin-image-area ${marking ? "pin-marking" : ""}`} title={t(marking ? "pins.numberHint" : "pins.dragHint")}
        onMouseDown={(e) => {
          if (e.button === 0 && !marking) void getCurrentWindow().startDragging().catch((err) => setError(String(err)));
        }}>
        <div className="pin-image-content" style={{ width: (info?.width ?? 0) * (info?.zoom ?? 1), height: (info?.height ?? 0) * (info?.zoom ?? 1) }}
          onClick={(e) => {
            if (!marking) return;
            const box = e.currentTarget.getBoundingClientRect();
            const point = imagePoint(e.clientX - box.left, e.clientY - box.top, box, dimensions);
            if (point) annotate("add", point);
          }}>
        {image && <img ref={imageRef} src={image} alt={t("pins.image")} draggable={false}
          style={{ opacity: info?.opacity ?? 1 }} onLoad={ready}
          onError={() => { setError(t("pins.imageError")); void pins.action(label, "ready").catch(() => {}); }} />}
        {dimensions.width > 0 && <svg className="pin-markers" style={{ opacity: info?.opacity ?? 1 }}
          viewBox={`0 0 ${dimensions.width} ${dimensions.height}`} aria-label={t("pins.numbers")}>
          {marks.map((mark, index) => <g key={index}>
            <circle cx={mark.x} cy={mark.y} r={mark.radius} fill="#ef4444" stroke="white" strokeWidth={mark.radius * 0.12} />
            <text x={mark.x} y={mark.y + mark.radius * 0.06} fill="white" fontWeight="700"
              fontSize={mark.radius * (index >= 99 ? 0.9 : 1.15)} textAnchor="middle" dominantBaseline="central">{index + 1}</text>
          </g>)}
        </svg>}
        </div>
      </div>
      <div className="pin-controls">
        <div className="pin-control-row">
          <button className={`pin-text-button ${marking ? "pin-tool-active" : ""}`} aria-pressed={marking}
            title={t("pins.numberHint")} onClick={() => setMarking(!marking)}>{t("pins.numbers")} {marks.length + 1}</button>
          <button className="pin-text-button" disabled={!marks.length} onClick={() => { annotate("undo"); }}>{t("pins.undo")}</button>
          <button className="pin-text-button" disabled={!marks.length} onClick={() => { annotate("clear"); }}>{t("pins.clear")}</button>
          <span className="pin-spacer" />
          <button className="pin-text-button" disabled={copying || !dimensions.width} onClick={() => void copy()}>{t(copied ? "pins.copied" : "pins.copy")}</button>
        </div>
        <div className="pin-control-row">
          <button title={t("pins.zoomOut")} aria-label={t("pins.zoomOut")} onClick={() => act("zoom", 1 / 1.1)}>−</button>
          <span className="pin-scale">{Math.round((info?.zoom ?? 1) * 100)}%</span>
          <button title={t("pins.zoomIn")} aria-label={t("pins.zoomIn")} onClick={() => act("zoom", 1.1)}>+</button>
          <label className="pin-opacity" title={t("pins.opacity")}>
            <span>◐</span>
            <input aria-label={t("pins.opacity")} type="range" min="15" max="100"
              value={Math.round((info?.opacity ?? 1) * 100)} onChange={(e) => act("opacity", Number(e.target.value) / 100)} />
            <span>{Math.round((info?.opacity ?? 1) * 100)}%</span>
          </label>
        </div>
        <div className="pin-control-row">
          <button className="pin-text-button" title={t("pins.throughHint")} onClick={() => act("through")}>{t("pins.through")}</button>
          <span className="pin-spacer" />
          <button className="pin-text-button" onClick={() => act("hide")}>{t("pins.hide")}</button>
          <button className="pin-text-button" onClick={() => act("close")}>{t("pins.close")}</button>
        </div>
      </div>
      {error && <div className="pin-error" role="alert">{t("pins.failed")}: {error}</div>}
    </div>
  );
}
