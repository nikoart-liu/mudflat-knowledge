import { useCallback, useEffect, useRef, useState } from 'react';
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

const COLOR_MAP: Record<number, string> = {
  1: '#d4edda',
  2: '#e2d5f1',
  3: '#fff3cd',
  4: '#d1ecf1',
  5: '#f8d7da',
};

function colorOf(card: CardRow): string {
  return COLOR_MAP[card.colorStyle] ?? '#e9ecef';
}

function fmtDate(unix: number): string {
  const d = new Date(unix * 1000);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
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
  const [syncing, setSyncing] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  const showToast = useCallback((msg: string) => {
    setToast(msg);
    window.setTimeout(() => setToast(null), 2600);
  }, []);

  const refreshMeta = useCallback(async () => {
    setBooks(await call<BookRow[]>('list_books'));
    setTags(await call<TagRow[]>('list_tags'));
  }, []);

  useEffect(() => {
    refreshMeta().catch((e) => showToast(String(e)));
  }, [refreshMeta, showToast]);

  // 卡片加载：view.cards / 标签 / 星标变化触发
  useEffect(() => {
    if (view.name !== 'cards') return;
    const filter: CardFilter = {
      ...emptyFilter(),
      bookId: view.bookId,
      tagIds: selectedTagIds,
      starredOnly,
    };
    call<CardRow[]>('query_cards', { filter, limit: 500, offset: 0 })
      .then(setCards)
      .catch((e) => showToast(String(e)));
  }, [view, selectedTagIds, starredOnly, showToast]);

  // 搜索：debounce 250ms
  useEffect(() => {
    if (view.name !== 'cards') return;
    const q = query.trim();
    if (!q) return;
    const t = window.setTimeout(() => {
      call<CardRow[]>('search_cards', { q, filter: emptyFilter() })
        .then(setCards)
        .catch((e) => showToast(String(e)));
    }, 250);
    return () => window.clearTimeout(t);
  }, [query, view, showToast]);

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
      if (view.name === 'cards') {
        const filter: CardFilter = {
          ...emptyFilter(),
          bookId: view.bookId,
          tagIds: selectedTagIds,
          starredOnly,
        };
        setCards(await call<CardRow[]>('query_cards', { filter, limit: 500, offset: 0 }));
      }
    } catch (e) {
      showToast(`同步失败：${String(e)}`);
    } finally {
      setSyncing(null);
    }
  }, [refreshMeta, showToast, view, selectedTagIds, starredOnly]);

  const loadDemo = useCallback(async () => {
    try {
      const n = await call<number>('load_demo_data');
      showToast(`已载入示例数据 ${n} 张卡`);
      await refreshMeta();
    } catch (e) {
      showToast(String(e));
    }
  }, [refreshMeta, showToast]);

  const toggleTagFilter = (id: number) =>
    setSelectedTagIds((prev) => (prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id]));

  const activeBook = view.name === 'cards' && view.bookId ? books.find((b) => b.id === view.bookId) : null;

  return (
    <div className="app">
      <header className="topbar">
        <div className="logo">📚 泥滩知识</div>
        <input
          className="search"
          placeholder="搜索卡片全文…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === 'Escape' && setQuery('')}
        />
        {import.meta.env.DEV && (
          <button className="ghost" onClick={loadDemo}>载入示例数据</button>
        )}
        <button disabled={!!syncing} onClick={doSync}>{syncing ?? '同步'}</button>
        <button className={`ghost ${view.name === 'settings' ? 'active' : ''}`} onClick={() => setView({ name: 'settings' })}>
          设置
        </button>
      </header>

      <div className="body">
        <aside className="sidebar">
          <section>
            <h3>书架</h3>
            <button
              className={`side-item ${view.name === 'cards' && !view.bookId ? 'active' : ''}`}
              onClick={() => { setQuery(''); setView({ name: 'cards', bookId: null }); }}
            >
              全部卡片
            </button>
            {books.map((b) => (
              <button
                key={b.id}
                className={`side-item ${view.name === 'cards' && view.bookId === b.id ? 'active' : ''}`}
                onClick={() => { setQuery(''); setView({ name: 'cards', bookId: b.id }); }}
              >
                {b.cover ? <img src={b.cover} alt="" loading="lazy" /> : <span className="cover-ph" />}
                <span className="side-title">{b.title}</span>
                <span className="badge">{b.noteCount + b.reviewCount}</span>
              </button>
            ))}
            {!books.length && <p className="hint">还没有书。请先到设置页配置 API Key 并同步，或在开发模式载入示例数据。</p>}
          </section>
          <section>
            <h3>标签</h3>
            {tags.map((t) => (
              <button
                key={t.id}
                className={`chip ${selectedTagIds.includes(t.id) ? 'active' : ''}`}
                onClick={() => toggleTagFilter(t.id)}
              >
                {t.name}
              </button>
            ))}
            {!tags.length && <p className="hint">尚无标签</p>}
          </section>
          <section>
            <h3>回顾</h3>
            <button className="side-item" onClick={() => setView({ name: 'review' })}>
              🔄 每日回顾
            </button>
            <label className="star-toggle">
              <input type="checkbox" checked={starredOnly} onChange={(e) => setStarredOnly(e.target.checked)} />
              只看星标
            </label>
          </section>
        </aside>

        <main className="main">
          {view.name === 'cards' && (
            <>
              <div className="wall-head">
                <h2>{query ? `搜索：“${query}”` : activeBook ? activeBook.title : '全部卡片'}</h2>
                <button className="fab" title="新建卡片" onClick={() => setCreating(true)}>＋</button>
              </div>
              <div className="wall">
                {cards.map((c) => (
                  <Card key={c.id} card={c} onEdit={() => setEditing(c)} onChanged={refreshMeta} onToast={showToast} />
                ))}
                {!cards.length && <p className="hint">没有匹配的卡片。</p>}
              </div>
            </>
          )}
          {view.name === 'review' && (
            <ReviewView onToast={showToast} onExit={() => setView({ name: 'cards', bookId: null })} />
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
            if (view.name === 'cards') {
              const filter: CardFilter = { ...emptyFilter(), bookId: view.bookId, tagIds: selectedTagIds, starredOnly };
              setCards(await call<CardRow[]>('query_cards', { filter, limit: 500, offset: 0 }));
            }
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
          }}
          onToast={showToast}
        />
      )}
      {toast && <div className="toast">{toast}</div>}
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
  const toggleStar = async () => {
    await call('toggle_starred', { id: card.id, starred: !card.starred }).catch((e) => onToast(String(e)));
    onChanged();
  };
  const remove = async () => {
    if (!window.confirm('删除这张卡片？')) return;
    await call('delete_card', { id: card.id }).catch((e) => onToast(String(e)));
    onChanged();
  };
  return (
    <article className={`card kind-${card.kind}`} style={{ borderLeft: `4px solid ${colorOf(card)}` }}>
      <div className="card-actions">
        <button className={card.starred ? 'starred' : ''} title="星标" onClick={toggleStar}>⭐</button>
        <button title="编辑" onClick={onEdit}>✎</button>
        <button title="删除" onClick={remove}>🗑</button>
      </div>
      <p className="card-text">{card.text}</p>
      {card.kind === 'thought' && card.abstractText && (
        <details className="abstract">
          <summary>原文</summary>
          <blockquote>{card.abstractText}</blockquote>
        </details>
      )}
      {card.note && <p className="card-note">✎ {card.note}</p>}
      {!!card.tags.length && (
        <div className="tag-row">{card.tags.map((t) => <span key={t} className="chip small">{t}</span>)}</div>
      )}
      {(card.bookTitle || card.chapterTitle) && (
        <footer className="card-foot">
          {[card.bookTitle, card.chapterTitle, fmtDate(card.createdAt)].filter(Boolean).join(' · ')}
        </footer>
      )}
    </article>
  );
}

// ---------- 编辑弹层 ----------

function EditModal({ card, onClose, onSaved, onToast }: {
  card: CardRow;
  onClose: () => void;
  onSaved: () => void;
  onToast: (m: string) => void;
}) {
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
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>{isSelf ? '编辑自建卡' : '补写想法'}</h3>
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
            <span key={t} className="chip small" onClick={() => removeTag(t)} title="点击移除">{t} ×</span>
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
          <button onClick={save}>保存</button>
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
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>新建卡片</h3>
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

// ---------- 复习视图 ----------

function ReviewView({ onToast, onExit }: { onToast: (m: string) => void; onExit: () => void }) {
  const [queue, setQueue] = useState<CardRow[]>([]);
  const [idx, setIdx] = useState(0);
  const [revealed, setRevealed] = useState(false);
  const [loading, setLoading] = useState(true);
  const loadingRef = useRef(false);

  useEffect(() => {
    if (loadingRef.current) return;
    loadingRef.current = true;
    call<CardRow[]>('get_due_cards', { limit: 30 })
      .then((c) => setQueue(c))
      .catch((e) => onToast(String(e)))
      .finally(() => setLoading(false));
  }, [onToast]);

  const grade = async (rating: string) => {
    const card = queue[idx];
    try {
      await call('grade_review', { cardId: card.id, rating });
      setRevealed(false);
      setIdx((i) => i + 1);
    } catch (e) {
      onToast(String(e));
    }
  };

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onExit();
      if (e.key === ' ') {
        e.preventDefault();
        if (!revealed) setRevealed(true);
      }
      if (revealed && ['1', '2', '3', '4'].includes(e.key)) {
        grade(['again', 'hard', 'good', 'easy'][Number(e.key) - 1]);
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  });

  if (loading) return <div className="review"><p className="hint">加载队列…</p></div>;
  if (idx >= queue.length)
    return (
      <div className="review">
        <h2>{queue.length ? '今日回顾完成 🎉' : '当前没有到期卡片'}</h2>
        <button onClick={onExit}>返回卡片墙</button>
      </div>
    );
  const card = queue[idx];
  return (
    <div className="review">
      <div className="review-top">
        <span>剩余 {queue.length - idx} 张</span>
        <button className="ghost" onClick={onExit}>退出 (Esc)</button>
      </div>
      <div className="review-card" onClick={() => !revealed && setRevealed(true)}>
        <footer className="card-foot">
          {[card.bookTitle, card.chapterTitle].filter(Boolean).join(' · ') || '自建卡'}
        </footer>
        {revealed ? (
          <>
            <p className="review-text">{card.text}</p>
            {card.abstractText && <blockquote>{card.abstractText}</blockquote>}
            {card.note && <p className="card-note">✎ {card.note}</p>}
          </>
        ) : (
          <p className="review-hint">按空格 / 点击翻面</p>
        )}
      </div>
      {revealed && (
        <div className="grades">
          <button className="g-again" onClick={() => grade('again')}>1 忘记</button>
          <button className="g-hard" onClick={() => grade('hard')}>2 困难</button>
          <button className="g-good" onClick={() => grade('good')}>3 记得</button>
          <button className="g-easy" onClick={() => grade('easy')}>4 简单</button>
        </div>
      )}
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
        <button onClick={onSync} disabled={syncing}>{syncing ? '同步中…' : '立即同步'}</button>
      </section>
      <section>
        <h3>关于</h3>
        <p className="hint">数据目录：{status?.dataDir ?? '未知'}（mudflat.db）</p>
        <p className="hint">纯本地存储 · 无账号 · 无云同步</p>
      </section>
    </div>
  );
}

---

# Layouts 说明

本项目无独立 layout 目录。`src/App.tsx`（上方为完整源码）同时承担：

- **App Shell**（`<div className="app">`）：`.topbar` 顶栏（logo、全局搜索、示例数据按钮[仅 dev]、同步按钮、设置按钮）+ `.body`（`.sidebar` 左侧栏 + `.main` 主区）
- **侧栏** `.sidebar`：三组 —— 书架（全部卡片 + 封面缩略书名 + 笔记数 badge）、标签（chip 多选）、回顾（每日回顾入口 + 只看星标开关）
- **主区三态**：cards（瀑布流 `.wall` columns:3）、review（`.review` 居中单卡）、settings（`.settings` 表单区）
- **弹层** `.modal-mask > .modal`（编辑/新建），**toast** `.toast` 底部居中

完整样式：`src/App.css`（见 theme.md）
