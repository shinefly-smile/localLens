use rusqlite::{Connection, Result as SqlResult};
use std::path::PathBuf;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;
use walkdir::WalkDir;

// ── 数据结构 ──────────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct ImportResult {
    pub files_imported: usize,
    pub chunks_created: usize,
    pub skipped: usize,
}

#[derive(serde::Serialize)]
pub struct SearchResult {
    pub content: String,
    pub file_name: String,
    pub file_path: String,
    pub chunk_index: i64,
}

// ── 数据库初始化 ──────────────────────────────────────────────────────────────

fn db_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().expect("Failed to get app data dir");
    std::fs::create_dir_all(&dir).expect("Failed to create app data dir");
    dir.join("locallens.db")
}

fn open_db(app: &tauri::AppHandle) -> Result<Connection, String> {
    let path = db_path(app);
    let conn = Connection::open(&path).map_err(|e| e.to_string())?;
    init_schema(&conn).map_err(|e| e.to_string())?;
    Ok(conn)
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

        CREATE INDEX IF NOT EXISTS idx_chunks_file    ON chunks(file_id);
        CREATE INDEX IF NOT EXISTS idx_chunks_content ON chunks(content);
        ",
    )
}

// ── 文本分段 ──────────────────────────────────────────────────────────────────

/// 将文本按段落分段，超长段落再按句子切分
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
            // 长段落按句子合并
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

/// 弹出文件夹选择框，导入所有 .txt 文件
#[tauri::command]
async fn select_and_import_folder(app: tauri::AppHandle) -> Result<ImportResult, String> {
    let selected = app.dialog().file().blocking_pick_folder();

    let folder_path = match selected {
        Some(tauri_plugin_dialog::FilePath::Path(p)) => p,
        None => return Err("cancelled".to_string()),
        #[allow(unreachable_patterns)]
        _ => return Err("Unsupported path type".to_string()),
    };

    let conn = open_db(&app)?;

    let mut files_imported = 0usize;
    let mut chunks_created = 0usize;
    let mut skipped = 0usize;

    for entry in WalkDir::new(&folder_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext != "txt" {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // 读取文件内容（跳过无法 UTF-8 解码的文件）
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

        // 删除旧 chunks（重新导入场景）
        conn.execute(
            "DELETE FROM chunks WHERE file_id = ?1",
            rusqlite::params![file_id],
        )
        .map_err(|e| e.to_string())?;

        // 分段并写入
        let chunks = segment_text(&content);
        for (i, chunk) in chunks.iter().enumerate() {
            conn.execute(
                "INSERT INTO chunks (file_id, content, chunk_index) VALUES (?1, ?2, ?3)",
                rusqlite::params![file_id, chunk, i as i64],
            )
            .map_err(|e| e.to_string())?;
            chunks_created += 1;
        }

        files_imported += 1;
    }

    Ok(ImportResult {
        files_imported,
        chunks_created,
        skipped,
    })
}

/// 关键词搜索，返回匹配的文本块
#[tauri::command]
async fn search_text(app: tauri::AppHandle, query: String) -> Result<Vec<SearchResult>, String> {
    let q = query.trim().to_string();
    if q.is_empty() {
        return Ok(vec![]);
    }

    let conn = open_db(&app)?;
    let like = format!("%{}%", q);

    let mut stmt = conn
        .prepare(
            "SELECT c.content, f.name, f.path, c.chunk_index
             FROM chunks c
             JOIN files f ON c.file_id = f.id
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
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(results)
}

/// 获取当前数据库统计信息
#[tauri::command]
async fn get_stats(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let path = db_path(&app);
    if !path.exists() {
        return Ok(serde_json::json!({ "files": 0, "chunks": 0 }));
    }
    let conn = open_db(&app)?;
    let files: i64 = conn
        .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
        .unwrap_or(0);
    let chunks: i64 = conn
        .query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))
        .unwrap_or(0);
    Ok(serde_json::json!({ "files": files, "chunks": chunks }))
}

// ── 应用入口 ──────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            select_and_import_folder,
            search_text,
            get_stats
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
