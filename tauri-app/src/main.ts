import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ── 类型定义 ──────────────────────────────────────────────────────────────────

interface ImportResult {
  files_imported: number;
  chunks_created: number;
  skipped: number;
  embeddings_generated: number;
}

interface SearchResult {
  content: string;
  file_name: string;
  file_path: string;
  chunk_index: number;
  score: number;       // 语义相似度 0–1（关键词模式为 0）
  is_semantic: boolean;
}

interface ImportProgressPayload {
  current: number;
  total: number;
  file: string;
  phase: "reading" | "embedding" | "done";
  chunk?: number;
  chunks?: number;
}

// ── 分页状态 ──────────────────────────────────────────────────────────────────

const PAGE_SIZE = 10;
let allResults: SearchResult[] = [];
let currentQuery = "";
let shownCount = 0;

// ── DOM 工具 ──────────────────────────────────────────────────────────────────

const $ = <T extends HTMLElement>(id: string) =>
  document.getElementById(id) as T;

// ── 模型状态 ──────────────────────────────────────────────────────────────────

type ModelStatus = "loading" | "ready" | "unavailable" | `failed:${string}`;

async function initModelStatus() {
  try {
    const s = await invoke<string>("get_model_status");
    applyModelStatus(s as ModelStatus);
  } catch {
    // ignore
  }

  await listen<string>("model-status", (event) => {
    applyModelStatus(event.payload as ModelStatus);
  });
}

function applyModelStatus(status: ModelStatus) {
  const badge = $("model-badge");
  const indicator = $("model-indicator");

  badge.classList.remove("loading", "ready", "failed", "unavailable");

  if (status === "ready") {
    badge.textContent = "语义搜索";
    badge.classList.add("ready");
    indicator.title = "all-MiniLM-L6-v2 已就绪";
  } else if (status === "loading") {
    badge.textContent = "模型加载中…";
    badge.classList.add("loading");
    indicator.title = "正在加载 ONNX 模型";
  } else if (status === "unavailable") {
    badge.textContent = "关键词模式";
    badge.classList.add("unavailable");
    indicator.title = "模型文件未找到，使用关键词搜索";
  } else {
    badge.textContent = "关键词模式";
    badge.classList.add("failed");
    const reason = status.startsWith("failed:") ? status.slice(7) : status;
    indicator.title = `模型加载失败: ${reason}`;
  }
}

// ── 统计信息 ──────────────────────────────────────────────────────────────────

async function loadStats() {
  try {
    const stats = await invoke<{ files: number; chunks: number; embeddings: number }>(
      "get_stats"
    );
    $("stat-files").textContent = String(stats.files);
    $("stat-chunks").textContent = String(stats.chunks);
    $("stat-embeddings").textContent = String(stats.embeddings);
    if (stats.files > 0) $("empty-state").style.display = "none";
  } catch (e) {
    console.error("get_stats failed:", e);
  }
}

// ── 导入 ──────────────────────────────────────────────────────────────────────

let importUnlisten: UnlistenFn | null = null;

async function importFolder() {
  const btn = $<HTMLButtonElement>("import-btn");
  const statusEl = $("import-status");
  const progressWrap = $("progress-wrap");
  const progressBar = $<HTMLElement>("progress-bar");
  const progressLabel = $("progress-label");

  btn.disabled = true;
  btn.innerHTML = '<span class="btn-icon">⋯</span> 导入中…';
  statusEl.textContent = "";
  statusEl.className = "import-status";
  progressWrap.style.display = "block";
  progressBar.style.width = "0%";
  progressLabel.textContent = "准备中…";

  importUnlisten = await listen<ImportProgressPayload>(
    "import-progress",
    (event) => {
      const p = event.payload;
      if (p.phase === "done") {
        progressBar.style.width = "100%";
        progressLabel.textContent = "完成";
        return;
      }
      const pct = p.total > 0 ? Math.round((p.current / p.total) * 100) : 0;
      progressBar.style.width = `${pct}%`;
      const phaseText = p.phase === "embedding" ? `向量化段落…` : "读取文件…";
      progressLabel.textContent = `${p.current}/${p.total} ${phaseText} ${p.file}`;
    }
  );

  try {
    const result = await invoke<ImportResult>("select_and_import_folder");
    const embNote =
      result.embeddings_generated > 0
        ? `，生成 ${result.embeddings_generated} 条向量`
        : "（未生成向量，模型未就绪）";
    statusEl.textContent =
      `已导入 ${result.files_imported} 个文件，` +
      `${result.chunks_created} 个段落${embNote}` +
      (result.skipped > 0 ? `，跳过 ${result.skipped} 个` : "");
    statusEl.className = "import-status success";
    await loadStats();
  } catch (e) {
    const msg = String(e);
    if (msg === "cancelled") {
      statusEl.textContent = "已取消";
    } else {
      statusEl.textContent = `导入失败: ${msg}`;
      statusEl.className = "import-status error";
    }
  } finally {
    btn.disabled = false;
    btn.innerHTML = '<span class="btn-icon">⊕</span> 导入 TXT 文件夹';
    importUnlisten?.();
    importUnlisten = null;
    setTimeout(() => {
      progressWrap.style.display = "none";
    }, 1500);
  }
}

// ── 文本工具 ──────────────────────────────────────────────────────────────────

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function highlight(text: string, keyword: string): string {
  const safe = escapeHtml(text);
  if (!keyword.trim()) return safe;
  const esc = keyword.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return safe.replace(new RegExp(`(${esc})`, "gi"), "<mark>$1</mark>");
}

/**
 * 从 content 中提取摘要：
 * - 关键词存在时：取关键词前后各 contextChars 字符
 * - 语义搜索无精确匹配：显示前 150 字符
 * - 关键词搜索无匹配：显示前 2×contextChars 字符
 */
function extractSnippet(
  content: string,
  keyword: string,
  contextChars: number,
  isSemantic: boolean
): { text: string; isFull: boolean } {
  const lower = content.toLowerCase();
  const kw = keyword.trim().toLowerCase();
  const idx = kw.length > 0 ? lower.indexOf(kw) : -1;

  if (idx === -1) {
    const limit = isSemantic ? 150 : contextChars * 2;
    const isFull = content.length <= limit;
    return {
      text: isFull ? content : content.slice(0, limit) + "…",
      isFull,
    };
  }

  const start = Math.max(0, idx - contextChars);
  const end = Math.min(content.length, idx + keyword.length + contextChars);
  const isFull = start === 0 && end === content.length;
  let text = content.slice(start, end);
  if (start > 0) text = "…" + text;
  if (end < content.length) text += "…";
  return { text, isFull };
}

// ── 卡片构建 ──────────────────────────────────────────────────────────────────

const FILE_ICON = `<svg width="13" height="13" viewBox="0 0 16 16" fill="none"
  stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
  <path d="M3 1h7l4 4v10H3V1z"/>
  <path d="M10 1v4h4"/>
  <path d="M6 8h5M6 11h3" stroke-opacity="0.6"/>
</svg>`;

function buildCard(r: SearchResult, query: string): HTMLElement {
  const card = document.createElement("div");
  card.className = "result-card";

  const { text: snippetText, isFull } = extractSnippet(
    r.content, query, 80, r.is_semantic
  );
  const snippetHtml = highlight(snippetText, query);
  const fullHtml    = highlight(r.content,    query);

  // 底部徽章：KW / AI + 相似度
  const modeBadgeHtml = r.is_semantic
    ? `<span class="mode-badge ai-badge">AI</span><span class="score-val">${Math.round(r.score * 100)}%</span>`
    : `<span class="mode-badge kw-badge">KW</span>`;

  // 展开按钮（内容超出摘要时才显示）
  const expandBtnHtml = isFull
    ? ""
    : `<button class="card-expand-btn" data-expanded="false">展开全文 ↓</button>`;

  card.innerHTML = `
    <div class="card-header">
      <span class="card-file-icon">${FILE_ICON}</span>
      <span class="card-file-name" title="${escapeHtml(r.file_path)}">${escapeHtml(r.file_name)}</span>
      <span class="card-chunk-badge">段落&nbsp;#${r.chunk_index + 1}</span>
    </div>
    <div class="card-snippet">${snippetHtml}</div>
    <div class="card-full" style="display:none">${fullHtml}</div>
    <div class="card-footer">
      <div class="card-meta">${modeBadgeHtml}</div>
      ${expandBtnHtml}
    </div>
  `;

  if (!isFull) {
    const expandBtn = card.querySelector(".card-expand-btn") as HTMLButtonElement;
    const snippetEl = card.querySelector(".card-snippet") as HTMLElement;
    const fullEl    = card.querySelector(".card-full")    as HTMLElement;

    expandBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      const expanded = expandBtn.dataset.expanded === "true";
      if (expanded) {
        fullEl.style.display    = "none";
        snippetEl.style.display = "";
        expandBtn.textContent   = "展开全文 ↓";
        expandBtn.dataset.expanded = "false";
        expandBtn.classList.remove("is-expanded");
      } else {
        snippetEl.style.display = "none";
        fullEl.style.display    = "";
        expandBtn.textContent   = "收起 ↑";
        expandBtn.dataset.expanded = "true";
        expandBtn.classList.add("is-expanded");
      }
    });
  }

  return card;
}

// ── 分页渲染 ──────────────────────────────────────────────────────────────────

function renderPage() {
  const list  = $("results-list");
  const count = $("results-count");

  // 取出 lmBtn（可能已在 DOM 中），先移除再末尾追加，避免 insertBefore 引用问题
  let lmBtn = list.querySelector<HTMLButtonElement>(".load-more-btn");
  if (!lmBtn) return;
  lmBtn.remove();

  const batch = allResults.slice(shownCount, shownCount + PAGE_SIZE);
  for (const r of batch) {
    list.appendChild(buildCard(r, currentQuery));
  }
  shownCount += batch.length;

  // 将 lmBtn 追加回末尾
  list.appendChild(lmBtn);

  if (allResults.length > PAGE_SIZE) {
    count.textContent = `找到 ${allResults.length} 条结果，已显示 ${shownCount} 条`;
  }

  if (shownCount >= allResults.length) {
    lmBtn.style.display = "none";
  } else {
    lmBtn.style.display = "flex";
    lmBtn.textContent = `加载更多（还有 ${allResults.length - shownCount} 条）`;
  }
}

// ── 搜索 ──────────────────────────────────────────────────────────────────────

async function doSearch() {
  const query = ($<HTMLInputElement>("search-input")).value.trim();
  if (!query) return;

  allResults  = [];
  shownCount  = 0;
  currentQuery = query;

  const list   = $("results-list");
  const header = $("results-header");
  const count  = $("results-count");

  // 清空结果列表，放入新的 lmBtn
  list.innerHTML = "";
  const lmBtn = document.createElement("button");
  lmBtn.className     = "load-more-btn";
  lmBtn.style.display = "none";
  lmBtn.addEventListener("click", renderPage);
  list.appendChild(lmBtn);

  $("empty-state").style.display = "none";
  count.textContent = "搜索中…";
  header.style.display = "flex";

  try {
    const results = await invoke<SearchResult[]>("search_text", { query });

    if (results.length === 0) {
      count.textContent = `未找到「${query}」相关内容`;
      return;
    }

    allResults = results;
    const mode = results[0].is_semantic ? "语义搜索" : "关键词搜索";
    count.textContent = `${mode}：找到 ${results.length} 条结果`;
    renderPage();
  } catch (e) {
    count.textContent = `搜索出错: ${e}`;
  }
}

// ── 初始化 ────────────────────────────────────────────────────────────────────

window.addEventListener("DOMContentLoaded", async () => {
  await initModelStatus();
  loadStats();

  $("import-btn").addEventListener("click", importFolder);
  $("search-btn").addEventListener("click", doSearch);
  $<HTMLInputElement>("search-input").addEventListener("keydown", (e) => {
    if (e.key === "Enter") doSearch();
  });
});
