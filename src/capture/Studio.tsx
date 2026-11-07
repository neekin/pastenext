import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { HotkeyRecorder } from "../settings/Settings";
import { availableMonitors } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { useI18n } from "../i18n";
import { api } from "../api";
import { mediaApi, type MediaAsset, type CaptureOptions } from "./api";
import Editor from "./Editor";
import { pins } from "../pins/api";
import { marksFromPin } from "./pinEdits";
import { drafts } from "./history";
import Transfer from "./Transfer";
import LongCapture from "./LongCapture";
import Preferences from "./Preferences";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./capture.css";
import { CAPTURE_HOTKEY, RECORD_HOTKEY, SCROLL_HOTKEY } from "./hotkeys";

const isMac=/Mac|iPhone|iPad/.test(navigator.platform);
type Tab = "long" | "capture" | "record" | "transfer" | "library" | "edit";
export default function Studio() {
  const { locale } = useI18n();
  const l = (zh: string, en: string) => (locale === "en" ? en : zh);
  const [tab, setTab] = useState<Tab>("capture"),
    [assets, setAssets] = useState<MediaAsset[]>([]),
    [active, setActive] = useState<MediaAsset | null>(null),
    [shared, setShared] = useState<MediaAsset | null>(null);
  const [pinSession, setPinSession] = useState<{ id: string; label: string } | null>(null);
  const [exporting, setExporting] = useState(false),
    [videoQuality, setVideoQuality] = useState(90);
  const [scrollKey, setScrollKey] = useState(SCROLL_HOTKEY);
  const [hotkeyErrors, setHotkeyErrors] = useState<Record<string, string>>({});
  const [captureKey, setCaptureKey] = useState(CAPTURE_HOTKEY),
    [recordKey, setRecordKey] = useState(RECORD_HOTKEY);
  const [options, setOptions] = useState<CaptureOptions>({
    mode: "region",
    delay: 0,
    display: 1,
    cursor: false,
    clicks: true,
  });
  const [elapsed, setElapsed] = useState(0);
  const [busy, setBusy] = useState(false),
    [recording, setRecording] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [monitors, setMonitors] = useState(1),
    [videoFormat, setVideoFormat] = useState("mp4");
  const refresh = () =>
    mediaApi
      .list()
      .then(setAssets)
      .catch((e) => setError(String(e)));
  useEffect(() => {
    void refresh();
    void invoke<Record<string, string>>("capture_hotkey_errors").then(
      setHotkeyErrors,
    );
    void availableMonitors()
      .then((m) => setMonitors(Math.max(1, m.length)))
      .catch(() => {});
    void api.getSettings().then((s) => {
      setScrollKey(s.scroll_hotkey || SCROLL_HOTKEY);
      setCaptureKey(s.capture_hotkey || CAPTURE_HOTKEY);
      setRecordKey(s.record_hotkey || RECORD_HOTKEY);
      if (s.capture_options) {
        try {
          const saved = JSON.parse(s.capture_options);
          setOptions((o) => ({ ...o, ...saved }));
        } catch {}
      }
    });
    let alive = true;
    const openPinEdit = async () => {
      if (!alive) return;
      const request = await pins.takeEdit();
      if (!alive || !request) return;
      drafts.set(request.asset.id, { marks: marksFromPin(request.marks), redo: [] });
      setPinSession({ id: request.asset.id, label: request.pinLabel });
      setActive(request.asset);
      setTab("edit");
      setError(""); setNotice("");
      void refresh();
    };
    const pinEdit = listen("pin-edit-ready", () => { void openPinEdit().catch(e => setError(String(e))); }, { target: { kind: "WebviewWindow", label: "studio" } });
    void pinEdit.then(() => openPinEdit()).catch(e => setError(String(e)));
    const pinEnded = listen<string>("pin-edit-ended", event => setPinSession(s => s?.id === event.payload ? null : s), { target: { kind: "WebviewWindow", label: "studio" } });
    const modes=listen<CaptureOptions>("capture-options-updated",e=>setOptions(e.payload));
    const u = listen<MediaAsset>("media-updated", () => void refresh());
    const er = listen<string>("capture-error", (e) => setError(e.payload));
    const open = listen<string>("studio-tab", (e) => setTab(e.payload as Tab));
    const ready = listen<MediaAsset>("capture-ready", (e) => {
      setActive(e.payload);
      void api.getSettings().then((s) => {
        let edit = true;
        try {
          edit = JSON.parse(s.capture_preferences || "{}").editAfter !== false;
        } catch {}
        setTab(e.payload.kind === "video" || edit ? "edit" : "library");
      });
      void refresh();
    });
    const state = listen(
      "capture-state",
      () =>
        void mediaApi.status().then((s) => {
          setBusy(s.busy);
          setRecording(s.recording);
        }),
    );
    return () => {
      alive = false;
      for (const p of [u, er, open, ready, state, modes, pinEdit, pinEnded]) p.then((f) => f());
    };
  }, []);
  useEffect(() => {
    const status = () =>
      void mediaApi
        .status()
        .then((s) => {
          setBusy(s.busy);
          setRecording(s.recording);
          setElapsed(Math.floor(s.elapsedMs / 1000));
        })
        .catch((e) => setError(String(e)));
    status();
    const focus = getCurrentWindow().onFocusChanged((e) => {
      if (e.payload) status();
    });
    const timer = recording ? setInterval(status, 1000) : undefined;
    return () => {
      clearInterval(timer);
      focus.then((f) => f());
    };
  }, [recording]);
  useEffect(() => {
    if (pinSession && (tab !== "edit" || active?.id !== pinSession.id)) {
      void pins.cancelEdit(pinSession.id, false).catch(e => setError(String(e)));
      setPinSession(null);
    }
  }, [pinSession, tab, active?.id]);
  const setOption = (next: Partial<CaptureOptions>) => {
    const value = { ...options, ...next };
    setOptions(value);
    void api
      .setSetting("capture_options", JSON.stringify(value))
      .catch((e) => setError(String(e)));
  };
  async function capture(video = false) {
    setBusy(true);
    setRecording(video);
    setError("");
    setNotice("");
    try {
      const item = await mediaApi.capture(
        {
          ...options,
          mode: video && options.mode === "window" ? "region" : options.mode,
        },
        video,
      );
      if (item) {
        setActive(item);
        const settings = await api.getSettings();
        let edit = true;
        try {
          edit =
            JSON.parse(settings.capture_preferences || "{}").editAfter !==
            false;
        } catch {}
        setTab(video || edit ? "edit" : "library");
        await refresh();
      } else setNotice(l("已取消", "Cancelled"));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setRecording(false);
    }
  }
  async function exportVideo() {
    if (!active || exporting) return;
    setExporting(true);
    setError("");
    try {
      const path = await mediaApi.export(active.id, videoFormat, videoQuality);
      if (path) setNotice(l("已保存到：", "Saved to: ") + path);
    } catch (e) {
      setError(String(e));
    } finally {
      setExporting(false);
    }
  }
  const share = (item: MediaAsset) => {
    setShared(item);
    setTab("transfer");
  };
  async function importImage() {
    try {
      const item = await mediaApi.import();
      if (item) {
        setActive(item);
        setTab("edit");
        await refresh();
      }
    } catch (e) {
      setError(String(e));
    }
  }
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (
        e.defaultPrevented ||
        e.isComposing ||
        !(e.metaKey || e.ctrlKey) ||
        (e.target as HTMLElement).closest(
          "input,textarea,select,[contenteditable=true]",
        )
      )
        return;
      if (e.key.toLowerCase() === "o") {
        e.preventDefault();
        void importImage();
      } else if (e.key === ",") {
        e.preventDefault();
        void api.showSettings();
      } else if (e.key.toLowerCase() === "w") {
        e.preventDefault();
        void getCurrentWindow().close();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);
  const nav: [Tab, string, string, string][] = [
    ["capture", "⌗", "截图", "Capture"],
    ["long", "↕", "长截图", "Scroll capture"],
    ["record", "◉", "录屏", "Record"],
    ["transfer", "▦", "扫码传图", "Transfer"],
    ["library", "▧", "媒体历史", "Library"],
  ];
  return (
    <div className="capture-app">
      <aside className="capture-nav">
        <div className="capture-brand">
          <span>P</span>
          <div>
            PasteNext<small>{l("截图工作台", "Capture studio")}</small>
          </div>
        </div>
        <nav>
          {nav.map(([key, icon, zh, en]) => (
            <button
              key={key}
              className={tab === key ? "active" : ""}
              onClick={() => {
                if (key === "record" && options.mode === "window")
                  setOption({ mode: "region" });
                setTab(key);
              }}
            >
              <b>{icon}</b>
              {l(zh, en)}
            </button>
          ))}
        </nav>
        <div className="capture-nav-bottom">
          <button onClick={() => void importImage()}>
            ＋ {l("打开图片", "Open image")}
          </button>
          <p>{l("捕捉 · 标注 · 分享", "Capture · Annotate · Share")}</p>
        </div>
      </aside>
      <main className="capture-main">
        {hotkeyErrors.reference && (
          <div role="status" className="capture-notice">
            {hotkeyErrors.reference}
            <button
              onClick={() => setHotkeyErrors((e) => ({ ...e, reference: "" }))}
            >
              ✕
            </button>
          </div>
        )}
        {error && (
          <div role="alert" className="capture-error">
            {error}
            <button onClick={() => setError("")}>✕</button>
          </div>
        )}
        {notice && (
          <div role="status" className="capture-notice">
            {notice}
          </div>
        )}
        {(tab === "capture" || tab === "record") && (
          <section className="capture-home">
            <div className="capture-eyebrow">
              {tab === "record" ? "SCREEN RECORDING" : "SCREEN CAPTURE"}
            </div>
            <h1>
              {tab === "record"
                ? l("把过程，记录下来。", "Make the moment move.")
                : l("看见，然后捕捉。", "See it. Capture it.")}
            </h1>
            <p>
              {tab === "record"
                ? l(
                    "录制选定区域或整个屏幕，完成后导出为视频或 GIF。",
                    "Record a region or display, then export a video or GIF.",
                  )
                : l(
                    "截取屏幕中的灵感，标注重点，随时复制与分享。",
                    "Capture an idea, mark what matters, and share it.",
                  )}
            </p>
            <div className="capture-mode-grid">
              {(
                [
                  "region",
                  ...(tab === "capture" ? ["window"] : []),
                  "screen",
                ] as CaptureOptions["mode"][]
              ).map((mode) => (
                <button
                  className={options.mode === mode ? "selected" : ""}
                  key={mode}
                  onClick={() => setOption({ mode })}
                >
                  <span className={`capture-mode-icon ${mode}`}>
                    <i />
                  </span>
                  <strong>
                    {mode === "region"
                      ? l("区域", "Region")
                      : mode === "window"
                        ? l("窗口", "Window")
                        : l("全屏", "Display")}
                  </strong>
                  <small>
                    {mode === "region"
                      ? l("拖动框选所需范围", "Drag to select an area")
                      : mode === "window"
                        ? l("点选目标窗口", "Click the target window")
                        : l("捕捉完整屏幕", "Capture a whole display")}
                  </small>
                </button>
              ))}
            </div>
            <div className="capture-options">
              <label>
                {l("倒计时", "Delay")}
                <select
                  value={options.delay}
                  onChange={(e) => setOption({ delay: Number(e.target.value) })}
                >
                  <option value={0}>{l("立即", "None")}</option>
                  {[3, 5, 10].map((s) => (
                    <option key={s} value={s}>
                      {s}s
                    </option>
                  ))}
                </select>
              </label>
              {options.mode === "screen" && (
                <label>
                  {l("屏幕", "Display")}
                  <select
                    value={options.display}
                    onChange={(e) =>
                      setOption({ display: Number(e.target.value) })
                    }
                  >
                    {Array.from({ length: monitors }, (_, i) => (
                      <option key={i} value={i + 1}>
                        {l("屏幕 ", "Display ")}
                        {i + 1}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              {tab === "record" ? (
                <label>
                  <input
                    type="checkbox"
                    checked={options.clicks}
                    onChange={(e) => setOption({ clicks: e.target.checked })}
                  />
                  {l("显示鼠标点击", "Show mouse clicks")}
                </label>
              ) : (
                options.mode === "screen" && (
                  <label>
                    <input
                      type="checkbox"
                      checked={options.cursor}
                      onChange={(e) => setOption({ cursor: e.target.checked })}
                    />
                    {l("包含鼠标指针", "Include pointer")}
                  </label>
                )
              )}
            </div>
            <button
              className="primary capture-start"
              disabled={busy}
              onClick={() => void capture(tab === "record")}
            >
              {busy
                ? l("正在进行…", "In progress…")
                : tab === "record"
                  ? l("开始录屏", "Start recording")
                  : l("开始截图", "Take screenshot")}{" "}
              <span>↗</span>
            </button>
            {busy && (
              <div className="capture-actions">
                {recording && (
                  <span role="status">
                    {l("录屏进行中", "Recording")} · {Math.floor(elapsed / 60)}:
                    {String(elapsed % 60).padStart(2, "0")}
                  </span>
                )}
                <button
                  onClick={() =>
                    mediaApi.stop(!recording).catch((e) => setError(String(e)))
                  }
                >
                  {recording
                    ? l("结束并保存", "Stop and save")
                    : l("取消截图", "Cancel")}
                </button>
                {recording && (
                  <button
                    onClick={() =>
                      mediaApi.stop(true).catch((e) => setError(String(e)))
                    }
                  >
                    {l("放弃录制", "Discard recording")}
                  </button>
                )}
              </div>
            )}
            {hotkeyErrors[tab === "record" ? "record" : "capture"] && (
              <div role="alert" className="capture-error">
                {hotkeyErrors[tab === "record" ? "record" : "capture"]}
              </div>
            )}
            <div className="capture-shortcut">
              <span>{l("全局快捷键", "Global shortcut")}</span>
              <HotkeyRecorder
                value={tab === "record" ? recordKey : captureKey}
                defaultValue={
                  tab === "record" ? RECORD_HOTKEY : CAPTURE_HOTKEY
                }
                onSave={async (accelerator) => {
                  await invoke("set_capture_hotkey", {
                    kind: tab === "record" ? "record" : "capture",
                    accelerator,
                  });
                  setHotkeyErrors(
                    await invoke<Record<string, string>>(
                      "capture_hotkey_errors",
                    ),
                  );
                  tab === "record"
                    ? setRecordKey(accelerator)
                    : setCaptureKey(accelerator);
                }}
              />
            </div>
            <p className="capture-footnote">
              {isMac?l("按 Esc 取消选区。首次截图需要允许屏幕录制权限。","Press Esc to cancel. Allow Screen Recording on first use."):l("截图时可按 1 / 2 / 3 切换模式，按 Esc 取消。录屏可再次按快捷键结束。","Press 1 / 2 / 3 to switch modes or Esc to cancel. Press the recording shortcut again to stop.")}
              {isMac&&<button onClick={()=>mediaApi.permission().catch(e=>setError(String(e)))}>{l("打开权限设置","Open permissions")}</button>}
            </p>
            {tab === "capture" && <Preferences />}
          </section>
        )}
        {tab === "edit" &&
          active &&
          (active.kind === "image" ? (
            <Editor
              key={active.id}
              asset={active}
              pinSessionId={pinSession?.id === active.id ? pinSession.id : undefined}
              onShare={share}
              onSaved={() => void refresh()}
            />
          ) : (
            <section className="capture-video">
              <div className="capture-eyebrow">RECORDING READY</div>
              <h1>{l("录制完成。", "Recording ready.")}</h1>
              <video controls src={mediaApi.src(active)} />
              <div className="capture-actions">
                <select
                  disabled={exporting}
                  aria-label={l("录屏导出格式", "Video export format")}
                  value={videoFormat}
                  onChange={(e) => setVideoFormat(e.target.value)}
                >
                  <option>mp4</option>
                  <option>mov</option>
                  <option>gif</option>
                </select>
                <select
                  aria-label={l("导出画质", "Export quality")}
                  disabled={exporting}
                  value={videoQuality}
                  onChange={(e) => setVideoQuality(Number(e.target.value))}
                >
                  <option value={90}>{l("高清", "High quality")}</option>
                  <option value={70}>{l("较小文件", "Smaller file")}</option>
                </select>
                <button onClick={() => share(active)}>
                  {l("扫码下载", "Share to phone")}
                </button>
                <button
                  onClick={() =>
                    mediaApi
                      .copy(active.id)
                      .then(() =>
                        setNotice(l("视频文件已复制", "Video file copied")),
                      )
                      .catch((e) => setError(String(e)))
                  }
                >
                  {l("复制文件", "Copy file")}
                </button>
                <button
                  className="primary"
                  disabled={exporting}
                  onClick={() => void exportVideo()}
                >
                  {exporting
                    ? l("正在转换…", "Converting…")
                    : l("导出录屏", "Export recording")}
                </button>
              </div>
            </section>
          ))}
        {tab === "long" && (
          <LongCapture
            hotkey={scrollKey}
            onHotkey={async (accelerator) => {
              await invoke("set_capture_hotkey", {
                kind: "scroll",
                accelerator,
              });
              setScrollKey(accelerator);
            }}
            onDone={(item) => {
              setActive(item);
              setTab("edit");
              void refresh();
            }}
          />
        )}
        {tab === "transfer" && (
          <Transfer asset={shared} onReceived={() => void refresh()} />
        )}
        {tab === "library" && (
          <section className="capture-library">
            <div className="capture-eyebrow">YOUR CAPTURES</div>
            <h1>{l("每一次捕捉，都在这里。", "Everything you capture.")}</h1>
            <p>
              {l(
                "图片可以再次编辑，录屏可以转换格式或传到手机。",
                "Reopen images, export recordings, or send them to your phone.",
              )}
            </p>
            <div className="capture-library-grid">
              {assets.map((item) => (
                <button
                  key={item.id}
                  onClick={() => {
                    setActive(item);
                    setTab("edit");
                  }}
                >
                  {item.kind === "image" ? (
                    <img src={mediaApi.src(item)} loading="lazy" />
                  ) : (
                    <div className="capture-video-thumb">▷</div>
                  )}
                  <strong>{item.name}</strong>
                  <small>
                    {new Date(item.createdAt).toLocaleString()} ·{" "}
                    {item.kind === "image"
                      ? `${item.width} × ${item.height}`
                      : "VIDEO"}
                  </small>
                </button>
              ))}
            </div>
            {!assets.length && (
              <div className="capture-empty">
                {l(
                  "还没有截图，开始捕捉第一个画面吧。",
                  "Your first capture will appear here.",
                )}
              </div>
            )}
          </section>
        )}
      </main>
    </div>
  );
}
