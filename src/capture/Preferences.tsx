import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { api } from "../api";
import { useI18n } from "../i18n";
export interface CapturePreferences {
  highResolution: boolean;
  sound: boolean;
  autoSave: boolean;
  saveDir: string;
  format: string;
  quality: number;
  editAfter: boolean;
}
export const defaultPreferences: CapturePreferences = {
  highResolution: true,
  sound: false,
  autoSave: false,
  saveDir: "",
  format: "png",
  quality: 92,
  editAfter: true,
};
export default function Preferences() {
  const { locale } = useI18n();
  const l = (zh: string, en: string) => (locale === "en" ? en : zh);
  const [prefs, setPrefs] = useState(defaultPreferences),
    [error, setError] = useState("");
  useEffect(() => {
    void api.getSettings().then((s) => {
      try {
        setPrefs({
          ...defaultPreferences,
          ...JSON.parse(s.capture_preferences || "{}"),
        });
      } catch {}
    });
  }, []);
  async function update(patch: Partial<CapturePreferences>) {
    const next = { ...prefs, ...patch };
    try {
      await api.setSetting("capture_preferences", JSON.stringify(next));
      setPrefs(next);
      setError("");
    } catch (e) {
      setError(String(e));
    }
  }
  async function folder() {
    try {
      const path = await invoke<string | null>("capture_choose_folder");
      if (path) await update({ saveDir: path });
    } catch (e) {
      setError(String(e));
    }
  }
  return (
    <details className="capture-preferences">
      <summary>{l("截图偏好设置", "Capture preferences")}</summary>
      <div className="capture-options">
        <label>
          {l("图片分辨率", "Resolution")}
          <select
            value={prefs.highResolution ? "retina" : "standard"}
            onChange={(e) =>
              void update({ highResolution: e.target.value === "retina" })
            }
          >
            <option value="retina">Retina {l("高清", "quality")}</option>
            <option value="standard">{l("普通清晰度", "Standard (1×)")}</option>
          </select>
        </label>
        <label>
          {l("默认格式", "Default format")}
          <select
            value={prefs.format}
            onChange={(e) => void update({ format: e.target.value })}
          >
            <option value="png">PNG</option>
            <option value="jpg">JPEG</option>
          </select>
        </label>
        {prefs.format === "jpg" && (
          <label>
            JPEG {l("质量", "quality")}
            <input
              type="range"
              min="20"
              max="100"
              value={prefs.quality}
              onChange={(e) =>
                void update({
                  quality: Math.max(20, Math.min(100, Number(e.target.value))),
                })
              }
            />
          </label>
        )}
        <label>
          <input
            type="checkbox"
            checked={prefs.sound}
            onChange={(e) => void update({ sound: e.target.checked })}
          />
          {l("截图完成时播放声音", "Play capture sound")}
        </label>
        <label>
          <input
            type="checkbox"
            checked={prefs.editAfter}
            onChange={(e) => void update({ editAfter: e.target.checked })}
          />
          {l("截图后立即编辑", "Edit after capture")}
        </label>
        <label>
          <input
            type="checkbox"
            checked={prefs.autoSave}
            disabled={!prefs.saveDir}
            onChange={(e) => void update({ autoSave: e.target.checked })}
          />
          {l("自动保存到文件夹", "Automatically save to folder")}
        </label>
      </div>
      <button onClick={() => void folder()}>
        {prefs.saveDir || l("选择保存文件夹", "Choose save folder")}
      </button>
      {error && (
        <div role="alert" className="capture-error">
          {error}
        </div>
      )}
    </details>
  );
}
