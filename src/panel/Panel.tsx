import { useCallback, useEffect, useRef, useState } from "react";
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
  const [animKey, setAnimKey] = useState(0);
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

  // 关闭面板:先播放淡出退场动画,动画结束后再真正隐藏窗口,与进场对称
  const EXIT_MS = 200;
  const requestHide = useCallback(() => {
    setEntered(false);
    setLeaving(true);
    window.setTimeout(() => {
      api.hidePanel().catch(() => {});
    }, EXIT_MS);
  }, []);

  const pasteClip = useCallback((id: number, plain: boolean = false) => {
    const now = Date.now();
    if (lastPasteRef.current.id === id && now - lastPasteRef.current.at < 600) {
      return;
    }
    lastPasteRef.current = { id, at: now };
    playTick();
    // 粘贴成功后淡出关闭(失败则保留面板以便重试)
    api.pasteClip(id, plain).then(() => requestHide()).catch(() => {});
  }, [playTick, requestHide]);

  const searchRef = useRef<HTMLInputElement>(null);
  const reloadRef = useRef<() => void>(() => {});

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
      // 先立即隐藏(entered/leaving 都清掉),屏蔽窗口几何 resize 那一两帧(860 宽 → 全宽)带来的滚动条闪烁
      setEntered(false);
      setLeaving(false);
      // 等两帧,确保 WebView 已按全宽重排,再揭幕并播放进场动画
      requestAnimationFrame(() =>
        requestAnimationFrame(() => {
          setEntered(true);
          setAnimKey((k) => k + 1);
          reloadRef.current();
          loadCfg();
          // 应用可能连续运行好几天不重启,每次唤起面板都重新判定一次试用阶段
          licenseReloadRef.current();
          searchRef.current?.focus();
          searchRef.current?.select();
          // 每次唤起都刷新权限状态:授权后提示条自动消失;
          // 未授权时本会话内自动弹出一次系统原生授权提示
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

  // 编辑抽屉打开时,点击抽屉以外的区域(空白 / 其它卡片)关闭抽屉。
  // 用 capture 阶段拦截,命中检测以抽屉根节点为准,避免误伤抽屉内部交互。
  useEffect(() => {
    if (!detail) return;
    const onDown = (e: MouseEvent) => {
      const node = e.target as Node;
      if (detailRootRef.current && !detailRootRef.current.contains(node)) {
        setDetail(null);
        setDetailAnchor(null);
      }
    };
    window.addEventListener("mousedown", onDown, true);
    return () => window.removeEventListener("mousedown", onDown, true);
  }, [detail]);

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
    if (detail) return;
    if (e.key === "Escape") {
      e.preventDefault();
      requestHide();
    } else if ((e.metaKey || e.ctrlKey) && /^[1-9]$/.test(e.key)) {
      // 快速粘贴:⌘/Ctrl + 1..9 直接粘贴第 N 条
      e.preventDefault();
      const c = clips[Number(e.key) - 1];
      if (c) pasteClip(c.id, plainFor(e));
    } else if (e.metaKey && (e.key === "[" || e.key === "]")) {
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
    >
      <div
        key={animKey}
        className={`panel-surface h-full flex flex-col rounded-t-2xl bg-white/90 dark:bg-neutral-900/90 backdrop-blur-xl ring-1 ring-black/10 dark:ring-white/15 overflow-hidden ${entered ? "entered" : ""} ${leaving ? "leaving" : ""}`}
      >
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

        {/* 卡片流 */}
        <div className="flex-1 flex items-stretch gap-3 px-4 py-3 overflow-x-auto">
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
              index={i}
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
