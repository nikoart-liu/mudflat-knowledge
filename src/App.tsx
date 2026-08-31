import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Channel } from '@tauri-apps/api/core';
import {
  call,
  emptyFilter,
  type BookRow,
  type CardFilter,
  type CardRow,
  type ReviewRating,
  type ReviewSettings,
  type SettingsInfo,
  type SetupStatus,
  type SrsState,
  type SyncEventPayload,
  type SyncSummary,
  type TagRow,
} from './types';
import './App.css';

type CardsView = { name: 'cards'; bookId: number | null };
type View = CardsView | { name: 'review'; bookId: number | null } | { name: 'settings' };

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
    lastViewRef.current = view.name === 'review'
      ? { name: 'review', bookId: view.bookId }
      : { name: 'cards', bookId: view.bookId };
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
    <div className={`app${reviewing ? ' is-review' : ''}${readingActive ? ' is-reading' : ''}${view.name === 'settings' ? ' is-settings' : ''}`}>
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

function SettingsView({ onToast, hasKey, onKeyChange }: {
  onToast: (m: string) => void;
  hasKey: boolean;
  onKeyChange: () => Promise<void> | void;
}) {
  const [key, setKey] = useState('');
  const [status, setStatus] = useState<SettingsInfo | null>(null);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<string | null>(null);
  const [batchSize, setBatchSize] = useState<number | null>(null);

  useEffect(() => {
    call<SettingsInfo>('get_settings').then(setStatus).catch(() => {});
    call<ReviewSettings>('get_review_settings')
      .then((s) => setBatchSize(s.batchSize))
      .catch(() => setBatchSize(20));
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
        <h3>四、关于</h3>
        <p className="hint">数据目录：{status?.dataDir ?? '未知'}（mudflat.db）</p>
        <p className="hint">纯本地存储 · 无账号 · 无云同步</p>
      </section>
    </div>
  );
}
