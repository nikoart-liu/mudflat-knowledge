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
  empty-title:
    fontFamily: '"Iowan Old Style", "Palatino", "Songti SC", "STSong", "Noto Serif SC", "SimSun", serif'
    fontSize: "22px"
    fontWeight: 700
    fontStyle: italic
    lineHeight: 1.3
  empty-step-title:
    fontFamily: '"Iowan Old Style", "Palatino", "Songti SC", "STSong", "Noto Serif SC", "SimSun", serif'
    fontSize: "16px"
    fontWeight: 700
    fontStyle: italic
    lineHeight: 1.3
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
1. **刊物隐喻即信息架构**：侧栏是目录索引（mono 序号 + 衬线书名），卡片墙是栏目版面（章节分隔行 + 三栏网格），卡片是刊物里的一条摘录，翻牌回顾是每日抽卡重读，设置是「版权页」。
2. **衬线是特权**：`--font-display`（Iowan Old Style / Palatino / 宋体族）只授予刊头、章节标题、书名、想法引文、编目大字、空态步名。正文、按钮永远是黑体；Key 输入是唯一的输入例外（版权页与创刊页同规）。
3. **线分两级**：`--editorial-line #27272a` 墨线（刊头底边、章节分隔、弹层边、复习卡边）与 `--hairline #d4d4d8` 栏线（卡片框、页脚线、分区线）。没有第三种线，没有阴影，没有渐变。
4. **直角即纸边**：全系统 border-radius: 0。纸边就是裁切的边。
5. **金是点睛**：`--ember #b45309` 只允许出现在——内容头眉标（当前刊物 / 创刊 / 总索引 / 星标专辑 / 检索结果）、目录检索范围眉标（`--ember-deep` 变体）、星标卡 3px 书脊线、到期徽标实底、进度条当前段、focus ring、删除 hover。单屏金色合计 ≤ 视觉面积 5%。
6. **页码感元数据**：日期、计数、章节一律 mono tabular；卡片页脚是「来源（左）+ 日期（右）」的页码行，压在栏线之下。
7. **中文排印细节**：prose 面（卡片正文、批注、翻牌正文、引文、空态正文）启用 `text-spacing-trim: trim-start`（行首开引号半角，中文出版物惯例）与 `hanging-punctuation: first allow-end`（行尾标点悬挂），均为渐进增强；居中且加字距的衬线文字（翻牌标题/范围行/编目眉标）必须用等值 `padding-left` 补偿尾随字距，否则视觉中心偏左。

## 2. 色彩
- 三阶纸面：`--paper #fdfcfb`（主表面）/ `--panel #f5f4f0`（侧栏、次级、进度条槽）/ `--selection #e9e7e2`（hover / 选中 / code）。
- 墨阶：`--ink #1c1917` 正文 / `--ink-muted #6b6b74` 辅助（paper 5.1:1、panel 4.8:1，双面过 AA）/ selection 底上用 `--ink-muted-deep #5f5f66` 保 AA。
- 身份底：自建卡 `--self-wash #f0f4f8`（蓝灰杂志插页）+ `--self-line #cbd5e1` 边。
- 语义色仅两处：破坏性 `--grade-again #b3372e`（确认按钮、标签删除 hover）；成功反馈 `--grade-good #3b7a4e`。
- 没有暗色变体；没有纯黑 #000 / 纯白 #fff 大面积（封面版框内衬允许 #fff）。

## 3. 组件规范
- **刊头（Topbar）**：52px；2px 墨线压底；logo mark 20px + 衬线字标 20px；检索为下划线输入（无边无底，focus 下划线转金）；主动作「同步」为墨实底直角钮。
- **目录（Sidebar）**：220px 纸板底 + 栏线；组眉标 mono 10px/0.18em；书行 = mono 两位序号 + 衬线 13px/700 书名（active 转斜体）+ mono 10px 分类小字（仅在置顶行出现，显示大类名——置顶区脱离分组，小字补回书架位置；树内书行紧贴眉标不重复）+ mono 计数，active 用 selection 底 + 2px 左墨轨（hover 只换底色、不碰边框——墨轨在任何指针状态下幸存）；全部行共享 40px 文本起点（icon 行 13px 图标 + 5px 补差，书行 18px 序号列）。书单层级 = 本期要目 + 置顶区 + 一级分类树：「本期要目」为近 7 天有新划线的书组成的虚拟索引组（0 本时退场；组内金方点退场，组眉标即信号），置顶区之前；「置顶 · N」眉标不封顶，计数让成本可见；分类树只有一级大类（2026-09-04 定稿：数据键「大类-子类」在展示层取首段，子类不出层）：大类眉标 mono 11px/0.14em（可点的栏目头，比组眉标大一阶以示可按；9px 汉字 mono 低于辨识下限），右缘合计计数**含置顶书**，按体量降序，书行直挂其下。眉标可点折叠（行首 mono 三角 ▾/▸ 示位，aria-expanded，← 收起 / → 展开；状态存本地记忆 `mudflat.collapsed-cats`，键为大类名，折叠只隐行、编目号不重排）；无分类归「未分类」挂尾。**目录号是终身编目号**：按入馆次序（DB id 序）分配，置顶/折叠/树内排序变化不改号，同一本书在要目与置顶区同号——跳号是编目页的样子，与复习卡背「编目大字」同源。近 7 天有新划线的书行名侧带 5px 实底金方点（语义同到期徽标）。范围受限检索时书架组顶出 mono 9px「检索范围 · …」金眉标（ember-deep，panel 底 6.4:1 AA），active 行在检索中保持点亮。键盘漫游：书单整体只占一个 Tab 停靠位（roving tabindex），↑↓ 循环移动、Home/End 跳端、0.8s 内连打可打印字符按书名前缀跳书（空格与 `/` 除外）。翻牌项到期计数为琥珀实底徽标（is-review 下侧栏整体退场，翻牌项不带 active 死状态）；星标行右缘计数 = 全馆星标总数；标签为墨线框字，hover 反白。
- **章节头（Content header）**：眉标 mono 11px/0.2em 琥珀（当前刊物/总索引/星标专辑/检索结果；馆空为「创刊」）+ 衬线 30px 标题（馆空为「尚未接上」）+ 衬线斜体作者 + mono 计数 chips（总计一枚墨底反白；书内「章节 N」为目次入口，载全后可点开直角墨框篇目——衬线斜体章名 + 右缘 mono 张数，按 chapterUid 升序、未分章垫底；点一章滚到该章分隔行，墙的「最近划过的章在前」分组顺序不动；Esc / 点外面 / 选中收起。本书有到期卡时加「翻牌 N」实底金徽标，点击直达本书翻牌；「线索」下划线钮打开主题脑图）+ 封面 80×110 墨框版框（内衬 3px 白边）+ 右侧动作区（mono「置顶/已置顶」钮 + mono「长读/版面」切换钮 + 40px 方章新建钮，hover 反白；馆空时动作区退下）。
- **线索（Mind map）**：书名直角墨框居中，一级主题沿椭圆均分、二级再外扩；墨线相连（中心 2px、枝 1.5px）。节点仍是直角纸边，衬线标题；点开抽屉看证据，不把划线铺成叶子。无圆角气泡、无彩虹枝。
- **卡片（Card）**：直角纸框 1px 栏线，hover 框线转墨；眉标 mono 9px（首个标签，否则卡别：划线/想法/编者按；星标卡眉标转金）；正文 14px/1.7 钳 5 行；**想法卡 = 衬线 16px/700 + 2px 发丝下划引文排法**；原文折叠 = 栏左 2px 细线斜体引文；批注素排钳 2 行；页脚 = 栏线 + 来源/日期 mono 页码行；星标 3px 琥珀左书脊。
- **卡片墙分组**：总索引按书分组（衬线书名分隔行）；**书内按章节分组**（衬线章名分隔行，章缺失归「未分章」）——用户进了一本书，关注的只是这一本，分隔行跟书的结构走，不跟日历月份走。
- **长读（Reading）**：卡片墙的单栏通读变体，为「重读一本书的笔记」服务。≤720px 单栏居中、钳行解除（通读不折叠）、卡片框退场只留底部栏线（自建卡栏线用 self-line）；星标由实心星钮与金字眉标承担，书脊线退场；章名分隔行即书脊。V 键或头部 mono 钮切换，选择存本地记忆。
- **翻牌回顾（Review）**：衬线 22px/0.3em「翻牌回顾」居中（进入/结算为扉页 24px 下边距；进行中收为栏眉 16px，贴页码行）。剩余张数 mono 眉标 + 衬线计数（tabular）；进度为 6px 连续墨条（当前段染金、余段纸板槽）；卡牌 2px 墨边 + 内衬栏线框，背面衬线编目大字 88px、底缘 mono「空格」示能；翻面 rotateY 420ms，飞出 240ms。评「忘了 / 困难」后若有回忆支架：原卡收成约 220px 高（行宽不变，长文卡内自滚），下方「换个角度」释义与邻卡自滚、不钳行；「再讲一句 / 下一张」跟在文后，文长时钉在可视底。翻牌可整馆或按书取卡（本书翻牌）：按书时标题下加 mono 范围行「本书 · 书名」，文案以本书为主语（本书到期 / 这本书当前没有到期卡片），入口在章节头「翻牌 N」徽标。翻牌时侧栏、检索、同步与刊头动作都退下，避免齿轮卸掉进行中的批次。
- **版权页（Settings）**：衬线 28px 标题 + 琥珀下划；分区以栏线起头，mono 标题「一、二、三、四、五、」；Key 输入 = 墨框直角 + 衬线 15px + focus 金圈。
- **弹层**：直角 1px 墨边，衬线标题，输入 focus 金圈；破坏性动作为社论红实底。
- **Toast**：墨底纸字 mono，直角，底部居中。首次把空馆填上时改口「接到墙上：…从左边目录挑一本。」，不做弹层庆祝。
- **空态 / 创刊页**：无独立欢迎屏、无样例卡、无导览。馆空无 Key 时墙面是发刊词——与章节头左缘对齐的栏宽（≤34rem），衬线斜体 22px 标题 + 正文 14px/1.7 说明为何，下接两步目录（mono 序号 01/02 + 衬线斜体步名，墨线起头）：去签发 Key（外链微信读书 Skills 开通页）与贴到本机（墨框衬线 15px 输入，focus 金圈；主动作「保存并同步」，次动作「只保存」）。有 Key 无书则只留同步。检索/筛选空态给一句原因 + 清除动作。侧栏「刊物」无书时一行提示。翻牌无到期卡时补一句「同步之后，到期的划线会排进今日队列。」不把人送到版权页去贴 Key。

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
