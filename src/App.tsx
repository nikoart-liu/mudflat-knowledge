import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Channel } from '@tauri-apps/api/core';
import {
  call,
  emptyFilter,
  type BookRow,
  type CardFilter,
  type CardRow,
  type SettingsInfo,
  type SyncEventPayload,
  type SyncSummary,
  type TagRow,
} from './types';
import './App.css';

type View =
  | { name: 'cards'; bookId: number | null }
  | { name: 'review' }
  | { name: 'settings' };



function fmtDate(unix: number): string {
  const d = new Date(unix * 1000);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

// 内联线性图标（lucide 描边路径），离线可用
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

// 封面：<img> 加载失败回退为竖排书名占位块
function Cover({ src, title, large }: { src: string; title: string; large?: boolean }) {
  const [failed, setFailed] = useState(false);
  const cls = large ? 'cover-lg' : 'cover-box';
  if (!src || failed) {
    return <div className={cls} style={{ background: large ? 'var(--ember-wash)' : undefined }}><span className="cover-fallback">{title.slice(0, large ? 8 : 4)}</span></div>;
  }
  return <div className={cls}><img src={src} alt="" loading="lazy" onError={() => setFailed(true)} /></div>;
}

export default function App() {
  const [view, setView] = useState<View>({ name: 'cards', bookId: null });
  const [books, setBooks] = useState<BookRow[]>([]);
  const [tags, setTags] = useState<TagRow[]>([]);
  const [selectedTagIds, setSelectedTagIds] = useState<number[]>([]);
  const [starredOnly, setStarredOnly] = useState(false);
  const [query, setQuery] = useState('');
  const [cards, setCards] = useState<CardRow[]>([]);
  const [editing, setEditing] = useState<CardRow | null>(null);
  const [creating, setCreating] = useState(false);
  const [tagToDelete, setTagToDelete] = useState<TagRow | null>(null);
  const [syncing, setSyncing] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [dueCount, setDueCount] = useState(0);
  const contentHeaderRef = useRef<HTMLDivElement>(null);
  const [chH, setChH] = useState(76);

  // 分组吸顶钉在 content-header 下缘；其高度随视图（书目头图/搜索头）变化
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
    setBooks(await call<BookRow[]>('list_books'));
    setTags(await call<TagRow[]>('list_tags'));
    call<number>('get_due_count').then(setDueCount).catch(() => {});
  }, []);

  // 卡片重载：任何卡片级变更（加星/删除/编辑/搜索词）后走这里，保持列表与 DB 一致
  const reloadCards = useCallback(async () => {
    if (view.name !== 'cards') return;
    const q = query.trim();
    if (q) {
      setCards(await call<CardRow[]>('search_cards', { q, filter: emptyFilter() }));
      return;
    }
    const filter: CardFilter = {
      ...emptyFilter(),
      bookId: view.bookId,
      tagIds: selectedTagIds,
      starredOnly,
    };
    setCards(await call<CardRow[]>('query_cards', { filter, limit: 500, offset: 0 }));
  }, [view, query, selectedTagIds, starredOnly]);

  useEffect(() => {
    refreshMeta().catch((e) => showToast(String(e)));
  }, [refreshMeta, showToast]);

  // 卡片加载：reloadCards 的依赖（view / 标签 / 星标 / 搜索词）变化触发
  useEffect(() => {
    if (query.trim()) return; // 搜索词非空时由下方 debounce 分支接管
    reloadCards().catch((e) => showToast(String(e)));
  }, [reloadCards, showToast]);

  // 搜索：debounce 250ms
  useEffect(() => {
    if (view.name !== 'cards' || !query.trim()) return;
    const t = window.setTimeout(() => {
      reloadCards().catch((e) => showToast(String(e)));
    }, 250);
    return () => window.clearTimeout(t);
  }, [query, view, reloadCards, showToast]);


  const doSync = useCallback(async () => {
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
      showToast(`同步失败：${String(e)}`);
    } finally {
      setSyncing(null);
    }
  }, [refreshMeta, reloadCards, showToast]);

  const toggleTagFilter = (id: number) =>
    setSelectedTagIds((prev) => (prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id]));

  // 删除标签：后端级联清掉 card_tags；selectedTagIds 换新数组引用必然触发
  // reloadCards 依赖链，卡片列表按新选中集刷新，无需手动调用
  const deleteTag = async (t: TagRow) => {
    try {
      await call('delete_tag', { tagId: t.id });
      setSelectedTagIds((prev) => prev.filter((id) => id !== t.id));
      await refreshMeta();
      showToast(`已删除标签「${t.name}」`);
    } catch (e) {
      showToast(String(e));
    }
  };

  const activeBook = view.name === 'cards' && view.bookId ? books.find((b) => b.id === view.bookId) : null;

  // 卡片墙分组地标：全部视图按书、单书视图按月；搜索结果保持相关性平铺
  const wallGroups = useMemo(() => {
    if (view.name !== 'cards' || query.trim()) return null;
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
  }, [cards, view, query]);

  return (
    <div className="app">
      <header className="topbar">
        <div className="logo">
          <span className="mark"><Icon name="layers" size={15} /></span>
          <span>MUDFLAT KNOWLEDGE</span>
        </div>
        <div className="search">
          <Icon name="search" size={13} />
          <input
            placeholder="检索卡片全文…"
            aria-label="检索卡片全文"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === 'Escape' && setQuery('')}
          />
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginLeft: 'auto' }}>
          <button className="primary" disabled={!!syncing} onClick={doSync}>{syncing ?? '同步'}</button>
          <button
            className={`ghost ${view.name === 'settings' ? 'active' : ''}`}
            onClick={() => setView({ name: 'settings' })}
            title="设置"
            aria-label="设置"
            aria-current={view.name === 'settings' ? 'true' : undefined}
          >
            <Icon name="gear" size={14} />
          </button>
        </div>
      </header>

      <div className="body">
        <aside className="sidebar">
          <section className="side-group side-review">
            <h2>Flip / 翻牌</h2>
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
              <h2>Library / 书架</h2>
              <button
                className={`side-item ${view.name === 'cards' && !view.bookId ? 'active' : ''}`}
                aria-current={view.name === 'cards' && !view.bookId ? 'true' : undefined}
                onClick={() => { setQuery(''); setView({ name: 'cards', bookId: null }); }}
              >
                <Icon name="layers" size={13} />
                <span className="grow">全部卡片</span>
                <span className="count">{books.reduce((n, b) => n + b.noteCount + b.reviewCount, 0)}</span>
              </button>
              <button
                className={`side-item ${starredOnly ? 'active' : ''}`}
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
                  className={`side-item ${view.name === 'cards' && view.bookId === b.id ? 'active' : ''}`}
                  aria-current={view.name === 'cards' && view.bookId === b.id ? 'true' : undefined}
                  onClick={() => { setQuery(''); setView({ name: 'cards', bookId: b.id }); }}
                  title={b.title}
                >
                  <Cover src={b.cover} title={b.title} />
                  <span className="grow">{b.title}</span>
                  <span className="count">{b.noteCount + b.reviewCount}</span>
                </button>
              ))}
            </section>
          </div>
          {/* 底固定区：标签筛选常驻可见 */}
          <section className="side-group side-tags">
            <h2>Tags / 标签</h2>
            <div className="tag-cloud">
              {tags.map((t) => (
                <span key={t.id} className="chip-item">
                  <button
                    className={`chip ${selectedTagIds.includes(t.id) ? 'active' : ''}`}
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
                  {activeBook ? (
                    <>
                      <Cover src={activeBook.cover} title={activeBook.title} large />
                      <div style={{ minWidth: 0 }}>
                        <div className="eyebrow">Reading Library / {activeBook.author || '佚名'}</div>
                        <h2>{activeBook.title}</h2>
                        <div className="head-meta">
                          <span className="chip">划线 {activeBook.noteCount}</span>
                          <span className="chip">想法 {activeBook.reviewCount}</span>
                          <span className="card-date">{cards.length} shown</span>
                        </div>
                      </div>
                    </>
                  ) : (
                    <div style={{ minWidth: 0 }}>
                      {query && <div className="eyebrow">Search / 搜索结果</div>}
                      <h2>{query ? `“${query}”` : '全部卡片'}</h2>
                      <div className="head-meta"><span className="card-date">{cards.length} shown</span></div>
                    </div>
                  )}
                </div>
                <button className="fab" title="新建卡片" aria-label="新建卡片" onClick={() => setCreating(true)}>
                  <Icon name="plus" size={16} />
                </button>
              </div>
              <div className="wall" style={{ '--ch-h': `${chH}px` } as React.CSSProperties}>
                {wallGroups
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
                  </section>}
                {!cards.length && <p className="hint empty-state">没有匹配的卡片。</p>}
              </div>
            </>
          )}
          {view.name === 'review' && (
            <ReviewView onToast={showToast} onExit={() => { refreshMeta(); setView({ name: 'cards', bookId: null }); }} />
          )}
          {view.name === 'settings' && <SettingsView onToast={showToast} onSync={doSync} syncing={!!syncing} />}
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
          message={`删除标签「${tagToDelete.name}」？将把它从所有卡片上移除。`}
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

// ---------- 卡片 ----------

function Card({ card, onEdit, onChanged, onToast }: {
  card: CardRow;
  onEdit: () => void;
  onChanged: () => void;
  onToast: (m: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [overflowed, setOverflowed] = useState(false);
  const textRef = useRef<HTMLParagraphElement>(null);

  // 未展开时测量钳制截断：有溢出才给展开承诺（指针 + 键盘语义）
  useEffect(() => {
    if (expanded) return;
    const el = textRef.current;
    if (el) setOverflowed(el.scrollHeight > el.clientHeight + 1);
  }, [expanded, card.text, card.note]);

  const toggleExpand = () => setExpanded((v) => !v);
  const [confirming, setConfirming] = useState(false);

  const toggleStar = async () => {
    await call('toggle_starred', { id: card.id, starred: !card.starred }).catch((e) => onToast(String(e)));
    onChanged();
  };
  // WKWebView 未实现 window.confirm（wry 无 delegate，恒返回 false），改用应用内确认弹层
  const remove = () => setConfirming(true);
  const confirmRemove = async () => {
    setConfirming(false);
    await call('delete_card', { id: card.id }).catch((e) => onToast(String(e)));
    onChanged();
  };
  return (
    <>
    <article className={`card kind-${card.kind}${expanded ? ' expanded' : ''}`}>
      <div className="card-actions">
        <button className={card.starred ? 'starred' : ''} title="星标" aria-label="星标" aria-pressed={card.starred} onClick={toggleStar}><Icon name="star" size={13} /></button>
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
        message="删除这张卡片？此操作不可撤销。"
        onConfirm={confirmRemove}
        onCancel={() => setConfirming(false)}
      />
    )}
    </>
  );
}

// 弹层对话框共用行为：打开聚焦首控件、Tab 循环、Esc 关闭、关闭归还焦点。
// onClose 走 ref，避免父组件重渲染时 effect 重跑把焦点拽回首个控件。
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

function ConfirmModal({ message, onConfirm, onCancel }: {
  message: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const ref = useDialog(onCancel);
  return (
    <div className="modal-mask" onClick={onCancel}>
      <div className="modal modal-confirm" ref={ref} role="dialog" aria-modal="true" aria-labelledby="modal-confirm-title" onClick={(e) => e.stopPropagation()}>
        <h3 id="modal-confirm-title">确认</h3>
        <p>{message}</p>
        <div className="modal-actions">
          <button className="ghost" onClick={onCancel}>取消</button>
          <button className="primary danger" onClick={onConfirm}>删除</button>
        </div>
      </div>
    </div>
  );
}

// ---------- 编辑弹层 ----------

function EditModal({ card, onClose, onSaved, onToast }: {
  card: CardRow;
  onClose: () => void;
  onSaved: () => void;
  onToast: (m: string) => void;
}) {
  const ref = useDialog(onClose);
  const [note, setNote] = useState(card.note);
  const [text, setText] = useState(card.text);
  const [tagName, setTagName] = useState('');
  const isSelf = card.kind === 'self';
  const save = async () => {
    try {
      await call('update_card', {
        id: card.id,
        note,
        text: isSelf ? text : undefined,
      });
      onSaved();
    } catch (e) {
      onToast(String(e));
    }
  };
  const addTag = async () => {
    if (!tagName.trim()) return;
    await call('add_tag', { cardId: card.id, name: tagName.trim() }).catch((e) => onToast(String(e)));
    setTagName('');
    onSaved();
  };
  const removeTag = async (t: string) => {
    await call('remove_tag', { cardId: card.id, name: t }).catch((e) => onToast(String(e)));
    onSaved();
  };
  return (
    <div className="modal-mask" onClick={onClose}>
      <div className="modal" ref={ref} role="dialog" aria-modal="true" aria-labelledby="modal-edit-title" onClick={(e) => e.stopPropagation()}>
        <h3 id="modal-edit-title">{isSelf ? '编辑自建卡' : '补写想法'}</h3>
        {isSelf ? (
          <textarea rows={6} value={text} onChange={(e) => setText(e.target.value)} placeholder="正文" />
        ) : (
          <blockquote className="quote-box">{card.text}</blockquote>
        )}
        {!isSelf && (
          <textarea rows={5} value={note} onChange={(e) => setNote(e.target.value)} placeholder="写下你的想法…" autoFocus />
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
          />
          <button onClick={addTag}>加标签</button>
        </div>
        <div className="modal-actions">
          <button className="ghost" onClick={onClose}>取消</button>
          <button className="primary" onClick={save}>保存</button>
        </div>
      </div>
    </div>
  );
}

// ---------- 新建自建卡 ----------

function CreateModal({ onClose, onSaved, onToast }: {
  onClose: () => void;
  onSaved: () => void;
  onToast: (m: string) => void;
}) {
  const [text, setText] = useState('');
  const [tagsInput, setTagsInput] = useState('');
  const ref = useDialog(onClose);
  const save = async () => {
    if (!text.trim()) return;
    try {
      const tagNames = tagsInput.split(/[,，\s]+/).filter(Boolean);
      await call('create_card', { text: text.trim(), tagNames });
      onSaved();
      onClose();
    } catch (e) {
      onToast(String(e));
    }
  };
  return (
    <div className="modal-mask" onClick={onClose}>
      <div className="modal" ref={ref} role="dialog" aria-modal="true" aria-labelledby="modal-create-title" onClick={(e) => e.stopPropagation()}>
        <h3 id="modal-create-title">新建卡片</h3>
        <textarea rows={7} value={text} onChange={(e) => setText(e.target.value)} placeholder="记录一个想法…" autoFocus />
        <input value={tagsInput} onChange={(e) => setTagsInput(e.target.value)} placeholder="标签（逗号分隔，可空）" />
        <div className="modal-actions">
          <button className="ghost" onClick={onClose}>取消</button>
          <button onClick={save}>保存</button>
        </div>
      </div>
    </div>
  );
}

// ---------- 复习 · 翻牌 ----------

// 翻牌是一副编目卡牌：背面朝上，mono 编目号做悬念；翻面读原文，再点飞出换下一张。
// 评级按钮移除：翻过即静默记 Good，间隔重复调度照常延续。

function ReviewView({ onToast, onExit }: { onToast: (m: string) => void; onExit: () => void }) {
  const [queue, setQueue] = useState<CardRow[]>([]);
  const [idx, setIdx] = useState(0);
  const [flipped, setFlipped] = useState(false);
  const [flying, setFlying] = useState(false);
  const [loading, setLoading] = useState(true);
  const loadingRef = useRef(false);
  const flyingRef = useRef(false);

  useEffect(() => {
    if (loadingRef.current) return;
    loadingRef.current = true;
    call<CardRow[]>('get_due_cards', { limit: 30 })
      .then((c) => setQueue(c))
      .catch((e) => onToast(String(e)))
      .finally(() => setLoading(false));
  }, [onToast]);

  const advance = () => {
    if (flyingRef.current) return;
    flyingRef.current = true;
    const card = queue[idx];
    if (card) call('grade_review', { cardId: card.id, rating: 'good' }).catch((e) => onToast(String(e)));
    setFlying(true);
    window.setTimeout(() => {
      flyingRef.current = false;
      setFlying(false);
      setFlipped(false);
      setIdx((i) => i + 1);
    }, 240);
  };

  const flipOrAdvance = () => {
    if (flyingRef.current) return;
    if (flipped) advance();
    else setFlipped(true);
  };

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      // 焦点在按钮/输入框上时交给原生激活，避免一次按键同时翻卡+点按钮
      if (t && (t.tagName === 'BUTTON' || t.tagName === 'INPUT' || t.tagName === 'TEXTAREA')) return;
      if (e.key === 'Escape') onExit();
      if (e.key === ' ' || e.key === 'Enter') {
        e.preventDefault();
        flipOrAdvance();
      }
      if (e.key === 'ArrowRight' && flipped) advance();
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [flipped, idx, queue, onExit]);

  if (loading) return <div className="review"><p className="hint">加载队列…</p></div>;
  if (idx >= queue.length)
    return (
      <div className="review">
        <div className="review-top"><span>Flip / 翻牌</span></div>
        <div className="deck-done">
          <p className="review-text">{queue.length ? '这副翻完了。' : '当前没有到期卡片'}</p>
          <p className="review-hint">{queue.length ? `共翻阅 ${queue.length} 张 · 间隔重复讲究少而勤。` : '新卡片会自动进入队列。'}</p>
          <button className="primary" onClick={onExit}>返回卡片墙</button>
        </div>
      </div>
    );

  const card = queue[idx];
  const under = [queue[idx + 1], queue[idx + 2]];
  return (
    <div className="review">
      <div className="review-top">
        <span>Review / 剩余 {queue.length - idx} 张</span>
        <button className="ghost" onClick={onExit}>退出 (Esc)</button>
      </div>
      <div className="review-ticks" aria-hidden="true">
        {queue.map((_, i) => <i key={i} className={i < idx ? 'done' : i === idx ? 'now' : ''} />)}
      </div>
      <div className="deck-stage" onClick={flipOrAdvance}>
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
                <span className="deck-back-label">Flip / 翻牌</span>
                <span className="deck-back-num mono">{String(idx + 1).padStart(2, '0')}<span> / {queue.length}</span></span>
              </div>
              <span className="deck-back-hint"><Icon name="refresh" size={12} />轻触翻面</span>
            </div>
            <div className="deck-face front">
              <span className="card-source">{[card.bookTitle, card.chapterTitle].filter(Boolean).join(' / ') || '自建卡'}</span>
              <div className="deck-front-inner">
                <p className="review-text">{card.text}</p>
                {card.abstractText && <blockquote>{card.abstractText}</blockquote>}
                {card.note && <p className="card-note">{card.note}</p>}
              </div>
              <span className="deck-next-hint">空格 / 点击 · 翻过这张</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ---------- 设置 ----------

function SettingsView({ onToast, onSync, syncing }: { onToast: (m: string) => void; onSync: () => void; syncing: boolean }) {
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
      setTestResult(`失败：${String(e)}`);
    } finally {
      setTesting(false);
    }
  };

  const saveKey = async () => {
    try {
      await call('save_api_key', { key: key.trim() });
      onToast('API Key 已存入钥匙串');
    } catch (e) {
      onToast(String(e));
    }
  };

  const clearKey = async () => {
    try {
      await call('clear_api_key');
      onToast('已清除 API Key');
    } catch (e) {
      onToast(String(e));
    }
  };

  return (
    <div className="settings">
      <h2>设置</h2>
      <section>
        <h3>微信读书 API Key</h3>
        <p className="hint">
          到 <a href="https://weread.qq.com/r/weread-skills" target="_blank" rel="noreferrer">weread.qq.com/r/weread-skills</a> 开通官方 Skills，
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
          <button onClick={saveKey} disabled={!key.trim()}>保存到钥匙串</button>
          <button className="ghost" onClick={clearKey}>清除</button>
        </div>
        {testResult && <p className={testResult.startsWith('失败') ? 'err' : 'ok'}>{testResult}</p>}
      </section>
      <section>
        <h3>手动同步</h3>
        <p className="hint">
          上次全量同步：
          {status?.lastFullSync ? new Date(status.lastFullSync * 1000).toLocaleString() : '从未'}
        </p>
        <button className="primary" onClick={onSync} disabled={syncing}>{syncing ? '同步中…' : '立即同步'}</button>
      </section>
      <section>
        <h3>关于</h3>
        <p className="hint">数据目录：{status?.dataDir ?? '未知'}（mudflat.db）</p>
        <p className="hint">纯本地存储 · 无账号 · 无云同步</p>
      </section>
    </div>
  );
}
