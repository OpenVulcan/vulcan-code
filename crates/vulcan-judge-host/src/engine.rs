//! ONNX Runtime session that scores encoded calls in bounded batches.
//! 以有界批次为已编码调用评分的 ONNX Runtime 会话。

use std::path::Path;

use ort::environment::Environment;
use ort::ep::{ArenaExtendStrategy, CUDA, ExecutionProviderLibrary};
use ort::session::builder::{GraphOptimizationLevel, SessionBuilder};
use ort::session::{RunOptions, Session};
use ort::value::Tensor;
use vulcan_permission_judge::package::OUTPUT_FIELD;
use vulcan_permission_judge::protocol::{JudgeDevice, JudgeOutcome};
use vulcan_permission_judge::{JudgePackage, JudgeSettings, RuntimeConfig, decide};

use crate::sequence::{EncodeOutcome, Encoder};

/// Intra-op threads of the session.
/// 会话的算子内线程数。
const INTRA_OP_THREADS: usize = 4;

/// Execution-provider name the WebGPU plugin library registers.
/// WebGPU 插件库注册的执行提供程序名称。
const WEBGPU_EP_NAME: &str = "WebGpuExecutionProvider";

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
    /// Declared after `session` so the plugin is unregistered only after the session is released.
    /// 声明在 `session` 之后，使插件只在会话释放后才注销。
    _plugin: PluginRegistration,
}

/// Unregisters a plugin execution-provider library when dropped.
/// 析构时注销插件执行提供程序库。
struct PluginRegistration(Option<ExecutionProviderLibrary>);

impl Drop for PluginRegistration {
    fn drop(&mut self) {
        if let Some(library) = self.0.take() {
            // The process is releasing the engine; a failed unregistration has no caller to inform.
            // 进程正在释放引擎；注销失败时没有可通知的调用方。
            let _ = library.unregister();
        }
    }
}

impl Engine {
    /// Creates the session on exactly the requested device; an unavailable device is an error.
    /// 在请求的设备上创建会话；设备不可用即报错。
    /// The ONNX Runtime library must already be initialized for this process; a plugin device
    /// takes its execution-provider library, which no other device accepts.
    /// 本进程必须已经初始化 ONNX Runtime 库；插件设备需要其执行提供程序库，其他设备不接受该库。
    pub fn load(
        package: &JudgePackage,
        device: JudgeDevice,
        execution_provider_library: Option<&Path>,
    ) -> Result<Self, String> {
        if !package.config.devices.contains(&device) {
            return Err(format!(
                "This package was validated on {:?} only; it does not run on {device:?}.",
                package.config.devices
            ));
        }
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
        let mut plugin = PluginRegistration(None);
        match (device, execution_provider_library) {
            (JudgeDevice::Cpu, None) => {}
            (JudgeDevice::Cuda, None) => {
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
            (JudgeDevice::WebGpu, Some(library)) => {
                let environment = Environment::current().map_err(|error| {
                    format!("Failed to access the runtime environment: {error}")
                })?;
                plugin.0 = Some(
                    environment
                        .register_ep_library("webgpu", existing_file(library)?)
                        .map_err(|error| {
                            format!("Failed to register the WebGPU library: {error}")
                        })?,
                );
                let gpu = environment
                    .devices()
                    .find(|candidate| candidate.ep().is_ok_and(|name| name == WEBGPU_EP_NAME))
                    .ok_or("The WebGPU library found no usable GPU.")?;
                builder = builder.with_devices([gpu], None).map_err(fail)?;
            }
            (JudgeDevice::WebGpu, None) => {
                return Err(String::from(
                    "The WebGPU device requires its execution-provider library.",
                ));
            }
            (JudgeDevice::Cpu | JudgeDevice::Cuda, Some(_)) => {
                return Err(String::from(
                    "Only a plugin device accepts an execution-provider library.",
                ));
            }
        }
        let session = builder
            .commit_from_file(package.file(&package.config.model_file))
            .map_err(|error| format!("Failed to load the model graph: {error}"))?;
        Ok(Self {
            session,
            config: package.config.clone(),
            device,
            trim,
            _plugin: plugin,
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
            let rows: Vec<&[i64]> = batch
                .iter()
                .map(|position| encoded[*position].1.as_slice())
                .collect();
            let scores = self.run(&rows, encoder.pad)?;
            for (position, logits) in batch.into_iter().zip(scores) {
                let (index, row) = &encoded[position];
                outcomes[*index] = Some(JudgeOutcome::Evaluated {
                    label: decide(logits, self.config.allow_margin)
                        .map_err(|error| error.to_string())?,
                    logits,
                    input_tokens: row.len() as u32,
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
    fn run(&mut self, rows: &[&[i64]], pad: i64) -> Result<Vec<[f32; 3]>, String> {
        let count = rows.len();
        let length = rows.iter().map(|row| row.len()).max().unwrap_or(0);
        if count == 0 || length == 0 {
            return Ok(Vec::new());
        }
        let mut ids = vec![pad; count * length];
        let mut attention = vec![0_i64; count * length];
        for (index, row) in rows.iter().enumerate() {
            let start = index * length;
            ids[start..start + row.len()].copy_from_slice(row);
            attention[start..start + row.len()].fill(1);
        }
        let tensor = |error: ort::Error| format!("Failed to build an input tensor: {error}");
        let inputs = ort::inputs! {
            "input_ids" => Tensor::from_array(([count, length], ids)).map_err(tensor)?,
            "attention_mask" => Tensor::from_array(([count, length], attention)).map_err(tensor)?,
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
        let (_, scores) = outputs[OUTPUT_FIELD]
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
    ort::init_from(existing_file(library)?)
        .map_err(|error| {
            format!(
                "Failed to load ONNX Runtime from `{}`: {error}",
                library.display()
            )
        })?
        .commit();
    Ok(())
}

/// Accepts only an existing file given by an absolute path.
/// 只接受以绝对路径给出的已存在文件。
fn existing_file(path: &Path) -> Result<&Path, String> {
    if path.is_absolute() && path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "Runtime library `{}` must be an existing absolute file path.",
            path.display()
        ))
    }
}

/// Groups encoded rows by length into batches bounded by row count and padded token budget.
/// 按长度将已编码行分组为同时受行数与填充词元预算约束的批次。
/// Returns positions into `encoded`.
/// 返回 `encoded` 中的位置。
/// A row longer than the padded budget runs alone.
/// 超过填充预算的单行单独运行。
fn batches(encoded: &[(usize, Vec<i64>)], config: &RuntimeConfig) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..encoded.len()).collect();
    order.sort_by_key(|position| encoded[*position].1.len());
    let rows = config.micro_batch.min(config.max_request_items) as usize;
    let budget = config.padded_batch_tokens as usize;
    let mut batches = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for position in order {
        let length = encoded[position].1.len();
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

    #[test]
    fn batches_respect_row_and_padded_token_budgets() {
        // A 300-token limit with a 100-token padded budget: the 150-token row must run alone.
        // 上限 300 词元、填充预算 100 词元：150 词元的行必须单独运行。
        let config = config(300);
        let encoded: Vec<(usize, Vec<i64>)> = [30, 10, 60, 20, 10, 150]
            .into_iter()
            .enumerate()
            .map(|(index, length)| (index, vec![1; length]))
            .collect();
        let batches = batches(&encoded, &config);
        for batch in &batches {
            assert!(batch.len() <= config.micro_batch as usize);
            let longest = batch.iter().map(|p| encoded[*p].1.len()).max().unwrap();
            assert!(
                batch.len() == 1 || batch.len() * longest <= config.padded_batch_tokens as usize
            );
        }
        assert!(batches.contains(&vec![5]), "an over-budget row runs alone");
        let mut seen: Vec<usize> = batches.concat();
        seen.sort_unstable();
        assert_eq!(
            seen,
            [0, 1, 2, 3, 4, 5],
            "every row is scheduled exactly once"
        );
    }
}
