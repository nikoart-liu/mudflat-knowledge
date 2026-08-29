---
name: Mudflat Knowledge
description: 桌面端卡片笔记应用：微信读书划线与想法的本地检索与回顾，书卷编辑气质的瑞士网格工具
colors:
  paper: "#f8f5f1"
  panel: "#efece7"
  selection: "#ebe7e0"
  hairline: "#c7c4bc"
  hairline-strong: "#a9a49c"
  ink: "#23201c"
  umber: "#3a3227"
  umber-deep: "#2e2820"
  ink-strong: "#13110e"
  ink-muted: "#6d6860"
  ink-muted-deep: "#666059"
  ember-amber: "#b57f12"
  ember-deep: "#996605"
  ember-text: "#8a5f05"
  ember-wash: "#f8f0db"
  ember-wash-hover: "#f2e8cf"
  grade-again: "#bd4238"
  grade-again-deep: "#ae3d34"
  grade-good: "#3b834e"
  grade-easy: "#32669a"
typography:
  reading:
    fontFamily: "-apple-system, PingFang SC, Hiragino Sans GB, system-ui, sans-serif"
    fontSize: "19px"
    fontWeight: 400
    lineHeight: 1.9
    letterSpacing: "normal"
  emphasized-body:
    fontFamily: "-apple-system, PingFang SC, Hiragino Sans GB, system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 600
    lineHeight: 1.55
    letterSpacing: "normal"
  body:
    fontFamily: "-apple-system, PingFang SC, Hiragino Sans GB, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.65
    letterSpacing: "-0.01em"
  label:
    fontFamily: "-apple-system, PingFang SC, Hiragino Sans GB, system-ui, sans-serif"
    fontSize: "10px"
    fontWeight: 700
    lineHeight: 1.4
    letterSpacing: "0.12em"
  meta-mono:
    fontFamily: 'ui-monospace, SF Mono, Menlo, monospace'
    fontSize: "10px"
    fontWeight: 400
    lineHeight: 1.5
  section-heading:
    fontFamily: "-apple-system, PingFang SC, Hiragino Sans GB, system-ui, sans-serif"
    fontSize: "17px"
    fontWeight: 700
    lineHeight: 1.25
    letterSpacing: "normal"
  dialog-body:
    fontFamily: "-apple-system, PingFang SC, Hiragino Sans GB, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.6
    letterSpacing: "normal"
  meta-body:
    fontFamily: "-apple-system, PingFang SC, Hiragino Sans GB, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: "normal"
  ui-meta:
    fontFamily: "-apple-system, PingFang SC, Hiragino Sans GB, system-ui, sans-serif"
    fontSize: "11px"
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: "0.03em"
  spine-fallback:
    fontFamily: "-apple-system, PingFang SC, Hiragino Sans GB, system-ui, sans-serif"
    fontSize: "8px"
    fontWeight: 400
    lineHeight: 1.0
    letterSpacing: "-1px"
  catalog-number:
    fontFamily: 'ui-monospace, SF Mono, Menlo, monospace'
    fontSize: "44px"
    fontWeight: 400
    lineHeight: 1.0
    letterSpacing: "0.04em"
rounded:
  hairline-radius: "2px"
  flat: "0"
spacing:
  micro: "4px"
  tight: "8px"
  inner: "12px"
  card-pad: "20px 40px 20px 24px"
  section: "24px"
  loose: "48px"
components:
  button-primary:
    backgroundColor: "{colors.umber}"
    textColor: "{colors.paper}"
    rounded: "{rounded.hairline-radius}"
    padding: "5px 11px"
  button-primary-hover:
    backgroundColor: "{colors.umber-deep}"
    textColor: "{colors.paper}"
  button-default:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    rounded: "{rounded.hairline-radius}"
    padding: "5px 11px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.ink-muted}"
  chip-filter:
    backgroundColor: "{colors.selection}"
    textColor: "{colors.ink-muted-deep}"
    rounded: "{rounded.hairline-radius}"
    padding: "1px 6px"
  chip-filter-active:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.paper}"
  input-field:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink}"
    rounded: "{rounded.hairline-radius}"
  card-wall:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink}"
  card-starred:
    backgroundColor: "{colors.ember-wash}"
    textColor: "{colors.ink}"
---

# Design System: Mudflat Knowledge

## 1. Overview

**Creative North Star: "藏书人的索引柜（The Annotated Card Catalog）"**

整个界面是一座藏书人亲手维护的索引柜：柜架是严格对齐的瑞士网格；每本书是一只纸板抽屉，抽屉与抽屉之间留出一截暖纸；每一张卡是四边发丝线勾出的索引卡，卡与卡之间是纸板走道，不是投影也不是空气。mono 字体的书号与日期像图书馆编目一样冷静地钉在角落。金色只出现在藏书人真正「下笔承诺」的地方：一颗星标、一个选中态、一次确认。除此之外，纸就是纸，墨就是墨。

系统性格是融合产物：**网格即秩序，纸墨即温度**。结构上寸土必争地对齐（发丝分割线、锐角 2px 半径、无阴影、mono 元数据），底色上保留旧书的呼吸（暖纸中性色整体染向品牌金的颜色相，OKLCH 中 hue≈80–85）。界面密度优先：单屏尽量多卡，但密度靠排版达成，从不靠压缩文字或缩小行距。

这套系统明确拒绝四样东西：「AI 风」模板感、SaaS 暗色仪表盘风、过度复古做旧、花哨干扰元素。

**Key Characteristics:**
- 暖纸底色 × 冷静发丝线：所有结构由 1px 线与色调层进承担，没有任何阴影
- 全局锐角：圆角统一 2px 或直角，装饰性大圆角禁止出现
- 金色 ≤ 视觉面积 5%，只在语义承诺处现身
- 中文正文为王：高亮文本是页面对比最强的元素
- 元数据编目化：mono、tabular-nums、小字号、低对比但不失辨识

## 2. Colors

暖纸中性色打底，一缕烬火琥珀承诺；复习语义色退居全局角色（朱砂仅破坏性确认、青苔仅成功提示，黛蓝暂缺位）。

### Primary
- **烬火琥珀 Ember Amber** (#b57f12, oklch(63.5% 0.128 78)): 星标、活跃筛选、复习摘要引用线、「我的想法」卡底色的来源。这是系统的唯一声部；阳光晒过旧书页上的烫金，不刺眼。
- **琥珀加深 Ember Deep** (#996605): 烬火琥珀的按下/hover 态。
- **琥珀晕染 Ember Wash** (#f8f0db): 星标卡片与自建卡片底色，带一点温度的纸面隆起。

### Secondary
- **朱砂褪印 Cinnabar Ghost** (#bd4238): 破坏性确认（删除卡片等），仅实底按钮或文字，永不整片铺开。
- **青苔绿 Moss Green** (#3b834e): 成功提示（同步完成等）。同上，描边级使用。
- **黛蓝 Indigo Blue** (#32669a): 语义暂缺位，保留供数据可视化扩展。

### Neutral
- **暖纸 Warm Paper** (#f8f5f1): 主表面。整体染向品牌相，接近白但没有一处纯白。
- **纸板 Board** (#efece7): 侧栏、次级面板、输入框静止底色、卡片墙抽屉槽（卡与卡之间的走道）。
- **石蜡 Paraffin** (#ebe7e0): 选中、hover、code 底色。
- **发丝线 Hairline** (#c7c4bc): 一级分割线、输入框边框。
- **深发丝线 Hairline Strong** (#a9a49c): 弹层边框、复习卡边框等需要更重定义的地方。
- **墨 Ink** (#23201c): 正文文字。
- **褐墨 Umber** (#3a3227): 主动作实底（同步、保存、新建卡 FAB），hover 转褐墨按压 #2e2820。比墨暖一阶，像旧书的封壳，让深色实底长在纸色相里而不是浮在纸上。
- **焦墨 Soot Ink** (#13110e): 选中文本、全系统最深的一格，代替黑色。
- **灰墨 Ash Ink** (#6d6860): 辅助文字、元数据、图标。

### Named Rules
**承诺金规则 (The Pledge Gold Rule)。** 烬火琥珀只在需要「承诺」的交互处出现：星标、选中、主动作确认、引用溯源。装饰性使用金色是被禁止的；它少，所以准。金色落到文字或图标上时必须用加深的琥珀文字档 --ember-text (#8a5f05，浅底 ≥4.5:1)；--ember 本体只用于 ≥3:1 即可的非文本承诺位（星标实点、focus 边、进度刻度）。

**发丝承重规则 (The Hairline Structure Rule)。** 所有层级与结构信息由 1px 发丝线和相邻色调差承担。阴影永远不可用；如果一个区块显得太轻，加一条深发丝线或退一层纸色，而不是投阴影。

## 3. Typography

**Display Font:** 无独立展示字体；中文环境下标题与正文共用系统黑体栈。
**Body Font:** `-apple-system, PingFang SC, Hiragino Sans GB, system-ui`。
**Label/Mono Font:** `ui-monospace, SF Mono, Menlo`。

**Character:** 同一字族内的分层完全依靠字号、字重与间距完成，就像同一支笔写出的不同字号。这种刻意的单调让「内容长什么样」成为唯一变量。

### Hierarchy
- **Reading（阅读级）** (400, 19px, line-height 1.9): 翻牌时的正文字号。全系统最高优先级的排版参数。
- **Emphasized Body（自述级）** (600, 16px, line-height 1.55): 用户自己写的想法卡，比借来的划线更响。
- **Body（正文级）** (400, 14px, line-height 1.65, letter-spacing -0.01em): 卡片墙中划线文本的默认等级。
- **Label（眉标级）** (700, 10px, letter-spacing 0.12em): 区块小标、设置分组头。
- **Meta Mono（编目级）** (400, 10-11px, ui-monospace, tabular-nums): 书名出处、日期、计数，全部编目化处理。
- **Section Heading（区块标题级）** (700, 17px, line-height 1.25): 内容区页标题与设置页标题。
- **Dialog Body（弹层正文级）** (400, 13px, line-height 1.6): 弹层输入框、引用框、确认文案、卡注与原文摘要。
- **Meta Body（元目级）** (400, 12px, line-height 1.5): 侧栏条目、toast、卡牌提示等次级 chrome。
- **UI Meta（界面编目级）** (400/600, 11px, letter-spacing 0.03em): 按钮、来源行、提示行等最小 chrome 文本。
- **Spine Fallback（书脊兜底级）** (400, 8px, 竖排): 封面加载失败时的竖排书名占位。
- **Catalog Number（编目号级）** (400, 44px, ui-monospace): 复习卡背面的编目序号展示，全系统唯一的展示字号。

### Named Rules
**正文为王规则 (The Content Is King Rule)。** 任何页面上对比最强的元素必须是内容文本（划线或想法）。chrome 文本不得超过 14px；正文区域的非交互辅助文字不得重于 500。交互控件豁免：按钮可用 600、眉标级可用 700（见 Typography Hierarchy），豁免不延伸到正文区内的非交互 chrome。
**阅读呼吸规则 (The Reading Breath Rule)。** 凡承担连续中文段落阅读的区域，行高不低于 1.65，回顾场景固定 1.9。密度不足时缩窄侧栏、增加栏数，永不压缩行距。

## 4. Elevation

完全平面化的系统。没有阴影，没有模糊，没有玻璃感；深度完全由三层手段表达：1px 发丝线的粗细变化（一级 #c7c4bc 与强级 #a9a49c）、相邻色调差（paper → panel → selection 三阶）、以及弹层唯一的暗化遮罩 rgba(35,32,28,0.45)。

### Named Rules
**平面承重规则 (The Flat Load-Bearing Rule)。** 表面在静止状态永远是平的。任何新组件如果看起来需要阴影来「浮起来」，说明它的边界定义失败；回到发丝线重画边界。唯一的例外是弹层遮罩。

## 5. Components

### Buttons
- **Shape:** 锐利的小圆角，全局统一 2px，永不变大。
- **Primary:** 褐墨实底 #3a3227 配暖纸文字 #f8f5f1；5px×11px 内边距；无渐变无阴影。注意主按钮用墨系不用金：金留给星标；褐墨是墨向纸色相走的一步，避免冷黑方块突兀。
- **Default:** 暖纸底 + 发丝线边框，hover 时边框加深为墨色。
- **Ghost:** 透明底灰墨字，用于工具行与快捷操作；激活时染石蜡底。
- **Hover / Focus:** 主/default 的 hover 为实底加深；无过渡动画框架要求，仅颜色位移。

### Chips
- **Style:** 石蜡底 #ebe7e0 + 灰墨字 + 发丝线边框，1px×6px 极紧凑内边距，10px 大写风格标签（中文用间距补偿）。
- **State:** 选中态反转为焦墨底暖纸字；未选中的可删除项 hover 时文字转墨。

### Cards / Containers
- **Corner Style:** 直角到 2px 卡片壁。
- **Background:** 划线卡为暖纸；用户自建想法卡为琥珀晕染 #f8f0db，是卡片身份差异的第一信号。墙体抽屉槽为纸板 #efece7，走道露出纸板。
- **Shadow Strategy:** 见 Elevation，无条件禁止。间隔靠纸板走道与暖纸休息，不用投影把卡「浮起来」。
- **Border:** 每张卡四边 1px 发丝线，成为闭合的索引卡模块；不再靠共享右/下边拼成一张墙。
- **Wall rhythm:** 卡与卡 8px（tight）；抽屉内缘 12px（inner）；抽屉与抽屉 24px（section）。
- **Internal Padding:** 20px 40px 20px 24px。右侧多出一列给常驻星标，正文不与印记抢行。
- **Meta Floor Alignment:** 卡片为纵向弹性布局，编目带（标签 / 出处 / 日期）以 margin-top:auto 钉在卡底：同一排卡片无论正文长短，编目带底线全部对齐，像抽屉里并列的目录卡。带内节奏：上缘 16px、标签簇与出处间 7px、出处与日期间 4px。
- **Star mark:** 星标常驻为右上角 SVG 印记（未星灰墨描边，已星琥珀实心）；编辑与删除悬停或键盘聚焦时才展开，不另用 Unicode 星号。

### Inputs / Fields
- **Style:** 纸板底 #efece7 + 发丝线边框 + 2px 圆角，无内阴影。
- **Focus:** 边框转墨、底色提回暖纸；不加光晕。
- **Error / Disabled:** 错误文案用朱砂褪印；禁用态整体降透明至 40%。

### Navigation
- **Structure:** 40px 顶栏 + 220px 左侧栏，均以发丝线定界，侧栏底为纸板层。
- **Sidebar Zones:** 侧栏为三区结构：顶固定「回顾」组（有到期卡时条目以琥珀晕染提示、mono 计数转金）、中滚动「书架」组（书单只在此段内滚动）、底固定「标签」组（上限高度，独立滚动）。书单长度只消耗中段空间，不挤走其余两区。
- **Sidebar Item:** 6px×12px 行内边距，左侧组别眉标 10px/700/0.12em；active 用石蜡底表示，配 mono 计数。
- **Counters:** 书本计数一律 mono 10px，tabular 数字。

### Review Deck (signature component)
复习模式的主舞台是一副编目卡牌：待翻的卡背面朝上，内衬发丝框居中一枚 mono 编目号（当前序号 / 总数）与「轻触翻面」提示（唯一金色落点是提示前的小刷新符）；牌堆后两张以 panel → selection 三阶底色加轻微旋转错位成扇形，纵深只由色调差与发丝线承担。点击或空格翻面（rotateY 420ms ease-out），正面以阅读级 19px/1.9 居中呈现划线原文；再点一次卡牌飞出（240ms），下一张顶上。无评级按钮：翻过即静默记 Good。顶部一排 2px 打孔卡刻度记录进度，读过的染墨、当前的染金。

## 6. Do's and Don'ts

### Do:
- **Do** 保持整体单主题浅色暖纸基调；新增表面必须落在 paper / panel / selection 三阶之一。
- **Do** 让所有结构与层级走发丝线 + 色调差，唯一允许的遮罩是弹层的 rgba(35,32,28,0.45)。
- **Do** 把金色 (#b57f12) 当作承诺预算花：每个屏幕的金色区域合计不超过视觉面积的 5%。
- **Do** 为连续中文正文保住 1.65 以上的行高和 65–75ch 的行长上限；回顾正文 1.9。

### Don't:
- **Don't** 出现「AI 风」模板感：emoji 当图标、千篇一律浅底圆角卡片、模板化布局，全部明令禁止。
- **Don't** 使用 SaaS 暗色仪表盘风：深底蓝光、渐变、玻璃拟态一律禁止；本系统没有暗色变体。
- **Don't** 过度复古做旧：宋体大字报式怀旧或仿真纸质拟物都不许；工具感不能丢。
- **Don't** 引入花哨干扰元素：多彩标签云、装饰性噪点纹理、抢注意力的动效都在禁区。
- **Don't** 在卡片、列表项、引用上用大于 1px 的彩色侧边条纹充当强调（现行实现中的 2px 左边线属于遗留违规，须改为背景晕染、引导字形或完整边框方案）。
- **Don't** 用纯黑 #000 或纯白 #fff；最深的格子是焦墨 #13110e，最亮的是暖纸 #f8f5f1。
- **Don't** 给任何静止表面添加阴影或模糊；「太轻」回炉到 Elevation 规则找答案。
