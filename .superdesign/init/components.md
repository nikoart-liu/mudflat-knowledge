# Components

本项目是 Tauri 2 + React 18 + TypeScript + Vite 单窗口应用，**无独立组件库、无 shadcn/AntD 等依赖**。所有 UI 组件集中在 `src/App.tsx` 内部定义（函数组件），样式为 `src/App.css` 手写原生 CSS（无 Tailwind）。

## src/App.tsx — 全部组件（单文件）

导出：`App`（根组件，view 状态机）。内部组件：

### App
根布局：顶栏 + 侧栏 + 主区（cards/review/settings 三态）。管理 books/tags/cards 查询、同步进度、编辑/新建弹层与 toast。

### Card
单张笔记卡片。highlight 左侧 4px 色条（colorStyle 1-5 → 浅色底色映射），thought 大字正文 + `<details>` 折叠原文，self 卡米黄底。右上 hover 显示 ⭐/✎/🗑 操作。

```tsx
function Card({ card, onEdit, onChanged, onToast }: {
  card: CardRow; onEdit: () => void; onChanged: () => void; onToast: (m: string) => void;
}) {
  const toggleStar = async () => { await call('toggle_starred', { id: card.id, starred: !card.starred }).catch((e) => onToast(String(e))); onChanged(); };
  const remove = async () => {
    if (!window.confirm('删除这张卡片？')) return;
    await call('delete_card', { id: card.id }).catch((e) => onToast(String(e))); onChanged();
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
        <details className="abstract"><summary>原文</summary><blockquote>{card.abstractText}</blockquote></details>
      )}
      {card.note && <p className="card-note">✎ {card.note}</p>}
      {!!card.tags.length && <div className="tag-row">{card.tags.map((t) => <span key={t} className="chip small">{t}</span>)}</div>}
      {(card.bookTitle || card.chapterTitle) && (
        <footer className="card-foot">{[card.bookTitle, card.chapterTitle, fmtDate(card.createdAt)].filter(Boolean).join(' · ')}</footer>
      )}
    </article>
  );
}
```

### EditModal
编辑弹层：self 卡编辑 text textarea；划线/想法卡展示原文引用块 + 补写想法 textarea；标签 chip 增删。

### CreateModal
新建自建卡弹层：正文 textarea + 标签输入（逗号分隔）。

### ReviewView
复习模式：单卡居中，初始遮内容只显书名·章节，空格/点击翻面，翻面后 1 忘记/2 困难/3 记得/4 简单 四键评分，Esc 退出，顶部显示剩余数。

### SettingsView
设置页：API Key 密码框 + 测试连接/保存到钥匙串/清除 + 手动同步 + 关于（数据目录）。

完整源码见 `layouts.md` 与仓库文件 `src/App.tsx`（组件同文件）。
