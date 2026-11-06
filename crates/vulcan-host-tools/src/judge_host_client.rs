//! Parent-side client for one isolated `vulcan-judge-host` worker process.
//! 单个隔离 `vulcan-judge-host` worker 进程的父端客户端。
//!
//! Requests are answered strictly in order, so the client owns the pipes exclusively and callers
//! serialize access. A worker `Failed` response is an ordinary error and keeps the worker usable;
//! a transport failure, protocol violation or timeout kills the worker, after which every request
//! fails until a new worker is spawned.
//! 请求严格按顺序应答，因此客户端独占管道，由调用方串行访问。worker 的 `Failed` 响应是普通错误，
//! worker 仍可使用；传输失败、协议违规或超时会结束 worker，此后所有请求均失败，直到派生新 worker。

use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::time::{Duration, timeout};
use vulcan_permission_judge::protocol::{
    BenchmarkReport, HostStatus, JUDGE_HOST_CONTRACT, JUDGE_HOST_LINE_PREFIX, JudgeDevice,
    JudgeHostRequest, JudgeHostResponse, JudgeInput, JudgeOutcome, LoadReport, decode_line,
    encode_line,
};

use crate::native_host_binary::resolve_native_host_binary;
use crate::process_creation::new_background_tokio_command;

/// Waiting limit for the worker's `Ready` line.
/// 等待 worker `Ready` 行的上限。
const JUDGE_HOST_READY_TIMEOUT: Duration = Duration::from_secs(10);

/// Waiting limit for package verification, session creation and the self-check.
/// 等待模型包验证、会话创建与自检的上限。
const JUDGE_HOST_LOAD_TIMEOUT: Duration = Duration::from_secs(300);

/// Waiting limit for one evaluation request.
/// 等待单个评估请求的上限。
pub const JUDGE_HOST_EVALUATE_TIMEOUT: Duration = Duration::from_secs(120);

/// Waiting limit for one benchmark request.
/// 等待单个基准请求的上限。
const JUDGE_HOST_BENCHMARK_TIMEOUT: Duration = Duration::from_secs(300);

/// Waiting limit for one status request.
/// 等待单个状态请求的上限。
const JUDGE_HOST_STATUS_TIMEOUT: Duration = Duration::from_secs(10);

/// Waiting limit for the worker to exit after its input closes.
/// worker 输入关闭后等待其退出的上限。
const JUDGE_HOST_EXIT_TIMEOUT: Duration = Duration::from_secs(5);

/// Largest stdout line accepted from the worker.
/// 接受的 worker 标准输出单行最大字节数。
const JUDGE_HOST_MAX_LINE_BYTES: usize = 4 << 20;

/// Trailing worker diagnostics kept for error messages.
/// 为错误信息保留的 worker 诊断尾部字节数。
const JUDGE_HOST_STDERR_TAIL_BYTES: usize = 8 << 10;

/// Variable the platform dynamic loader searches for dependent libraries.
/// 平台动态加载器搜索依赖库所用的变量。
const LIBRARY_SEARCH_VARIABLE: &str = if cfg!(windows) {
    "PATH"
} else if cfg!(target_os = "macos") {
    "DYLD_LIBRARY_PATH"
} else {
    "LD_LIBRARY_PATH"
};

/// Prepends `library_dirs` to the worker's library search variable without touching the parent.
/// 将 `library_dirs` 前置到 worker 的库搜索变量，不修改父进程环境。
fn apply_library_search_path(
    command: &mut tokio::process::Command,
    library_dirs: &[PathBuf],
) -> Result<(), String> {
    if library_dirs.is_empty() {
        return Ok(());
    }
    let mut paths: Vec<PathBuf> = Vec::new();
    for directory in library_dirs {
        if !paths.contains(directory) {
            paths.push(directory.clone());
        }
    }
    if let Some(inherited) = std::env::var_os(LIBRARY_SEARCH_VARIABLE) {
        paths.extend(std::env::split_paths(&inherited));
    }
    let joined = std::env::join_paths(paths).map_err(|error| {
        format!("Failed to build the judge host {LIBRARY_SEARCH_VARIABLE}: {error}")
    })?;
    command.env(LIBRARY_SEARCH_VARIABLE, joined);
    Ok(())
}

/// Owns one worker process and its pipes.
/// 持有一个 worker 进程及其管道。
pub struct JudgeHostClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    stderr_tail: Arc<Mutex<VecDeque<u8>>>,
    next_id: u64,
    broken: Option<String>,
}

impl JudgeHostClient {
    /// Starts a worker and waits for its `Ready` line with the expected contract.
    /// 启动 worker 并等待其带预期契约的 `Ready` 行。
    ///
    /// # Parameters / 参数
    /// - `library_dirs`: Directories prepended to the worker's library search path, such as the CUDA runtime.
    /// - `library_dirs`：前置到 worker 库搜索路径的目录，例如 CUDA 运行库目录。
    pub async fn spawn(library_dirs: &[PathBuf]) -> Result<Self, String> {
        let host_path = resolve_native_host_binary("vulcan-judge-host", "VULCAN_JUDGE_HOST")?;
        let mut command =
            new_background_tokio_command(crate::runtime_assets::native_host_path(&host_path));
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        apply_library_search_path(&mut command, library_dirs)?;
        let mut child = command.spawn().map_err(|error| {
            format!(
                "Failed to start judge host `{}`: {error}",
                host_path.display()
            )
        })?;
        let pipe = |name: &str| format!("Judge host started without a {name} pipe.");
        let stdin = child.stdin.take().ok_or_else(|| pipe("stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| pipe("stdout"))?;
        let mut stderr = child.stderr.take().ok_or_else(|| pipe("stderr"))?;
        let stderr_tail = Arc::new(Mutex::new(VecDeque::new()));
        let tail = Arc::clone(&stderr_tail);
        tokio::spawn(async move {
            let mut buffer = [0_u8; 4096];
            while let Ok(read) = stderr.read(&mut buffer).await {
                if read == 0 {
                    break;
                }
                if let Ok(mut tail) = tail.lock() {
                    tail.extend(&buffer[..read]);
                    let excess = tail.len().saturating_sub(JUDGE_HOST_STDERR_TAIL_BYTES);
                    tail.drain(..excess);
                }
            }
        });
        let mut client = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            stderr_tail,
            next_id: 1,
            broken: None,
        };
        match timeout(JUDGE_HOST_READY_TIMEOUT, client.read_response()).await {
            Ok(Ok(JudgeHostResponse::Ready { contract })) if contract == JUDGE_HOST_CONTRACT => {
                Ok(client)
            }
            Ok(Ok(JudgeHostResponse::Ready { contract })) => Err(client.fail(format!(
                "Judge host speaks `{contract}`, but `{JUDGE_HOST_CONTRACT}` is required."
            ))),
            Ok(Ok(other)) => Err(client.fail(format!(
                "Judge host sent {other:?} before announcing readiness."
            ))),
            Ok(Err(error)) => Err(client.fail(error)),
            Err(_) => Err(client.fail(String::from(
                "Judge host did not announce readiness in time.",
            ))),
        }
    }

    /// Operating-system process id of the worker, while it runs.
    /// worker 运行期间的操作系统进程标识。
    pub fn process_id(&self) -> Option<u32> {
        self.child.id()
    }

    /// Whether requests can still reach the worker; false after a transport failure, protocol
    /// violation or timeout.
    /// 请求是否仍能到达 worker；传输失败、协议违规或超时后为 false。
    pub fn is_usable(&self) -> bool {
        self.broken.is_none()
    }

    /// Verifies and loads a package on exactly `device`; a worker accepts one load.
    /// 在指定 `device` 上验证并加载模型包；一个 worker 只接受一次加载。
    pub async fn load(
        &mut self,
        package_dir: PathBuf,
        runtime_library: PathBuf,
        execution_provider_library: Option<PathBuf>,
        device: JudgeDevice,
    ) -> Result<LoadReport, String> {
        let response = self
            .call(JUDGE_HOST_LOAD_TIMEOUT, |id| JudgeHostRequest::Load {
                id,
                package_dir,
                runtime_library,
                execution_provider_library,
                device,
            })
            .await?;
        match response {
            JudgeHostResponse::Loaded { report, .. } => Ok(report),
            other => Err(self.unexpected(&other)),
        }
    }

    /// Judges calls; outcomes keep the input order.
    /// 判定调用；结果保持输入顺序。
    pub async fn evaluate(&mut self, items: Vec<JudgeInput>) -> Result<Vec<JudgeOutcome>, String> {
        let count = items.len();
        let response = self
            .call(JUDGE_HOST_EVALUATE_TIMEOUT, |id| {
                JudgeHostRequest::Evaluate { id, items }
            })
            .await?;
        match response {
            JudgeHostResponse::Evaluated { outcomes, .. } if outcomes.len() == count => {
                Ok(outcomes)
            }
            other => Err(self.unexpected(&other)),
        }
    }

    /// Times repeated runs over `batch_size` packaged inputs.
    /// 对 `batch_size` 条模型包输入的重复运行计时。
    pub async fn benchmark(&mut self, batch_size: u32) -> Result<BenchmarkReport, String> {
        let response = self
            .call(JUDGE_HOST_BENCHMARK_TIMEOUT, |id| {
                JudgeHostRequest::Benchmark { id, batch_size }
            })
            .await?;
        match response {
            JudgeHostResponse::Benchmarked { report, .. } => Ok(report),
            other => Err(self.unexpected(&other)),
        }
    }

    /// Reports the loaded model and process resource use.
    /// 报告已加载模型与进程资源占用。
    pub async fn status(&mut self) -> Result<HostStatus, String> {
        let response = self
            .call(JUDGE_HOST_STATUS_TIMEOUT, |id| JudgeHostRequest::Status {
                id,
            })
            .await?;
        match response {
            JudgeHostResponse::Status { status, .. } => Ok(*status),
            other => Err(self.unexpected(&other)),
        }
    }

    /// Closes the worker input and waits briefly for it to exit, then kills it.
    /// 关闭 worker 输入并短暂等待其退出，随后结束进程。
    pub async fn shutdown(mut self) {
        // Tokio's pipe shutdown does not close the handle; only dropping it gives the worker EOF.
        // Tokio 管道的 shutdown 不会关闭句柄；只有丢弃它才能让 worker 读到 EOF。
        drop(self.stdin);
        if timeout(JUDGE_HOST_EXIT_TIMEOUT, self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.kill().await;
        }
    }

    /// Sends one request and reads its response; worker failures become errors.
    /// 发送一个请求并读取其响应；worker 失败转为错误。
    async fn call(
        &mut self,
        limit: Duration,
        build: impl FnOnce(u64) -> JudgeHostRequest,
    ) -> Result<JudgeHostResponse, String> {
        if let Some(reason) = &self.broken {
            return Err(format!("The judge host is no longer usable: {reason}"));
        }
        let id = self.next_id;
        self.next_id += 1;
        let line = encode_line(&build(id))?;
        let exchange = async {
            self.stdin
                .write_all(format!("{line}\n").as_bytes())
                .await
                .and(self.stdin.flush().await)
                .map_err(|error| format!("Failed to send a judge-host request: {error}"))?;
            self.read_response().await
        };
        let response = match timeout(limit, exchange).await {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => return Err(self.fail(error)),
            Err(_) => {
                return Err(self.fail(format!(
                    "The judge host did not answer within {} seconds.",
                    limit.as_secs()
                )));
            }
        };
        match response {
            JudgeHostResponse::Failed {
                id: Some(answered),
                error,
            } if answered == id => Err(error),
            JudgeHostResponse::Failed { id: None, error } => {
                Err(self.fail(format!("The judge host could not read a request: {error}")))
            }
            response if response_id(&response) == Some(id) => Ok(response),
            other => Err(self.unexpected(&other)),
        }
    }

    /// Reads the next protocol line, skipping unprefixed native output.
    /// 读取下一条协议行，跳过无前缀的原生输出。
    async fn read_response(&mut self) -> Result<JudgeHostResponse, String> {
        loop {
            let mut buffer = Vec::new();
            let read = (&mut self.stdout)
                .take(JUDGE_HOST_MAX_LINE_BYTES as u64 + 1)
                .read_until(b'\n', &mut buffer)
                .await
                .map_err(|error| format!("Failed to read from the judge host: {error}"))?;
            if read == 0 {
                return Err(String::from("The judge host exited."));
            }
            if buffer.len() > JUDGE_HOST_MAX_LINE_BYTES {
                return Err(String::from("The judge host wrote an oversized line."));
            }
            let line = String::from_utf8_lossy(&buffer);
            if line.starts_with(JUDGE_HOST_LINE_PREFIX) {
                return decode_line(&line);
            }
        }
    }

    /// Reports a response of the wrong kind and abandons the worker.
    /// 报告类型错误的响应并放弃该 worker。
    fn unexpected(&mut self, response: &JudgeHostResponse) -> String {
        self.fail(format!(
            "The judge host sent an unexpected response: {response:?}"
        ))
    }

    /// Marks the worker unusable, kills it and returns the error with recent diagnostics.
    /// 将 worker 标记为不可用并结束进程，返回附带最近诊断信息的错误。
    fn fail(&mut self, error: String) -> String {
        let _ = self.child.start_kill();
        self.broken = Some(error.clone());
        let tail = self
            .stderr_tail
            .lock()
            .map(|tail| {
                String::from_utf8_lossy(&tail.iter().copied().collect::<Vec<_>>()).into_owned()
            })
            .unwrap_or_default();
        let tail = tail.trim();
        if tail.is_empty() {
            error
        } else {
            format!("{error} Worker diagnostics: {tail}")
        }
    }
}

/// Request id echoed by a response, when it carries one.
/// 响应回显的请求标识（如有）。
fn response_id(response: &JudgeHostResponse) -> Option<u64> {
    match response {
        JudgeHostResponse::Ready { .. } => None,
        JudgeHostResponse::Loaded { id, .. }
        | JudgeHostResponse::Evaluated { id, .. }
        | JudgeHostResponse::Benchmarked { id, .. }
        | JudgeHostResponse::Status { id, .. } => Some(*id),
        JudgeHostResponse::Failed { id, .. } => *id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Talks to the real worker binary when the test build can find it.
    /// 测试构建能找到真实 worker 二进制时与其通信。
    #[tokio::test]
    async fn worker_failures_keep_the_worker_and_status_works_before_a_load() {
        if resolve_native_host_binary("vulcan-judge-host", "VULCAN_JUDGE_HOST").is_err() {
            return;
        }
        let mut client = JudgeHostClient::spawn(&[])
            .await
            .expect("start the judge host");
        let status = client.status().await.expect("status before a load");
        assert!(status.model.is_none());
        assert!(
            client.evaluate(Vec::new()).await.is_err(),
            "nothing is loaded"
        );
        let refused = client
            .load(
                PathBuf::from("relative/package"),
                PathBuf::from("relative/onnxruntime.dll"),
                None,
                JudgeDevice::Cpu,
            )
            .await;
        assert!(refused.is_err());
        assert!(
            client.status().await.is_ok(),
            "a worker failure leaves the worker usable"
        );
        client.shutdown().await;
    }
}
