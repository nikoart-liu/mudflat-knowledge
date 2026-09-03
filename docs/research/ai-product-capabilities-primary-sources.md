# 调研：现代 AI 接入知识管理/知识库类产品的能力图景（一手来源）

- 调研日期：2026-09-01
- 目的：为「滩涂拾遗」评估"AI 能做什么、值得做什么、代价是什么"提供事实底座。本文只陈述有据可查的能力与约束；对本产品的推论单独放在 §7，与来源事实明确区分。
- 方法：三个并行研究通道分别覆盖 (a) Notion + Google、(b) OpenAI + Anthropic、(c) Microsoft + 端侧/论文，所有引用页面均经网页抓取实际取回核对（非仅搜索摘要），关键页面另经主笔二次抽查复验（§9）。共核验约 50 个一手页面：厂商产品/帮助/开发者文档、安全与隐私页、官方协议（MCP）与 arXiv 论文。**不含任何二手博客/媒体来源。**
- 约定：每条事实后附一手来源链接，关键处附英文原文引句。产品名以抓取当日官方页面为准：NotebookLM 已更名 **Gemini Notebook**、Vertex AI Search 更名 **Agent Search**、ChatGPT connectors 更名 **Apps in ChatGPT**。个别搜索发现但未抓取验证的官方页面一律标注 `[未复验]` 或直接不用。

---

## 1. 核心结论（TL;DR）

1. 行业已收敛出**八种可复用的 AI 能力模式**：来源接地问答（RAG）、上下文内写作辅助、自动结构化、学习材料再生产、跨库连接、工具调用/Agent、个性化记忆、端侧推理。每一种都有多家厂商官方文档佐证，均已是量产能力而非概念。
2. "来源接地问答"是知识管理产品最成熟的模式，其技术底座也高度收敛：**embedding + 向量库 + 分块 + 混合检索（词法 BM25 + 向量）+ 重排 + 引用回溯 + 查询时权限校验**。Microsoft、Notion、Anthropic、OpenAI、Google 五家文档对这套流水线的描述互相印证。
3. **引用与权限是信任层，不是可选项**：Notion Enterprise Search 承诺"总是引用来源"；Microsoft 与 Notion 都文档化了"查询时校验权限"（permissions checked at query time, not just during indexing）；Google 承诺"只检索用户有权访问的内容"。
4. **"默认不训练"已成行业标配隐私承诺**：OpenAI（API 数据自 2023-03 起默认不训练）、Notion（合同禁止 AI 子处理器用客户数据训练）、Microsoft（租户数据不训练基础模型）、Google Workspace（未经许可不用客户数据训练）。本地优先产品可以把这条线画得更靠前：数据不出设备。
5. **端侧推理已是桌面本地产品的现实选项**：Apple 向第三方 App 开放系统内置约 3B 参数端侧模型（离线、隐私、随 OS 分发）；Ollama 提供本地 REST API。但 Apple 明确该模型"不为世界知识与高级推理设计"——能力上限是设计输入，不是工程缺陷。
6. 主要风险集中在四处：**幻觉**（厂商对策是人工复核 + 引用 + 免责声明）、**检索质量**（分块策略、上下文位置敏感性）、**成本/延迟**（prompt caching 已成标配，缓存输入最高降价 90%）、**数据治理**（留存期、索引新鲜度、断开后的删除时序）。

## 2. 能力模式总览

| # | 模式 | 一句话定义 | 代表产品（均有官方文档） |
|---|---|---|---|
| P1 | 来源接地问答 | 对用户知识库提问，答案只基于库内内容并引用出处 | Notion Enterprise Search、Gemini Notebook（原 NotebookLM）、Microsoft Copilot、ChatGPT/Claude Projects、OpenAI File Search |
| P2 | 上下文内写作辅助 | 在编辑器内改写/摘要/续写，可引用库内其他材料 | Notion AI（AI 块/行内编辑）、Gemini in Docs |
| P3 | 自动结构化 | 从内容抽取属性/标签/摘要，写入结构化字段 | Notion AI autofill（AI database properties） |
| P4 | 学习材料再生产 | 把已有知识转成卡片、测验、音频讨论、报告、思维导图 | Gemini Notebook（Flashcards/Quizzes/Audio Overview）、Claude 学习模式 |
| P5 | 跨库连接 | 把外部应用内容纳入检索与问答范围，权限随人走 | Notion AI Connectors、ChatGPT Apps、Microsoft 365 Copilot（Graph） |
| P6 | 工具调用与 Agent | 模型调用应用定义的函数/外部工具，多步完成任务 | OpenAI Function Calling、Model Context Protocol（MCP）、Deep Research |
| P7 | 个性化记忆 | 跨会话记住用户上下文与偏好 | ChatGPT Memory / Projects 内置记忆 |
| P8 | 端侧/本地推理 | 模型在设备或本机运行，数据不出本地 | Apple Foundation Models framework、Ollama |

## 3. 分模式详解

### P1 来源接地问答（RAG Q&A）——知识管理产品的核心模式

**定义**：用户对"自己的知识库"提问；系统先检索相关内容片段，再让模型仅基于这些片段作答，并给出可回溯的引用。

**产品实例**（每条附一手来源）：

- **Notion Enterprise Search**：在工作区与已连接应用（Slack、Google Drive、Jira 等）中找答案并回答；官方承诺回答时"总是引用来源，以便回到源头"。— [Enterprise Search in Notion](https://www.notion.com/help/enterprise-search)，引句："When Enterprise Search answers a question using information from your workspace or a connected app, it'll always cite its sources so you can go back to the source."
- **Gemini Notebook（原 NotebookLM）**：只基于用户上传的来源作答，来源里没有就拒绝回答——这是产品级的反幻觉设计；多来源时"先按问题检索最相关信息，再据此构建回答"。— [FAQ](https://support.google.com/gemininotebook/answer/16269187?hl=en)，引句："Gemini Notebook answers questions based on the information provided in your uploaded sources. If the answer isn't in the source material, it won't provide a response." / "When your notebook contains many sources, Gemini Notebook retrieves the most relevant information based on your question, then builds a response from it."
- **Microsoft 365 Copilot**：用 Microsoft Graph 中"用户有权访问的内容"对接地，检索经语义索引（数十亿向量级）。— [Copilot 架构](https://learn.microsoft.com/en-us/microsoft-365/copilot/microsoft-365-copilot-architecture)、[语义索引](https://learn.microsoft.com/en-us/microsoftsearch/semantic-index-for-copilot)，引句："Copilot preprocesses the input prompt by using grounding and accesses Microsoft Graph in the user's tenant."
- **OpenAI File Search（平台能力）**：Responses API 内置工具，对上传文件构成的 vector store 做"语义 + 关键词"检索，宿主托管、回答带 `file_citation` 文件引用。— [File search](https://platform.openai.com/docs/guides/tools-file-search)，引句："It enables models to retrieve information in a knowledge base of previously uploaded files through semantic and keyword search."
- **Claude Projects（消费端）**：项目知识库在接近上下文上限时自动切 RAG 模式，容量最多扩 10 倍。— [What are projects?](https://support.claude.com/en/articles/9517075-what-are-projects)，引句："Claude seamlessly enables RAG mode to expand capacity by up to 10x while maintaining response quality."

**底层技术需求**：

- Embedding 模型 + 向量库：OpenAI embeddings 文档将"搜索"列为第一用例，第三代模型默认 1536/3072 维 — [Embeddings](https://platform.openai.com/docs/guides/embeddings)；Notion 自述用 OpenAI 零留存 embeddings API 为每页生成向量，存入 Turbopuffer 向量库 — [Notion AI 安全实践](https://www.notion.com/help/notion-ai-security-practices)。
- 分块（chunking）：把长文档切成数百 token 的片段分别嵌入 — [Anthropic Contextual Retrieval](https://www.anthropic.com/engineering/contextual-retrieval)；Azure 将 chunking 的动因说破："除非原始文档很小，chunking 是满足 embedding 模型 token 输入限制所必需的" — [Integrated vectorization](https://learn.microsoft.com/en-us/azure/search/vector-search-integrated-vectorization)。Notion 按 span 分块，并把作者、**权限**等元数据随向量一起入库 — [Notion 工程博客：两年向量搜索](https://www.notion.com/blog/two-years-of-vector-search-at-notion)。
- 混合检索 + 融合 + 重排：Azure AI Search 官方推荐"关键词（BM25）+ 向量（HNSW/eKNN）并行、RRF 融合、可选语义重排"，并给出基准结论"混合检索 + 语义重排显著提升相关性" — [Hybrid search overview](https://learn.microsoft.com/en-us/azure/search/hybrid-search-overview)。Anthropic 工程侧给出量化收益：上下文化分块使 top-20 检索失败率 5.7%→3.7%（-35%），叠加 Contextual BM25 至 2.9%（-49%），再加重排至 1.9%（-67%）；并确认"top-20 片段优于 top-10/top-5" — [Contextual Retrieval](https://www.anthropic.com/engineering/contextual-retrieval)。
- 查询时权限校验：Notion 明文"权限在查询时校验，而不只在索引时" — [Enterprise Search 安全实践](https://www.notion.com/help/enterprise-search-security-and-privacy-practices)；Microsoft："Semantic Index 遵循基于用户身份的访问边界，接地过程只访问当前用户被授权访问的内容" — [Copilot 隐私](https://learn.microsoft.com/en-us/microsoft-365/copilot/microsoft-365-copilot-privacy)。
- 长上下文与缓存（小库可以直接塞上下文）：Anthropic 指出知识库小于约 20 万 token（约 500 页）时可不做 RAG、直接放入 prompt，配合 prompt caching 提效 — [Contextual Retrieval](https://www.anthropic.com/engineering/contextual-retrieval)；Gemini 2.5 Pro 输入上限 1,048,576 token — [模型文档](https://ai.google.dev/gemini-api/docs/models/gemini-2.5-pro)。

**产品价值**：把"找知识"从关键词匹配升级为问答案；跨书/跨应用找"我写过/我划过什么"；引用可回溯，让用户敢信。厂商把它作为旗舰能力销售（Notion AI、Copilot 均为付费附加）。

**风险与约束**：检索失败与幻觉仍是常态风险（见 §6.1）；索引新鲜度有延迟（Notion 连接器接入内容最长 72 小时才可被检索 — [Connectors](https://www.notion.com/help/notion-ai-connectors)）；上下文位置敏感（§6.5）。

### P2 上下文内写作辅助

**定义**：在编辑器/文档内完成改写、摘要、翻译、续写，可引用库内其他材料，产出落回文档本身。

**产品实例**：

- Notion：AI 块（整块内容由 prompt 驱动生成）、页面级摘要/翻译/行内编辑。— [Notion AI FAQs](https://www.notion.com/help/notion-ai-faqs)，引句："You can power an entire block with Notion AI. Type /AI Block into a page and give Notion AI a prompt for what it should generate."
- Gemini in Docs：可基于 Drive/Gmail 中的其他文件起草与润色，带来源选择器，并把 Drive/Gmail/网页中的论据与引用直接拉进文档。— [Collaborate with Gemini in Docs](https://support.google.com/docs/answer/14206696?hl=en)、[Write & edit with Gemini in Docs](https://support.google.com/docs/answer/13447609?hl=en)，引句："You can draft and refine content in Google Docs using information directly from your files in Drive, Chat, Gmail, and the web."

**底层技术需求**：与 P1 共用检索底座；额外需要编辑器内联交互（选中即改、diff 确认）与低延迟（流式输出、prompt caching）。

**产品价值**：降低"从划线到成文"的启动成本；把已有素材变成可读文本。对"重读轻写"的产品，写作辅助是次要闭环，摘要辅助更贴合。

**风险与约束**：产出风格同质化、与既有排版语言冲突（本仓库 PRODUCT.md 把"AI 风模板感"列为反参考，即此类顾虑）；生成内容需可回退、可编辑。

### P3 自动结构化（属性抽取/自动标签/Autofill）

**定义**：让模型读内容、产出结构化字段值：摘要、要点、翻译、分类标签，直接写进数据库属性或卡片元数据。

**产品实例**：

- Notion AI autofill：官方示例包括"摘要页面内容、抽取关键信息（人名/日期/行动项）、翻译、用单选/多选属性打标签分类"。— [Notion AI autofill](https://www.notion.com/help/autofill)，引句："Try using Notion AI on a database property to generate summaries, extract action items, and more."
- 作用域分层明确：Basic Autofill 只看本行/本页、不联网；Agent Autofill 可用工作区搜索找关联上下文再填。— 同上，引句："Basic Autofill does not browse the web. It uses the content in the specific row or page (not across all your Notion pages)."

**底层技术需求**：结构化输出（JSON/schema 约束）、受控词表（select 属性）、人工确认流；跨页作用域需要把 P1 检索接进来。

**产品价值**：元数据冷启动——划线多、标签少是知识库常态；把"手工整理"变成"确认整理"。标签一致性直接影响后续检索与筛选质量。

**风险与约束**：错标签会污染检索与筛选；必须有批量人工复核与改写路径；作用域越界（跨页取材）会放大权限与隐私面。

### P4 学习材料再生产（制卡/测验/音频/报告）

**定义**：把库内已有知识再加工成学习形态：问答卡、测验、音频对谈、报告、思维导图。

**产品实例**：

- Gemini Notebook：Flashcards/Quizzes"把你来源里的信息变成互动学习材料" — [官方帮助](https://support.google.com/gemininotebook/answer/16958963?hl=en)，引句："Flashcards or Quizzes help you master your material by turning information from your sources into interactive study aids."
- Audio Overview：两位 AI 主持人基于上传来源做深度对谈，且用户可以加入对话互动。— [官方帮助](https://support.google.com/gemininotebook/answer/16212820?hl=en)，引句："Audio Overviews are deep-dive discussions between AI hosts that provide in-depth summaries of the key topics in your uploaded sources."
- 产物矩阵：chats、audio/video overviews、reports、mind maps 等 — [官方帮助](https://support.google.com/gemininotebook/answer/16179536?hl=en)。
- Claude 学习模式（教育场景）：引导推理而非直接给答案，苏格拉底式提问。— [Introducing Claude for Education](https://www.anthropic.com/news/introducing-claude-for-education)，引句："Learning mode: A new Claude experience that guides students' reasoning process rather than providing answers, helping develop critical thinking skills."

**底层技术需求**：P1 检索底座 + 面向"出题"的任务模板 + 质量抽检；音频/多模态需要额外生成管线。

**产品价值**：把"囤积"变"消化"，与间隔重复（SRS）天然同构——由模型生成问题面，用户主动回忆，而不是被动重读。这是知识管理产品里少有的"直接提升学习效果"的能力。

**风险与约束**：生成的问答质量参差，错题会污染复习队列（需人工确认/评分回流）；"代读"式功能可能替代而非促进学习——Claude 学习模式正是对这一点的产品化纠偏。

### P5 跨库连接器与企业搜索

**定义**：把外部应用（聊天、网盘、工单、邮箱）内容纳入知识库的检索与问答范围，权限随用户身份走。

**产品实例**：

- Notion AI Connectors：官方支持表覆盖 Slack、Microsoft Teams、Google Drive、SharePoint/OneDrive、Jira、GitHub、Linear、Gmail、Outlook、日历；Slack 场景下 Agent"以你的身份行动，因此只能访问你本就能看到的内容"。— [Notion AI Connectors](https://www.notion.com/help/notion-ai-connectors)，引句："It acts as you, so it can only access what you can already see."
- ChatGPT Apps（原 connectors）：连接 Drive/Slack 等服务，"部分应用支持预先同步以索引内容"，可与 Deep Research 结合做多源引用分析；自定义应用基于 MCP 构建。— [Apps in ChatGPT](https://help.openai.com/en/articles/11487775-connectors-in-chatgpt)，引句："Build apps using the Model Context Protocol (MCP) to let ChatGPT call approved tools and retrieve information from services."
- Deep Research：跨网页、上传文件与已连接应用做多步研究，产出带引用的结构化报告，"来源可包括 Google Drive 或 SharePoint 等文档库"。— [Deep research in ChatGPT](https://help.openai.com/en/articles/10500283-deep-research)。

**底层技术需求**：OAuth 接入、增量索引管线（Notion：接入后最长 72 小时完成摄取；断开后一小时不可搜、一天内删除数据 — 同上）、权限映射（以用户身份查询外部系统）、向量库存第三方内容向量（Notion 用 Turbopuffer 存连接器内容向量 — 同上）。

**产品价值**：知识库从"一个库"升格为"个人/组织事实中枢"；问题不必先想"在哪个 app 里"。

**风险与约束**：权限映射是最容易出错的一层；同步延迟与断开后的数据残留有明确时序承诺（见引文）；外部系统 API 变更是长期维护成本。

### P6 工具调用与 Agent 化

**定义**：模型不只回答，还调用应用定义的函数/外部工具，按"模型提议 → 应用执行 → 结果回传"循环完成任务。

**产品实例**：

- OpenAI Function Calling：官方五步循环——"1. 带工具列表请求模型 2. 收到工具调用 3. 应用侧执行 4. 把结果再次请求模型 5. 得到最终回答（或更多工具调用）"。— [Function calling](https://platform.openai.com/docs/guides/function-calling)。
- Model Context Protocol（MCP）：开放标准，官方比喻是"AI 应用的 USB-C 口"；客户端-服务端架构（宿主为每个服务端建一个客户端），数据层为 JSON-RPC，传输层 stdio（本机进程）与 Streamable HTTP（远程），标准化三类原语：工具（Tools）、资源（Resources）、提示（Prompts）。Claude、ChatGPT、VS Code、Cursor 均已支持。— [MCP 介绍](https://modelcontextprotocol.io/docs/getting-started/intro)、[架构](https://modelcontextprotocol.io/docs/learn/architecture)，引句："Think of MCP like a USB-C port for AI applications."
- Deep Research（OpenAI 官方定位）："OpenAI 的下一个 agent，可独立替你工作……综合数百个在线来源产出研究分析师水平的报告"，且"每个输出都有完整引用"。— [Introducing deep research](https://openai.com/index/introducing-deep-research)。
- Claude API web search 工具：回答自带来源引用，按 $10/千次搜索计费。— [Web search tool](https://docs.claude.com/en/docs/agents-and-tools/tool-use/web-search-tool)。

**底层技术需求**：function calling/MCP 客户端、工具权限与确认 UI、沙箱化执行、成本控制（工具循环会放大 token 消耗）。

**产品价值**：从"回答问题"到"替你动手"（整理、归档、跨系统搬运）；MCP 让"接一次、处处可用"，中小产品可以用标准协议接入大生态。

**风险与约束**：误操作/越权动作（写操作必须有人工确认）；MCP 服务端的供应链信任；Agent 循环的成本与失控风险。

### P7 个性化记忆

**定义**：跨会话记住用户上下文（聊过什么、传过什么文件、连了哪些应用），在后续交互中直接复用。

**产品实例**：

- ChatGPT Memory："自动记住你的聊天、文件与已连接应用中的有用上下文，个性化你的体验"；项目级记忆与全局记忆相互隔离。— [Memory FAQ](https://help.openai.com/articles/8590148-memory-faq)，引句："memory helps ChatGPT automatically remember useful context from your chats, files, and connected apps."
- ChatGPT Projects："内置记忆——记住你在该项目里创建或上传的所有聊天与文件"，团队可共享项目作为"活的上下文中枢"；对共享项目数据用于训练，要求"每位贡献者与所有者都开启'为所有人改进模型'"。— [Projects in ChatGPT](https://help.openai.com/en/articles/10169521-using-projects-in-chatgpt)。

**底层技术需求**：记忆的存储/检索/遗忘机制（用户可查看可删除）、作用域隔离（项目内 vs 全局）、训练用途的显式同意。

**产品价值**：越用越懂用户，减少重复交代背景；对回顾型产品即"记得你上次在哪里卡住"。

**风险与约束**：错误记忆会被固化并放大；记忆是敏感数据，泄露面大于单次会话；作用域隔离必须可验证。

### P8 端侧/本地推理

**定义**：模型在用户设备或本机进程内运行，内容不上传；是本地优先产品的 AI 路径。

**产品实例**：

- Apple Foundation Models framework（WWDC25）：向第三方 App 开放驱动 Apple Intelligence 的端侧 LLM，Swift API，覆盖 macOS/iOS/iPadOS/visionOS；官方明确"端侧约 30 亿参数、2-bit 量化""数据进出模型都留在设备上、可离线""已内置于操作系统"，同时直言其"不为世界知识与高级推理设计，那是服务端大模型的活"。— [Meet the Foundation Models framework（WWDC25）](https://developer.apple.com/videos/play/wwdc2025/286/)、[Code-along（WWDC25）](https://developer.apple.com/videos/play/wwdc2025/259/)，引句："The on-device model we just used is a large language model with 3 billion parameters, each quantized to 2 bits. … It's not designed for world knowledge or advanced reasoning."
- Ollama：本机运行开源模型的本地 REST API（默认 `http://localhost:11434/api`），macOS/Windows/Linux；同一 API 形态也以 `https://ollama.com/api` 提供云模型。— [API 介绍](https://docs.ollama.com/api/introduction)、[Quickstart](https://docs.ollama.com/quickstart)。（其认证页称本地访问无需认证，`[未复验]` — [Authentication](https://docs.ollama.com/api/authentication)。）

**底层技术需求**：设备/系统模型运行时或本机推理服务；小型模型下的任务适配（模板、约束输出）；本地向量索引（SQLite/本地文件可承载）；本机 API 的访问边界（其他本地进程也能访问无鉴权端口）。

**产品价值**：与"无账号、无云端、纯本地"的产品定位零冲突；离线可用；隐私叙事完整（数据不出设备比"厂商承诺不训练"更彻底）。

**风险与约束**：能力上限明确（3B 量化模型不适合世界知识/复杂推理——Apple 原话）；中文小模型质量需逐一评估；本机 API 端口对同机进程开放，需注意访问边界；硬件门槛。

## 4. 底层技术需求横向清单

| 技术组件 | 作用 | 一手依据（选列） |
|---|---|---|
| Embedding 模型 | 文本→向量，语义检索的基础 | [OpenAI Embeddings](https://platform.openai.com/docs/guides/embeddings)（1536/3072 维）；[Notion 安全实践](https://www.notion.com/help/notion-ai-security-practices)（OpenAI 零留存 embeddings + Turbopuffer） |
| 向量库 + 权限元数据 | 近邻检索 + 按人过滤 | [Notion 工程博客](https://www.notion.com/blog/two-years-of-vector-search-at-notion)（span 携带作者与权限元数据，数十亿对象） |
| 分块 chunking | 适配 embedding token 上限、独立命中 | [Azure Integrated vectorization](https://learn.microsoft.com/en-us/azure/search/vector-search-integrated-vectorization)；[Anthropic Contextual Retrieval](https://www.anthropic.com/engineering/contextual-retrieval)（数百 token/块） |
| 上下文化分块 | 给块补"它出自哪"的前缀，检索失败率 -35%~-49% | [Anthropic Contextual Retrieval](https://www.anthropic.com/engineering/contextual-retrieval)（5.7%→3.7%→2.9%，成本 $1.02/百万文档 token） |
| 混合检索（BM25+向量）+ RRF 融合 | 词法精确命中 + 语义泛化，两条腿并行 | [Azure Hybrid search](https://learn.microsoft.com/en-us/azure/search/hybrid-search-overview)；Anthropic："BM25 用词法匹配找精确词句，特别适合含唯一标识符的查询" |
| 重排 reranking | 只把最相关片段送入模型；叠加后失败率 -67% | [Azure Hybrid search](https://learn.microsoft.com/en-us/azure/search/hybrid-search-overview)（语义重排）；[Anthropic](https://www.anthropic.com/engineering/contextual-retrieval)（top-20 最优） |
| 引用机制 | 答案锚定到原文片段，可回溯验证 | [Anthropic Citations](https://docs.claude.com/en/docs/build-with-claude/citations)（句子级切分、字符索引/PDF 页码）；OpenAI `file_citation`；Gemini `url_citation`（start/end index，[Grounding](https://ai.google.dev/gemini-api/docs/google-search)）；NotebookLM 来源外拒答 |
| 查询时权限校验 | AI 不放大既有访问边界 | [Notion Enterprise Search 安全实践](https://www.notion.com/help/enterprise-search-security-and-privacy-practices)；[Copilot 隐私](https://learn.microsoft.com/en-us/microsoft-365/copilot/microsoft-365-copilot-privacy)；[Azure RAG 指南](https://learn.microsoft.com/en-us/azure/search/retrieval-augmented-generation-overview)（文档级安全裁剪/查询时过滤） |
| Prompt caching | 重复长前缀（知识库/系统提示）降价提速 | [OpenAI](https://platform.openai.com/docs/guides/prompt-caching)（缓存输入最高 -90%，KV 张量缓存，最小 1024/2048 token，组织间隔离）；[Anthropic](https://docs.claude.com/en/docs/build-with-claude/prompt-caching)（5 分钟 TTL 免费续期、写 +25%/读 10%、至多 4 断点）；[Gemini](https://ai.google.dev/gemini-api/docs/caching)（2.5 起隐式缓存默认开启） |
| 长上下文 | 小库免 RAG 直塞；大文档整读 | [Gemini 2.5 Pro](https://ai.google.dev/gemini-api/docs/models/gemini-2.5-pro)（输入 1,048,576 token）；[Gemini 文档理解](https://ai.google.dev/gemini-api/docs/document-processing)（最长 1000 页，Files API 复用）；Anthropic：<200k token 直接入 prompt |
| 结构化输出/受控词表 | 属性抽取、自动打标落地 | [Notion autofill](https://www.notion.com/help/autofill)（select/multi-select 属性打标） |
| Function calling / MCP | 接入应用函数与外部工具生态 | [OpenAI Function calling](https://platform.openai.com/docs/guides/function-calling)；[MCP 架构](https://modelcontextprotocol.io/docs/learn/architecture)（JSON-RPC；stdio/Streamable HTTP；Tools/Resources/Prompts） |
| 端侧模型运行时 | 本地推理、离线、隐私 | [Apple Foundation Models（WWDC25）](https://developer.apple.com/videos/play/wwdc2025/286/)（~3B、2-bit、随 OS 分发）；[Ollama](https://docs.ollama.com/api/introduction)（localhost:11434） |
| RAG 编排框架 | 摄取→转换→嵌入→索引→检索→生成的标准管线 | [Google RAG Engine](https://docs.cloud.google.com/gemini-enterprise-agent-platform/build/rag-engine/rag-overview)（六步管线，"用私有信息丰富 LLM 上下文，减少幻觉"） |

## 5. 产品价值归纳

1. **从"存"到"问"**：检索成本从"我记得有个词"降到"直接问"；这是所有厂商把 AI 问答作为旗舰付费能力的共同原因（Notion AI、Microsoft Copilot、NotebookLM Plus）。
2. **从"囤积"到"消化"**：P4 是唯一直接作用于学习效果的能力——划线/笔记转卡片、测验、音频对谈；与 SRS 产品的目标函数（长期记住）同向。
3. **从"手工整理"到"确认整理"**：P3 把整理成本压缩到一个确认动作；标签/属性质量决定后续检索上限。
4. **从"单库"到"中枢"**：P5 让"在哪找"消失；权限随人走（"以你的身份"）是跨库成立的前提。
5. **信任本身是产品能力**：引用回溯、查询时权限、默认不训练、零保留——这些不是合规脚注，而是被厂商写成帮助文档并对外承诺的卖点（见 §3 各引文）。
6. **隐私分层成为差异化空间**：云厂商做到"不训练/零保留/权限内"，本地产品可以做到"数据不出设备"，后者是纯本地知识库相对大厂产品少有的结构性优势。

## 6. 主要风险与约束

### 6.1 幻觉与"人必须在场"
OpenAI 官方安全指南把幻觉列为一等局限，并要求开发者做人工复核："我们建议尽可能让人类在投入使用前复核输出……人应能拿到验证输出所需的全部信息（例如应用在总结笔记时，人应能轻松回看原始笔记）" — [Safety best practices](https://developers.openai.com/api/docs/guides/safety-best-practices)。Microsoft 在主要 Office 应用中提供可开关的免责声明"AI 生成内容可能不准确" — [AI Disclaimers](https://learn.microsoft.com/en-us/microsoft-365/copilot/microsoft-365-ai-disclaimers)；Notion 在自家 Enterprise Search 文档里直接提醒用户"务必核查所有答案的准确性"（"Be sure to double-check all answers for accuracy." — [Enterprise Search](https://www.notion.com/help/enterprise-search)）。产品级对策：引用回溯（P1）、来源外拒答（NotebookLM）、学习模式不直接给答案（Claude）。

### 6.2 权限与治理
AI 不应放大既有访问边界——三家都有明文：Microsoft"数据访问始终以登录用户权限为界，服务边界内不赋予租户级可见性" — [Copilot 架构](https://learn.microsoft.com/en-us/microsoft-365/copilot/microsoft-365-copilot-architecture)；Notion"LLM 看不到也用不了用户本无权访问的信息" + "权限在查询时校验" — [安全实践](https://www.notion.com/help/notion-ai-security-practices)、[Enterprise Search 安全实践](https://www.notion.com/help/enterprise-search-security-and-privacy-practices)；Azure RAG 指南要求"用户与 agent 只能取回被授权内容"，经典 RAG 用文档级安全裁剪/查询时过滤 — [RAG overview](https://learn.microsoft.com/en-us/azure/search/retrieval-augmented-generation-overview)。对个人本地应用，对应物是"本机多用户/共享目录"场景，通常不成立但需自觉。

### 6.3 隐私与数据留存
各厂商"默认不训练"的原文见 §3/§1.4。留存参数（同一来源，均一手）：OpenAI API 默认滥用监控日志至多 30 天，另提供 Zero Data Retention — [Data controls](https://developers.openai.com/api/docs/guides/your-data)；Notion Enterprise 计划 LLM 供应商默认零数据保留，页面/工作区删除后向量库中的 embeddings 60 天内删除 — [Notion AI 安全实践](https://www.notion.com/help/notion-ai-security-practices)；Google Workspace Gemini 的提示与响应保留"90 天至无限期，由管理员决定"，Gemini Notebook 会话结束不保留 — [Workspace 生成式 AI 隐私中心](https://knowledge.workspace.google.com/admin/generative-ai/generative-ai-in-google-workspace-privacy-hub)。**对本地产品：接云 API 即引入这些留存与子处理条款，需明示；纯端侧则天然为空集。**

### 6.4 成本与延迟
模型调用按 token 计费，检索质量与成本直接挂钩（OpenAI file search 文档明言：限制返回结果数"可减少 token 与延迟，但可能以答案质量为代价" — [File search](https://platform.openai.com/docs/guides/tools-file-search)）。缓释手段已成标配：prompt caching（OpenAI 缓存输入最高 -90%；Anthropic 读 10%/写 +25%；Gemini 隐式缓存默认开启，见 §4 表）、托管工具化（file search 由 OpenAI 托管）、按次计价的专项工具（Claude web search $10/千次）。

### 6.5 上下文与检索质量
"Lost in the Middle"：相关内容在长上下文中部时性能显著下降，"即使对明确的长上下文模型也是如此" — [arXiv:2307.03172](https://arxiv.org/abs/2307.03172)。分块大小/边界/重叠影响检索表现，需要按域调优并"始终跑评测"（Anthropic 实现注意事项） — [Contextual Retrieval](https://www.anthropic.com/engineering/contextual-retrieval)。RAG 范式本身出自 Lewis et al. 2020：参数化记忆（模型）+ 非参数化记忆（稠密向量索引）组合，生成"更具体、多样、事实性"的文本 — [arXiv:2005.11401](https://arxiv.org/abs/2005.11401)。

### 6.6 索引新鲜度与数据生命周期
连接器摄取最长 72 小时；断开后一小时不可搜、"数据将在断开一天内删除"；页面删除后向量 60 天内清除（Notion，来源同上）。设计 AI 功能时必须回答：内容改了/删了，索引与缓存何时跟上？

### 6.7 端侧能力上限
Apple 明文其端侧模型"不为世界知识与高级推理设计"（§P8 引文）。端侧路径的现实定位：受限任务的模板化生成（打标、出题、摘要、改写），而非开放问答。另需评估中文小模型质量与本机 API 访问边界。

### 6.8 生态锁定
多模型绑定（Notion 让用户在 GPT/Claude/Gemini 间选择 — [Enterprise Search](https://www.notion.com/help/enterprise-search)）与开放协议（MCP）是两个公开的解锁手段；自建向量层（Notion 自述迁移到 Turbopuffer 并用 Ray 跑开源 embedding 模型，"不再被外部供应商卡脖子" — [Notion 工程博客](https://www.notion.com/blog/two-years-of-vector-search-at-notion)）是第三种。

## 7. 对滩涂拾遗的启示（推论，非来源事实；供后续版本讨论）

> 本节是分析推论。产品现状：纯本地 SQLite、FTS5 trigram 中文子串搜索、SM-2 简化回顾、无账号无云；PRODUCT.md 定位"重读轻写"、反"AI 风模板感"；v0.2 PRD 把 AI 功能列为非目标，但 P2 候选里已有"AI 问题面"。

| 模式 | 与本产品的契合点 | 主要障碍 |
|---|---|---|
| P4 学习材料再生产 | 与 SRS 目标函数同构：AI 生成问题面 → 用户主动回忆 → 评分回流；NotebookLM 的"来源外拒答"与 Claude 学习模式的"引导不给答案"是可直接借鉴的交互范式 | 生成的卡片质量需要人工确认流与淘汰机制，否则污染复习队列（对应 v0.2 刚建好的"移出回顾"能力） |
| P1 混合检索 | 现有 FTS5 trigram 即词法腿（≈BM25）；按 Azure/Anthropic 的收敛形态补一条 embedding 腿 + 融合重排即为"混合检索"，中文分块与 embedding 选型是关键变量 | 中文 embedding 质量评估、向量存储落 SQLite 或本地文件的选型 |
| P3 自动打标 | 解决"划线多、标签少"的冷启动；受控词表 + 确认流 | 错标签污染筛选；需要批量复核 UI |
| P8 端侧推理 | 与"无账号、无云、无遥测"零冲突；macOS 上 Apple Foundation Models 随 OS 分发，Ollama 是进阶用户路径 | 3B 级模型只适合模板化任务（打标/出题/摘要），不适合开放问答；中文质量需实测 |
| 云 API 备选（BYO-Key） | 复用 keystore.rs 的 key 文件模式；厂商"默认不训练"承诺可写进隐私说明 | 与纯本地叙事的张力：数据一旦出设备，留存/子处理条款即适用（§6.3），须显式开关与明示 |

三条设计底线（从 §6 直接导出）：**一、任何 AI 产出必须可回溯到原文（引用/高亮定位），不可回溯就不上；二、AI 产出先入"草稿/建议"态，人确认后才成为卡片的正式字段；三、默认端侧或显式 BYO-Key，绝不静默上传。** UI 上，AI 应隐入"回顾/编辑"流（生成问题面、建议标签），不做仪表盘——与 PRODUCT.md 反"AI 风"的既定审美一致。

## 8. 来源清单（全部为官方一手来源，均经实际抓取核对）

**Notion**
1. Enterprise Search in Notion — https://www.notion.com/help/enterprise-search
2. What is Notion AI? FAQs — https://www.notion.com/help/notion-ai-faqs
3. Notion AI Connectors — https://www.notion.com/help/notion-ai-connectors
4. Notion AI security & privacy practices — https://www.notion.com/help/notion-ai-security-practices
5. Enterprise Search security & privacy practices — https://www.notion.com/help/enterprise-search-security-and-privacy-practices
6. Notion AI autofill for databases — https://www.notion.com/help/autofill
7. Two years of vector search at Notion（官方工程博客）— https://www.notion.com/blog/two-years-of-vector-search-at-notion

**Google**
8. Gemini Notebook（原 NotebookLM）FAQ — https://support.google.com/gemininotebook/answer/16269187?hl=en
9. Generate Audio Overview — https://support.google.com/gemininotebook/answer/16212820?hl=en
10. Generate Flashcards or Quizzes — https://support.google.com/gemininotebook/answer/16958963?hl=en
11. 输出类型与隐私（chats/audio/video/reports/mind maps）— https://support.google.com/gemininotebook/answer/16179536?hl=en
12. Collaborate with Gemini in Google Docs — https://support.google.com/docs/answer/14206696?hl=en
13. Write & edit with Gemini in Docs — https://support.google.com/docs/answer/13447609?hl=en
14. Generative AI in Google Workspace Privacy Hub — https://knowledge.workspace.google.com/admin/generative-ai/generative-ai-in-google-workspace-privacy-hub
15. Grounding with Google Search（Gemini API）— https://ai.google.dev/gemini-api/docs/google-search
16. Document understanding（Gemini API）— https://ai.google.dev/gemini-api/docs/document-processing
17. Context caching（Gemini API）— https://ai.google.dev/gemini-api/docs/caching
18. Gemini 2.5 Pro 模型文档 — https://ai.google.dev/gemini-api/docs/models/gemini-2.5-pro
19. Vertex AI Search（更名 Agent Search）— https://cloud.google.com/generative-ai-app-builder/docs/introduction
20. RAG Engine overview — https://docs.cloud.google.com/gemini-enterprise-agent-platform/build/rag-engine/rag-overview

**OpenAI**
21. File search — https://platform.openai.com/docs/guides/tools-file-search
22. Vector embeddings — https://platform.openai.com/docs/guides/embeddings
23. Function calling — https://platform.openai.com/docs/guides/function-calling
24. Prompt caching — https://platform.openai.com/docs/guides/prompt-caching
25. Apps in ChatGPT（connectors）— https://help.openai.com/en/articles/11487775-connectors-in-chatgpt
26. Deep research in ChatGPT — https://help.openai.com/en/articles/10500283-deep-research
27. Introducing deep research — https://openai.com/index/introducing-deep-research
28. Projects in ChatGPT — https://help.openai.com/en/articles/10169521-using-projects-in-chatgpt
29. Memory FAQ — https://help.openai.com/articles/8590148-memory-faq
30. Data controls in the OpenAI platform — https://developers.openai.com/api/docs/guides/your-data
31. Safety best practices — https://developers.openai.com/api/docs/guides/safety-best-practices

**Anthropic**
32. Citations — https://docs.claude.com/en/docs/build-with-claude/citations
33. Prompt caching — https://docs.claude.com/en/docs/build-with-claude/prompt-caching
34. Introducing Contextual Retrieval（官方工程博客）— https://www.anthropic.com/engineering/contextual-retrieval
35. MCP：What is MCP — https://modelcontextprotocol.io/docs/getting-started/intro
36. MCP：Architecture — https://modelcontextprotocol.io/docs/learn/architecture
37. What are projects? — https://support.claude.com/en/articles/9517075-what-are-projects
38. Web search tool — https://docs.claude.com/en/docs/agents-and-tools/tool-use/web-search-tool
39. Introducing Claude for Education — https://www.anthropic.com/news/introducing-claude-for-education

**Microsoft**
40. Microsoft Copilot architecture — https://learn.microsoft.com/en-us/microsoft-365/copilot/microsoft-365-copilot-architecture
41. Data, Privacy, and Security for Microsoft Copilot — https://learn.microsoft.com/en-us/microsoft-365/copilot/microsoft-365-copilot-privacy
42. Semantic index for Copilot — https://learn.microsoft.com/en-us/microsoftsearch/semantic-index-for-copilot
43. Hybrid search in Azure AI Search — https://learn.microsoft.com/en-us/azure/search/hybrid-search-overview
44. RAG in Azure AI Search — https://learn.microsoft.com/en-us/azure/search/retrieval-augmented-generation-overview
45. Integrated vectorization — https://learn.microsoft.com/en-us/azure/search/vector-search-integrated-vectorization
46. Turn on AI Disclaimers — https://learn.microsoft.com/en-us/microsoft-365/copilot/microsoft-365-ai-disclaimers

**Apple / Ollama / 论文**
47. Meet the Foundation Models framework（WWDC25）— https://developer.apple.com/videos/play/wwdc2025/286/
48. Code-along: on-device AI with Foundation Models（WWDC25）— https://developer.apple.com/videos/play/wwdc2025/259/
49. Ollama API introduction — https://docs.ollama.com/api/introduction
50. Ollama Quickstart — https://docs.ollama.com/quickstart
51. Lewis et al. 2020, Retrieval-Augmented Generation for Knowledge-Intensive NLP Tasks — https://arxiv.org/abs/2005.11401
52. Liu et al., Lost in the Middle — https://arxiv.org/abs/2307.03172

唯一以 `[未复验]` 引用的条目：Ollama Authentication（https://docs.ollama.com/api/authentication）。搜索发现但未抓取、故未引用的官方页面：Google Workspace AI 隐私页（workspace.google.com/security/ai-privacy）、OpenAI《Why language models hallucinate》、Notion Q&A 发布博客等。

## 9. 调研方法与局限

- **验证方式**：三条并行研究通道于 2026-09-01 各自用网页抓取工具实际取回全部引用页面并摘录原文；主笔对三个承重页面（MCP 介绍、OpenAI File Search、Anthropic Contextual Retrieval）做了独立复验，内容一致。所有引句为当日页面快照，厂商文档会随版本更新，引用前建议按来源清单复核现行页面。
- **已知空白**：(1) Obsidian 无任何官方 AI 立场/功能页（仅首页有"本地/离线"隐私定位），故正文未引用；(2) NotebookLM 的"报告/思维导图"仅在输出类型枚举页出现，无独立功能文档；(3) Apple 端侧模型的推理成本优势是公开报道的常识，但未取得逐字官方表述，本文未采信该表述；(4) OpenAI 平台文档已迁移至 developers.openai.com 域名，旧 platform.openai.com/docs 链接仍可解析。
- **本文件只增不改**：本文为独立调研产物，未改动仓库其他任何文件。
