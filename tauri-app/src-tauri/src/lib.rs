mod embedding;

use embedding::{bytes_to_vec, cosine_sim, vec_to_bytes, EmbeddingModel};
use rusqlite::{Connection, Result as SqlResult};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use walkdir::WalkDir;

// ── 全局模型实例（静态，避免把非 Send 类型放进 Tauri managed state） ──────────

static MODEL: OnceLock<Mutex<Option<EmbeddingModel>>> = OnceLock::new();

fn model_lock() -> &'static Mutex<Option<EmbeddingModel>> {
    MODEL.get_or_init(|| Mutex::new(None))
}

// ── 应用状态 ──────────────────────────────────────────────────────────────────

/// 模型加载状态（存入 managed state 供命令查询）
#[derive(Clone)]
struct ModelStatusState(Arc<Mutex<ModelStatus>>);

#[derive(Clone, PartialEq)]
enum ModelStatus {
    Loading,
    Ready,
    Failed(String),
    Unavailable, // 资源文件不存在
}

impl ModelStatus {
    fn as_str(&self) -> String {
        match self {
            ModelStatus::Loading => "loading".into(),
            ModelStatus::Ready => "ready".into(),
            ModelStatus::Failed(e) => format!("failed:{e}"),
            ModelStatus::Unavailable => "unavailable".into(),
        }
    }
}

/// 内存中的向量缓存（避免每次搜索都查 DB）
struct VectorCache {
    /// (chunk_id, embedding)
    entries: Vec<(i64, Vec<f32>)>,
    /// false 表示导入后缓存失效，下次搜索时重建
    valid: bool,
}

impl VectorCache {
    fn new() -> Self {
        Self {
            entries: vec![],
            valid: false,
        }
    }
    fn invalidate(&mut self) {
        self.valid = false;
        self.entries.clear();
    }
}

#[derive(Clone)]
struct CacheState(Arc<RwLock<VectorCache>>);

// ── 数据结构 ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ImportResult {
    pub files_imported: usize,
    pub chunks_created: usize,
    pub skipped: usize,
    pub embeddings_generated: usize,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub content: String,
    pub file_name: String,
    pub file_path: String,
    pub chunk_index: i64,
    /// 语义相似度 0.0–1.0（关键词模式下为 0.0）
    pub score: f32,
    /// true = 语义搜索，false = 关键词回退
    pub is_semantic: bool,
}

// ── 数据库 ────────────────────────────────────────────────────────────────────

fn db_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().expect("app data dir");
    std::fs::create_dir_all(&dir).ok();
    dir.join("locallens.db")
}

fn open_db(app: &tauri::AppHandle) -> Result<Connection, String> {
    Connection::open(db_path(app))
        .map_err(|e| e.to_string())
        .and_then(|conn| {
            init_schema(&conn).map_err(|e| e.to_string())?;
            Ok(conn)
        })
}

fn init_schema(conn: &Connection) -> SqlResult<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS files (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            path         TEXT NOT NULL UNIQUE,
            name         TEXT NOT NULL,
            imported_at  DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS chunks (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            file_id      INTEGER NOT NULL REFERENCES files(id),
            content      TEXT NOT NULL,
            chunk_index  INTEGER NOT NULL
        );
        -- 向量存储：BLOB = 384 × f32 little-endian
        CREATE TABLE IF NOT EXISTS chunk_embeddings (
            chunk_id  INTEGER PRIMARY KEY REFERENCES chunks(id),
            embedding BLOB NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_chunks_file    ON chunks(file_id);
        CREATE INDEX IF NOT EXISTS idx_chunks_content ON chunks(content);
        ",
    )
}

// ── 资源路径解析 ──────────────────────────────────────────────────────────────

/// 开发模式用编译期 CARGO_MANIFEST_DIR，生产模式用 resource_dir()
fn resource_dir(app: &tauri::AppHandle) -> PathBuf {
    #[cfg(debug_assertions)]
    {
        let _ = app;
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/resources"))
    }
    #[cfg(not(debug_assertions))]
    {
        app.path()
            .resource_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
    }
}

// ── 文本分段 ──────────────────────────────────────────────────────────────────

fn segment_text(text: &str) -> Vec<String> {
    const MAX: usize = 500;
    const MIN: usize = 30;
    let mut chunks = Vec::new();
    for para in text.split("\n\n") {
        let para = para.trim();
        if para.len() < MIN {
            continue;
        }
        if para.len() <= MAX {
            chunks.push(para.to_string());
        } else {
            let mut buf = String::new();
            for sent in para.split(". ") {
                let sent = sent.trim();
                if sent.is_empty() {
                    continue;
                }
                if !buf.is_empty() && buf.len() + sent.len() + 2 > MAX {
                    if buf.len() >= MIN {
                        chunks.push(buf.trim().to_string());
                    }
                    buf.clear();
                }
                if !buf.is_empty() {
                    buf.push_str(". ");
                }
                buf.push_str(sent);
            }
            if buf.len() >= MIN {
                chunks.push(buf.trim().to_string());
            }
        }
    }
    chunks
}

// ── Tauri 命令 ────────────────────────────────────────────────────────────────

/// 返回当前模型加载状态字符串
#[tauri::command]
async fn get_model_status(state: tauri::State<'_, ModelStatusState>) -> Result<String, String> {
    Ok(state.0.lock().unwrap().as_str())
}

/// 选择文件夹、导入 TXT、生成 embedding，实时发送进度事件
#[tauri::command]
async fn select_and_import_folder(
    app: tauri::AppHandle,
    model_st: tauri::State<'_, ModelStatusState>,
    cache_st: tauri::State<'_, CacheState>,
) -> Result<ImportResult, String> {
    let selected = app.dialog().file().blocking_pick_folder();
    let folder_path = match selected {
        Some(tauri_plugin_dialog::FilePath::Path(p)) => p,
        None => return Err("cancelled".to_string()),
        #[allow(unreachable_patterns)]
        _ => return Err("Unsupported path type".to_string()),
    };

    let model_ready = *model_st.0.lock().unwrap() == ModelStatus::Ready;
    let conn = open_db(&app)?;

    // 先收集所有 TXT 文件，得到总数用于进度
    let txt_files: Vec<_> = WalkDir::new(&folder_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().is_file()
                && e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .unwrap_or("")
                    .to_lowercase()
                    == "txt"
        })
        .collect();

    let total = txt_files.len();
    let mut files_imported = 0usize;
    let mut chunks_created = 0usize;
    let mut skipped = 0usize;
    let mut embeddings_generated = 0usize;

    for (idx, entry) in txt_files.iter().enumerate() {
        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // 进度事件
        app.emit(
            "import-progress",
            serde_json::json!({
                "current": idx + 1,
                "total":   total,
                "file":    &file_name,
                "phase":   "reading",
            }),
        )
        .ok();

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        // 查找或插入文件记录
        let file_id: i64 = {
            let existing: Option<i64> = conn
                .query_row(
                    "SELECT id FROM files WHERE path = ?1",
                    rusqlite::params![path_str],
                    |r| r.get(0),
                )
                .ok();
            if let Some(id) = existing {
                conn.execute(
                    "UPDATE files SET imported_at = CURRENT_TIMESTAMP WHERE id = ?1",
                    rusqlite::params![id],
                )
                .map_err(|e| e.to_string())?;
                id
            } else {
                conn.execute(
                    "INSERT INTO files (path, name) VALUES (?1, ?2)",
                    rusqlite::params![path_str, file_name],
                )
                .map_err(|e| e.to_string())?;
                conn.last_insert_rowid()
            }
        };

        // 删旧数据（支持重新导入）
        conn.execute(
            "DELETE FROM chunk_embeddings WHERE chunk_id IN (SELECT id FROM chunks WHERE file_id=?1)",
            rusqlite::params![file_id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM chunks WHERE file_id = ?1",
            rusqlite::params![file_id],
        )
        .map_err(|e| e.to_string())?;

        let chunks = segment_text(&content);
        let chunk_count = chunks.len();

        for (ci, chunk_text) in chunks.into_iter().enumerate() {
            conn.execute(
                "INSERT INTO chunks (file_id, content, chunk_index) VALUES (?1, ?2, ?3)",
                rusqlite::params![file_id, &chunk_text, ci as i64],
            )
            .map_err(|e| e.to_string())?;
            let chunk_id = conn.last_insert_rowid();
            chunks_created += 1;

            // 生成 embedding
            if model_ready {
                let emb_opt: Option<Vec<f32>> = {
                    let mut guard = model_lock().lock().unwrap();
                    guard.as_mut().and_then(|m| m.encode(&chunk_text).ok())
                };

                if let Some(emb) = emb_opt {
                    conn.execute(
                        "INSERT INTO chunk_embeddings (chunk_id, embedding) VALUES (?1, ?2)",
                        rusqlite::params![chunk_id, vec_to_bytes(&emb)],
                    )
                    .map_err(|e| e.to_string())?;
                    embeddings_generated += 1;
                }
            }

            // 每处理 5 个 chunk 发一次进度（减少事件量）
            if ci % 5 == 0 || ci == chunk_count - 1 {
                app.emit(
                    "import-progress",
                    serde_json::json!({
                        "current": idx + 1,
                        "total":   total,
                        "file":    &file_name,
                        "phase":   "embedding",
                        "chunk":   ci + 1,
                        "chunks":  chunk_count,
                    }),
                )
                .ok();
            }
        }

        files_imported += 1;
    }

    // 导入完成，使向量缓存失效
    cache_st.0.write().unwrap().invalidate();

    app.emit(
        "import-progress",
        serde_json::json!({
            "current": total,
            "total":   total,
            "file":    "",
            "phase":   "done",
        }),
    )
    .ok();

    Ok(ImportResult {
        files_imported,
        chunks_created,
        skipped,
        embeddings_generated,
    })
}

/// 语义搜索（模型可用时）或关键词搜索（模型不可用时回退）
#[tauri::command]
async fn search_text(
    app: tauri::AppHandle,
    model_st: tauri::State<'_, ModelStatusState>,
    cache_st: tauri::State<'_, CacheState>,
    query: String,
) -> Result<Vec<SearchResult>, String> {
    let q = query.trim().to_string();
    if q.is_empty() {
        return Ok(vec![]);
    }

    if *model_st.0.lock().unwrap() == ModelStatus::Ready {
        match semantic_search(&app, &cache_st, &q) {
            Ok(results) if !results.is_empty() => return Ok(results),
            Ok(_) => {} // 语义无结果，fall through 到关键词
            Err(e) => eprintln!("[LocalLens] 语义搜索失败，回退关键词: {e}"),
        }
    }

    keyword_search(&app, &q)
}

fn semantic_search(
    app: &tauri::AppHandle,
    cache_st: &CacheState,
    query: &str,
) -> Result<Vec<SearchResult>, String> {
    // 1. 生成查询向量
    let query_emb: Vec<f32> = {
        let mut guard = model_lock().lock().unwrap();
        guard.as_mut().and_then(|m| m.encode(query).ok())
    }
    .ok_or("查询向量生成失败")?;

    // 2. 确保缓存有效
    ensure_cache_valid(app, cache_st)?;

    // 3. 余弦相似度排序，取 Top 20
    let top_ids: Vec<(i64, f32)> = {
        let cache = cache_st.0.read().unwrap();
        let mut scored: Vec<(i64, f32)> = cache
            .entries
            .iter()
            .map(|(id, emb)| (*id, cosine_sim(&query_emb, emb)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(20);
        scored
    };

    if top_ids.is_empty() {
        return Ok(vec![]);
    }

    // 4. 批量查询 chunk 内容
    let conn = open_db(app)?;
    let mut results = Vec::with_capacity(top_ids.len());
    for (chunk_id, score) in &top_ids {
        if let Ok((content, file_name, file_path, chunk_index)) = conn.query_row(
            "SELECT c.content, f.name, f.path, c.chunk_index
             FROM chunks c JOIN files f ON c.file_id = f.id
             WHERE c.id = ?1",
            rusqlite::params![chunk_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        ) {
            results.push(SearchResult {
                content,
                file_name,
                file_path,
                chunk_index,
                score: *score,
                is_semantic: true,
            });
        }
    }

    Ok(results)
}

/// 确保内存向量缓存与数据库同步
fn ensure_cache_valid(app: &tauri::AppHandle, cache_st: &CacheState) -> Result<(), String> {
    // fast path：读锁检查
    {
        let cache = cache_st.0.read().unwrap();
        if cache.valid {
            return Ok(());
        }
    }
    // slow path：写锁重建
    let conn = open_db(app)?;
    let mut stmt = conn
        .prepare("SELECT chunk_id, embedding FROM chunk_embeddings")
        .map_err(|e| e.to_string())?;

    let entries: Vec<(i64, Vec<f32>)> = stmt
        .query_map([], |row| {
            let id: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            Ok((id, blob))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .map(|(id, blob)| (id, bytes_to_vec(&blob)))
        .collect();

    let mut cache = cache_st.0.write().unwrap();
    cache.entries = entries;
    cache.valid = true;
    Ok(())
}

/// 关键词回退搜索（LIKE 查询）
fn keyword_search(app: &tauri::AppHandle, query: &str) -> Result<Vec<SearchResult>, String> {
    let conn = open_db(app)?;
    let like = format!("%{}%", query.trim());
    let mut stmt = conn
        .prepare(
            "SELECT c.content, f.name, f.path, c.chunk_index
             FROM chunks c JOIN files f ON c.file_id = f.id
             WHERE c.content LIKE ?1
             ORDER BY length(c.content) ASC
             LIMIT 30",
        )
        .map_err(|e| e.to_string())?;

    let results: Vec<SearchResult> = stmt
        .query_map(rusqlite::params![like], |row| {
            Ok(SearchResult {
                content: row.get(0)?,
                file_name: row.get(1)?,
                file_path: row.get(2)?,
                chunk_index: row.get(3)?,
                score: 0.0,
                is_semantic: false,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(results)
}

/// 统计信息
#[tauri::command]
async fn get_stats(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    if !db_path(&app).exists() {
        return Ok(serde_json::json!({ "files": 0, "chunks": 0, "embeddings": 0 }));
    }
    let conn = open_db(&app)?;
    let files: i64 = conn
        .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
        .unwrap_or(0);
    let chunks: i64 = conn
        .query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))
        .unwrap_or(0);
    let embeddings: i64 = conn
        .query_row("SELECT COUNT(*) FROM chunk_embeddings", [], |r| r.get(0))
        .unwrap_or(0);
    Ok(serde_json::json!({ "files": files, "chunks": chunks, "embeddings": embeddings }))
}

// ── 应用入口 ──────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let model_status = ModelStatusState(Arc::new(Mutex::new(ModelStatus::Loading)));
    let cache = CacheState(Arc::new(RwLock::new(VectorCache::new())));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(model_status)
        .manage(cache)
        .setup(|app| {
            // 后台线程加载模型，不阻塞 UI
            let handle = app.handle().clone();
            let status_arc = app.state::<ModelStatusState>().0.clone();

            std::thread::spawn(move || {
                let res = resource_dir(&handle);
                let model_path = res.join("model.onnx");
                let tok_path = res.join("tokenizer.json");

                if !model_path.exists() || !tok_path.exists() {
                    *status_arc.lock().unwrap() = ModelStatus::Unavailable;
                    handle.emit("model-status", "unavailable").ok();
                    eprintln!(
                        "[LocalLens] 模型文件未找到，请将 model.onnx 和 tokenizer.json 放入 {}",
                        res.display()
                    );
                    return;
                }

                match EmbeddingModel::load(&model_path, &tok_path) {
                    Ok(model) => {
                        *model_lock().lock().unwrap() = Some(model);
                        *status_arc.lock().unwrap() = ModelStatus::Ready;
                        handle.emit("model-status", "ready").ok();
                        eprintln!("[LocalLens] 语义搜索模型加载成功");
                    }
                    Err(e) => {
                        eprintln!("[LocalLens] 模型加载失败: {e}");
                        *status_arc.lock().unwrap() = ModelStatus::Failed(e.clone());
                        handle.emit("model-status", format!("failed:{e}")).ok();
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_model_status,
            select_and_import_folder,
            search_text,
            get_stats,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
