import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { mediaApi, type MediaAsset } from "./api";
export default function PinnedImage() {
  const [item, setItem] = useState<MediaAsset | null>(null),
    [error, setError] = useState("");
  useEffect(() => {
    const id = new URLSearchParams(location.search).get("id");
    invoke<MediaAsset>("media_get", { id })
      .then(setItem)
      .catch((e) => setError(String(e)));
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") void getCurrentWindow().close();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, []);
  return (
    <div
      style={{
        width: "100vw",
        height: "100vh",
        background: "#202333",
        position: "relative",
        display: "grid",
        placeItems: "center",
      }}
      onMouseDown={(e) => {
        if (e.button === 0 && !(e.target as HTMLElement).closest("button"))
          void getCurrentWindow().startDragging();
      }}
      onDoubleClick={() => void getCurrentWindow().close()}
    >
      {item && (
        <img
          src={mediaApi.src(item)}
          draggable={false}
          style={{
            width: "100%",
            height: "100%",
            objectFit: "contain",
            pointerEvents: "none",
          }}
        />
      )}
      {error && <span style={{ color: "white" }}>{error}</span>}
      <button
        title="关闭贴图 / Close pinned image (Esc)"
        onClick={() => void getCurrentWindow().close()}
        style={{
          position: "absolute",
          right: 5,
          top: 5,
          border: 0,
          borderRadius: 20,
          width: 24,
          height: 24,
          background: "#151827bb",
          color: "white",
          cursor: "pointer",
        }}
      >
        ×
      </button>
    </div>
  );
}
