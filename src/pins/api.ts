import type { MediaAsset } from "../capture/api";
import type { NumberMark } from "./annotations";
import { invoke } from "@tauri-apps/api/core";

export const supportsPins = /Mac/.test(navigator.platform);
export interface PinInfo {
  label: string;
  clipId: number;
  mediaId?: string;
  zoom: number;
  width: number;
  height: number;
  opacity: number;
  hidden: boolean;
  clickThrough: boolean;
  marks: NumberMark[];
}
export type PinAction = "resize" | "ready" | "zoom" | "opacity" | "through" | "unlock" | "hide" | "restore" | "close";
export const pins = {
  menu: (x: number, y: number) => invoke<void>("pin_context_menu", { x, y, marking: false }),
  opacityInfo: () => invoke<PinInfo>("pin_opacity_info"),
  setOpacity: (value: number) => invoke<void>("pin_opacity_set", { value }),
  edit: () => invoke<void>("pin_edit"),
  applyEdit: (sessionId: string, assetId: string) => invoke<void>("apply_pin_edit", { sessionId, assetId }),
  cancelEdit: (sessionId: string, hideStudio = true) => invoke<void>("cancel_pin_edit", { sessionId, hideStudio }),
  takeEdit: () => invoke<{ asset: MediaAsset; marks: NumberMark[]; pinLabel: string } | null>("take_pin_edit"),
  create: (id: number) => invoke<void>("pin_clip", { id }),
  mark: (label: string, action: "add" | "undo" | "clear", mark?: NumberMark) =>
    invoke<void>("pin_mark", { label, action, mark: mark ?? null }),
  content: () => invoke<{ info: PinInfo; image: string }>("pin_content"),
  copy: (png: string) => invoke<void>("pin_copy", { png }),
  list: () => invoke<PinInfo[]>("list_pins"),
  action: (label: string, action: PinAction, value?: number) =>
    invoke<void>("pin_action", { label, action, value: value ?? null }),
  all: (action: "hide" | "restore" | "unlock") => invoke<void>("pins_action", { action }),
};
