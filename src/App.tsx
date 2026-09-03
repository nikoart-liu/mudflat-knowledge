import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent } from 'react';
import { Channel } from '@tauri-apps/api/core';
import {
  call,
  emptyFilter,
  type BookRow,
  type CardFilter,
  type CardRow,
  type ReviewRating,
  type LlmDraft,
  type LlmProvider,
  type LlmSettings,
  type Mindmap,
  type MindmapEventPayload,
  type MindmapNode,
  type MindmapStatus,
  type ReviewSettings,
  type SettingsInfo,
  type SetupStatus,
  type SrsState,
  type SyncEventPayload,
  type SyncSummary,
  type TagRow,
  type ClueTask,
} from './types';
import './App.css';
import { exportClueOutline, layoutMindmap, visibleClue } from './mindmap-layout';
import { formatClueElapsed, formatClueProgress } from './clue-progress';

type CardsView = { name: 'cards'; bookId: number | null };
type View = CardsView | { name: 'review'; bookId: number | null } | { name: 'mindmap'; bookId: number } | { name: 'settings' };

export type WallScope = 'book' | 'chapter';
export type WallGroup = { key: string; label: string; mono: boolean; cards: CardRow[] };

/// 卡片墙分组（纯函数，供单测）。保持输入顺序（created_at DESC）：
/// - scope='book'：总索引按书分组，衬线书名做分隔行；
/// - scope='chapter'：书内按章节分组——用户进了一本书，心里装的是这本书的结构，
///   而不是日历月份；chapterUid 缺失时归入「未分章」。
export function buildWallGroups(cards: CardRow[], scope: WallScope): WallGroup[] {
  const map = new Map<string, WallGroup>();
  for (const c of cards) {
    let key: string, label: string, mono = false;
    if (scope === 'book') {
      key = c.bookTitle || 'self';
      label = c.bookTitle || '自建卡';
    } else {
      // 以裁切后的章名为准：有章名按 uid 分组；章名空白（哪怕有 uid）归「未分章」，
      // 因为对用户可辨的只有章名，uid 单独成组只会多出一行无名的分隔线。
      const t = c.chapterTitle?.trim();
      key = t ? `c-${c.chapterUid ?? t}` : 'no-chapter';
      label = t || '未分章';
    }
    let g = map.get(key);
    if (!g) { g = { key, label, mono, cards: [] }; map.set(key, g); }
    g.cards.push(c);
  }
  return [...map.values()];
}

const PAGE = 500;
const SEARCH_CAP = 200;

function fmtDate(unix: number): string {
  const d = new Date(unix * 1000);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

function explainError(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  const s = raw.replace(/^Error:\s*/, '');
  if (/transformCallback|Cannot read properties of undefined|IPC|invoke/i.test(s)) {
    return '现在连不上本地服务。请用应用窗口打开，不要用浏览器。';
  }
  if (/尚未保存 API Key/.test(s)) {
    return '还没有 Key。到设置里粘贴以 wrk- 开头的微信读书 API Key。';
  }
  if (/读取 API Key 失败/.test(s)) {
    return '读取 Key 失败。到设置里重新保存 API Key。';
  }
  return s;
}

// 微信读书 Skills 开通页：设置页里的外链，点击调系统默认浏览器打开。
const WEREAD_SKILLS_URL = 'https://weread.qq.com/r/weread-skills';

/// 外链打开：Tauri 里走后端命令唤起系统浏览器；纯浏览器环境（vite dev 直开）回退 window.open。
async function openExternal(url: string): Promise<void> {
  try {
    await call('open_external', { url });
  } catch {
    window.open(url, '_blank', 'noopener');
  }
}

// 四档评分：数字键 1–4 与点击一致（R1）。文案用中文，Again 等枚举仅内部使用。
const RATING_DEFS: { key: ReviewRating; num: number; label: string }[] = [
  { key: 'again', num: 1, label: '忘了' },
  { key: 'hard', num: 2, label: '困难' },
  { key: 'good', num: 3, label: '记得' },
  { key: 'easy', num: 4, label: '简单' },
];

const REVIEW_BATCH_OPTIONS = [10, 20, 30];
const LLM_PROVIDERS: { key: LlmProvider; label: string; baseUrl: string; model: string }[] = [
  { key: 'off', label: '关闭', baseUrl: '', model: '' },
  { key: 'openai', label: 'OpenAI', baseUrl: 'https://api.openai.com/v1', model: 'gpt-4o-mini' },
  { key: 'ollama', label: 'Ollama', baseUrl: 'http://127.0.0.1:11434/v1', model: 'qwen2.5' },
  { key: 'custom', label: '自定义', baseUrl: '', model: '' },
];
const EXCLUDE_HINT_KEY = 'mudflat.exclude-hint-seen';
const READING_MODE_KEY = 'mudflat.reading-mode';

function nowSecs(): number {
  return Math.floor(Date.now() / 1000);
}

/// 把距下次到期的秒数翻成「10 分钟后 / 明天 / 3 天后」（R1.4）。
function humanizeDue(deltaSec: number): string {
  if (deltaSec <= 0) return '现在';
  const minutes = Math.round(deltaSec / 60);
  if (minutes < 60) return `${Math.max(1, minutes)} 分钟后`;
  const hours = Math.round(deltaSec / 3600);
  if (hours < 24) return `${hours} 小时后`;
  const days = Math.round(deltaSec / 86400);
  if (days <= 1) return '明天';
  return `${days} 天后`;
}

const ICON_PATHS: Record<string, string> = {  layers: '<path d="m12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83Z"/><path d="m22 17.65-9.17 4.16a2 2 0 0 1-1.66 0L2 17.65"/><path d="m22 12.65-9.17 4.16a2 2 0 0 1-1.66 0L2 12.65"/>',
  search: '<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>',
  refresh: '<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/>',
  plus: '<path d="M5 12h14"/><path d="M12 5v14"/>',
  star: '<path d="M11.5 3.6a.5.5 0 0 1 .95 0l2.05 6.1h6.4a.5.5 0 0 1 .36.86l-5.2 4.9 1.9 6.2a.5.5 0 0 1-.74.56L12 18.8l-5.22 3.42a.5.5 0 0 1-.74-.56l1.9-6.2-5.2-4.9a.5.5 0 0 1 .36-.86h6.4Z"/>',
  pen: '<path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"/><path d="m15 5 4 4"/>',
  trash: '<path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>',
  gear: '<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>',
  book: '<path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1 0-5H20"/>',
};

function Icon({ name, size = 14 }: { name: keyof typeof ICON_PATHS | string; size?: number }) {
  const path = ICON_PATHS[name] ?? ICON_PATHS.book;
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor"
      strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"
      dangerouslySetInnerHTML={{ __html: path }} />
  );
}

function Cover({ src, title, large }: { src: string; title: string; large?: boolean }) {
  const [failed, setFailed] = useState(false);
  const cls = large ? 'cover-lg' : 'cover-box';
  const showImg = src && !failed;
  return (
    <div className={cls} style={{ background: large && !showImg ? 'var(--panel)' : undefined }} aria-hidden={large ? undefined : true}>
      {showImg
        ? <img src={src} alt={large ? title : ''} loading="lazy" onError={() => setFailed(true)} />
        : <span className="cover-fallback">{title.slice(0, large ? 8 : 4)}</span>}
    </div>
  );
}

export default function App() {
  const [view, setView] = useState<View>({ name: 'cards', bookId: null });
  const [books, setBooks] = useState<BookRow[]>([]);
  const [tags, setTags] = useState<TagRow[]>([]);
  const [selectedTagIds, setSelectedTagIds] = useState<number[]>([]);
  const [starredOnly, setStarredOnly] = useState(false);
  const [query, setQuery] = useState('');
  const [cards, setCards] = useState<CardRow[]>([]);
  const [cardTotal, setCardTotal] = useState(0);
  const [cardsReady, setCardsReady] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [editing, setEditing] = useState<CardRow | null>(null);
  const [creating, setCreating] = useState(false);
  const [tagToDelete, setTagToDelete] = useState<TagRow | null>(null);
  const [syncing, setSyncing] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [dueCount, setDueCount] = useState(0);
  // 本书清样入口：书内到期数（随全局 dueCount 变化重查，覆盖同步后/回顾归来）
  const [bookDue, setBookDue] = useState(0);
  const [setup, setSetup] = useState<SetupStatus | null>(null);
  // P1.3 搜索继承上下文：默认只在当前书/标签/星标范围内搜，可一键扩到全部卡片
  const [searchAll, setSearchAll] = useState(false);
  // 长读模式：单栏通读，为「重读一本书的笔记」服务（V 切换，本地记忆）
  const [reading, setReading] = useState(() => {
    try { return localStorage.getItem(READING_MODE_KEY) === '1'; } catch { return false; }
  });
  useEffect(() => {
    try { localStorage.setItem(READING_MODE_KEY, reading ? '1' : '0'); } catch { /* 本地存储不可用时忽略 */ }
  }, [reading]);
  const contentHeaderRef = useRef<HTMLDivElement>(null);
  const mainRef = useRef<HTMLElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const [chH, setChH] = useState(76);
  const lastViewRef = useRef<Exclude<View, { name: 'settings' }>>({ name: 'cards', bookId: null });

  const searching = !!query.trim();
  const hasKey = setup?.hasKey ?? false;
  const hasBooks = setup?.hasBooks ?? false;
  const emptyLibrary = !hasBooks && cards.length === 0 && !searching;
  const hideFab = emptyLibrary;

  useEffect(() => {
    const el = contentHeaderRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setChH(el.offsetHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, [view.name]);

  const showToast = useCallback((msg: string) => {
    setToast(msg);
    window.setTimeout(() => setToast(null), 2600);
  }, []);

  const refreshMeta = useCallback(async () => {
    const [bookRows, tagRows, due, status] = await Promise.all([
      call<BookRow[]>('list_books'),
      call<TagRow[]>('list_tags'),
      call<number>('get_due_count').catch(() => 0),
      call<SetupStatus>('get_setup_status'),
    ]);
    setBooks(bookRows);
    setTags(tagRows);
    setDueCount(due);
    setSetup(status);
  }, []);

  const [clueTasks, setClueTasks] = useState<Map<number, ClueTask>>(new Map());
  const clueTimersRef = useRef<Map<number, number>>(new Map());

  useEffect(() => {
    return () => {
      for (const t of clueTimersRef.current.values()) {
        window.clearInterval(t);
      }
      clueTimersRef.current.clear();
    };
  }, []);

  const startGenerateClue = useCallback(async (book: { id: number; title: string }) => {
    const existing = clueTasks.get(book.id);
    if (existing?.busy) return;

    const oldTimer = clueTimersRef.current.get(book.id);
    if (oldTimer) {
      window.clearInterval(oldTimer);
      clueTimersRef.current.delete(book.id);
    }

    const started = Date.now();
    const tick = window.setInterval(() => {
      const sec = Math.floor((Date.now() - started) / 1000);
      setClueTasks((prev) => {
        const cur = prev.get(book.id);
        if (!cur || !cur.busy) return prev;
        const next = new Map(prev);
        next.set(book.id, { ...cur, elapsed: sec });
        return next;
      });
    }, 1000);
    clueTimersRef.current.set(book.id, tick);

    setClueTasks((prev) => {
      const next = new Map(prev);
      next.set(book.id, {
        bookId: book.id,
        bookTitle: book.title,
        busy: true,
        progress: '整理卡片…',
        progressFrac: null,
        progressFails: [],
        elapsed: 0,
        error: null,
        result: null,
      });
      return next;
    });

    try {
      const chan = new Channel<MindmapEventPayload>();
      chan.onmessage = (ev) => {
        const line = formatClueProgress(ev);
        setClueTasks((prev) => {
          const cur = prev.get(book.id);
          if (!cur) return prev;
          const next = new Map(prev);
          const fails = ev.stage === 'chapter_failed'
            ? [...cur.progressFails, formatClueProgress(ev)]
            : cur.progressFails;
          next.set(book.id, {
            ...cur,
            progress: line || cur.progress,
            progressFrac: ev.total > 0 ? { current: ev.current, total: ev.total } : cur.progressFrac,
            progressFails: fails,
          });
          return next;
        });
      };
      const result = await call<Mindmap>('generate_mindmap', { bookId: book.id, onProgress: chan });
      setClueTasks((prev) => {
        const cur = prev.get(book.id);
        const next = new Map(prev);
        next.set(book.id, {
          bookId: book.id,
          bookTitle: book.title,
          busy: false,
          progress: null,
          progressFrac: null,
          progressFails: cur?.progressFails ?? [],
          elapsed: 0,
          error: null,
          result,
        });
        return next;
      });
      showToast(`《${book.title}》线索归纳完成`);
    } catch (e) {
      const msg = explainError(e);
      setClueTasks((prev) => {
        const cur = prev.get(book.id);
        const next = new Map(prev);
        next.set(book.id, {
          bookId: book.id,
          bookTitle: book.title,
          busy: false,
          progress: null,
          progressFrac: null,
          progressFails: cur?.progressFails ?? [],
          elapsed: 0,
          error: msg,
          result: null,
        });
        return next;
      });
      showToast(msg.split('\n')[0] || msg);
    } finally {
      const t = clueTimersRef.current.get(book.id);
      if (t) {
        window.clearInterval(t);
        clueTimersRef.current.delete(book.id);
      }
    }
  }, [clueTasks, showToast]);

  const runningClueTask = useMemo(() => {
    for (const t of clueTasks.values()) {
      if (t.busy) return t;
    }
    return null;
  }, [clueTasks]);

  const wallFilter = useCallback((): CardFilter => ({
    ...emptyFilter(),
    bookId: view.name === 'cards' ? view.bookId : null,
    tagIds: selectedTagIds,
    starredOnly,
  }), [view, selectedTagIds, starredOnly]);

  const reloadCards = useCallback(async () => {
    if (view.name !== 'cards') return;
    const q = query.trim();
    try {
      if (q) {
        const filter = searchAll ? emptyFilter() : wallFilter();
        const rows = await call<CardRow[]>('search_cards', { q, filter });
        setCards(rows);
        setCardTotal(rows.length);
        return;
      }
      const filter = wallFilter();
      const [rows, total] = await Promise.all([
        call<CardRow[]>('query_cards', { filter, limit: PAGE, offset: 0 }),
        call<number>('count_cards', { filter }),
      ]);
      setCards(rows);
      setCardTotal(total);
    } finally {
      setCardsReady(true);
    }
  }, [view, query, wallFilter, searchAll]);

  const loadMore = async () => {
    if (searching || loadingMore || cards.length >= cardTotal) return;
    setLoadingMore(true);
    try {
      const filter = wallFilter();
      const rows = await call<CardRow[]>('query_cards', { filter, limit: PAGE, offset: cards.length });
      setCards((prev) => [...prev, ...rows]);
    } catch (e) {
      showToast(explainError(e));
    } finally {
      setLoadingMore(false);
    }
  };

  useEffect(() => {
    refreshMeta().catch((e) => showToast(explainError(e)));
  }, [refreshMeta, showToast]);

  useEffect(() => {
    if (query.trim()) return;
    reloadCards().catch((e) => showToast(explainError(e)));
  }, [reloadCards, showToast]);

  useEffect(() => {
    if (view.name !== 'cards' || !query.trim()) return;
    const t = window.setTimeout(() => {
      reloadCards().catch((e) => showToast(explainError(e)));
    }, 250);
    return () => window.clearTimeout(t);
  }, [query, view, reloadCards, showToast]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      const typing = t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable);
      if (e.key === '/' && !typing && !e.metaKey && !e.ctrlKey && view.name === 'cards') {
        e.preventDefault();
        searchRef.current?.focus();
      }
      if (e.key === 'v' && !typing && !e.metaKey && !e.ctrlKey && !e.altKey && view.name === 'cards') {
        e.preventDefault();
        setReading((r) => !r);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [view.name]);

  const doSync = useCallback(async () => {
    if (!hasKey) {
      showToast('还没有 Key。到设置里粘贴以 wrk- 开头的微信读书 API Key。');
      setView({ name: 'settings' });
      return;
    }
    setSyncing('准备同步…');
    try {
      const chan = new Channel<SyncEventPayload>();
      chan.onmessage = (ev) => {
        if (ev.stage === 'pulling') setSyncing(`同步 ${ev.current}/${ev.total}：${ev.bookTitle}`);
        else if (ev.stage === 'books') setSyncing(`已更新书目 ${ev.total} 本`);
        else if (ev.stage === 'repair') setSyncing(`一次性修复卡片时间：全量重拉 ${ev.total} 本书`);
        else if (ev.stage === 'book_failed') setSyncing(`「${ev.bookTitle}」同步失败，继续下一本`);
        else if (ev.stage === 'done') setSyncing(null);
      };
      const summary: SyncSummary = await call('sync_all', { onProgress: chan });
      const parts = [`成功 ${summary.booksSynced} 本`];
      if (summary.booksFailed > 0) parts.push(`失败 ${summary.booksFailed} 本`);
      parts.push(`新增 ${summary.added} 张`, `移除 ${summary.removed} 张`);
      let msg = `同步完成：${parts.join(' · ')}`;
      if (summary.failures?.length) {
        const names = summary.failures.map((f) => f.title).slice(0, 3).join('、');
        msg += `\n失败书目：${names}${summary.failures.length > 3 ? ' 等' : ''}，下次同步会自动重试`;
      }
      showToast(msg);
      await refreshMeta();
      await reloadCards();
    } catch (e) {
      showToast(explainError(e));
    } finally {
      setSyncing(null);
    }
  }, [hasKey, refreshMeta, reloadCards, showToast]);

  const toggleTagFilter = (id: number) =>
    setSelectedTagIds((prev) => (prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id]));

  const deleteTag = async (t: TagRow) => {
    try {
      await call('delete_tag', { tagId: t.id });
      setSelectedTagIds((prev) => prev.filter((id) => id !== t.id));
      await refreshMeta();
      showToast(`已删除标签「${t.name}」`);
    } catch (e) {
      showToast(explainError(e));
    }
  };

  // 墙的滚动位置是上一本书的阅读痕迹：换书/换索引时回到墙顶，否则会落在
  // 半截内容上，误以为书只有这么几张卡。从回顾/设置回到同一面墙时不经此
  // 函数，原位保留。
  const scrollTopHome = () => {
    if (mainRef.current) mainRef.current.scrollTop = 0;
  };

  const clearFilters = () => {
    setQuery('');
    setStarredOnly(false);
    setSelectedTagIds([]);
    setSearchAll(false);
    setView({ name: 'cards', bookId: null });
    scrollTopHome();
  };

  const toggleSettings = () => {
    if (view.name === 'settings') {
      setView(lastViewRef.current);
      return;
    }
    lastViewRef.current = { name: 'cards', bookId: view.bookId };
    setView({ name: 'settings' });
  };

  const goCards = (bookId: number | null) => {
    setQuery('');
    setSearchAll(false);
    // 进书即净：书是用户此刻唯一的关注范围，上一本书/总索引留下的标签与星标
    // 筛选不该跟着进来，否则「这本书怎么只有几张卡」是错误归因。
    setSelectedTagIds([]);
    setStarredOnly(false);
    setView({ name: 'cards', bookId });
    scrollTopHome();
  };

  const activeBook = !searching && view.name === 'cards' && view.bookId
    ? books.find((b) => b.id === view.bookId) ?? null
    : null;
  const filtered = starredOnly || selectedTagIds.length > 0 || (view.name === 'cards' && view.bookId !== null);

  const activeBookId = activeBook?.id ?? null;
  useEffect(() => {
    if (!activeBookId) { setBookDue(0); return; }
    let alive = true;
    call<number>('get_due_count', { bookId: activeBookId })
      .then((n) => { if (alive) setBookDue(n); })
      .catch(() => { if (alive) setBookDue(0); });
    return () => { alive = false; };
  }, [activeBookId, dueCount]);

  const wallGroups = useMemo(() => {
    if (view.name !== 'cards' || searching) return null;
    return buildWallGroups(cards, view.bookId ? 'chapter' : 'book');
  }, [cards, view, searching]);

  // 书内章节数：只在当前墙已全部载入时展示，避免分页时给错数
  const chapterCount = useMemo(() => {
    if (!activeBook) return 0;
    const s = new Set<number>();
    for (const c of cards) if (c.chapterUid != null) s.add(c.chapterUid);
    return s.size;
  }, [activeBook, cards]);

  const countLabel = (() => {
    if (searching) {
      const scope = !searchAll && filtered ? '正在搜当前范围' : '正在搜全部卡片';
      if (cards.length >= SEARCH_CAP) return `前 ${SEARCH_CAP} 条 · ${scope}`;
      return `${cards.length} 张 · ${scope}`;
    }
    if (cardTotal > cards.length) return `已显示 ${cards.length} / 共 ${cardTotal}`;
    return `${cards.length} 张`;
  })();

  const reviewing = view.name === 'review';
  const readingActive = reading && view.name === 'cards' && !searching;

  return (
    <div className={`app${reviewing ? ' is-review' : ''}${view.name === 'mindmap' ? ' is-mindmap' : ''}${readingActive ? ' is-reading' : ''}${view.name === 'settings' ? ' is-settings' : ''}`}>
      <header className="topbar">
        <div className="logo">
          <img className="mark" src="/logo-mark.svg" width={20} height={20} alt="" />
          <span>滩涂拾遗</span>
        </div>
        <div className="search">
          <Icon name="search" size={13} />
          <input
            ref={searchRef}
            type="search"
            placeholder={filtered ? '在当前范围检索…' : '检索全部卡片…'}
            aria-label={filtered ? '在当前范围内检索卡片，按 / 聚焦' : '检索全部卡片，按 / 聚焦'}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === 'Escape' && setQuery('')}
          />
        </div>
        <div className="top-actions">
          {runningClueTask && (
            <button
              type="button"
              className="ghost clue-top-indicator"
              onClick={() => setView({ name: 'mindmap', bookId: runningClueTask.bookId })}
              title={`《${runningClueTask.bookTitle}》线索正在归纳中，点击查看进度`}
            >
              <span className="clue-top-dot" />
              归纳《{runningClueTask.bookTitle}》{runningClueTask.progressFrac ? ` ${Math.round((runningClueTask.progressFrac.current / runningClueTask.progressFrac.total) * 100)}%` : '…'}
            </button>
          )}
          <button
            className="primary top-sync"
            disabled={!!syncing || !hasKey}
            title={!hasKey ? '请先到设置填写 API Key' : undefined}
            onClick={doSync}
          >
            {syncing ?? '同步'}
          </button>
          <button
            className={`ghost ${view.name === 'settings' ? 'active' : ''}`}
            onClick={toggleSettings}
            title="设置"
            aria-label="设置"
            aria-pressed={view.name === 'settings'}
          >
            <Icon name="gear" size={14} />
          </button>
        </div>
      </header>

      <div className="body">
        <aside className="sidebar">
          <section className="side-group side-review">
            <button
              className={`side-item review-cta${dueCount > 0 ? ' due' : ''}${view.name === 'review' ? ' active' : ''}`}
              onClick={() => setView({ name: 'review', bookId: null })}
              title="翻牌"
              aria-current={view.name === 'review' ? 'true' : undefined}
            >
              <Icon name="refresh" size={13} />
              <span className="grow">翻牌回顾</span>
              {dueCount > 0 && <span className="count">{dueCount}</span>}
            </button>
          </section>
          <div className="side-scroll">
            <section className="side-group">
              <h2>书架</h2>
              <button
                className={`side-item ${!searching && view.name === 'cards' && !view.bookId ? 'active' : ''}`}
                aria-current={!searching && view.name === 'cards' && !view.bookId ? 'true' : undefined}
                onClick={() => goCards(null)}
              >
                <Icon name="layers" size={13} />
                <span className="grow">全部索引</span>
                <span className="count">{books.reduce((n, b) => n + b.noteCount + b.reviewCount, 0)}</span>
              </button>
              <button
                className={`side-item ${!searching && starredOnly ? 'active' : ''}`}
                onClick={() => setStarredOnly((v) => !v)}
                aria-pressed={starredOnly}
                title="只看星标卡片"
              >
                <Icon name="star" size={13} />
                <span className="grow">星标项目</span>
              </button>
              <div className="sub-eyebrow">刊物</div>
              {books.map((b, i) => (
                <button
                  key={b.id}
                  className={`side-item ${!searching && view.name === 'cards' && view.bookId === b.id ? 'active' : ''}`}
                  aria-current={!searching && view.name === 'cards' && view.bookId === b.id ? 'true' : undefined}
                  onClick={() => goCards(b.id)}
                  title={b.title}
                >
                  <span className="book-no" aria-hidden="true">{String(i + 1).padStart(2, '0')}</span>
                  <span className="grow book-title">{b.title}</span>
                  <span className="count">{b.noteCount + b.reviewCount}</span>
                </button>
              ))}
            </section>
          </div>
          <section className="side-group side-tags">
            <h2>标签</h2>
            <div className="tag-cloud">
              {tags.map((t) => (
                <span key={t.id} className="chip-item">
                  <button
                    className={`chip ${!searching && selectedTagIds.includes(t.id) ? 'active' : ''}`}
                    aria-pressed={selectedTagIds.includes(t.id)}
                    onClick={() => toggleTagFilter(t.id)}
                  >
                    {t.name}
                  </button>
                  <button
                    className="chip-del"
                    aria-label={`删除标签 ${t.name}`}
                    title={`删除标签「${t.name}」`}
                    onClick={() => setTagToDelete(t)}
                  >
                    ×
                  </button>
                </span>
              ))}
              {!tags.length && <p className="hint">尚无标签</p>}
            </div>
          </section>
        </aside>

        <main className="main" ref={mainRef}>
          {view.name === 'cards' && (
            <>
              <div className="content-header" ref={contentHeaderRef}>
                <div className="head-left">
                  {searching ? (
                    <div style={{ minWidth: 0 }}>
                      <div className="head-eyebrow">检索结果</div>
                      <h2>「{query.trim()}」</h2>
                      <div className="head-meta">
                        <span className="card-date">{countLabel}</span>
                        {filtered && (
                          searchAll ? (
                            <button className="link-btn" onClick={() => setSearchAll(false)}>只搜当前范围</button>
                          ) : (
                            <button className="link-btn" onClick={() => setSearchAll(true)}>搜索全部卡片</button>
                          )
                        )}
                      </div>
                    </div>
                  ) : activeBook ? (
                    <>
                      <Cover src={activeBook.cover} title={activeBook.title} large />
                      <div style={{ minWidth: 0 }}>
                        <div className="head-eyebrow">当前刊物</div>
                        <h2>{activeBook.title}</h2>
                        <div className="head-meta">
                          <span className="author">{activeBook.author || '佚名'}</span>
                          <span className="chip">划线 {activeBook.noteCount}</span>
                          <span className="chip">想法 {activeBook.reviewCount}</span>
                          {chapterCount > 0 && cards.length >= cardTotal && (
                            <span className="chip">章节 {chapterCount}</span>
                          )}
                          {bookDue > 0 && (
                            <button
                              className="due-chip"
                              onClick={() => setView({ name: 'review', bookId: activeBook.id })}
                              title="只翻这本书的到期卡片"
                            >
                              翻牌 {bookDue}
                            </button>
                          )}
                          <button
                            className={`link-btn${clueTasks.get(activeBook.id)?.busy ? ' clue-busy' : ''}`}
                            onClick={() => setView({ name: 'mindmap', bookId: activeBook.id })}
                            title={clueTasks.get(activeBook.id)?.busy ? '线索正在归纳中，点击查看进度' : '用划线归纳一本书的主题线索'}
                          >
                            {clueTasks.get(activeBook.id)?.busy
                              ? `线索 · 归纳中${clueTasks.get(activeBook.id)?.progressFrac ? ` ${Math.round(((clueTasks.get(activeBook.id)?.progressFrac?.current ?? 0) / (clueTasks.get(activeBook.id)?.progressFrac?.total ?? 1)) * 100)}%` : '…'}`
                              : '线索'}
                          </button>
                          <span className="card-date">{countLabel}</span>
                        </div>
                      </div>
                    </>
                  ) : (
                    <div style={{ minWidth: 0 }}>
                      <div className="head-eyebrow">{starredOnly ? '星标专辑' : '总索引'}</div>
                      <h2>{starredOnly ? '星标项目' : '全部索引'}</h2>
                      <div className="head-meta"><span className="card-date">{countLabel}</span></div>
                    </div>
                  )}
                </div>
                <div className="head-actions">
                  {!searching && (
                    <button
                      className={`ghost view-toggle${readingActive ? ' active' : ''}`}
                      onClick={() => setReading((r) => !r)}
                      title={readingActive ? '切回版面墙（V）' : '长读 · 单栏通读（V）'}
                      aria-pressed={readingActive}
                    >
                      {readingActive ? '版面' : '长读'}
                    </button>
                  )}
                  {!hideFab && (
                    <button className="fab" title="新建卡片" aria-label="新建卡片" onClick={() => setCreating(true)}>
                      <Icon name="plus" size={16} />
                    </button>
                  )}
                </div>
              </div>
              <div className="wall" style={{ '--ch-h': `${chH}px` } as React.CSSProperties}>
                {cards.length > 0 && (wallGroups
                  ? wallGroups.map((g) => (
                    <section className="wall-group" key={g.key}>
                      <div className="wall-group-head">
                        <span className={`g-label${g.mono ? ' mono' : ''}`}>{g.label}</span>
                        <span className="g-count">{g.cards.length}</span>
                      </div>
                      {g.cards.map((c) => (
                        <Card key={c.id} card={c} onEdit={() => setEditing(c)} onChanged={reloadCards} onToast={showToast} />
                      ))}
                    </section>
                  ))
                  : <section className="wall-group">
                    {cards.map((c) => (
                      <Card key={c.id} card={c} onEdit={() => setEditing(c)} onChanged={reloadCards} onToast={showToast} />
                    ))}
                  </section>)}
                {cards.length > 0 && cards.length < cardTotal && !searching && (
                  <div className="wall-more">
                    <span className="mono">{countLabel}</span>
                    <button onClick={loadMore} disabled={loadingMore}>{loadingMore ? '载入中…' : '继续载入'}</button>
                  </div>
                )}
                {!cards.length && (
                  <EmptyWall
                    ready={cardsReady}
                    needsSetup={emptyLibrary && !hasKey}
                    hasKey={hasKey}
                    hasBooks={hasBooks}
                    searching={searching}
                    filtered={filtered}
                    query={query.trim()}
                    syncing={syncing}
                    onSetup={() => setView({ name: 'settings' })}
                    onSync={doSync}
                    onClear={clearFilters}
                  />
                )}
              </div>
            </>
          )}
          {view.name === 'mindmap' && (
            <MindmapView
              book={books.find((b) => b.id === view.bookId) ?? { id: view.bookId, title: '当前刊物' }}
              cards={cards}
              onToast={showToast}
              onExit={() => { refreshMeta(); setView({ name: 'cards', bookId: view.bookId }); }}
              onOpenCard={(card) => setEditing(card)}
              onNeedSettings={() => setView({ name: 'settings' })}
              activeTask={clueTasks.get(view.bookId)}
              onStartGenerate={startGenerateClue}
            />
          )}
          {view.name === 'review' && (
            <ReviewView
              book={view.bookId != null ? books.find((b) => b.id === view.bookId) ?? null : null}
              onToast={showToast}
              onExit={() => { refreshMeta(); setView({ name: 'cards', bookId: view.bookId }); }}
              hasKey={hasKey}
              hasBooks={hasBooks}
            />
          )}
          {view.name === 'settings' && (
            <SettingsView
              onToast={showToast}
              hasKey={hasKey}
              onKeyChange={refreshMeta}
            />
          )}
        </main>
      </div>

      {editing && (
        <EditModal
          card={editing}
          onClose={() => setEditing(null)}
          onPatched={async () => {
            await refreshMeta();
            await reloadCards();
          }}
          onSaved={async () => {
            setEditing(null);
            await refreshMeta();
            await reloadCards();
          }}
          onToast={showToast}
        />
      )}
      {creating && (
        <CreateModal
          onClose={() => setCreating(false)}
          onSaved={async () => {
            setCreating(false);
            await refreshMeta();
            await reloadCards();
          }}
          onToast={showToast}
        />
      )}
      {tagToDelete && (
        <ConfirmModal
          title="删除标签"
          message={`删除标签「${tagToDelete.name}」？将把它从所有卡片上移除。`}
          confirmLabel="删除标签"
          onConfirm={() => {
            const t = tagToDelete;
            setTagToDelete(null);
            if (t) deleteTag(t);
          }}
          onCancel={() => setTagToDelete(null)}
        />
      )}
      {toast && <div className="toast" role="status">{toast}</div>}
    </div>
  );
}

function EmptyWall({
  ready, needsSetup, hasKey, hasBooks, searching, filtered, query, syncing,
  onSetup, onSync, onClear,
}: {
  ready: boolean;
  needsSetup: boolean;
  hasKey: boolean;
  hasBooks: boolean;
  searching: boolean;
  filtered: boolean;
  query: string;
  syncing: string | null;
  onSetup: () => void;
  onSync: () => void;
  onClear: () => void;
}) {
  if (!ready) return <p className="hint empty-state">载入卡片…</p>;
  if (needsSetup || (!hasKey && !hasBooks)) {
    return (
      <div className="empty-setup">
        <p className="empty-title">把微信读书的划线接到这面墙上</p>
        <p className="empty-body">到设置粘贴 API Key，再同步。划线和想法会变成可检索、可翻牌的卡片。</p>
        <button className="primary" onClick={onSetup}>填写 API Key</button>
      </div>
    );
  }
  if (hasKey && !hasBooks) {
    return (
      <div className="empty-setup">
        <p className="empty-title">Key 已经存好，墙上还是空的</p>
        <p className="empty-body">同步一次，微信读书里的划线会出现在这里。</p>
        <button className="primary" onClick={onSync} disabled={!!syncing}>{syncing ?? '同步'}</button>
      </div>
    );
  }
  if (searching) {
    return (
      <div className="empty-setup">
        <p className="empty-title">没有找到「{query}」</p>
        <p className="empty-body">
          {filtered ? '检索范围是当前书、标签或星标，可点「搜索全部卡片」扩大范围。' : '检索范围是全部卡片。'}
          换个词，或按 Esc 退出检索。
        </p>
      </div>
    );
  }
  if (filtered) {
    return (
      <div className="empty-setup">
        <p className="empty-title">没有匹配的卡片</p>
        <p className="empty-body">当前书、标签或星标筛过了墙。清掉筛选，或换一本书。</p>
        <button onClick={onClear}>清除筛选</button>
      </div>
    );
  }
  return (
    <div className="empty-setup">
      <p className="empty-title">墙上还没有卡片</p>
      <p className="empty-body">同步微信读书，或自己写一张。</p>
      <button className="primary" onClick={onSync} disabled={!hasKey || !!syncing}>{syncing ?? '同步'}</button>
    </div>
  );
}

function Card({ card, onEdit, onChanged, onToast }: {
  card: CardRow;
  onEdit: () => void;
  onChanged: () => void;
  onToast: (m: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [overflowed, setOverflowed] = useState(false);
  const textRef = useRef<HTMLParagraphElement>(null);

  useEffect(() => {
    if (expanded) return;
    const el = textRef.current;
    if (el) setOverflowed(el.scrollHeight > el.clientHeight + 1);
  }, [expanded, card.text, card.note]);

  const toggleExpand = () => setExpanded((v) => !v);
  const [confirming, setConfirming] = useState(false);

  const toggleStar = async () => {
    await call('toggle_starred', { id: card.id, starred: !card.starred }).catch((e) => onToast(explainError(e)));
    onChanged();
  };
  const remove = () => setConfirming(true);
  const confirmRemove = async () => {
    setConfirming(false);
    await call('delete_card', { id: card.id }).catch((e) => onToast(explainError(e)));
    onChanged();
  };
  const isSelf = card.kind === 'self';
  // 确认文案与真实数据行为一致（R4）：自建卡物理删除；同步卡写隐藏墓碑，同步也不会恢复
  const confirmCopy = isSelf
    ? { title: '永久删除', message: '永久删除这张自建卡？此操作不可撤销。', label: '永久删除' }
    : { title: '隐藏卡片', message: '从滩涂拾遗中隐藏这张卡片？之后同步也不会恢复。', label: '隐藏卡片' };
  return (
    <>
    <article className={`card kind-${card.kind}${card.starred ? ' starred' : ''}${expanded ? ' expanded' : ''}`}>
      <button
        className={`card-star${card.starred ? ' starred' : ''}`}
        title="星标"
        aria-label="星标"
        aria-pressed={card.starred}
        onClick={toggleStar}
      ><Icon name="star" size={13} /></button>
      <div className="card-actions">
        <button title="编辑" aria-label="编辑" onClick={onEdit}><Icon name="pen" size={13} /></button>
        <button title={isSelf ? '删除' : '隐藏'} aria-label={isSelf ? '删除' : '隐藏'} onClick={remove}><Icon name="trash" size={13} /></button>
      </div>
      <div className="card-eyebrow">
        {card.tags.length
          ? card.tags.join(' · ')
          : isSelf ? '编者按' : card.kind === 'thought' ? '想法' : '划线'}
      </div>
      <p
        ref={textRef}
        className={`card-text${overflowed ? ' overflowed' : ''}`}
        role={overflowed ? 'button' : undefined}
        tabIndex={overflowed ? 0 : undefined}
        aria-expanded={overflowed ? expanded : undefined}
        title={overflowed && !expanded ? '展开全文' : undefined}
        onClick={overflowed ? toggleExpand : undefined}
        onKeyDown={overflowed ? (e) => {
          if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggleExpand(); }
        } : undefined}
      >{card.text}</p>
      {card.kind === 'thought' && card.abstractText && (
        <details className="abstract">
          <summary>原文</summary>
          <blockquote>{card.abstractText}</blockquote>
        </details>
      )}
      {card.note && <p className="card-note">{card.note}</p>}
      <div className="card-meta">
        <span className="card-source">
          {[card.bookTitle, card.chapterTitle].filter(Boolean).join(' / ') || '自建卡'}
        </span>
        {card.excludedFromReview && <span className="excluded-flag">已移出回顾</span>}
        <span className="card-date">{fmtDate(card.createdAt)}</span>
      </div>
    </article>
    {confirming && (
      <ConfirmModal
        title={confirmCopy.title}
        message={confirmCopy.message}
        confirmLabel={confirmCopy.label}
        onConfirm={confirmRemove}
        onCancel={() => setConfirming(false)}
      />
    )}
    </>
  );
}

function useDialog(onClose: () => void) {
  const ref = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    const modal = ref.current;
    if (!modal) return;
    const prev = document.activeElement as HTMLElement | null;
    const focusables = () =>
      Array.from(modal.querySelectorAll<HTMLElement>('button, input, textarea, [href], [tabindex]:not([tabindex="-1"])'))
        .filter((el) => !el.hasAttribute('disabled'));
    (focusables()[0] ?? modal).focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { e.preventDefault(); closeRef.current(); return; }
      if (e.key !== 'Tab') return;
      const list = focusables();
      if (!list.length) return;
      const first = list[0];
      const last = list[list.length - 1];
      if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
      else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
      prev?.focus();
    };
  }, []);
  return ref;
}

function ConfirmModal({ title, message, confirmLabel, onConfirm, onCancel }: {
  title: string;
  message: string;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const ref = useDialog(onCancel);
  return (
    <div className="modal-mask" onClick={onCancel}>
      <div className="modal modal-confirm" ref={ref} role="dialog" aria-modal="true" aria-labelledby="modal-confirm-title" onClick={(e) => e.stopPropagation()}>
        <h3 id="modal-confirm-title">{title}</h3>
        <p>{message}</p>
        <div className="modal-actions">
          <button className="ghost" onClick={onCancel}>取消</button>
          <button className="primary danger" onClick={onConfirm}>{confirmLabel}</button>
        </div>
      </div>
    </div>
  );
}

function EditModal({ card, onClose, onSaved, onPatched, onToast }: {
  card: CardRow;
  onClose: () => void;
  onSaved: (patch: { note: string; text: string }) => void;
  /** 局部字段（排除状态等）即时生效但不关弹层时，刷新外部列表 */
  onPatched?: (patch: { included: boolean }) => void;
  onToast: (m: string) => void;
}) {
  const ref = useDialog(onClose);
  const [note, setNote] = useState(card.note);
  const [text, setText] = useState(card.text);
  const [tagName, setTagName] = useState('');
  const [included, setIncluded] = useState(!card.excludedFromReview);
  const isSelf = card.kind === 'self';
  const save = async () => {
    if (isSelf && !text.trim()) {
      onToast('卡片正文不能为空。');
      return;
    }
    try {
      await call('update_card', {
        id: card.id,
        note,
        text: isSelf ? text : undefined,
      });
      onSaved({ note, text });
    } catch (e) {
      onToast(explainError(e));
    }
  };
  const addTag = async () => {
    if (!tagName.trim()) return;
    await call('add_tag', { cardId: card.id, name: tagName.trim() }).catch((e) => onToast(explainError(e)));
    setTagName('');
    onSaved({ note, text });
  };
  const removeTag = async (t: string) => {
    await call('remove_tag', { cardId: card.id, name: t }).catch((e) => onToast(explainError(e)));
    onSaved({ note, text });
  };
  // 纳入/移出回顾：即时生效不关弹层。恢复后立即进入待回顾状态由后端保证（R2）
  const toggleIncluded = async (next: boolean) => {
    const prev = included;
    setIncluded(next);
    try {
      await call('set_excluded_from_review', { id: card.id, excluded: !next });
      onPatched?.({ included: next });
    } catch (e) {
      setIncluded(prev);
      onToast(explainError(e));
    }
  };
  return (
    <div className="modal-mask" onClick={onClose}>
      <div className="modal" ref={ref} role="dialog" aria-modal="true" aria-labelledby="modal-edit-title" onClick={(e) => e.stopPropagation()}>
        <h3 id="modal-edit-title">{isSelf ? '编辑自建卡' : '补写想法'}</h3>
        {isSelf ? (
          <textarea rows={6} value={text} onChange={(e) => setText(e.target.value)} placeholder="正文" aria-label="正文" />
        ) : (
          <blockquote className="quote-box">{card.text}</blockquote>
        )}
        {!isSelf && (
          <textarea rows={5} value={note} onChange={(e) => setNote(e.target.value)} placeholder="写下你的想法…" aria-label="想法" autoFocus />
        )}
        <label className="switch-row" title="关闭后此卡不再进入回顾队列，仍保留在卡片墙与搜索中">
          <input
            type="checkbox"
            checked={included}
            onChange={(e) => toggleIncluded(e.target.checked)}
          />
          <span className="switch-label">纳入翻牌回顾</span>
        </label>
        <p className="hint switch-hint">关闭后此卡不再进入回顾队列，仍保留在卡片墙与搜索中。</p>
        <div className="tag-editor">
          {card.tags.map((t) => (
            <button key={t} className="chip small deletable" onClick={() => removeTag(t)} aria-label={'移除标签 ' + t} title="点击移除">{t} ×</button>
          ))}
          <input
            value={tagName}
            onChange={(e) => setTagName(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && addTag()}
            placeholder="新标签，回车添加"
            aria-label="新标签"
          />
          <button onClick={addTag}>加标签</button>
        </div>
        <div className="modal-actions">
          <button className="ghost" onClick={onClose}>取消</button>
          <button className="primary" onClick={save} disabled={isSelf && !text.trim()}>保存</button>
        </div>
      </div>
    </div>
  );
}

function CreateModal({ onClose, onSaved, onToast }: {
  onClose: () => void;
  onSaved: () => void;
  onToast: (m: string) => void;
}) {
  const [text, setText] = useState('');
  const [tagsInput, setTagsInput] = useState('');
  const [emptyHint, setEmptyHint] = useState(false);
  const ref = useDialog(onClose);
  const save = async () => {
    if (!text.trim()) {
      setEmptyHint(true);
      return;
    }
    try {
      const tagNames = tagsInput.split(/[,，\s]+/).filter(Boolean);
      await call('create_card', { text: text.trim(), tagNames });
      onSaved();
    } catch (e) {
      onToast(explainError(e));
    }
  };
  return (
    <div className="modal-mask" onClick={onClose}>
      <div className="modal" ref={ref} role="dialog" aria-modal="true" aria-labelledby="modal-create-title" onClick={(e) => e.stopPropagation()}>
        <h3 id="modal-create-title">新建卡片</h3>
        <textarea
          rows={7}
          value={text}
          onChange={(e) => { setText(e.target.value); if (e.target.value.trim()) setEmptyHint(false); }}
          placeholder="记录一个想法…"
          aria-label="卡片正文"
          aria-invalid={emptyHint || undefined}
          autoFocus
        />
        {emptyHint && <p className="err field-hint">先写一句再保存。</p>}
        <input value={tagsInput} onChange={(e) => setTagsInput(e.target.value)} placeholder="标签（逗号分隔，可空）" aria-label="标签" />
        <div className="modal-actions">
          <button className="ghost" onClick={onClose}>取消</button>
          <button className="primary" onClick={save} disabled={!text.trim()}>保存</button>
        </div>
      </div>
    </div>
  );
}

// 具名导出供前端交互测试直接渲染（PRD 12.1）
// book 非空 = 本书清样：队列、剩余数、文案都只围绕这一本书。
export function ReviewView({ book = null, onToast, onExit, hasKey, hasBooks }: {
  book?: { id: number; title: string } | null;
  onToast: (m: string) => void;
  onExit: () => void;
  hasKey: boolean;
  hasBooks: boolean;
}) {
  const bookId = book?.id ?? null;
  const scopeLine = book ? <p className="review-scope mono">本书 · {book.title}</p> : null;
  const [phase, setPhase] = useState<'entry' | 'review' | 'settling'>('entry');
  const [entryReady, setEntryReady] = useState(false);
  const [dueTotal, setDueTotal] = useState(0);
  const [batchSize, setBatchSize] = useState(20);
  const [queue, setQueue] = useState<CardRow[]>([]);
  const [idx, setIdx] = useState(0);
  const [flipped, setFlipped] = useState(false);
  const [grading, setGrading] = useState(false);
  const [flying, setFlying] = useState(false);
  const [dueNote, setDueNote] = useState<string | null>(null);
  const [todayLeft, setTodayLeft] = useState(0);
  const [excludeTarget, setExcludeTarget] = useState<CardRow | null>(null);
  const [editing, setEditing] = useState<CardRow | null>(null);
  const [settling, setSettling] = useState<{ remaining: number; rated: Record<ReviewRating, number>; excluded: number } | null>(null);
  const gradingRef = useRef(false);
  const flyingRef = useRef(false);
  const flippedRef = useRef(false);
  flippedRef.current = flipped;
  const ratedRef = useRef<Record<ReviewRating, number>>({ again: 0, hard: 0, good: 0, easy: 0 });
  const excludedRef = useRef(0);

  // 进入页：今日到期总数 + 批次设置（R3.1/R3.2）；本书清样时按书取数
  useEffect(() => {
    let alive = true;
    Promise.all([
      call<number>('get_due_count', { bookId }).catch(() => 0),
      call<ReviewSettings>('get_review_settings').catch(() => ({ batchSize: 20 })),
    ]).then(([due, settings]) => {
      if (!alive) return;
      setDueTotal(due);
      setBatchSize(settings.batchSize);
      setEntryReady(true);
    });
    return () => { alive = false; };
  }, [bookId]);

  const startBatch = async () => {
    // 取队列前重查真实到期数，作为「今日总剩余」的起点（R3.2）
    const due = await call<number>('get_due_count', { bookId }).catch(() => dueTotal);
    const cards = await call<CardRow[]>('get_due_cards', { limit: batchSize, bookId }).catch((e) => {
      onToast(explainError(e));
      return null;
    });
    if (!cards) return;
    if (!cards.length) {
      // 到期数在打开期间被清空（如另一次回顾）：回到进入页刷新
      setDueTotal(due);
      setEntryReady(true);
      return;
    }
    ratedRef.current = { again: 0, hard: 0, good: 0, easy: 0 };
    excludedRef.current = 0;
    setQueue(cards);
    setIdx(0);
    setFlipped(false);
    setDueTotal(due);
    setTodayLeft(due);
    setPhase('review');
  };

  const refreshDueTotal = async (): Promise<number> => {
    try {
      const n = await call<number>('get_due_count', { bookId });
      setDueTotal(n);
      return n;
    } catch {
      return todayLeft;
    }
  };

  // 结算：重查真实到期数，分支「今天翻完了 / 本批完成，还剩 N」（R3.3）
  const settle = async () => {
    let remaining = todayLeft;
    try {
      remaining = await call<number>('get_due_count', { bookId });
      setDueTotal(remaining);
    } catch { /* 用本地估算兜底 */ }
    setSettling({ remaining, rated: { ...ratedRef.current }, excluded: excludedRef.current });
    setPhase('settling');
  };

  const flyOutThen = (after: () => void) => {
    flyingRef.current = true;
    setFlying(true);
    window.setTimeout(() => {
      flyingRef.current = false;
      setFlying(false);
      setFlipped(false);
      setDueNote(null);
      after();
    }, 240);
  };

  // 四档评分（R1）：只接受第一次请求；失败停留当前卡、进度不递增、可重试
  const rate = async (rating: ReviewRating) => {
    const card = queue[idx];
    if (!card || gradingRef.current || flyingRef.current || !flippedRef.current) return;
    gradingRef.current = true;
    setGrading(true);
    try {
      const next: SrsState = await call('grade_review', { cardId: card.id, rating });
      ratedRef.current[rating] += 1;
      setTodayLeft((t) => Math.max(0, t - 1));
      setDueNote(`下次回顾 · ${humanizeDue(next.due_at - nowSecs())}`);
      gradingRef.current = false;
      setGrading(false);
      flyOutThen(() => {
        const nextIdx = idx + 1;
        setIdx(nextIdx);
        if (nextIdx >= queue.length) void settle();
      });
    } catch (e) {
      gradingRef.current = false;
      setGrading(false);
      onToast(explainError(e));
    }
  };

  const doExclude = async (card: CardRow) => {
    if (flyingRef.current || gradingRef.current) return;
    flyingRef.current = true;
    try {
      await call('set_excluded_from_review', { id: card.id, excluded: true });
      try { localStorage.setItem(EXCLUDE_HINT_KEY, '1'); } catch { /* 本地存储不可用时忽略 */ }
      excludedRef.current += 1;
      setTodayLeft((t) => Math.max(0, t - 1));
      onToast('已移出回顾 · 卡片仍保留在卡片墙与搜索中');
      flyingRef.current = false;
      flyOutThen(() => {
        const nextIdx = idx + 1;
        setIdx(nextIdx);
        if (nextIdx >= queue.length) void settle();
      });
    } catch (e) {
      flyingRef.current = false;
      onToast(explainError(e));
    }
  };

  // 回顾正面「移出回顾」次级动作（R2）：首次给说明，之后直接执行
  const requestExclude = () => {
    const card = queue[idx];
    if (!card || gradingRef.current || flyingRef.current) return;
    let seen = false;
    try { seen = !!localStorage.getItem(EXCLUDE_HINT_KEY); } catch { /* 忽略 */ }
    if (seen) void doExclude(card);
    else setExcludeTarget(card);
  };

  const unflip = () => {
    if (flyingRef.current || gradingRef.current || !flippedRef.current) return;
    setFlipped(false);
  };

  // 空格/Enter 只负责翻面，不再自动提交 Good（R1.2）
  const flip = () => {
    if (flyingRef.current || gradingRef.current || flippedRef.current) return;
    setFlipped(true);
  };

  const continueNextBatch = async () => {
    const remaining = await refreshDueTotal();
    if (remaining <= 0) {
      setSettling(null);
      setPhase('entry');
      setEntryReady(true);
      return;
    }
    const cards = await call<CardRow[]>('get_due_cards', { limit: batchSize, bookId }).catch((e) => {
      onToast(explainError(e));
      return null;
    });
    if (!cards?.length) {
      setSettling(null);
      setPhase('entry');
      setEntryReady(true);
      return;
    }
    ratedRef.current = { again: 0, hard: 0, good: 0, easy: 0 };
    excludedRef.current = 0;
    setSettling(null);
    setQueue(cards);
    setIdx(0);
    setFlipped(false);
    setTodayLeft(remaining);
    setPhase('review');
  };

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      const typing = t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable);
      if (e.key === 'Escape') {
        if (!typing && !editing && !excludeTarget) { e.preventDefault(); onExit(); }
        return;
      }
      if (typing || editing || excludeTarget) return;
      const onButton = t?.tagName === 'BUTTON';
      if (phase === 'entry') {
        if (!onButton && e.key === 'Enter' && entryReady && dueTotal > 0) { e.preventDefault(); void startBatch(); }
        return;
      }
      if (phase === 'settling') {
        if (!onButton && e.key === 'Enter' && settling && settling.remaining > 0) { e.preventDefault(); void continueNextBatch(); }
        return;
      }
      if ((e.key === '1' || e.key === '2' || e.key === '3' || e.key === '4') && flippedRef.current) {
        e.preventDefault();
        const def = RATING_DEFS[Number(e.key) - 1];
        void rate(def.key);
        return;
      }
      if (onButton) return; // 按钮焦点上让 Space/Enter 走默认点击，避免双触发
      if (e.key === 'z' || e.key === 'Z' || e.key === 'Backspace') {
        e.preventDefault();
        unflip();
        return;
      }
      if (e.key === ' ' || e.key === 'Enter') {
        e.preventDefault();
        flip();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [phase, idx, queue, flipped, grading, flying, editing, excludeTarget, entryReady, dueTotal, settling, onExit]);

  // ---------- 进入页 ----------
  if (phase === 'entry') {
    const batchNow = Math.min(batchSize, Math.max(dueTotal, 0));
    return (
      <div className="review">
        <h2 className="review-title">清样 · 翻牌回顾</h2>
        {scopeLine}
        {!entryReady ? (
          <p className="hint">载入队列…</p>
        ) : dueTotal <= 0 ? (
          <div className="deck-done">
            <p className="review-text">{book ? '这本书当前没有到期卡片' : '当前没有到期卡片'}</p>
            <p className="review-hint">
              {!hasKey
                ? '先到设置填写 API Key，再同步。到期的卡片会排进这副牌。'
                : !hasBooks
                  ? '同步之后，新卡片会自动进入队列。'
                  : book
                    ? '其他书的到期卡片不受影响。新划线会自动进入这本书的队列。'
                    : '新卡片会自动进入队列。间隔重复讲究少而勤。'}
            </p>
            <button className="primary" onClick={onExit}>返回卡片墙</button>
          </div>
        ) : (
          <div className="deck-done">
            <p className="review-text">{book ? '本书' : '今日'}到期 {dueTotal} 张 · 本批 {batchNow} 张</p>
            <p className="review-hint">
              {book ? '只翻这一本书的到期卡片。' : ''}
              每批 {batchSize} 张，完成一批再看今天还剩多少。设置里可改为 10 / 20 / 30。
            </p>
            <div className="deck-done-actions">
              <button className="primary" onClick={() => void startBatch()}>开始翻牌（Enter）</button>
              <button className="ghost" onClick={onExit}>返回卡片墙</button>
            </div>
          </div>
        )}
      </div>
    );
  }

  // ---------- 结算页 ----------
  if (phase === 'settling' && settling) {
    const processed = settling.rated.again + settling.rated.hard + settling.rated.good + settling.rated.easy + settling.excluded;
    const dist = `忘了 ${settling.rated.again} · 困难 ${settling.rated.hard} · 记得 ${settling.rated.good} · 简单 ${settling.rated.easy}` +
      (settling.excluded > 0 ? ` · 移出 ${settling.excluded}` : '');
    const allDone = settling.remaining <= 0;
    return (
      <div className="review">
        <h2 className="review-title">清样 · 翻牌回顾</h2>
        {scopeLine}
        <div className="deck-done">
          <p className="review-text">{allDone ? '今天翻完了' : `本批完成，今天还剩 ${settling.remaining} 张`}</p>
          <p className="review-hint">本批处理 {processed} 张（{dist}）</p>
          <div className="deck-done-actions">
            {allDone ? (
              <button className="primary" onClick={onExit}>返回卡片墙</button>
            ) : (
              <>
                <button className="primary" onClick={() => void continueNextBatch()}>继续下一批（Enter）</button>
                <button className="ghost" onClick={onExit}>返回卡片墙</button>
              </>
            )}
          </div>
        </div>
      </div>
    );
  }

  const card = queue[idx];
  if (!card) {
    // 竞态兜底：队列被清空时回到进入页
    return (
      <div className="review">
        <h2 className="review-title">清样 · 翻牌回顾</h2>
        <div className="deck-done">
          <p className="review-text">当前没有到期卡片</p>
          <button className="primary" onClick={onExit}>返回卡片墙</button>
        </div>
      </div>
    );
  }
  const under = [queue[idx + 1], queue[idx + 2]];
  return (
    <div className="review">
      <h2 className="review-title">清样 · 翻牌回顾</h2>
      {scopeLine}
      <div className="review-top">
        <div className="review-remaining">
          <span className="hint-mono">本批剩余 · 今日剩余</span>
          <span className="review-count">{queue.length - idx} 张 · {todayLeft} 张</span>
        </div>
        <button className="ghost" onClick={onExit}>退出（Esc）</button>
      </div>
      <div className="review-ticks" aria-hidden="true">
        {queue.map((_, i) => <i key={i} className={i < idx ? 'done' : i === idx ? 'now' : ''} />)}
      </div>
      <div
        className="deck-stage"
        role="button"
        tabIndex={0}
        aria-label={flipped ? '翻过这张并用 1–4 评分' : '翻面阅读'}
        onClick={flip}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); flip(); }
        }}
      >
        {under.map((c, k) => c && (
          <div key={c.id} className={`deck-under u${k + 1}`} aria-hidden="true">
            <span className="deck-back-frame" />
          </div>
        ))}
        <div className={`deck-card${flying ? ' flying' : ''}`}>
          <div className={`deck-flipper${flipped ? ' flipped' : ''}`}>
            <div className="deck-face back">
              <span className="deck-back-frame" />
              <div className="deck-back-inner">
                <span className="deck-back-label">翻牌</span>
                <span className="deck-back-num mono">{String(idx + 1).padStart(2, '0')}<span> / {queue.length}</span></span>
              </div>
              <span className="deck-back-hint"><Icon name="refresh" size={12} />点击或空格翻面</span>
            </div>
            <div className="deck-face front">
              <span className="card-source">{[card.bookTitle, card.chapterTitle].filter(Boolean).join(' / ') || '自建卡'}</span>
              <div className="deck-front-inner">
                <p className="review-text">{card.text}</p>
                {card.abstractText && <blockquote>{card.abstractText}</blockquote>}
                {card.note && <p className="card-note">{card.note}</p>}
              </div>
              {dueNote && <span className="deck-due-note">{dueNote}</span>}
              <span className="deck-next-hint">按你此刻的熟悉程度选择</span>
            </div>
          </div>
        </div>
      </div>
      {flipped && (
        <div className="review-rating" role="group" aria-label="记忆评分">
          {RATING_DEFS.map((d) => (
            <button
              key={d.key}
              className={`rate-btn rate-${d.key}`}
              disabled={grading || flying}
              onClick={(e) => { e.stopPropagation(); void rate(d.key); }}
            >
              <kbd>{d.num}</kbd>{d.label}
            </button>
          ))}
        </div>
      )}
      {flipped && (
        <div className="review-tools">
          <button
            className="ghost"
            disabled={grading || flying}
            onClick={(e) => { e.stopPropagation(); requestExclude(); }}
          >
            移出回顾
          </button>
          <button
            className="ghost"
            onClick={(e) => { e.stopPropagation(); setEditing(card); }}
          >
            记下想法
          </button>
        </div>
      )}
      {excludeTarget && (
        <ConfirmModal
          title="移出回顾"
          message={`把「${excludeTarget.text.slice(0, 24)}${excludeTarget.text.length > 24 ? '…' : ''}」移出回顾？它仍会保留在卡片墙与搜索中，只是不再进入到期队列；之后可在编辑弹层重新纳入。`}
          confirmLabel="移出回顾"
          onConfirm={() => {
            const c = excludeTarget;
            setExcludeTarget(null);
            if (c) void doExclude(c);
          }}
          onCancel={() => setExcludeTarget(null)}
        />
      )}
      {editing && (
        <EditModal
          card={editing}
          onClose={() => setEditing(null)}
          onSaved={(patch) => {
            setQueue((qs) => qs.map((c) => (c.id === editing.id ? { ...c, note: patch.note, text: patch.text } : c)));
            setEditing(null);
          }}
          onPatched={(p) => {
            setQueue((qs) => qs.map((c) => (c.id === editing.id ? { ...c, excludedFromReview: !p.included } : c)));
          }}
          onToast={onToast}
        />
      )}
    </div>
  );
}

export function MindmapView({
  book,
  cards,
  onToast,
  onExit,
  onOpenCard,
  onNeedSettings,
  activeTask,
  onStartGenerate,
}: {
  book: { id: number; title: string };
  cards: CardRow[];
  onToast: (m: string) => void;
  onExit: () => void;
  onOpenCard: (card: CardRow) => void;
  onNeedSettings: () => void;
  activeTask?: ClueTask;
  onStartGenerate?: (book: { id: number; title: string }) => Promise<void>;
}) {
  const [status, setStatus] = useState<MindmapStatus | null>(null);
  const [map, setMap] = useState<Mindmap | null>(null);
  const [localBusy, setLocalBusy] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const [localProgress, setLocalProgress] = useState<string | null>(null);
  const [localProgressFrac, setLocalProgressFrac] = useState<{ current: number; total: number } | null>(null);
  const [localProgressFails, setLocalProgressFails] = useState<string[]>([]);
  const [localElapsed, setLocalElapsed] = useState(0);
  const [openId, setOpenId] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [confirmRegenerating, setConfirmRegenerating] = useState(false);
  const [allBookCards, setAllBookCards] = useState<CardRow[]>(cards);

  const busy = activeTask ? activeTask.busy : localBusy;
  const progress = activeTask ? activeTask.progress : localProgress;
  const progressFrac = activeTask ? activeTask.progressFrac : localProgressFrac;
  const progressFails = activeTask ? activeTask.progressFails : localProgressFails;
  const elapsed = activeTask ? activeTask.elapsed : localElapsed;
  const error = activeTask?.error ?? localError;

  useEffect(() => {
    if (activeTask?.result) {
      setMap(activeTask.result);
      setStatus((s) => (s ? { ...s, cached: activeTask.result, stale: false } : s));
    }
  }, [activeTask?.result]);

  const loadBookCards = useCallback(async () => {
    try {
      const rows = await call<CardRow[]>('query_cards', {
        filter: { ...emptyFilter(), bookId: book.id },
        limit: 10000,
        offset: 0,
      });
      if (rows && rows.length > 0) {
        setAllBookCards(rows);
      }
    } catch {
      // 保持使用传入的 cards 作为降级
    }
  }, [book.id]);

  useEffect(() => {
    loadBookCards();
  }, [loadBookCards]);

  useEffect(() => {
    if (cards && cards.length > 0) {
      setAllBookCards((prev) => {
        const m = new Map(prev.map((c) => [c.id, c]));
        for (const c of cards) m.set(c.id, c);
        return Array.from(m.values());
      });
    }
  }, [cards]);

  const byId = useMemo(() => {
    const m = new Map<number, CardRow>();
    for (const c of cards) m.set(c.id, c);
    for (const c of allBookCards) m.set(c.id, c);
    return m;
  }, [cards, allBookCards]);

  useEffect(() => {
    let alive = true;
    call<MindmapStatus>('get_mindmap_status', { bookId: book.id })
      .then((s) => {
        if (!alive) return;
        setStatus(s);
        if (s.cached) setMap(s.cached);
      })
      .catch((e) => onToast(explainError(e)));
    return () => { alive = false; };
  }, [book.id, onToast]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      const typing = t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable);
      if (typing) return;
      if (e.key !== 'Escape') return;
      e.preventDefault();
      if (openId) {
        setOpenId(null);
      } else if (expandedId) {
        setExpandedId(null);
      } else {
        onExit();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [openId, expandedId, onExit]);

  const generate = async () => {
    if (busy) return;
    if ((status?.cardCount ?? 0) < 2) {
      setLocalError('至少两张卡片才能归纳。先同步这本书，或再划两条。');
      return;
    }
    if (onStartGenerate) {
      setLocalError(null);
      void onStartGenerate(book);
      return;
    }
    setLocalBusy(true);
    setLocalError(null);
    setLocalProgress('整理卡片…');
    setLocalProgressFrac(null);
    setLocalProgressFails([]);
    setLocalElapsed(0);
    const started = Date.now();
    const tick = window.setInterval(() => {
      setLocalElapsed(Math.floor((Date.now() - started) / 1000));
    }, 1000);
    try {
      const chan = new Channel<MindmapEventPayload>();
      chan.onmessage = (ev) => {
        const line = formatClueProgress(ev);
        if (line) setLocalProgress(line);
        if (ev.total > 0) setLocalProgressFrac({ current: ev.current, total: ev.total });
        if (ev.stage === 'chapter_failed') {
          setLocalProgressFails((prev) => [...prev, formatClueProgress(ev)]);
        }
      };
      const next = await call<Mindmap>('generate_mindmap', { bookId: book.id, onProgress: chan });
      setMap(next);
      setStatus((s) => s ? { ...s, cached: next, stale: false } : s);
      setLocalProgress(null);
      setLocalProgressFrac(null);
      setLocalProgressFails([]);
    } catch (e) {
      const msg = explainError(e);
      setLocalError(msg);
      setLocalProgress(null);
      setLocalProgressFrac(null);
      onToast(msg.split('\n')[0] || msg);
    } finally {
      window.clearInterval(tick);
      setLocalBusy(false);
      setLocalElapsed(0);
    }
  };

  const handleCopyOutline = async () => {
    if (!map) return;
    try {
      const text = exportClueOutline(map);
      await navigator.clipboard.writeText(text);
      onToast('已复制线索大纲到剪贴板');
    } catch {
      onToast('复制大纲失败，请重试');
    }
  };

  const openNode = map && openId
    ? findTheme(map.root, openId)
    : null;
  const evidence = openNode
    ? openNode.sourceCardIds.map((id) => byId.get(id)).filter((c): c is CardRow => !!c)
    : [];

  return (
    <div className="mindmap">
      <h2 className="review-title">线索 · {book.title}</h2>
      <p className="review-scope mono">概要节点 · 点开才看划线</p>
      <div className="review-top">
        <div className="review-remaining">
          <span className="hint-mono">卡片 · 主题</span>
          <span className="review-count">
            {status ? `${status.cardCount} 张` : '…'}
            {map ? ` · ${map.stats.themes} 题` : ''}
          </span>
        </div>
        <div className="mm-head-actions">
          {map && (
            <button
              type="button"
              className="ghost"
              onClick={handleCopyOutline}
              title="复制为 Markdown 树状大纲"
            >
              复制大纲
            </button>
          )}
          <button className="ghost" onClick={onExit}>返回（Esc）</button>
        </div>
      </div>
      {!status && <p className="hint">载入线索…</p>}
      {status?.providerOff && !map && (
        <div className="deck-done">
          <p className="review-text">还没有启用语言模型</p>
          <p className="review-hint">到设置「四、语言模型」选择供应商后，才能把划线归纳成主题线索。</p>
          <div className="deck-done-actions">
            <button className="primary" onClick={onNeedSettings}>去设置</button>
            <button className="ghost" onClick={onExit}>返回卡片墙</button>
          </div>
        </div>
      )}
      {status && !status.providerOff && !map && (
        <div className="deck-done">
          <p className="review-text">把这本书的划线收成一张图</p>
          <p className="review-hint">
            将发送本书 {status.cardCount} 张卡片的标题级文本，不会上传整库。画面上只有概要，点节点才看原文。
          </p>
          {status.chatEndpoint && (
            <p className="hint-mono">请求 {status.model ?? '模型'} · POST {status.chatEndpoint}</p>
          )}
          <ClueProgress
            busy={busy}
            line={progress}
            frac={progressFrac}
            elapsed={elapsed}
            fails={progressFails}
          />
          {error && <p className="err">{error}</p>}
          <div className="deck-done-actions">
            <button className="primary" onClick={() => void generate()} disabled={busy}>
              {busy ? '归纳中…' : '生成线索'}
            </button>
            <button className="ghost" onClick={onExit}>返回卡片墙</button>
          </div>
        </div>
      )}
      {error && map && <p className="err">{error}</p>}
      {map && (
        <>
          <p className="hint">
            {status?.stale ? '划线已变，线索还是上次的。' : ''}
            <button
              className="link-btn"
              onClick={() => {
                if (status?.stale) {
                  void generate();
                } else {
                  setConfirmRegenerating(true);
                }
              }}
              disabled={busy}
            >
              {busy ? '归纳中…' : (status?.stale ? '更新' : '重新生成')}
            </button>
          </p>
          <ClueProgress
            busy={busy}
            line={progress}
            frac={progressFrac}
            elapsed={elapsed}
            fails={progressFails}
          />
          <div className="mm-body">
            <MindmapCanvas
              mapRoot={map.root}
              openId={openId}
              onOpen={setOpenId}
              expandedId={expandedId}
              onToggleExpand={(id) => setExpandedId((cur) => cur === id ? null : id)}
            />
            {openNode && (
              <aside
                className="mm-side-drawer mm-drawer"
                role="dialog"
                aria-modal="false"
                aria-labelledby="mm-drawer-title"
                onPointerDown={(e) => e.stopPropagation()}
              >
                <div className="mm-drawer-header">
                  <div className="mm-drawer-top">
                    <h3 id="mm-drawer-title">{openNode.label}</h3>
                    <button
                      type="button"
                      className="mm-drawer-close"
                      onClick={() => setOpenId(null)}
                      title="收起证据抽屉（Esc）"
                    >
                      关闭
                    </button>
                  </div>
                  {openNode.summary && <p className="hint">{openNode.summary}</p>}
                  <p className="hint-mono">证据 {evidence.length} 张</p>
                </div>
                <div className="mm-evidence">
                  {evidence.map((c) => (
                    <button
                      key={c.id}
                      type="button"
                      className="mm-ev-item"
                      onClick={() => onOpenCard(c)}
                    >
                      <div className="mm-ev-head">
                        <span className="card-eyebrow">
                          {c.starred ? '★ ' : ''}
                          {c.kind === 'thought' ? '想法' : c.kind === 'self' ? '编者按' : '划线'}
                          {c.chapterTitle ? ` · ${c.chapterTitle}` : ''}
                        </span>
                        {c.tags && c.tags.length > 0 && (
                          <span className="mm-ev-tags">
                            {c.tags.map((t) => <span key={t} className="tag">#{t}</span>)}
                          </span>
                        )}
                      </div>
                      <span className="mm-ev-text">{c.text}</span>
                      {c.kind === 'thought' && c.abstractText && (
                        <blockquote className="mm-ev-abstract">
                          {c.abstractText}
                        </blockquote>
                      )}
                      {c.note && <p className="card-note mm-ev-note">{c.note}</p>}
                    </button>
                  ))}
                  {evidence.length === 0 && (
                    <p className="hint">这些卡不在当前墙里，仍可在全书搜索中找到。</p>
                  )}
                </div>
                <div className="mm-drawer-actions">
                  <button
                    type="button"
                    className="ghost"
                    onClick={() => {
                      const text = `# ${openNode.label}${openNode.summary ? `\n> ${openNode.summary}` : ''}\n\n` +
                        evidence.map((c, i) => `${i + 1}. ${c.text}${c.note ? `\n   注：${c.note}` : ''}`).join('\n');
                      navigator.clipboard.writeText(text).then(() => onToast('已复制该题证据')).catch(() => {});
                    }}
                    title="复制此主题及全部证据"
                  >
                    复制本题
                  </button>
                  <button type="button" className="ghost" onClick={() => setOpenId(null)}>
                    关闭
                  </button>
                </div>
              </aside>
            )}
          </div>
          {map.warnings.length > 0 && (
            <p className="hint">{map.warnings[map.warnings.length - 1]}</p>
          )}
        </>
      )}
      {confirmRegenerating && (
        <ConfirmModal
          title="重新归纳线索"
          message="确定要重新归纳吗？当前已生成的线索结构将被新结果覆盖。"
          confirmLabel="确认重新生成"
          onConfirm={() => {
            setConfirmRegenerating(false);
            void generate();
          }}
          onCancel={() => setConfirmRegenerating(false)}
        />
      )}
    </div>
  );
}

function ClueProgress({ busy, line, frac, elapsed, fails }: {
  busy: boolean;
  line: string | null;
  frac: { current: number; total: number } | null;
  elapsed: number;
  fails: string[];
}) {
  const elapsedText = formatClueElapsed(elapsed);
  if (!busy && !line && fails.length === 0) return null;
  const total = frac?.total ?? 0;
  const current = frac?.current ?? 0;
  const pct = total > 0 ? Math.min(100, Math.round((current / total) * 100)) : 0;
  return (
    <div className="mm-progress">
      {line && (
        <p className="mm-progress-line" role="status" aria-live="polite" aria-busy={busy}>
          {line}
        </p>
      )}
      {elapsedText && <p className="mm-progress-elapsed">{elapsedText}</p>}
      {total > 1 && (
        <div className="mm-progress-meter">
          <div className="mm-progress-bar" aria-hidden="true">
            <i style={{ width: `${pct}%` }} />
          </div>
          <span className="mm-progress-frac mono">{current} / {total}</span>
        </div>
      )}
      {fails.length > 0 && (
        <ul className="mm-fails">
          {fails.map((f, i) => <li key={`${i}-${f}`}>{f}</li>)}
        </ul>
      )}
    </div>
  );
}

function panToShow(
  pan: { x: number; y: number },
  rect: { minX: number; minY: number; maxX: number; maxY: number },
  viewW: number,
  viewH: number,
  pad = 32,
): { x: number; y: number } {
  let { x, y } = pan;
  const w = rect.maxX - rect.minX;
  const h = rect.maxY - rect.minY;
  if (w > viewW - pad * 2) x = viewW / 2 - (rect.minX + rect.maxX) / 2;
  else {
    const l = rect.minX + x;
    const r = rect.maxX + x;
    if (l < pad) x += pad - l;
    if (r > viewW - pad) x -= r - (viewW - pad);
  }
  if (h > viewH - pad * 2) y = viewH / 2 - (rect.minY + rect.maxY) / 2;
  else {
    const t = rect.minY + y;
    const b = rect.maxY + y;
    if (t < pad) y += pad - t;
    if (b > viewH - pad) y -= b - (viewH - pad);
  }
  return { x, y };
}

function MindmapCanvas({
  mapRoot,
  openId,
  onOpen,
  expandedId,
  onToggleExpand,
}: {
  mapRoot: MindmapNode;
  openId: string | null;
  onOpen: (id: string) => void;
  expandedId: string | null;
  onToggleExpand: (id: string) => void;
}) {
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [zoom, setZoom] = useState(1);
  const [dragging, setDragging] = useState(false);
  const stageRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ px: number; py: number; panX: number; panY: number; moved: boolean } | null>(null);
  const centered = useRef(false);
  const vis = useMemo(() => visibleClue(mapRoot, expandedId), [mapRoot, expandedId]);
  const childCount = useMemo(() => {
    const m = new Map<string, number>();
    for (const c of mapRoot.children) m.set(c.id, c.children.length);
    return m;
  }, [mapRoot]);
  const expandIds = useMemo(() => {
    const s = new Set<string>();
    for (const [id, n] of childCount) if (n > 0) s.add(id);
    return s;
  }, [childCount]);
  const laid = useMemo(() => layoutMindmap(vis, expandIds), [vis, expandIds]);

  const resetView = useCallback((targetZoom = 1) => {
    const stage = stageRef.current;
    if (!stage) return;
    const sw = stage.clientWidth;
    const sh = stage.clientHeight;
    const hub = laid.nodes.find((n) => n.depth === 0);
    if (!hub) return;
    setZoom(targetZoom);
    setPan({
      x: sw / 2 - (hub.x + hub.w / 2) * targetZoom,
      y: sh / 2 - (hub.y + hub.h / 2) * targetZoom,
    });
  }, [laid]);

  useEffect(() => {
    centered.current = false;
    setPan({ x: 0, y: 0 });
    setZoom(1);
  }, [mapRoot]);

  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;
    const sw = stage.clientWidth;
    const sh = stage.clientHeight;
    if (sw < 8 || sh < 8) return;
    const hub = laid.nodes.find((n) => n.depth === 0);
    if (!hub) return;
    if (!expandedId) {
      if (centered.current) {
        setPan({
          x: sw / 2 - (hub.x + hub.w / 2) * zoom,
          y: sh / 2 - (hub.y + hub.h / 2) * zoom,
        });
        return;
      }
      setPan({
        x: sw / 2 - (hub.x + hub.w / 2) * zoom,
        y: sh / 2 - (hub.y + hub.h / 2) * zoom,
      });
      centered.current = true;
      return;
    }
    const follow = laid.nodes.filter((n) => n.depth === 2 || n.id === expandedId);
    if (!follow.length) return;
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    for (const n of follow) {
      minX = Math.min(minX, n.x);
      minY = Math.min(minY, n.y);
      maxX = Math.max(maxX, n.x + n.w);
      maxY = Math.max(maxY, n.y + n.h);
    }
    setPan((cur) => panToShow(cur, { minX: minX * zoom, minY: minY * zoom, maxX: maxX * zoom, maxY: maxY * zoom }, sw, sh));
  }, [laid, expandedId, zoom]);

  const changeZoom = useCallback((getNextZoom: (cur: number) => number) => {
    const stage = stageRef.current;
    if (!stage) return;
    const sw = stage.clientWidth;
    const sh = stage.clientHeight;
    const cx = sw / 2;
    const cy = sh / 2;
    setZoom((curZoom) => {
      const nextZoom = Math.min(2.0, Math.max(0.4, Number(getNextZoom(curZoom).toFixed(2))));
      if (nextZoom === curZoom) return curZoom;
      setPan((curPan) => ({
        x: cx - (cx - curPan.x) * (nextZoom / curZoom),
        y: cy - (cy - curPan.y) * (nextZoom / curZoom),
      }));
      return nextZoom;
    });
  }, []);

  useEffect(() => {
    const el = stageRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      if (e.ctrlKey || e.metaKey) {
        const factor = e.deltaY < 0 ? 1.08 : 0.92;
        changeZoom((cur) => cur * factor);
      } else {
        setPan((p) => ({ x: p.x - e.deltaX, y: p.y - e.deltaY }));
      }
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [changeZoom]);

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    const t = e.target as HTMLElement;
    if (t.closest('button') || t.closest('.mm-controls') || t.closest('.mm-side-drawer')) {
      return;
    }
    drag.current = { px: e.clientX, py: e.clientY, panX: pan.x, panY: pan.y, moved: false };
    e.currentTarget.setPointerCapture(e.pointerId);
    setDragging(true);
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    setPan({
      x: drag.current.panX + (e.clientX - drag.current.px),
      y: drag.current.panY + (e.clientY - drag.current.py),
    });
  };
  const endDrag = (e: PointerEvent<HTMLDivElement>) => {
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
    drag.current = null;
    setDragging(false);
  };

  return (
    <div
      ref={stageRef}
      className={`mm-stage${dragging ? ' is-panning' : ''}`}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
    >
      <div
        className="mm-canvas"
        style={{
          width: laid.width,
          height: laid.height,
          transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
          transformOrigin: '0 0',
        }}
      >
        <svg className="mm-wires" width={laid.width} height={laid.height} aria-hidden="true">
          {laid.edges.map((e) => (
            <line
              key={`${e.from}-${e.to}`}
              x1={e.x1} y1={e.y1} x2={e.x2} y2={e.y2}
              className={e.from === mapRoot.id ? 'mm-wire hub' : 'mm-wire'}
            />
          ))}
        </svg>
        {laid.nodes.map((p) => (
          p.depth === 0 ? (
            <div
              key={p.id}
              className="mm-hub"
              style={{ left: p.x, top: p.y, width: p.w, height: p.h }}
            >
              <span className="mm-hub-label">{p.node.label}</span>
            </div>
          ) : (
            <div
              key={p.id}
              className={`mm-chip depth-${p.depth}${openId === p.id ? ' active' : ''}${expandedId === p.id ? ' is-expanded' : ''}`}
              style={{ left: p.x, top: p.y, width: p.w, minHeight: p.h }}
            >
              <button
                type="button"
                className="mm-chip-main"
                onClick={() => onOpen(p.id)}
              >
                <span className="mm-chip-label">{p.node.label}</span>
                <span className="mm-chip-count mono">{p.node.sourceCardIds.length}</span>
              </button>
              {(childCount.get(p.id) ?? 0) > 0 && (
                <button
                  type="button"
                  className="mm-chip-expand mono"
                  aria-pressed={expandedId === p.id}
                  onClick={(e) => {
                    e.stopPropagation();
                    onToggleExpand(p.id);
                  }}
                >
                  子题 {childCount.get(p.id)}
                </button>
              )}
            </div>
          )
        ))}
      </div>
      <div
        className="mm-controls"
        aria-label="画布缩放与复位"
        onPointerDown={(e) => e.stopPropagation()}
      >
        <button
          type="button"
          className="mm-ctrl-btn"
          title="放大"
          onClick={() => changeZoom((z) => z + 0.15)}
        >
          +
        </button>
        <button
          type="button"
          className="mm-ctrl-btn mm-ctrl-pct"
          title="还原 100%"
          onClick={() => resetView(1)}
        >
          {Math.round(zoom * 100)}%
        </button>
        <button
          type="button"
          className="mm-ctrl-btn"
          title="缩小"
          onClick={() => changeZoom((z) => z - 0.15)}
        >
          -
        </button>
        <button
          type="button"
          className="mm-ctrl-btn"
          title="居中视野"
          onClick={() => resetView(zoom)}
        >
          居中
        </button>
      </div>
    </div>
  );
}

function findTheme(node: MindmapNode, id: string): MindmapNode | null {
  if (node.id === id && node.kind !== 'book') return node;
  for (const c of node.children) {
    const hit = findTheme(c, id);
    if (hit) return hit;
  }
  return null;
}

export function SettingsView({ onToast, hasKey, onKeyChange }: {
  onToast: (m: string) => void;
  hasKey: boolean;
  onKeyChange: () => Promise<void> | void;
}) {
  const [key, setKey] = useState('');
  const [status, setStatus] = useState<SettingsInfo | null>(null);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<string | null>(null);
  const [batchSize, setBatchSize] = useState<number | null>(null);
  const [llm, setLlm] = useState<LlmSettings | null>(null);
  const [llmProvider, setLlmProvider] = useState<LlmProvider>('off');
  const [llmBaseUrl, setLlmBaseUrl] = useState('');
  const [llmModel, setLlmModel] = useState('');
  const [llmKey, setLlmKey] = useState('');
  const [llmTesting, setLlmTesting] = useState(false);
  const [llmResult, setLlmResult] = useState<string | null>(null);

  useEffect(() => {
    call<SettingsInfo>('get_settings').then(setStatus).catch(() => {});
    call<ReviewSettings>('get_review_settings')
      .then((s) => setBatchSize(s.batchSize))
      .catch(() => setBatchSize(20));
    call<LlmSettings>('get_llm_settings')
      .then((s) => {
        setLlm(s);
        setLlmProvider(s.provider);
        setLlmBaseUrl(s.baseUrl);
        setLlmModel(s.model);
      })
      .catch(() => {
        setLlm({ provider: 'off', baseUrl: '', model: '', hasKey: false });
      });
  }, []);

  const saveBatch = async (size: number) => {
    const prev = batchSize;
    setBatchSize(size);
    try {
      await call('set_review_batch_size', { size });
      await onKeyChange();
    } catch (e) {
      setBatchSize(prev);
      onToast(explainError(e));
    }
  };

  const testConn = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const n = await call<number>('test_connection', { key: key.trim() });
      setTestResult(`连接成功：共 ${n} 本有笔记的书`);
    } catch (e) {
      setTestResult(`失败：${explainError(e)}`);
    } finally {
      setTesting(false);
    }
  };

  const saveKey = async () => {
    try {
      await call('save_api_key', { key: key.trim() });
      await onKeyChange();
      onToast('API Key 已保存到本机。点顶栏「同步」接进划线。');
    } catch (e) {
      onToast(explainError(e));
    }
  };

  const llmDraft = (): LlmDraft => ({
    provider: llmProvider,
    baseUrl: llmBaseUrl.trim(),
    model: llmModel.trim(),
    key: llmKey.trim(),
  });

  const applyLlmPreset = (next: LlmProvider) => {
    const preset = LLM_PROVIDERS.find((p) => p.key === next);
    setLlmProvider(next);
    setLlmResult(null);
    if (!preset) return;
    if (next === 'off') {
      setLlmBaseUrl('');
      setLlmModel('');
      setLlmKey('');
      return;
    }
    const urlIsPreset = !llmBaseUrl.trim()
      || LLM_PROVIDERS.some((p) => p.baseUrl && p.baseUrl === llmBaseUrl.trim());
    const modelIsPreset = !llmModel.trim()
      || LLM_PROVIDERS.some((p) => p.model && p.model === llmModel.trim());
    if (urlIsPreset && preset.baseUrl) setLlmBaseUrl(preset.baseUrl);
    if (modelIsPreset && preset.model) setLlmModel(preset.model);
  };

  const testLlm = async () => {
    setLlmTesting(true);
    setLlmResult(null);
    try {
      const msg = await call<string>('test_llm_connection', { draft: llmDraft() });
      setLlmResult(msg);
    } catch (e) {
      setLlmResult(`失败：${explainError(e)}`);
    } finally {
      setLlmTesting(false);
    }
  };

  const saveLlm = async () => {
    try {
      const saved = await call<LlmSettings>('save_llm_settings', { draft: llmDraft() });
      setLlm(saved);
      setLlmProvider(saved.provider);
      setLlmBaseUrl(saved.baseUrl);
      setLlmModel(saved.model);
      setLlmKey('');
      onToast(saved.provider === 'off' ? '已关闭语言模型。' : '语言模型已保存到本机。默认不会上传整库。');
    } catch (e) {
      onToast(explainError(e));
    }
  };

  const clearLlm = async () => {
    try {
      await call('clear_llm_settings');
      setLlm({ provider: 'off', baseUrl: '', model: '', hasKey: false });
      setLlmProvider('off');
      setLlmBaseUrl('');
      setLlmModel('');
      setLlmKey('');
      setLlmResult(null);
      onToast('已清除语言模型配置');
    } catch (e) {
      onToast(explainError(e));
    }
  };

  const clearKey = async () => {
    try {
      await call('clear_api_key');
      setKey('');
      await onKeyChange();
      onToast('已清除 API Key');
    } catch (e) {
      onToast(explainError(e));
    }
  };

  return (
    <div className="settings">
      <h2>设置</h2>
      <section>
        <h3>一、微信读书 API Key</h3>
        <p className="hint">
          {hasKey ? '本机已存有 Key。再贴一张会覆盖。' : '还没有保存 Key。'}
          {' '}到 <a href={WEREAD_SKILLS_URL} onClick={(e) => { e.preventDefault(); void openExternal(WEREAD_SKILLS_URL); }}>weread.qq.com/r/weread-skills</a> 开通官方 Skills，
          签发以 <code>wrk-</code> 或 <code>WRK-</code> 开头的 Key 后粘贴到这里。
        </p>
        <input
          type="password"
          value={key}
          onChange={(e) => setKey(e.target.value)}
          placeholder="wrk-..."
          aria-label="微信读书 API Key"
        />
        <div className="row">
          <button onClick={testConn} disabled={testing || !key.trim()}>{testing ? '测试中…' : '测试连接'}</button>
          <button className="primary" onClick={saveKey} disabled={!key.trim()}>保存到本机</button>
          <button className="ghost" onClick={clearKey} disabled={!hasKey && !key.trim()}>清除</button>
        </div>
        {testResult && <p className={testResult.startsWith('失败') ? 'err' : 'ok'}>{testResult}</p>}
      </section>
      <section>
        <h3>二、同步</h3>
        <p className="hint">
          上次全量同步：
          {status?.lastFullSync ? new Date(status.lastFullSync * 1000).toLocaleString() : '从未'}
        </p>
        <p className="hint">同步入口在顶栏。没有 Key 时按钮是关上的。单本书失败不影响其他书，下次同步自动重试。</p>
      </section>
      <section>
        <h3>三、回顾</h3>
        <p className="hint">每批翻多少张。小批次完成感更真实，看得到今天还剩多少。</p>
        <div className="row batch-options" role="group" aria-label="每批张数">
          {REVIEW_BATCH_OPTIONS.map((n) => (
            <button
              key={n}
              className={batchSize === n ? 'active' : ''}
              aria-pressed={batchSize === n}
              disabled={batchSize === null}
              onClick={() => void saveBatch(n)}
            >
              {n} 张
            </button>
          ))}
        </div>
      </section>
      <section>
        <h3>四、语言模型</h3>
        <p className="hint">
          默认关闭。脑图、问题面等能力需要时才出网；每次只发送当前动作用到的卡片，不会上传整库。
          Key 存在本机数据目录的 <code>llm.key</code>，与微信读书 Key 分开。
        </p>
        <p className="hint">
          {llm
            ? (llm.provider === 'off'
              ? '当前未启用。'
              : `当前：${LLM_PROVIDERS.find((p) => p.key === llm.provider)?.label ?? llm.provider}${llm.hasKey ? ' · 已存 Key' : ' · 未存 Key'}`)
            : '载入配置…'}
        </p>
        <div className="row batch-options" role="group" aria-label="语言模型供应商">
          {LLM_PROVIDERS.map((p) => (
            <button
              key={p.key}
              className={llmProvider === p.key ? 'active' : ''}
              aria-pressed={llmProvider === p.key}
              disabled={llm === null}
              onClick={() => applyLlmPreset(p.key)}
            >
              {p.label}
            </button>
          ))}
        </div>
        {llmProvider !== 'off' && (
          <>
            <label className="field-label" htmlFor="llm-base">接口地址</label>
            <input
              id="llm-base"
              value={llmBaseUrl}
              onChange={(e) => setLlmBaseUrl(e.target.value)}
              placeholder={LLM_PROVIDERS.find((p) => p.key === llmProvider)?.baseUrl || 'https://api.example.com/v1'}
              aria-label="语言模型接口地址"
            />
            <label className="field-label" htmlFor="llm-model">模型</label>
            <input
              id="llm-model"
              value={llmModel}
              onChange={(e) => setLlmModel(e.target.value)}
              placeholder={LLM_PROVIDERS.find((p) => p.key === llmProvider)?.model || '模型名'}
              aria-label="语言模型名"
            />
            <label className="field-label" htmlFor="llm-key">API Key</label>
            <input
              id="llm-key"
              type="password"
              value={llmKey}
              onChange={(e) => setLlmKey(e.target.value)}
              placeholder={llm?.hasKey ? '已保存，留空则沿用' : (llmProvider === 'ollama' ? '本机可留空' : 'sk-...')}
              aria-label="语言模型 API Key"
            />
            <p className="hint">
              {llmProvider === 'ollama'
                ? 'Ollama 默认走本机 11434 端口，一般不用 Key。请先在本机运行对应模型。'
                : llmProvider === 'custom'
                  ? '兼容 OpenAI 的 /v1 接口即可，例如 DeepSeek、硅基流动。非本机地址必须是 https。'
                  : 'Key 只存在本机。'}
              {' '}测试连接只访问 GET /models，不会保存。生成线索走 POST /chat/completions，用的是点「保存到本机」之后的配置。
            </p>
          </>
        )}
        <div className="row">
          {llmProvider !== 'off' && (
            <button onClick={testLlm} disabled={llmTesting || llm === null}>{llmTesting ? '测试中…' : '测试连接'}</button>
          )}
          <button className="primary" onClick={saveLlm} disabled={llm === null}>保存到本机</button>
          <button className="ghost" onClick={clearLlm} disabled={!llm || (llm.provider === 'off' && !llm.hasKey)}>清除</button>
        </div>
        {llmResult && <p className={llmResult.startsWith('失败') ? 'err' : 'ok'}>{llmResult}</p>}
      </section>
      <section>
        <h3>五、关于</h3>
        <p className="hint">数据目录：{status?.dataDir ?? '未知'}（mudflat.db）</p>
        <p className="hint">纯本地存储 · 无账号 · 无云同步</p>
      </section>
    </div>
  );
}
