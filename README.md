# 滩涂拾遗

桌面端「卡片笔记」应用：把你在**微信读书**里的划线与想法变成一张张卡片，支持浏览、全文搜索、星标与标签、基于间隔重复（SM-2 简化版）的每日回顾抽卡，并可直接给划线补写想法或新建独立卡片。

技术栈：Tauri 2 + React 18 + TypeScript + Vite；后端 Rust（rusqlite + reqwest）。纯本地 SQLite 存储，无账号、无云同步；API Key 存本地数据目录下的 Key 文件。

## 获取微信读书 API Key

1. 打开官方 Skills 开通页：<https://weread.qq.com/r/weread-skills>
2. 按页面指引开通「微信读书 Skills」并签发 API Key，Key 以 `wrk-`（或 `WRK-`）前缀开头。
3. 启动本应用 → 设置 → 粘贴 Key → 「测试连接」确认可见书本总数 → 「保存到本机」。
4. 回到顶栏点「同步」，划线与想法会以卡片形式进入本地库。

## 开发

前置：Node ≥ 20、Rust stable。

```bash
npm install
npm run tauri dev     # 打开开发窗口
```

```bash
npm run tauri build   # 打包
cd src-tauri && cargo test   # 后端单元测试 + 数据层行为测试
```

## 发布安装包

GitHub Actions 会在推送 `v*` 版本标签时构建 macOS（Apple Silicon / Intel）、Windows x64 与 Linux x64 安装包，并自动创建 GitHub Release。标签版本必须与 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 中的版本一致：

```bash
git tag v0.1.0
git push origin v0.1.0
```

创建 Release 需要仓库允许 GitHub Actions 写入：Settings → Actions → General → Workflow permissions → Read and write permissions。若未开启，安装包仍会作为 workflow artifacts 保留，但不会出现在 GitHub Releases。

Windows 安装包使用 NSIS（`.exe`），不生成 MSI：WiX 无法处理中文产品名。macOS 包使用 ad-hoc 签名但未经过 Apple 公证，Windows 包也未使用商业代码签名证书；首次安装时系统可能显示安全提示。正式对外分发前，应在仓库 Secrets 中配置对应平台的签名与公证凭据。

## 数据与目录

- 数据库：`~/Library/Application Support/com.mudflat.knowledge/mudflat.db`（SQLite，WAL 模式，FTS5 trigram 中文子串搜索）
- 微信读书 API Key：`~/Library/Application Support/com.mudflat.knowledge/api.key`（明文小文件，权限 0600，与数据库同目录，删除即清除）
- 语言模型：同目录 `llm.json`（供应商/地址/模型）+ `llm.key`（0600）；设置里默认关闭，与微信读书 Key 分开

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
src-tauri/src/keystore.rs 本机 Key 文件读写（api.key，0600）
src-tauri/src/llm.rs     语言模型供应商配置（llm.json + llm.key）
```
