import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useI18n } from "../i18n";
import { pins } from "./api";
import "./pins.css";

export default function PinOpacity() {
  const { t } = useI18n();
  const [percent, setPercent] = useState(100), [input, setInput] = useState("100"), [error, setError] = useState("");
  const chain = useRef(Promise.resolve());
  useEffect(() => {
    let alive = true;
    const refresh = () => void pins.opacityInfo().then(info => {
      if (alive) { const value = Math.round(info.opacity * 100); setPercent(value); setInput(String(value)); }
    }).catch(e => { if (alive) setError(String(e)); });
    refresh();
    const focus = getCurrentWindow().onFocusChanged(e => { if (e.payload) refresh(); });
    return () => { alive = false; focus.then(stop => stop()); };
  }, []);
  const change = (value: number) => {
    setPercent(value);
    chain.current = chain.current.then(() => pins.setOpacity(value / 100)).then(() => setError("")).catch(e => setError(String(e)));
  };
  return <div className="pin-opacity-popover" onKeyDown={e => { if (e.key === "Escape" || e.key === "Enter") void getCurrentWindow().hide(); }}>
    <div className="pin-opacity-heading"><strong>{t("pins.opacity")}</strong><label>
      <input type="number" min="15" max="100" step="1" value={input} aria-label={t("pins.opacityValue")}
        onChange={e => { setInput(e.target.value); const value = Number(e.target.value); if (e.target.value && value >= 15 && value <= 100) change(value); }}
        onBlur={() => setInput(String(percent))} /><span>%</span>
    </label></div>
    <input type="range" min="15" max="100" step="1" value={percent} aria-label={t("pins.opacity")}
      onChange={e => { const value = Number(e.target.value); setInput(String(value)); change(value); }} />
    {error && <div role="alert" className="pin-opacity-error">{error}</div>}
  </div>;
}
