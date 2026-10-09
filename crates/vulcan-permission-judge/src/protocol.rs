//! Line protocol between the service and one `vulcan-judge-host` worker process.
//! 服务与单个 `vulcan-judge-host` worker 进程之间的行协议。
//!
//! Each message is one JSON object on its own line, prefixed with [`JUDGE_HOST_LINE_PREFIX`] so
//! stray output from native libraries can never be decoded as protocol data. Requests carry an
//! `id` that the matching response echoes. The worker sends [`JudgeHostResponse::Ready`] once at
//! start-up and answers requests strictly in order.
//! 每条消息是独占一行的 JSON 对象，带 [`JUDGE_HOST_LINE_PREFIX`] 前缀，使原生库的意外输出永远不会
//! 被解码为协议数据。请求携带 `id`，对应响应原样返回。worker 启动时发送一次
//! [`JudgeHostResponse::Ready`]，并严格按顺序应答请求。

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::decision::JudgeLabel;

/// The only judge-host protocol version accepted by this build.
/// 本构建唯一接受的判定宿主协议版本。
pub const JUDGE_HOST_CONTRACT: &str = "judge-host-v1";

/// Prefix of every protocol line.
/// 每条协议行的前缀。
pub const JUDGE_HOST_LINE_PREFIX: &str = "VJH1 ";

/// Execution device for the model session.
/// 模型会话的执行设备。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgeDevice {
    /// NVIDIA GPU through the CUDA execution provider.
    /// 通过 CUDA 执行提供程序使用 NVIDIA GPU。
    Cuda,
    /// Any GPU with a WebGPU backend (Direct3D 12, Vulkan or Metal) through the WebGPU plugin
    /// execution provider.
    /// 通过 WebGPU 插件执行提供程序使用任何具备 WebGPU 后端（Direct3D 12、Vulkan 或 Metal）的 GPU。
    #[serde(rename = "webgpu")]
    WebGpu,
    /// CPU execution provider.
    /// CPU 执行提供程序。
    Cpu,
}

/// One call to judge: exact option modes plus the rendered call.
/// 一次待判定调用：精确选项档位加渲染后的调用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JudgeInput {
    /// Mode for every policy option.
    /// 每个策略选项的档位。
    pub settings: BTreeMap<String, String>,
    /// Output of [`crate::render_call`].
    /// [`crate::render_call`] 的输出。
    pub rendered: String,
}

/// Requests the service sends to the worker.
/// 服务发送给 worker 的请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum JudgeHostRequest {
    /// Verifies a package, creates the session on one device and runs the package self-check.
    /// 验证模型包，在指定设备上创建会话并运行模型包自检。
    Load {
        /// Request id.
        /// 请求标识。
        id: u64,
        /// Absolute package directory.
        /// 绝对模型包目录。
        package_dir: PathBuf,
        /// ONNX Runtime shared library to load.
        /// 要加载的 ONNX Runtime 共享库。
        runtime_library: PathBuf,
        /// Plugin execution-provider library; required for, and only accepted with, a plugin
        /// device such as WebGPU.
        /// 插件执行提供程序库；插件设备（如 WebGPU）必需，其他设备不接受。
        execution_provider_library: Option<PathBuf>,
        /// Device to use; a device that fails is reported, never replaced.
        /// 使用的设备；失败时如实报告，绝不替换。
        device: JudgeDevice,
    },
    /// Judges calls with the loaded model, preserving order.
    /// 用已加载模型判定调用，并保持顺序。
    Evaluate {
        /// Request id.
        /// 请求标识。
        id: u64,
        /// Calls to judge.
        /// 待判定调用。
        items: Vec<JudgeInput>,
    },
    /// Times encoding plus inference of `batch_size` self-check inputs.
    /// 计时 `batch_size` 条自检输入的编码加推理。
    Benchmark {
        /// Request id.
        /// 请求标识。
        id: u64,
        /// Rows per timed run.
        /// 每次计时运行的行数。
        batch_size: u32,
    },
    /// Reports the loaded model and resource use.
    /// 报告已加载模型与资源占用。
    Status {
        /// Request id.
        /// 请求标识。
        id: u64,
    },
}

/// Responses the worker sends to the service.
/// worker 发送给服务的响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum JudgeHostResponse {
    /// First line after start-up.
    /// 启动后的第一行。
    Ready {
        /// Protocol version spoken by the worker.
        /// worker 使用的协议版本。
        contract: String,
    },
    /// The package is verified, loaded and passed its self-check.
    /// 模型包已验证、已加载并通过自检。
    Loaded {
        /// Request id.
        /// 请求标识。
        id: u64,
        /// Load measurements.
        /// 加载测量结果。
        report: LoadReport,
    },
    /// Outcomes in request order.
    /// 按请求顺序排列的结果。
    Evaluated {
        /// Request id.
        /// 请求标识。
        id: u64,
        /// One outcome per input.
        /// 每条输入一个结果。
        outcomes: Vec<JudgeOutcome>,
    },
    /// Benchmark timings.
    /// 基准计时。
    Benchmarked {
        /// Request id.
        /// 请求标识。
        id: u64,
        /// Timings.
        /// 计时结果。
        report: BenchmarkReport,
    },
    /// Current status.
    /// 当前状态。
    Status {
        /// Request id.
        /// 请求标识。
        id: u64,
        /// Status snapshot.
        /// 状态快照。
        status: HostStatus,
    },
    /// The request failed; nothing was allowed.
    /// 请求失败；没有任何放行。
    Failed {
        /// Request id, absent when the request line could not be read.
        /// 请求标识；请求行无法读取时缺省。
        id: Option<u64>,
        /// English failure description.
        /// 英文失败描述。
        error: String,
    },
}

/// Measurements of one successful load.
/// 一次成功加载的测量结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadReport {
    /// Device the session runs on.
    /// 会话运行的设备。
    pub device: JudgeDevice,
    /// Package verification time.
    /// 模型包验证耗时。
    pub verify_ms: f64,
    /// Runtime and session creation time.
    /// 运行时与会话创建耗时。
    pub load_ms: f64,
    /// Self-check inference time, which also warms the session.
    /// 自检推理耗时，同时预热会话。
    pub warmup_ms: f64,
    /// Self-check inputs that matched their expected decision.
    /// 与预期判定一致的自检输入数。
    pub self_check_items: u32,
}

/// Result for one input.
/// 单条输入的结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum JudgeOutcome {
    /// The model scored the call.
    /// 模型已为调用评分。
    Evaluated {
        /// Decision after the margin rule.
        /// 应用余量规则后的判定。
        label: JudgeLabel,
        /// Raw scores in label order.
        /// 按标签顺序排列的原始分数。
        logits: [f32; 3],
        /// Sequence length the model read.
        /// 模型读取的序列长度。
        input_tokens: u32,
    },
    /// The complete sequence exceeds the model limit; the model did not run.
    /// 完整序列超过模型上限；模型未运行。
    InputTokenLimit {
        /// Sequence length that was refused.
        /// 被拒绝的序列长度。
        input_tokens: u32,
    },
}

/// Timings of repeated runs over the same batch.
/// 对同一批次重复运行的计时。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkReport {
    /// Device the session runs on.
    /// 会话运行的设备。
    pub device: JudgeDevice,
    /// Rows per run.
    /// 每次运行的行数。
    pub batch_size: u32,
    /// Timed runs after warm-up.
    /// 预热后的计时运行次数。
    pub rounds: u32,
    /// Median run time.
    /// 运行时间中位数。
    pub p50_ms: f64,
    /// 95th-percentile run time.
    /// 运行时间第 95 百分位。
    pub p95_ms: f64,
    /// Median time per row.
    /// 每行时间中位数。
    pub per_item_ms: f64,
    /// Rows per second at the median.
    /// 按中位数计的每秒行数。
    pub items_per_second: f64,
}

/// Loaded model and resource use; unknown metrics stay absent rather than zero.
/// 已加载模型与资源占用；未知指标保持缺省而非填 0。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostStatus {
    /// Loaded model, when any.
    /// 已加载模型（如有）。
    pub model: Option<LoadedModel>,
    /// Physical memory of the worker process.
    /// worker 进程的物理内存。
    pub working_set_bytes: Option<u64>,
    /// Private committed memory of the worker process.
    /// worker 进程的私有提交内存。
    pub private_bytes: Option<u64>,
    /// Evaluations since load.
    /// 加载以来的评估次数。
    pub evaluations: u64,
    /// Median of recent evaluation latencies.
    /// 最近评估延迟的中位数。
    pub recent_p50_ms: Option<f64>,
    /// 95th percentile of recent evaluation latencies.
    /// 最近评估延迟的第 95 百分位。
    pub recent_p95_ms: Option<f64>,
}

/// Identity of the loaded model.
/// 已加载模型的身份。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadedModel {
    /// Package directory.
    /// 模型包目录。
    pub package_dir: PathBuf,
    /// Digest of the package manifest.
    /// 模型包清单摘要。
    pub manifest_sha256: String,
    /// Policy revision.
    /// 策略修订。
    pub policy_version: String,
    /// Weight precision.
    /// 权重精度。
    pub precision: String,
    /// Device in use.
    /// 使用中的设备。
    pub device: JudgeDevice,
    /// Load measurements.
    /// 加载测量结果。
    pub report: LoadReport,
}

/// Encodes one message as a protocol line without the trailing newline.
/// 将一条消息编码为不含结尾换行的协议行。
pub fn encode_line<T: Serialize>(message: &T) -> Result<String, String> {
    serde_json::to_string(message)
        .map(|json| format!("{JUDGE_HOST_LINE_PREFIX}{json}"))
        .map_err(|error| format!("Failed to encode a judge-host message: {error}"))
}

/// Decodes one protocol line; a line without the prefix is not protocol data.
/// 解码一条协议行；没有前缀的行不是协议数据。
pub fn decode_line<T: DeserializeOwned>(line: &str) -> Result<T, String> {
    let json = line
        .trim_end_matches(['\r', '\n'])
        .strip_prefix(JUDGE_HOST_LINE_PREFIX)
        .ok_or_else(|| String::from("The line is not a judge-host protocol line."))?;
    serde_json::from_str(json).map_err(|error| format!("Malformed judge-host message: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_round_trip_and_unprefixed_output_is_rejected() {
        let request = JudgeHostRequest::Evaluate {
            id: 7,
            items: vec![JudgeInput {
                settings: BTreeMap::from([("writes".into(), "allow".into())]),
                rendered: "Tool: shell_command\nArguments: {}".into(),
            }],
        };
        let line = encode_line(&request).unwrap();
        assert!(!line.contains('\n'), "a message must stay on one line");
        assert_eq!(
            decode_line::<JudgeHostRequest>(&format!("{line}\r\n")),
            Ok(request)
        );
        assert!(decode_line::<JudgeHostRequest>("CUDA warning: something").is_err());

        let response = JudgeHostResponse::Evaluated {
            id: 7,
            outcomes: vec![
                JudgeOutcome::Evaluated {
                    label: JudgeLabel::Review,
                    logits: [0.5, 1.0, -1.0],
                    input_tokens: 230,
                },
                JudgeOutcome::InputTokenLimit {
                    input_tokens: 40000,
                },
            ],
        };
        let line = encode_line(&response).unwrap();
        assert_eq!(decode_line::<JudgeHostResponse>(&line), Ok(response));
    }
}
