import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { api, onClipsUpdated, onPanelShown } from "../api";
import { useI18n } from "../i18n";
import type { Board, Clip, ClipKind } from "../types";
import ClipCard from "./ClipCard";
import DetailDrawer from "./DetailDrawer";
import LicenseGate from "../license/LicenseGate";
import { useLicense } from "../license/useLicense";

export default function Panel() {
  const [clips, setClips] = useState<Clip[]>([]);
  const [boards, setBoards] = useState<Board[]>([]);
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState<ClipKind | null>(null);
  const [boardId, setBoardId] = useState<number | null>(null); // null = 历史
  const [selected, setSelected] = useState(0);
  const [detail, setDetail] = useState<Clip | null>(null);
  // 编辑浮层的锚点(来源卡片在视口中的位置)与根节点 ref,用于把浮层锚定到卡片列、并做"点外部关闭"命中检测
  const [detailAnchor, setDetailAnchor] = useState<DOMRect | null>(null);
  const detailRootRef = useRef<HTMLDivElement>(null);
  const [addingBoard, setAddingBoard] = useState(false);
  const [newBoard, setNewBoard] = useState("");
  const [axTrusted, setAxTrusted] = useState(true);
  // 进场可见性闸门:唤起瞬间先把面板隐藏,等 WebView 把窗口几何(全宽)应用完再揭幕,
  // 否则首帧仍是 860 宽,会出现横向滚动条闪烁 + 面板"展开"的跳变
  const [entered, setEntered] = useState(false);
  // 退场动画:关闭时先播放淡出,结束后再真正隐藏窗口
  const [leaving, setLeaving] = useState(false);
  // 每个会话只自动弹一次系统授权提示
  const axPromptedRef = useRef(false);
  // 运行时设置(纯文本模式/修饰键/音效等)
  const [cfg, setCfg] = useState<Record<string, string>>({});
  const cfgRef = useRef<Record<string, string>>({});

  const { t } = useI18n();
  const license = useLicense();
  // useLicense 的 reload 引用保持稳定,但放进 ref 更省心,避免 effect 依赖抖动
  const licenseReloadRef = useRef(license.reload);
  licenseReloadRef.current = license.reload;

  const kinds: { key: ClipKind | null; label: string }[] = [
    { key: null, label: t("panel.kind.all") },
    { key: "text", label: t("panel.kind.text") },
    { key: "rich_text", label: t("panel.kind.richText") },
    { key: "image", label: t("panel.kind.image") },
    { key: "files", label: t("panel.kind.files") },
  ];

  const loadCfg = useCallback(() => {
    api.getSettings().then((s) => {
      setCfg(s);
      cfgRef.current = s;
    }).catch(() => {});
  }, []);

  useEffect(() => {
    loadCfg();
  }, [loadCfg]);

  // 防止双击卡片触发两次粘贴
  const lastPasteRef = useRef<{ id: number; at: number }>({ id: -1, at: 0 });

  // 粘贴音效:短促轻响(WebAudio 合成,无外部资源)
  const playTick = useCallback(() => {
    if (cfgRef.current.sound_enabled === "false") return;
    try {
      const Ctx = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
      const ctx = new Ctx();
      const o = ctx.createOscillator();
      const g = ctx.createGain();
      o.type = "sine";
      o.frequency.setValueAtTime(880, ctx.currentTime);
      o.frequency.exponentialRampToValueAtTime(440, ctx.currentTime + 0.07);
      g.gain.setValueAtTime(0.1, ctx.currentTime);
      g.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.09);
      o.connect(g).connect(ctx.destination);
      o.start();
      o.stop(ctx.currentTime + 0.1);
      o.onended = () => ctx.close();
    } catch {
      /* 忽略音频错误 */
    }
  }, []);

  // 判断本次粘贴是否使用纯文本:全局开关与修饰键互为反向
  const plainFor = useCallback((e: Partial<{ shiftKey: boolean; altKey: boolean; ctrlKey: boolean; metaKey: boolean }>) => {
    const mod = cfgRef.current.plain_modifier || "shift";
    const held = mod === "shift" ? !!e.shiftKey : mod === "option" ? !!e.altKey : !!e.ctrlKey;
    const always = cfgRef.current.paste_plain_always === "true";
    return always ? !held : held;
  }, []);

  // 退场:直接交给 Rust 在 OS 窗口级做下滑(见 src-tauri/src/lib.rs hide_panel),
  // 与进场的窗口级上滑对称。这里不再等 CSS 退场动画,否则会有 230ms 的空档再下滑。
  const requestHide = useCallback(() => {
    api.hidePanel().catch(() => {});
  }, []);

  const pasteClip = useCallback((id: number, plain: boolean = false) => {
    const now = Date.now();
    if (lastPasteRef.current.id === id && now - lastPasteRef.current.at < 600) {
      return;
    }
    lastPasteRef.current = { id, at: now };
    playTick();
    // 粘贴成功后滑出关闭(失败则保留面板以便重试)
    api.pasteClip(id, plain).then(() => requestHide()).catch(() => {});
  }, [playTick, requestHide]);

  const searchRef = useRef<HTMLInputElement>(null);
  const reloadRef = useRef<() => void>(() => {});
  // 高度自适应:测量内容自然高度(搜索栏 + 提示条 + 看板栏 + 卡片流),
  // 交给 Rust 把窗口高度调成恰好撑满,避免出现纵向滚动条
  const contentRef = useRef<HTMLDivElement>(null);
  const lastHeightRef = useRef<number>(0);

  const reload = useCallback(() => {
    api
      .listClips({ query, kind, boardId })
      .then((rows) => {
        setClips(rows);
        setSelected((s) => Math.min(s, Math.max(rows.length - 1, 0)));
      })
      .catch(() => {});
  }, [query, kind, boardId]);
  reloadRef.current = reload;

  // 高度自适应:把内容自然高度交给 Rust 调窗口高度,让内容刚好撑满、不出纵向滚动条。
  // 只有高度真的变了(>0.5px)才下发,避免后台每次剪贴板更新都触发 IPC 与窗口抖动。
  const fitHeight = useCallback(() => {
    const el = contentRef.current;
    if (!el) return;
    const h = el.getBoundingClientRect().height;
    if (!h) return;
    if (Math.abs(h - lastHeightRef.current) < 0.5) return;
    lastHeightRef.current = h;
    api.setPanelHeight(Math.ceil(h)).catch(() => {});
  }, []);

  // 内容/提示条/看板变化后重新贴合高度(布局阶段同步测量,先于绘制,不产生视觉跳动)
  useLayoutEffect(() => {
    fitHeight();
  }, [fitHeight, clips.length, boardId, kind, query, axTrusted, license.phase, addingBoard]);

  useEffect(() => {
    reload();
  }, [reload]);

  useEffect(() => {
    const checkAx = () => {
      if (/Mac|iPhone|iPad/.test(navigator.platform)) {
        api.getAccessibilityTrusted().then(setAxTrusted).catch(() => {});
      }
    };
    checkAx();
    api.getBoards().then(setBoards).catch(() => {});
  }, []);

  useEffect(() => {
    const u1 = onClipsUpdated(() => reloadRef.current());
    const u2 = onPanelShown(() => {
      setEntered(false);
      setLeaving(false);
      // 抽屉不跨唤醒保留(上次会话的编辑状态已过时)
      setDetail(null);
      setDetailAnchor(null);
      // 先同步触发内容加载,等数据就绪后再揭幕,避免面板滑入时内容跳动
      reloadRef.current();
      loadCfg();
      licenseReloadRef.current();
      // 等两帧,确保 WebView 已按全宽重排且数据已渲染,再揭幕播放进场动画
      requestAnimationFrame(() =>
        requestAnimationFrame(() => {
          fitHeight();
          setEntered(true);
          searchRef.current?.focus();
          searchRef.current?.select();
          if (/Mac|iPhone|iPad/.test(navigator.platform)) {
            api
              .getAccessibilityTrusted()
              .then((trusted) => {
                setAxTrusted(trusted);
                if (!trusted && !axPromptedRef.current) {
                  axPromptedRef.current = true;
                  api
                    .requestAccessibility()
                    .then(setAxTrusted)
                    .catch(() => {});
                }
              })
              .catch(() => {});
          }
        })
      );
    });
    return () => {
      u1.then((f) => f());
      u2.then((f) => f());
    };
  }, []);

  // 面板失焦自动隐藏(详情抽屉打开时保留)。
  // 只有真正获得过焦点的窗口才响应 blur,避免显示瞬间的伪 blur 事件立即隐藏面板
  useEffect(() => {
    let hadFocus = false;
    const onFocus = () => {
      hadFocus = true;
    };
    const onBlur = () => {
      if (hadFocus) {
        hadFocus = false;
        setDetail(null);
        setDetailAnchor(null);
        requestHide();
      }
    };
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
    };
  }, [detail]);

  // 统一的「点空白隐藏面板」语义:
  // - 点击可交互元素(按钮/输入框/卡片/链接等):元素自身逻辑,面板保留
  // - 其余一切非交互区域 —— 包括编辑抽屉自身的留白 —— 一律关闭抽屉并隐藏面板。
  //   豁免名单只认「真正可交互的东西」,避免「看起来是空白却点不动」的顿挫感
  const onRootMouseDown = useCallback(
    (e: React.MouseEvent) => {
      const el = e.target instanceof HTMLElement ? e.target : null;
      if (!el) return;
      if (el.closest("button, input, textarea, select, a, label, [data-clip-card], [data-no-autohide]")) return;
      setDetail(null);
      setDetailAnchor(null);
      requestHide();
    },
    [requestHide]
  );

  const addBoard = async () => {
    const name = newBoard.trim();
    if (!name) return;
    setNewBoard("");
    setAddingBoard(false);
    try {
      const b = await api.createBoard(name);
      setBoards((bs) => [...bs, b]);
      setBoardId(b.id);
    } catch {
      /* 重名等错误忽略 */
    }
  };

  const cycleBoard = (dir: 1 | -1) => {
    const order: (number | null)[] = [null, ...boards.map((b) => b.id)];
    const idx = order.indexOf(boardId);
    setBoardId(order[(idx + dir + order.length) % order.length]);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    // Esc 永远退场:先关抽屉,再隐藏面板(不受抽屉编辑状态影响)
    if (e.key === "Escape") {
      e.preventDefault();
      setDetail(null);
      setDetailAnchor(null);
      requestHide();
      return;
    }
    // 编辑抽屉打开时,其余按键(导航/回车粘贴)不参与面板操作,
    // 避免在抽屉输入框里打字时误触发卡片导航
    if (detail) return;
    if (e.metaKey && (e.key === "[" || e.key === "]")) {
      // ⌘[ / ⌘] 切换上一个/下一个看板
      e.preventDefault();
      cycleBoard(e.key === "]" ? 1 : -1);
    } else if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      e.preventDefault();
      setSelected((s) => Math.min(s + 1, clips.length - 1));
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      e.preventDefault();
      setSelected((s) => Math.max(s - 1, 0));
    } else if (e.key === "Enter" && clips[selected]) {
      e.preventDefault();
      pasteClip(clips[selected].id, plainFor(e));
    }
  };

  const tabCls = (active: boolean) =>
    `px-2.5 h-7 rounded-lg text-xs transition-colors ${
      active
        ? "bg-indigo-500/15 text-indigo-600 dark:text-indigo-300 font-medium"
        : "text-neutral-500 dark:text-neutral-400 hover:bg-black/5 dark:hover:bg-white/10"
    }`;

  return (
    <div
      className="relative h-full flex flex-col select-none overflow-hidden"
      onKeyDown={onKeyDown}
      onMouseDown={onRootMouseDown}
    >
      <div
        className={`panel-surface h-full flex flex-col rounded-t-2xl bg-white/90 dark:bg-neutral-900/90 backdrop-blur-xl ring-1 ring-black/10 dark:ring-white/15 overflow-hidden ${entered ? "entered" : ""} ${leaving ? "leaving" : ""}`}
      >
        {/* 高度测量容器:自然高度 = 搜索栏 + 提示条 + 看板栏 + 卡片流,
            测量它并把结果交给 Rust 设置窗口高度,使内容刚好撑满、无纵向滚动条 */}
        <div ref={contentRef} className="flex flex-col">
        {/* 搜索 + 类型筛选 */}
        <div className="flex items-center gap-2 px-4 pt-3.5">
          <input
            ref={searchRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("panel.searchPlaceholder")}
            autoFocus
            className="flex-1 h-9 px-3 rounded-lg bg-black/5 dark:bg-white/10 text-sm outline-none text-neutral-800 dark:text-neutral-100 placeholder:text-neutral-400 dark:placeholder:text-neutral-500"
          />
          <div className="flex gap-1">
            {kinds.map((k) => (
              <button
                key={k.label}
                onClick={() => setKind(k.key)}
                className={`px-2.5 h-9 rounded-lg text-xs transition-colors ${
                  kind === k.key
                    ? "bg-indigo-500 text-white"
                    : "bg-black/5 dark:bg-white/10 text-neutral-600 dark:text-neutral-300 hover:bg-black/10 dark:hover:bg-white/20"
                }`}
              >
                {k.label}
              </button>
            ))}
            <button
              onClick={() => api.showSettings().catch(() => {})}
              title={t("panel.settingsTitle")}
              className="w-9 h-9 rounded-lg bg-black/5 dark:bg-white/10 text-neutral-500 dark:text-neutral-300 hover:bg-black/10 dark:hover:bg-white/20 transition-colors text-sm"
            >
              ⚙
            </button>
          </div>
        </div>

        {!axTrusted && (
          <button
            onClick={() => api.openAccessibilitySettings().catch(() => {})}
            className="mx-4 mt-2 text-left text-xs px-3 py-1.5 rounded-lg bg-amber-100 text-amber-800 dark:bg-amber-500/20 dark:text-amber-300"
          >
            ⚠️ {t("panel.axWarning")}
          </button>
        )}

        {/* 授权状态条:试用期内只是一行小字,不打断操作 */}
        {license.phase !== "licensed" && (
          <button
            onClick={() => api.showSettings().catch(() => {})}
            className={`mx-4 mt-2 text-left text-[11px] px-3 py-1 rounded-lg transition-colors ${
              license.phase === "expired"
                ? "bg-amber-100 text-amber-800 dark:bg-amber-500/20 dark:text-amber-300 hover:bg-amber-200 dark:hover:bg-amber-500/30"
                : "text-neutral-400 dark:text-neutral-500 hover:bg-black/5 dark:hover:bg-white/10"
            }`}
          >
            {license.phase === "expired"
              ? t("license.banner.expired")
              : t("license.banner.trial", { n: license.daysLeft })}
          </button>
        )}

        {/* 看板标签页 */}
        <div className="flex items-center gap-1 px-4 mt-2">
          <button onClick={() => setBoardId(null)} className={tabCls(boardId === null)}>
            {t("panel.board.history")}
          </button>
          {boards.map((b) => (
            <button
              key={b.id}
              onClick={() => setBoardId(b.id)}
              onDoubleClick={() => {
                const name = window.prompt?.(t("panel.board.renamePrompt"), b.name);
                if (name && name.trim()) {
                  api.renameBoard(b.id, name.trim()).then(() => api.getBoards().then(setBoards)).catch(() => {});
                }
              }}
              className={tabCls(boardId === b.id)}
              title={t("panel.board.renameTitle")}
            >
              {b.name}
            </button>
          ))}
          <button
            onClick={() => setAddingBoard(true)}
            className={tabCls(false)}
            title={t("panel.board.add")}
          >
            {t("panel.board.add")}
          </button>
          {addingBoard && (
            <input
              value={newBoard}
              onChange={(e) => setNewBoard(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") addBoard();
                if (e.key === "Escape") {
                  setAddingBoard(false);
                  setNewBoard("");
                }
              }}
              onBlur={() => {
                if (!newBoard.trim()) setAddingBoard(false);
              }}
              autoFocus
              placeholder={t("panel.board.placeholder")}
              className="h-7 px-2 w-40 rounded-lg bg-black/5 dark:bg-white/10 text-xs outline-none text-neutral-800 dark:text-neutral-100"
            />
          )}
        </div>

        {/* 卡片流:去掉 flex-1 让它保持自然高度(卡片 224 + 上下内边距 24 = 248),
            并显式 overflow-y-hidden —— CSS 规则下 overflow-x-auto 会把另一轴从 visible
            隐式提升为 auto,不显式关掉就会冒出纵向滚动条 */}
        <div className="flex items-stretch gap-3 px-4 py-3 overflow-x-auto overflow-y-hidden min-h-[248px]">
          {clips.length === 0 && (
            <div className="m-auto text-sm text-neutral-400 dark:text-neutral-500">
              {t("panel.empty")}
            </div>
          )}
          {clips.map((c, i) => (
            <ClipCard
              key={c.id}
              clip={c}
              selected={i === selected}
              boards={boards}
              onClick={(e) => pasteClip(c.id, plainFor(e))}
              onDetail={(rect) => {
                setDetailAnchor(rect);
                setDetail(c);
              }}
            />
          ))}
        </div>
        </div>
      </div>

      {detail && (
        <DetailDrawer
          clip={detail}
          boards={boards}
          anchor={detailAnchor}
          rootRef={detailRootRef}
          onClose={() => {
            setDetail(null);
            setDetailAnchor(null);
            reload();
          }}
        />
      )}

      <LicenseGate license={license} />
    </div>
  );
}
