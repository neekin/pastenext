import { invoke, convertFileSrc } from "@tauri-apps/api/core";
export interface MediaAsset {
  id: string;
  kind: "image" | "video";
  name: string;
  path: string;
  width: number;
  height: number;
  createdAt: number;
  clipId: number;
}
export interface CaptureOptions {
  mode: "region" | "window" | "screen";
  delay: number;
  display: number;
  cursor: boolean;
  clicks: boolean;
}
export const mediaApi = {
  pin: (id: string) => invoke<void>("media_pin", { id }),
  show: () => invoke<void>("show_capture_studio"),
  capture: (options: CaptureOptions, video = false) =>
    invoke<MediaAsset | null>("capture_screen", { options, video }),
  stop: (discard = false) => invoke<void>("stop_capture", { discard }),
  status: () =>
    invoke<{ busy: boolean; recording: boolean; elapsedMs: number }>(
      "capture_status",
    ),
  list: () => invoke<MediaAsset[]>("media_list"),
  import: () => invoke<MediaAsset | null>("media_import"),
  commit: (dataUrl: string, name: string) =>
    invoke<MediaAsset>("media_commit_image", { dataUrl, name }),
  copy: (id: string) => invoke<void>("media_copy", { id }),
  ocr: (id: string) => invoke<string>("media_ocr", { id }),
  export: (id: string, format: string, quality: number) =>
    invoke<string | null>("media_export", { id, format, quality }),
  permission: () => invoke<void>("open_screen_permission_settings"),
  src: (asset: MediaAsset) => convertFileSrc(asset.path),
};
