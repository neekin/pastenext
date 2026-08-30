import type { LicenseInfo } from "../api";

export const TRIAL_DAYS = 7;
const DAY_MS = 86_400_000;

/**
 * - `licensed` 已激活,永不打扰
 * - `firstRun` 首次启动当天,弹一次「欢迎试用 / 直接激活」
 * - `trial`    试用期内(第 2–7 天),**不打扰**
 * - `expired`  试用期结束,每天首次打开时提醒一次
 */
export type LicensePhase = "licensed" | "firstRun" | "trial" | "expired";

export interface LicensePhaseResult {
  phase: LicensePhase;
  /** 试用剩余天数(已激活时为 0) */
  daysLeft: number;
  /** 当前是否应该弹窗 */
  needsPrompt: boolean;
}

/** 本地时区的日期键。用它而不是 UTC,否则东八区要到早上 8 点才算新的一天。 */
export function dayKey(ms: number): string {
  if (!ms) return "";
  const d = new Date(ms);
  return `${d.getFullYear()}-${d.getMonth() + 1}-${d.getDate()}`;
}

/**
 * 判定试用阶段与是否需要弹窗。
 *
 * 这是纯函数:不碰网络、不碰 React,输入一份授权快照就能得到结论,方便直接断言。
 */
export function resolveLicensePhase(
  info: LicenseInfo,
  dismissedToday: boolean
): LicensePhaseResult {
  if (info.activated) {
    return { phase: "licensed", daysLeft: 0, needsPrompt: false };
  }

  // 系统时间被往回拨时 now 会小于首次启动时间。此时按已过期处理 ——
  // 回拨时钟不该成为无限续期的手段。
  const clockRolledBack = info.now < info.first_launch_at;
  const daysUsed = Math.floor((info.now - info.first_launch_at) / DAY_MS);
  const expired = clockRolledBack || daysUsed >= TRIAL_DAYS;
  const daysLeft = Math.max(0, TRIAL_DAYS - Math.max(0, daysUsed));

  if (expired) {
    // 只在当天还没提醒过时弹一次
    return {
      phase: "expired",
      daysLeft: 0,
      needsPrompt: !dismissedToday && dayKey(info.last_prompt_at) !== dayKey(info.now),
    };
  }
  if (info.last_prompt_at === 0) {
    // 还没弹过任何窗 = 首次启动,给一次「欢迎试用」的说明
    return { phase: "firstRun", daysLeft, needsPrompt: !dismissedToday };
  }
  // 试用期内的第 2–7 天:安静
  return { phase: "trial", daysLeft, needsPrompt: false };
}
