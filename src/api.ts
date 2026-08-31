import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Board, Clip, ClipKind, Tag } from "./types";

export interface LicenseInfo {
  activated: boolean;
  email: string;
  masked_key: string;
  first_launch_at: number;
  last_prompt_at: number;
  now: number;
  purchase_url: string;
}

export interface ListParams {
  query?: string | null;
  kind?: ClipKind | null;
  boardId?: number | null;
  tag?: string | null;
  limit?: number;
  offset?: number;
}

export const api = {
  listClips: (p: ListParams) =>
    invoke<Clip[]>("list_clips", {
      query: p.query ?? null,
      kind: p.kind ?? null,
      boardId: p.boardId ?? null,
      tag: p.tag ?? null,
      limit: p.limit ?? 200,
      offset: p.offset ?? 0,
    }),
  getClip: (id: number) => invoke<Clip | null>("get_clip", { id }),
  getBoards: () => invoke<Board[]>("get_boards"),
  createBoard: (name: string) => invoke<Board>("create_board", { name }),
  renameBoard: (id: number, name: string) => invoke<void>("rename_board", { id, name }),
  deleteBoard: (id: number) => invoke<void>("delete_board", { id }),
  getTags: () => invoke<Tag[]>("get_tags"),
  copyClip: (id: number) => invoke<void>("copy_clip", { id }),
  pasteClip: (id: number, plain?: boolean) => invoke<void>("paste_clip", { id, plain: plain ?? null }),
  deleteClip: (id: number) => invoke<void>("delete_clip", { id }),
  clearHistory: () => invoke<void>("clear_history"),
  editClip: (id: number, text: string) => invoke<void>("edit_clip", { id, text }),
  setNote: (id: number, note: string) => invoke<void>("set_note", { id, note }),
  addTag: (clipId: number, name: string) => invoke<Tag>("add_tag", { clipId, name }),
  removeTag: (clipId: number, tagId: number) => invoke<void>("remove_tag", { clipId, tagId }),
  moveClipToBoard: (id: number, boardId: number | null) => invoke<void>("move_clip_to_board", { id, boardId }),
  getSettings: () => invoke<Record<string, string>>("get_settings"),
  setSetting: (key: string, value: string) => invoke<void>("set_setting", { key, value }),
  setHotkey: (accelerator: string) => invoke<void>("set_hotkey", { accelerator }),
  setQuickPasteEnabled: (enabled: boolean) =>
    invoke<void>("set_quick_paste_enabled", { enabled }),
  getLicenseInfo: () => invoke<LicenseInfo>("get_license_info"),
  activateLicense: (email: string, key: string) =>
    invoke<void>("activate_license", { email, key }),
  dismissLicensePrompt: () => invoke<void>("dismiss_license_prompt"),
  getAutostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enable: boolean) => invoke<void>("set_autostart", { enable }),
  setShowDockIcon: (show: boolean) => invoke<void>("set_show_dock_icon", { show }),
  setShowTrayIcon: (show: boolean) => invoke<void>("set_show_tray_icon", { show }),
  setTrayLeftAction: (action: string) => invoke<void>("set_tray_left_action", { action }),
  getExcludedApps: () => invoke<string[]>("get_excluded_apps"),
  addExcludedApp: (app: string) => invoke<void>("add_excluded_app", { app }),
  removeExcludedApp: (app: string) => invoke<void>("remove_excluded_app", { app }),
  getSourceApps: () => invoke<string[]>("get_source_apps"),
  getFrontmostApp: () => invoke<{ name: string; bundle: string | null } | null>("get_frontmost_app"),
  getAccessibilityTrusted: () => invoke<boolean>("get_accessibility_trusted"),
  requestAccessibility: () => invoke<boolean>("request_accessibility"),
  openAccessibilitySettings: () => invoke<void>("open_accessibility_settings"),
  hidePanel: () => invoke<void>("hide_panel"),
  showSettings: () => invoke<void>("show_settings"),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
};

export interface UpdateInfo {
  current: string;
  latest: string;
  hasUpdate: boolean;
  url: string;
}

/** 检测 GitHub Releases 上的最新版本,与当前版本做语义化比较。零额外依赖(CSP 为 null 允许外连)。 */
export async function checkUpdate(current: string): Promise<UpdateInfo> {
  const res = await fetch("https://api.github.com/repos/neekin/pastenext/releases/latest", {
    headers: { Accept: "application/vnd.github+json" },
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const rel = (await res.json()) as { tag_name?: string; html_url?: string };
  const latest = String(rel.tag_name ?? "").replace(/^v/, "");
  const url = rel.html_url || "https://github.com/neekin/pastenext/releases";
  return {
    current,
    latest,
    hasUpdate: compareVersion(latest, current) > 0,
    url,
  };
}

function compareVersion(a: string, b: string): number {
  const pa = a.split(".").map((n) => parseInt(n, 10) || 0);
  const pb = b.split(".").map((n) => parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const x = pa[i] || 0;
    const y = pb[i] || 0;
    if (x !== y) return x - y;
  }
  return 0;
}

export function onClipsUpdated(cb: () => void) {
  return listen<unknown>("clips-updated", cb);
}

export function onPanelShown(cb: () => void) {
  return listen<unknown>("panel-shown", cb);
}

export function onSettingsChanged(cb: (key: string, value: string) => void) {
  return listen<{ key: string; value: string }>("settings-changed", (e) =>
    cb(e.payload.key, e.payload.value)
  );
}
