//! Host-owned VulcanMemoryMesh child-process lifecycle and trusted readiness handshake.
//! 宿主持有的 VulcanMemoryMesh 子进程生命周期与可信就绪握手。

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        OnceLock, RwLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use uuid::Uuid;
use vulcan_ai_contract::{
    AiOperation, EmbeddingEncoding, EmbeddingInput, EmbeddingInputContent,
    EmbeddingInvalidInputPolicy, EmbeddingOutputKind, EmbeddingRequest, EmbeddingSpaceFingerprint,
    EmbeddingTask, INFERENCE_CONTRACT_VERSION,
};
use vulcan_provider_adapters::{
    ConnectionModelCatalog, InferenceRouteBinding, PersistedAiConfiguration,
    SecondaryRouteSelection,
};
use vulcan_service_api::{
    INFERENCE_SERVICE_API_VERSION, InferenceServiceDiscoverySnapshot,
    inference_service_discovery_snapshot_path,
};
use vulcan_vmm::{
    MANAGED_CONTRACT_OWNER, MANAGED_CONTRACT_VERSION, MANAGED_INFERENCE_MODE,
    MANAGED_RUNTIME_BUILD_ID, ManagedAssets, ManagedConfig, ManagedControllerLease,
    ManagedGrpcConfig, ManagedGrpcKeepalive, ManagedGrpcRequestTimeout, ManagedInferenceConfig,
    ManagedLoggingConfig, ManagedManagementConfig, ManagedMemoryPipelineConfig, ManagedNoiseConfig,
    ManagedParent, ManagedPiiConfig, ManagedPostActionConfig, ManagedPreCheckConfig,
    ManagedPromptConfig, ManagedRelationalConfig, ManagedRetentionConfig, ManagedRuntimeConfig,
    ManagedStatus, ManagedStorageConfig, ManagedVectorConfig, ResolveProjectRequest,
    ResolveUserRequest, TonicVmmClient, VmmClient, VmmTransportConfig,
};

/// Stable isolated Controller space used only by the managed VMM runtime.
/// 仅由托管 VMM 运行时使用的稳定隔离 Controller Space。
const MANAGED_VMM_SPACE_ID: &str = "vmm-memory";

/// Human-readable label for the isolated managed VMM Controller space.
/// 隔离托管 VMM Controller Space 的可读标签。
const MANAGED_VMM_SPACE_LABEL: &str = "VulcanMemoryMesh";

/// Maximum time allowed for child readiness and authenticated Healthz.
/// 子进程就绪与鉴权 Healthz 的最长允许时间。
const MANAGED_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

/// Stable manifest filename placed beside the packaged VMM configuration assets.
/// 放置在打包 VMM 配置资产旁的稳定清单文件名。
const MANAGED_RUNTIME_MANIFEST_FILE: &str = "vmm-runtime-manifest.json";

/// Version of the persistent embedding-space compatibility marker.
/// 持久化向量空间兼容标记的版本。
const MANAGED_EMBEDDING_SPACE_STATE_VERSION: u32 = 1;

/// Stable file name for the persistent embedding-space compatibility marker.
/// 持久化向量空间兼容标记的稳定文件名。
const MANAGED_EMBEDDING_SPACE_STATE_FILE: &str = "embedding-space-v1.json";

/// Persists the exact vector-space identity that owns the managed VMM data root.
/// 持久化拥有托管 VMM 数据根的精确向量空间身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManagedEmbeddingSpaceState {
    /// Exact marker contract version.
    /// 精确标记契约版本。
    version: u32,
    /// Exact provider-neutral embedding-space fingerprint.
    /// 精确且供应商无关的向量空间指纹。
    fingerprint: EmbeddingSpaceFingerprint,
}

/// Exact VMM inference capability preflight evaluated before the child starts.
/// 在子进程启动前评估的准确 VMM 推理能力预检。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedVmmCapabilityPreflight {
    /// Exact vector dimension required by the managed data schema.
    /// 托管数据 Schema 所需的准确向量维度。
    pub(crate) embedding_dimension: u32,
    /// Exact embedding-space identity that must remain compatible with persisted vectors.
    /// 必须与持久化向量保持兼容的精确向量空间身份。
    pub(crate) embedding_fingerprint: Option<EmbeddingSpaceFingerprint>,
    /// Whether both PreCheck LLM routes are configured.
    /// 是否已配置两个 PreCheck LLM 路由。
    pub(crate) precheck_ready: bool,
    /// Controlled PreCheck degradation reason.
    /// 受控的 PreCheck 降级原因。
    pub(crate) precheck_error: Option<String>,
    /// Whether both PostAction LLM routes are configured.
    /// 是否已配置两个 PostAction LLM 路由。
    pub(crate) postaction_ready: bool,
    /// Controlled PostAction degradation reason.
    /// 受控的 PostAction 降级原因。
    pub(crate) postaction_error: Option<String>,
    /// Whether the manual profile-instruction route is configured.
    /// 是否已配置手工画像指令路由。
    pub(crate) profile_instruction_ready: bool,
    /// Controlled profile-instruction degradation reason.
    /// 受控的画像指令降级原因。
    pub(crate) profile_instruction_error: Option<String>,
    /// Whether a rerank route is configured.
    /// 是否已配置重排路由。
    pub(crate) rerank_ready: bool,
    /// Controlled rerank degradation reason.
    /// 受控的重排降级原因。
    pub(crate) rerank_error: Option<String>,
}

/// Process-local status exposed to the service state and settings projection.
/// 暴露给服务状态与设置投影的进程内状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedMemoryMeshSnapshot {
    /// Structured managed data-space safety state.
    /// 结构化托管数据空间安全状态。
    pub(crate) data_space_state: vulcan_service_api::VmmDataSpaceState,
    /// Stable lifecycle phase.
    /// 稳定生命周期阶段。
    pub(crate) phase: String,
    /// Actual authenticated gRPC target when ready.
    /// 就绪时的实际鉴权 gRPC 目标。
    pub(crate) grpc_target: Option<String>,
    /// Non-secret Controller endpoint.
    /// 不含秘密的 Controller 端点。
    pub(crate) controller_endpoint: String,
    /// Stable isolated Controller space.
    /// 稳定隔离的 Controller Space。
    pub(crate) controller_space_id: String,
    /// Stable managed data root.
    /// 稳定托管数据根。
    pub(crate) data_root: PathBuf,
    /// Stable managed runtime log path.
    /// 稳定托管运行时日志路径。
    pub(crate) log_path: PathBuf,
    /// Last controlled lifecycle failure.
    /// 最近一次受控生命周期失败。
    pub(crate) last_error: Option<String>,
    /// Whether the exact configured numeric user and project mappings resolve.
    /// 准确配置的数字用户与项目映射是否可解析。
    pub(crate) scope_ready: bool,
    /// Controlled scope reconciliation diagnostic.
    /// 受控的作用域对账诊断。
    pub(crate) scope_error: Option<String>,
    /// Exact inference capability state retained with this runtime generation.
    /// 与当前运行时代次一同保留的准确推理能力状态。
    pub(crate) capabilities: ManagedVmmCapabilityPreflight,
}

/// Process-local lifecycle projection retained even when child startup fails.
/// 即使子进程启动失败也会保留的进程内生命周期投影。
static MANAGED_MEMORY_MESH_SNAPSHOT: OnceLock<RwLock<Option<ManagedMemoryMeshSnapshot>>> =
    OnceLock::new();

/// Monotonic process-local manifest generation.
/// 进程内单调递增的清单代次。
static MANAGED_MEMORY_MESH_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Owns one child process, its ephemeral secrets, and its generated runtime files.
/// 持有一个子进程、其临时秘密及生成的运行期文件。
pub(crate) struct ManagedMemoryMeshRuntime {
    child: Option<Child>,
    #[cfg(windows)]
    child_job: Option<vulcan_process_lifecycle::WindowsKillOnCloseJob>,
    manifest_path: PathBuf,
    status_path: PathBuf,
    shutdown_path: PathBuf,
    access_token: String,
    management_listen_addr: String,
    management_access_token: String,
    snapshot: ManagedMemoryMeshSnapshot,
}

/// Secret-bearing target copied only for one managed VMM heartbeat request.
/// 仅为一次托管 VMM 心跳请求复制的含密目标。
pub(crate) struct ManagedMemoryMeshHeartbeatTarget {
    /// Actual loopback gRPC target published by the managed child.
    /// 托管子进程发布的实际回环 gRPC 目标。
    pub(crate) grpc_target: String,
    /// Ephemeral bearer token owned by the current child generation.
    /// 当前子进程代次持有的临时 Bearer 令牌。
    pub(crate) access_token: String,
}

/// Secret-bearing target copied only for one backend management request.
/// 仅为一次后端管理请求复制的含密目标。
pub(crate) struct ManagedMemoryMeshManagementTarget {
    /// Absolute management API origin for the managed or external service.
    /// 托管或外部服务的管理 API 绝对源地址。
    pub(crate) base_url: String,
    /// Ephemeral management Bearer token owned by the current child generation.
    /// 当前子进程代次持有的临时管理 Bearer Token。
    pub(crate) access_token: String,
    /// Request timeout selected by the owning backend configuration.
    /// 持有方后端配置选择的请求超时。
    pub(crate) timeout: std::time::Duration,
}

/// Prepared host-owned inputs for one isolated managed VMM maintenance run.
/// 一次隔离托管 VMM 维护运行所需的宿主持有输入。
#[derive(Debug, Clone)]
pub(crate) struct ManagedVmmMaintenancePreparation {
    /// Exact maintenance binary adjacent to the configured runtime.
    /// 与已配置运行时相邻的准确维护二进制。
    pub(crate) migrate_binary_path: PathBuf,
    /// Ephemeral strict managed manifest consumed by `vmm-migrate`.
    /// 由 `vmm-migrate` 消费的临时严格托管清单。
    pub(crate) manifest_path: PathBuf,
    /// Stable managed VMM data root.
    /// 稳定托管 VMM 数据根。
    pub(crate) data_root: PathBuf,
    /// Exact vector-space identity that must be persisted only after rebuild succeeds.
    /// 仅在重建成功后才能持久化的准确向量空间身份。
    pub(crate) embedding_fingerprint: EmbeddingSpaceFingerprint,
}

impl ManagedMemoryMeshRuntime {
    /// Starts one managed VMM child and returns only after authenticated Healthz succeeds.
    /// 启动一个托管 VMM 子进程，并仅在鉴权 Healthz 成功后返回。
    ///
    /// # Parameters
    /// 参数
    /// `binary_path` is the explicit managed VMM executable, `controller_endpoint` is the
    /// already-running shared Controller, `vmm_config` is the persisted transport/behavior snapshot,
    /// and `capabilities` is the exact inference preflight.
    /// `binary_path` 是显式托管 VMM 可执行文件，`controller_endpoint` 是已运行的共享 Controller，
    /// `vmm_config` 是持久化传输/行为快照，`capabilities` 是准确推理能力预检。
    ///
    /// # Returns
    /// 返回值
    /// A live child-process owner after manifest, readiness, identity, and Healthz validation.
    /// 在清单、就绪、身份与 Healthz 校验完成后返回存活的子进程所有者。
    pub(crate) async fn start(
        binary_path: &Path,
        controller_endpoint: &str,
        vmm_config: &vulcan_vmm::VmmConfigLayer,
        capabilities: &ManagedVmmCapabilityPreflight,
    ) -> Result<Self, String> {
        let binary_path = validate_managed_vmm_package(binary_path)?;
        let discovery_path =
            inference_service_discovery_snapshot_path(dirs::home_dir(), std::env::temp_dir());
        let discovery = load_inference_discovery(&discovery_path)?;
        let parent_started_at = discovery
            .started_at_unix_ms
            .parse::<u64>()
            .map_err(|error| format!("VMM 推理发现文件包含无效宿主启动身份：{error}"))?;
        let application_root = vulcan_workspace_store::data_plane::application_data_root();
        let managed_root = application_root.join("vmm");
        let runtime_root = managed_root.join("runtime");
        let data_root = managed_root.join("data");
        fs::create_dir_all(&runtime_root)
            .map_err(|error| format!("创建 VMM 托管运行目录失败：{error}"))?;
        fs::create_dir_all(&data_root)
            .map_err(|error| format!("创建 VMM 托管数据目录失败：{error}"))?;
        let embedding_fingerprint = capabilities
            .embedding_fingerprint
            .as_ref()
            .ok_or_else(|| String::from("VMM 向量空间预检未产生精确指纹。"))?;
        let data_space_state = inspect_managed_embedding_space(&data_root, embedding_fingerprint)?;
        if let Some(error) = managed_embedding_space_blocker(data_space_state) {
            publish_data_space_failed_snapshot(
                controller_endpoint,
                data_space_state,
                error.clone(),
                capabilities.clone(),
            );
            return Err(error);
        }
        // Persist ownership before the child can initialize storage. A failed first startup must
        // not leave an unmarked non-empty root that permanently blocks every subsequent retry.
        // 在子进程可能初始化存储前持久化归属。首次启动失败不得留下无标记的非空目录，
        // 从而永久阻断之后的每一次重试。
        persist_managed_embedding_space(&data_root, embedding_fingerprint)?;
        let manifest_path = runtime_root.join("managed-config.json");
        let status_path = runtime_root.join("managed-status.json");
        let shutdown_path = runtime_root.join("managed-shutdown");
        remove_runtime_file(&manifest_path)?;
        remove_runtime_file(&status_path)?;
        remove_runtime_file(&shutdown_path)?;

        let instance_id = Uuid::new_v4().to_string();
        let generation = next_generation(parent_started_at);
        let access_token = format!(
            "{}{}",
            Uuid::new_v4().as_simple(),
            Uuid::new_v4().as_simple()
        );
        let management_access_token = format!(
            "{}{}",
            Uuid::new_v4().as_simple(),
            Uuid::new_v4().as_simple()
        );
        let system_root = binary_path
            .parent()
            .and_then(Path::parent)
            .map(|root| root.join("configs"))
            .ok_or_else(|| String::from("无法从 VMM 可执行文件解析打包资源目录。"))?;
        let preferred_listen_addr = preferred_vmm_address(&discovery)?;
        let manifest = default_managed_manifest(
            &instance_id,
            generation,
            parent_started_at,
            &system_root,
            &data_root,
            &status_path,
            &shutdown_path,
            &preferred_listen_addr,
            &access_token,
            &management_access_token,
            controller_endpoint,
            &discovery_path,
            vmm_config,
            capabilities.embedding_dimension,
        )?;
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|error| format!("序列化 VMM 托管清单失败：{error}"))?;
        let manifest_digest = format!("{:x}", Sha256::digest(&manifest_bytes));
        atomic_write_private(&manifest_path, &manifest_bytes)?;
        let log_path = managed_root.join("logs").join("vmm-managed.log");
        if let Some(parent) = log_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("创建 VMM 日志目录失败：{error}"))?;
        }
        let stdout = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|error| format!("打开 VMM 日志文件失败：{error}"))?;
        let stderr = stdout
            .try_clone()
            .map_err(|error| format!("复制 VMM 日志句柄失败：{error}"))?;
        let mut command = Command::new(&binary_path);
        command
            .arg("--vulcan-managed-config")
            .arg(&manifest_path)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        configure_hidden_child(&mut command);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                cleanup_ephemeral_runtime_files(&manifest_path, &status_path, &shutdown_path);
                return Err(format!("启动 VMM 托管子进程失败：{error}"));
            }
        };
        #[cfg(windows)]
        let child_job = match vulcan_process_lifecycle::WindowsKillOnCloseJob::attach(&child) {
            Ok(job) => job,
            Err(error) => {
                stop_child(&mut child);
                cleanup_ephemeral_runtime_files(&manifest_path, &status_path, &shutdown_path);
                return Err(format!(
                    "绑定 VMM 子进程到 Windows Job Object 失败：{error}"
                ));
            }
        };
        let child_id = child.id();
        let status = match wait_for_status(
            &mut child,
            &status_path,
            &instance_id,
            generation,
            child_id,
            std::process::id(),
            &manifest_digest,
        )
        .await
        {
            Ok(status) => status,
            Err(error) => {
                stop_child(&mut child);
                cleanup_ephemeral_runtime_files(&manifest_path, &status_path, &shutdown_path);
                return Err(error);
            }
        };
        if let Err(error) = health_check(&status.grpc_listen_addr, &access_token, vmm_config).await
        {
            stop_child(&mut child);
            cleanup_ephemeral_runtime_files(&manifest_path, &status_path, &shutdown_path);
            return Err(error);
        }
        if let Err(error) =
            management_health_check(&status.management_listen_addr, &management_access_token).await
        {
            stop_child(&mut child);
            cleanup_ephemeral_runtime_files(&manifest_path, &status_path, &shutdown_path);
            return Err(error);
        }
        let scope_error =
            reconcile_configured_scope(&status.grpc_listen_addr, &access_token, vmm_config)
                .await
                .err();
        Ok(Self {
            child: Some(child),
            #[cfg(windows)]
            child_job: Some(child_job),
            manifest_path,
            status_path,
            shutdown_path,
            access_token,
            management_listen_addr: status.management_listen_addr,
            management_access_token,
            snapshot: ManagedMemoryMeshSnapshot {
                data_space_state: vulcan_service_api::VmmDataSpaceState::Ready,
                phase: String::from("ready"),
                grpc_target: Some(status.grpc_listen_addr),
                controller_endpoint: controller_endpoint.to_string(),
                controller_space_id: String::from(MANAGED_VMM_SPACE_ID),
                data_root,
                log_path,
                last_error: None,
                scope_ready: scope_error.is_none(),
                scope_error,
                capabilities: capabilities.clone(),
            },
        })
    }

    /// Returns the current non-secret lifecycle snapshot.
    /// 返回当前不含秘密的生命周期快照。
    pub(crate) fn snapshot(&self) -> ManagedMemoryMeshSnapshot {
        self.snapshot.clone()
    }

    /// Refreshes child liveness and converts an unexpected exit into an explicit crashed snapshot.
    /// 刷新子进程存活状态，并把意外退出转换成明确的 crashed 快照。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` when the child has exited or its status cannot be queried.
    /// 当子进程已经退出或无法查询状态时返回 `true`。
    pub(crate) fn refresh_liveness(&mut self) -> bool {
        let Some(child) = self.child.as_mut() else {
            return matches!(self.snapshot.phase.as_str(), "crashed" | "waiting_retry");
        };
        match child.try_wait() {
            Ok(None) => false,
            Ok(Some(status)) => {
                self.child = None;
                #[cfg(windows)]
                {
                    self.child_job = None;
                }
                self.snapshot.phase = String::from("crashed");
                self.snapshot.grpc_target = None;
                self.snapshot.last_error = Some(format!("VMM 子进程意外退出：{status}"));
                publish_snapshot(self.snapshot.clone());
                true
            }
            Err(error) => {
                self.snapshot.phase = String::from("crashed");
                self.snapshot.grpc_target = None;
                self.snapshot.last_error = Some(format!("查询 VMM 子进程状态失败：{error}"));
                publish_snapshot(self.snapshot.clone());
                true
            }
        }
    }

    /// Returns the exact secret-bearing target for one background heartbeat.
    /// 返回一次后台心跳使用的准确含密目标。
    ///
    /// # Returns
    /// 返回值
    /// A target only while the owned child can still answer heartbeat requests.
    /// 仅当持有的子进程仍可响应心跳请求时返回目标。
    pub(crate) fn heartbeat_target(&self) -> Option<ManagedMemoryMeshHeartbeatTarget> {
        if self.child.is_none() || !matches!(self.snapshot.phase.as_str(), "ready" | "unhealthy") {
            return None;
        }
        Some(ManagedMemoryMeshHeartbeatTarget {
            grpc_target: self.snapshot.grpc_target.clone()?,
            access_token: self.access_token.clone(),
        })
    }

    /// Returns the exact secret-bearing target for one backend management request.
    /// 返回一次后端管理请求使用的准确含密目标。
    pub(crate) fn management_target(&self) -> Option<ManagedMemoryMeshManagementTarget> {
        if self.child.is_none() || !matches!(self.snapshot.phase.as_str(), "ready" | "unhealthy") {
            return None;
        }
        Some(ManagedMemoryMeshManagementTarget {
            base_url: format!("http://{}", self.management_listen_addr),
            access_token: self.management_access_token.clone(),
            timeout: std::time::Duration::from_secs(30),
        })
    }

    /// Applies one authenticated heartbeat result to the retained lifecycle snapshot.
    /// 将一次鉴权心跳结果应用到保留的生命周期快照。
    ///
    /// # Parameters
    /// 参数
    /// `result` is the exact Healthz result returned for this owned child generation.
    /// `result` 是当前持有子进程代次返回的准确 Healthz 结果。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the user-facing lifecycle snapshot changed.
    /// 当面向用户的生命周期快照发生变化时返回 `true`。
    pub(crate) fn record_heartbeat_result(&mut self, result: Result<(), String>) -> bool {
        if self.child.is_none() || !matches!(self.snapshot.phase.as_str(), "ready" | "unhealthy") {
            return false;
        }
        if !apply_managed_heartbeat_result(&mut self.snapshot, result) {
            return false;
        }
        publish_snapshot(self.snapshot.clone());
        true
    }

    /// Records one controlled automatic-restart failure on the supervised generation.
    /// 在受监督代次上记录一次受控自动重启失败。
    ///
    /// # Parameters
    /// 参数
    /// `error` is the actionable failure retained for WebUI diagnostics.
    /// `error` 是为 WebUI 诊断保留的可操作失败原因。
    pub(crate) fn record_restart_failure(&mut self, error: String) {
        self.snapshot.phase = String::from("waiting_retry");
        self.snapshot.grpc_target = None;
        self.snapshot.last_error = Some(error);
        publish_snapshot(self.snapshot.clone());
    }

    /// Returns the in-memory authorization value expected by the existing VMM transport.
    /// 返回现有 VMM 传输层期望的内存鉴权值。
    pub(crate) fn authorization_value(&self) -> String {
        format!("Bearer {}", self.access_token)
    }

    /// Stops the owned child and removes only ephemeral runtime files.
    /// 停止持有的子进程，并仅删除临时运行期文件。
    pub(crate) fn stop(&mut self) -> Result<(), String> {
        self.snapshot.phase = String::from("stopping");
        if let Some(mut child) = self.child.take() {
            request_graceful_stop(&self.shutdown_path, &mut child);
        }
        #[cfg(windows)]
        {
            self.child_job = None;
        }
        let cleanup_errors = [
            remove_runtime_file(&self.manifest_path),
            remove_runtime_file(&self.status_path),
            remove_runtime_file(&self.shutdown_path),
        ]
        .into_iter()
        .filter_map(Result::err)
        .collect::<Vec<_>>();
        self.snapshot.phase = String::from("stopped");
        self.snapshot.grpc_target = None;
        self.snapshot.last_error = None;
        self.access_token.clear();
        self.management_access_token.clear();
        self.management_listen_addr.clear();
        publish_snapshot(self.snapshot.clone());
        if cleanup_errors.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "清理 VMM 托管运行文件失败：{}",
                cleanup_errors.join("; ")
            ))
        }
    }
}

/// Applies one heartbeat outcome to a live managed lifecycle snapshot.
/// 将一次心跳结果应用到存活的托管生命周期快照。
///
/// # Parameters
/// 参数
/// `snapshot` is the exact lifecycle state owned by the current child generation.
/// `snapshot` 是当前子进程代次持有的准确生命周期状态。
/// `result` is the authenticated Healthz outcome.
/// `result` 是鉴权 Healthz 结果。
///
/// # Returns
/// 返回值
/// `true` when the snapshot changed.
/// 当快照发生变化时返回 `true`。
fn apply_managed_heartbeat_result(
    snapshot: &mut ManagedMemoryMeshSnapshot,
    result: Result<(), String>,
) -> bool {
    let previous = snapshot.clone();
    match result {
        Ok(()) => {
            snapshot.phase = String::from("ready");
            snapshot.last_error = None;
        }
        Err(error) => {
            snapshot.phase = String::from("unhealthy");
            snapshot.last_error = Some(format!("VMM 托管心跳失败：{error}"));
        }
    }
    snapshot != &previous
}

/// Prepares one strict managed manifest for the isolated VMM maintenance companion.
/// 为隔离的 VMM 维护配套程序准备一份严格托管清单。
///
/// # Parameters
/// 参数
/// `binary_path` is the exact packaged `vmm-local` path whose sibling maintenance binary is used.
/// `binary_path` 是准确的打包 `vmm-local` 路径，其同级维护二进制会被使用。
/// `controller_endpoint` is the already-running shared VLDB Controller endpoint.
/// `controller_endpoint` 是已经运行的共享 VLDB Controller 端点。
/// `vmm_config` is the host-authoritative runtime behavior snapshot.
/// `vmm_config` 是宿主权威的运行时行为快照。
/// `capabilities` contains the exact embedding dimension and fingerprint.
/// `capabilities` 包含准确的向量维度与指纹。
///
/// # Returns
/// 返回值
/// Prepared paths and identity without modifying the historical data marker.
/// 返回准备好的路径与身份，且不修改历史数据标记。
pub(crate) fn prepare_managed_vmm_maintenance(
    binary_path: &Path,
    controller_endpoint: &str,
    vmm_config: &vulcan_vmm::VmmConfigLayer,
    capabilities: &ManagedVmmCapabilityPreflight,
) -> Result<ManagedVmmMaintenancePreparation, String> {
    let binary_path = validate_managed_vmm_package(binary_path)?;
    let migrate_binary_path = binary_path.with_file_name(if cfg!(windows) {
        "vmm-migrate.exe"
    } else {
        "vmm-migrate"
    });
    if !migrate_binary_path.is_file() {
        return Err(format!(
            "VMM 维护程序不存在：{}",
            migrate_binary_path.display()
        ));
    }
    let embedding_fingerprint = capabilities
        .embedding_fingerprint
        .clone()
        .ok_or_else(|| String::from("VMM 向量空间预检未产生精确指纹。"))?;
    let discovery_path =
        inference_service_discovery_snapshot_path(dirs::home_dir(), std::env::temp_dir());
    let discovery = load_inference_discovery(&discovery_path)?;
    let parent_started_at = discovery
        .started_at_unix_ms
        .parse::<u64>()
        .map_err(|error| format!("VMM 推理发现文件包含无效宿主启动身份：{error}"))?;
    let application_root = vulcan_workspace_store::data_plane::application_data_root();
    let managed_root = application_root.join("vmm");
    let runtime_root = managed_root.join("runtime");
    let data_root = managed_root.join("data");
    fs::create_dir_all(&runtime_root)
        .map_err(|error| format!("创建 VMM 托管运行目录失败：{error}"))?;
    fs::create_dir_all(&data_root)
        .map_err(|error| format!("创建 VMM 托管数据目录失败：{error}"))?;
    let manifest_path = runtime_root.join("managed-maintenance-config.json");
    let status_path = runtime_root.join("managed-maintenance-status.json");
    let shutdown_path = runtime_root.join("managed-maintenance-shutdown");
    remove_runtime_file(&manifest_path)?;
    remove_runtime_file(&status_path)?;
    remove_runtime_file(&shutdown_path)?;
    let system_root = binary_path
        .parent()
        .and_then(Path::parent)
        .map(|root| root.join("configs"))
        .ok_or_else(|| String::from("无法从 VMM 可执行文件解析打包资源目录。"))?;
    let preferred_listen_addr = preferred_vmm_address(&discovery)?;
    let instance_id = Uuid::new_v4().to_string();
    let generation = next_generation(parent_started_at);
    let access_token = format!(
        "{}{}",
        Uuid::new_v4().as_simple(),
        Uuid::new_v4().as_simple()
    );
    let management_access_token = format!(
        "{}{}",
        Uuid::new_v4().as_simple(),
        Uuid::new_v4().as_simple()
    );
    let manifest = default_managed_manifest(
        &instance_id,
        generation,
        parent_started_at,
        &system_root,
        &data_root,
        &status_path,
        &shutdown_path,
        &preferred_listen_addr,
        &access_token,
        &management_access_token,
        controller_endpoint,
        &discovery_path,
        vmm_config,
        capabilities.embedding_dimension,
    )?;
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("序列化 VMM 托管维护清单失败：{error}"))?;
    atomic_write_private(&manifest_path, &manifest_bytes)?;
    Ok(ManagedVmmMaintenancePreparation {
        migrate_binary_path,
        manifest_path,
        data_root,
        embedding_fingerprint,
    })
}

/// Finalizes a successful managed rebuild by atomically publishing its exact vector identity.
/// 通过原子发布准确向量身份完成一次成功的托管重建。
pub(crate) fn finalize_managed_vmm_rebuild(
    data_root: &Path,
    fingerprint: &EmbeddingSpaceFingerprint,
) -> Result<(), String> {
    persist_managed_embedding_space(data_root, fingerprint)
}

/// Removes the ephemeral managed maintenance manifest after a terminal outcome.
/// 在维护进入终态后删除临时托管维护清单。
pub(crate) fn cleanup_managed_vmm_maintenance_manifest(path: &Path) {
    let _ = remove_runtime_file(path);
}

/// Evaluates exact VMM embedding and foundational LLM requirements plus optional capabilities.
/// 评估准确的 VMM 向量与基础 LLM 要求，以及按用途划分的可选能力。
///
/// # Parameters
/// 参数
/// `configuration` is the authoritative unified AI configuration.
/// `configuration` 是权威统一 AI 配置。
/// `catalogs` contains the exact connection-scoped model capability snapshots.
/// `catalogs` 包含精确的连接作用域模型能力快照。
/// `optional_routes` contains runtime-validated degradation facts for optional capabilities.
/// `optional_routes` 包含可选能力经运行时校验后的降级事实。
///
/// # Returns
/// 返回值
/// A complete capability preflight, or a controlled required-foundation error.
/// 返回完整能力预检，或返回受控的必需基础能力错误。
pub(crate) fn configured_vmm_capability_preflight(
    configuration: &PersistedAiConfiguration,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
    optional_routes: &crate::configured_inference::VmmOptionalInferenceRouteValidation,
) -> Result<ManagedVmmCapabilityPreflight, String> {
    let profile = configuration
        .inference_profiles
        .get("vmm")
        .ok_or_else(|| String::from("VMM 内部推理配置档不存在。"))?;
    let route = profile
        .embedding
        .as_ref()
        .ok_or_else(|| String::from("VMM 内部推理未配置向量模型。"))?;
    let connection = configuration
        .connection_graph
        .connections
        .get(route.connection_id.as_str())
        .ok_or_else(|| {
            format!(
                "VMM 向量路由引用了不存在的账号连接 {}。",
                route.connection_id
            )
        })?;
    if connection.provider_id != route.provider_id
        || connection.service_entry_id != route.service_entry_id
    {
        return Err(format!(
            "VMM 向量路由 {}/{} 与账号连接 {} 的供应商入口不一致。",
            route.provider_id, route.service_entry_id, route.connection_id
        ));
    }
    let model = catalogs
        .get(route.connection_id.as_str())
        .and_then(|catalog| {
            catalog
                .models
                .iter()
                .find(|model| model.id == route.model_id)
        })
        .ok_or_else(|| {
            format!(
                "VMM 向量模型 {} 不存在于账号连接 {} 的精确目录中。",
                route.model_id, route.connection_id
            )
        })?;
    let embedding_capabilities = model
        .capabilities
        .embedding
        .as_ref()
        .ok_or_else(|| format!("VMM 向量模型 {} 未声明向量能力契约。", route.model_id))?;
    let key = PersistedAiConfiguration::internal_model_operation_key(
        route.provider_id.as_str(),
        route.model_id.as_str(),
        AiOperation::EmbeddingCreate,
    );
    let overrides = configuration.internal_model_overrides.get(key.as_str());
    let embedding_dimension = configuration
        .internal_model_overrides
        .get(key.as_str())
        .and_then(|overrides| overrides.embedding_dimensions)
        .filter(|dimension| *dimension > 0)
        .or(embedding_capabilities.default_dimensions)
        .filter(|dimension| *dimension > 0)
        .ok_or_else(|| {
            format!(
                "VMM 向量模型 {} 的精确连接目录未声明默认维度，请在内部推理参数中设置维度。",
                route.model_id
            )
        })?;
    let embedding_task = overrides
        .and_then(|overrides| overrides.embedding_task)
        .unwrap_or(EmbeddingTask::ProviderDefault);
    let embedding_output_kind = overrides
        .and_then(|overrides| overrides.embedding_output_kind)
        .unwrap_or(EmbeddingOutputKind::Dense);
    let embedding_encoding = overrides
        .and_then(|overrides| overrides.embedding_encoding)
        .unwrap_or(EmbeddingEncoding::Float);
    let compatibility_request = EmbeddingRequest {
        contract_version: INFERENCE_CONTRACT_VERSION,
        request_id: String::from("vmm-managed-embedding-preflight"),
        idempotency_key: None,
        consumer_profile_id: String::from("vmm"),
        inputs: vec![EmbeddingInput {
            input_id: String::from("compatibility-probe"),
            content: EmbeddingInputContent::Text {
                text: String::from("compatibility"),
            },
        }],
        task: embedding_task,
        dimensions: Some(embedding_dimension),
        output_kind: embedding_output_kind,
        encoding: embedding_encoding,
        invalid_input_policy: EmbeddingInvalidInputPolicy::FailBatch,
    };
    compatibility_request
        .validate()
        .and_then(|()| embedding_capabilities.validate_request(&compatibility_request))
        .map_err(|error| format!("VMM 向量空间能力预检失败：{error}"))?;
    let binding_id = match &route.binding {
        InferenceRouteBinding::Action { action_binding_id } => action_binding_id.clone(),
        InferenceRouteBinding::Protocol { .. } => {
            return Err(String::from("VMM 向量路由必须使用动作绑定。"));
        }
    };
    let secondary_route_key = PersistedAiConfiguration::connection_model_key(
        route.connection_id.as_str(),
        route.model_id.as_str(),
    );
    let secondary_route_id = configuration
        .secondary_routes
        .get(secondary_route_key.as_str())
        .and_then(|selection| match selection {
            SecondaryRouteSelection::Automatic => None,
            SecondaryRouteSelection::PinnedEndpoint { route_id, .. } => Some(route_id.clone()),
        });
    let embedding_fingerprint = EmbeddingSpaceFingerprint {
        provider_id: route.provider_id.clone(),
        service_entry_id: route.service_entry_id.clone(),
        deployment_id: Some(connection.credential_id.clone()),
        secondary_route_id,
        model_id: route.model_id.clone(),
        binding_id,
        capability_revision: model.capabilities.evidence_revision.clone(),
        task: embedding_task,
        dimensions: embedding_dimension,
        output_kind: embedding_output_kind,
        encoding: embedding_encoding,
        normalization: embedding_capabilities.normalization,
        distance_metric: embedding_capabilities.distance_metric,
    };
    let capability = |purposes: &[&str], label: &str| {
        let missing = purposes
            .iter()
            .copied()
            .filter(|purpose| !profile.llm_routes.contains_key(*purpose))
            .collect::<Vec<_>>();
        if missing.is_empty() {
            (true, None)
        } else {
            (
                false,
                Some(format!(
                    "{label} 缺少内部推理用途：{}。",
                    missing.join("、")
                )),
            )
        }
    };
    let (precheck_ready, precheck_error) = capability(&["precheck_l1", "precheck_l2"], "PreCheck");
    let (postaction_ready, postaction_error) =
        capability(&["postaction_l1", "postaction_l2"], "PostAction");
    // Foundational recall and writeback models must exist before a fresh memory space is started.
    // 全新记忆空间启动前必须已经具备基础召回与写回模型。
    if let Some(error) = precheck_error.as_ref() {
        return Err(format!("VMM 初始化前必须配置基础 LLM：{error}"));
    }
    if let Some(error) = postaction_error.as_ref() {
        return Err(format!("VMM 初始化前必须配置基础 LLM：{error}"));
    }
    let profile_instruction_ready = optional_routes.profile_instruction_error.is_none();
    let rerank_ready = optional_routes.rerank_error.is_none();
    Ok(ManagedVmmCapabilityPreflight {
        embedding_dimension,
        embedding_fingerprint: Some(embedding_fingerprint),
        precheck_ready,
        precheck_error,
        postaction_ready,
        postaction_error,
        profile_instruction_ready,
        profile_instruction_error: optional_routes.profile_instruction_error.clone(),
        rerank_ready,
        rerank_error: optional_routes.rerank_error.clone(),
    })
}

impl Drop for ManagedMemoryMeshRuntime {
    /// Prevents a normally dropped runtime owner from leaving the child process behind.
    /// 防止正常释放运行时所有者时遗留子进程。
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Reads the current non-secret managed runtime snapshot without exposing child ownership.
/// 读取当前不含秘密的托管运行时快照，且不暴露子进程所有权。
pub(crate) fn snapshot_from_state(
    state: &crate::SharedServiceBridgeState,
) -> Option<ManagedMemoryMeshSnapshot> {
    let runtime = state
        .lock()
        .ok()?
        .managed_memory_mesh_runtime
        .as_ref()
        .map(std::sync::Arc::clone);
    if let Some(runtime) = runtime
        && let Ok(mut runtime) = runtime.lock()
    {
        if runtime.refresh_liveness() {
            crate::vmm_hooks::clear_managed_vmm_transport_overlay();
        }
        return Some(runtime.snapshot());
    }
    lifecycle_snapshot()
}

/// Returns the current secret-bearing management target for one backend-only HTTP request.
/// 为一次仅后端 HTTP 请求返回当前含密管理目标。
pub(crate) fn management_target_from_state(
    state: &crate::SharedServiceBridgeState,
) -> Option<ManagedMemoryMeshManagementTarget> {
    let managed_runtime = state
        .lock()
        .ok()?
        .managed_memory_mesh_runtime
        .as_ref()
        .map(std::sync::Arc::clone);
    if let Some(target) =
        managed_runtime.and_then(|runtime| runtime.lock().ok()?.management_target())
    {
        return Some(target);
    }
    let configuration = crate::provider_configuration_from_state(state).ok()?;
    external_management_target_from_layer(&configuration.vmm)
}

/// Builds one external management target only when the effective configuration is not managed locally.
/// 仅当最终配置不是本机托管模式时构建外部管理目标。
fn external_management_target_from_layer(
    layer: &vulcan_vmm::VmmConfigLayer,
) -> Option<ManagedMemoryMeshManagementTarget> {
    if layer
        .install
        .as_ref()
        .is_some_and(|install| install.managed_install_enabled)
    {
        return None;
    }
    let base_url = layer
        .management_base_url
        .as_deref()?
        .trim()
        .trim_end_matches('/')
        .to_string();
    let access_token = layer.management_bearer_token.as_deref()?.trim().to_string();
    if base_url.is_empty() || access_token.is_empty() {
        return None;
    }
    let parsed = reqwest::Url::parse(&base_url).ok()?;
    let loopback = parsed
        .host_str()
        .is_some_and(|host| matches!(host, "127.0.0.1" | "localhost" | "::1"));
    if parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || (parsed.scheme() != "https" && !(parsed.scheme() == "http" && loopback))
        || layer
            .management_timeout_ms
            .is_some_and(|value| !(1_000..=120_000).contains(&value))
    {
        return None;
    }
    Some(ManagedMemoryMeshManagementTarget {
        base_url,
        access_token,
        timeout: std::time::Duration::from_millis(layer.management_timeout_ms.unwrap_or(30_000)),
    })
}

/// Synchronizes one managed-runtime transition into every retained product-facing VMM state.
/// 将一次托管运行时转换同步到所有保留的面向产品 VMM 状态。
///
/// # Parameters
/// 参数
/// `state` is the canonical service state that owns both the runtime and workbench projection.
/// `state` 是同时持有运行时与工作台投影的规范服务状态。
///
/// # Returns
/// 返回值
/// Returns the updated workbench snapshot, or a lock error.
/// 返回更新后的工作台快照，或锁错误。
pub(crate) fn synchronize_workbench_runtime(
    state: &crate::SharedServiceBridgeState,
) -> Result<vulcan_service_api::WorkbenchSnapshot, String> {
    let snapshot = snapshot_from_state(state);
    let mut guard = state
        .lock()
        .map_err(|error| format!("锁定服务状态以同步 VMM 运行态失败：{error}"))?;
    // Complete workspace and provider inputs allow the canonical VMM snapshot builder to keep
    // lifecycle, health, prerequisites, capabilities, and diagnostics mutually consistent.
    // 完整的工作区与 Provider 输入允许规范 VMM 快照构建器保持生命周期、健康度、前置条件、能力和诊断彼此一致。
    let next_vmm_memory = match (
        snapshot.as_ref(),
        guard.workspace_snapshot.as_ref(),
        guard.agent_runtime.as_ref(),
    ) {
        (Some(snapshot), Some(workspace), Some(agent_runtime)) => {
            crate::vmm::refresh_vmm_memory_runtime_projection(
                &guard.workbench.vmm_memory,
                workspace,
                &agent_runtime.provider_configuration,
                snapshot,
            )
        }
        _ => {
            // Early bootstrap and external-mode states may not own all canonical inputs yet.
            // Preserve their established behavior while still projecting the exact runtime fields.
            // 启动早期与外部模式可能尚未持有全部规范输入；保留既有行为，同时仍投影准确的运行时字段。
            let mut vmm_memory = guard.workbench.vmm_memory.clone();
            apply_managed_snapshot_to_runtime(&mut vmm_memory.runtime, snapshot);
            vmm_memory
        }
    };
    if guard.workbench.vmm_memory != next_vmm_memory {
        guard.workbench.vmm_memory = next_vmm_memory;
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench.revision = guard.workbench_generation;
    }
    Ok(guard.workbench.clone())
}

/// Projects one managed process snapshot into the retained WebUI runtime summary.
/// 将一份托管进程快照投影到保留的 WebUI 运行时摘要。
///
/// # Parameters
/// 参数
/// `runtime` is the mutable retained WebUI runtime summary.
/// `runtime` 是待更新的可变 WebUI 运行时摘要。
/// `snapshot` is the exact managed process snapshot, or `None` for external mode.
/// `snapshot` 是准确的托管进程快照；外部模式时为 `None`。
fn apply_managed_snapshot_to_runtime(
    runtime: &mut vulcan_service_api::VmmRuntimeSummary,
    snapshot: Option<ManagedMemoryMeshSnapshot>,
) {
    match snapshot {
        Some(snapshot) => {
            // Runtime capabilities are usable only while the managed child is actually ready.
            // 托管子进程真实就绪时，运行时能力才可被视为可用。
            let process_ready = snapshot.phase == "ready";
            runtime.data_space_state = snapshot.data_space_state;
            runtime.process_ready = process_ready;
            runtime.managed_phase = snapshot.phase;
            runtime.managed_grpc_target = snapshot.grpc_target;
            runtime.managed_controller_endpoint = snapshot.controller_endpoint;
            runtime.managed_controller_space = snapshot.controller_space_id;
            runtime.managed_data_root = snapshot.data_root.to_string_lossy().into_owned();
            runtime.managed_log_path = snapshot.log_path.to_string_lossy().into_owned();
            runtime.managed_last_error = snapshot.last_error;
            runtime.scope_ready = process_ready && snapshot.scope_ready;
            runtime.scope_error = snapshot.scope_error;
            runtime.precheck_ready = process_ready && snapshot.capabilities.precheck_ready;
            runtime.precheck_error = snapshot.capabilities.precheck_error;
            runtime.postaction_ready = process_ready && snapshot.capabilities.postaction_ready;
            runtime.postaction_error = snapshot.capabilities.postaction_error;
            runtime.profile_instruction_ready =
                process_ready && snapshot.capabilities.profile_instruction_ready;
            runtime.profile_instruction_error = snapshot.capabilities.profile_instruction_error;
            runtime.rerank_ready = process_ready && snapshot.capabilities.rerank_ready;
            runtime.rerank_error = snapshot.capabilities.rerank_error;
        }
        None => {
            runtime.process_ready = false;
            runtime.managed_phase = String::from("external");
            runtime.managed_grpc_target = None;
            runtime.managed_controller_endpoint.clear();
            runtime.managed_controller_space.clear();
            runtime.managed_data_root.clear();
            runtime.managed_log_path.clear();
            runtime.managed_last_error = None;
            runtime.scope_ready = false;
            runtime.scope_error = None;
            runtime.precheck_ready = false;
            runtime.precheck_error = None;
            runtime.postaction_ready = false;
            runtime.postaction_error = None;
            runtime.profile_instruction_ready = false;
            runtime.profile_instruction_error = None;
            runtime.rerank_ready = false;
            runtime.rerank_error = None;
        }
    }
}

/// Publishes one non-secret lifecycle snapshot for workbench diagnostics.
/// 发布一份不含秘密的生命周期快照供工作台诊断使用。
///
/// # Parameters
/// 参数
/// `snapshot` is the exact current managed-runtime lifecycle projection.
/// `snapshot` 是当前托管运行时的精确生命周期投影。
pub(crate) fn publish_snapshot(snapshot: ManagedMemoryMeshSnapshot) {
    if let Ok(mut slot) = MANAGED_MEMORY_MESH_SNAPSHOT
        .get_or_init(|| RwLock::new(None))
        .write()
    {
        *slot = Some(snapshot);
    }
}

/// Clears the process-local lifecycle projection when managed mode is disabled.
/// 在托管模式关闭时清除进程内生命周期投影。
pub(crate) fn clear_snapshot() {
    if let Ok(mut slot) = MANAGED_MEMORY_MESH_SNAPSHOT
        .get_or_init(|| RwLock::new(None))
        .write()
    {
        *slot = None;
    }
}

/// Marks the retained lifecycle projection as vector-space ready after official maintenance.
/// 在正式维护完成后把保留的生命周期投影标记为向量空间就绪。
pub(crate) fn publish_maintenance_data_space_ready() {
    let Some(mut snapshot) = lifecycle_snapshot() else {
        return;
    };
    snapshot.data_space_state = vulcan_service_api::VmmDataSpaceState::Ready;
    snapshot.phase = String::from("stopped");
    snapshot.grpc_target = None;
    snapshot.last_error = None;
    snapshot.scope_error = None;
    publish_snapshot(snapshot);
}

/// Builds and publishes a controlled failed lifecycle projection.
/// 构建并发布一份受控失败生命周期投影。
///
/// # Parameters
/// 参数
/// `controller_endpoint` is the known shared Controller endpoint and `error` is the actionable failure.
/// `controller_endpoint` 是已知共享 Controller 端点，`error` 是可操作失败原因。
pub(crate) fn publish_failed_snapshot(controller_endpoint: &str, error: impl Into<String>) {
    let error = error.into();
    let data_space_state = lifecycle_snapshot()
        .map(|snapshot| snapshot.data_space_state)
        .unwrap_or_default();
    publish_snapshot(ManagedMemoryMeshSnapshot {
        data_space_state,
        phase: String::from("failed"),
        grpc_target: None,
        controller_endpoint: controller_endpoint.to_string(),
        controller_space_id: String::from(MANAGED_VMM_SPACE_ID),
        data_root: managed_data_root(),
        log_path: managed_log_path(),
        last_error: Some(error.clone()),
        scope_ready: false,
        scope_error: Some(error.clone()),
        capabilities: unavailable_capabilities(error.as_str()),
    });
}

/// Publishes a failed startup snapshot while retaining an exact data-space blocker.
/// 发布启动失败快照，并保留准确的数据空间阻塞状态。
///
/// # Parameters
/// 参数
/// `controller_endpoint` is the non-secret controller endpoint.
/// `controller_endpoint` 是不含秘密的控制器端点。
/// `data_space_state` is the exact blocking inspection result.
/// `data_space_state` 是准确的阻塞检查结果。
/// `error` is the controlled user-facing failure.
/// `error` 是受控的用户可见失败信息。
/// `capabilities` is the preflight result already verified before data inspection.
/// `capabilities` 是数据检查前已经验证的能力预检结果。
///
/// # Returns
/// 返回值
/// This function returns nothing after publishing the process-local lifecycle state.
/// 发布进程内生命周期状态后无返回值。
fn publish_data_space_failed_snapshot(
    controller_endpoint: &str,
    data_space_state: vulcan_service_api::VmmDataSpaceState,
    error: String,
    capabilities: ManagedVmmCapabilityPreflight,
) {
    publish_snapshot(ManagedMemoryMeshSnapshot {
        data_space_state,
        phase: String::from("failed"),
        grpc_target: None,
        controller_endpoint: controller_endpoint.to_string(),
        controller_space_id: String::from(MANAGED_VMM_SPACE_ID),
        data_root: managed_data_root(),
        log_path: managed_log_path(),
        last_error: Some(error.clone()),
        scope_ready: false,
        scope_error: Some(error),
        capabilities,
    });
}

/// Builds a capability projection for startup failures that occurred before preflight publication.
/// 为能力预检发布前发生的启动失败构建能力投影。
fn unavailable_capabilities(reason: &str) -> ManagedVmmCapabilityPreflight {
    ManagedVmmCapabilityPreflight {
        embedding_dimension: 0,
        embedding_fingerprint: None,
        precheck_ready: false,
        precheck_error: Some(reason.to_string()),
        postaction_ready: false,
        postaction_error: Some(reason.to_string()),
        profile_instruction_ready: false,
        profile_instruction_error: Some(reason.to_string()),
        rerank_ready: false,
        rerank_error: Some(reason.to_string()),
    }
}

/// Returns the latest process-local lifecycle projection.
/// 返回最近一份进程内生命周期投影。
fn lifecycle_snapshot() -> Option<ManagedMemoryMeshSnapshot> {
    MANAGED_MEMORY_MESH_SNAPSHOT
        .get_or_init(|| RwLock::new(None))
        .read()
        .ok()
        .and_then(|slot| slot.clone())
}

/// Resolves the stable managed VMM data root from the authoritative application data root.
/// 从权威应用数据根解析稳定的托管 VMM 数据根。
fn managed_data_root() -> PathBuf {
    vulcan_workspace_store::data_plane::application_data_root()
        .join("vmm")
        .join("data")
}

/// Resolves the stable managed VMM log path from the authoritative application data root.
/// 从权威应用数据根解析稳定的托管 VMM 日志路径。
fn managed_log_path() -> PathBuf {
    vulcan_workspace_store::data_plane::application_data_root()
        .join("vmm")
        .join("logs")
        .join("vmm-managed.log")
}

/// Allocates a process-local generation that remains monotonic across controlled restarts.
/// 分配一个在受控重启期间保持单调递增的进程内代次。
///
/// # Parameters
/// 参数
/// `floor` is the current host start identity and prevents zero or stale initial generations.
/// `floor` 是当前宿主启动身份，用于避免零值或陈旧的初始代次。
///
/// # Returns
/// 返回值
/// Returns the newly allocated generation.
/// 返回新分配的代次。
fn next_generation(floor: u64) -> u64 {
    let floor = floor.max(1);
    loop {
        let current = MANAGED_MEMORY_MESH_GENERATION.load(Ordering::Acquire);
        let next = current.max(floor).saturating_add(1);
        if MANAGED_MEMORY_MESH_GENERATION
            .compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return next;
        }
    }
}

/// Inspects managed VMM data ownership without mutating files or hiding historical data.
/// 在不修改文件或隐藏历史数据的前提下检查托管 VMM 数据归属。
///
/// # Parameters
/// 参数
/// `data_root` is the stable managed data directory.
/// `data_root` 是稳定的托管数据目录。
/// `fingerprint` is the exact embedding-space identity requested for startup.
/// `fingerprint` 是启动请求的准确向量空间身份。
///
/// # Returns
/// 返回值
/// A structured safety state, or an I/O failure when the directory cannot be inspected.
/// 返回结构化安全状态；目录无法检查时返回 I/O 失败。
fn inspect_managed_embedding_space(
    data_root: &Path,
    fingerprint: &EmbeddingSpaceFingerprint,
) -> Result<vulcan_service_api::VmmDataSpaceState, String> {
    let state_path = data_root.join(MANAGED_EMBEDDING_SPACE_STATE_FILE);
    if !state_path.exists() {
        let contains_data = fs::read_dir(data_root)
            .map_err(|error| format!("检查 VMM 托管数据目录失败：{error}"))?
            .next()
            .transpose()
            .map_err(|error| format!("读取 VMM 托管数据目录项失败：{error}"))?
            .is_some();
        if contains_data {
            return Ok(vulcan_service_api::VmmDataSpaceState::UnmarkedHistory);
        }
        return Ok(vulcan_service_api::VmmDataSpaceState::Empty);
    }
    let bytes =
        fs::read(&state_path).map_err(|error| format!("读取 VMM 向量空间标记失败：{error}"))?;
    let state: ManagedEmbeddingSpaceState = match serde_json::from_slice(&bytes) {
        Ok(state) => state,
        Err(_) => return Ok(vulcan_service_api::VmmDataSpaceState::MarkerCorrupt),
    };
    if state.version != MANAGED_EMBEDDING_SPACE_STATE_VERSION {
        return Ok(vulcan_service_api::VmmDataSpaceState::MarkerVersionUnsupported);
    }
    if state.fingerprint != *fingerprint {
        return Ok(vulcan_service_api::VmmDataSpaceState::FingerprintChanged);
    }
    Ok(vulcan_service_api::VmmDataSpaceState::Initialized)
}

/// Verifies one managed data root against the exact embedding space in lifecycle tests.
/// 在生命周期测试中根据准确向量空间验证一个托管数据根。
#[cfg(test)]
fn verify_managed_embedding_space(
    data_root: &Path,
    fingerprint: &EmbeddingSpaceFingerprint,
) -> Result<(), String> {
    let state = inspect_managed_embedding_space(data_root, fingerprint)?;
    if let Some(error) = managed_embedding_space_blocker(state) {
        return Err(error);
    }
    Ok(())
}

/// Returns the controlled blocking message for one inspected data-space state.
/// 返回单个数据空间检查状态对应的受控阻塞信息。
///
/// # Parameters
/// 参数
/// `state` is the exact result produced by the read-only inspection.
/// `state` 是只读检查产生的准确结果。
///
/// # Returns
/// 返回值
/// `None` for safe startup states, otherwise one actionable blocker.
/// 安全启动状态返回 `None`，否则返回一条可操作阻塞信息。
fn managed_embedding_space_blocker(state: vulcan_service_api::VmmDataSpaceState) -> Option<String> {
    match state {
        vulcan_service_api::VmmDataSpaceState::Empty
        | vulcan_service_api::VmmDataSpaceState::Initialized
        | vulcan_service_api::VmmDataSpaceState::Ready => None,
        vulcan_service_api::VmmDataSpaceState::UnmarkedHistory => Some(String::from(
            "VMM 托管数据目录包含未确认的历史数据。请选择“稍后处理”或“备份并重建”；现有数据不会被自动覆盖。",
        )),
        vulcan_service_api::VmmDataSpaceState::FingerprintChanged => Some(String::from(
            "VMM 向量空间已变化，禁止复用历史索引。请选择备份并重建后再启动。",
        )),
        vulcan_service_api::VmmDataSpaceState::MarkerCorrupt => Some(String::from(
            "VMM 数据空间标记已损坏。请选择备份并重建；不会直接覆盖现有数据。",
        )),
        vulcan_service_api::VmmDataSpaceState::MarkerVersionUnsupported => Some(String::from(
            "VMM 数据空间标记版本不受支持。请先备份并重建，或升级到支持该版本的 VMM。",
        )),
        vulcan_service_api::VmmDataSpaceState::SchemaMigrationRequired => Some(String::from(
            "VMM 数据结构需要正式迁移，当前版本不会自动修改历史数据。",
        )),
        vulcan_service_api::VmmDataSpaceState::RebuildRunning => {
            Some(String::from("VMM 数据空间正在重建，请等待完成后再启动。"))
        }
        vulcan_service_api::VmmDataSpaceState::Unknown => {
            Some(String::from("VMM 数据空间状态尚未确认，当前不会启动。"))
        }
    }
}

/// Persists the exact embedding space only after authenticated managed startup succeeds.
/// 仅在经过鉴权的托管启动成功后持久化精确向量空间。
///
/// # Parameters
/// 参数
/// `data_root` is the stable managed VMM data root and `fingerprint` is the verified
/// embedding-space identity.
/// `data_root` 是稳定的托管 VMM 数据根，`fingerprint` 是已验证的向量空间身份。
///
/// # Returns
/// 返回值
/// Success after an atomic private write, or an actionable persistence failure.
/// 原子私有写入完成后返回成功，否则返回可操作的持久化失败。
fn persist_managed_embedding_space(
    data_root: &Path,
    fingerprint: &EmbeddingSpaceFingerprint,
) -> Result<(), String> {
    let state = ManagedEmbeddingSpaceState {
        version: MANAGED_EMBEDDING_SPACE_STATE_VERSION,
        fingerprint: fingerprint.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&state)
        .map_err(|error| format!("序列化 VMM 向量空间标记失败：{error}"))?;
    atomic_write_private(&data_root.join(MANAGED_EMBEDDING_SPACE_STATE_FILE), &bytes)
}

/// Builds the complete explicit VMM runtime snapshot from audited compiled defaults.
/// 基于已审计的编译期默认值构建完整显式 VMM 运行时快照。
#[allow(clippy::too_many_arguments)]
fn default_managed_manifest(
    instance_id: &str,
    generation: u64,
    parent_started_at: u64,
    system_root: &Path,
    data_root: &Path,
    status_path: &Path,
    shutdown_path: &Path,
    preferred_listen_addr: &str,
    access_token: &str,
    management_access_token: &str,
    controller_endpoint: &str,
    discovery_path: &Path,
    vmm_config: &vulcan_vmm::VmmConfigLayer,
    embedding_dimension: u32,
) -> Result<ManagedConfig, String> {
    let runtime_settings = vmm_config
        .install
        .as_ref()
        .map(|install| &install.runtime_settings)
        .ok_or_else(|| String::from("托管 VMM 配置缺少安装与运行时行为设置。"))?;
    let max_receive_message_bytes =
        u32::try_from(vmm_config.max_receive_message_bytes.unwrap_or(1 << 20))
            .map_err(|_| String::from("VMM 最大接收字节数超过托管协议支持的 u32 范围。"))?;
    let request_timeout_ms = vmm_config.receive_timeout_ms.unwrap_or(30_000);
    let request_timeout = milliseconds_as_duration(request_timeout_ms);
    Ok(ManagedConfig {
        contract_version: MANAGED_CONTRACT_VERSION,
        runtime_build_id: String::from(MANAGED_RUNTIME_BUILD_ID),
        owner: String::from(MANAGED_CONTRACT_OWNER),
        instance_id: instance_id.to_string(),
        generation,
        parent: ManagedParent {
            process_id: std::process::id(),
            started_at_unix_ms: parent_started_at,
        },
        assets: ManagedAssets {
            system_root: system_root.to_string_lossy().into_owned(),
            user_override_root: None,
        },
        runtime: ManagedRuntimeConfig {
            data_root: data_root.to_string_lossy().into_owned(),
            status_file: status_path.to_string_lossy().into_owned(),
            shutdown_file: shutdown_path.to_string_lossy().into_owned(),
            grpc: ManagedGrpcConfig {
                preferred_listen_addr: preferred_listen_addr.to_string(),
                allow_ephemeral_fallback: true,
                access_token: access_token.to_string(),
                max_receive_message_bytes,
                request_timeout: ManagedGrpcRequestTimeout {
                    workspace: request_timeout.clone(),
                    pre_check: milliseconds_as_duration(request_timeout_ms.min(30_000)),
                    post_action: request_timeout,
                },
                shutdown_timeout: String::from("10s"),
                keepalive: ManagedGrpcKeepalive {
                    enabled: true,
                    time: milliseconds_as_duration(
                        vmm_config.grpc_keepalive_time_ms.unwrap_or(30_000),
                    ),
                    timeout: milliseconds_as_duration(
                        vmm_config.grpc_keepalive_timeout_ms.unwrap_or(10_000),
                    ),
                    max_connection_idle: String::from("0s"),
                    max_connection_age: String::from("0s"),
                    max_connection_age_grace: String::from("0s"),
                    min_ping_interval: String::from("20s"),
                    permit_without_stream: vmm_config
                        .grpc_keepalive_permit_without_calls
                        .unwrap_or(true),
                },
            },
            management: ManagedManagementConfig {
                preferred_listen_addr: String::from("127.0.0.1:0"),
                allow_ephemeral_fallback: true,
                access_token: management_access_token.to_string(),
                max_request_body_bytes: 1 << 20,
                default_page_size: 30,
                max_page_size: 100,
                read_header_timeout: String::from("5s"),
                request_timeout: String::from("30s"),
                idle_timeout: String::from("1m"),
                shutdown_timeout: String::from("10s"),
            },
            storage: ManagedStorageConfig {
                mode: String::from("controller"),
                controller_endpoint: controller_endpoint.to_string(),
                controller_auto_spawn: false,
                controller_space_id: String::from(MANAGED_VMM_SPACE_ID),
                controller_space_label: String::from(MANAGED_VMM_SPACE_LABEL),
                controller_lease: ManagedControllerLease {
                    endpoint: controller_endpoint.to_string(),
                    auto_spawn: false,
                    executable: String::new(),
                    process_mode: String::from("managed"),
                    minimum_uptime: String::from("5m"),
                    idle_timeout: String::from("15m"),
                    lease_ttl: String::from("10s"),
                    connect_timeout: String::from("5s"),
                    startup_timeout: String::from("15s"),
                    startup_retry_interval: String::from("250ms"),
                    lease_renew_interval: String::from("2s"),
                    request_timeout: String::from("30s"),
                    space_id: String::from(MANAGED_VMM_SPACE_ID),
                    space_label: String::from(MANAGED_VMM_SPACE_LABEL),
                },
            },
            inference: ManagedInferenceConfig {
                mode: String::from(MANAGED_INFERENCE_MODE),
                discovery_file: discovery_path.to_string_lossy().into_owned(),
                expected_caller_id: String::from("vmm-local"),
                consumer_profile_id: String::from("vmm"),
                startup_timeout: String::from("30s"),
                max_connections_per_host: runtime_settings.inference_max_connections_per_host,
                embedding_dimension,
            },
            logging: ManagedLoggingConfig {
                level: String::from("info"),
                format: String::from("text"),
                debug_rpc_payloads: false,
                llm_output_enabled: false,
                protect_payloads: false,
                payload_encryption_key: String::new(),
            },
            pii: ManagedPiiConfig {
                default_language: runtime_settings.pii_default_language.clone(),
            },
            noise: ManagedNoiseConfig {
                enabled: runtime_settings.noise_enabled,
                default_language: runtime_settings.noise_default_language.clone(),
                semantic_enabled: runtime_settings.noise_semantic_enabled,
                semantic_threshold: runtime_settings.noise_semantic_threshold,
            },
            prompts: ManagedPromptConfig {
                prompt_language: runtime_settings.prompt_language.clone(),
            },
            vector: ManagedVectorConfig {
                provider: String::from("lancedb"),
            },
            relational: ManagedRelationalConfig {
                provider: String::from("sqlite"),
            },
            pre_check: ManagedPreCheckConfig {
                intent_timeout: runtime_settings.precheck_intent_timeout.clone(),
                top_k: runtime_settings.precheck_top_k,
                search_scope: runtime_settings.precheck_search_scope.clone(),
                similarity_threshold: runtime_settings.precheck_similarity_threshold,
            },
            post_action: ManagedPostActionConfig {
                input_mode: runtime_settings.postaction_input_mode.clone(),
                session_analysis_turn_threshold: runtime_settings.postaction_turn_threshold,
                session_analysis_token_threshold: runtime_settings.postaction_token_threshold,
                session_analysis_idle_timeout: runtime_settings.postaction_idle_timeout.clone(),
                session_analysis_history_turns: runtime_settings.postaction_history_turns,
                session_analysis_max_input_tokens: runtime_settings.postaction_max_input_tokens,
                session_analysis_timeout: runtime_settings.postaction_analysis_timeout.clone(),
                failure_pass_threshold: runtime_settings.postaction_failure_pass_threshold,
                max_queue_workers: runtime_settings.postaction_max_queue_workers,
            },
            memory_pipeline: ManagedMemoryPipelineConfig {
                max_search_keywords: runtime_settings.pipeline_max_search_keywords,
                min_similarity_score: Some(runtime_settings.pipeline_min_similarity_score),
                replace_min_similarity_score: Some(
                    runtime_settings.pipeline_replace_min_similarity_score,
                ),
                hard_dedupe_cosine_threshold: Some(runtime_settings.pipeline_hard_dedupe_threshold),
                hard_dedupe_pool_top_k: runtime_settings.pipeline_hard_dedupe_pool_top_k,
                hybrid_enabled: runtime_settings.pipeline_hybrid_enabled,
                lexical_top_k: runtime_settings.pipeline_lexical_top_k,
                rrf_k: runtime_settings.pipeline_rrf_k,
                mmr_enabled: runtime_settings.pipeline_mmr_enabled,
                mmr_lambda: runtime_settings.pipeline_mmr_lambda,
                weibull_enabled: runtime_settings.pipeline_weibull_enabled,
                weibull_shape: runtime_settings.pipeline_weibull_shape,
                weibull_scale_hours: runtime_settings.pipeline_weibull_scale_hours,
                weibull_min_multiplier: runtime_settings.pipeline_weibull_min_multiplier,
                weibull_reinforce_weight: runtime_settings.pipeline_weibull_reinforce_weight,
                weibull_cross_session_boost: runtime_settings.pipeline_weibull_cross_session_boost,
            },
            retention: ManagedRetentionConfig {
                enabled: runtime_settings.retention_enabled,
                recycle_scan_interval: runtime_settings.retention_scan_interval.clone(),
                turn_keep_extra_turns: runtime_settings.retention_turn_keep_extra_turns,
                session_idle_recycle_after: runtime_settings.retention_session_idle_after.clone(),
                trash_retention: runtime_settings.retention_trash_duration.clone(),
                protect_priority_floor: runtime_settings.retention_priority_floor.clone(),
                protect_memory_level_floor: runtime_settings.retention_memory_level_floor.clone(),
                skip_protected_shared_memories: runtime_settings.retention_skip_protected_shared,
            },
        },
    })
}

/// Formats one millisecond duration using the exact Go duration unit accepted by VMM.
/// 使用 VMM 接受的准确 Go 时长单位格式化一个毫秒时长。
fn milliseconds_as_duration(value: u64) -> String {
    format!("{value}ms")
}

/// Loads and validates the exact current inference discovery before child startup.
/// 在子进程启动前加载并校验精确的当前推理发现。
fn load_inference_discovery(path: &Path) -> Result<InferenceServiceDiscoverySnapshot, String> {
    let bytes = fs::read(path).map_err(|error| format!("读取 VMM 推理发现文件失败：{error}"))?;
    let discovery: InferenceServiceDiscoverySnapshot = serde_json::from_slice(&bytes)
        .map_err(|error| format!("解析 VMM 推理发现文件失败：{error}"))?;
    if discovery.api_version != INFERENCE_SERVICE_API_VERSION
        || discovery.status != "ready"
        || discovery.process_id != std::process::id()
        || discovery.caller_id != "vmm-local"
        || discovery.consumer_profile_id != "vmm"
    {
        return Err(String::from("VMM 推理发现文件与当前宿主身份或协议不匹配。"));
    }
    let origin = reqwest::Url::parse(&discovery.base_url)
        .map_err(|error| format!("VMM 推理发现地址无效：{error}"))?;
    let host = origin
        .host_str()
        .and_then(|host| host.parse::<IpAddr>().ok())
        .ok_or_else(|| String::from("VMM 推理发现地址必须使用显式回环 IP。"))?;
    if !host.is_loopback() || origin.scheme() != "http" {
        return Err(String::from("VMM 推理发现地址必须是 HTTP 回环地址。"));
    }
    Ok(discovery)
}

/// Calculates the preferred VMM port from the actual WebUI inference origin.
/// 从实际 WebUI 推理源计算首选 VMM 端口。
fn preferred_vmm_address(discovery: &InferenceServiceDiscoverySnapshot) -> Result<String, String> {
    let origin = reqwest::Url::parse(&discovery.base_url)
        .map_err(|error| format!("解析 WebUI 推理地址失败：{error}"))?;
    let port = origin
        .port_or_known_default()
        .ok_or_else(|| String::from("WebUI 推理地址缺少端口。"))?;
    if port <= u16::MAX - 10 {
        Ok(format!("127.0.0.1:{}", port + 10))
    } else {
        Ok(String::from("127.0.0.1:0"))
    }
}

/// Waits for an atomic ready document while also observing early child exit.
/// 等待原子就绪文档，同时观察子进程是否提前退出。
#[allow(clippy::too_many_arguments)]
async fn wait_for_status(
    child: &mut Child,
    status_path: &Path,
    instance_id: &str,
    generation: u64,
    child_id: u32,
    parent_id: u32,
    manifest_digest: &str,
) -> Result<ManagedStatus, String> {
    let deadline = Instant::now() + MANAGED_STARTUP_TIMEOUT;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("查询 VMM 子进程状态失败：{error}"))?
        {
            return Err(format!("VMM 子进程在就绪前退出：{status}"));
        }
        if status_path.is_file() {
            let bytes =
                fs::read(status_path).map_err(|error| format!("读取 VMM 就绪状态失败：{error}"))?;
            let status: ManagedStatus = serde_json::from_slice(&bytes)
                .map_err(|error| format!("解析 VMM 就绪状态失败：{error}"))?;
            validate_status(
                &status,
                instance_id,
                generation,
                child_id,
                parent_id,
                manifest_digest,
            )?;
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Err(String::from("等待 VMM 子进程就绪超时。"));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Validates every identity and protocol field in one child readiness document.
/// 校验一份子进程就绪文档中的每个身份与协议字段。
fn validate_status(
    status: &ManagedStatus,
    instance_id: &str,
    generation: u64,
    child_id: u32,
    parent_id: u32,
    manifest_digest: &str,
) -> Result<(), String> {
    if status.contract_version != MANAGED_CONTRACT_VERSION
        || status.runtime_build_id != MANAGED_RUNTIME_BUILD_ID
        || status.instance_id != instance_id
        || status.generation != generation
        || status.process_id != child_id
        || status.parent_process_id != parent_id
        || status.manifest_digest != manifest_digest
        || status.grpc_proto_version != 1
        || status.management_protocol_version != 1
        || !status.management_ready
        || status.inference_protocol_version != INFERENCE_SERVICE_API_VERSION
    {
        return Err(String::from("VMM 就绪状态的身份、摘要或协议版本不匹配。"));
    }
    let address = status
        .grpc_listen_addr
        .parse::<SocketAddr>()
        .map_err(|error| format!("VMM 就绪地址无效：{error}"))?;
    if !address.ip().is_loopback() {
        return Err(String::from("VMM 就绪地址不是回环地址。"));
    }
    let management_address = status
        .management_listen_addr
        .parse::<SocketAddr>()
        .map_err(|error| format!("VMM 管理就绪地址无效：{error}"))?;
    if !management_address.ip().is_loopback() {
        return Err(String::from("VMM 管理就绪地址不是回环地址。"));
    }
    Ok(())
}

/// Minimal authenticated capabilities response used only during managed child startup.
/// 仅在托管子进程启动期间使用的最小鉴权能力响应。
#[derive(Debug, Deserialize)]
struct ManagementCapabilitiesProbe {
    /// Exact management protocol version returned by the child.
    /// 子进程返回的精确管理协议版本。
    protocol_version: u32,
}

/// Calls the authenticated management capabilities endpoint before exposing any administration proxy.
/// 在暴露任何管理代理前调用带鉴权的管理能力端点。
async fn management_health_check(listen_addr: &str, access_token: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("创建 VMM 管理健康检查客户端失败：{error}"))?;
    let response = client
        .get(format!("http://{listen_addr}/management/v1/capabilities"))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|error| format!("VMM 管理健康检查失败：{error}"))?;
    if !response.status().is_success() {
        return Err(format!("VMM 管理健康检查返回 HTTP {}。", response.status()));
    }
    let capabilities = response
        .json::<ManagementCapabilitiesProbe>()
        .await
        .map_err(|error| format!("解析 VMM 管理能力失败：{error}"))?;
    if capabilities.protocol_version != 1 {
        return Err(format!(
            "VMM 管理协议版本不匹配：期望 1，实际 {}。",
            capabilities.protocol_version
        ));
    }
    Ok(())
}

/// Calls authenticated Healthz before exposing the runtime target.
/// 在暴露运行时目标前调用鉴权 Healthz。
pub(crate) async fn health_check(
    grpc_target: &str,
    access_token: &str,
    vmm_config: &vulcan_vmm::VmmConfigLayer,
) -> Result<(), String> {
    let client = TonicVmmClient::connect(managed_transport_config(
        grpc_target,
        access_token,
        vmm_config,
    ))
    .await
    .map_err(|error| format!("连接 VMM 托管 gRPC 失败：{error}"))?;
    let health = client
        .healthz()
        .await
        .map_err(|error| format!("VMM 托管 Healthz 失败：{error}"))?;
    if !health.status.eq_ignore_ascii_case("ok") {
        return Err(format!("VMM 托管 Healthz 返回异常状态：{}", health.status));
    }
    Ok(())
}

/// Builds the exact ephemeral transport used for managed readiness and reconciliation calls.
/// 构建托管就绪检查与对账调用使用的准确临时传输。
fn managed_transport_config(
    grpc_target: &str,
    access_token: &str,
    vmm_config: &vulcan_vmm::VmmConfigLayer,
) -> VmmTransportConfig {
    VmmTransportConfig {
        grpc_target: grpc_target.to_string(),
        grpc_api_key: Some(format!("Bearer {access_token}")),
        handshake_timeout: Duration::from_millis(vmm_config.handshake_timeout_ms.unwrap_or(5_000)),
        workspace_timeout: Duration::from_millis(vmm_config.receive_timeout_ms.unwrap_or(30_000)),
        precheck_timeout: Duration::from_millis(
            vmm_config.receive_timeout_ms.unwrap_or(30_000).min(30_000),
        ),
        post_action_timeout: Duration::from_millis(vmm_config.receive_timeout_ms.unwrap_or(30_000)),
        background_timeout: Duration::from_millis(
            vmm_config.receive_timeout_ms.unwrap_or(30_000).max(120_000),
        ),
        max_receive_message_bytes: vmm_config.max_receive_message_bytes,
        keepalive_time: Duration::from_millis(vmm_config.grpc_keepalive_time_ms.unwrap_or(30_000)),
        keepalive_timeout: Duration::from_millis(
            vmm_config.grpc_keepalive_timeout_ms.unwrap_or(10_000),
        ),
        keepalive_while_idle: vmm_config
            .grpc_keepalive_permit_without_calls
            .unwrap_or(true),
    }
}

/// Resolves the exact configured numeric VMM scope without inventing host-to-VMM path mappings.
/// 解析准确配置的数字 VMM 作用域，不虚构宿主到 VMM 的路径映射。
async fn reconcile_configured_scope(
    grpc_target: &str,
    access_token: &str,
    vmm_config: &vulcan_vmm::VmmConfigLayer,
) -> Result<(), String> {
    let user_id = vmm_config
        .user_id
        .ok_or_else(|| String::from("托管 VMM 尚未配置数字用户 ID。"))?;
    let project_id = vmm_config
        .project_id
        .ok_or_else(|| String::from("托管 VMM 尚未配置数字项目 ID。"))?;
    let client = TonicVmmClient::connect(managed_transport_config(
        grpc_target,
        access_token,
        vmm_config,
    ))
    .await
    .map_err(|error| format!("连接 VMM 托管对账 gRPC 失败：{error}"))?;
    let user = client
        .resolve_user(ResolveUserRequest {
            user_ref: user_id.to_string(),
            confirm_create: false,
        })
        .await
        .map_err(|error| format!("解析 VMM 用户 {user_id} 失败：{error}"))?
        .user
        .ok_or_else(|| format!("VMM 用户 {user_id} 不存在；请先在用户管理中显式创建。"))?;
    if user.user_id != user_id {
        return Err(format!(
            "VMM 用户对账返回 ID {}，与配置 ID {user_id} 不一致。",
            user.user_id
        ));
    }
    let project = client
        .resolve_project(ResolveProjectRequest {
            project_ref: project_id.to_string(),
        })
        .await
        .map_err(|error| format!("解析 VMM 项目 {project_id} 失败：{error}"))?
        .project
        .ok_or_else(|| format!("VMM 项目 {project_id} 不存在；请先在项目管理中显式创建。"))?;
    if project.project_id != project_id {
        return Err(format!(
            "VMM 项目对账返回 ID {}，与配置 ID {project_id} 不一致。",
            project.project_id
        ));
    }
    Ok(())
}

/// Writes one private runtime document by same-directory temporary replacement.
/// 通过同目录临时替换写入一份私有运行期文档。
fn atomic_write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| String::from("VMM 托管清单缺少父目录。"))?;
    let temporary = parent.join(format!(".managed-config-{}.tmp", Uuid::new_v4()));
    let write_result = (|| {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| format!("创建 VMM 临时清单失败：{error}"))?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("写入 VMM 临时清单失败：{error}"))?;
        set_private_file_permissions(&temporary)?;
        fs::rename(&temporary, path).map_err(|error| format!("发布 VMM 托管清单失败：{error}"))
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

/// Removes one known ephemeral runtime file without touching data directories.
/// 删除一个已知临时运行期文件，不触碰数据目录。
fn remove_runtime_file(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("删除陈旧 VMM 运行期文件失败：{error}")),
    }
}

/// Best-effort removes only the known ephemeral files after managed startup fails.
/// 在托管启动失败后尽力删除且只删除已知的临时运行期文件。
///
/// # Parameters
/// 参数
/// `manifest_path`, `status_path`, and `shutdown_path` are host-derived exact files.
/// `manifest_path`、`status_path` 与 `shutdown_path` 是宿主派生的精确文件。
fn cleanup_ephemeral_runtime_files(manifest_path: &Path, status_path: &Path, shutdown_path: &Path) {
    for path in [manifest_path, status_path, shutdown_path] {
        let _ = remove_runtime_file(path);
    }
}

/// Validates the explicitly configured managed executable and its packaged resource directory.
/// 校验显式配置的托管可执行文件及其配套打包资源目录。
///
/// # Parameters
/// 参数
/// `path` is the exact configured `vmm-local` executable path.
/// `path` 是已配置的准确 `vmm-local` 可执行文件路径。
///
/// # Returns
/// 返回值
/// Returns the canonical executable path only when both the executable and adjacent package
/// resources are usable.
/// 仅当可执行文件及其相邻打包资源均可用时返回规范化可执行文件路径。
pub(crate) fn validate_managed_vmm_package(path: &Path) -> Result<PathBuf, String> {
    let path =
        fs::canonicalize(path).map_err(|error| format!("解析 VMM 托管可执行文件失败：{error}"))?;
    let metadata =
        fs::metadata(&path).map_err(|error| format!("读取 VMM 可执行文件信息失败：{error}"))?;
    if !metadata.is_file() {
        return Err(format!("VMM 托管路径不是文件：{}", path.display()));
    }
    let system_root = path
        .parent()
        .and_then(Path::parent)
        .map(|root| root.join("configs"))
        .ok_or_else(|| String::from("无法从 VMM 可执行文件解析打包资源目录。"))?;
    if !system_root.is_dir() {
        return Err(format!("VMM 打包资源目录不存在：{}", system_root.display()));
    }
    validate_managed_runtime_manifest(&path, &system_root)?;
    Ok(path)
}

/// Validates the packaged build identity, binary digests, and deterministic config-tree digest.
/// 校验打包构建身份、二进制摘要与确定性的配置树摘要。
fn validate_managed_runtime_manifest(binary_path: &Path, config_root: &Path) -> Result<(), String> {
    let manifest_path = config_root.join(MANAGED_RUNTIME_MANIFEST_FILE);
    let body =
        fs::read(&manifest_path).map_err(|error| format!("读取 VMM 运行时清单失败：{error}"))?;
    let manifest: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|error| format!("解析 VMM 运行时清单失败：{error}"))?;
    if manifest["manifest_version"].as_u64() != Some(1)
        || manifest["managed_contract_version"].as_u64()
            != Some(u64::from(MANAGED_CONTRACT_VERSION))
        || manifest["inference_protocol_version"].as_u64()
            != Some(u64::from(INFERENCE_SERVICE_API_VERSION))
        || manifest["build_id"].as_str() != Some(MANAGED_RUNTIME_BUILD_ID)
    {
        return Err(String::from("VMM 运行时清单的版本或 build ID 不匹配。"));
    }
    for field in ["platform", "source_revision", "source_state_digest"] {
        if manifest[field]
            .as_str()
            .is_none_or(|value| value.trim().is_empty())
        {
            return Err(format!("VMM 运行时清单缺少非空字段 `{field}`。"));
        }
    }
    let binary_names = [
        binary_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| String::from("无法解析 VMM 主二进制文件名。"))?,
        if cfg!(windows) {
            "vmm-migrate.exe"
        } else {
            "vmm-migrate"
        },
    ];
    let binaries = manifest["binaries"]
        .as_object()
        .ok_or_else(|| String::from("VMM 运行时清单缺少 binaries 对象。"))?;
    for binary_name in binary_names {
        let binary_path = binary_path
            .parent()
            .ok_or_else(|| String::from("无法解析 VMM 二进制目录。"))?
            .join(binary_name);
        let expected = binaries
            .get(binary_name)
            .and_then(|entry| entry.get("sha256"))
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("VMM 运行时清单缺少 `{binary_name}` 的 SHA-256。"))?;
        let actual = sha256_file(&binary_path)?;
        if actual != expected {
            return Err(format!(
                "VMM 二进制 `{binary_name}` 的 SHA-256 与清单不匹配。"
            ));
        }
    }
    let expected_config_digest = manifest["config_assets_sha256"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| String::from("VMM 运行时清单缺少 config_assets_sha256。"))?;
    let actual_config_digest = managed_config_assets_digest(config_root)?;
    if actual_config_digest != expected_config_digest {
        return Err(String::from("VMM 配置资产摘要与运行时清单不匹配。"));
    }
    Ok(())
}

/// Computes one stable SHA-256 over sorted relative config paths and their bytes.
/// 对排序后的相对配置路径及文件字节计算稳定 SHA-256。
fn managed_config_assets_digest(root: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    collect_managed_config_files(root, root, &mut files)?;
    files.sort();
    let mut digest = Sha256::new();
    for path in files {
        let relative = path
            .strip_prefix(root)
            .map_err(|error| format!("解析 VMM 配置相对路径失败：{error}"))?
            .to_string_lossy()
            .replace('\\', "/");
        digest.update(relative.as_bytes());
        digest.update([0]);
        let bytes = fs::read(&path)
            .map_err(|error| format!("读取 VMM 配置资产 `{}` 失败：{error}", path.display()))?;
        digest.update(bytes);
        digest.update([0]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

/// Collects regular configuration files while rejecting symlink escapes.
/// 收集普通配置文件，同时拒绝符号链接逃逸。
fn collect_managed_config_files(
    _root: &Path,
    current: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    for entry in fs::read_dir(current)
        .map_err(|error| format!("读取 VMM 配置目录 `{}` 失败：{error}", current.display()))?
    {
        let entry = entry.map_err(|error| format!("读取 VMM 配置目录项失败：{error}"))?;
        let entry_path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("读取 VMM 配置目录项类型失败：{error}"))?;
        if file_type.is_symlink() {
            return Err(format!(
                "VMM 配置资产不允许使用符号链接：{}",
                entry_path.display()
            ));
        }
        if file_type.is_dir() {
            collect_managed_config_files(_root, &entry_path, files)?;
        } else if file_type.is_file()
            && entry_path.file_name().and_then(|name| name.to_str())
                != Some(MANAGED_RUNTIME_MANIFEST_FILE)
        {
            files.push(entry_path);
        }
    }
    Ok(())
}

/// Computes one lowercase SHA-256 for an exact packaged binary.
/// 为精确的打包二进制计算小写 SHA-256。
fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("读取 VMM 打包二进制 `{}` 失败：{error}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// Terminates and reaps one child after a bounded graceful observation period.
/// 在有限优雅观察期后终止并回收一个子进程。
fn stop_child(child: &mut Child) {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Requests managed graceful shutdown and only force-kills after the bounded timeout.
/// 请求托管运行时优雅关闭，并仅在有限超时后强制终止。
///
/// # Parameters
/// 参数
/// `shutdown_path` is the host-owned marker and `child` is the exact owned process.
/// `shutdown_path` 是宿主持有的关闭标记，`child` 是精确持有的子进程。
fn request_graceful_stop(shutdown_path: &Path, child: &mut Child) {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }
    let _ = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(shutdown_path)
        .and_then(|file| file.sync_all());
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => break,
        }
    }
    stop_child(child);
}

/// Applies platform-specific private permissions to one runtime file.
/// 为一个运行期文件应用平台专用私有权限。
#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("设置 VMM 清单私有权限失败：{error}"))
}

/// Keeps Windows files under the current user's application-data ACL.
/// 在 Windows 上沿用当前用户应用数据目录的 ACL。
#[cfg(windows)]
fn set_private_file_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Hides the managed child console on Windows.
/// 在 Windows 上隐藏托管子进程控制台。
#[cfg(windows)]
pub(crate) fn configure_hidden_child(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

/// Leaves child process configuration unchanged on non-Windows platforms.
/// 在非 Windows 平台保持子进程配置不变。
#[cfg(not(windows))]
pub(crate) fn configure_hidden_child(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use vulcan_ai_contract::{
        CapabilitySupport, DistanceMetric, EmbeddingCapabilities, EmbeddingEncoding,
        EmbeddingNormalization, EmbeddingOutputKind, EmbeddingTask, InferenceUsageScope,
        ModelCapabilities, ReasoningControlMode,
    };
    use vulcan_provider_adapters::{
        ConnectionModel, CredentialConnection, CredentialMode, InferenceProfile,
        InferenceRouteBinding, InferenceRouteConfiguration,
    };

    /// Builds deterministic optional-route validation facts for capability projection tests.
    /// 为能力投影测试构建确定性的可选路由校验事实。
    ///
    /// # Parameters
    /// 参数
    /// `profile_instruction_error` and `rerank_error` are the exact controlled failures.
    /// `profile_instruction_error` 与 `rerank_error` 是精确的受控失败。
    ///
    /// # Returns
    /// 返回值
    /// One optional-route validation snapshot.
    /// 返回一份可选路由校验快照。
    fn optional_route_validation(
        profile_instruction_error: Option<&str>,
        rerank_error: Option<&str>,
    ) -> crate::configured_inference::VmmOptionalInferenceRouteValidation {
        crate::configured_inference::VmmOptionalInferenceRouteValidation {
            profile_instruction_error: profile_instruction_error.map(str::to_string),
            rerank_error: rerank_error.map(str::to_string),
        }
    }

    /// Verifies the generated host manifest survives strict semantic JSON round-trip.
    /// 验证宿主生成的清单可通过严格的 JSON 语义往返。
    #[test]
    fn managed_manifest_round_trips_without_field_loss() {
        let root = std::env::temp_dir().join(format!("vulcan-vmm-contract-{}", Uuid::new_v4()));
        let vmm_config = vulcan_vmm::VmmConfigLayer {
            install: Some(vulcan_vmm::VmmInstallReservation::default()),
            ..vulcan_vmm::VmmConfigLayer::default()
        };
        let manifest = default_managed_manifest(
            "instance",
            7,
            11,
            &root.join("configs"),
            &root.join("data"),
            &root.join("status.json"),
            &root.join("shutdown"),
            "127.0.0.1:0",
            "secret",
            "management-secret",
            "http://127.0.0.1:17625",
            &root.join("inference.json"),
            &vmm_config,
            1024,
        )
        .expect("build managed manifest");
        let body = serde_json::to_vec(&manifest).expect("serialize managed manifest");
        let decoded: ManagedConfig =
            serde_json::from_slice(&body).expect("deserialize managed manifest");
        assert_eq!(decoded, manifest);
        assert_eq!(
            decoded.runtime.shutdown_file,
            root.join("shutdown").to_string_lossy()
        );
        assert_eq!(decoded.runtime.storage.controller_lease.lease_ttl, "10s");
        assert_eq!(
            decoded
                .runtime
                .storage
                .controller_lease
                .lease_renew_interval,
            "2s"
        );
    }

    /// Verifies readiness validation rejects a mismatched child identity.
    /// 验证就绪校验会拒绝不匹配的子进程身份。
    #[test]
    fn managed_status_rejects_mismatched_child_identity() {
        let status = ManagedStatus {
            contract_version: MANAGED_CONTRACT_VERSION,
            runtime_build_id: String::from(MANAGED_RUNTIME_BUILD_ID),
            instance_id: String::from("instance"),
            generation: 7,
            process_id: 42,
            parent_process_id: 11,
            manifest_digest: String::from("digest"),
            grpc_listen_addr: String::from("127.0.0.1:17635"),
            grpc_proto_version: 1,
            management_listen_addr: String::from("127.0.0.1:17636"),
            management_protocol_version: 1,
            management_ready: true,
            inference_protocol_version: INFERENCE_SERVICE_API_VERSION,
        };
        let error = validate_status(&status, "instance", 7, 43, 11, "digest")
            .expect_err("mismatched child must fail");
        assert!(error.contains("不匹配"));
    }

    /// Verifies generations remain strictly increasing across controlled restarts.
    /// 验证代次在受控重启之间保持严格递增。
    #[test]
    fn managed_generation_is_monotonic() {
        let first = next_generation(100);
        let second = next_generation(100);
        assert!(second > first);
    }

    /// Verifies package validation rejects a standalone binary without its required assets.
    /// 验证打包校验会拒绝缺少必需资源的孤立二进制文件。
    #[test]
    fn managed_package_validation_requires_adjacent_config_assets() {
        let root = std::env::temp_dir().join(format!("vulcan-vmm-package-{}", Uuid::new_v4()));
        let binary_dir = root.join("bin");
        fs::create_dir_all(&binary_dir).expect("binary directory should be created");
        let binary_path = binary_dir.join(if cfg!(windows) {
            "vmm-local.exe"
        } else {
            "vmm-local"
        });
        let migrate_binary_path = binary_dir.join(if cfg!(windows) {
            "vmm-migrate.exe"
        } else {
            "vmm-migrate"
        });
        fs::write(&binary_path, b"test").expect("test binary should be written");
        fs::write(&migrate_binary_path, b"migrate-test")
            .expect("maintenance binary should be written");

        let error = validate_managed_vmm_package(&binary_path)
            .expect_err("package without configs must be rejected");
        assert!(error.contains("打包资源目录"));

        let config_root = root.join("configs");
        fs::create_dir_all(&config_root).expect("config directory should be created");
        fs::write(config_root.join("base.yaml"), b"fixture: true\n")
            .expect("fixture config should be written");
        let config_digest = managed_config_assets_digest(&config_root)
            .expect("fixture config digest should be computed");
        let manifest = serde_json::json!({
            "manifest_version": 1,
            "platform": if cfg!(windows) { "windows-amd64" } else { "test" },
            "managed_contract_version": MANAGED_CONTRACT_VERSION,
            "inference_protocol_version": INFERENCE_SERVICE_API_VERSION,
            "source_revision": "fixture",
            "source_state_digest": "fixture",
            "build_id": MANAGED_RUNTIME_BUILD_ID,
            "binaries": {
                binary_path.file_name().expect("fixture binary must have a file name").to_string_lossy(): {
                    "sha256": sha256_file(&binary_path).expect("fixture binary digest")
                },
                migrate_binary_path.file_name().expect("fixture maintenance binary must have a file name").to_string_lossy(): {
                    "sha256": sha256_file(&migrate_binary_path).expect("fixture maintenance digest")
                }
            },
            "config_assets_sha256": config_digest,
        });
        fs::write(
            config_root.join(MANAGED_RUNTIME_MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).expect("fixture manifest should serialize"),
        )
        .expect("fixture manifest should be written");
        let resolved = validate_managed_vmm_package(&binary_path)
            .expect("package with adjacent configs should pass");
        assert_eq!(
            resolved,
            fs::canonicalize(&binary_path).expect("binary should canonicalize")
        );
        fs::remove_dir_all(&root).expect("test package should be removed");
    }

    /// Verifies an intentional stopped runtime is never reclassified as a crash.
    /// 验证主动停止的运行时绝不会被重新归类为崩溃。
    #[test]
    fn stopped_runtime_is_not_reported_as_crashed() {
        let root = std::env::temp_dir().join(format!("vulcan-vmm-stopped-{}", Uuid::new_v4()));
        let mut runtime = ManagedMemoryMeshRuntime {
            child: None,
            #[cfg(windows)]
            child_job: None,
            manifest_path: root.join("manifest.json"),
            status_path: root.join("status.json"),
            shutdown_path: root.join("shutdown"),
            access_token: String::from("runtime-secret"),
            management_listen_addr: String::from("127.0.0.1:17902"),
            management_access_token: String::from("management-secret"),
            snapshot: ManagedMemoryMeshSnapshot {
                data_space_state: vulcan_service_api::VmmDataSpaceState::Unknown,
                phase: String::from("stopped"),
                grpc_target: None,
                controller_endpoint: String::from("http://127.0.0.1:19801"),
                controller_space_id: String::from(MANAGED_VMM_SPACE_ID),
                data_root: root.join("data"),
                log_path: root.join("managed.log"),
                last_error: Some(String::from("stale error")),
                scope_ready: false,
                scope_error: Some(String::from("test")),
                capabilities: unavailable_capabilities("test"),
            },
        };

        assert!(!runtime.refresh_liveness());
        runtime.stop().expect("stopped runtime cleanup");
        assert!(runtime.snapshot.last_error.is_none());
        assert!(runtime.access_token.is_empty());
        assert!(runtime.management_access_token.is_empty());
        assert!(runtime.management_listen_addr.is_empty());
        runtime.snapshot.phase = String::from("waiting_retry");
        assert!(runtime.refresh_liveness());
    }

    /// Verifies heartbeat failures become visible and later success restores readiness.
    /// 验证心跳失败会变为可见状态，后续成功会恢复就绪。
    #[test]
    fn managed_heartbeat_result_round_trips_unhealthy_state() {
        let root = std::env::temp_dir().join(format!("vulcan-vmm-heartbeat-{}", Uuid::new_v4()));
        let mut snapshot = ManagedMemoryMeshSnapshot {
            data_space_state: vulcan_service_api::VmmDataSpaceState::Ready,
            phase: String::from("ready"),
            grpc_target: Some(String::from("127.0.0.1:17901")),
            controller_endpoint: String::from("http://127.0.0.1:19801"),
            controller_space_id: String::from(MANAGED_VMM_SPACE_ID),
            data_root: root.join("data"),
            log_path: root.join("managed.log"),
            last_error: None,
            scope_ready: true,
            scope_error: None,
            capabilities: unavailable_capabilities("test"),
        };

        assert!(apply_managed_heartbeat_result(
            &mut snapshot,
            Err(String::from("timeout"))
        ));
        assert_eq!(snapshot.phase, "unhealthy");
        assert_eq!(
            snapshot.last_error.as_deref(),
            Some("VMM 托管心跳失败：timeout")
        );
        assert!(!apply_managed_heartbeat_result(
            &mut snapshot,
            Err(String::from("timeout"))
        ));
        assert!(apply_managed_heartbeat_result(&mut snapshot, Ok(())));
        assert_eq!(snapshot.phase, "ready");
        assert!(snapshot.last_error.is_none());
    }

    /// Verifies retained workbench synchronization includes every managed capability field.
    /// 验证保留工作台同步会包含全部托管能力字段。
    #[test]
    fn managed_snapshot_projection_keeps_capability_state_complete() {
        let root = std::env::temp_dir().join(format!("vulcan-vmm-projection-{}", Uuid::new_v4()));
        let mut runtime = vulcan_service_api::VmmRuntimeSummary::default();
        apply_managed_snapshot_to_runtime(
            &mut runtime,
            Some(ManagedMemoryMeshSnapshot {
                data_space_state: vulcan_service_api::VmmDataSpaceState::Ready,
                phase: String::from("ready"),
                grpc_target: Some(String::from("127.0.0.1:17901")),
                controller_endpoint: String::from("http://127.0.0.1:19801"),
                controller_space_id: String::from(MANAGED_VMM_SPACE_ID),
                data_root: root.join("data"),
                log_path: root.join("managed.log"),
                last_error: None,
                scope_ready: false,
                scope_error: Some(String::from("scope unavailable")),
                capabilities: ManagedVmmCapabilityPreflight {
                    embedding_dimension: 1024,
                    embedding_fingerprint: None,
                    precheck_ready: true,
                    precheck_error: None,
                    postaction_ready: false,
                    postaction_error: Some(String::from("postaction unavailable")),
                    profile_instruction_ready: true,
                    profile_instruction_error: None,
                    rerank_ready: false,
                    rerank_error: Some(String::from("rerank unavailable")),
                },
            }),
        );

        assert!(runtime.process_ready);
        assert!(!runtime.scope_ready);
        assert_eq!(runtime.scope_error.as_deref(), Some("scope unavailable"));
        assert!(runtime.precheck_ready);
        assert!(!runtime.postaction_ready);
        assert!(runtime.profile_instruction_ready);
        assert!(!runtime.rerank_ready);

        apply_managed_snapshot_to_runtime(&mut runtime, None);
        assert!(!runtime.process_ready);
        assert_eq!(runtime.managed_phase, "external");
        assert!(!runtime.precheck_ready);
        assert!(runtime.precheck_error.is_none());
        assert!(!runtime.profile_instruction_ready);
        assert!(runtime.profile_instruction_error.is_none());
    }

    /// Verifies stopped managed processes never expose preflight checks as live capabilities.
    /// 验证托管进程停止后绝不会把预检结果误报为实时可用能力。
    #[test]
    fn stopped_managed_snapshot_disables_all_live_capabilities() {
        let root =
            std::env::temp_dir().join(format!("vulcan-vmm-stopped-projection-{}", Uuid::new_v4()));
        let mut runtime = vulcan_service_api::VmmRuntimeSummary::default();
        apply_managed_snapshot_to_runtime(
            &mut runtime,
            Some(ManagedMemoryMeshSnapshot {
                data_space_state: vulcan_service_api::VmmDataSpaceState::Ready,
                phase: String::from("stopped"),
                grpc_target: None,
                controller_endpoint: String::from("http://127.0.0.1:19801"),
                controller_space_id: String::from(MANAGED_VMM_SPACE_ID),
                data_root: root.join("data"),
                log_path: root.join("managed.log"),
                last_error: None,
                scope_ready: true,
                scope_error: None,
                capabilities: ManagedVmmCapabilityPreflight {
                    embedding_dimension: 1024,
                    embedding_fingerprint: None,
                    precheck_ready: true,
                    precheck_error: None,
                    postaction_ready: true,
                    postaction_error: None,
                    profile_instruction_ready: true,
                    profile_instruction_error: None,
                    rerank_ready: true,
                    rerank_error: None,
                },
            }),
        );

        assert!(!runtime.process_ready);
        assert!(!runtime.scope_ready);
        assert!(!runtime.precheck_ready);
        assert!(!runtime.postaction_ready);
        assert!(!runtime.profile_instruction_ready);
        assert!(!runtime.rerank_ready);
    }

    /// Verifies capability preflight rejects a VMM profile without required embedding.
    /// 验证能力预检会拒绝缺少必需向量配置的 VMM 配置档。
    #[test]
    fn managed_capability_preflight_requires_embedding() {
        let error = configured_vmm_capability_preflight(
            &PersistedAiConfiguration::default(),
            &BTreeMap::new(),
            &optional_route_validation(
                Some("VMM 手工画像指令未配置。"),
                Some("VMM 重排路由未配置。"),
            ),
        )
        .expect_err("VMM profile without embedding must fail");
        assert!(error.contains("未配置向量模型"), "{error}");
    }

    /// Verifies the exact connection catalog supplies the default dimension when no local override exists.
    /// 验证没有本地覆盖时，由精确连接目录提供默认向量维度。
    #[test]
    fn managed_embedding_dimension_uses_exact_catalog_default() {
        let llm_route = InferenceRouteConfiguration {
            provider_id: String::from("provider"),
            service_entry_id: String::from("entry"),
            connection_id: String::from("llm-connection"),
            model_id: String::from("llm-model"),
            binding: InferenceRouteBinding::Protocol {
                protocol_profile_id: String::from("protocol"),
            },
            operation: AiOperation::ConversationRespond,
            usage_scope: InferenceUsageScope::ExternalService,
        };
        let embedding_route = InferenceRouteConfiguration {
            provider_id: String::from("alibaba"),
            service_entry_id: String::from("model-studio-cn"),
            connection_id: String::from("embedding-connection"),
            model_id: String::from("qwen3.7-text-embedding"),
            binding: InferenceRouteBinding::Action {
                action_binding_id: String::from("alibaba.model_studio.embeddings.v1"),
            },
            operation: AiOperation::EmbeddingCreate,
            usage_scope: InferenceUsageScope::ExternalService,
        };
        let mut configuration = PersistedAiConfiguration::default();
        configuration.connection_graph.connections.insert(
            String::from("embedding-connection"),
            CredentialConnection {
                id: String::from("embedding-connection"),
                provider_id: String::from("alibaba"),
                service_entry_id: String::from("model-studio-cn"),
                credential_profile_id: String::from("api-key"),
                credential_mode: CredentialMode::Token,
                credential_id: String::from("credential-embedding"),
                display_name: String::from("Embedding Account"),
                secret_reference: String::from("secret-embedding"),
                management_secret_reference: None,
                service_origin_override: None,
                prompt_cache_by_protocol: BTreeMap::new(),
            },
        );
        configuration.inference_profiles.insert(
            String::from("vmm"),
            InferenceProfile {
                llm_routes: [
                    "precheck_l1",
                    "precheck_l2",
                    "postaction_l1",
                    "postaction_l2",
                    "profile_instruction",
                ]
                .into_iter()
                .map(|purpose| (purpose.to_string(), llm_route.clone()))
                .collect(),
                embedding: Some(embedding_route),
                rerank: None,
            },
        );
        let capabilities = ModelCapabilities {
            operations: vec![AiOperation::EmbeddingCreate],
            embedding: Some(EmbeddingCapabilities {
                tasks: BTreeSet::from([EmbeddingTask::ProviderDefault]),
                input_resources: BTreeSet::new(),
                output_kinds: BTreeSet::from([EmbeddingOutputKind::Dense]),
                encodings: BTreeSet::from([EmbeddingEncoding::Float]),
                dimensions: BTreeSet::from([1024, 2560]),
                minimum_dimensions: Some(256),
                maximum_dimensions: Some(2560),
                default_dimensions: Some(1024),
                maximum_inputs: Some(20),
                maximum_tokens_per_input: Some(128_000),
                normalization: EmbeddingNormalization::Unknown,
                distance_metric: DistanceMetric::Unknown,
            }),
            reasoning: CapabilitySupport::Unsupported,
            reasoning_control: ReasoningControlMode::FixedOff,
            ..ModelCapabilities::default()
        };
        let catalogs = BTreeMap::from([(
            String::from("embedding-connection"),
            ConnectionModelCatalog {
                connection_id: String::from("embedding-connection"),
                models: vec![ConnectionModel {
                    id: String::from("qwen3.7-text-embedding"),
                    display_name: String::from("Qwen3.7 Text Embedding"),
                    capabilities,
                    capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(
                    ),
                    protocol_variants: BTreeMap::new(),
                    resolved_protocol_binding: None,
                }],
                generation: 1,
                page_count: 1,
                discovery_schema_revision: String::from("test.embedding.models.v1"),
            },
        )]);

        let optional_routes = optional_route_validation(None, Some("VMM 内部推理未配置重排模型。"));
        let preflight =
            configured_vmm_capability_preflight(&configuration, &catalogs, &optional_routes)
                .expect("capability preflight");
        assert_eq!(preflight.embedding_dimension, 1024);
        let fingerprint = preflight
            .embedding_fingerprint
            .as_ref()
            .expect("embedding fingerprint");
        assert_eq!(
            fingerprint.deployment_id.as_deref(),
            Some("credential-embedding")
        );
        assert_eq!(fingerprint.binding_id, "alibaba.model_studio.embeddings.v1");
        assert!(preflight.precheck_ready);
        assert!(preflight.postaction_ready);
        assert!(preflight.profile_instruction_ready);
        assert!(!preflight.rerank_ready);
        assert!(preflight.rerank_error.is_some());

        let invalid_optional_routes = optional_route_validation(
            Some("VMM 手工画像指令模型已被禁用。"),
            Some("VMM 重排动作绑定无效。"),
        );
        let degraded = configured_vmm_capability_preflight(
            &configuration,
            &catalogs,
            &invalid_optional_routes,
        )
        .expect("optional failures must degrade without blocking the VMM foundation");
        assert!(!degraded.profile_instruction_ready);
        assert_eq!(
            degraded.profile_instruction_error.as_deref(),
            Some("VMM 手工画像指令模型已被禁用。")
        );
        assert!(!degraded.rerank_ready);
        assert_eq!(
            degraded.rerank_error.as_deref(),
            Some("VMM 重排动作绑定无效。")
        );

        configuration
            .inference_profiles
            .get_mut("vmm")
            .expect("VMM profile")
            .llm_routes
            .remove("precheck_l2");
        let error =
            configured_vmm_capability_preflight(&configuration, &catalogs, &optional_routes)
                .expect_err("missing foundational LLM route must block process startup");
        assert!(error.contains("基础 LLM"), "{error}");
        assert!(error.contains("precheck_l2"), "{error}");
    }

    /// Verifies one empty managed data root adopts and then enforces one exact embedding space.
    /// 验证一个空托管数据根会接纳并随后强制执行一个精确向量空间。
    #[test]
    fn managed_embedding_space_marker_round_trips_and_rejects_changes() {
        let root =
            std::env::temp_dir().join(format!("vulcan-vmm-embedding-space-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create embedding-space root");
        let fingerprint = test_embedding_fingerprint();

        verify_managed_embedding_space(&root, &fingerprint).expect("empty root must be accepted");
        persist_managed_embedding_space(&root, &fingerprint).expect("persist embedding space");
        verify_managed_embedding_space(&root, &fingerprint).expect("same fingerprint must match");

        let mut changed = fingerprint;
        changed.model_id = String::from("different-model");
        let error = verify_managed_embedding_space(&root, &changed)
            .expect_err("changed embedding space must be rejected");
        assert!(error.contains("向量空间已变化"), "{error}");
        let _ = fs::remove_dir_all(root);
    }

    /// Verifies historical data without an ownership marker cannot be adopted silently.
    /// 验证缺少归属标记的历史数据不能被静默接纳。
    #[test]
    fn managed_embedding_space_rejects_unmarked_non_empty_root() {
        let root =
            std::env::temp_dir().join(format!("vulcan-vmm-unmarked-space-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create unmarked root");
        fs::write(root.join("historical.db"), b"history").expect("write historical marker");

        let error = verify_managed_embedding_space(&root, &test_embedding_fingerprint())
            .expect_err("unmarked historical data must be rejected");
        assert!(error.contains("未确认的历史数据"), "{error}");
        assert!(error.contains("备份并重建"), "{error}");
        let _ = fs::remove_dir_all(root);
    }

    /// Verifies corrupt and unsupported markers remain distinct recovery states.
    /// 验证损坏标记与不受支持标记保持为不同恢复状态。
    #[test]
    fn managed_embedding_space_reports_exact_marker_failures() {
        let root =
            std::env::temp_dir().join(format!("vulcan-vmm-marker-states-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create marker-state root");
        let marker_path = root.join(MANAGED_EMBEDDING_SPACE_STATE_FILE);
        let fingerprint = test_embedding_fingerprint();

        fs::write(&marker_path, b"{invalid").expect("write corrupt marker");
        assert_eq!(
            inspect_managed_embedding_space(&root, &fingerprint).expect("inspect corrupt marker"),
            vulcan_service_api::VmmDataSpaceState::MarkerCorrupt
        );

        let unsupported = ManagedEmbeddingSpaceState {
            version: MANAGED_EMBEDDING_SPACE_STATE_VERSION + 1,
            fingerprint: fingerprint.clone(),
        };
        fs::write(
            &marker_path,
            serde_json::to_vec(&unsupported).expect("serialize unsupported marker"),
        )
        .expect("write unsupported marker");
        assert_eq!(
            inspect_managed_embedding_space(&root, &fingerprint)
                .expect("inspect unsupported marker"),
            vulcan_service_api::VmmDataSpaceState::MarkerVersionUnsupported
        );

        let changed = ManagedEmbeddingSpaceState {
            version: MANAGED_EMBEDDING_SPACE_STATE_VERSION,
            fingerprint: EmbeddingSpaceFingerprint {
                model_id: String::from("changed-model"),
                ..fingerprint.clone()
            },
        };
        fs::write(
            &marker_path,
            serde_json::to_vec(&changed).expect("serialize changed marker"),
        )
        .expect("write changed marker");
        assert_eq!(
            inspect_managed_embedding_space(&root, &fingerprint).expect("inspect changed marker"),
            vulcan_service_api::VmmDataSpaceState::FingerprintChanged
        );

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies a stopped managed deployment cannot silently fall through to stale external management settings.
    /// 验证已停止的托管部署不会静默回退到陈旧的外部管理设置。
    #[test]
    fn external_management_target_rejects_managed_configuration() {
        let layer = vulcan_vmm::VmmConfigLayer {
            management_base_url: Some(String::from("http://127.0.0.1:17635")),
            management_bearer_token: Some(String::from("external-secret")),
            install: Some(vulcan_vmm::VmmInstallReservation {
                managed_install_enabled: true,
                ..vulcan_vmm::VmmInstallReservation::default()
            }),
            ..vulcan_vmm::VmmConfigLayer::default()
        };

        assert!(external_management_target_from_layer(&layer).is_none());
    }

    /// Verifies a configured external deployment retains its exact safe endpoint and timeout.
    /// 验证已配置的外部部署保留其准确安全端点与超时。
    #[test]
    fn external_management_target_accepts_explicit_external_configuration() {
        let layer = vulcan_vmm::VmmConfigLayer {
            management_base_url: Some(String::from("http://127.0.0.1:17635/")),
            management_bearer_token: Some(String::from("external-secret")),
            management_timeout_ms: Some(4_321),
            ..vulcan_vmm::VmmConfigLayer::default()
        };

        let target = external_management_target_from_layer(&layer)
            .expect("external management target should be available");
        assert_eq!(target.base_url, "http://127.0.0.1:17635");
        assert_eq!(target.access_token, "external-secret");
        assert_eq!(target.timeout, std::time::Duration::from_millis(4_321));
    }

    /// Rejects URL-embedded credentials so management authentication has exactly one bearer-token source.
    /// 拒绝 URL 内嵌凭据，确保管理鉴权只有一个 Bearer Token 来源。
    #[test]
    fn external_management_target_rejects_url_credentials() {
        let layer = vulcan_vmm::VmmConfigLayer {
            management_base_url: Some(String::from("https://user:secret@example.com")),
            management_bearer_token: Some(String::from("management-token")),
            ..vulcan_vmm::VmmConfigLayer::default()
        };
        assert!(external_management_target_from_layer(&layer).is_none());
    }

    /// Rejects an out-of-contract timeout even when persisted state bypassed the settings form.
    /// 即使持久化状态绕过设置表单，也拒绝超出契约范围的超时值。
    #[test]
    fn external_management_target_rejects_invalid_timeout() {
        let layer = vulcan_vmm::VmmConfigLayer {
            management_base_url: Some(String::from("https://memory.example.test")),
            management_bearer_token: Some(String::from("management-token")),
            management_timeout_ms: Some(999),
            ..vulcan_vmm::VmmConfigLayer::default()
        };
        assert!(external_management_target_from_layer(&layer).is_none());
    }

    /// Builds one deterministic embedding-space fingerprint for lifecycle tests.
    /// 为生命周期测试构建一个确定性向量空间指纹。
    ///
    /// # Returns
    /// 返回值
    /// A complete non-secret fingerprint covering account, route, model, and vector semantics.
    /// 返回覆盖账号、路由、模型与向量语义的完整非秘密指纹。
    fn test_embedding_fingerprint() -> EmbeddingSpaceFingerprint {
        EmbeddingSpaceFingerprint {
            provider_id: String::from("provider"),
            service_entry_id: String::from("entry"),
            deployment_id: Some(String::from("credential")),
            secondary_route_id: None,
            model_id: String::from("embedding-model"),
            binding_id: String::from("embedding-binding"),
            capability_revision: String::from("revision"),
            task: EmbeddingTask::ProviderDefault,
            dimensions: 1024,
            output_kind: EmbeddingOutputKind::Dense,
            encoding: EmbeddingEncoding::Float,
            normalization: EmbeddingNormalization::L2,
            distance_metric: DistanceMetric::Cosine,
        }
    }
}
