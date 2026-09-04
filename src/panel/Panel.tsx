import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { api, onClipsUpdated, onPanelShown, onSettingsChanged } from "../api";
import { useI18n } from "../i18n";
import type { Board, Clip, ClipKind, SmartCollection } from "../types";
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
  // 筛选维度:来源应用(精确匹配 source_app 列)与时间范围(仅支持到天,前端折算成 since 毫秒)
  const [sourceApp, setSourceApp] = useState<string | null>(null);
  const [range, setRange] = useState<"all" | "today" | "7d" | "30d">("all");
  const [sourceApps, setSourceApps] = useState<string[]>([]);
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
  // 当前激活的智能集合(与 sourceApp/kind 过滤联动,操作工具栏会退出智能集合态)
  const [activeSmart, setActiveSmart] = useState<SmartCollection | null>(null);
  // 粘贴队列:有序的 clip id 列表(Shift/⌘+点击卡片加入/移出)
  const [queueIds, setQueueIds] = useState<number[]>([]);
  const [queueError, setQueueError] = useState("");
  // 揭示密码锁:本会话内验证一次后不再询问(应用重启后需重新验证)
  const [revealUnlocked, setRevealUnlocked] = useState(false);
  const revealLockOn = cfg.reveal_lock === "on";

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

  // 智能集合定义存于 settings(随 settings-changed 实时刷新,loadCfg 即刷新)
  const smartCols: SmartCollection[] = useMemo(() => {
    try {
      const list = JSON.parse(cfg.smart_collections || "[]");
      return Array.isArray(list) ? list : [];
    } catch {
      return [];
    }
  }, [cfg]);

  // 进入智能集合:cross-board 查询(boardId=-1),规则映射到 sourceApp/kind 过滤
  const clickSmart = useCallback((sc: SmartCollection) => {
    setActiveSmart(sc);
    setBoardId(-1); // -1 = 不限看板(含手动看板里的内容)
    if (sc.type === "source_app") {
      setSourceApp(sc.value);
      setKind(null);
    } else {
      setKind(sc.value as ClipKind);
      setSourceApp(null);
    }
  }, []);

  // 退出智能集合态:任何手动过滤操作都会调用
  const leaveSmart = useCallback(() => {
    setActiveSmart((cur) => (cur ? null : cur));
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

  // 粘贴队列:Shift/⌘/Ctrl+点击卡片 加入/移出
  const toggleQueue = useCallback((id: number) => {
    setQueueIds((ids) => {
      if (ids.includes(id)) return ids.filter((x) => x !== id);
      return [...ids, id];
    });
  }, []);

  // 开始顺序粘贴:面板先收起,后端立即粘贴第 1 条,之后每按一次全局热键粘贴下一条
  const startQueue = useCallback(async () => {
    if (queueIds.length === 0) return;
    try {
      await api.hidePanel();
      await api.queueStart(queueIds);
      setQueueIds([]);
      setQueueError("");
    } catch (e) {
      setQueueError(String(e));
      setTimeout(() => setQueueError(""), 4000);
    }
  }, [queueIds]);

  // 全选当前筛选结果进入队列(按当前列表顺序)
  const queueSelectAll = useCallback(() => {
    setQueueIds(clips.map((c) => c.id));
  }, [clips]);

  const searchRef = useRef<HTMLInputElement>(null);
  const reloadRef = useRef<() => void>(() => {});

  // 时间范围 → since 毫秒时间戳(后端按 created_at >= since 过滤)
  const since = useMemo(() => {
    if (range === "all") return null;
    const d = new Date();
    if (range === "today") d.setHours(0, 0, 0, 0);
    else d.setDate(d.getDate() - (range === "7d" ? 7 : 30));
    return d.getTime();
  }, [range]);
  // 高度自适应:测量内容自然高度(搜索栏 + 提示条 + 看板栏 + 卡片流),
  // 交给 Rust 把窗口高度调成恰好撑满,避免出现纵向滚动条
  const contentRef = useRef<HTMLDivElement>(null);
  const lastHeightRef = useRef<number>(0);

  const reload = useCallback(() => {
    api
      .listClips({ query, kind, boardId, sourceApp, since })
      .then((rows) => {
        setClips(rows);
        setSelected((s) => Math.min(s, Math.max(rows.length - 1, 0)));
      })
      .catch(() => {});
  }, [query, kind, boardId, sourceApp, since]);
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
  }, [fitHeight, clips.length, boardId, kind, query, axTrusted, license.phase, addingBoard, queueIds.length]);

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
    api.getSourceApps().then(setSourceApps).catch(() => {});
  }, []);

  useEffect(() => {
    const u1 = onClipsUpdated(() => reloadRef.current());
    // 智能集合/其它设置变更时刷新 cfg(集合定义即 settings JSON)
    const u3 = onSettingsChanged(() => loadCfg());
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
      u3.then((f) => f());
    };
  }, []);

  // 面板失焦自动隐藏:点击面板之外(桌面/其它应用)时窗口收不到任何点击事件,
  // blur 是唯一的退场信号。守卫初值取 document.hasFocus():监听器挂载时窗口
  // 可能已持有焦点(不会再来 focus 事件),若初值恒为 false,之后的 blur 会被
  // 静默吞掉 —— 面板僵住不退场。监听器只挂一次,不随抽屉开合重建。
  useEffect(() => {
    let hadFocus = document.hasFocus();
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
  }, [requestHide]);

  // 统一的「点空白隐藏面板」语义(窗口级 capture 监听,先于一切子元素事件):
  // - 点击可交互元素(按钮/输入框/卡片/链接等):元素自身逻辑,面板保留
  // - 其余一切非交互区域 —— 包括编辑抽屉自身的留白 —— 一律关闭抽屉并隐藏面板。
  //   豁免名单只认「真正可交互的东西」,避免「看起来是空白却点不动」的顿挫感
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      const el = e.target instanceof HTMLElement ? e.target : null;
      if (!el) return;
      if (el.closest("button, input, textarea, select, a, label, [data-clip-card], [data-no-autohide]")) return;
      setDetail(null);
      setDetailAnchor(null);
      requestHide();
    };
    window.addEventListener("mousedown", onDown, true);
    return () => window.removeEventListener("mousedown", onDown, true);
  }, [requestHide]);

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

  // 两段式 Esc:抽屉打开时先关抽屉;队列非空时先清队列;最后才隐藏面板
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      if (detail) {
        setDetail(null);
        setDetailAnchor(null);
      } else if (queueIds.length > 0) {
        setQueueIds([]);
      } else {
        requestHide();
      }
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
    } else if (e.key === "Enter" && queueIds.length > 0) {
      // 队列非空:回车 = 开始顺序粘贴
      e.preventDefault();
      void startQueue();
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
    >
      <div
        className={`panel-surface h-full flex flex-col rounded-t-2xl bg-white/90 dark:bg-neutral-900/90 backdrop-blur-xl ring-1 ring-black/10 dark:ring-white/15 overflow-hidden ${entered ? "entered" : ""} ${leaving ? "leaving" : ""}`}
      >
        {/* 高度测量容器:自然高度 = 搜索栏 + 提示条 + 看板栏 + 卡片流,
            测量它并把结果交给 Rust 设置窗口高度,使内容刚好撑满、无纵向滚动条 */}
        <div ref={contentRef} className="flex flex-col">
        {/* 搜索 + 类型筛选 */}
        <div className="flex flex-wrap items-center gap-2 px-4 pt-3.5">
          <input
            ref={searchRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("panel.searchPlaceholder")}
            autoFocus
            className="flex-1 h-9 px-3 rounded-lg bg-black/5 dark:bg-white/10 text-sm outline-none text-neutral-800 dark:text-neutral-100 placeholder:text-neutral-400 dark:placeholder:text-neutral-500"
          />
          <div className="flex gap-1 items-center">
            <select
              value={sourceApp ?? ""}
              onChange={(e) => {
                leaveSmart();
                setSourceApp(e.target.value === "" ? null : e.target.value);
              }}
              title={t("panel.filter.sourceApp")}
              className="h-9 px-2 max-w-[120px] rounded-lg bg-black/5 dark:bg-white/10 text-xs outline-none text-neutral-600 dark:text-neutral-300"
            >
              <option value="">{t("panel.filter.allSources")}</option>
              {sourceApps.map((a) => (
                <option key={a} value={a}>
                  {a}
                </option>
              ))}
            </select>
            <select
              value={range}
              onChange={(e) => setRange(e.target.value as typeof range)}
              title={t("panel.filter.time")}
              className="h-9 px-2 rounded-lg bg-black/5 dark:bg-white/10 text-xs outline-none text-neutral-600 dark:text-neutral-300"
            >
              <option value="all">{t("panel.filter.time.all")}</option>
              <option value="today">{t("panel.filter.time.today")}</option>
              <option value="7d">{t("panel.filter.time.7d")}</option>
              <option value="30d">{t("panel.filter.time.30d")}</option>
            </select>
            {kinds.map((k) => (
              <button
                key={k.label}
                onClick={() => {
                  leaveSmart();
                  setKind(k.key);
                }}
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

        {/* 看板标签页:历史 | 智能集合(✨) | 手动看板 | + 新建 */}
        <div className="flex items-center gap-1 px-4 mt-2 flex-wrap">
          <button
            onClick={() => {
              setActiveSmart(null);
              setBoardId(null);
            }}
            className={tabCls(boardId === null && !activeSmart)}
          >
            {t("panel.board.history")}
          </button>
          {smartCols.map((sc) => (
            <button
              key={sc.id}
              onClick={() => clickSmart(sc)}
              onDoubleClick={() => {
                const name = window.prompt?.(t("panel.board.renamePrompt"), sc.name);
                if (name && name.trim()) {
                  api
                    .renameSmartCollection(sc.id, name.trim())
                    .then(loadCfg)
                    .catch(() => {});
                }
              }}
              className={tabCls(activeSmart?.id === sc.id)}
              title={t("panel.smart.renameTitle")}
            >
              ✨ {sc.name}
            </button>
          ))}
          {boards.map((b) => (
            <button
              key={b.id}
              onClick={() => {
                setActiveSmart(null);
                setBoardId(b.id);
              }}
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

        {/* 粘贴队列操作条:选中非空时出现 */}
        {queueIds.length > 0 && (
          <div className="mx-4 mt-2 flex items-center justify-between gap-2 rounded-lg bg-violet-500/10 ring-1 ring-violet-500/30 px-3 py-1.5 text-xs">
            {queueError ? (
              <span className="text-red-500 dark:text-red-400 truncate">⚠️ {queueError}</span>
            ) : (
              <span className="text-violet-700 dark:text-violet-300 font-medium">
                {t("panel.queue.count", { n: queueIds.length })}
              </span>
            )}
            <div className="flex items-center gap-2 shrink-0">
              <button
                onClick={queueSelectAll}
                className="px-2 py-0.5 rounded-md hover:bg-violet-500/15 text-violet-700 dark:text-violet-300"
              >
                {t("panel.queue.selectAll")}
              </button>
              <button
                onClick={() => setQueueIds([])}
                className="px-2 py-0.5 rounded-md hover:bg-violet-500/15 text-neutral-500 dark:text-neutral-400"
              >
                {t("panel.queue.clear")}
              </button>
              <button
                onClick={() => void startQueue()}
                className="px-2.5 py-1 rounded-md bg-violet-500 text-white hover:bg-violet-600 font-medium"
              >
                {t("panel.queue.start")}
              </button>
            </div>
          </div>
        )}

        {/* 卡片流:去掉 flex-1 让它保持自然高度(卡片 224 + 上下内边距 24 = 248),
            并显式 overflow-y-hidden —— CSS 规则下 overflow-x-auto 会把另一轴从 visible
            隐式提升为 auto,不显式关掉就会冒出纵向滚动条 */}
        <div className="flex items-stretch gap-3 px-4 py-3 overflow-x-auto overflow-y-hidden min-h-[248px]">
          {clips.length === 0 && (
            <div className="m-auto text-sm text-neutral-400 dark:text-neutral-500">
              {t("panel.empty")}
            </div>
          )}
          {clips.map((c, i) => {
            const qi = queueIds.indexOf(c.id);
            return (
              <ClipCard
                key={c.id}
                clip={c}
                selected={i === selected}
                queueIndex={qi >= 0 ? qi + 1 : undefined}
                boards={boards}
                onClick={(e) => {
                  // Shift/⌘/Ctrl+点击 = 加入/移出粘贴队列;普通点击 = 粘贴
                  if (e.shiftKey || e.metaKey || e.ctrlKey) {
                    toggleQueue(c.id);
                    return;
                  }
                  pasteClip(c.id, plainFor(e));
                }}
                onDetail={(rect) => {
                  setDetailAnchor(rect);
                  setDetail(c);
                }}
              />
            );
          })}
        </div>
        </div>
      </div>

      {detail && (
        <DetailDrawer
          clip={detail}
          boards={boards}
          anchor={detailAnchor}
          rootRef={detailRootRef}
          revealLock={revealLockOn}
          unlocked={revealUnlocked}
          onUnlocked={() => setRevealUnlocked(true)}
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
