import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useI18n } from "../i18n";
import { pins, type PinInfo, type PinAction } from "./api";
import { renderAnnotatedImage } from "./annotations";
import { draggedZoom } from "./resize";
import "./pins.css";

export default function PinWindow() {
  const { t } = useI18n();
  const [info, setInfo] = useState<PinInfo | null>(null);
  const [image, setImage] = useState("");
  const [imageRevision, setImageRevision] = useState(0);
  const marks = info?.marks ?? [];
  const [dimensions, setDimensions] = useState({ width: 0, height: 0 });
  const copying = useRef(false);
  const editing = useRef(false);
  const infoRef = useRef(info);
  infoRef.current = info;
  const resizeDrag = useRef<{ x: number; y: number; width: number; height: number; zoom: number } | null>(null);
  const resizeFrame = useRef(0);
  const pendingResize = useRef<number | null>(null);
  const [resizing, setResizing] = useState(false);
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
  const resizeTo = useCallback((zoom: number) => {
    pendingResize.current = zoom;
    if (!resizeFrame.current) resizeFrame.current = requestAnimationFrame(() => {
      resizeFrame.current = 0;
      const next = pendingResize.current;
      pendingResize.current = null;
      if (next !== null) act("resize", next);
    });
  }, [act]);
  useEffect(() => () => cancelAnimationFrame(resizeFrame.current), []);
  const edit = useCallback(async () => {
    if (editing.current) return;
    editing.current = true;
    try { await chain.current; await pins.edit(); setError(""); }
    catch (e) { setError(String(e)); }
    finally { editing.current = false; }
  }, []);
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
      const stop = await listen<T>(name, event => { if (alive) callback(event.payload); }, { target: { kind: "WebviewWindow", label } });
      if (alive) stops.push(stop); else stop();
    };
    void (async () => {
      await subscribe<PinInfo>("pin-changed", value => { if (value.label === label) setInfo(value); });
      await subscribe("pin-image-updated", () => {
        void pins.content().then(content => {
          if (alive) { setInfo(content.info); setImage(content.image); setImageRevision(v => v + 1); }
        }).catch(e => { if (alive) setError(String(e)); });
      });
      await subscribe<string>("pin-save-error", value => setError(String(value)));
      await subscribe<string>("pin-menu-action", action => {
        if (action === "copy") void copy();
        if (action === "edit") void edit();
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
  }, [label, copy, edit]);

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
    let gestureZoom = 1;
    const gestureStart = (event: Event) => {
      event.preventDefault(); gestureZoom = infoRef.current?.zoom ?? 1;
    };
    const gestureChange = (event: Event) => {
      event.preventDefault();
      const scale = (event as Event & { scale: number }).scale;
      if (!menuOpen.current && Number.isFinite(scale) && scale > 0) resizeTo(gestureZoom * scale);
    };
    node.addEventListener("wheel", wheel, { passive: false });
    node.addEventListener("gesturestart", gestureStart, { passive: false });
    node.addEventListener("gesturechange", gestureChange, { passive: false });
    return () => {
      node.removeEventListener("wheel", wheel); cancelAnimationFrame(frame);
      node.removeEventListener("gesturestart", gestureStart); node.removeEventListener("gesturechange", gestureChange);
    };
  }, [act, resizeTo]);

  const ready = () => {
    const img = imageRef.current;
    if (img) setDimensions({ width: img.naturalWidth, height: img.naturalHeight });
    act("ready");
  };
  const showMenu = async (x: number, y: number) => {
    if (!info || menuOpen.current) return;
    menuOpen.current = true;
    try { await chain.current; await pins.menu(x, y); }
    catch (e) { setError(String(e)); }
    finally { menuOpen.current = false; }
  };
  return (
    <div ref={surface} className={`pin-window ${resizing ? "pin-resizing" : ""}`} tabIndex={0}
      onContextMenu={(e) => { e.preventDefault(); void showMenu(e.clientX, e.clientY); }}
      onKeyDown={(e) => {
        if (menuOpen.current) return;
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "c") {
          e.preventDefault(); void copy(); return;
        }
        if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
          e.preventDefault(); void showMenu(innerWidth / 2, innerHeight / 2); return;
        }
        if (e.key === "Escape") { e.preventDefault(); act("hide"); }
        if (e.key === "+" || e.key === "=") { e.preventDefault(); act("zoom", 1.1); }
        if (e.key === "-") { e.preventDefault(); act("zoom", 1 / 1.1); }
      }}>
      <div className="pin-image-area" title={t("pins.dragHint")}
        onMouseDown={(e) => {
          if (e.button === 0 && !e.ctrlKey && !resizeDrag.current && !menuOpen.current)
            void getCurrentWindow().startDragging().catch((err) => setError(String(err)));
        }}>
        <div className="pin-image-content" style={{ width: (info?.width ?? 0) * (info?.zoom ?? 1), height: (info?.height ?? 0) * (info?.zoom ?? 1) }}>
        {image && <img key={imageRevision} ref={imageRef} src={image} alt={t("pins.image")} draggable={false}
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
      <button type="button" className="pin-resize-handle" aria-label={t("pins.resize")} title={t("pins.resizeHint")}
        onPointerDown={(e) => {
          if (e.button !== 0 || !info || menuOpen.current) return;
          e.preventDefault(); e.stopPropagation();
          resizeDrag.current = { x: e.screenX, y: e.screenY, width: info.width * info.zoom, height: info.height * info.zoom, zoom: info.zoom };
          setResizing(true);
          try { e.currentTarget.setPointerCapture(e.pointerId); } catch { /* Some trackpad events have no capturable pointer. */ }
        }}
        onPointerMove={(e) => {
          const start = resizeDrag.current;
          if (!start) return;
          e.preventDefault(); e.stopPropagation();
          resizeTo(draggedZoom(start.zoom, start.width, start.height, e.screenX - start.x, e.screenY - start.y));
        }}
        onPointerUp={(e) => {
          if (!resizeDrag.current) return;
          e.preventDefault(); e.stopPropagation();
          resizeDrag.current = null; setResizing(false);
          try { e.currentTarget.releasePointerCapture(e.pointerId); } catch { /* Capture may already have ended. */ }
        }}
        onLostPointerCapture={() => { resizeDrag.current = null; setResizing(false); }}
        onPointerCancel={() => { resizeDrag.current = null; setResizing(false); }}
        onKeyDown={(e) => {
          if (info && ["ArrowLeft", "ArrowUp", "ArrowRight", "ArrowDown"].includes(e.key)) {
            e.preventDefault(); e.stopPropagation();
            act("resize", info.zoom * (["ArrowLeft", "ArrowUp"].includes(e.key) ? 1 / 1.05 : 1.05));
          }
        }} />
      {feedback && <div className="pin-feedback" role="status">{feedback}</div>}
      {error && <div className="pin-error" role="alert">{t("pins.failed")}: {error}</div>}
    </div>
  );
}
