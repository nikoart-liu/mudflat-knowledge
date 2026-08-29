# Design System — Mudflat Knowledge 泥滩知识

## 产品上下文
桌面端「卡片笔记」应用（Tauri 2 + React 19）：微信读书的划线与想法变成卡片。三个视图：卡片墙（章节分组 + 搜索 + 继续载入）、翻牌回顾（清样校对，翻面即记 Good）、设置（API Key/同步/关于）。用户场景：长时间阅读与整理中文长文本，重读轻写、内容为王。桌面窗口 1200×800（min 900×600）。全中文 UI。

## 当前风格（reproduction 基准 = v2 已实现「刊物排印」）
规范来源 DESIGN.md（normative）；实现 src/App.css。视觉语言：中文文学刊物内页。
- 三阶纸面：`--paper #fdfcfb` / `--panel #f5f4f0`（侧栏/次级）/ `--selection #e9e7e2`（hover/选中）
- 两级线：`--editorial-line #27272a` 墨线（刊头 2px 底边、章节分隔、弹层与复习卡边）+ `--hairline #d4d4d8` 栏线（卡片框、页脚线）；全平面、全直角（radius 0）、无阴影无渐变
- 墨阶：`--ink #1c1917` / `--ink-muted #71717a`；自建卡身份底 `--self-wash #f0f4f8` + `--self-line #cbd5e1`
- 琥珀点睛金 `--ember #b45309`：内容头眉标、星标 3px 书脊线、到期徽标实底、进度当前段、focus ring、删除 hover —— 预算 ≤5%
- 字体三轨：`--font-display`（Iowan Old Style/Palatino/宋体族衬线：刊头 20px、章节头 30px、章名斜体 18px、想法引文 16px/700+发丝下划、编目号 88px）/ `--font-body`（黑体：正文 14px/1.7、阅读级 19px/1.9）/ `--font-mono`（眉标 9-11px/0.12-0.2em、页码日期计数 tabular）
- 刊头：52px + 2px 墨线；logo-mark.svg 20px + 衬线字标；检索为下划线输入；同步 = 墨实底直角钮
- 目录侧栏：组眉标 mono 大写；书行 = mono 两位序号 + 衬线书名（active 斜体）+ mono 计数；标签墨线框字 hover 反白
- 卡片墙：grid 三栏（≤1100px 两栏）；章节分隔行 = 衬线斜体章名 + 墨线通栏 + mono 计数；卡片眉标 mono 9px（首个标签否则卡别：划线/想法/编者按）；页脚 = 栏线 + 来源/日期页码行
- 清样回顾：衬线 22px/0.3em 标题；6px 连续墨条进度（当前段染金）；卡牌 2px 墨边 + 内衬栏线框 + 衬线编目大字
- 版权页设置：衬线 28px 标题 + 琥珀下划；分区 mono 标题「一、二、三、」；Key 输入墨框衬线 + focus 金圈
- 图标：内嵌 SVG 线性图标（stroke 2，lucide 风）；logo 为 `/logo-mark.svg`「潮线索引卡」mark
- highlight colorStyle 1-5 数据字段存在（types.ts CardRow）但 UI 不渲染五色（v1 起已移除，v2 延续）

## 约束
- 全中文 UI；数据结构（CardRow 字段）不变；三视图 + 弹层结构保留
- 信息密度与长文排版是核心体验（正文 ≥14px/1.7，回顾 19px/1.9）
- 桌面端 1200×800 常驻；键盘可达（/ 聚焦搜索、空格翻卡、Esc 退出）
