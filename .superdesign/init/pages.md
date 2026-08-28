# Pages（视图依赖树）

单文件应用，所有视图在同一 `src/App.tsx`。依赖树即完整文件清单：

## 卡片墙（默认视图）
Entry: src/App.tsx（App 组件 cards 分支）
Dependencies:
- src/App.tsx（全部内嵌组件：Card / EditModal / CreateModal / SettingsView / ReviewView）
- src/App.css（全部样式）
- src/types.ts（后端数据类型镜像 + invoke 封装，无 UI）
- src/main.tsx（ReactDOM 挂载）
- index.html（#root + 标题 Mudflat Knowledge）

## 回顾视图
Entry: src/App.tsx（ReviewView）
Dependencies: 同上（单文件应用）

## 设置视图
Entry: src/App.tsx（SettingsView）
Dependencies: 同上（单文件应用）

## 上下文文件清单（设计/迭代时全部传入）
- src/App.tsx:1-566（含 Card、弹层、ReviewView、SettingsView 完整渲染代码）
- src/App.css（全文 167 行）
- src/types.ts（仅类型，可省略以省预算）
