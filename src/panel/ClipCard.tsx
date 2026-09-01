import { useEffect, useMemo, useRef, useState, type MouseEvent, type DragEvent } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import DOMPurify from "dompurify";
import { api } from "../api";
import { useI18n } from "../i18n";
import type { Board, Clip } from "../types";

/** 类型视觉主题:色带 + 徽章配色 */
const KIND_THEME: Record<string, { bar: string; badge: string; icon: string }> = {
  text: { bar: "bg-slate-400/70", badge: "bg-slate-500/15 text-slate-600 dark:text-slate-300", icon: "T" },
  rich_text: { bar: "bg-sky-500/70", badge: "bg-sky-500/15 text-sky-600 dark:text-sky-300", icon: "¶" },
  image: { bar: "bg-emerald-500/70", badge: "bg-emerald-500/15 text-emerald-600 dark:text-emerald-300", icon: "▣" },
  files: { bar: "bg-amber-500/70", badge: "bg-amber-500/15 text-amber-600 dark:text-amber-300", icon: "▤" },
};

/** 按扩展名给文件一个可辨识的图标 */
const FILE_ICON: [RegExp, string][] = [
  [/\.(png|jpe?g|gif|webp|bmp|tiff?|heic|avif|svg)$/i, "🖼"],
  [/\.(mp4|mov|m4v|avi|mkv|webm|flv)$/i, "🎬"],
  [/\.(mp3|wav|flac|aac|m4a|ogg)$/i, "🎵"],
  [/\.(zip|rar|7z|tar|gz|bz2|xz|dmg|iso)$/i, "📦"],
  [/\.(pdf|docx?|xlsx?|pptx?|pages|numbers|key|rtf|md|txt)$/i, "📄"],
  [/\.(swift|ts|tsx|js|jsx|rs|py|go|java|rb|c|cpp|h|json|ya?ml|toml)$/i, "⌨"],
];

function fileIcon(name: string) {
  for (const [re, icon] of FILE_ICON) if (re.test(name)) return icon;
  return "📄";
}

function relTime(ts: number, t: (key: "time.justNow" | "time.minutesAgo" | "time.hoursAgo" | "time.daysAgo", vars?: Record<string, string | number>) => string) {
  const d = Date.now() - ts;
  const m = Math.floor(d / 60000);
  if (m < 1) return t("time.justNow");
  if (m < 60) return t("time.minutesAgo", { n: m });
  const h = Math.floor(m / 60);
  if (h < 24) return t("time.hoursAgo", { n: h });
  const days = Math.floor(h / 24);
  if (days < 7) return t("time.daysAgo", { n: days });
  return new Date(ts).toLocaleDateString();
}

interface Props {
  clip: Clip;
  selected: boolean;
  boards: Board[];
  onClick: (e: MouseEvent) => void;
  /** 触发编辑时,把卡片在视口中的位置回传,供编辑浮层锚定到卡片所在列 */
  onDetail: (rect: DOMRect) => void;
}

export default function ClipCard({ clip, selected, boards, onClick, onDetail }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [dims, setDims] = useState<{ w: number; h: number } | null>(null);
  const { t } = useI18n();
  const rel = useMemo(() => relTime(clip.createdAt, t), [clip.createdAt, t]);
  const theme = KIND_THEME[clip.kind] ?? KIND_THEME.text;

  useEffect(() => {
    if (selected) {
      ref.current?.scrollIntoView({ behavior: "smooth", inline: "nearest", block: "nearest" });
    }
  }, [selected]);

  const handleDragStart = (e: DragEvent<HTMLDivElement>) => {
    e.dataTransfer.effectAllowed = "copy";
    if (clip.kind === "image" && clip.imagePath) {
      // 图片:优先提供文件 URL,同时保留纯文本路径作为降级
      e.dataTransfer.setData("text/uri-list", `file://${clip.imagePath}`);
      e.dataTransfer.setData("text/plain", clip.imagePath);
    } else if (clip.kind === "files" && clip.filePaths && clip.filePaths.length > 0) {
      const paths = clip.filePaths.join("\n");
      const urls = clip.filePaths.map((p) => `file://${p}`).join("\n");
      e.dataTransfer.setData("text/uri-list", urls);
      e.dataTransfer.setData("text/plain", paths);
    } else {
      // 文本 / 富文本 / 空
      const text = clip.text || "";
      e.dataTransfer.setData("text/plain", text);
      if (clip.kind === "rich_text" && clip.html) {
        e.dataTransfer.setData("text/html", clip.html);
      }
    }
  };

  const preview = () => {
    if (clip.kind === "image" && clip.imagePath) {
      return (
        <div className="relative w-full h-full">
          <img
            src={convertFileSrc(clip.imagePath)}
            className="w-full h-full object-cover rounded-lg"
            draggable={false}
            onLoad={(e) =>
              setDims({ w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })
            }
          />
          {dims && (
            <span className="absolute bottom-1 right-1 px-1.5 py-0.5 rounded-md bg-black/60 text-white text-[10px] font-medium tabular-nums">
              {dims.w}×{dims.h}
            </span>
          )}
          {clip.text && (
            <span
              title={t("clip.ocrBadge")}
              className="absolute bottom-1 left-1 px-1.5 py-0.5 rounded-md bg-indigo-500/90 text-white text-[10px] font-medium"
            >
              OCR
            </span>
          )}
        </div>
      );
    }
    if (clip.kind === "files" && clip.filePaths) {
      const names = clip.filePaths.map((p) => p.split(/[\\/]/).pop() || p);
      return (
        <div className="w-full h-full flex flex-col gap-1 justify-start overflow-hidden pt-1">
          {names.slice(0, 4).map((n, i) => (
            <div key={i} className="text-xs truncate text-neutral-700 dark:text-neutral-200">
              {fileIcon(n)} {n}
            </div>
          ))}
          {names.length > 4 && (
            <div className="text-xs text-neutral-400">{t("clip.files.more", { count: names.length - 4 })}</div>
          )}
        </div>
      );
    }
    if (clip.kind === "rich_text" && clip.html) {
      // 富文本所见即所得预览:净化后按原样式渲染,所见即所得
      return (
        <div className="html-preview-wrap">
          <div
            className="html-preview"
            dangerouslySetInnerHTML={{ __html: DOMPurify.sanitize(clip.html) }}
          />
        </div>
      );
    }
    return (
      <div className="w-full h-full overflow-hidden">
        <div className="text-[12.5px] leading-5 whitespace-pre-wrap break-all line-clamp-8 text-neutral-800 dark:text-neutral-100">
          {clip.text || t("clip.empty")}
        </div>
      </div>
    );
  };

  const moveTo = async (boardId: number | null) => {
    setMenuOpen(false);
    await api.moveClipToBoard(clip.id, boardId).catch(() => {});
  };

  return (
    <div
      ref={ref}
      data-clip-card
      draggable
      onClick={onClick}
      onDragStart={handleDragStart}
      className={`group relative shrink-0 w-[188px] h-[224px] rounded-xl flex flex-col cursor-grab active:cursor-grabbing transition-all ring-1 overflow-hidden ${
        selected
          ? "ring-2 ring-indigo-500 shadow-lg"
          : "ring-black/10 dark:ring-white/10 hover:ring-indigo-400/60"
      } bg-neutral-50/90 dark:bg-neutral-800/70`}
    >
      {/* 顶部类型色带:扫视时一眼区分文本 / 富文本 / 图片 / 文件 */}
      <div className={`h-[3px] w-full shrink-0 ${theme.bar}`} />

      <div className="flex-1 overflow-hidden p-2.5 pt-2">{preview()}</div>

      {/* 笔记:有则显示一行,没有就不占地方 */}
      {clip.note && (
        <div className="px-2.5 pb-1 text-[10.5px] truncate text-neutral-500 dark:text-neutral-400">
          📝 {clip.note}
        </div>
      )}

      <div className="px-2.5 py-1.5 border-t border-black/5 dark:border-white/10 flex items-center justify-between gap-1 text-[10.5px] text-neutral-400 dark:text-neutral-500">
        <span className="truncate flex items-center gap-1">
          {/* 类型徽章:配合顶部色带,颜色 + 形状双重区分 */}
          <span
            className={`w-[15px] h-[15px] shrink-0 rounded text-[9px] font-bold flex items-center justify-center ${theme.badge}`}
          >
            {theme.icon}
          </span>
          {clip.sourceApp || t("detail.source.unknown")} · {rel}
        </span>
        <span className="flex items-center gap-1 shrink-0">
          {clip.useCount > 0 && (
            <span title={t("detail.useCount") + ` ${clip.useCount}`} className="tabular-nums">
              ↻{clip.useCount}
            </span>
          )}
          {clip.tags.length > 0 && <span title={clip.tags.map((t) => t.name).join(", ")}>🏷️</span>}
        </span>
      </div>

      {/* 悬停操作 */}
      <div className="absolute top-1.5 right-1.5 hidden group-hover:flex gap-1">
        <button
          onClick={(e) => {
            e.stopPropagation();
            if (ref.current) onDetail(ref.current.getBoundingClientRect());
          }}
          title={t("panel.editTitle")}
          className="w-6 h-6 rounded-md bg-white/90 dark:bg-neutral-700/90 shadow text-xs flex items-center justify-center hover:bg-indigo-500 hover:text-white transition-colors"
        >
          ✎
        </button>
        <button
          onClick={(e) => {
            e.stopPropagation();
            setMenuOpen(true);
          }}
          title={t("panel.moreTitle")}
          className="w-6 h-6 rounded-md bg-white/90 dark:bg-neutral-700/90 shadow text-xs flex items-center justify-center hover:bg-indigo-500 hover:text-white transition-colors"
        >
          ⋯
        </button>
      </div>

      {menuOpen && (
        <>
          <div
            className="fixed inset-0 z-40"
            onClick={(e) => {
              e.stopPropagation();
              setMenuOpen(false);
            }}
          />
          <div
            className="absolute z-50 top-8 right-1.5 w-44 rounded-xl bg-white dark:bg-neutral-800 shadow-xl ring-1 ring-black/10 dark:ring-white/10 py-1 text-xs text-neutral-700 dark:text-neutral-200"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="px-3 py-1 text-neutral-400">{t("clip.moveToBoard")}</div>
            <button
              className={`w-full text-left px-3 py-1.5 hover:bg-black/5 dark:hover:bg-white/10 ${clip.boardId === null ? "text-indigo-500" : ""}`}
              onClick={() => moveTo(null)}
            >
              {t("clip.moveToHistory")}
            </button>
            {boards.map((b) => (
              <button
                key={b.id}
                className={`w-full text-left px-3 py-1.5 hover:bg-black/5 dark:hover:bg-white/10 ${clip.boardId === b.id ? "text-indigo-500" : ""}`}
                onClick={() => moveTo(b.id)}
              >
                {b.name}
              </button>
            ))}
            <div className="my-1 border-t border-black/5 dark:border-white/10" />
            <button
              className="w-full text-left px-3 py-1.5 hover:bg-red-50 dark:hover:bg-red-500/10 text-red-600 dark:text-red-400"
              onClick={async () => {
                setMenuOpen(false);
                await api.deleteClip(clip.id).catch(() => {});
              }}
            >
              {t("detail.delete")}
            </button>
          </div>
        </>
      )}
    </div>
  );
}
