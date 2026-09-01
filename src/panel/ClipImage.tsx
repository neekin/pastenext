import { useEffect, useState, type SyntheticEvent } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Props {
  path: string;
  className?: string;
  draggable?: boolean;
  onLoad?: (e: SyntheticEvent<HTMLImageElement>) => void;
}

/**
 * 以 base64 data URL 渲染本地图片,绕过 Tauri 的 asset 协议 / 作用域限制。
 *
 * 背景:此前用 convertFileSrc() 生成的 asset:// URL 在 Windows(WebView2)上会被严格的作用域
 * 校验挡掉——一旦图片落在 $APPDATA 作用域之外(例如便携模式的 exe/Data 目录),缩略图就空白。
 * base64 data URL 与平台、路径、协议作用域都无关,渲染稳定。
 */
export default function ClipImage({ path, className, draggable, onLoad }: Props) {
  const [src, setSrc] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    invoke<string>("read_image_base64", { path })
      .then((d) => {
        if (active) setSrc(d);
      })
      .catch(() => {
        if (active) setSrc(null);
      });
    return () => {
      active = false;
    };
  }, [path]);

  if (!src) return <div className={className} />;
  return (
    <img
      src={src}
      className={className}
      draggable={draggable ?? false}
      onLoad={onLoad}
    />
  );
}
