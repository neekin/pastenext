import { useEffect } from "react";
import Panel from "./panel/Panel";
import Settings from "./settings/Settings";
import { api, onSettingsChanged, onPanelShown } from "./api";
import { applyTheme } from "./theme";
import { useI18n, type Locale } from "./i18n";

export default function App() {
  // 双入口:index.html → 面板,settings.html → 设置窗口
  const isSettings = window.location.pathname.endsWith("settings.html");
  const { setLocale } = useI18n();

  useEffect(() => {
    const refresh = () =>
      api
        .getSettings()
        .then((s) => {
          applyTheme(s.theme);
          if (s.locale === "zh-CN" || s.locale === "en") {
            setLocale(s.locale as Locale);
          }
        })
        .catch(() => {});
    refresh();
    // 设置窗口里改了主题 / 语言 → 所有窗口实时生效
    const u1 = onSettingsChanged((key, value) => {
      if (key === "theme") applyTheme(value);
      if (key === "locale" && (value === "zh-CN" || value === "en")) {
        setLocale(value as Locale);
      }
    });
    // 面板每次唤起也刷新一次(覆盖设置在隐藏期间被修改的情况)
    const u2 = onPanelShown(refresh);
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onSys = () => refresh();
    mq.addEventListener("change", onSys);
    return () => {
      u1.then((f) => f());
      u2.then((f) => f());
      mq.removeEventListener("change", onSys);
    };
  }, [setLocale]);

  return isSettings ? <Settings /> : <Panel />;
}
