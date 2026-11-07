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
  const copying = useRef(false);
  const menuOpen = useRef(false);
  const [feedback, setFeedback] = useState("");
  const imageRef = useRef<HTMLImageElement>(null);
  const [error, setError] = useState("");
  const surface = useRef<HTMLDivElement>(null);
  const chain = useRef(Promise.resolve());
  const label = getCurrentWindow().label;
  // Serialize image operations so rapid wheel input cannot overwrite newer state.
  const enqueue = useCallback((operation: () => Promise<void>) => {
    chain.current = chain.current.then(operation)
      .then(() => setError("")).catch((e) => setError(String(e)));
  }, []);
  const act = useCallback((action: PinAction, value?: number) => {
    enqueue(() => pins.action(label, action, value));
  }, [enqueue, label]);
  const annotate = (action: "add" | "undo" | "clear", mark?: NumberMark) => {
    enqueue(() => pins.mark(label, action, mark));
  };
  const copy = useCallback(async () => {
    if (!imageRef.current?.naturalWidth || copying.current) return;
    copying.current = true;
    try {
      await chain.current;
      const content = await pins.content();
      await pins.copy(renderAnnotatedImage(imageRef.current, content.info.marks));
      setError(""); setFeedback(t("pins.copied"));
    } catch (e) { setError(String(e)); }
    finally { copying.current = false; }
  }, [t]);

  useEffect(() => {
    let alive = true;
    const stops: (() => void)[] = [];
    const subscribe = async <T,>(name: string, callback: (value: T) => void) => {
      const stop = await listen<T>(name, event => { if (alive) callback(event.payload); });
      if (alive) stops.push(stop); else stop();
    };
    void (async () => {
      await subscribe<PinInfo>("pin-changed", value => { if (value.label === label) setInfo(value); });
      await subscribe<string>("pin-save-error", value => setError(String(value)));
      await subscribe<string>("pin-menu-action", action => {
        if (action === "copy") void copy();
        if (action === "number") setMarking(value => !value);
      });
      const content = await pins.content();
      if (alive) { setInfo(content.info); setImage(content.image); }
    })().catch((e) => {
      if (alive) {
        setError(String(e));
        void pins.action(label, "ready").catch(() => {});
      }
    });
    return () => { alive = false; stops.forEach(stop => stop()); };
  }, [label, copy]);

  useEffect(() => {
    if (!feedback) return;
    const timeout = setTimeout(() => setFeedback(""), 1500);
    return () => clearTimeout(timeout);
  }, [feedback]);

  useEffect(() => {
    const node = surface.current;
    if (!node) return;
    let delta = 0;
    let frame = 0;
    const wheel = (event: WheelEvent) => {
      event.preventDefault();
      if (menuOpen.current) return;
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
  const showMenu = async (x: number, y: number) => {
    if (!info || menuOpen.current) return;
    menuOpen.current = true;
    try { await chain.current; await pins.menu(x, y, marking); }
    catch (e) { setError(String(e)); }
    finally { menuOpen.current = false; }
  };
  return (
    <div ref={surface} className="pin-window" tabIndex={0}
      onContextMenu={(e) => { e.preventDefault(); void showMenu(e.clientX, e.clientY); }}
      onKeyDown={(e) => {
        if (menuOpen.current) return;
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z") {
          e.preventDefault(); annotate("undo"); return;
        }
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "c") {
          e.preventDefault(); void copy(); return;
        }
        if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
          e.preventDefault(); void showMenu(innerWidth / 2, innerHeight / 2); return;
        }
        if (e.key === "Escape" && marking) { e.preventDefault(); setMarking(false); return; }
        if (e.key === "Escape") { e.preventDefault(); act("hide"); }
        if (e.key === "+" || e.key === "=") { e.preventDefault(); act("zoom", 1.1); }
        if (e.key === "-") { e.preventDefault(); act("zoom", 1 / 1.1); }
      }}>
      <div className={`pin-image-area ${marking ? "pin-marking" : ""}`} title={t(marking ? "pins.numberHint" : "pins.dragHint")}
        onMouseDown={(e) => {
          if (e.button === 0 && !e.ctrlKey && !marking && !menuOpen.current)
            void getCurrentWindow().startDragging().catch((err) => setError(String(err)));
        }}>
        <div className="pin-image-content" style={{ width: (info?.width ?? 0) * (info?.zoom ?? 1), height: (info?.height ?? 0) * (info?.zoom ?? 1) }}
          onClick={(e) => {
            if (!marking || e.ctrlKey || menuOpen.current) return;
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
      {feedback && <div className="pin-feedback" role="status">{feedback}</div>}
      {error && <div className="pin-error" role="alert">{t("pins.failed")}: {error}</div>}
    </div>
  );
}
