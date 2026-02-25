//! 文本 embedding 模块（ort 2.0-rc.11 API）

use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;
use tokenizers::Tokenizer;

const MAX_SEQ: usize = 128;

// ── EmbeddingModel ────────────────────────────────────────────────────────────

pub struct EmbeddingModel {
    session: Session,
    tokenizer: Tokenizer,
    /// 部分 ONNX 导出不含 token_type_ids 输入，加载时自动检测
    has_type_ids: bool,
}

unsafe impl Send for EmbeddingModel {}
unsafe impl Sync for EmbeddingModel {}

impl EmbeddingModel {
    pub fn load(model_path: &Path, tokenizer_path: &Path) -> Result<Self, String> {
        if !model_path.exists() {
            return Err(format!("模型文件未找到: {}", model_path.display()));
        }
        if !tokenizer_path.exists() {
            return Err(format!("Tokenizer 未找到: {}", tokenizer_path.display()));
        }

        // 全局 ORT 初始化（幂等）
        ort::init().with_name("LocalLens").commit();

        let session = Session::builder()
            .map_err(|e| format!("SessionBuilder 失败: {e}"))?
            .commit_from_file(model_path)
            .map_err(|e| format!("模型加载失败: {e}"))?;

        // 检测模型是否需要 token_type_ids 输入
        let has_type_ids = session
            .inputs()
            .iter()
            .any(|i| i.name() == "token_type_ids");
        eprintln!(
            "[LocalLens] 模型输入检测: token_type_ids={}",
            has_type_ids
        );

        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| format!("Tokenizer 加载失败: {e}"))?;

        Ok(Self {
            session,
            tokenizer,
            has_type_ids,
        })
    }

    /// 将文本编码为 L2-normalized 向量
    pub fn encode(&mut self, text: &str) -> Result<Vec<f32>, String> {
        let enc = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| e.to_string())?;

        let seq_len = enc.get_ids().len().min(MAX_SEQ);

        let input_ids: Vec<i64> = enc.get_ids()[..seq_len].iter().map(|&x| x as i64).collect();
        let attn_mask: Vec<i64> = enc.get_attention_mask()[..seq_len]
            .iter()
            .map(|&x| x as i64)
            .collect();
        let mask_f32: Vec<f32> = enc.get_attention_mask()[..seq_len]
            .iter()
            .map(|&x| x as f32)
            .collect();

        let ids_ort = Tensor::<i64>::from_array(([1_usize, seq_len], input_ids))
            .map_err(|e| e.to_string())?;
        let mask_ort = Tensor::<i64>::from_array(([1_usize, seq_len], attn_mask))
            .map_err(|e| e.to_string())?;

        let outputs = if self.has_type_ids {
            let type_ids: Vec<i64> = enc.get_type_ids()[..seq_len]
                .iter()
                .map(|&x| x as i64)
                .collect();
            let types_ort = Tensor::<i64>::from_array(([1_usize, seq_len], type_ids))
                .map_err(|e| e.to_string())?;
            self.session
                .run(ort::inputs![
                    "input_ids"      => ids_ort,
                    "attention_mask" => mask_ort,
                    "token_type_ids" => types_ort,
                ])
                .map_err(|e| format!("推理失败: {e}"))?
        } else {
            self.session
                .run(ort::inputs![
                    "input_ids"      => ids_ort,
                    "attention_mask" => mask_ort,
                ])
                .map_err(|e| format!("推理失败: {e}"))?
        };

        // 提取 last_hidden_state: [1, seq_len, hidden_dim]
        let (_, flat) = outputs["last_hidden_state"]
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;

        let hidden_dim = flat.len() / seq_len;

        // Mean pooling（attention mask 加权）
        let mask_sum: f32 = mask_f32.iter().sum::<f32>().max(1e-9);
        let mut pooled = vec![0.0f32; hidden_dim];
        for (t, &m) in mask_f32.iter().enumerate() {
            if m == 0.0 {
                continue;
            }
            let off = t * hidden_dim;
            for d in 0..hidden_dim {
                pooled[d] += flat[off + d] * m;
            }
        }
        for v in &mut pooled {
            *v /= mask_sum;
        }

        Ok(l2_normalize(pooled))
    }
}

// ── 工具函数 ──────────────────────────────────────────────────────────────────

fn l2_normalize(mut v: Vec<f32>) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
    for x in &mut v {
        *x /= norm;
    }
    v
}

/// 余弦相似度（两个向量均已 L2 归一化，直接点积）
pub fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Vec<f32> → little-endian 字节（SQLite BLOB 存储）
pub fn vec_to_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// SQLite BLOB 字节 → Vec<f32>
pub fn bytes_to_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}
