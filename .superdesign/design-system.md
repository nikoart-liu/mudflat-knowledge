# Design System — Mudflat Knowledge 泥滩知识

## 产品上下文
桌面端「卡片笔记」应用：微信读书的划线与想法变成卡片。核心页面：卡片墙（瀑布流）、每日回顾（SRS 抽卡）、设置（API Key/同步）。用户场景：长时间阅读与整理中文长文本，重读轻写、内容为王。桌面窗口 1200×800。

## 当前风格（reproduction 基准）
- 定位：暖灰纸感 + solarized 金强调的浅色单主题
- 色彩：`--bg #f7f6f3` / `--panel #fff` / `--line #e5e2dc` / `--ink #2d2a26` / `--muted #8a857c` / `--accent #b58900` / `--accent-soft #fdf6e3`
- highlight 色条映射：1 绿 #d4edda、2 紫 #e2d5f1、3 黄 #fff3cd、4 蓝 #d1ecf1、5 红 #f8d7da
- 字体：系统 PingFang SC 栈；正文 13-13.5px/1.75；卡片 thought 正文 15px medium
- 形状：按钮/输入 8px 圆角、卡片 10px、弹层 12px；阴影极轻 `0 1px 3px rgba(0,0,0,.06)`
- 图标：emoji（📚 ⭐ ✎ 🗑 🔄 ＋）
- 布局：顶栏 48px + 左侧栏 230px + 主区瀑布流 columns:3（≤1100px 两列）

## 用户痛点（本轮迭代方向）
当前界面「太 AI 风」：emoji 当图标、千篇一律的浅底圆角卡片、系统默认字体、缺乏阅读产品的书卷气与编辑感。重设计需摆脱模板感，建立有性格的视觉语言。

## 约束
- 全中文 UI；数据结构（CardRow 字段）不变；三视图 + 弹层结构保留
- 信息密度优先：单屏尽量多卡；中长文本排版是核心体验
- highlight 五色语义保留（用户已有颜色习惯）
