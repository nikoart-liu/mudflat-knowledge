# `/user/notebooks` 实测回包探针：分类字段与契约文档偏差

日期：2026-09-04（以仓库当前时间为准）。方法：用与 `gateway.rs` 相同的信封（POST
`/api/agent/gateway`，`skill_version: 1.0.4`，参数平铺）分页拉全量 97 本 notebook，
只统计字段结构与分类分布，不落书名内容。目的：回答「同步数据是否包含书籍分类」，
并核对 `docs/weread-skills.md`（上游 notes.md v1.0.4）与真实回包的差异。

## 结论

1. **分类字段存在且全覆盖**：`book.categories` 是数组，元素形如
   `{categoryId: 1100000, subCategoryId: 1100001, categoryType: 0, title: "经济理财-财经"}`。
   `title` 是「大类-子类」合并串。实测 97/97 本都有分类，12 本有多个分类（数组多元素）。
2. **当前解析器把它丢了**：`gateway.rs` 的 `BookInfo` 只反序列化
   bookId/title/author/cover；`book` 实际返回约 40 个字段（deepLink、translator、
   format、bookStatus、publishTime、language 等），其余全部被 serde 静默丢弃。
3. **`sort` 是「最近笔记时间」**：notebook 顶层 `sort` 为 Unix 秒级时间戳，
   实测值与各书最近笔记活动吻合；当前只当翻页游标用，未落库。
   这是「新书/最近活跃书」排序的现成数据源。
4. **契约文档三处偏差**（若要使用相关字段，以下为准，勿信文档）：
   - 文档 `books[].book` 只写「title, author, cover 等」，未记录 `categories`；
   - `markedStatus` 文档写「1=读完, 0=在读」，实测分布为 `{1: 3, 2: 60, 3: 16, 4: 18}`，
     0 从未出现，文档映射明显过期（真实枚举含义待进一步核实）；
   - `readingProgress` 实测为 0–100 的百分比整数（11、99 等）；本应用
     `sync.rs` 入库时恒写 0（`reading_progress` 现为死列），属可顺手修复项。

## 分类分布（本库 97 本）

- 经济理财合计 55 本（理财 19 / 财经 16 / 商业 12 / 管理 8），占 57%；
- 其余为 37 个子类的长尾，多数只有 1–2 本；大类约 12 个。

含义：按分类**分组**侧栏对本库价值有限（57% 挤在一桶，长尾每组 1–2 本），
分类更适合做书行元数据小字或过滤维度；若做分组，多分类书需定「取第一个」类规则。

## 对可见性需求的落点

- 「新书难找」：`list_books` 现按 `note_count + review_count DESC` 排序，新书必然沉底；
  `sort`（远端最近笔记时间）与本地 `MAX(cards.created_at)` 都可作「最近活跃」信号。
- 「书级标签 vs 置顶」：本地标签体系（`tags`/`card_tags`）目前 0 使用；书级组织
  建议先用 `pinned` 布尔 + 远端分类，不引入第二套标签语义。
