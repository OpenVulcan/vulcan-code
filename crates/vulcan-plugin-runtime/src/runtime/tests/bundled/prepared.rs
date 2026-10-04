//! Real bundled dependencies complete the Lua, managed-process and Hook migration checks.
//! 真实内置依赖补齐 Lua、受管进程和 Hook 的迁移检查。

use super::super::test_manager_with_distribution;
use crate::{
    VulcanPluginActionCaller, VulcanPluginActionRequest, VulcanPluginInvocationRequest,
    VulcanPluginRuntimeManager, VulcanPluginRuntimeSettings, VulcanPluginRuntimeState,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use vulcan_hooks::{
    HookActor, HookCapabilitySet, HookContext, HookInvocation, HookMode, HookPayload,
    HookRegistration, HookScope,
};

/// One finite test deadline bounds transport shutdown and explicit runtime closure.
/// 单个有限测试期限约束传输关闭及显式运行时关闭。
const TEST_DEADLINE: Duration = Duration::from_secs(15);

/// Own isolated state while reading the repository's explicitly prepared bundled distribution.
/// 拥有隔离状态，同时读取仓库明确准备的内置发行依赖。
struct PreparedFixture {
    /// Manager must drop before its temporary state directory is removed.
    /// 管理器必须先于临时状态目录删除而释放。
    manager: VulcanPluginRuntimeManager,
    /// Source identity also supplies the authorized test workspace.
    /// 源码身份也提供已授权的测试工作区。
    repository: PathBuf,
    /// Keep configuration, generation directories and operation journals alive for the test.
    /// 在测试期间保留配置、代次目录及操作日志。
    _root: tempfile::TempDir,
}

impl PreparedFixture {
    /// Open prepared dependencies without downloading; return the formally attached production manager.
    /// 打开已准备依赖，不执行下载；返回已附加正式运行时的生产管理器。
    fn new() -> Self {
        // Source identity is independent from the shell working directory.
        // 源码身份独立于终端工作目录。
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("repository root")
            .to_path_buf();
        // This is the documented make sync distribution, never a guessed system Node executable.
        // 此处为文档规定的 make sync 发行目录，绝非猜测的系统 Node 程序。
        let distribution = repository.join("output/dependencies/runtimes");
        assert!(
            distribution.is_dir(),
            "Run make init-system and make sync before bundled integration tests."
        );
        // State ownership remains private even when dependency artifacts are shared for read access.
        // 即使依赖产物共享读取，状态所有权仍保持私有。
        let root = tempfile::tempdir().expect("isolated prepared test state");
        // Private engine root for the trusted bridge.
        // 可信桥使用的私有引擎根。
        let runtime_root = root.path().join("runtime");
        std::fs::create_dir_all(&runtime_root).expect("private Lua runtime");
        // Read synchronized packages while keeping mutable execution state private.
        // 读取已同步插件包，同时将可变执行状态保持私有。
        let manager = test_manager_with_distribution(
            &runtime_root,
            repository.join("output/plugins"),
            distribution,
        );
        manager
            .initialize_formal_runtime(VulcanPluginRuntimeSettings::default())
            .expect("formal parent before version-two catalog");
        Self {
            manager,
            repository,
            _root: root,
        }
    }

    /// Build an owner-bound real debug invocation from the exact tool identity and arguments.
    /// 根据精确工具身份及参数构建绑定所有者的真实调试调用。
    /// Return a request whose workspace and controller owner cannot be chosen by Lua.
    /// 返回工作区及控制器所有者均不能由 Lua 选择的请求。
    fn debug_request(&self, tool: &str, arguments: Value) -> VulcanPluginInvocationRequest {
        VulcanPluginInvocationRequest {
            tool_id: tool.into(),
            arguments,
            owner_session_id: "bundled-debug-owner".into(),
            workspace_root: self.repository.clone(),
        }
    }

    /// Close all original domains and prove that their real resource ownership has finished.
    /// 关闭全部原始执行域，并证明其真实资源所有权已结束。
    async fn close(&self) {
        self.manager
            .request_formal_runtime_close()
            .expect("request complete owner shutdown");
        tokio::time::timeout(TEST_DEADLINE, async {
            while !self
                .manager
                .poll_formal_runtime_closed()
                .expect("observe shutdown")
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("bundled runtime releases its resources");
    }
}

/// Run one real browser action and both screenshot Hooks under version-two capability admission.
/// 在第二版能力入场校验下运行真实浏览器 Action 及两个截图 Hook。
#[tokio::test(flavor = "current_thread")]
#[ignore = "Requires make init-system and make sync for the pinned Node and browser package."]
async fn bundled_prepared_browser_actions_and_hooks_preserve_contract() {
    // Enabling remains an explicit host policy; the shipped default is still disabled.
    // 启用仍是显式宿主策略；随附默认值仍为停用。
    let fixture = PreparedFixture::new();
    // Resolve the production catalog under the explicit host enable policy.
    // 按明确宿主启用策略解析生产目录。
    let catalog = fixture
        .manager
        .refresh_catalog(
            &BTreeSet::new(),
            &BTreeSet::from(["vulcan-browser".to_string()]),
        )
        .expect("prepared browser catalog");
    // Require the real descriptor to be executable before exercising its operations.
    // 执行操作前要求真实描述符可执行。
    let plugin = catalog
        .plugins
        .get("vulcan-browser")
        .expect("real browser descriptor");
    assert_eq!(
        plugin.runtime_state,
        VulcanPluginRuntimeState::Enabled,
        "{:?}",
        plugin.diagnostics
    );
    assert_eq!(
        plugin
            .manifest
            .as_ref()
            .expect("real manifest")
            .schema_version,
        crate::VULCAN_PLUGIN_MANIFEST_SCHEMA_VERSION
    );
    fixture
        .manager
        .prewarm_plugin_runtime("vulcan-browser")
        .await
        .expect("real browser bundle initialization");
    // Read the production defaults through the native callback and update only a declared setting.
    // 通过原生回调读取生产默认值，并仅更新已声明设置。
    let original = fixture
        .manager
        .invoke_action(VulcanPluginActionRequest {
            plugin_id: "vulcan-browser".into(),
            action_id: "settings-get".into(),
            arguments: json!({}),
            caller: VulcanPluginActionCaller::PluginUi,
            workspace_root: fixture.repository.clone(),
            reuse_enabled_runtime: true,
        })
        .await
        .expect("browser settings read");
    // Preserve other settings while exercising the native update callback.
    // 执行原生更新回调时保留其他设置。
    let mut changed = original.clone();
    changed["actionTimeoutMs"] = json!("6000");
    // Capture the complete returned configuration for comparison.
    // 捕获返回的完整配置用于比较。
    let updated = fixture
        .manager
        .invoke_action(VulcanPluginActionRequest {
            plugin_id: "vulcan-browser".into(),
            action_id: "settings-update".into(),
            arguments: changed.clone(),
            caller: VulcanPluginActionCaller::PluginUi,
            workspace_root: fixture.repository.clone(),
            reuse_enabled_runtime: true,
        })
        .await
        .expect("browser settings update");
    assert_eq!(updated, changed);
    // The registration Hook reads the same updated setting through the same original native authority.
    // 注册 Hook 通过同一原始原生权威读取相同的已更新设置。
    let registration = catalog
        .enabled_hooks
        .get("vulcan-plugin/vulcan-browser/screenshot-registration")
        .expect("real screenshot registration");
    // Capture the actual Lua Hook projection through the production bridge.
    // 捕获经生产桥返回的真实 Lua Hook 投影。
    let transformed = fixture.manager.invoke_hook(registration, browser_invocation(
        &registration.registration_id, "tool.registration", HookScope::Global, &fixture.repository,
        json!({"definition":{"description":"Take a screenshot.","parameters_schema":{"type":"object","properties":{"filename":{"type":"string"}}}}})
    )).await.expect("real registration Hook").payload.expect("transformed definition").value;
    assert!(
        transformed["definition"]["description"]
            .as_str()
            .expect("description")
            .contains("6000 ms")
    );
    assert_eq!(
        transformed["definition"]["parameters_schema"]["additionalProperties"],
        false
    );
    // The session Hook removes only filename, preserving the original MCP operation arguments.
    // 会话 Hook 仅移除 filename，保留原 MCP 操作参数。
    let before = catalog
        .enabled_hooks
        .get("vulcan-plugin/vulcan-browser/screenshot-before")
        .expect("real screenshot before Hook");
    // Retain the output object to verify that only filename was removed.
    // 保留输出对象，验证仅 filename 被移除。
    let arguments = fixture
        .manager
        .invoke_hook(
            before,
            browser_invocation(
                &before.registration_id,
                "tool.before",
                HookScope::Session,
                &fixture.repository,
                json!({"arguments":{"filename":"ignored.png","fullPage":true}}),
            ),
        )
        .await
        .expect("real before Hook")
        .payload
        .expect("transformed arguments")
        .value;
    assert_eq!(arguments["arguments"], json!({"fullPage":true}));
    fixture.close().await;
}

/// Construct the exact trusted browser Hook frame; return a production invocation with the supplied payload.
/// 构建精确可信浏览器 Hook 帧；返回携带所给载荷的生产调用。
/// registration, point and scope select the catalog route; workspace remains host-owned.
/// registration、point 和 scope 选择目录路由；workspace 保持宿主持有。
fn browser_invocation(
    registration: &str,
    point: &str,
    scope: HookScope,
    workspace: &Path,
    payload: Value,
) -> HookInvocation {
    HookInvocation {
        registration: HookRegistration::new(registration, point, HookMode::Transform),
        context: HookContext {
            run_id: uuid::Uuid::new_v4().to_string(),
            point: point.into(),
            scope,
            actor: HookActor::new("bundled-migration-test"),
            workspace_id: Some(workspace.to_string_lossy().into_owned()),
            project_id: None,
            session_id: Some("browser-owner".into()),
            conversation_id: Some("browser-owner".into()),
            user_id: None,
            capabilities: HookCapabilitySet::default(),
            metadata: BTreeMap::from([("workspace_root".into(), json!(workspace))]),
        },
        payload: HookPayload { value: payload },
    }
}

/// Run the actual Lua bundle, managed Node sidecar and DAP connection through the public tool/session route.
/// 通过公开工具及会话路径运行真实 Lua 包、受管 Node 进程及 DAP 连接。
#[tokio::test(flavor = "current_thread")]
#[ignore = "Requires make init-system and make sync for the pinned managed Node distribution."]
async fn bundled_prepared_debug_session_uses_real_sidecar_and_releases_resources() {
    // The ephemeral loopback peer belongs only to this test; no user's debug session is contacted.
    // 临时回环对端仅属于当前测试；不连接任何用户调试会话。
    let fixture = PreparedFixture::new();
    // Resolve the production catalog under the explicit host enable policy.
    // 按明确宿主启用策略解析生产目录。
    let catalog = fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("debug catalog");
    // Require the real descriptor to be executable before exercising its operations.
    // 执行操作前要求真实描述符可执行。
    let plugin = catalog
        .plugins
        .get("vulcan-debug-runtime")
        .expect("real debug descriptor");
    assert_eq!(
        plugin
            .manifest
            .as_ref()
            .expect("synced debug manifest")
            .schema_version,
        crate::VULCAN_PLUGIN_MANIFEST_SCHEMA_VERSION,
        "Run make sync after updating the authoritative plugin manifests."
    );
    assert_eq!(
        plugin.runtime_state,
        VulcanPluginRuntimeState::Enabled,
        "{:?}",
        plugin.diagnostics
    );
    // Ask the operating system for a private unused loopback port.
    // 要求操作系统提供私有空闲回环端口。
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("isolated DAP peer");
    // Use the exact bound port as the real sidecar's DAP endpoint.
    // 使用精确绑定端口作为真实 sidecar 的 DAP 端点。
    let port = listener.local_addr().expect("bound peer address").port();
    // Completion proves the original sidecar transport has closed.
    // 任务完成证明原始 sidecar 传输已关闭。
    let peer = tokio::spawn(serve_dap(listener));
    // Open, call and close all use one original pinned VM and one actual sidecar process.
    // 打开、调用及关闭均使用同一个原始固定 VM 和一个真实 sidecar 进程。
    let opened = fixture
        .manager
        .invoke(fixture.debug_request(
            "debug_session_open",
            json!({"protocol":"dap","target":{"host":"127.0.0.1","port":port}}),
        ))
        .await
        .expect("open actual managed debug session");
    assert_eq!(opened["result"]["connected"], true);
    // The host-created ID binds later calls to the original pinned VM.
    // 宿主创建的 ID 将后续调用绑定到原始固定 VM。
    let session_id = opened["session_id"]
        .as_str()
        .expect("host session identity")
        .to_string();
    for command in ["initialize", "evaluate"] {
        // Distinct replies prove requests traversed the live original sidecar.
        // 不同响应证明请求经过存活的原始 sidecar。
        let result = fixture
            .manager
            .invoke(fixture.debug_request(
                "debug_session_request",
                json!({"session_id":session_id,"command":command,"arguments":{},"timeout_ms":2000}),
            ))
            .await
            .expect("request through Lua and managed Node");
        assert_eq!(result["result"]["echoed"], command);
        assert_eq!(result["session_id"], session_id);
    }
    // Another controller cannot claim the live debug process merely by copying its public ID.
    // 其他控制器不能仅复制公开 ID 就占用存活调试进程。
    let mut foreign =
        fixture.debug_request("debug_session_status", json!({"session_id":session_id}));
    foreign.owner_session_id = "foreign-owner".into();
    assert!(fixture.manager.invoke(foreign).await.is_err());
    // The original authorized owner remains valid after the rejected foreign claim.
    // 拒绝外部冒用后，原始授权所有者仍有效。
    let status = fixture
        .manager
        .invoke(fixture.debug_request("debug_session_status", json!({"session_id":session_id})))
        .await
        .expect("original debug owner remains valid");
    assert_eq!(status["result"]["connected"], true);
    // The declared close tool must retire the original protocol and host session.
    // 声明的关闭工具必须退役原始协议及宿主会话。
    let closed = fixture
        .manager
        .invoke(fixture.debug_request("debug_session_close", json!({"session_id":session_id})))
        .await
        .expect("close original sidecar and VM");
    assert_eq!(closed["closed"], true);
    assert_eq!(closed["result"]["closed"], true);
    assert!(
        fixture
            .manager
            .formal_sessions("bundled-debug-owner")
            .expect("retired sessions")
            .is_empty()
    );
    assert_eq!(
        fixture
            .manager
            .active_session_count("vulcan-debug-runtime")
            .expect("actual live reservation count"),
        0
    );
    assert_eq!(
        tokio::time::timeout(TEST_DEADLINE, peer)
            .await
            .expect("peer sees socket closure")
            .expect("peer task")
            .expect("peer protocol"),
        ["initialize", "evaluate"]
    );
    fixture.close().await;
}

/// Serve the supplied listener's single DAP client and return observed commands when that real socket closes.
/// 服务所给 listener 的单个 DAP 客户端，并在真实套接字关闭时返回已观测命令。
/// Only bounded test frames are accepted; framing errors return diagnostics rather than hanging the test.
/// 仅接受有界测试帧；分帧错误返回诊断，不使测试挂起。
async fn serve_dap(listener: tokio::net::TcpListener) -> Result<Vec<String>, String> {
    // The accepted transport belongs to the actual managed Node sidecar.
    // 已接受传输属于真实受管 Node sidecar。
    let (stream, _) = listener.accept().await.map_err(|error| error.to_string())?;
    // Buffer only the accepted sidecar connection.
    // 仅缓冲已接受的 sidecar 连接。
    let mut reader = BufReader::new(stream);
    // Preserve observed order independently from the test's expected sequence.
    // 独立于测试预期序列保留已观测顺序。
    let mut commands = Vec::new();
    loop {
        // Sidecar output has one exact Content-Length header and one separating empty line.
        // Sidecar 输出具有单个精确 Content-Length 头及一个分隔空行。
        let mut header = String::new();
        if reader
            .read_line(&mut header)
            .await
            .map_err(|error| error.to_string())?
            == 0
        {
            return Ok(commands);
        }
        // The exact DAP header controls bounded body allocation.
        // 精确 DAP 消息头决定有界消息体分配。
        let length: usize = header
            .strip_prefix("Content-Length: ")
            .ok_or("missing DAP content length")?
            .trim()
            .parse()
            .map_err(|_| "invalid DAP content length")?;
        if length > 4096 {
            return Err("test DAP frame exceeds bounded fixture size".into());
        }
        // Require the exact protocol separator before reading the body.
        // 读取消息体前要求精确协议分隔行。
        let mut separator = String::new();
        reader
            .read_line(&mut separator)
            .await
            .map_err(|error| error.to_string())?;
        if separator != "\r\n" {
            return Err("invalid DAP header separator".into());
        }
        // Allocate only the previously validated frame size.
        // 仅分配已校验的帧大小。
        let mut body = vec![0; length];
        reader
            .read_exact(&mut body)
            .await
            .map_err(|error| error.to_string())?;
        // Decode one complete sidecar request from its declared frame.
        // 从声明的帧解码一条完整 sidecar 请求。
        let request: Value = serde_json::from_slice(&body).map_err(|error| error.to_string())?;
        // Use the received command as the response and observation identity.
        // 使用收到的命令作为响应及观测身份。
        let command = request["command"]
            .as_str()
            .ok_or("DAP request command missing")?
            .to_string();
        commands.push(command.clone());
        // Echo correlation data independently from the host's outer plugin session ID.
        // 独立于宿主外层插件会话 ID 回传关联数据。
        let response = serde_json::to_vec(&json!({
            "seq":commands.len(),"type":"response","request_seq":request["seq"],
            "command":command,"success":true,"body":{"echoed":command}
        }))
        .map_err(|error| error.to_string())?;
        reader
            .get_mut()
            .write_all(format!("Content-Length: {}\r\n\r\n", response.len()).as_bytes())
            .await
            .map_err(|error| error.to_string())?;
        reader
            .get_mut()
            .write_all(&response)
            .await
            .map_err(|error| error.to_string())?;
    }
}
