import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { pins, type PinInfo, supportsPins } from "./api";
import { useI18n } from "../i18n";

export default function PinManager() {
  const { t } = useI18n();
  const [items, setItems] = useState<PinInfo[]>([]);
  const [open, setOpen] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!supportsPins) return;
    let alive = true;
    const stops: (() => void)[] = [];
    const refresh = () => pins.list().then((value) => { if (alive) setItems(value); });
    const reload = () => { void refresh().catch((e) => { if (alive) setError(String(e)); }); };
    for (const subscription of [
      listen("pins-changed", reload, { target: { kind: "WebviewWindow", label: "panel" } }),
      getCurrentWindow().onFocusChanged(event => { if (event.payload) reload(); }),
    ]) void subscription.then(stop => { if (alive) { stops.push(stop); reload(); } else stop(); })
      .catch(e => { if (alive) setError(String(e)); });
    return () => { alive = false; stops.forEach(stop => stop()); };
  }, []);
  const run = (task: () => Promise<unknown>) => {
    if (busy) return;
    setBusy(true);
    void task().then(() => pins.list()).then(value => { setItems(value); setError(""); })
      .catch(e => { setError(String(e)); setOpen(true); }).finally(() => setBusy(false));
  };
  if (!supportsPins) return null;
  const hidden = items.filter(pin => pin.hidden).length;
  return <div className="relative flex items-center gap-1" onKeyDown={(e) => e.stopPropagation()}>
    {hidden > 0 && <button disabled={busy} onClick={() => run(() => pins.all("restore-hidden"))}
      className="h-9 px-2 rounded-lg bg-indigo-500 text-white text-xs hover:bg-indigo-600 disabled:opacity-50">
      {t("pins.restoreHidden", { n: hidden })}
    </button>}
    <button className="h-9 px-2 rounded-lg bg-black/5 dark:bg-white/10 text-xs text-neutral-600 dark:text-neutral-300"
      aria-expanded={open} onClick={() => setOpen(!open)}>{t("pins.manager")} {items.length}</button>
    {open && <>
      <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
      <div className="absolute z-50 right-0 top-10 w-72 p-3 rounded-xl bg-white dark:bg-neutral-800 shadow-xl text-xs text-neutral-700 dark:text-neutral-200"
        onKeyDown={(e) => { if (e.key === "Escape") setOpen(false); }}>
        <div className="flex flex-wrap gap-2 mb-2">
          <button disabled={busy || !items.length} onClick={() => run(() => pins.all("hide"))}>{t("pins.hideAll")}</button>
          <button disabled={busy || !items.length} onClick={() => run(() => pins.all("restore"))}>{t("pins.restoreAll")}</button>
          <button disabled={busy || !items.length} onClick={() => run(() => pins.all("unlock"))}>{t("pins.unlockAll")}</button>
        </div>
        <p className="text-neutral-400 mb-2">{t("pins.savedHint")}</p>
        {!items.length && <p>{t("pins.empty")}</p>}
        <div className="max-h-40 overflow-y-auto">
          {items.map((pin) => <div key={pin.label} className="flex gap-2 items-center py-1 border-t border-black/5 dark:border-white/10">
            <span className="flex-1 truncate">{pin.mediaId ? `${t("panel.capture")} · ${pin.mediaId.slice(-6)}` : `#${pin.clipId}`} · {t(pin.hidden ? "pins.hidden" : pin.clickThrough ? "pins.through" : "pins.visible")}</span>
            <button disabled={busy} onClick={() => run(() => pins.action(pin.label, "restore"))}>{t("pins.restore")}</button>
            <button disabled={busy} onClick={() => run(() => pins.action(pin.label, "close"))}>{t("pins.close")}</button>
          </div>)}
        </div>
        {error && <p role="alert" className="text-red-500 mt-2">{t("pins.failed")}: {error}</p>}
      </div>
    </>}
  </div>;
}
