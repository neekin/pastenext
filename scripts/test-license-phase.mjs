/**
 * 试用弹窗策略的行为测试。
 *
 * 这里锁定的核心约定是:**7 天试用期内(第 2–7 天)不打扰用户**,
 * 只在首次启动弹一次、以及试用期结束后每天弹一次。
 *
 * 运行:node --experimental-strip-types scripts/test-license-phase.mjs
 * 或:  pnpm test:license
 */
import assert from "node:assert/strict";
import { resolveLicensePhase, dayKey, TRIAL_DAYS } from "../src/license/phase.ts";

const DAY = 86_400_000;
// 2026-08-31 12:00 本地时间,避开任何跨日的边界抖动
const T0 = new Date(2026, 7, 31, 12, 0, 0).getTime();

const info = (over = {}) => ({
  activated: false,
  email: "",
  masked_key: "",
  first_launch_at: T0,
  last_prompt_at: 0,
  now: T0,
  purchase_url: "https://example.com/buy",
  ...over,
});

let passed = 0;
function check(name, fn) {
  fn();
  passed += 1;
  console.log(`  ✓ ${name}`);
}

console.log("\n试用期策略\n");

check("首次启动:弹一次,阶段为 firstRun", () => {
  const r = resolveLicensePhase(info({ now: T0 }), false);
  assert.equal(r.phase, "firstRun");
  assert.equal(r.needsPrompt, true);
  assert.equal(r.daysLeft, TRIAL_DAYS);
});

check("首次启动点了关闭:当天不再弹", () => {
  const r = resolveLicensePhase(info({ now: T0 }), true);
  assert.equal(r.needsPrompt, false);
});

check("第 1 天晚些时候重启:last_prompt 已写入,不重复弹", () => {
  const r = resolveLicensePhase(info({ now: T0 + 3 * 3_600_000, last_prompt_at: T0 }), false);
  assert.equal(r.phase, "trial");
  assert.equal(r.needsPrompt, false);
});

check("第 2 天:安静,不打扰", () => {
  const r = resolveLicensePhase(info({ now: T0 + DAY, last_prompt_at: T0 }), false);
  assert.equal(r.phase, "trial");
  assert.equal(r.needsPrompt, false);
  assert.equal(r.daysLeft, 6);
});

check("第 7 天(试用最后一天):依然安静", () => {
  const r = resolveLicensePhase(info({ now: T0 + 6 * DAY, last_prompt_at: T0 }), false);
  assert.equal(r.phase, "trial");
  assert.equal(r.needsPrompt, false);
  assert.equal(r.daysLeft, 1);
});

check("第 8 天(试用结束):开始每天提醒一次", () => {
  const r = resolveLicensePhase(info({ now: T0 + 7 * DAY, last_prompt_at: T0 }), false);
  assert.equal(r.phase, "expired");
  assert.equal(r.needsPrompt, true);
  assert.equal(r.daysLeft, 0);
});

check("过期当天点了「继续使用」:当天不再弹", () => {
  const t8 = T0 + 7 * DAY;
  const r = resolveLicensePhase(info({ now: t8, last_prompt_at: t8 }), false);
  assert.equal(r.needsPrompt, false);
});

check("过期次日重新打开:再提醒一次", () => {
  const t8 = T0 + 7 * DAY;
  const t9 = T0 + 8 * DAY;
  const r = resolveLicensePhase(info({ now: t9, last_prompt_at: t8 }), false);
  assert.equal(r.phase, "expired");
  assert.equal(r.needsPrompt, true);
});

check("一天内反复打开:只提醒一次", () => {
  const t9 = T0 + 8 * DAY;
  const dismissedAt = t9 + 2 * 3_600_000;
  const r1 = resolveLicensePhase(info({ now: t9 + 6 * 3_600_000, last_prompt_at: dismissedAt }), false);
  const r2 = resolveLicensePhase(info({ now: t9 + 10 * 3_600_000, last_prompt_at: dismissedAt }), false);
  assert.equal(r1.needsPrompt, false);
  assert.equal(r2.needsPrompt, false);
});

check("已激活:永不打扰", () => {
  const r = resolveLicensePhase(info({ activated: true, now: T0 + 30 * DAY }), false);
  assert.equal(r.phase, "licensed");
  assert.equal(r.needsPrompt, false);
});

check("时钟回拨:按已过期处理,不能靠改时间无限续期", () => {
  const r = resolveLicensePhase(info({ now: T0 - 30 * DAY, last_prompt_at: 0 }), false);
  assert.equal(r.phase, "expired");
  assert.equal(r.needsPrompt, true);
});

console.log("\n日期键\n");

check("同一天的不同时刻得到同一个键", () => {
  const a = new Date(2026, 7, 31, 0, 5).getTime();
  const b = new Date(2026, 7, 31, 23, 55).getTime();
  assert.equal(dayKey(a), dayKey(b));
});

check("跨日得到不同的键", () => {
  const a = new Date(2026, 7, 31, 23, 59).getTime();
  const b = new Date(2026, 8, 1, 0, 1).getTime();
  assert.notEqual(dayKey(a), dayKey(b));
});

check("0 得到空串(从未弹过窗)", () => {
  assert.equal(dayKey(0), "");
});

console.log(`\n全部 ${passed} 项通过\n`);
