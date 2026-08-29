import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Channel } from '@tauri-apps/api/core';
import {
  call,
  emptyFilter,
  type BookRow,
  type CardFilter,
  type CardRow,
  type SettingsInfo,
  type SetupStatus,
  type SyncEventPayload,
  type SyncSummary,
  type TagRow,
} from './types';
import './App.css';

type CardsView = { name: 'cards'; bookId: number | null };
type View = CardsView | { name: 'review' } | { name: 'settings' };

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
    return '读钥匙串失败。到设置里重新保存 API Key。';
  }
  return s;
}

const ICON_PATHS: Record<string, string> = {
  layers: '<path d="m12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83Z"/><path d="m22 17.65-9.17 4.16a2 2 0 0 1-1.66 0L2 17.65"/><path d="m22 12.65-9.17 4.16a2 2 0 0 1-1.66 0L2 12.65"/>',
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
    <div className={cls} style={{ background: large && !showImg ? 'var(--ember-wash)' : undefined }} aria-hidden={large ? undefined : true}>
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
  const [setup, setSetup] = useState<SetupStatus | null>(null);
  const contentHeaderRef = useRef<HTMLDivElement>(null);
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
        const rows = await call<CardRow[]>('search_cards', { q, filter: emptyFilter() });
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
  }, [view, query, wallFilter]);

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
        else if (ev.stage === 'done') setSyncing(null);
      };
      const summary: SyncSummary = await call('sync_all', { onProgress: chan });
      showToast(`同步完成：${summary.booksSynced} 本 · 划线 ${summary.highlights} · 想法 ${summary.thoughts}`);
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

  const clearFilters = () => {
    setQuery('');
    setStarredOnly(false);
    setSelectedTagIds([]);
    setView({ name: 'cards', bookId: null });
  };

  const toggleSettings = () => {
    if (view.name === 'settings') {
      setView(lastViewRef.current);
      return;
    }
    lastViewRef.current = view.name === 'review' ? { name: 'review' } : { name: 'cards', bookId: view.bookId };
    setView({ name: 'settings' });
  };

  const goCards = (bookId: number | null) => {
    setQuery('');
    setView({ name: 'cards', bookId });
  };

  const activeBook = !searching && view.name === 'cards' && view.bookId
    ? books.find((b) => b.id === view.bookId) ?? null
    : null;
  const filtered = starredOnly || selectedTagIds.length > 0 || (view.name === 'cards' && view.bookId !== null);

  const wallGroups = useMemo(() => {
    if (view.name !== 'cards' || searching) return null;
    const byBook = !view.bookId;
    const map = new Map<string, { key: string; label: string; mono: boolean; cards: CardRow[] }>();
    for (const c of cards) {
      let key: string, label: string, mono = false;
      if (byBook) {
        key = c.bookTitle || 'self';
        label = c.bookTitle || '自建卡';
      } else {
        const d = new Date(c.createdAt * 1000);
        key = `${d.getFullYear()}-${d.getMonth()}`;
        label = `${d.getFullYear()} · ${String(d.getMonth() + 1).padStart(2, '0')}`;
        mono = true;
      }
      let g = map.get(key);
      if (!g) { g = { key, label, mono, cards: [] }; map.set(key, g); }
      g.cards.push(c);
    }
    return [...map.values()];
  }, [cards, view, searching]);

  const countLabel = (() => {
    if (searching) {
      if (cards.length >= SEARCH_CAP) return `前 ${SEARCH_CAP} 条，请把词写得更具体`;
      return `${cards.length} 张 · 正在搜全部卡片`;
    }
    if (cardTotal > cards.length) return `已显示 ${cards.length} / 共 ${cardTotal}`;
    return `${cards.length} 张`;
  })();

  const reviewing = view.name === 'review';

  return (
    <div className={`app${reviewing ? ' is-review' : ''}`}>
      <header className="topbar">
        <div className="logo">
          <img className="mark" src="/logo-mark.svg" width={18} height={18} alt="" />
          <span>泥滩知识</span>
        </div>
        <div className="search">
          <Icon name="search" size={13} />
          <input
            ref={searchRef}
            type="search"
            placeholder="检索全部卡片…"
            aria-label="检索全部卡片，按 / 聚焦"
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
              onClick={() => setView({ name: 'review' })}
              title="翻牌"
              aria-current={view.name === 'review' ? 'true' : undefined}
            >
              <Icon name="refresh" size={13} />
              <span className="grow">翻牌</span>
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
                <span className="grow">全部卡片</span>
                <span className="count">{books.reduce((n, b) => n + b.noteCount + b.reviewCount, 0)}</span>
              </button>
              <button
                className={`side-item ${!searching && starredOnly ? 'active' : ''}`}
                onClick={() => setStarredOnly((v) => !v)}
                aria-pressed={starredOnly}
                title="只看星标卡片"
              >
                <Icon name="star" size={13} />
                <span className="grow">只看星标</span>
              </button>
              {books.map((b) => (
                <button
                  key={b.id}
                  className={`side-item ${!searching && view.name === 'cards' && view.bookId === b.id ? 'active' : ''}`}
                  aria-current={!searching && view.name === 'cards' && view.bookId === b.id ? 'true' : undefined}
                  onClick={() => goCards(b.id)}
                  title={b.title}
                >
                  <Cover src={b.cover} title={b.title} />
                  <span className="grow">{b.title}</span>
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

        <main className="main">
          {view.name === 'cards' && (
            <>
              <div className="content-header" ref={contentHeaderRef}>
                <div className="head-left">
                  {searching ? (
                    <div style={{ minWidth: 0 }}>
                      <h2>「{query.trim()}」</h2>
                      <div className="head-meta"><span className="card-date">{countLabel}</span></div>
                    </div>
                  ) : activeBook ? (
                    <>
                      <Cover src={activeBook.cover} title={activeBook.title} large />
                      <div style={{ minWidth: 0 }}>
                        <h2>{activeBook.title}</h2>
                        <div className="head-meta">
                          <span>{activeBook.author || '佚名'}</span>
                          <span className="chip">划线 {activeBook.noteCount}</span>
                          <span className="chip">想法 {activeBook.reviewCount}</span>
                          <span className="card-date">{countLabel}</span>
                        </div>
                      </div>
                    </>
                  ) : (
                    <div style={{ minWidth: 0 }}>
                      <h2>{starredOnly ? '星标卡片' : '全部卡片'}</h2>
                      <div className="head-meta"><span className="card-date">{countLabel}</span></div>
                    </div>
                  )}
                </div>
                {!hideFab && (
                  <button className="fab" title="新建卡片" aria-label="新建卡片" onClick={() => setCreating(true)}>
                    <Icon name="plus" size={16} />
                  </button>
                )}
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
              onToast={showToast}
              onExit={() => { refreshMeta(); setView({ name: 'cards', bookId: null }); }}
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
        <p className="empty-title">Key 已经在钥匙串里，墙上还是空的</p>
        <p className="empty-body">同步一次，微信读书里的划线会出现在这里。</p>
        <button className="primary" onClick={onSync} disabled={!!syncing}>{syncing ?? '同步'}</button>
      </div>
    );
  }
  if (searching) {
    return (
      <div className="empty-setup">
        <p className="empty-title">没有找到「{query}」</p>
        <p className="empty-body">检索范围是全部卡片。换个词，或按 Esc 退出检索。</p>
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
        <button title="删除" aria-label="删除" onClick={remove}><Icon name="trash" size={13} /></button>
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
        {!!card.tags.length && (
          <div className="tag-row">{card.tags.map((t) => <span key={t} className="chip small">{t}</span>)}</div>
        )}
        <span className="card-source">
          {[card.bookTitle, card.chapterTitle].filter(Boolean).join(' / ') || '自建卡'}
        </span>
        <span className="card-date">{fmtDate(card.createdAt)}</span>
      </div>
    </article>
    {confirming && (
      <ConfirmModal
        title="删除卡片"
        message="删除这张卡片？此操作不可撤销。"
        confirmLabel="删除卡片"
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

function EditModal({ card, onClose, onSaved, onToast }: {
  card: CardRow;
  onClose: () => void;
  onSaved: (patch: { note: string; text: string }) => void;
  onToast: (m: string) => void;
}) {
  const ref = useDialog(onClose);
  const [note, setNote] = useState(card.note);
  const [text, setText] = useState(card.text);
  const [tagName, setTagName] = useState('');
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

function ReviewView({ onToast, onExit, hasKey, hasBooks }: {
  onToast: (m: string) => void;
  onExit: () => void;
  hasKey: boolean;
  hasBooks: boolean;
}) {
  const [queue, setQueue] = useState<CardRow[]>([]);
  const [idx, setIdx] = useState(0);
  const [flipped, setFlipped] = useState(false);
  const [flying, setFlying] = useState(false);
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState<CardRow | null>(null);
  const loadingRef = useRef(false);
  const flyingRef = useRef(false);
  const flippedRef = useRef(false);
  flippedRef.current = flipped;

  useEffect(() => {
    if (loadingRef.current) return;
    loadingRef.current = true;
    call<CardRow[]>('get_due_cards', { limit: 30 })
      .then((c) => setQueue(c))
      .catch((e) => onToast(explainError(e)))
      .finally(() => setLoading(false));
  }, [onToast]);

  const advance = () => {
    if (flyingRef.current) return;
    flyingRef.current = true;
    const card = queue[idx];
    if (card) call('grade_review', { cardId: card.id, rating: 'good' }).catch((e) => onToast(explainError(e)));
    setFlying(true);
    window.setTimeout(() => {
      flyingRef.current = false;
      setFlying(false);
      setFlipped(false);
      setIdx((i) => i + 1);
    }, 240);
  };

  const unflip = () => {
    if (flyingRef.current || !flippedRef.current) return;
    setFlipped(false);
  };

  const flipOrAdvance = () => {
    if (flyingRef.current || editing) return;
    if (flipped) advance();
    else setFlipped(true);
  };

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === 'BUTTON' || t.tagName === 'INPUT' || t.tagName === 'TEXTAREA')) return;
      if (e.key === 'Escape') { onExit(); return; }
      if (e.key === 'z' || e.key === 'Z' || e.key === 'Backspace') {
        e.preventDefault();
        unflip();
        return;
      }
      if (e.key === ' ' || e.key === 'Enter') {
        e.preventDefault();
        flipOrAdvance();
      }
      if (e.key === 'ArrowRight' && flipped) advance();
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [flipped, idx, queue, onExit, editing]);

  if (loading) return <div className="review"><p className="hint">加载队列…</p></div>;
  if (idx >= queue.length) {
    const done = queue.length > 0;
    const hint = done
      ? `共翻阅 ${queue.length} 张 · 间隔重复讲究少而勤。`
      : !hasKey
        ? '先到设置填写 API Key，再同步。到期的卡片会排进这副牌。'
        : !hasBooks
          ? '同步之后，新卡片会自动进入队列。'
          : '新卡片会自动进入队列。间隔重复讲究少而勤。';
    return (
      <div className="review">
        <div className="review-top"><span>翻牌</span></div>
        <div className="deck-done">
          <p className="review-text">{done ? '这副翻完了。' : '当前没有到期卡片'}</p>
          <p className="review-hint">{hint}</p>
          <button className="primary" onClick={onExit}>返回卡片墙</button>
        </div>
      </div>
    );
  }

  const card = queue[idx];
  const under = [queue[idx + 1], queue[idx + 2]];
  return (
    <div className="review">
      <div className="review-top">
        <span>剩余 {queue.length - idx} 张</span>
        <button className="ghost" onClick={onExit}>退出（Esc）</button>
      </div>
      <div className="review-ticks" aria-hidden="true">
        {queue.map((_, i) => <i key={i} className={i < idx ? 'done' : i === idx ? 'now' : ''} />)}
      </div>
      <div
        className="deck-stage"
        role="button"
        tabIndex={0}
        aria-label={flipped ? '翻过这张' : '翻面阅读'}
        onClick={flipOrAdvance}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); flipOrAdvance(); }
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
              <span className="deck-next-hint">点击或空格翻过这张 · Z 退回背面</span>
            </div>
          </div>
        </div>
      </div>
      {flipped && (
        <div className="review-tools">
          <button
            className="ghost"
            onClick={(e) => { e.stopPropagation(); setEditing(card); }}
          >
            记下想法
          </button>
        </div>
      )}
      {editing && (
        <EditModal
          card={editing}
          onClose={() => setEditing(null)}
          onSaved={(patch) => {
            setQueue((qs) => qs.map((c) => (c.id === editing.id ? { ...c, note: patch.note, text: patch.text } : c)));
            setEditing(null);
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

  useEffect(() => {
    call<SettingsInfo>('get_settings').then(setStatus).catch(() => {});
  }, []);

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
      onToast('API Key 已存入钥匙串。点顶栏「同步」接进划线。');
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
        <h3>微信读书 API Key</h3>
        <p className="hint">
          {hasKey ? '钥匙串里已有 Key。再贴一张会覆盖。' : '还没有保存 Key。'}
          {' '}到 <a href="https://weread.qq.com/r/weread-skills" target="_blank" rel="noreferrer">weread.qq.com/r/weread-skills</a> 开通官方 Skills，
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
          <button className="primary" onClick={saveKey} disabled={!key.trim()}>保存到钥匙串</button>
          <button className="ghost" onClick={clearKey} disabled={!hasKey && !key.trim()}>清除</button>
        </div>
        {testResult && <p className={testResult.startsWith('失败') ? 'err' : 'ok'}>{testResult}</p>}
      </section>
      <section>
        <h3>同步</h3>
        <p className="hint">
          上次全量同步：
          {status?.lastFullSync ? new Date(status.lastFullSync * 1000).toLocaleString() : '从未'}
        </p>
        <p className="hint">同步入口在顶栏。没有 Key 时按钮是关上的。</p>
      </section>
      <section>
        <h3>关于</h3>
        <p className="hint">数据目录：{status?.dataDir ?? '未知'}（mudflat.db）</p>
        <p className="hint">纯本地存储 · 无账号 · 无云同步</p>
      </section>
    </div>
  );
}
