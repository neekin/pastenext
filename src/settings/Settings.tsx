import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "../api";
import { applyTheme } from "../theme";
import { useI18n, type Locale } from "../i18n";
import LicenseGate from "../license/LicenseGate";
import { useLicense } from "../license/useLicense";

const REPO_URL = "https://github.com/neekin/pastenext";
const PRIVACY_URL = `${REPO_URL}/blob/main/PRIVACY.md`;
const TERMS_URL = `${REPO_URL}/blob/main/TERMS.md`;
// 授权信息还没拉回来时的兜底购买入口,拉到之后以 Rust 侧的 purchase_url 为准
const PURCHASE_URL = `${REPO_URL}#buy`;

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

export default function Settings() {
  const { t, locale, setLocale } = useI18n();
  const [settings, setSettings] = useState<Record<string, string>>({});
  const [apps, setApps] = useState<string[]>([]);
  const [sourceApps, setSourceApps] = useState<string[]>([]);
  const [newApp, setNewApp] = useState("");
  const [hotkey, setHotkey] = useState("CmdOrCtrl+Shift+V");
  const [hotkeyMsg, setHotkeyMsg] = useState("");
  const [autostart, setAutostart] = useState(false);
  const [maxItems, setMaxItems] = useState("0");
  const [confirmClear, setConfirmClear] = useState(false);
  const isMac = /Mac|iPhone|iPad/.test(navigator.platform);
  const [axTrusted, setAxTrusted] = useState(true);
  const [version, setVersion] = useState("");
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
        setHotkey(s.hotkey ?? "CmdOrCtrl+Shift+V");
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

  const saveHotkey = async () => {
    setHotkeyMsg("");
    try {
      await api.setHotkey(hotkey.trim());
      setSettings((s) => ({ ...s, hotkey: hotkey.trim() }));
      setHotkeyMsg(t("settings.hotkey.saved"));
    } catch (e) {
      setHotkeyMsg(t("settings.hotkey.error", { message: String(e) }));
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
            <div className="flex items-center gap-2">
              {hotkeyMsg && <span className="text-xs text-neutral-400">{hotkeyMsg}</span>}
              <input
                value={hotkey}
                onChange={(e) => setHotkey(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && saveHotkey()}
                className={inputCls + " font-mono"}
              />
              <button
                onClick={saveHotkey}
                className="h-8 px-3 rounded-lg bg-indigo-500 text-white text-xs hover:bg-indigo-600"
              >
                {t("settings.hotkey.apply")}
              </button>
            </div>
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
            <span className={lbl}>{t("about.source")}</span>
            <button
              onClick={() => api.openUrl(REPO_URL).catch(() => {})}
              className="h-8 px-3 rounded-lg bg-black/5 dark:bg-white/10 text-xs hover:bg-black/10 dark:hover:bg-white/20"
            >
              {t("about.repo")}
            </button>
          </div>
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
        </Section>

        <p className="text-xs text-neutral-400 text-center pb-4">
          {t("settings.footer", { name: t("app.name") })}
        </p>
      </div>

      <LicenseGate license={license} />
    </div>
  );
}
