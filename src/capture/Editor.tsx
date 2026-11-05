import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  useReducer,
} from "react";
import { api } from "../api";
import { useI18n } from "../i18n";
import { mediaApi, type MediaAsset } from "./api";
import {
  cropFor,
  paint,
  renderImage,
  type Mark,
  type Point,
  type Tool,
} from "./drawing";

import { editHistory, drafts } from "./history";

const tools: [Tool, string, string, string][] = [
  ["crop", "⌗", "裁剪", "Crop"],
  ["rectangle", "□", "矩形", "Rectangle"],
  ["ellipse", "○", "椭圆", "Ellipse"],
  ["arrow", "↗", "箭头", "Arrow"],
  ["line", "╱", "直线", "Line"],
  ["pen", "✎", "画笔", "Pen"],
  ["highlight", "▰", "荧光笔", "Highlight"],
  ["text", "T", "文字", "Text"],
  ["number", "①", "序号", "Number"],
  ["mosaic", "▦", "马赛克", "Mosaic"],
  ["blur", "◌", "模糊", "Blur"],
  ["sticker", "☺", "贴纸", "Sticker"],
];
export default function Editor({
  asset,
  onShare,
  onSaved,
}: {
  asset: MediaAsset;
  onShare: (asset: MediaAsset) => void;
  onSaved: () => void;
}) {
  const { locale } = useI18n();
  const l = (zh: string, en: string) => (locale === "en" ? en : zh);
  const [tool, setTool] = useState<Tool>("arrow"),
    [color, setColor] = useState("#f45159"),
    [width, setWidth] = useState(4),
    [text, setText] = useState(""),
    [sticker, setSticker] = useState("👍");
  const [history, dispatch] = useReducer(
    editHistory,
    drafts.get(asset.id) || { marks: [], redo: [] },
  );
  const { marks, redo } = history;
  const [zoom, setZoom] = useState(1),
    [fit, setFit] = useState(1),
    [quality, setQuality] = useState(92);
  const committed = useRef<HTMLCanvasElement | null>(null);
  useEffect(() => {
    drafts.set(asset.id, history);
  }, [asset.id, history]);
  const [ready, setReady] = useState(false),
    [busy, setBusy] = useState(false),
    [message, setMessage] = useState(""),
    [error, setError] = useState(""),
    [ocr, setOcr] = useState<string | null>(null),
    [format, setFormat] = useState("png");
  const canvas = useRef<HTMLCanvasElement>(null),
    viewport = useRef<HTMLDivElement>(null),
    image = useRef<HTMLImageElement | null>(null),
    draft = useRef<Mark | null>(null),
    frame = useRef(0);
  useEffect(() => {
    void api.getSettings().then((s) => {
      try {
        const p = JSON.parse(s.capture_preferences || "{}");
        if (p.format) setFormat(p.format);
        if (p.quality) setQuality(p.quality);
      } catch {}
    });
  }, []);
  const saved = useRef<{ signature: string; asset: MediaAsset }>({
    signature: "[]",
    asset,
  });
  const scale = fit * zoom;
  useEffect(() => {
    let active = true;
    setReady(false);
    dispatch({
      type: "restore",
      history: drafts.get(asset.id) || { marks: [], redo: [] },
    });
    setOcr(null);
    setError("");
    saved.current = { signature: "[]", asset };
    api
      .readImage(asset.path)
      .then((src) => {
        const img = new Image();
        img.onload = () => {
          if (active) {
            image.current = img;
            setReady(true);
          }
        };
        img.onerror = () =>
          active && setError(l("无法打开图片", "Unable to load image"));
        img.src = src;
      })
      .catch((e) => active && setError(String(e)));
    return () => {
      active = false;
      cancelAnimationFrame(frame.current);
    };
  }, [asset.id]);
  useLayoutEffect(() => {
    if (!ready || !viewport.current || !image.current) return;
    const el = viewport.current;
    const measure = () =>
      setFit(
        Math.min(
          1,
          (el.clientWidth - 72) / image.current!.naturalWidth,
          (el.clientHeight - 72) / image.current!.naturalHeight,
        ),
      );
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    measure();
    return () => observer.disconnect();
  }, [ready]);
  function draw() {
    if (!ready || !canvas.current || !image.current) return;
    const c = canvas.current,
      img = image.current;
    if (c.width !== img.naturalWidth) c.width = img.naturalWidth;
    if (c.height !== img.naturalHeight) c.height = img.naturalHeight;
    const ctx = c.getContext("2d")!;
    ctx.clearRect(0, 0, c.width, c.height);
    ctx.drawImage(committed.current || img, 0, 0);
    if (draft.current) paint(ctx, draft.current, true);
    const crop = cropFor(
      draft.current?.tool === "crop" ? [...marks, draft.current] : marks,
      c.width,
      c.height,
    );
    if (crop.width !== c.width || crop.height !== c.height) {
      ctx.save();
      ctx.fillStyle = "#11182788";
      ctx.beginPath();
      ctx.rect(0, 0, c.width, c.height);
      ctx.rect(crop.x, crop.y, crop.width, crop.height);
      ctx.fill("evenodd");
      ctx.strokeStyle = "#a5b4fc";
      ctx.lineWidth = 2 / scale;
      ctx.strokeRect(crop.x, crop.y, crop.width, crop.height);
      ctx.restore();
    }
  }
  useLayoutEffect(() => {
    if (ready && image.current) {
      committed.current = renderImage(image.current, marks, false);
      draw();
    }
  }, [marks, ready]);
  useLayoutEffect(draw, [scale]);
  function scheduleDraw() {
    cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(draw);
  }
  function point(e: React.PointerEvent): Point {
    const r = canvas.current!.getBoundingClientRect();
    return {
      x: Math.max(
        0,
        Math.min(
          canvas.current!.width,
          ((e.clientX - r.left) / r.width) * canvas.current!.width,
        ),
      ),
      y: Math.max(
        0,
        Math.min(
          canvas.current!.height,
          ((e.clientY - r.top) / r.height) * canvas.current!.height,
        ),
      ),
    };
  }
  function push(mark: Mark) {
    dispatch({ type: "push", mark });
    setMessage("");
  }
  const undo = () => dispatch({ type: "undo" });
  const redoOne = () => dispatch({ type: "redo" });
  async function ensureSaved() {
    const signature = JSON.stringify(marks);
    if (saved.current.signature === signature) return saved.current.asset;
    const data = renderImage(image.current!, marks).toDataURL("image/png");
    const next = await mediaApi.commit(data, asset.name);
    saved.current = { signature, asset: next };
    onSaved();
    return next;
  }
  async function act(action: "copy" | "save" | "ocr" | "share" | "pin") {
    if (busy || !ready) return;
    setBusy(true);
    setError("");
    setMessage("");
    try {
      const current = await ensureSaved();
      if (action === "copy") {
        await mediaApi.copy(current.id);
        setMessage(
          l("已复制到剪贴板，并保存在历史中", "Copied and saved to history"),
        );
      }
      if (action === "save") {
        const path = await mediaApi.export(current.id, format, quality);
        if (path) setMessage(l("已保存到：", "Saved to: ") + path);
      }
      if (action === "ocr") {
        const result = await mediaApi.ocr(current.id);
        setOcr(result);
        if (!result) setMessage(l("未识别到文字", "No text found"));
      }
      if (action === "share") onShare(current);
      if (action === "pin") await mediaApi.pin(current.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (busy) return;
      if (
        (e.target as HTMLElement).closest("input,textarea,select") ||
        e.isComposing
      )
        return;
      if (e.metaKey || e.ctrlKey) {
        if (e.key.toLowerCase() === "z") {
          e.preventDefault();
          e.shiftKey ? redoOne() : undo();
        }
        if (e.key.toLowerCase() === "s") {
          e.preventDefault();
          void act("save");
        }
        if (e.key === "-" || e.key === "_") {
          e.preventDefault();
          setZoom((z) => Math.max(0.25, z - 0.25));
        }
        if (e.key === "=" || e.key === "+") {
          e.preventDefault();
          setZoom((z) => Math.min(4, z + 0.25));
        }
        if (e.key.toLowerCase() === "c") {
          e.preventDefault();
          void act("copy");
        }
      } else if (e.key === "Escape" && draft.current) {
        draft.current = null;
        scheduleDraw();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [marks, redo, ready, busy, format, quality]);
  return (
    <div className="capture-editor">
      <div className="capture-editor-top">
        <div>
          <strong>{asset.name}</strong>
          <span>
            {asset.width} × {asset.height} px
          </span>
        </div>
        <div className="capture-actions">
          <button
            onClick={undo}
            disabled={!marks.length || busy}
            title={l("撤销 ⌘Z", "Undo ⌘Z")}
          >
            ↶
          </button>
          <button
            onClick={redoOne}
            disabled={!redo.length || busy}
            title={l("重做 ⇧⌘Z", "Redo ⇧⌘Z")}
          >
            ↷
          </button>
          <button disabled={busy || !ready} onClick={() => void act("pin")}>
            {l("贴图", "Pin")}
          </button>
          <button disabled={busy || !ready} onClick={() => void act("ocr")}>
            {l("识别文字", "Extract text")}
          </button>
          <button disabled={busy || !ready} onClick={() => void act("share")}>
            {l("扫码传图", "Share to phone")}
          </button>
          <select
            aria-label={l("保存格式", "Export format")}
            value={format}
            onChange={(e) => setFormat(e.target.value)}
          >
            <option value="png">PNG</option>
            <option value="jpg">JPEG</option>
          </select>
          {format === "jpg" && (
            <input
              aria-label={l("JPEG 质量", "JPEG quality")}
              title={l("JPEG 质量", "JPEG quality")}
              type="range"
              min="20"
              max="100"
              value={quality}
              onChange={(e) =>
                setQuality(Math.max(20, Math.min(100, Number(e.target.value))))
              }
              style={{ width: 55 }}
            />
          )}
          <button disabled={busy || !ready} onClick={() => void act("save")}>
            {l("保存", "Save")}
          </button>
          <button
            className="primary"
            disabled={busy || !ready}
            onClick={() => void act("copy")}
          >
            {busy ? l("处理中…", "Working…") : l("复制图片", "Copy image")}
          </button>
        </div>
      </div>
      <div className="capture-editor-body">
        <aside className="capture-tools">
          {tools.map(([key, icon, zh, en]) => (
            <button
              key={key}
              aria-label={l(zh, en)}
              title={l(zh, en)}
              aria-pressed={key === tool}
              className={key === tool ? "selected" : ""}
              onClick={() => setTool(key)}
            >
              <b>{icon}</b>
              <small>{l(zh, en)}</small>
            </button>
          ))}
        </aside>
        <div className="capture-canvas-area" ref={viewport}>
          <div
            className="capture-canvas-space"
            style={{
              minWidth: ready ? asset.width * scale + 72 : undefined,
              minHeight: ready ? asset.height * scale + 72 : undefined,
            }}
          >
            {!ready && <span>{l("正在载入图片…", "Loading image…")}</span>}
            <canvas
              ref={canvas}
              aria-label={l("图片标注画布", "Image annotation canvas")}
              style={{
                display: ready ? "block" : "none",
                width: asset.width * scale,
                height: asset.height * scale,
              }}
              onPointerDown={(e) => {
                if (e.button !== 0 || busy) return;
                const p = point(e);
                const mark: Mark = {
                  tool,
                  from: p,
                  to: p,
                  color,
                  width,
                  points: [p],
                };
                if (
                  tool === "text" ||
                  tool === "number" ||
                  tool === "sticker"
                ) {
                  mark.text =
                    tool === "text"
                      ? text
                      : tool === "sticker"
                        ? sticker
                        : String(
                            marks.filter((m) => m.tool === "number").length + 1,
                          );
                  if (mark.text) push(mark);
                  else
                    setMessage(
                      l(
                        "先在下方输入标注文字，再点击画布",
                        "Enter text below, then click the canvas",
                      ),
                    );
                  return;
                }
                e.currentTarget.setPointerCapture(e.pointerId);
                draft.current = mark;
              }}
              onPointerMove={(e) => {
                if (!draft.current) return;
                const p = point(e);
                draft.current.to = p;
                if (
                  draft.current.tool === "pen" ||
                  draft.current.tool === "highlight"
                ) {
                  const last =
                    draft.current.points?.[draft.current.points.length - 1];
                  if (!last || Math.hypot(p.x - last.x, p.y - last.y) >= 1.5)
                    draft.current.points?.push(p);
                }
                scheduleDraw();
              }}
              onPointerUp={(e) => {
                if (!draft.current) return;
                cancelAnimationFrame(frame.current);
                const mark = draft.current;
                mark.to = point(e);
                draft.current = null;
                if (
                  Math.hypot(mark.to.x - mark.from.x, mark.to.y - mark.from.y) >
                    2 ||
                  (mark.points?.length || 0) > 2
                )
                  push(mark);
                else scheduleDraw();
              }}
              onPointerCancel={() => {
                draft.current = null;
                scheduleDraw();
              }}
            />
          </div>
        </div>
      </div>
      <div className="capture-properties">
        <label>
          {l("颜色", "Color")}{" "}
          <input
            type="color"
            aria-label={l("标注颜色", "Annotation color")}
            value={color}
            onChange={(e) => setColor(e.target.value)}
          />
        </label>
        <label>
          {l("粗细", "Width")}{" "}
          <input
            type="range"
            min="2"
            max="14"
            value={width}
            onChange={(e) => setWidth(Number(e.target.value))}
          />
          <span>{width}</span>
        </label>
        {tool === "text" && (
          <input
            className="capture-text-value"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={l(
              "输入文字后点击图片",
              "Type text, then click the image",
            )}
          />
        )}
        {tool === "sticker" && (
          <select
            aria-label={l("贴纸", "Sticker")}
            value={sticker}
            onChange={(e) => setSticker(e.target.value)}
          >
            {["👍", "✅", "❌", "⭐", "💡", "❗", "❤️", "🎯"].map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
        )}
        <div className="capture-zoom">
          <button onClick={() => setZoom((z) => Math.max(0.25, z - 0.25))}>
            −
          </button>
          <button onClick={() => setZoom(1)}>{Math.round(scale * 100)}%</button>
          <button onClick={() => setZoom((z) => Math.min(4, z + 0.25))}>
            ＋
          </button>
        </div>
      </div>
      {message && (
        <div role="status" className="capture-notice">
          {message}
        </div>
      )}
      {error && (
        <div role="alert" className="capture-error">
          {error}
        </div>
      )}
      {ocr !== null && (
        <div className="capture-ocr">
          <div>
            <strong>{l("识别结果", "Recognized text")}</strong>
            <button
              onClick={() =>
                api
                  .copyText(ocr)
                  .then(() => setMessage(l("文字已复制", "Text copied")))
                  .catch((e) => setError(String(e)))
              }
              disabled={!ocr}
            >
              {l("复制文字", "Copy text")}
            </button>
            <button onClick={() => setOcr(null)}>✕</button>
          </div>
          <textarea value={ocr} onChange={(e) => setOcr(e.target.value)} />
        </div>
      )}
    </div>
  );
}
