# LOCALLENS 产品规划文档（最终版）

**Your files. Your device. Your answers.**

最轻量的本地 AI 文档语义搜索工具，开箱即用。

> 版本 1.0 — 最终版 | 2026年2月 | 保密文件

---

## 1. 执行摘要

> LocalLens 是一款轻量级、隐私优先的 AI 文档搜索工具，完全运行在用户本地设备上。用户可以用自然语言搜索所有本地文档——不上传云端、不需要 GPU、不需要注册账号。核心差异化：在竞品需要 32GB+ 内存的场景下，我们只需 8GB。

### 1.1 要解决的问题

知识工作者平均每天花 2.5 小时搜索信息。现有方案迫使用户做一个不可能的取舍：上传到云端（隐私风险）或使用重量级本地工具（需要 32GB+ 内存和独立显卡）。

结果：大多数人仍然依赖 Ctrl+F 和文件夹浏览。只能搜文件名，不能搜文件内容；只能搜关键词，不能搜语义。

### 1.2 解决方案

LocalLens 在用户 CPU 上运行一个小型高效的 AI 模型（23MB）来理解文档语义。用户指向文件夹，工具自动建立可搜索索引。然后用自然语言提问，即可获得文件中的精确段落，附带来源引用。

### 1.3 核心差异化对比

| 维度 | Hyperlink AI | PrivateGPT | LocalLens |
|------|-------------|------------|-----------|
| 最低内存 | 18GB（推荐 32GB） | 16GB+ | **8GB** |
| 是否需要 GPU | 推荐 | 推荐 | **不需要** |
| 安装包大小 | 1GB+ | 500MB+ | **约 35MB** |
| 配置复杂度 | 中等 | 高（面向开发者） | **零配置** |
| 跨设备同步 | 无 | 无 | **计划中（Pro 版）** |
| 价格 | 免费（暂时） | 免费（开源） | **Freemium** |

---

## 2. 目标市场与用户画像

### 2.1 主战场

英文海外市场（美国、英国、欧盟、澳大利亚）。原因：付费意愿更高，分发渠道成熟（Product Hunt、Hacker News、Reddit），对隐私工具的需求更强。

产品界面英文，但内置多语言 embedding 模型，天然支持中文文档搜索。中文用户也能用，但不专门运营中文市场。

### 2.2 用户画像

#### 画像 A：研究者

- **谁：** 博士生、学术研究人员、分析师
- **痛点：** 数百篇 PDF 论文，记不清哪篇论文说了什么
- **场景：** "找到讨论 X 和 Y 相关性的那篇论文"
- **付费意愿：** 中等（$8–12/月）

#### 画像 B：隐私敏感型专业人士

- **谁：** 律师、咨询顾问、财务顾问
- **痛点：** 处理机密文件，不能使用云端 AI 工具
- **场景：** "在我的合同里搜索特定条款，不上传任何内容"
- **付费意愿：** 高（$12–20/月 或 $149–199 买断）

#### 画像 C：知识囤积者

- **谁：** 写作者、内容创作者、有大量笔记的开发者
- **痛点：** 多年积累的笔记、文章、电子书分散在各种格式中
- **场景：** "找2年前写过的一篇关于这个主题的东西"
- **付费意愿：** 中等（$8/月 或买断）

---

## 3. 产品路线图

> **策略：6 周内发布聚焦的 MVP。用真实用户验证。验证成功后再投入高级功能。跨设备同步是 Pro 版升级功能，不是 MVP。**

### 3.1 第一阶段：MVP（第 1–6 周）

*目标：最小可用产品，证明核心价值——本地语义搜索开箱即用。*

**核心功能：**

1. **文件夹导入** — 拖拽或浏览选择文件夹。支持 PDF、TXT、Markdown、DOCX 格式。
2. **本地索引** — 自动提取文本、分段、生成嵌入向量。使用 all-MiniLM-L6-v2（23MB ONNX 模型）。后台处理，带进度条。
3. **语义搜索** — 自然语言搜索框。返回 Top-K 相关段落，附带源文件名、页码/章节引用、相似度分数。
4. **打开源文件** — 点击搜索结果，用系统默认程序打开原始文件。

**MVP 不包含的功能：**

- 不包含 LLM / AI 对话 / RAG（仅做 embedding 搜索，更快更轻）
- 不包含跨设备同步
- 不包含扫描版 PDF 的 OCR
- 不包含自动更新机制（MVP 阶段手动下载）

### 3.2 第二阶段：付费价值（第 3–4 个月）

*目标：增加让用户愿意付费的功能。*

1. **AI 问答模式** — 通过本地 Ollama 实现 RAG 问答。同时支持可选的 OpenAI/Claude API。
2. **更多格式支持** — EPUB、HTML、.eml 邮件文件、Notion 导出 ZIP。
3. **智能文件夹监控** — 文件变化时自动增量索引，无需手动重新导入。
4. **多语言嵌入模型** — 切换到 multilingual-e5-small，支持中文、日文等。
5. **搜索历史与收藏** — 保存常用搜索，标记重要结果。

### 3.3 第三阶段：增长与护城河（第 5–8 个月）

*目标：构建跨设备同步作为核心差异化和付费升级驱动力。*

1. **跨设备索引同步（Pro）** — 端到端加密的 SQLite 同步，通过用户自己的云存储（iCloud/Google Drive/Dropbox）实现。在家里的笔记本上搜索办公室电脑的文档。原始文件永远不离开源设备。
2. **OCR 支持** — 搜索扫描版 PDF 文档。
3. **文档关系图谱** — 可视化展示相关文档之间的联系。
4. **Excel/CSV 内容搜索** — 搜索电子表格单元格内容。目前没有竞品做得好。

---

## 4. 技术架构

> **设计原则：零外部依赖。用户下载一个文件，双击即可运行。不需要 Python、Node.js、Docker、GPU 驱动。**

### 4.1 技术栈

| 组件 | 技术选型 | 选型理由 |
|------|---------|---------|
| 桌面外壳 | Tauri 2.0 | 约10MB 安装包，原生体感，系统 WebView |
| 后端逻辑 | Rust | 快速、安全、编译为单一二进制 |
| ML 推理 | ONNX Runtime（ort crate） | CPU 推理，无 Python 依赖 |
| 嵌入模型 | all-MiniLM-L6-v2（23MB） | 384维向量，CPU 上快速运行 |
| 向量存储 | SQLite + sqlite-vec | 单文件数据库，零配置 |
| 文档解析 | Rust 原生库（lopdf, docx-rs 等） | 无外部工具依赖 |
| 前端 UI | HTML + 原生 JS | 简单，无框架开销 |
| 文件监控 | notify crate | 跨平台文件系统监控 |

### 4.2 架构图

```
Tauri 2.0 Shell
├─ 前端（HTML/JS）← 搜索 UI、设置页、索引状态
│    │ Tauri IPC（invoke）
├─ Rust 后端
│    ├─ 文件监控器（notify crate）
│    ├─ 文档解析器（PDF、DOCX、TXT、MD）
│    ├─ 文本分段器（语义感知切分）
│    ├─ ONNX Runtime（嵌入向量推理）
│    └─ SQLite + sqlite-vec（向量 + 元数据存储）
```

### 4.3 数据库结构

```sql
-- 已索引的文件
CREATE TABLE files (
    id INTEGER PRIMARY KEY,
    path TEXT UNIQUE NOT NULL,
    filename TEXT NOT NULL,
    format TEXT NOT NULL,          -- 'pdf', 'docx', 'txt', 'md', 'epub'
    size_bytes INTEGER,
    modified_at TEXT,
    indexed_at TEXT DEFAULT (datetime('now')),
    chunk_count INTEGER DEFAULT 0
);

-- 文本分段
CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    file_id INTEGER REFERENCES files(id) ON DELETE CASCADE,
    chunk_index INTEGER,
    content TEXT NOT NULL,
    char_start INTEGER,
    char_end INTEGER
);

-- 向量索引（sqlite-vec 虚拟表）
CREATE VIRTUAL TABLE chunk_embeddings USING vec0(
    chunk_id INTEGER PRIMARY KEY,
    embedding FLOAT[384]
);
```

### 4.4 文本分段策略

- 按段落分割（双换行符）
- 段落超过 500 字符时，按句子切分并重新组合到 300–500 字符
- 相邻 chunk 之间 50 字符重叠，保证跨边界上下文
- 存储每个 chunk 在原文中的精确位置（char_start, char_end）

### 4.5 搜索流程

```
用户查询
  → ONNX Runtime 编码为 384 维向量
    → sqlite-vec 余弦相似度搜索
      → 返回 Top-K chunk
        → 关联文件元数据
          → 展示：文件名 + 段落 + 分数
```

### 4.6 性能目标

| 指标 | 目标值 | 备注 |
|------|-------|------|
| 最低内存 | 系统 8GB | 应用本身约使用 100MB |
| 安装包大小 | <35MB | Tauri 外壳 + ONNX 模型 |
| 索引速度 | CPU 上 50–100 chunks/秒 | 后台处理，不阻塞 |
| 搜索延迟 | 100K chunks 下 <200ms | sqlite-vec HNSW |
| 冷启动 | <2 秒 | Tauri 优于 Electron |

---

## 5. 商业模式

### 5.1 定价策略

| 层级 | 价格 | 功能 |
|------|------|------|
| 免费版 | $0 | 最多索引 500 个文件。基础语义搜索。无 AI 问答。 |
| Pro（订阅） | $8–12/月 或 $79/年 | 无限文件。AI 问答。OCR。跨设备同步。优先更新。 |
| Pro（买断） | $149–199 | 功能同订阅版。含 1 年更新。续费可选 $49/年。 |

提供一次性买断是有意为之的竞争优势。隐私导向的用户强烈偏好拥有而非订阅。Obsidian 已经证明这种模式对开发者工具是有效的。

### 5.2 收入预期（保守估算）

| 时间线 | 免费用户 | 付费用户 | MRR 预估 |
|--------|---------|---------|---------|
| 第 1–2 月（MVP 发布） | 100–300 | 5–20 | $50–200 |
| 第 3–4 月（Pro 上线） | 1,000+ | 50–100 | $500–1,000 |
| 第 6–8 月（稳定增长） | 5,000+ | 200–500 | $2,000–5,000 |
| 第 12 月（成熟目标） | 10,000+ | 500–1,000 | $5,000–10,000 |

备注：这是保守估计。如果第 6 个月 MRR 达到 $3K+，年化收入约 $36K–60K，可以作为一个不错的副业收入。

---

## 6. 上市推广策略

### 6.1 发布前（MVP 开发期间）

- 在 X/Twitter 上 #buildinpublic 公开构建过程
- 参与 Reddit r/selfhosted、r/LocalLLaMA、r/productivity 讨论
- 英文落地页 + waitlist（Cloudflare Pages + ConvertKit）
- **验证目标：2 周内收集 100+ waitlist 注册**

### 6.2 发布日

- Product Hunt 发布（目标当日前 5）
- Hacker News Show HN 帖子
- Reddit 相关子版发帖
- 主动联系报道隐私工具的科技博主和 YouTuber

### 6.3 持续增长

- SEO 内容：博客文章瞄准 "best local AI search"、"private document search" 等长尾关键词
- YouTube 演示视频展示真实使用场景
- GitHub 开源核心搜索引擎（MIT 许可）构建信任和社区
- Obsidian/Logseq 社区插件，接入现有用户生态

### 6.4 分发基础设施

| 资产 | 平台 | 成本 |
|------|------|------|
| 落地页 | Cloudflare Pages | 免费 |
| 域名 | locallens.app 或类似 | 约 $10/年 |
| 邮件收集 | ConvertKit | 免费（10K 订阅者以内） |
| 应用安装包 | GitHub Releases | 免费 |
| 自动更新 | Tauri 内置 updater | 免费（基于 GitHub） |

> **总运营成本：约 $10/年（仅域名费用）。其余全部使用免费服务。**

---

## 7. 竞品分析

| 竞品 | 优势 | 劣势 | 你的优势 |
|------|------|------|---------|
| Hyperlink AI | 完整 RAG、免费、NVIDIA 背书 | 需 32GB+ 内存，无跨设备 | 8GB 即可运行，计划同步 |
| LumiFind | 100+ 格式、多语言 | 付费（众筹）、无 RAG | 开源核心，更轻量 |
| LaSearch | 轻量、简洁 UX | 仅 Mac、Beta、单设备 | 跨平台，更多格式 |
| Spacedrive | P2P 同步、Rust、开源 | 文件管理器，非内容搜索 | 专注文档内容 |
| PrivateGPT | 完整 RAG、自托管 | 面向开发者、配置复杂 | 零配置消费级应用 |
| Reor | AI 笔记、本地、开源 | 仅支持 Markdown | 支持所有文档格式 |

---

## 8. 关键风险与应对

1. **平台风险（苹果/微软内置类似功能）：** 操作系统级搜索会是通用的。深度用户需要的精细度（多格式、跨设备同步、高级过滤）是平台默认功能不会提供的。快速行动，在平台动手之前建立用户忠诚度。

2. **免费竞品主导（Hyperlink AI 永久免费）：** 不在功能上竞争，在轻量级 + 跨设备上竞争。无法运行 Hyperlink（8GB 机器）的用户目前没有替代品。

3. **Rust 学习曲线拖慢开发：** 大量使用 AI 辅助开发（Claude、Copilot）。前 2 周纯学习 Rust 基础，不写产品代码。接受前期慢速。

4. **本地 embedding 质量不够好：** 提供可选的云端 API 集成（OpenAI/Claude）。默认保持本地运行保障隐私。

---

## 9. 开发时间表

| 周次 | 里程碑 | 交付物 |
|------|--------|--------|
| 第 1–2 周 | Rust 基础学习 | 命令行工具：读取并打印文件内容 |
| 第 3–4 周 | 核心管线 | Tauri 应用：导入文件夹 → 解析 → 嵌入 → 存储 |
| 第 5–6 周 | 搜索 + 打磨 | 完整搜索 UI、DOCX/MD/EPUB 支持、打包 |
| 第 7 周 | 落地页 + waitlist | Cloudflare Pages 上线，ConvertKit 表单 |
| 第 8 周 | 公开发布 | Product Hunt、HN、Reddit 发帖 |
| 第 9–12 周 | 迭代反馈 | Bug 修复、格式支持、准备第二阶段 |
| 第 13–16 周 | 第二阶段：Pro 功能 | AI 问答、文件夹监控、支付集成 |
| 第 17–24 周 | 第三阶段：跨设备 | 加密索引同步、Pro 版发布 |

---

## 10. 本周立即行动

> **以下是本周就可以开始做的事情——在做任何其他事情之前先完成这些。**

### 第 1–2 天：搭建开发环境

- 通过 rustup 安装 Rust（rustup.rs）
- 安装 RustRover（或 VS Code + rust-analyzer）
- 阅读 Rust Book 第 1–5 章（所有权、借用、结构体）
- 写一个 "hello world" 程序，读取文件并打印内容

### 第 3–5 天：第一个 Rust 练习

- 构建一个命令行工具，递归遍历目录
- 提取 .txt 文件的文本内容
- 统计总词数和文件数
- 学习点：文件 I/O、错误处理、迭代器、String vs &str

### 第 6–7 天：Tauri 项目初始化

- 安装 Tauri CLI（cargo install tauri-cli）
- 创建新 Tauri 项目，使用原生 HTML 前端
- 实现一个简单页面：搜索框通过 IPC 调用 Rust 函数
- 验证可以在你的机器上构建和运行

### 持续进行：验证工作

- 注册域名（locallens.app 或 locallens.dev）
- 在 Cloudflare Pages 上部署落地页（已创建的 HTML 文件）
- 连接 ConvertKit 收集 waitlist 邮箱
- 在 Reddit r/selfhosted 发帖："I'm building a lightweight local AI search engine for your documents — would you use this?"

---

*文档结束。祝 LocalLens 顺利。加油！*