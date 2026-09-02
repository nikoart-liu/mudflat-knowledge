---
name: 滩涂拾遗
description: 桌面端卡片笔记应用：微信读书划线与想法的本地检索与回顾。v2「刊物排印」——把卡片墙做成一本中文文学刊物的内页。
colors:
  paper: "#fdfcfb"
  panel: "#f5f4f0"
  selection: "#e9e7e2"
  ink: "#1c1917"
  ink-muted: "#6b6b74"
  ink-muted-deep: "#5f5f66"
  editorial-line: "#27272a"
  hairline: "#d4d4d8"
  ember: "#b45309"
  ember-deep: "#92400e"
  ember-text: "#9a4a08"
  self-wash: "#f0f4f8"
  self-line: "#cbd5e1"
  grade-again: "#b3372e"
  grade-again-deep: "#992f27"
  grade-good: "#3b7a4e"
typography:
  masthead:
    fontFamily: '"Iowan Old Style", "Palatino", "Songti SC", "STSong", "Noto Serif SC", "SimSun", serif'
    fontSize: "20px"
    fontWeight: 700
  section-title:
    fontFamily: '"Iowan Old Style", "Palatino", "Songti SC", "STSong", "Noto Serif SC", "SimSun", serif'
    fontSize: "30px"
    fontWeight: 700
    lineHeight: 1.1
  settings-title:
    fontFamily: '"Iowan Old Style", "Palatino", "Songti SC", "STSong", "Noto Serif SC", "SimSun", serif'
    fontSize: "28px"
    fontWeight: 700
  review-title:
    fontFamily: '"Iowan Old Style", "Palatino", "Songti SC", "STSong", "Noto Serif SC", "SimSun", serif'
    fontSize: "22px"
    fontWeight: 700
    letterSpacing: "0.3em"
  group-label:
    fontFamily: '"Iowan Old Style", "Palatino", "Songti SC", "STSong", "Noto Serif SC", "SimSun", serif'
    fontSize: "18px"
    fontWeight: 700
    fontStyle: italic
  card-quote:
    fontFamily: '"Iowan Old Style", "Palatino", "Songti SC", "STSong", "Noto Serif SC", "SimSun", serif'
    fontSize: "16px"
    fontWeight: 700
    lineHeight: 1.6
    textDecoration: "underline 2px hairline, offset 5px"
  key-input:
    fontFamily: '"Iowan Old Style", "Palatino", "Songti SC", "STSong", "Noto Serif SC", "SimSun", serif'
    fontSize: "15px"
    fontWeight: 400
  reading:
    fontFamily: "-apple-system, PingFang SC, Hiragino Sans GB, system-ui, sans-serif"
    fontSize: "19px"
    fontWeight: 400
    lineHeight: 1.9
  body:
    fontFamily: "-apple-system, PingFang SC, Hiragino Sans GB, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.7
  ui:
    fontFamily: "-apple-system, PingFang SC, Hiragino Sans GB, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 400
  interface:
    fontFamily: "-apple-system, PingFang SC, Hiragino Sans GB, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 400
  small:
    fontFamily: "-apple-system, PingFang SC, Hiragino Sans GB, system-ui, sans-serif"
    fontSize: "11px"
    fontWeight: 400
  eyebrow:
    fontFamily: 'ui-monospace, "SF Mono", Menlo, monospace'
    fontSize: "9px"
    fontWeight: 700
    letterSpacing: "0.12em"
  mono-meta:
    fontFamily: 'ui-monospace, "SF Mono", Menlo, monospace'
    fontSize: "10px"
    fontWeight: 400
    lineHeight: 1.5
  catalog-number:
    fontFamily: '"Iowan Old Style", "Palatino", "Songti SC", "STSong", "Noto Serif SC", "SimSun", serif'
    fontSize: "88px"
    fontWeight: 700
---

# 滩涂拾遗 v2 —「刊物排印」(The Literary Gazette) 规范

> 本文件是 normative 规范，与 `src/App.css` 同步演进。v1「藏书人的索引柜（瑞士网格×暖纸）」自 v2 起废止。
> 设计稿：superdesign 项目 3c3a70c2 方向B（draft c767015d-8a62-4c26-889e-af3418d5c7dc）。

## 0. 一句话
把应用排成一本中文文学刊物的内页：**宋体衬线管标题，黑体管正文，mono 管页码**；结构由墨线与栏线承担，全平面、全直角；琥珀金只作点睛，不做装饰面。

## 1. 设计原则
1. **刊物隐喻即信息架构**：侧栏是目录索引（mono 序号 + 衬线书名），卡片墙是栏目版面（章节分隔行 + 三栏网格），卡片是刊物里的一条摘录，翻牌回顾是「清样校对」，设置是「版权页」。
2. **衬线是特权**：`--font-display`（Iowan Old Style / Palatino / 宋体族）只授予刊头、章节标题、书名、想法引文、编目大字。正文、按钮、输入永远是黑体。
3. **线分两级**：`--editorial-line #27272a` 墨线（刊头底边、章节分隔、弹层边、复习卡边）与 `--hairline #d4d4d8` 栏线（卡片框、页脚线、分区线）。没有第三种线，没有阴影，没有渐变。
4. **直角即纸边**：全系统 border-radius: 0。纸边就是裁切的边。
5. **金是点睛**：`--ember #b45309` 只允许出现在——内容头眉标「当前刊物」、星标卡 3px 书脊线、到期徽标实底、进度条当前段、focus ring、删除 hover。单屏金色合计 ≤ 视觉面积 5%。
6. **页码感元数据**：日期、计数、章节一律 mono tabular；卡片页脚是「来源（左）+ 日期（右）」的页码行，压在栏线之下。
7. **中文排印细节**：prose 面（卡片正文、批注、清样正文、引文、空态正文）启用 `text-spacing-trim: trim-start`（行首开引号半角，中文出版物惯例）与 `hanging-punctuation: first allow-end`（行尾标点悬挂），均为渐进增强；居中且加字距的衬线文字（清样标题/范围行/编目眉标）必须用等值 `padding-left` 补偿尾随字距，否则视觉中心偏左。

## 2. 色彩
- 三阶纸面：`--paper #fdfcfb`（主表面）/ `--panel #f5f4f0`（侧栏、次级、进度条槽）/ `--selection #e9e7e2`（hover / 选中 / code）。
- 墨阶：`--ink #1c1917` 正文 / `--ink-muted #6b6b74` 辅助（paper 5.1:1、panel 4.8:1，双面过 AA）/ selection 底上用 `--ink-muted-deep #5f5f66` 保 AA。
- 身份底：自建卡 `--self-wash #f0f4f8`（蓝灰杂志插页）+ `--self-line #cbd5e1` 边。
- 语义色仅两处：破坏性 `--grade-again #b3372e`（确认按钮、标签删除 hover）；成功反馈 `--grade-good #3b7a4e`。
- 没有暗色变体；没有纯黑 #000 / 纯白 #fff 大面积（封面版框内衬允许 #fff）。

## 3. 组件规范
- **刊头（Topbar）**：52px；2px 墨线压底；logo mark 20px + 衬线字标 20px；检索为下划线输入（无边无底，focus 下划线转金）；主动作「同步」为墨实底直角钮。
- **目录（Sidebar）**：220px 纸板底 + 栏线；组眉标 mono 10px/0.18em；书行 = mono 两位序号 + 衬线书名（active 转斜体）+ mono 计数，active 用 selection 底 + 2px 左墨轨；翻牌项到期计数为琥珀实底徽标（全刊唯一实底金块）；标签为墨线框字，hover 反白。
- **章节头（Content header）**：眉标 mono 11px/0.2em 琥珀（当前刊物/总索引/星标专辑/检索结果）+ 衬线 30px 标题 + 衬线斜体作者 + mono 计数 chips（总计一枚墨底反白；书内加「章节 N」一枚，本书有到期卡时加「翻牌 N」实底金徽标，点击直达本书清样；「线索」下划线钮打开主题脑图）+ 封面 80×110 墨框版框（内衬 3px 白边）+ 右侧动作区（mono「长读/版面」切换钮 + 40px 方章新建钮，hover 反白）。
- **线索（Mind map）**：书名直角墨框居中，一级主题沿椭圆均分、二级再外扩；墨线相连（中心 2px、枝 1.5px）。节点仍是直角纸边，衬线标题；点开抽屉看证据，不把划线铺成叶子。无圆角气泡、无彩虹枝。
- **卡片（Card）**：直角纸框 1px 栏线，hover 框线转墨；眉标 mono 9px（首个标签，否则卡别：划线/想法/编者按；星标卡眉标转金）；正文 14px/1.7 钳 5 行；**想法卡 = 衬线 16px/700 + 2px 发丝下划引文排法**；原文折叠 = 栏左 2px 细线斜体引文；批注素排钳 2 行；页脚 = 栏线 + 来源/日期 mono 页码行；星标 3px 琥珀左书脊。
- **卡片墙分组**：总索引按书分组（衬线书名分隔行）；**书内按章节分组**（衬线章名分隔行，章缺失归「未分章」）——用户进了一本书，关注的只是这一本，分隔行跟书的结构走，不跟日历月份走。
- **长读（Reading）**：卡片墙的单栏通读变体，为「重读一本书的笔记」服务。≤720px 单栏居中、钳行解除（通读不折叠）、卡片框退场只留底部栏线（自建卡栏线用 self-line）；星标由实心星钮与金字眉标承担，书脊线退场；章名分隔行即书脊。V 键或头部 mono 钮切换，选择存本地记忆。
- **清样（Review）**：衬线 22px/0.3em「清样 · 翻牌回顾」居中；剩余张数 mono 眉标 + 衬线计数；进度为 6px 连续墨条（当前段染金、余段纸板槽）；卡牌 2px 墨边 + 内衬栏线框，背面衬线编目大字 88px；翻面 rotateY 420ms，飞出 240ms。清样可整馆或按书取卡（本书清样）：按书时标题下加 mono 范围行「本书 · 书名」，文案以本书为主语（本书到期 / 这本书当前没有到期卡片），入口在章节头「翻牌 N」徽标。
- **版权页（Settings）**：衬线 28px 标题 + 琥珀下划；分区以栏线起头，mono 标题「一、二、三、四、五、」；Key 输入 = 墨框直角 + 衬线 15px + focus 金圈。
- **弹层**：直角 1px 墨边，衬线标题，输入 focus 金圈；破坏性动作为社论红实底。
- **Toast**：墨底纸字 mono，直角，底部居中。

## 4. Do's and Don'ts
### Do
- Do 让全部结构落在 paper/panel/selection 三阶纸面 + 两级线上。
- Do 保住中文阅读参数：正文 ≥14px/1.7，回顾阅读 19px/1.9，行长可控。
- Do 把 mono 用在一切计数与日期（tabular-nums），书名/章名用衬线斜体制造目录感。
- Do 尊重 prefers-reduced-motion；全站动效只有 hover 换色、翻面与飞出。
### Don't
- Don't 出现 emoji 图标、圆角 >0、软阴影、渐变、玻璃拟态、暗色主题。
- Don't 用英文 UI 标签（mono 眉标里的拉丁词只允许 API Key / Esc 等术语）。
- Don't 让正文离开黑体：衬线只给标题、书名、引语、编目号。
- Don't 增加第三种强调色；金色只花在预算清单内的位置。
- Don't 用大于 3px 的彩色侧边条（星标书脊 3px 是唯一例外）。
