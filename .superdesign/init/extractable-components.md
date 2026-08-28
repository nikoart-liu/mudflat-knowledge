# Extractable Components

单文件小应用，布局组件全部内嵌在 `src/App.tsx`，按块提取：

## TopBar
- Source: `src/App.tsx`（header.topbar JSX 块）
- Category: layout
- Description: 顶栏 —— logo「📚 泥滩知识」+ 全局搜索框 + dev 示例数据按钮 + 同步按钮 + 设置按钮
- Extractable props: syncingLabel (string, default: "同步"), searchPlaceholder
- Hardcoded: 📚 emoji logo、按钮文案、全部 CSS

## Sidebar
- Source: `src/App.tsx`（aside.sidebar JSX 块）
- Category: layout
- Description: 左侧栏三组 —— 书架（全部卡片 + 书列表含封面/书名/计数 badge）、标签 chips、回顾（每日回顾 + 只看星标）
- Extractable props: activeBookId (number|null), selectedTagIds (number[]), starredOnly (boolean)
- Hardcoded: 「全部卡片」文案、🔄 emoji、分组标题、CSS

## Card
- Source: `src/App.tsx`（Card 函数组件）
- Category: basic
- Description: 笔记卡 —— kind 三态（highlight 色条/thought 大字+原文折叠/self 米黄），note 斜体缩进，标签 chips，hover 操作（星标/编辑/删除），书名·章节·日期页脚
- Extractable props: 无（数据驱动 card: CardRow）
- Hardcoded: ⭐✎🗑 emoji、colorStyle 色映射、CSS

## ReviewCard
- Source: `src/App.tsx`（ReviewView JSX）
- Category: basic
- Description: 复习单卡居中排版 + 翻面四评分键
- Extractable props: revealed (boolean), remaining (number)
- Hardcoded: 评分键文案与配色类 g-again/g-hard/g-good/g-easy

## Modal（编辑/新建共用壳）
- Source: `src/App.tsx`（EditModal / CreateModal）
- Category: basic
- Description: 遮罩 + 圆角弹层，textarea/输入 + 操作按钮排
- Extractable props: title (string)
- Hardcoded: CSS

基础 Button/Input/chip 为内联样式类，按规范跳过提取。
