import { invoke } from "@tauri-apps/api/core";

interface ImportResult {
  files_imported: number;
  chunks_created: number;
  skipped: number;
}

interface SearchResult {
  content: string;
  file_name: string;
  file_path: string;
  chunk_index: number;
}

// ── 分页状态 ──────────────────────────────────────────────────────────────────

const PAGE_SIZE = 10;
let allResults: SearchResult[] = [];
let currentQuery = "";
let shownCount = 0;

// ── DOM 引用 ──────────────────────────────────────────────────────────────────

const $ = <T extends HTMLElement>(id: string) =>
  document.getElementById(id) as T;

// ── 统计信息 ──────────────────────────────────────────────────────────────────

async function loadStats() {
  try {
    const stats = await invoke<{ files: number; chunks: number }>("get_stats");
    $("stat-files").textContent = String(stats.files);
    $("stat-chunks").textContent = String(stats.chunks);
    if (stats.files > 0) $("empty-state").style.display = "none";
  } catch (e) {
    console.error("get_stats failed:", e);
  }
}

// ── 导入 ──────────────────────────────────────────────────────────────────────

async function importFolder() {
  const btn = $<HTMLButtonElement>("import-btn");
  const status = $("import-status");
  btn.disabled = true;
  btn.textContent = "导入中...";
  status.textContent = "";
  status.className = "import-status";
  try {
    const r = await invoke<ImportResult>("select_and_import_folder");
    status.textContent =
      `已导入 ${r.files_imported} 个文件，生成 ${r.chunks_created} 个段落` +
      (r.skipped > 0 ? `（跳过 ${r.skipped} 个）` : "");
    status.className = "import-status success";
    await loadStats();
  } catch (e) {
    const msg = String(e);
    status.textContent = msg === "cancelled" ? "已取消" : `导入失败: ${msg}`;
    if (msg !== "cancelled") status.className = "import-status error";
  } finally {
    btn.disabled = false;
    btn.innerHTML = '<span class="btn-icon">&#8853;</span> 导入 TXT 文件夹';
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
  const esc = keyword.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return safe.replace(new RegExp(`(${esc})`, "gi"), "<mark>$1</mark>");
}

/**
 * 提取关键词前后 context 个字符的上下文摘要。
 * 返回摘要文本 和 isFull 标志（是否已包含完整内容）。
 */
function extractSnippet(
  content: string,
  keyword: string,
  context = 50
): { text: string; isFull: boolean } {
  const lower = content.toLowerCase();
  const kw = keyword.toLowerCase();
  const idx = lower.indexOf(kw);

  // 找不到时（极端情况），显示开头
  if (idx === -1) {
    const isFull = content.length <= context * 2;
    return {
      text: isFull ? content : content.slice(0, context * 2) + "…",
      isFull,
    };
  }

  const start = Math.max(0, idx - context);
  const end = Math.min(content.length, idx + keyword.length + context);
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
  <path d="M6 8h5M6 11h3" stroke-opacity="0.7"/>
</svg>`;

function buildCard(r: SearchResult, query: string): HTMLElement {
  const card = document.createElement("div");
  card.className = "result-card";

  const { text: snippetText, isFull } = extractSnippet(r.content, query);
  const snippetHtml = highlight(snippetText, query);
  const fullHtml = highlight(r.content, query);

  card.innerHTML = `
    <div class="card-header">
      <span class="card-file-icon">${FILE_ICON}</span>
      <span class="card-file-name" title="${escapeHtml(r.file_path)}">${escapeHtml(r.file_name)}</span>
      <span class="card-chunk-badge">段落&nbsp;#${r.chunk_index + 1}</span>
    </div>
    <div class="card-snippet">${snippetHtml}</div>
    <div class="card-full" style="display:none">${fullHtml}</div>
    ${isFull ? "" : '<button class="card-expand-btn" data-expanded="false">展开全文 ↓</button>'}
  `;

  if (!isFull) {
    const btn = card.querySelector(".card-expand-btn") as HTMLButtonElement;
    const snippetEl = card.querySelector(".card-snippet") as HTMLElement;
    const fullEl = card.querySelector(".card-full") as HTMLElement;

    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      const expanded = btn.dataset.expanded === "true";
      if (expanded) {
        fullEl.style.display = "none";
        snippetEl.style.display = "";
        btn.textContent = "展开全文 ↓";
        btn.dataset.expanded = "false";
        btn.classList.remove("is-expanded");
      } else {
        snippetEl.style.display = "none";
        fullEl.style.display = "";
        btn.textContent = "收起 ↑";
        btn.dataset.expanded = "true";
        btn.classList.add("is-expanded");
      }
    });
  }

  return card;
}

// ── 分页渲染 ──────────────────────────────────────────────────────────────────

function renderPage() {
  const list = $("results-list");
  const lmBtn = list.querySelector(".load-more-btn") as HTMLElement;
  const count = $("results-count");

  const batch = allResults.slice(shownCount, shownCount + PAGE_SIZE);
  for (const r of batch) {
    list.insertBefore(buildCard(r, currentQuery), lmBtn);
  }
  shownCount += batch.length;

  // 更新计数提示
  if (allResults.length > PAGE_SIZE) {
    count.textContent = `找到 ${allResults.length} 条结果，已显示 ${shownCount} 条`;
  }

  // 控制「加载更多」按钮
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

  allResults = [];
  shownCount = 0;
  currentQuery = query;

  const list = $("results-list");
  const header = $("results-header");
  const count = $("results-count");

  // 清空并重建（保留 load-more 占位）
  list.innerHTML = "";
  const lmBtn = document.createElement("button");
  lmBtn.className = "load-more-btn";
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
    count.textContent = `找到 ${results.length} 条结果`;
    renderPage();
  } catch (e) {
    count.textContent = `搜索出错: ${e}`;
  }
}

// ── 初始化 ────────────────────────────────────────────────────────────────────

window.addEventListener("DOMContentLoaded", () => {
  loadStats();
  $("import-btn").addEventListener("click", importFolder);
  $("search-btn").addEventListener("click", doSearch);
  $<HTMLInputElement>("search-input").addEventListener("keydown", (e) => {
    if (e.key === "Enter") doSearch();
  });
});
