import { createContext, useCallback, useContext, useEffect, useState } from "react";
import { zhCN } from "./zh";
import { en } from "./en";

const resources = {
  "zh-CN": zhCN,
  en,
} as const;

export type Locale = keyof typeof resources;
export type I18nKey = keyof typeof zhCN;

const STORAGE_KEY = "pastenext-locale";

export interface I18nCtx {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: (key: I18nKey, vars?: Record<string, string | number>) => string;
}

const Ctx = createContext<I18nCtx>({
  locale: "zh-CN",
  setLocale: () => {},
  t: (k) => k,
});

function detectLocale(): Locale {
  if (typeof window === "undefined") return "zh-CN";
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored && stored in resources) return stored as Locale;
  const nav = navigator.language;
  if (nav.startsWith("zh")) return "zh-CN";
  return "en";
}

export function I18nProvider({ children }: { children: React.ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(detectLocale);

  const setLocale = useCallback((next: Locale) => {
    setLocaleState(next);
    if (typeof window !== "undefined") {
      localStorage.setItem(STORAGE_KEY, next);
    }
  }, []);

  useEffect(() => {
    const onStorage = (e: StorageEvent) => {
      if (e.key === STORAGE_KEY && e.newValue && e.newValue in resources) {
        setLocaleState(e.newValue as Locale);
      }
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);

  const t = useCallback(
    (key: I18nKey, vars?: Record<string, string | number>) => {
      const dict = resources[locale] ?? resources["zh-CN"];
      let text = (dict[key] ?? resources["zh-CN"][key] ?? key) as string;
      if (vars) {
        for (const [k, v] of Object.entries(vars)) {
          text = text.replace(new RegExp(`\\{${k}\\}`, "g"), String(v));
        }
      }
      return text;
    },
    [locale]
  );

  return <Ctx.Provider value={{ locale, setLocale, t }}>{children}</Ctx.Provider>;
}

export const useI18n = () => useContext(Ctx);
