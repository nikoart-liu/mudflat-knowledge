# Extractable Components

当前实现（v1 瑞士网格）布局组件全部内嵌在 `src/App.tsx`（1127 行），按块提取。行号为当前文件真实位置。

## TopBar
- Source: `src/App.tsx:306:342`（header.topbar JSX 块）
- Category: layout
- Description: 顶栏 —— logo-mark.svg「潮线索引卡」+「泥滩知识」字标 + 全局搜索（放大镜图标 + placeholder「检索全部卡片…」）+ 同步主动作（primary 褐墨实底，同步中显示进度文案）+ 设置 ghost 图标按钮
- Extractable props: syncingLabel (string, default "同步"), searchPlaceholder, settingsActive (boolean)
- Hardcoded: 文案与全部 CSS

## Sidebar
- Source: `src/App.tsx:345:419`（aside.sidebar JSX 块）
- Category: layout
- Description: 左侧栏三区 —— 固定「回顾」区（翻牌 CTA，due>0 时琥珀提示 + mono 计数）/ 滚动「书架」区（全部卡片 + 只看星标 + 书列表：26×36 封面 + 书名 + mono 计数）/ 固定「标签」区（chips + 删除 ×）
- Extractable props: activeBookId (number|null), selectedTagIds (number[]), starredOnly (boolean), dueCount (number)
- Hardcoded: 分组眉标「书架/标签」、文案、CSS

## Card
- Source: `src/App.tsx:619:703`（Card 函数组件）
- Category: basic
- Description: 笔记卡 —— kind 三态（highlight 默认 paper 底 / thought 16px 600 强调正文 / self+starred ember-wash 晕染），溢出展开交互，abstract 折叠原文，note，标签 chips + 来源行 + mono 日期，hover 星标/编辑/删除
- Extractable props: 无（数据驱动 card: CardRow）

## ReviewDeck
- Source: `src/App.tsx:874:1039`（ReviewView）
- Category: basic
- Description: 翻牌回顾 —— 编目卡牌扇形堆叠、翻面 rotateY、进度打孔刻度、四键评分
- Extractable props: revealed (boolean), remaining (number)

## Modal（确认/编辑/新建共用壳）
- Source: `src/App.tsx:735:873`（ConfirmModal / EditModal / CreateModal）
- Category: basic
- Description: hairline-strong 边框弹层 + 遮罩 rgba(35,32,28,0.45)，表单与操作按钮排
- Extractable props: title (string), message (string)

基础 Button/Input/chip 为内联样式类，按规范跳过提取。
