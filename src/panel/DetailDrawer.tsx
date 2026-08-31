import { useEffect, useLayoutEffect, useState, type RefObject } from "react";
import DOMPurify from "dompurify";
import { api } from "../api";
import { useI18n } from "../i18n";
import type { Board, Clip } from "../types";

interface Props {
  clip: Clip;
  boards: Board[];
  onClose: () => void;
  /** 来源卡片在视口中的位置,用于把编辑浮层锚定到卡片所在列(而非右侧整高抽屉) */
  anchor: DOMRect | null;
  /** 浮层根节点 ref,供 Panel 做"点击外部关闭"命中检测 */
  rootRef?: RefObject<HTMLDivElement>;
}

export default function DetailDrawer({ clip, boards, onClose, anchor, rootRef }: Props) {
  const { t } = useI18n();
  const [cur, setCur] = useState<Clip>(clip);
  const [text, setText] = useState(clip.text ?? "");
  const [note, setNote] = useState(clip.note ?? "");
  const [tagName, setTagName] = useState("");
  const [knownTags, setKnownTags] = useState<string[]>([]);
  const [flash, setFlash] = useState("");

  useEffect(() => {
    api
      .getTags()
      .then((ts) => setKnownTags(ts.map((t) => t.name)))
      .catch(() => {});
  }, []);

  const refresh = () => {
    api
      .getClip(clip.id)
      .then((c) => c && setCur(c))
      .catch(() => {});
  };

  const say = (m: string) => {
    setFlash(m);
    setTimeout(() => setFlash(""), 1500);
  };

  const editable = cur.kind === "text" || cur.kind === "rich_text";

  // 锚定到来源卡片所在列:随卡片 x 定位、占满窗口高度,点击外部由 Panel 的
  // mousedown 监听关闭。窗口仅 380px 高,整高浮层既有足够编辑空间,又不会被裁切。
  useLayoutEffect(() => {
    const el = rootRef?.current;
    const parent = el?.parentElement;
    if (!el || !anchor || !parent) return;
    const c = parent.getBoundingClientRect();
    const W = Math.min(340, c.width - 16);
    let left = anchor.left - c.left;
    left = Math.max(8, Math.min(left, c.width - W - 8));
    el.style.width = `${W}px`;
    el.style.left = `${left}px`;
    el.style.top = "8px";
    el.style.bottom = "8px";
  }, [anchor, rootRef]);

  return (
    <div
      ref={rootRef}
      className="absolute z-50 flex flex-col rounded-2xl bg-white dark:bg-neutral-900 ring-1 ring-black/10 dark:ring-white/15 shadow-2xl overflow-hidden"
    >
      <div className="flex items-center justify-between px-4 pt-3.5 pb-2">
        <div className="text-sm font-semibold text-neutral-800 dark:text-neutral-100">{t("detail.title")}</div>
        <div className="flex items-center gap-2">
          {flash && <span className="text-xs text-emerald-500">{flash}</span>}
          <button
            onClick={onClose}
            className="w-6 h-6 rounded-md text-neutral-400 hover:bg-black/5 dark:hover:bg-white/10 text-xs"
          >
            ✕
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-4 pb-4 space-y-4">
        {editable && (
          <section>
            <label className="text-xs text-neutral-400">{t("detail.content")}</label>
            {cur.kind === "rich_text" && cur.html && (
              <>
                <div className="mt-1 rounded-lg bg-white ring-1 ring-black/10 dark:ring-white/10 overflow-hidden max-h-44 overflow-y-auto">
                  <div
                    className="html-preview html-preview-lg"
                    dangerouslySetInnerHTML={{ __html: DOMPurify.sanitize(cur.html) }}
                  />
                </div>
                <details className="mt-1.5">
                  <summary className="text-xs text-neutral-400 cursor-pointer hover:text-neutral-600 dark:hover:text-neutral-300">
                    {t("detail.htmlSource")}
                  </summary>
                  <pre className="mt-1 rounded-lg bg-black/5 dark:bg-white/10 p-2 text-[11px] leading-4 font-mono whitespace-pre-wrap break-all text-neutral-600 dark:text-neutral-300 max-h-32 overflow-y-auto">
                    {cur.html}
                  </pre>
                </details>
              </>
            )}
            <textarea
              value={text}
              onChange={(e) => setText(e.target.value)}
              rows={cur.kind === "rich_text" ? 3 : 6}
              className="mt-1 w-full rounded-lg bg-black/5 dark:bg-white/10 p-2 text-[13px] font-mono outline-none resize-none text-neutral-800 dark:text-neutral-100"
            />
            {cur.kind === "rich_text" ? (
              <div className="mt-1 flex items-center gap-2">
                <button
                  onClick={async () => {
                    await api.editClip(cur.id, text).catch(() => {});
                    refresh();
                    say(t("detail.saved"));
                  }}
                  className="px-2.5 py-1 rounded-md bg-indigo-500 text-white text-xs hover:bg-indigo-600"
                >
                  {t("detail.saveAsPlain")}
                </button>
                <span className="text-[11px] text-neutral-400">{t("detail.discardRich")}</span>
              </div>
            ) : (
              <button
                onClick={async () => {
                  await api.editClip(cur.id, text).catch(() => {});
                  refresh();
                  say(t("detail.contentSaved"));
                }}
                className="mt-1 px-2.5 py-1 rounded-md bg-indigo-500 text-white text-xs hover:bg-indigo-600"
              >
                {t("detail.saveContent")}
              </button>
            )}
          </section>
        )}

        <section>
          <label className="text-xs text-neutral-400">{t("detail.note")}</label>
          <textarea
            value={note}
            onChange={(e) => setNote(e.target.value)}
            onBlur={async () => {
              if (note !== (cur.note ?? "")) {
                await api.setNote(cur.id, note).catch(() => {});
                refresh();
                say(t("detail.noteSaved"));
              }
            }}
            rows={3}
            placeholder={t("detail.notePlaceholder")}
            className="mt-1 w-full rounded-lg bg-black/5 dark:bg-white/10 p-2 text-[13px] outline-none resize-none text-neutral-800 dark:text-neutral-100 placeholder:text-neutral-400"
          />
        </section>

        <section>
          <label className="text-xs text-neutral-400">{t("detail.tags")}</label>
          <div className="flex flex-wrap gap-1 mt-1">
            {cur.tags.map((t) => (
              <span
                key={t.id}
                className="px-2 py-0.5 rounded-full bg-indigo-100 dark:bg-indigo-500/20 text-indigo-600 dark:text-indigo-300 text-xs flex items-center gap-1"
              >
                {t.name}
                <button
                  onClick={async () => {
                    await api.removeTag(cur.id, t.id).catch(() => {});
                    refresh();
                  }}
                  className="hover:text-red-500"
                >
                  ×
                </button>
              </span>
            ))}
            {cur.tags.length === 0 && <span className="text-xs text-neutral-400">{t("detail.tags.empty")}</span>}
          </div>
          <input
            value={tagName}
            onChange={(e) => setTagName(e.target.value)}
            onKeyDown={async (e) => {
              if (e.key === "Enter" && tagName.trim()) {
                await api.addTag(cur.id, tagName.trim()).catch(() => {});
                setTagName("");
                refresh();
              }
            }}
            placeholder={t("detail.tagPlaceholder")}
            className="mt-2 w-full h-8 px-2 rounded-lg bg-black/5 dark:bg-white/10 text-[13px] outline-none text-neutral-800 dark:text-neutral-100 placeholder:text-neutral-400"
          />
          {knownTags.filter((n) => !cur.tags.some((t) => t.name === n)).length > 0 && (
            <div className="flex flex-wrap gap-1 mt-1.5">
              {knownTags
                .filter((n) => !cur.tags.some((t) => t.name === n))
                .slice(0, 8)
                .map((n) => (
                  <button
                    key={n}
                    onClick={async () => {
                      await api.addTag(cur.id, n).catch(() => {});
                      refresh();
                    }}
                    className="px-2 py-0.5 rounded-full bg-neutral-100 dark:bg-neutral-800 text-neutral-500 dark:text-neutral-400 text-[11px] hover:bg-indigo-100 dark:hover:bg-indigo-500/20"
                  >
                    + {n}
                  </button>
                ))}
            </div>
          )}
        </section>

        <section>
          <label className="text-xs text-neutral-400">{t("detail.board")}</label>
          <select
            value={cur.boardId ?? ""}
            onChange={async (e) => {
              const v = e.target.value === "" ? null : Number(e.target.value);
              await api.moveClipToBoard(cur.id, v).catch(() => {});
              refresh();
            }}
            className="mt-1 w-full h-8 px-2 rounded-lg bg-black/5 dark:bg-white/10 text-[13px] outline-none text-neutral-800 dark:text-neutral-100"
          >
            <option value="">{t("detail.board.history")}</option>
            {boards.map((b) => (
              <option key={b.id} value={b.id}>
                {b.name}
              </option>
            ))}
          </select>
        </section>

        <section className="text-xs text-neutral-400 space-y-0.5">
          <div>{t("detail.source")}:{cur.sourceApp || t("detail.source.unknown")}</div>
          <div>{t("detail.copiedAt")}:{new Date(cur.createdAt).toLocaleString()}</div>
          <div>{t("detail.useCount")}:{cur.useCount}</div>
        </section>
      </div>

      <div className="flex gap-2 px-4 py-3 border-t border-black/5 dark:border-white/10">
        <button
          onClick={async () => {
            await api.copyClip(cur.id).catch(() => {});
            say(t("detail.copiedToClipboard"));
          }}
          className="flex-1 h-8 rounded-lg bg-black/5 dark:bg-white/10 text-xs text-neutral-700 dark:text-neutral-200 hover:bg-black/10 dark:hover:bg-white/20"
        >
          {t("detail.copy")}
        </button>
        <button
          onClick={() => api.pasteClip(cur.id).catch(() => {})}
          className="flex-1 h-8 rounded-lg bg-indigo-500 text-white text-xs hover:bg-indigo-600"
        >
          {t("detail.paste")}
        </button>
        <button
          onClick={async () => {
            await api.deleteClip(cur.id).catch(() => {});
            onClose();
          }}
          className="flex-1 h-8 rounded-lg bg-red-50 dark:bg-red-500/15 text-xs text-red-600 dark:text-red-400 hover:bg-red-100"
        >
          {t("detail.delete")}
        </button>
      </div>
    </div>
  );
}
