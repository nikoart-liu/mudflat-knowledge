# Mudflat Knowledge

桌面端「卡片笔记」应用：把你在**微信读书**里的划线与想法变成一张张卡片，支持浏览、全文搜索、星标与标签、基于间隔重复（SM-2 简化版）的每日回顾抽卡，并可直接给划线补写想法或新建独立卡片。

技术栈：Tauri 2 + React 18 + TypeScript + Vite；后端 Rust（rusqlite + reqwest）。纯本地 SQLite 存储，无账号、无云同步；API Key 存 macOS 钥匙串。

## 获取微信读书 API Key

1. 打开官方 Skills 开通页：<https://weread.qq.com/r/weread-skills>
2. 按页面指引开通「微信读书 Skills」并签发 API Key，Key 以 `wrk-`（或 `WRK-`）前缀开头。
3. 启动本应用 → 设置 → 粘贴 Key → 「测试连接」确认可见书本总数 → 「保存到钥匙串」。
4. 回到顶栏点「同步」，划线与想法会以卡片形式进入本地库。

## 开发

前置：Node ≥ 20、Rust stable、Xcode Command Line Tools（macOS）。

```bash
npm install
npm run tauri dev     # 打开开发窗口
```

```bash
npm run tauri build   # 打包
cd src-tauri && cargo test   # 后端单元测试 + 数据层行为测试
```

## 数据与目录

- 数据库：`~/Library/Application Support/com.mudflat.knowledge/mudflat.db`（SQLite，WAL 模式，FTS5 trigram 中文子串搜索）
- API Key：macOS 钥匙串，服务名 `mudflat-knowledge`

## 接口契约备忘

唯一权威接口文档克隆在 `docs/weread-skills.md`（Tencent/WeChatReading skills/notes.md，v1.0.4）。要点：业务参数与 `api_name`、`skill_version` 平铺在 body 顶层；禁止 `params` 包裹与 `offset/limit`；`/review/list/mine` 的参数名是小写 `bookid`；`noteCount` 是划线条数而非总笔记数。

## 代码结构

```
src/                    React 前端（无状态库，view state 切换）
src/types.ts            后端命令类型镜像
src-tauri/src/db.rs     连接/迁移/全部 SQL
src-tauri/src/gateway.rs 网关 HTTP 客户端（300ms 节流）
src-tauri/src/sync.rs   同步引擎（增量拉取 + reconcile 软删）
src-tauri/src/srs.rs    SM-2 简化调度（纯函数 + 单测）
src-tauri/src/keychain.rs 钥匙串读写
```
