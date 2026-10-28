//! Request handling of one worker process: one load, then evaluations, benchmarks and status.
//! 单个 worker 进程的请求处理：一次加载，随后评估、基准与状态。

use std::collections::VecDeque;
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Instant;

use serde::Deserialize;
use tokenizers::Tokenizer;
use vulcan_permission_judge::package::{EXAMPLES_FILE, TOKENIZER_FILE, VALIDATION_FILE};
use vulcan_permission_judge::protocol::{
    BenchmarkReport, HostStatus, JUDGE_HOST_CONTRACT, JudgeDevice, JudgeHostRequest,
    JudgeHostResponse, JudgeInput, JudgeOutcome, LoadReport, LoadedModel, decode_line, encode_line,
};
use vulcan_permission_judge::{JudgeLabel, JudgePackage, JudgeSettings, MemoryDigestCache};

use crate::engine::{Engine, initialize_runtime};
use crate::gpu::GpuCounters;
use crate::sequence::Encoder;
use crate::status::{percentile, process_memory};

/// Evaluation latencies kept for the status percentiles.
/// 为状态百分位保留的评估延迟数量。
const RECENT_LATENCY_WINDOW: usize = 100;

/// Untimed benchmark runs before measuring.
/// 计时前的不计时基准运行次数。
const BENCHMARK_WARMUP_ROUNDS: u32 = 3;

/// Timed benchmark runs.
/// 计时基准运行次数。
const BENCHMARK_TIMED_ROUNDS: u32 = 20;

/// Expected self-check results shipped with the package.
/// 模型包随附的自检预期结果。
#[derive(Deserialize)]
struct Validation {
    examples: Vec<ExpectedExample>,
}

/// Expected result of one self-check input.
/// 单条自检输入的预期结果。
#[derive(Deserialize)]
struct ExpectedExample {
    decision: JudgeLabel,
    input_tokens: u32,
}

/// Packaged self-check inputs with their expected results, in the same order.
/// 模型包随附的自检输入及其预期结果，顺序一致。
struct SelfCheck {
    settings: Vec<JudgeSettings>,
    rendered: Vec<String>,
    expected: Vec<ExpectedExample>,
}

/// A loaded model with everything needed to serve requests.
/// 已加载模型及服务请求所需的一切。
struct Loaded {
    engine: Engine,
    encoder: Encoder,
    package: JudgePackage,
    examples: Vec<JudgeSettings>,
    rendered: Vec<String>,
    model: LoadedModel,
    evaluations: u64,
    latencies: VecDeque<f64>,
}

/// Lifecycle of the single ONNX Runtime a process may initialize.
/// 进程可初始化的唯一 ONNX Runtime 的生命周期。
enum State {
    /// No runtime is initialized yet.
    /// 尚未初始化运行时。
    Fresh,
    /// The runtime was initialized but the load did not finish; a new worker is required.
    /// 运行时已初始化但加载未完成；需要新的 worker。
    Spent,
    /// A model is serving.
    /// 模型正在服务。
    Loaded(Box<Loaded>),
}

/// Handles requests for one worker process.
/// 处理单个 worker 进程的请求。
pub struct Worker {
    state: State,
    cache: MemoryDigestCache,
    gpu: Option<GpuCounters>,
}

impl Default for Worker {
    fn default() -> Self {
        Self {
            state: State::Fresh,
            cache: MemoryDigestCache::default(),
            gpu: GpuCounters::open(std::process::id()),
        }
    }
}

impl Worker {
    /// Answers one request; every failure becomes a `Failed` response.
    /// 应答一个请求；所有失败都转为 `Failed` 响应。
    pub fn handle(&mut self, request: JudgeHostRequest) -> JudgeHostResponse {
        let (id, result) = match request {
            JudgeHostRequest::Load {
                id,
                package_dir,
                runtime_library,
                execution_provider_library,
                device,
            } => (
                id,
                self.load(
                    &package_dir,
                    &runtime_library,
                    execution_provider_library.as_deref(),
                    device,
                )
                .map(|report| JudgeHostResponse::Loaded { id, report }),
            ),
            JudgeHostRequest::Evaluate { id, items } => (
                id,
                self.evaluate(&items)
                    .map(|outcomes| JudgeHostResponse::Evaluated { id, outcomes }),
            ),
            JudgeHostRequest::Benchmark { id, batch_size } => (
                id,
                self.benchmark(batch_size)
                    .map(|report| JudgeHostResponse::Benchmarked { id, report }),
            ),
            JudgeHostRequest::Status { id } => (
                id,
                Ok(JudgeHostResponse::Status {
                    id,
                    status: Box::new(self.status()),
                }),
            ),
        };
        result.unwrap_or_else(|error| JudgeHostResponse::Failed {
            id: Some(id),
            error,
        })
    }

    /// Verifies the package, initializes the runtime once, loads the session and runs the
    /// self-check.
    /// 验证模型包，一次性初始化运行时，加载会话并运行自检。
    fn load(
        &mut self,
        package_dir: &Path,
        runtime_library: &Path,
        execution_provider_library: Option<&Path>,
        device: JudgeDevice,
    ) -> Result<LoadReport, String> {
        match self.state {
            State::Fresh => {}
            State::Spent | State::Loaded(_) => {
                return Err(String::from(
                    "This worker has already initialized its runtime; start a new worker to load a model.",
                ));
            }
        }
        let started = Instant::now();
        let package = JudgePackage::open(package_dir, &self.cache)
            .map_err(|error| format!("Package verification failed ({}): {error}", error.code()))?;
        let tokenizer = Tokenizer::from_file(package.file(TOKENIZER_FILE))
            .map_err(|error| format!("Failed to read the tokenizer: {error}"))?;
        let encoder = Encoder::new(tokenizer, &package.policy, &package.config)?;
        let SelfCheck {
            settings: examples,
            rendered,
            expected,
        } = self_check_inputs(&package)?;
        let verify_ms = elapsed_ms(started);

        let started = Instant::now();
        self.state = State::Spent;
        initialize_runtime(runtime_library)?;
        let mut engine = Engine::load(&package, device, execution_provider_library)?;
        let load_ms = elapsed_ms(started);

        let started = Instant::now();
        let outcomes = engine.predict(&encoder, &pairs(&examples, &rendered))?;
        let warmup_ms = elapsed_ms(started);
        for (index, (outcome, expected)) in outcomes.iter().zip(&expected).enumerate() {
            let matches = matches!(
                outcome,
                JudgeOutcome::Evaluated { label, input_tokens, .. }
                    if *label == expected.decision && *input_tokens == expected.input_tokens
            );
            if !matches {
                return Err(format!(
                    "Self-check input {index} did not reproduce the packaged decision."
                ));
            }
        }

        let report = LoadReport {
            device,
            verify_ms,
            load_ms,
            warmup_ms,
            self_check_items: expected.len() as u32,
            limits: package.config.batch_limits(),
        };
        let model = LoadedModel {
            package_dir: package.root().to_path_buf(),
            manifest_sha256: package.summary.manifest_sha256.clone(),
            policy_version: package.summary.policy_version.clone(),
            precision: package.summary.precision.clone(),
            device: engine.device,
            report: report.clone(),
        };
        self.state = State::Loaded(Box::new(Loaded {
            engine,
            encoder,
            package,
            examples,
            rendered,
            model,
            evaluations: 0,
            latencies: VecDeque::with_capacity(RECENT_LATENCY_WINDOW),
        }));
        Ok(report)
    }

    /// Judges calls in order; any invalid setting fails the whole request.
    /// 按顺序判定调用；任一无效设置使整个请求失败。
    fn evaluate(&mut self, items: &[JudgeInput]) -> Result<Vec<JudgeOutcome>, String> {
        let loaded = self.loaded()?;
        let limit = loaded.package.config.max_request_items as usize;
        if items.len() > limit {
            return Err(format!(
                "A request may contain at most {limit} calls; received {}.",
                items.len()
            ));
        }
        let mut settings = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            settings.push(
                loaded
                    .package
                    .policy
                    .exact_settings(&item.settings)
                    .map_err(|error| format!("Call {index} has invalid settings: {error}"))?,
            );
        }
        let rendered: Vec<String> = items.iter().map(|item| item.rendered.clone()).collect();
        let started = Instant::now();
        let outcomes = loaded
            .engine
            .predict(&loaded.encoder, &pairs(&settings, &rendered))?;
        if loaded.latencies.len() == RECENT_LATENCY_WINDOW {
            loaded.latencies.pop_front();
        }
        loaded.latencies.push_back(elapsed_ms(started));
        loaded.evaluations += 1;
        Ok(outcomes)
    }

    /// Times repeated runs over `batch_size` self-check inputs.
    /// 对 `batch_size` 条自检输入的重复运行计时。
    fn benchmark(&mut self, batch_size: u32) -> Result<BenchmarkReport, String> {
        let loaded = self.loaded()?;
        let limit = loaded.package.config.max_request_items;
        if batch_size == 0 || batch_size > limit {
            return Err(format!("The batch size must be between 1 and {limit}."));
        }
        let all = pairs(&loaded.examples, &loaded.rendered);
        let batch: Vec<(JudgeSettings, &str)> = all
            .iter()
            .cycle()
            .take(batch_size as usize)
            .cloned()
            .collect();
        for _ in 0..BENCHMARK_WARMUP_ROUNDS {
            loaded.engine.predict(&loaded.encoder, &batch)?;
        }
        let mut samples = Vec::with_capacity(BENCHMARK_TIMED_ROUNDS as usize);
        for _ in 0..BENCHMARK_TIMED_ROUNDS {
            let started = Instant::now();
            loaded.engine.predict(&loaded.encoder, &batch)?;
            samples.push(elapsed_ms(started));
        }
        let p50_ms = percentile(&samples, 0.5).unwrap_or_default();
        let p95_ms = percentile(&samples, 0.95).unwrap_or_default();
        Ok(BenchmarkReport {
            device: loaded.engine.device,
            batch_size,
            rounds: BENCHMARK_TIMED_ROUNDS,
            p50_ms,
            p95_ms,
            per_item_ms: p50_ms / f64::from(batch_size),
            items_per_second: if p50_ms > 0.0 {
                f64::from(batch_size) * 1000.0 / p50_ms
            } else {
                0.0
            },
        })
    }

    /// Reports the loaded model and resource use.
    /// 报告已加载模型与资源占用。
    fn status(&mut self) -> HostStatus {
        let memory = process_memory();
        let gpu = self.gpu.as_mut().map(GpuCounters::read).unwrap_or_default();
        let (model, evaluations, latencies) = match &self.state {
            State::Loaded(loaded) => (
                Some(loaded.model.clone()),
                loaded.evaluations,
                loaded.latencies.iter().copied().collect::<Vec<_>>(),
            ),
            State::Fresh | State::Spent => (None, 0, Vec::new()),
        };
        HostStatus {
            model,
            working_set_bytes: memory.working_set_bytes,
            private_bytes: memory.private_bytes,
            gpu_dedicated_bytes: gpu.dedicated_bytes,
            gpu_shared_bytes: gpu.shared_bytes,
            gpu_utilization_percent: gpu.utilization_percent,
            evaluations,
            recent_p50_ms: percentile(&latencies, 0.5),
            recent_p95_ms: percentile(&latencies, 0.95),
        }
    }

    fn loaded(&mut self) -> Result<&mut Loaded, String> {
        match &mut self.state {
            State::Loaded(loaded) => Ok(loaded),
            State::Fresh | State::Spent => Err(String::from("No model is loaded.")),
        }
    }
}

/// Sends `Ready`, then answers each request line in order until input ends.
/// 发送 `Ready`，随后按顺序应答每个请求行，直到输入结束。
pub fn serve(
    input: impl BufRead,
    mut output: impl Write,
    worker: &mut Worker,
) -> Result<(), String> {
    let mut send = |response: &JudgeHostResponse| -> Result<(), String> {
        let line = encode_line(response)?;
        writeln!(output, "{line}")
            .and_then(|()| output.flush())
            .map_err(|error| format!("Failed to write a response: {error}"))
    };
    send(&JudgeHostResponse::Ready {
        contract: JUDGE_HOST_CONTRACT.to_string(),
    })?;
    for line in input.lines() {
        let line = line.map_err(|error| format!("Failed to read a request: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match decode_line::<JudgeHostRequest>(&line) {
            Ok(request) => worker.handle(request),
            Err(error) => JudgeHostResponse::Failed { id: None, error },
        };
        send(&response)?;
    }
    Ok(())
}

/// Reads the packaged self-check inputs and their expected results.
/// 读取模型包随附的自检输入及其预期结果。
fn self_check_inputs(package: &JudgePackage) -> Result<SelfCheck, String> {
    let read = |file: &str| {
        std::fs::read(package.file(file))
            .map_err(|error| format!("Failed to read `{file}`: {error}"))
    };
    let inputs: Vec<JudgeInput> = serde_json::from_slice(&read(EXAMPLES_FILE)?)
        .map_err(|error| format!("Malformed `{EXAMPLES_FILE}`: {error}"))?;
    let validation: Validation = serde_json::from_slice(&read(VALIDATION_FILE)?)
        .map_err(|error| format!("Malformed `{VALIDATION_FILE}`: {error}"))?;
    if inputs.is_empty() || inputs.len() != validation.examples.len() {
        return Err(String::from(
            "The package self-check inputs and expected results do not correspond.",
        ));
    }
    let mut settings = Vec::with_capacity(inputs.len());
    for input in &inputs {
        settings.push(
            package
                .policy
                .exact_settings(&input.settings)
                .map_err(|error| format!("A package self-check input is invalid: {error}"))?,
        );
    }
    Ok(SelfCheck {
        settings,
        rendered: inputs.into_iter().map(|input| input.rendered).collect(),
        expected: validation.examples,
    })
}

fn pairs<'a>(settings: &[JudgeSettings], rendered: &'a [String]) -> Vec<(JudgeSettings, &'a str)> {
    settings
        .iter()
        .cloned()
        .zip(rendered.iter().map(String::as_str))
        .collect()
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn responses(output: &[u8]) -> Vec<JudgeHostResponse> {
        String::from_utf8_lossy(output)
            .lines()
            .map(|line| decode_line(line).unwrap())
            .collect()
    }

    #[test]
    fn requests_before_a_load_fail_with_their_id() {
        let input = [
            encode_line(&JudgeHostRequest::Evaluate {
                id: 1,
                items: Vec::new(),
            })
            .unwrap(),
            String::from("not a protocol line"),
            String::new(),
            encode_line(&JudgeHostRequest::Status { id: 2 }).unwrap(),
            encode_line(&JudgeHostRequest::Benchmark {
                id: 3,
                batch_size: 1,
            })
            .unwrap(),
        ]
        .join("\n");
        let mut output = Vec::new();
        serve(input.as_bytes(), &mut output, &mut Worker::default()).unwrap();
        let responses = responses(&output);
        assert_eq!(responses.len(), 5);
        assert!(
            matches!(&responses[0], JudgeHostResponse::Ready { contract } if contract == JUDGE_HOST_CONTRACT)
        );
        assert!(matches!(
            responses[1],
            JudgeHostResponse::Failed { id: Some(1), .. }
        ));
        assert!(matches!(
            responses[2],
            JudgeHostResponse::Failed { id: None, .. }
        ));
        assert!(matches!(
            &responses[3],
            JudgeHostResponse::Status { id: 2, status } if status.model.is_none() && status.evaluations == 0
        ));
        assert!(matches!(
            responses[4],
            JudgeHostResponse::Failed { id: Some(3), .. }
        ));
    }

    #[test]
    fn a_package_that_fails_verification_leaves_the_runtime_untouched() {
        let mut worker = Worker::default();
        let request = |id| JudgeHostRequest::Load {
            id,
            package_dir: PathBuf::from("relative/package"),
            runtime_library: PathBuf::from("relative/onnxruntime.dll"),
            execution_provider_library: None,
            device: JudgeDevice::Cpu,
        };
        assert!(matches!(
            worker.handle(request(1)),
            JudgeHostResponse::Failed { id: Some(1), .. }
        ));
        assert!(
            matches!(worker.state, State::Fresh),
            "verification runs before the runtime"
        );
        assert!(matches!(
            worker.handle(request(2)),
            JudgeHostResponse::Failed { id: Some(2), .. }
        ));
    }

    /// Loads a real package on the CPU. Run with `--ignored` after setting
    /// `VULCAN_JUDGE_TEST_PACKAGE` (package directory) and `VULCAN_JUDGE_TEST_ORT` (ONNX Runtime
    /// library), both absolute.
    /// 在 CPU 上加载真实模型包。设置绝对路径 `VULCAN_JUDGE_TEST_PACKAGE`（模型包目录）与
    /// `VULCAN_JUDGE_TEST_ORT`（ONNX Runtime 库）后以 `--ignored` 运行。
    #[test]
    #[ignore = "needs a model package and an ONNX Runtime library"]
    fn a_real_package_loads_judges_and_benchmarks_on_the_cpu() {
        let path = |name| {
            PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name:?} is not set")))
        };
        let mut worker = Worker::default();
        let response = worker.handle(JudgeHostRequest::Load {
            id: 1,
            package_dir: path("VULCAN_JUDGE_TEST_PACKAGE"),
            runtime_library: path("VULCAN_JUDGE_TEST_ORT"),
            execution_provider_library: None,
            device: JudgeDevice::Cpu,
        });
        let JudgeHostResponse::Loaded { report, .. } = response else {
            panic!("load failed: {response:?}");
        };
        assert!(report.self_check_items > 0);

        let State::Loaded(loaded) = &worker.state else {
            panic!("the worker must be loaded");
        };
        let mut items: Vec<JudgeInput> = loaded
            .examples
            .iter()
            .zip(&loaded.rendered)
            .map(|(settings, rendered)| JudgeInput {
                settings: settings.to_map(),
                rendered: rendered.clone(),
            })
            .collect();
        items.push(JudgeInput {
            settings: items[0].settings.clone(),
            rendered: "x ".repeat(40_000),
        });
        let JudgeHostResponse::Evaluated { outcomes, .. } =
            worker.handle(JudgeHostRequest::Evaluate { id: 2, items })
        else {
            panic!("evaluation failed");
        };
        assert!(matches!(
            outcomes.last(),
            Some(JudgeOutcome::InputTokenLimit { .. })
        ));
        assert!(
            outcomes[..outcomes.len() - 1]
                .iter()
                .all(|outcome| matches!(outcome, JudgeOutcome::Evaluated { .. }))
        );

        let JudgeHostResponse::Benchmarked { report, .. } =
            worker.handle(JudgeHostRequest::Benchmark {
                id: 3,
                batch_size: 10,
            })
        else {
            panic!("benchmark failed");
        };
        assert!(report.p50_ms > 0.0 && report.p95_ms >= report.p50_ms);
        let JudgeHostResponse::Status { status, .. } =
            worker.handle(JudgeHostRequest::Status { id: 4 })
        else {
            panic!("status failed");
        };
        assert_eq!(status.evaluations, 1);
        assert!(status.model.is_some() && status.recent_p50_ms.is_some());
    }
}
