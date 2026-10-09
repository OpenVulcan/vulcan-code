//! ONNX Runtime session that scores encoded calls in bounded batches.
//! 以有界批次为已编码调用评分的 ONNX Runtime 会话。

use std::path::Path;

use ort::ep::{ArenaExtendStrategy, CUDA};
use ort::session::builder::{GraphOptimizationLevel, SessionBuilder};
use ort::session::{RunOptions, Session};
use ort::value::Tensor;
use vulcan_permission_judge::protocol::{JudgeDevice, JudgeOutcome};
use vulcan_permission_judge::{JudgePackage, JudgeSettings, RuntimeConfig, decide};

use crate::sequence::{EncodeOutcome, Encoded, Encoder};

/// Intra-op threads of the session.
/// 会话的算子内线程数。
const INTRA_OP_THREADS: usize = 4;

/// One loaded model session.
/// 一个已加载的模型会话。
pub struct Engine {
    session: Session,
    config: RuntimeConfig,
    /// Device the session runs on.
    /// 会话运行的设备。
    pub device: JudgeDevice,
    /// Releases temporary GPU arenas after long sequences.
    /// 在长序列之后释放临时显存池。
    trim: Option<RunOptions>,
}

impl Engine {
    /// Creates the session on exactly the requested device; an unavailable device is an error.
    /// 在请求的设备上创建会话；设备不可用即报错。
    /// The ONNX Runtime library must already be initialized for this process.
    /// 本进程必须已经初始化 ONNX Runtime 库。
    pub fn load(package: &JudgePackage, device: JudgeDevice) -> Result<Self, String> {
        let fail =
            |error: ort::Error<SessionBuilder>| format!("Failed to build the session: {error}");
        let mut builder = Session::builder()
            .map_err(|error| format!("Failed to create a session builder: {error}"))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(fail)?
            .with_intra_threads(INTRA_OP_THREADS)
            .map_err(fail)?
            .with_config_entry("session.use_device_allocator_for_initializers", "1")
            .map_err(fail)?;
        let mut trim = None;
        if device == JudgeDevice::Cuda {
            let limit = usize::try_from(package.config.gpu_memory_limit_mib)
                .ok()
                .and_then(|mib| mib.checked_mul(1 << 20))
                .ok_or("The GPU memory limit does not fit this platform.")?;
            builder = builder
                .with_execution_providers([CUDA::default()
                    .with_tf32(false)
                    .with_arena_extend_strategy(ArenaExtendStrategy::NextPowerOfTwo)
                    .with_memory_limit(limit)
                    .build()
                    .error_on_failure()])
                .map_err(fail)?;
            let mut options = RunOptions::new()
                .map_err(|error| format!("Failed to create run options: {error}"))?;
            options
                .set("memory.enable_memory_arena_shrinkage", "gpu:0")
                .map_err(|error| format!("Failed to configure arena shrinkage: {error}"))?;
            trim = Some(options);
        }
        let session = builder
            .commit_from_file(package.file(&package.config.model_file))
            .map_err(|error| format!("Failed to load the model graph: {error}"))?;
        Ok(Self {
            session,
            config: package.config.clone(),
            device,
            trim,
        })
    }

    /// Encodes and scores calls, preserving caller order; calls over the token limit are not run.
    /// 编码并评分调用，保持调用方顺序；超过词元上限的调用不运行。
    pub fn predict(
        &mut self,
        encoder: &Encoder,
        items: &[(JudgeSettings, &str)],
    ) -> Result<Vec<JudgeOutcome>, String> {
        let mut outcomes: Vec<Option<JudgeOutcome>> = vec![None; items.len()];
        let mut encoded = Vec::new();
        for (index, (settings, rendered)) in items.iter().enumerate() {
            match encoder.encode(settings, rendered)? {
                EncodeOutcome::Encoded(row) => encoded.push((index, row)),
                EncodeOutcome::TooLong(length) => {
                    outcomes[index] = Some(JudgeOutcome::InputTokenLimit {
                        input_tokens: length as u32,
                    });
                }
            }
        }
        for batch in batches(&encoded, &self.config) {
            let rows: Vec<&Encoded> = batch.iter().map(|position| &encoded[*position].1).collect();
            let scores = self.run(&rows, encoder.pad)?;
            for (position, logits) in batch.into_iter().zip(scores) {
                let (index, row) = &encoded[position];
                outcomes[*index] = Some(JudgeOutcome::Evaluated {
                    label: decide(logits, self.config.allow_margin)
                        .map_err(|error| error.to_string())?,
                    logits,
                    input_tokens: row.ids.len() as u32,
                });
            }
        }
        outcomes
            .into_iter()
            .map(|outcome| {
                outcome.ok_or_else(|| String::from("A call was left without an outcome."))
            })
            .collect()
    }

    /// Runs one padded batch and returns three logits per row.
    /// 运行一个已填充批次，并返回每行三个分数。
    fn run(&mut self, rows: &[&Encoded], pad: i64) -> Result<Vec<[f32; 3]>, String> {
        let count = rows.len();
        let length = rows.iter().map(|row| row.ids.len()).max().unwrap_or(0);
        if count == 0 || length == 0 {
            return Ok(Vec::new());
        }
        let mut ids = vec![pad; count * length];
        let mut attention = vec![0_i64; count * length];
        let mut markers = Vec::with_capacity(count * 3);
        for (index, row) in rows.iter().enumerate() {
            let start = index * length;
            ids[start..start + row.ids.len()].copy_from_slice(&row.ids);
            attention[start..start + row.ids.len()].fill(1);
            markers.extend(row.markers);
        }
        let tensor = |error: ort::Error| format!("Failed to build an input tensor: {error}");
        let inputs = ort::inputs! {
            "input_ids" => Tensor::from_array(([count, length], ids)).map_err(tensor)?,
            "attention_mask" => Tensor::from_array(([count, length], attention)).map_err(tensor)?,
            "marker_pos" => Tensor::from_array(([count, 3], markers)).map_err(tensor)?,
            "marker_mask" => Tensor::from_array(([count, 3], vec![true; count * 3])).map_err(tensor)?,
            "qtype" => Tensor::from_array(([count], vec![0_i64; count])).map_err(tensor)?,
        };
        let trim = self
            .trim
            .as_ref()
            .filter(|_| length > self.config.arena_trim_above_tokens as usize);
        let outputs = match trim {
            Some(options) => self.session.run_with_options(inputs, options),
            None => self.session.run(inputs),
        }
        .map_err(|error| format!("Model inference failed: {error}"))?;
        let (_, scores) = outputs["logits"]
            .try_extract_tensor::<f32>()
            .map_err(|error| format!("The model returned no logits: {error}"))?;
        if scores.len() != count * 3 {
            return Err(String::from(
                "The model returned an unexpected logit shape.",
            ));
        }
        Ok(scores
            .chunks_exact(3)
            .map(|row| [row[0], row[1], row[2]])
            .collect())
    }
}

/// Initializes ONNX Runtime from a shared library; a process can do this only once.
/// 从共享库初始化 ONNX Runtime；每个进程只能执行一次。
pub fn initialize_runtime(library: &Path) -> Result<(), String> {
    if !library.is_absolute() || !library.is_file() {
        return Err(format!(
            "ONNX Runtime library `{}` must be an existing absolute file path.",
            library.display()
        ));
    }
    ort::init_from(library)
        .map_err(|error| {
            format!(
                "Failed to load ONNX Runtime from `{}`: {error}",
                library.display()
            )
        })?
        .commit();
    Ok(())
}

/// Groups encoded rows by length into batches bounded by row count and padded token budget.
/// 按长度将已编码行分组为同时受行数与填充词元预算约束的批次。
/// Returns positions into `encoded`.
/// 返回 `encoded` 中的位置。
fn batches(encoded: &[(usize, Encoded)], config: &RuntimeConfig) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..encoded.len()).collect();
    order.sort_by_key(|position| encoded[*position].1.ids.len());
    let rows = config.micro_batch.min(config.max_request_items) as usize;
    let budget = config.max_tokens as usize;
    let mut batches = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for position in order {
        let length = encoded[position].1.ids.len();
        if !current.is_empty() && (current.len() >= rows || (current.len() + 1) * length > budget) {
            batches.push(std::mem::take(&mut current));
        }
        current.push(position);
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequence::tests::config;

    fn row(length: usize) -> Encoded {
        Encoded {
            ids: vec![1; length],
            markers: [0, 1, 2],
        }
    }

    #[test]
    fn batches_respect_row_and_token_budgets() {
        let config = config(100);
        let encoded: Vec<(usize, Encoded)> = [30, 10, 60, 20, 10]
            .into_iter()
            .enumerate()
            .map(|(index, length)| (index, row(length)))
            .collect();
        let batches = batches(&encoded, &config);
        for batch in &batches {
            assert!(batch.len() <= config.micro_batch as usize);
            let longest = batch.iter().map(|p| encoded[*p].1.ids.len()).max().unwrap();
            assert!(batch.len() * longest <= config.max_tokens as usize);
        }
        let mut seen: Vec<usize> = batches.concat();
        seen.sort_unstable();
        assert_eq!(seen, [0, 1, 2, 3, 4], "every row is scheduled exactly once");
    }
}
