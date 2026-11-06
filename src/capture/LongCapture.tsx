import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "../i18n";
import { mediaApi, type MediaAsset } from "./api";
import { HotkeyRecorder } from "../settings/Settings";
import { SCROLL_HOTKEY } from "./hotkeys";
let scrollDraft: { frames: MediaAsset[]; overlaps: (number | null)[] } = {
  frames: [],
  overlaps: [],
};
export default function LongCapture({
  onDone,
  hotkey,
  onHotkey,
}: {
  onDone: (asset: MediaAsset) => void;
  hotkey: string;
  onHotkey: (value: string) => Promise<void>;
}) {
  const { locale } = useI18n();
  const l = (zh: string, en: string) => (locale === "en" ? en : zh);
  const [frames, setFrames] = useState<MediaAsset[]>(scrollDraft.frames),
    [overlaps, setOverlaps] = useState<(number | null)[]>(scrollDraft.overlaps),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  useEffect(() => {
    scrollDraft = { frames, overlaps };
  }, [frames, overlaps]);
  async function add(imported = false) {
    setBusy(true);
    setError("");
    try {
      const item = imported
        ? await mediaApi.import()
        : await mediaApi.capture({
            mode: "region",
            delay: 3,
            display: 1,
            cursor: false,
            clicks: false,
          });
      if (!item) return;
      if (frames.length && item.width !== frames[0].width)
        throw Error(
          l(
            "宽度不同，请重新截取同一宽度的滚动区域。",
            "Widths differ. Select the same scroll region.",
          ),
        );
      const next = [...frames, item];
      if (next.length > 1) {
        const values = await invoke<(number | null)[]>("media_stitch_analyze", {
          ids: next.map((f) => f.id),
        });
        setOverlaps(values);
      }
      setFrames(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  async function complete() {
    setBusy(true);
    setError("");
    try {
      onDone(
        await invoke<MediaAsset>("media_stitch", {
          ids: frames.map((f) => f.id),
          overlaps,
        }),
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  const ready =
    frames.length > 1 &&
    overlaps.length === frames.length - 1 &&
    overlaps.every((n) => n !== null);
  const previewWidth = Math.min(500, frames[0]?.width || 500),
    scale = previewWidth / (frames[0]?.width || 500);
  return (
    <section className="capture-long capture-library">
      <div className="capture-eyebrow">SCROLL CAPTURE</div>
      <h1>{l("一页不够，就接着往下。", "Keep the whole story.")}</h1>
      <p>
        {l(
          "截取内容区域，向下滚动约半屏，再截取同一区域。保留部分重叠，避免包含固定页眉。系统会识别接缝；无法确定时需要手动对齐。",
          "Capture the content area, scroll about half a page, then capture the same area. Keep some overlap and exclude fixed headers. Review or adjust each seam.",
        )}
      </p>
      <div className="capture-actions">
        <button
          className="primary"
          disabled={busy || frames.length >= 12}
          onClick={() => void add()}
        >
          {frames.length
            ? l("截取下一帧", "Capture next frame")
            : l("截取第一帧", "Capture first frame")}
        </button>
        <button
          disabled={busy || frames.length >= 12}
          onClick={() => void add(true)}
        >
          {l("添加已有图片", "Add image")}
        </button>
        <button
          disabled={busy || !frames.length}
          onClick={() => {
            setFrames((f) => f.slice(0, -1));
            setOverlaps((o) => o.slice(0, -1));
            setError("");
          }}
        >
          {l("移除最后一帧", "Remove last frame")}
        </button>
        <button disabled={busy || !ready} onClick={() => void complete()}>
          {busy
            ? l("处理中…", "Working…")
            : l("完成并编辑长图", "Finish and edit")}
        </button>
        <button
          disabled={busy || !frames.length}
          onClick={() => {
            setFrames([]);
            setOverlaps([]);
            setError("");
          }}
        >
          {l("重新开始", "Start over")}
        </button>
      </div>
      <div className="capture-shortcut">
        <span>{l("打开长截图", "Open scroll capture")}</span>
        <HotkeyRecorder
          value={hotkey}
          defaultValue={SCROLL_HOTKEY}
          onSave={onHotkey}
        />
      </div>
      {error && (
        <div role="alert" className="capture-error">
          {error}
        </div>
      )}
      {frames.length > 0 && (
        <p>
          {frames.length} / 12{" "}
          {l(
            "帧 · 确认接缝后完成拼接",
            "frames · review the seams before finishing",
          )}
        </p>
      )}
      <div className="capture-stitch-preview" style={{ width: previewWidth }}>
        {frames.map((f, i) => (
          <div key={f.id}>
            {i > 0 && (
              <label className="capture-seam">
                {l("接缝", "Seam")} {i} ·{" "}
                {overlaps[i - 1] === null
                  ? l(
                      "无法自动确定，请调整重叠高度",
                      "Manual alignment required",
                    )
                  : l("重叠高度", "Overlap")}{" "}
                <input
                  type="number"
                  min="0"
                  max={Math.min(f.height - 1, frames[i - 1].height)}
                  value={overlaps[i - 1] ?? ""}
                  placeholder="px"
                  onChange={(e) => {
                    const n =
                      e.target.value === ""
                        ? null
                        : Math.max(
                            0,
                            Math.min(
                              f.height - 1,
                              frames[i - 1].height,
                              Number(e.target.value),
                            ),
                          );
                    setOverlaps((o) => o.map((v, j) => (j === i - 1 ? n : v)));
                  }}
                />{" "}
                px
              </label>
            )}
            <div
              style={{
                height: (f.height - (i ? (overlaps[i - 1] ?? 0) : 0)) * scale,
                overflow: "hidden",
              }}
            >
              <img
                alt={`${l("第", "Frame ")}${i + 1}`}
                src={mediaApi.src(f)}
                style={{
                  display: "block",
                  width: previewWidth,
                  marginTop: -(i ? (overlaps[i - 1] ?? 0) : 0) * scale,
                }}
              />
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
