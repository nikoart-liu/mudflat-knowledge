:root {
  --bg: #f7f6f3;
  --panel: #ffffff;
  --line: #e5e2dc;
  --ink: #2d2a26;
  --muted: #8a857c;
  --accent: #b58900;
  --accent-soft: #fdf6e3;
  font-family: "PingFang SC", "Hiragino Sans GB", system-ui, sans-serif;
}

* { box-sizing: border-box; }
html, body, #root { margin: 0; height: 100%; background: var(--bg); color: var(--ink); }

.app { display: flex; flex-direction: column; height: 100vh; }

/* ---------- 顶栏 ---------- */
.topbar {
  display: flex; align-items: center; gap: 12px;
  padding: 10px 16px; background: var(--panel);
  border-bottom: 1px solid var(--line); flex-shrink: 0;
}
.logo { font-weight: 700; font-size: 15px; white-space: nowrap; }
.search {
  flex: 1; max-width: 420px; padding: 7px 12px;
  border: 1px solid var(--line); border-radius: 8px;
  font-size: 13px; background: var(--bg);
}
.search:focus { outline: none; border-color: var(--accent); }

button {
  padding: 6px 14px; border: 1px solid var(--line); border-radius: 8px;
  background: var(--panel); color: var(--ink); cursor: pointer; font-size: 13px;
}
button:hover:not(:disabled) { border-color: var(--accent); }
button:disabled { opacity: 0.5; cursor: default; }
button.ghost { background: transparent; border-color: transparent; color: var(--muted); }
button.ghost:hover { color: var(--ink); }
button.ghost.active { color: var(--accent); }

/* ---------- 布局 ---------- */
.body { display: flex; flex: 1; min-height: 0; }
.sidebar {
  width: 230px; overflow-y: auto; padding: 12px;
  border-right: 1px solid var(--line); background: var(--panel); flex-shrink: 0;
}
.sidebar h3 { font-size: 11px; color: var(--muted); text-transform: uppercase; letter-spacing: .08em; margin: 14px 4px 6px; }
.side-item {
  display: flex; align-items: center; gap: 8px; width: 100%;
  text-align: left; padding: 6px 8px; margin-bottom: 2px;
  border: none; background: transparent; border-radius: 6px;
}
.side-item:hover { background: var(--accent-soft); }
.side-item.active { background: var(--accent-soft); color: var(--accent); }
.side-item img, .cover-ph {
  width: 26px; height: 36px; object-fit: cover; border-radius: 3px;
  background: var(--line); flex-shrink: 0;
}
.side-title { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13px; }
.badge { font-size: 11px; color: var(--muted); }
.hint { font-size: 12px; color: var(--muted); line-height: 1.6; }
.star-toggle { display: flex; gap: 6px; align-items: center; font-size: 12px; color: var(--muted); margin-top: 8px; cursor: pointer; }

.chip {
  display: inline-block; padding: 3px 10px; margin: 0 4px 4px 0;
  border: 1px solid var(--line); border-radius: 999px; font-size: 12px;
  background: var(--panel); cursor: pointer;
}
.chip.active { background: var(--accent); color: #fff; border-color: var(--accent); }
.chip.small { padding: 1px 8px; font-size: 11px; cursor: default; }

/* ---------- 主区：瀑布流 ---------- */
.main { flex: 1; overflow-y: auto; padding: 16px 22px 80px; }
.wall-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
.wall-head h2 { font-size: 17px; margin: 0; }
.fab {
  width: 34px; height: 34px; border-radius: 50%;
  font-size: 20px; line-height: 1; display: grid; place-items: center;
  background: var(--accent); color: #fff; border: none;
}
.wall { columns: 3; column-gap: 14px; }
@media (max-width: 1100px) { .wall { columns: 2; } }

.card {
  break-inside: avoid; margin-bottom: 14px; position: relative;
  background: var(--panel); border-radius: 10px;
  padding: 14px 14px 10px; border-left: 4px solid #ccc;
  box-shadow: 0 1px 3px rgba(0,0,0,.06);
}
.card.kind-thought .card-text { font-size: 15px; font-weight: 500; }
.card.kind-self { background: var(--accent-soft); }
.card-text { margin: 0; font-size: 13.5px; line-height: 1.75; white-space: pre-wrap; }
.card-note { margin: 8px 0 0; font-style: italic; color: #6b6255; font-size: 13px; padding-left: 10px; border-left: 2px solid var(--line); }
.card-actions { position: absolute; top: 8px; right: 8px; display: none; gap: 2px; }
.card:hover .card-actions { display: flex; }
.card-actions button { border: none; background: transparent; font-size: 14px; padding: 3px 5px; }
.card-actions button.starred { filter: saturate(2); }
.card-foot { margin-top: 10px; font-size: 11px; color: var(--muted); }
.tag-row { margin-top: 6px; }
.abstract summary { font-size: 12px; color: var(--accent); cursor: pointer; margin-top: 6px; }
.abstract blockquote {
  margin: 6px 0; padding: 8px 12px; font-size: 13px; color: #55503f;
  background: var(--bg); border-left: 3px solid var(--accent); border-radius: 4px;
}

/* ---------- 弹层 ---------- */
.modal-mask {
  position: fixed; inset: 0; background: rgba(40,35,28,.4);
  display: grid; place-items: center; z-index: 50;
}
.modal {
  width: min(560px, 90vw); background: var(--panel); border-radius: 12px;
  padding: 18px; box-shadow: 0 10px 40px rgba(0,0,0,.2);
}
.modal h3 { margin: 0 0 12px; font-size: 15px; }
.modal textarea, .modal input[type="password"], .modal input:not([type]) {
  width: 100%; padding: 9px 12px; border: 1px solid var(--line);
  border-radius: 8px; font-size: 13px; resize: vertical; font-family: inherit;
  margin-bottom: 10px; background: var(--bg);
}
.modal textarea:focus, .modal input:focus { outline: none; border-color: var(--accent); }
.quote-box { font-size: 13px; color: #55503f; background: var(--bg); padding: 10px 12px; border-radius: 8px; border-left: 3px solid var(--accent); }
.modal-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 10px; }
.tag-editor { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
.tag-editor input { width: auto !important; flex: 1; min-width: 140px; margin-bottom: 0 !important; }

.toast {
  position: fixed; bottom: 24px; left: 50%; transform: translateX(-50%);
  background: #33302b; color: #fff; padding: 10px 18px; border-radius: 8px;
  font-size: 13px; z-index: 99; max-width: 70vw;
}
.err { color: #b3372e; } .ok { color: #3e7d47; }
.row { display: flex; gap: 8px; }

/* ---------- 复习 ---------- */
.review { display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; max-width: 720px; margin: 0 auto; }
.review-top { width: 100%; display: flex; justify-content: space-between; color: var(--muted); font-size: 13px; }
.review-card {
  width: 100%; min-height: 300px; margin: 20px 0; padding: 28px;
  background: var(--panel); border-radius: 14px; box-shadow: 0 2px 10px rgba(0,0,0,.07);
  display: flex; flex-direction: column; justify-content: center; cursor: pointer;
}
.review-text { font-size: 19px; line-height: 2; white-space: pre-wrap; margin: 14px 0 0; }
.review-hint { color: var(--muted); text-align: center; font-size: 14px; }
.review-card blockquote {
  margin: 14px 0 0; padding: 10px 14px; background: var(--bg);
  border-left: 3px solid var(--accent); border-radius: 6px; color: #55503f;
}
.grades { display: flex; gap: 10px; width: 100%; }
.grades button { flex: 1; padding: 12px 0; font-size: 14px; border-radius: 10px; }
.g-again { border-color: #d9534f !important; color: #d9534f; }
.g-hard { border-color: #f0ad4e !important; color: #b97a1c; }
.g-good { border-color: #5cb85c !important; color: #3e7d47; }
.g-easy { border-color: #5bc0de !important; color: #2c88a8; }

/* ---------- 设置 ---------- */
.settings { max-width: 640px; margin: 0 auto; }
.settings h2 { font-size: 17px; }
.settings section { background: var(--panel); border-radius: 12px; padding: 16px 18px; margin-bottom: 16px; }
.settings h3 { margin: 0 0 8px; font-size: 14px; }
.settings input[type="password"] {
  width: 100%; padding: 9px 12px; border: 1px solid var(--line);
  border-radius: 8px; font-size: 13px; background: var(--bg);
  font-family: ui-monospace, monospace;
}
.settings input:focus { outline: none; border-color: var(--accent); }
code { background: var(--bg); padding: 1px 5px; border-radius: 4px; font-size: 12px; }

---

# 颜色映射（App.tsx 内 colorOf/COLOR_MAP）

highlight `colorStyle` → 卡片左侧 4px 色条：
1 绿 #d4edda、2 紫 #e2d5f1、3 黄 #fff3cd、4 蓝 #d1ecf1、5 红 #f8d7da，其余 #e9ecef 灰。

# 设计令牌速览（theme 变量见上方 App.css :root）

- 背景 `--bg: #f7f6f3` 暖灰纸感；面板 `--panel: #ffffff`；描边 `--line: #e5e2dc`
- 墨色 `--ink: #2d2a26`；次级 `--muted: #8a857c`；强调 `--accent: #b58900`（solarized 金）
- 强调底 `--accent-soft: #fdf6e3`
- 字体：系统 PingFang SC 栈；正文 13-13.5px；行高 1.75
- 圆角：按钮/输入 8px、卡片 10px、弹层 12px；阴影 `0 1px 3px rgba(0,0,0,.06)`
