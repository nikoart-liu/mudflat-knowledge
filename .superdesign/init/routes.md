# Routes

无路由库（react-router 未引入）。`App.tsx` 用全局 `view` state 切换三个视图：

| view 值 | 内容 | 组件 |
|---|---|---|
| `{name:'cards', bookId}` | 卡片墙（按书/标签/星标筛选 + 搜索） | `App` 主区 cards 分支 + `Card` + `EditModal`/`CreateModal` |
| `{name:'review'}` | 每日回顾抽卡（SRS 评分） | `ReviewView` |
| `{name:'settings'}` | API Key / 同步 / 关于 | `SettingsView` |

入口：`index.html` → `src/main.tsx` → `App`。窗口 1200×800（min 900×600），Tauri 2。

页面说明：
- **卡片墙**：筛选状态下 `query_cards`；搜索框 debounce 250ms 走 `search_cards`；顶部右上浮动 ＋ 新建自建卡。
- **回顾**：`get_due_cards(30)` 队列，空格翻面 → 四键评分 `grade_review`。
- **设置**：`get_settings` 显示上次同步时间与数据目录；`test_connection` / `save_api_key` / `clear_api_key`。
