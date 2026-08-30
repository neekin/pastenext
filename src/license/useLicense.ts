import { useCallback, useEffect, useState } from "react";
import { api, onSettingsChanged, type LicenseInfo } from "../api";
import { resolveLicensePhase, type LicensePhase } from "./phase";

export { TRIAL_DAYS } from "./phase";
export type { LicensePhase } from "./phase";

export interface License {
  info: LicenseInfo | null;
  phase: LicensePhase;
  /** 试用剩余天数(已激活时为 0) */
  daysLeft: number;
  /** 当前是否应该弹窗 */
  needsPrompt: boolean;
  reload: () => Promise<void>;
  /** 记住「这次不用再提醒了」,今天之内不再弹 */
  dismiss: () => Promise<void>;
  activate: (email: string, key: string) => Promise<void>;
}

export function useLicense(): License {
  const [info, setInfo] = useState<LicenseInfo | null>(null);
  const [dismissedToday, setDismissedToday] = useState(false);

  const reload = useCallback(async () => {
    try {
      setInfo(await api.getLicenseInfo());
    } catch (e) {
      // 拿不到授权信息时按「已激活」处理,宁可少弹也不能弹坏
      console.error("[license] get_license_info failed", e);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  // 另一个窗口激活或关闭了提示弹窗时同步过来
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onSettingsChanged((key) => {
      if (
        key === "license_activated" ||
        key === "license_last_prompt_at" ||
        key === "license_email"
      ) {
        void reload();
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [reload]);

  const dismiss = useCallback(async () => {
    setDismissedToday(true);
    await api.dismissLicensePrompt().catch(() => {});
  }, []);

  const activate = useCallback(
    async (email: string, key: string) => {
      await api.activateLicense(email, key);
      // 重新拉一次,让 Rust 侧的激活状态成为唯一事实来源
      await reload();
    },
    [reload]
  );

  // 还没拉到授权信息时按已激活处理,避免在加载阶段闪一下弹窗
  const resolved = info
    ? resolveLicensePhase(info, dismissedToday)
    : { phase: "licensed" as LicensePhase, daysLeft: 0, needsPrompt: false };

  return { info, ...resolved, reload, dismiss, activate };
}
