import { useEffect, useState } from "react";
import { api } from "../api";
import { useI18n } from "../i18n";
import { TRIAL_DAYS, type License } from "./useLicense";

/**
 * 授权提醒弹窗。
 *
 * 两种形态:
 * - `firstRun` 首次启动:说明有 7 天试用,想直接买就填序列号
 * - `expired`  试用结束:每天提醒一次,两个出口 —— 「继续使用」或「关闭」
 *
 * 试用期内(第 2–7 天)不显示,见 useLicense.ts。
 */
export default function LicenseGate({ license }: { license: License }) {
  const { t } = useI18n();
  const { phase, info, daysLeft, needsPrompt, dismiss, activate } = license;

  const [email, setEmail] = useState("");
  const [serial, setSerial] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [justActivated, setJustActivated] = useState(false);

  const expired = phase === "expired";
  const open = needsPrompt && !justActivated;

  // 每次弹窗重新出现时清掉上一次的输入与报错
  useEffect(() => {
    if (open) {
      setError("");
      setBusy(false);
    }
  }, [open]);

  if (!open || !info) return null;

  const submit = async () => {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      await activate(email.trim(), serial.trim());
      setJustActivated(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const buy = () => {
    void api.openUrl(info.purchase_url).catch(() => {});
  };

  return (
    <div data-no-autohide className="fixed inset-0 z-[100] flex items-center justify-center">
      <div className="absolute inset-0 bg-black/40 backdrop-blur-[2px]" onClick={() => void dismiss()} />

      <div className="relative w-[430px] max-w-[92vw] rounded-2xl bg-white dark:bg-neutral-800 shadow-2xl ring-1 ring-black/10 dark:ring-white/10 overflow-hidden">
        {/* 顶部色带:试用中偏中性,过期后转为提示色 */}
        <div
          className={`h-1 ${expired ? "bg-amber-500" : "bg-gradient-to-r from-indigo-500 to-violet-500"}`}
        />

        <div className="p-5">
          <div className="flex items-start gap-3">
            <div
              className={`w-9 h-9 shrink-0 rounded-xl flex items-center justify-center text-base ${
                expired
                  ? "bg-amber-500/15 text-amber-600 dark:text-amber-400"
                  : "bg-indigo-500/15 text-indigo-600 dark:text-indigo-400"
              }`}
            >
              {expired ? "⏳" : "✨"}
            </div>
            <div className="min-w-0 flex-1">
              <div className="text-[15px] font-semibold text-neutral-900 dark:text-neutral-50">
                {expired ? t("license.expired.title") : t("license.trial.title")}
              </div>
              <div className="mt-0.5 text-xs leading-relaxed text-neutral-500 dark:text-neutral-400">
                {expired
                  ? t("license.expired.subtitle")
                  : t("license.trial.subtitle", { n: daysLeft || TRIAL_DAYS })}
              </div>
            </div>
            <button
              onClick={() => void dismiss()}
              title={t("license.close")}
              className="-mr-1 -mt-1 w-7 h-7 rounded-lg text-neutral-400 hover:bg-black/5 dark:hover:bg-white/10 hover:text-neutral-600 dark:hover:text-neutral-200"
            >
              ✕
            </button>
          </div>

          <div className="mt-4 space-y-2">
            <input
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder={t("license.emailPlaceholder")}
              spellCheck={false}
              className="w-full h-9 px-3 rounded-lg bg-neutral-100 dark:bg-neutral-900 text-sm text-neutral-800 dark:text-neutral-100 placeholder:text-neutral-400 outline-none ring-1 ring-transparent focus:ring-indigo-500/60"
            />
            <input
              value={serial}
              onChange={(e) => setSerial(e.target.value.toUpperCase())}
              onKeyDown={(e) => e.key === "Enter" && void submit()}
              placeholder={t("license.keyPlaceholder")}
              spellCheck={false}
              className="w-full h-9 px-3 rounded-lg bg-neutral-100 dark:bg-neutral-900 text-sm font-mono tracking-wide text-neutral-800 dark:text-neutral-100 placeholder:font-sans placeholder:tracking-normal placeholder:text-neutral-400 outline-none ring-1 ring-transparent focus:ring-indigo-500/60"
            />
            {error && (
              <div className="text-xs text-red-500 dark:text-red-400">{t("license.error", { message: error })}</div>
            )}
          </div>

          <div className="mt-4 flex items-center gap-2">
            <button
              onClick={buy}
              className="h-9 px-3.5 rounded-lg text-sm text-neutral-600 dark:text-neutral-300 hover:bg-black/5 dark:hover:bg-white/10"
            >
              {t("license.buy")}
            </button>
            <div className="flex-1" />
            <button
              onClick={() => void dismiss()}
              className="h-9 px-3.5 rounded-lg text-sm text-neutral-500 dark:text-neutral-400 hover:bg-black/5 dark:hover:bg-white/10"
            >
              {expired ? t("license.keepUsing") : t("license.keepUsing")}
            </button>
            <button
              onClick={() => void submit()}
              disabled={busy || !email.trim() || !serial.trim()}
              className="h-9 px-4 rounded-lg bg-indigo-500 text-white text-sm font-medium hover:bg-indigo-600 disabled:opacity-40 disabled:hover:bg-indigo-500"
            >
              {t("license.activate")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
