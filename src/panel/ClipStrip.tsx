import { memo, useLayoutEffect, useRef, useState, type MouseEvent } from "react";
import type { Board, Clip } from "../types";
import ClipCard from "./ClipCard";

const CARD_WIDTH = 188;
const GAP = 12;
const STEP = CARD_WIDTH + GAP;
const PADDING = 16;
const OVERSCAN = 2;

interface Props {
  clips: Clip[];
  selected: number;
  scrollSelection: boolean;
  focusSelection: boolean;
  pastingId: number | null;
  queueIds: number[];
  boards: Board[];
  emptyLabel: string;
  onDoubleClick: (id: number, e: MouseEvent) => void;
  onClick: (id: number, e: MouseEvent) => void;
  onDetail: (clip: Clip, rect: DOMRect) => void;
}

// 保留完整列表供搜索、队列和键盘导航使用，只挂载视口附近的卡片。
export default memo(function ClipStrip({ clips, selected, scrollSelection, focusSelection, pastingId, queueIds, boards, emptyLabel, onClick, onDoubleClick, onDetail }: Props) {
  const stripRef = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState({ left: 0, width: window.innerWidth });

  useLayoutEffect(() => {
    const el = stripRef.current;
    if (!el) return;
    const measure = () => setViewport((prev) =>
      prev.left === el.scrollLeft && prev.width === el.clientWidth
        ? prev : { left: el.scrollLeft, width: el.clientWidth }
    );
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    el.addEventListener("scroll", measure, { passive: true });
    measure();
    return () => {
      observer.disconnect();
      el.removeEventListener("scroll", measure);
    };
  }, []);

  useLayoutEffect(() => {
    const el = stripRef.current;
    if (!el || !clips.length || !scrollSelection) return;
    const left = PADDING + selected * STEP;
    const right = left + CARD_WIDTH;
    // 直接定位，避免连续按方向键时平滑滚动不断重启，也支持尚未挂载的卡片。
    if (left < el.scrollLeft + PADDING) el.scrollLeft = left - PADDING;
    else if (right > el.scrollLeft + el.clientWidth - PADDING) {
      el.scrollLeft = right - el.clientWidth + PADDING;
    }
    setViewport({ left: el.scrollLeft, width: el.clientWidth });
  }, [selected, clips.length, scrollSelection]);

  const start = Math.max(0, Math.min(clips.length - 1, Math.floor(viewport.left / STEP) - OVERSCAN));
  const end = Math.min(clips.length, Math.ceil((viewport.left + viewport.width) / STEP) + OVERSCAN);
  const width = clips.length ? clips.length * STEP - GAP + PADDING * 2 : undefined;

  return (
    <div ref={stripRef} role="listbox" className="overflow-x-auto overflow-y-hidden min-h-[248px]">
      {clips.length === 0 ? (
        <div className="h-[248px] flex items-center justify-center text-sm text-neutral-400 dark:text-neutral-500">
          {emptyLabel}
        </div>
      ) : (
        <div className="relative h-[248px]" style={{ width }}>
          <div className="absolute top-3 flex gap-3" style={{ left: PADDING + start * STEP }}>
            {clips.slice(start, end).map((clip, index) => {
              const qi = queueIds.indexOf(clip.id);
              return (
                <ClipCard
                  key={clip.id}
                  clip={clip}
                  selected={start + index === selected}
                  pasting={pastingId === clip.id}
                  focusSelection={focusSelection}
                  queueIndex={qi >= 0 ? qi + 1 : undefined}
                  boards={boards}
                  onClick={onClick}
                  onDoubleClick={onDoubleClick}
                  onDetail={onDetail}
                />
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
});
