import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, checkUpdate, type UpdateInfo } from "../api";
import { applyTheme } from "../theme";
import { useI18n, type Locale } from "../i18n";
import LicenseGate from "../license/LicenseGate";
import { useLicense } from "../license/useLicense";

const SITE_URL = "https://neekin.github.io/pastenext";
const PRIVACY_URL = SITE_URL;
const TERMS_URL = SITE_URL;
// 授权信息还没拉回来时的兜底购买入口,拉到之后以 Rust 侧的 purchase_url 为准
const PURCHASE_URL = `${SITE_URL}#buy`;

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="rounded-2xl bg-white dark:bg-neutral-800/60 ring-1 ring-black/5 dark:ring-white/10 p-5 space-y-3">
      <h2 className="text-sm font-semibold text-neutral-800 dark:text-neutral-100">{title}</h2>
      {children}
    </section>
  );
}

const row = "flex items-center justify-between gap-4";
const lbl = "text-[13px] text-neutral-600 dark:text-neutral-300";
const inputCls =
  "h-8 px-2 rounded-lg bg-black/5 dark:bg-white/10 text-[13px] outline-none text-neutral-800 dark:text-neutral-100 w-48";

const IS_MAC = /Mac|iPhone|iPad/.test(navigator.platform);
// 平台相关默认全局快捷键:Windows 用 Ctrl+Alt+V(更符合习惯),macOS 用 Cmd+Shift+V
const DEFAULT_HOTKEY = IS_MAC ? "CmdOrCtrl+Shift+V" : "Ctrl+Alt+V";

/** 把一次 keydown 事件转换成 Tauri 加速器字符串(如 "Ctrl+Alt+V" / "CmdOrCtrl+Shift+V")。
 *  无修饰键或非按键(仅修饰键)时返回 null,由调用方决定是否忽略。 */
function acceleratorFromEvent(e: KeyboardEvent): string | null {
  const code = e.code;
  let key: string | null = null;
  if (/^Key[A-Z]$/.test(code)) key = code.slice(3);
  else if (/^Digit[0-9]$/.test(code)) key = code.slice(5);
  else if (/^F([0-9]{1,2})$/.test(code)) key = code; // F1..F12
  else {
    const map: Record<string, string> = {
      Space: "Space",
      Enter: "Enter",
      Escape: "Escape",
      Tab: "Tab",
      Backspace: "Backspace",
      Delete: "Delete",
      ArrowUp: "Up",
      ArrowDown: "Down",
      ArrowLeft: "Left",
      ArrowRight: "Right",
      Comma: "Comma",
      Period: "Period",
      Slash: "Slash",
      Semicolon: "Semicolon",
      Quote: "Quote",
      BracketLeft: "BracketLeft",
      BracketRight: "BracketRight",
      Backslash: "Backslash",
      Minus: "Minus",
      Equal: "Equal",
      Backquote: "Backquote",
    };
    key = map[code] ?? null;
  }
  if (!key) return null;
  const mods: string[] = [];
  if (e.metaKey) mods.push("CmdOrCtrl");
  else if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (mods.length === 0) return null; // 全局快捷键必须含至少一个修饰键
  return [...mods, key].join("+");
}

/** 点击进入录制态,下一次按键即记录为该全局快捷键并立即保存。 */
function HotkeyRecorder({
  value,
  defaultValue,
  onSave,
}: {
  value: string;
  defaultValue: string;
  onSave: (accel: string) => Promise<void>;
}) {
  const { t } = useI18n();
  const [recording, setRecording] = useState(false);
  const [msg, setMsg] = useState("");
  const [display, setDisplay] = useState(value);

  useEffect(() => setDisplay(value), [value]);

  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setRecording(false);
        setMsg("");
        return;
      }
      const acc = acceleratorFromEvent(e);
      if (!acc) {
        if (e.key !== "Shift" && e.key !== "Control" && e.key !== "Alt" && e.key !== "Meta") {
          setMsg(t("settings.hotkey.needMod"));
        }
        return; // 只有修饰键,继续等待主键
      }
      setRecording(false);
      setDisplay(acc);
      setMsg("");
      onSave(acc)
        .then(() => setMsg(t("settings.hotkey.saved")))
        .catch((err) => setMsg(t("settings.hotkey.error", { message: String(err) })));
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording, onSave, t]);

  return (
    <div className="flex items-center gap-2 flex-wrap">
      <button
        type="button"
        onClick={() => {
          setMsg("");
          setRecording((r) => !r);
        }}
        className={`h-8 px-3 rounded-lg text-xs font-mono border ${
          recording
            ? "bg-indigo-500 text-white border-indigo-500"
            : "bg-black/5 dark:bg-white/10 border-black/10 dark:border-white/10 hover:bg-black/10 dark:hover:bg-white/20"
        }`}
      >
        {recording ? t("settings.hotkey.recording") : display || defaultValue}
      </button>
      {recording && (
        <span className="text-xs text-neutral-400">{t("settings.hotkey.hint")}</span>
      )}
      {!recording && display && display !== defaultValue && (
        <button
          type="button"
          onClick={() => {
            setMsg("");
            onSave(defaultValue)
              .then(() => setMsg(t("settings.hotkey.saved")))
              .catch((e) => setMsg(t("settings.hotkey.error", { message: String(e) })));
          }}
          className="text-xs text-neutral-400 hover:text-indigo-500"
        >
          {t("settings.hotkey.reset")}
        </button>
      )}
      {msg && <span className="text-xs text-neutral-400">{msg}</span>}
    </div>
  );
}

function HelpModal({ onClose }: { onClose: () => void }) {
  const { t } = useI18n();
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      onClick={onClose}
    >
      <div
        className="max-w-lg w-full max-h-[80vh] overflow-y-auto rounded-2xl bg-white dark:bg-neutral-800 ring-1 ring-black/5 dark:ring-white/10 p-6 space-y-4"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between">
          <h2 className="text-base font-semibold text-neutral-800 dark:text-neutral-100">{t("help.title")}</h2>
          <button
            onClick={onClose}
            className="h-7 w-7 rounded-lg text-neutral-400 hover:bg-black/5 dark:hover:bg-white/10"
            aria-label={t("help.close")}
          >
            ✕
          </button>
        </div>

        <p className="text-[13px] leading-6 text-neutral-600 dark:text-neutral-300">{t("help.intro")}</p>

        <div>
          <h3 className="text-[13px] font-semibold text-neutral-800 dark:text-neutral-100 mb-1">
            {t("help.shortcuts")}
          </h3>
          <ul className="text-[13px] text-neutral-600 dark:text-neutral-300 space-y-1 list-disc list-inside">
            <li>{t("help.hotkeyMac")}</li>
            <li>{t("help.hotkeyWin")}</li>
            <li>{t("help.quickPaste")}</li>
          </ul>
        </div>

        <div>
          <h3 className="text-[13px] font-semibold text-neutral-800 dark:text-neutral-100 mb-1">
            {t("help.portable")}
          </h3>
          <p className="text-[13px] leading-6 text-neutral-600 dark:text-neutral-300">{t("help.portableDesc")}</p>
        </div>

        <div>
          <h3 className="text-[13px] font-semibold text-neutral-800 dark:text-neutral-100 mb-1">
            {t("help.trial")}
          </h3>
          <p className="text-[13px] leading-6 text-neutral-600 dark:text-neutral-300">{t("help.trialDesc")}</p>
        </div>

        <div className="rounded-xl bg-indigo-50 dark:bg-indigo-500/10 p-3">
          <h3 className="text-[13px] font-semibold text-indigo-700 dark:text-indigo-300 mb-1">
            {t("help.promise")}
          </h3>
          <p className="text-[13px] leading-6 text-indigo-700/90 dark:text-indigo-200/90">
            {t("help.promiseDesc")}
          </p>
        </div>

        <div>
          <h3 className="text-[13px] font-semibold text-neutral-800 dark:text-neutral-100 mb-1">
            {t("help.faq")}
          </h3>
          <p className="text-[13px] leading-6 text-neutral-600 dark:text-neutral-300">{t("help.faqDesc")}</p>
        </div>

        <div>
          <h3 className="text-[13px] font-semibold text-neutral-800 dark:text-neutral-100 mb-1">
            {t("help.tips")}
          </h3>
          <ul className="text-[13px] text-neutral-600 dark:text-neutral-300 space-y-1 list-disc list-inside">
            <li>{t("help.tipSearch")}</li>
            <li>{t("help.tipBoard")}</li>
            <li>{t("help.tipTag")}</li>
          </ul>
        </div>

        <div>
          <h3 className="text-[13px] font-semibold text-neutral-800 dark:text-neutral-100 mb-1">
            {t("help.contact")}
          </h3>
          <p className="text-[13px] leading-6 text-neutral-600 dark:text-neutral-300">{t("help.contactDesc")}</p>
        </div>

        <button
          onClick={onClose}
          className="h-8 px-3 rounded-lg bg-indigo-500 text-white text-xs hover:bg-indigo-600"
        >
          {t("help.close")}
        </button>
      </div>
    </div>
  );
}

export default function Settings() {
  const { t, locale, setLocale } = useI18n();
  const [settings, setSettings] = useState<Record<string, string>>({});
  const [apps, setApps] = useState<string[]>([]);
  const [sourceApps, setSourceApps] = useState<string[]>([]);
  const [newApp, setNewApp] = useState("");
  const [hotkey, setHotkey] = useState(DEFAULT_HOTKEY);
  const [autostart, setAutostart] = useState(false);
  const [maxItems, setMaxItems] = useState("0");
  const [confirmClear, setConfirmClear] = useState(false);
  const isMac = /Mac|iPhone|iPad/.test(navigator.platform);
  const [axTrusted, setAxTrusted] = useState(true);
  const [version, setVersion] = useState("");
  const [helpOpen, setHelpOpen] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [updateMsg, setUpdateMsg] = useState("");
  const license = useLicense();
  const [licEmail, setLicEmail] = useState("");
  const [licKey, setLicKey] = useState("");
  const [licMsg, setLicMsg] = useState("");
  const [licBusy, setLicBusy] = useState(false);
  const [licEditing, setLicEditing] = useState(false);

  useEffect(() => {
    if (isMac) api.getAccessibilityTrusted().then(setAxTrusted).catch(() => {});
  }, [isMac]);

  useEffect(() => {
    getCurrentWindow().setTitle(`${t("app.name")} ${t("app.settings")}`).catch(() => {});
  }, [locale, t]);

  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
  }, []);

  useEffect(() => {
    api
      .getSettings()
      .then((s) => {
        setSettings(s);
        setHotkey(s.hotkey ?? DEFAULT_HOTKEY);
        setMaxItems(s.max_items ?? "0");
        if (s.locale === "zh-CN" || s.locale === "en") {
          setLocale(s.locale as Locale);
        }
      })
      .catch(() => {});
    api.getExcludedApps().then(setApps).catch(() => {});
    api.getSourceApps().then(setSourceApps).catch(() => {});
    api.getAutostart().then(setAutostart).catch(() => {});
    getCurrentWindow().setTitle(`${t("app.name")} ${t("app.settings")}`).catch(() => {});
  }, [setLocale, t]);

  const save = (k: string, v: string) => {
    setSettings((s) => ({ ...s, [k]: v }));
    api.setSetting(k, v).catch(() => {});
  };

  const saveHotkey = async (accel: string) => {
    await api.setHotkey(accel.trim());
    setSettings((s) => ({ ...s, hotkey: accel.trim() }));
    setHotkey(accel.trim());
  };

  const checkForUpdate = async () => {
    if (!version) return;
    setChecking(true);
    setUpdateMsg(t("about.checkingUpdate"));
    setUpdateInfo(null);
    try {
      const info = await checkUpdate(version);
      setUpdateInfo(info);
      setUpdateMsg(info.hasUpdate ? t("about.updateAvailable", { version: info.latest }) : t("about.updateUpToDate"));
    } catch (e) {
      setUpdateMsg(t("about.updateError", { message: String(e) }));
    } finally {
      setChecking(false);
    }
  };

  const addApp = async (name: string) => {
    const a = name.trim();
    if (!a || apps.includes(a)) return;
    await api.addExcludedApp(a).catch(() => {});
    setApps(await api.getExcludedApps());
    setNewApp("");
  };

  return (
    <div className="h-full overflow-y-auto bg-neutral-100 dark:bg-neutral-900 text-neutral-800 dark:text-neutral-100">
      <div className="max-w-2xl mx-auto p-8 space-y-5">
        <h1 className="text-lg font-bold">{t("settings.title")}</h1>

        <Section title={t("license.section")}>
          {license.info?.activated && !licEditing ? (
            <div className="space-y-3">
              <div className={row}>
                <span className={lbl}>{t("license.activatedTo", { email: license.info.email })}</span>
                <span className="inline-flex items-center gap-1.5 text-xs text-emerald-600 dark:text-emerald-400">
                  <span className="w-1.5 h-1.5 rounded-full bg-emerald-500" />
                  {t("license.status.active")}
                </span>
              </div>
              <div className={row}>
                <span className={lbl + " font-mono text-xs"}>
                  {t("license.keyLabel", { key: license.info.masked_key })}
                </span>
                <button
                  onClick={() => {
                    setLicEditing(true);
                    setLicMsg("");
                    setLicEmail(license.info?.email ?? "");
                    setLicKey("");
                  }}
                  className="text-xs text-indigo-600 dark:text-indigo-400 hover:underline"
                >
                  {t("license.change")}
                </button>
              </div>
            </div>
          ) : (
            <div className="space-y-3">
              <div className={row}>
                <span className={lbl}>
                  {license.phase === "expired"
                    ? t("license.banner.expired")
                    : t("license.banner.trial", { n: license.daysLeft })}
                </span>
                <button
                  onClick={() => {
                    if (license.info) void api.openUrl(license.info.purchase_url).catch(() => {});
                    else void api.openUrl(PURCHASE_URL).catch(() => {});
                  }}
                  className="h-8 px-3 rounded-lg bg-indigo-500 text-white text-xs hover:bg-indigo-600"
                >
                  {t("license.buy")}
                </button>
              </div>
              <div className="space-y-2">
                <input
                  value={licEmail}
                  onChange={(e) => setLicEmail(e.target.value)}
                  placeholder={t("license.emailPlaceholder")}
                  spellCheck={false}
                  className={inputCls + " w-full"}
                />
                <input
                  value={licKey}
                  onChange={(e) => setLicKey(e.target.value.toUpperCase())}
                  placeholder={t("license.keyPlaceholder")}
                  spellCheck={false}
                  className={inputCls + " w-full font-mono"}
                />
              </div>
              {licMsg && (
                <div
                  className={`text-xs ${licMsg.startsWith("✓") ? "text-emerald-600 dark:text-emerald-400" : "text-red-500 dark:text-red-400"}`}
                >
                  {licMsg}
                </div>
              )}
              {!licEditing && (
                <div className="text-[11px] text-neutral-400 dark:text-neutral-500">
                  {t("license.section.desc")}
                </div>
              )}
              <div className="flex items-center gap-2">
                <div className="flex-1" />
                {licEditing && (
                  <button
                    onClick={() => {
                      setLicEditing(false);
                      setLicMsg("");
                      setLicKey("");
                    }}
                    className="h-8 px-3 rounded-lg text-xs text-neutral-500 hover:bg-black/5 dark:hover:bg-white/10"
                  >
                    {t("detail.close")}
                  </button>
                )}
                <button
                  disabled={licBusy || !licEmail.trim() || !licKey.trim()}
                  onClick={async () => {
                    setLicBusy(true);
                    setLicMsg("");
                    try {
                      await license.activate(licEmail.trim(), licKey.trim());
                      setLicMsg("✓");
                      setLicEditing(false);
                      setLicKey("");
                    } catch (e) {
                      setLicMsg(String(e));
                    } finally {
                      setLicBusy(false);
                    }
                  }}
                  className="h-8 px-3.5 rounded-lg bg-indigo-500 text-white text-xs hover:bg-indigo-600 disabled:opacity-40 disabled:hover:bg-indigo-500"
                >
                  {t("license.activate")}
                </button>
              </div>
            </div>
          )}
        </Section>

        <Section title={t("settings.language")}>
          <div className={row}>
            <span className={lbl}>{t("settings.language.desc")}</span>
            <select
              value={locale}
              onChange={(e) => {
                const next = e.target.value as Locale;
                setLocale(next);
                api.setSetting("locale", next).catch(() => {});
              }}
              className={inputCls}
            >
              <option value="zh-CN">{t("settings.locale.zh-CN")}</option>
              <option value="en">{t("settings.locale.en")}</option>
            </select>
          </div>
        </Section>

        <Section title={t("settings.general")}>
          {isMac && (
            <>
              <div className={row}>
                <span className={lbl}>{t("settings.showDockIcon")}</span>
                <input
                  type="checkbox"
                  checked={settings.show_dock_icon === "true"}
                  onChange={async (e) => {
                    save("show_dock_icon", e.target.checked ? "true" : "false");
                    await api.setShowDockIcon(e.target.checked).catch(() => {});
                  }}
                  className="w-4 h-4 accent-indigo-500"
                />
              </div>
              <div className={row}>
                <span className={lbl}>{t("settings.accessibility")}</span>
                <div className="flex items-center gap-2">
                  <span className={`text-xs ${axTrusted ? "text-emerald-500" : "text-red-500"}`}>
                    {axTrusted ? t("settings.accessibility.authorized") : t("settings.accessibility.notAuthorized")}
                  </span>
                  {!axTrusted && (
                    <button
                      onClick={async () => setAxTrusted(await api.requestAccessibility())}
                      className="h-8 px-3 rounded-lg bg-indigo-500 text-white text-xs hover:bg-indigo-600"
                    >
                      {t("settings.accessibility.authorize")}
                    </button>
                  )}
                </div>
              </div>
            </>
          )}
          <div className={row}>
            <span className={lbl}>{t("settings.autostart")}</span>
            <input
              type="checkbox"
              checked={autostart}
              onChange={async (e) => {
                const v = e.target.checked;
                setAutostart(v);
                await api.setAutostart(v).catch(() => setAutostart(!v));
              }}
              className="w-4 h-4 accent-indigo-500"
            />
          </div>
          <div className={row}>
            <span className={lbl}>{t("settings.theme")}</span>
            <select
              value={settings.theme ?? "system"}
              onChange={(e) => {
                save("theme", e.target.value);
                applyTheme(e.target.value);
              }}
              className={inputCls}
            >
              <option value="system">{t("settings.theme.system")}</option>
              <option value="light">{t("settings.theme.light")}</option>
              <option value="dark">{t("settings.theme.dark")}</option>
            </select>
          </div>
          <div className={row}>
            <span className={lbl}>{t("settings.pastePlain")}</span>
            <input
              type="checkbox"
              checked={(settings.paste_plain_always ?? "false") === "true"}
              onChange={(e) => save("paste_plain_always", e.target.checked ? "true" : "false")}
              className="w-4 h-4 accent-indigo-500"
            />
          </div>
          <div className={row}>
            <span className={lbl}>{t("settings.plainModifier")}</span>
            <select
              value={settings.plain_modifier ?? "shift"}
              onChange={(e) => save("plain_modifier", e.target.value)}
              className={inputCls}
            >
              <option value="shift">Shift ⇧</option>
              <option value="option">Option ⌥</option>
              <option value="control">Control ⌃</option>
            </select>
          </div>
          <div className={row}>
            <span className={lbl}>{t("settings.soundEnabled")}</span>
            <input
              type="checkbox"
              checked={(settings.sound_enabled ?? "true") === "true"}
              onChange={(e) => save("sound_enabled", e.target.checked ? "true" : "false")}
              className="w-4 h-4 accent-indigo-500"
            />
          </div>
          {isMac && (
            <div className={row}>
              <span className={lbl}>{t("settings.showTrayIcon")}</span>
              <input
                type="checkbox"
                checked={(settings.show_tray_icon ?? "true") === "true"}
                onChange={async (e) => {
                  save("show_tray_icon", e.target.checked ? "true" : "false");
                  await api.setShowTrayIcon(e.target.checked).catch(() => {});
                }}
                className="w-4 h-4 accent-indigo-500"
              />
            </div>
          )}
          <div className={row}>
            <span className={lbl}>{t("settings.trayLeftAction")}</span>
            <select
              value={settings.tray_left_action ?? "panel"}
              onChange={async (e) => {
                save("tray_left_action", e.target.value);
                await api.setTrayLeftAction(e.target.value).catch(() => {});
              }}
              className={inputCls}
            >
              <option value="panel">{t("settings.trayLeftAction.panel")}</option>
              <option value="menu">{t("settings.trayLeftAction.menu")}</option>
            </select>
          </div>
          <div className={row}>
            <span className={lbl}>{t("settings.autoPaste")}</span>
            <input
              type="checkbox"
              checked={(settings.auto_paste ?? "true") === "true"}
              onChange={(e) => save("auto_paste", e.target.checked ? "true" : "false")}
              className="w-4 h-4 accent-indigo-500"
            />
          </div>
          <div className={row}>
            <span className={lbl}>
              {t("settings.quickPaste")}
              <span className="ml-1 text-xs text-neutral-400">{t("settings.quickPaste.desc")}</span>
            </span>
            <input
              type="checkbox"
              checked={(settings.quick_paste_enabled ?? "true") === "true"}
              onChange={async (e) => {
                save("quick_paste_enabled", e.target.checked ? "true" : "false");
                await api.setQuickPasteEnabled(e.target.checked).catch(() => {});
              }}
              className="w-4 h-4 accent-indigo-500"
            />
          </div>
          <div className={row}>
            <span className={lbl}>{t("settings.hotkey")}</span>
            <HotkeyRecorder value={hotkey} defaultValue={DEFAULT_HOTKEY} onSave={saveHotkey} />
          </div>
        </Section>

        <Section title={t("settings.excludedApps")}>
          <div className="flex flex-wrap gap-1.5">
            {apps.length === 0 && <span className="text-xs text-neutral-400">{t("settings.excluded.empty")}</span>}
            {apps.map((a) => (
              <span
                key={a}
                className="px-2 py-1 rounded-full bg-neutral-100 dark:bg-neutral-700 text-xs flex items-center gap-1"
              >
                {a}
                <button
                  onClick={async () => {
                    await api.removeExcludedApp(a).catch(() => {});
                    setApps(await api.getExcludedApps());
                  }}
                  className="text-neutral-400 hover:text-red-500"
                >
                  ×
                </button>
              </span>
            ))}
          </div>
          <div className="flex items-center gap-2">
            <input
              value={newApp}
              onChange={(e) => setNewApp(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && addApp(newApp)}
              placeholder={t("settings.excluded.placeholder")}
              className={inputCls + " w-64"}
            />
            <button
              onClick={() => addApp(newApp)}
              className="h-8 px-3 rounded-lg bg-indigo-500 text-white text-xs hover:bg-indigo-600"
            >
              {t("settings.excluded.add")}
            </button>
          </div>
          {sourceApps.length > 0 && (
            <div className="flex flex-wrap gap-1.5 items-center">
              <span className="text-xs text-neutral-400">{t("settings.excluded.recent")}</span>
              {sourceApps
                .filter((a) => !apps.some((x) => x.toLowerCase() === a.toLowerCase()))
                .slice(0, 8)
                .map((a) => (
                  <button
                    key={a}
                    onClick={() => addApp(a)}
                    className="px-2 py-0.5 rounded-full bg-neutral-100 dark:bg-neutral-700 text-[11px] text-neutral-500 dark:text-neutral-300 hover:bg-indigo-100 dark:hover:bg-indigo-500/20"
                  >
                    + {a}
                  </button>
                ))}
            </div>
          )}
        </Section>

        <Section title={t("settings.history")}>
          <div className={row}>
            <span className={lbl}>{t("settings.retention")}</span>
            <select
              value={settings.retention_days ?? "0"}
              onChange={(e) => save("retention_days", e.target.value)}
              className={inputCls}
            >
              <option value="0">{t("settings.retention.unlimited")}</option>
              <option value="30">{t("settings.retention.30")}</option>
              <option value="90">{t("settings.retention.90")}</option>
              <option value="365">{t("settings.retention.365")}</option>
            </select>
          </div>
          <div className={row}>
            <span className={lbl}>{t("settings.maxItems")}</span>
            <input
              type="number"
              min={0}
              value={maxItems}
              onChange={(e) => setMaxItems(e.target.value)}
              onBlur={() => {
                const n = Math.max(0, Number(maxItems) || 0);
                setMaxItems(String(n));
                save("max_items", String(n));
              }}
              className={inputCls}
            />
          </div>
          <div className={row}>
            <span className={lbl}>{t("settings.clearHistory")}</span>
            <button
              onClick={async () => {
                if (!confirmClear) {
                  setConfirmClear(true);
                  setTimeout(() => setConfirmClear(false), 3000);
                  return;
                }
                await api.clearHistory().catch(() => {});
                setConfirmClear(false);
              }}
              className={`h-8 px-3 rounded-lg text-xs ${
                confirmClear
                  ? "bg-red-500 text-white"
                  : "bg-red-50 dark:bg-red-500/15 text-red-600 dark:text-red-400"
              }`}
            >
              {confirmClear ? t("settings.clearHistory.confirm") : t("settings.clearHistory.button")}
            </button>
          </div>
        </Section>

        <Section title={t("about.title")}>
          <div className={row}>
            <span className={lbl}>{t("about.version")}</span>
            <span className="text-[13px] text-neutral-500 dark:text-neutral-400">{version || "—"}</span>
          </div>
          <div className={row}>
            <span className={lbl}>{t("about.help")}</span>
            <div className="flex flex-wrap items-center gap-2">
              <button
                onClick={() => setHelpOpen(true)}
                className="h-8 px-3 rounded-lg bg-black/5 dark:bg-white/10 text-xs hover:bg-black/10 dark:hover:bg-white/20"
              >
                {t("help.open")}
              </button>
              <button
                onClick={checkForUpdate}
                disabled={checking}
                className="h-8 px-3 rounded-lg bg-black/5 dark:bg-white/10 text-xs hover:bg-black/10 dark:hover:bg-white/20 disabled:opacity-50"
              >
                {checking ? t("about.checkingUpdate") : t("about.checkUpdate")}
              </button>
              {updateInfo?.hasUpdate && (
                <button
                  onClick={() => api.openUrl(updateInfo.url).catch(() => {})}
                  className="h-8 px-3 rounded-lg bg-indigo-500 text-white text-xs hover:bg-indigo-600"
                >
                  {t("about.downloadUpdate")}
                </button>
              )}
            </div>
          </div>
          {updateMsg && (
            <p className="text-[11px] leading-5 text-neutral-400 dark:text-neutral-500">{updateMsg}</p>
          )}
          <div>
            <div className={lbl}>{t("about.licenses")}</div>
            <p className="mt-1 text-[11px] leading-5 text-neutral-400 dark:text-neutral-500">
              {t("about.licenses.desc")}
            </p>
          </div>
          <div className="flex gap-2">
            <button
              onClick={() => api.openUrl(PRIVACY_URL).catch(() => {})}
              className="h-8 px-3 rounded-lg bg-black/5 dark:bg-white/10 text-xs hover:bg-black/10 dark:hover:bg-white/20"
            >
              {t("about.privacy")}
            </button>
            <button
              onClick={() => api.openUrl(TERMS_URL).catch(() => {})}
              className="h-8 px-3 rounded-lg bg-black/5 dark:bg-white/10 text-xs hover:bg-black/10 dark:hover:bg-white/20"
            >
              {t("about.terms")}
            </button>
          </div>
          <div className="rounded-xl bg-indigo-50 dark:bg-indigo-500/10 p-3 mt-2">
            <h3 className="text-[13px] font-semibold text-indigo-700 dark:text-indigo-300 mb-1">
              {t("about.promise")}
            </h3>
            <p className="text-[13px] leading-6 text-indigo-700/90 dark:text-indigo-200/90">
              {t("about.promiseDesc")}
            </p>
          </div>
        </Section>

        <p className="text-xs text-neutral-400 text-center pb-4">
          {t("settings.footer", { name: t("app.name") })}
        </p>
      </div>

      {helpOpen && <HelpModal onClose={() => setHelpOpen(false)} />}

      <LicenseGate license={license} />
    </div>
  );
}
