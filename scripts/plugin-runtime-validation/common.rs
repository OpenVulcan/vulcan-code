//! Bounded complete-host workload shared by both independent consumers.
//! 两个独立消费者共用的有限完整宿主负载。

use luaskills::{LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::{BufWriter, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use vulcan_plugin_runtime::{
    VulcanPluginActionCaller, VulcanPluginActionRequest, VulcanPluginHostServices,
    VulcanPluginInvocationRequest, VulcanPluginManifest, VulcanPluginRuntimeManager,
    VulcanPluginUiEventPublisher, VulcanPluginUiInvalidation,
};

/// Propagate explicit host, filesystem and contract failures to the process exit status.
/// 将明确的宿主、文件系统及契约错误传递到进程退出状态。
pub type Failure = Box<dyn std::error::Error + Send + Sync>;

/// Sole cap for the recent successful latency samples retained by a soak run.
/// 长稳运行保留的最近成功时延样本唯一上限。
const SOAK_LATENCY_SAMPLE_LIMIT: usize = 4096;

/// Sole ordered mixed-workload cycle; selection derives its period from this explicit declaration.
/// 唯一有序混合负载循环；选择周期从此显式声明派生。
const MIXED_WORKLOADS: &[&str] = &["hot", "slow_host", "compute", "io"];

/// Exact production plugin whose read-only UI settings action is measured.
/// 测量只读界面设置 Action 的精确生产插件。
pub const SETTINGS_GET_PLUGIN: &str = "vulcan-usage-statistics";

/// Sole declared round count for the bounded production action comparison.
/// 有限生产 Action 比较的唯一声明轮数。
const SETTINGS_GET_ROUNDS: usize = 3;

/// Sole sequential invocation count per production action round.
/// 每轮生产 Action 的唯一顺序调用数。
const SETTINGS_GET_CALLS: usize = 1000;

/// Explicit frozen production inputs; source files must also be passed to the existing evidence-file gate.
/// 显式冻结生产输入；源文件还必须传入既有 evidence-file 门禁。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsGetActionConfig {
    /// Exact original package directory supplying this consumer's unchanged manifest and required assets.
    /// 提供此消费者未变清单及必需资产的精确原包目录。
    pub package_source: PathBuf,
    /// One exact original bundle copied byte-for-byte into both consumers' private packages.
    /// 逐字节复制到两个消费者私有包的单个精确原 Bundle。
    pub bundle_source: PathBuf,
    /// Explicit capture or measurement stage; capture never invents an expected business result.
    /// 显式捕获或测量阶段；捕获绝不编造预期业务结果。
    validation: SettingsGetActionValidation,
}

/// Mutually exclusive finite production validation stages without an implicit expected-result fallback.
/// 互斥有限生产验证阶段，不隐式回退预期结果。
#[derive(Clone, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
enum SettingsGetActionValidation {
    /// Capture one actual schema-validated first action and close its original resources, without benchmark samples.
    /// 捕获一个实际通过 Schema 校验的首次 Action 并关闭原资源，不生成基准样本。
    Capture,
    /// Measure hot actions only against the complete result previously captured from both real consumers.
    /// 仅针对此前从两个真实消费者捕获的完整结果测量热态 Action。
    Measure {
        /// Complete actual captured projection, including every ordered catalog model and price.
        /// 完整实际捕获投影，包含每个有序目录模型及价格。
        expected_result: Value,
    },
}

/// Freeze one run's workload and deadlines from a required JSON file; no measurement defaults exist.
/// 从必需的 JSON 文件冻结单次负载及截止时间；不存在测量默认值。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Exact group: legacy, public_single, public_reusable, dedicated_reusable or formal_ui_action.
    /// 精确分组：legacy、public_single、public_reusable、dedicated_reusable 或 formal_ui_action。
    pub group: String,
    /// Exact workload: the original seven synthetic workloads or settings_get_action.
    /// 精确负载：原七种合成负载或 settings_get_action。
    pub workload: String,
    /// Explicit business inputs present only when workload declares settings_get_action.
    /// 仅 workload 声明 settings_get_action 时存在的显式业务输入。
    pub settings_get_action: Option<SettingsGetActionConfig>,
    /// Explicit worker concurrency; public invocation errors retain their actual unclassified diagnostics.
    /// 显式工作者并发；公开调用错误保留其真实未分类诊断。
    pub concurrency: usize,
    /// Minimum complete workload rounds.
    /// 最少完整负载轮数。
    pub rounds: usize,
    /// Sequential calls per stable worker in each round.
    /// 每轮每个稳定工作者顺序执行的调用数。
    pub calls_per_worker: usize,
    /// Minimum wall duration, enforced together with successful call count.
    /// 最短墙钟持续时间，与成功调用数量同时满足。
    pub minimum_duration_ms: u64,
    /// Minimum real successful invocation count; failed invocations never contribute.
    /// 最少真实成功调用数；失败调用不参与累计。
    pub minimum_calls: usize,
    /// Maximum wall duration; unmet minima fail instead of silently stopping.
    /// 最长墙钟持续时间；未满足最小要求时失败而非静默停止。
    pub maximum_duration_ms: u64,
    /// Explicit owner creation/invoke/drain cycles after the timed workload.
    /// 计时负载后的显式所有者创建、调用及排空循环数。
    pub owner_cycles: usize,
    /// Enforce the plan's two-hour, 100000-call and 1000-cycle soak minima.
    /// 强制计划的两小时、十万调用及一千循环长稳下限。
    pub soak: bool,
    /// Plugin-wide declared active session budget, identical in both manifests.
    /// 插件级声明活动会话预算，在两个清单中保持一致。
    pub max_active_sessions: usize,
    /// Exact operation timeout in both manifests.
    /// 两个清单中的精确操作超时。
    pub timeout_ms: u64,
    /// Deadline for observing physical cleanup, outside invocation latency.
    /// 观测物理清理的截止时间，位于调用时延之外。
    pub cleanup_timeout_ms: u64,
    /// Real native host callback delay for slow_host.
    /// slow_host 的真实原生宿主回调延迟。
    pub host_delay_ms: u64,
    /// Exact arithmetic loop count.
    /// 精确算术循环次数。
    pub compute_iterations: u64,
    /// Explicit arithmetic count executed during each actual module initialization.
    /// 每次真实模块初始化执行的显式算术次数。
    pub initialization_iterations: u64,
    /// Exact native fixture-file and JSON input/output byte count.
    /// 精确原生夹具文件及 JSON 输入输出字节数。
    pub io_bytes: usize,
    /// Explicit reusable module retirement TTL.
    /// 显式复用模块退役 TTL。
    pub reuse_idle_ttl_ms: u64,
    /// Explicit reusable module use limit.
    /// 显式复用模块使用次数上限。
    pub reuse_max_uses: u64,
    /// Same declared legacy engine pool options for both engine adapters.
    /// 两个引擎适配器使用相同声明的旧引擎池选项。
    pub engine_pool: LuaVmPoolConfig,
    /// Complete formal settings; old consumer records these without pretending to apply them.
    /// 完整正式设置；旧消费者记录它们，但不假称应用。
    pub formal_settings: Value,
}

/// Shared Lua business decodes every envelope itself; reuse hooks only verify and clear state.
/// 共用 Lua 业务自行解码每次信封；复用钩子只验证和清除状态。
const BUSINESS: &str = r#"
-- Module-local identity and hygiene survive only actual reuse.
-- 模块本地身份及卫生状态仅在实际复用时存活。
local M, tenant, workspace, active, count, sequence = {}, nil, nil, nil, 0, 0
-- Freeze the explicit initialization loop from the same host fixture configuration.
-- 从相同宿主夹具配置冻结显式初始化循环。
local initialization_iterations = __VALIDATION_INITIALIZATION_ITERATIONS__
-- Retain actual initialization arithmetic so its work cannot be replaced by a synthetic response.
-- 保留实际初始化算术结果，使其工作不能被合成响应替代。
local initialization_total = 0
-- Execute initialization once per original module, before any request dispatch or reuse hook.
-- 在任何请求分发或复用钩子之前，每个原始模块执行一次初始化。
for i = 1, initialization_iterations do initialization_total = (initialization_total + i) % 1000003 end
-- Verify no prior request remains; envelope is unused and return true permits entry.
-- 验证不存在残留请求；不使用 envelope，返回 true 允许进入。
function M.before_call(envelope) assert(active == nil); return true end
-- Decode envelope, verify stable partition and FIFO sequence, and return actual business values.
-- 解码 envelope，验证稳定分区及顺序序号，返回实际业务值。
function M.dispatch(envelope)
    -- Request belongs exclusively to this original envelope.
    -- 请求仅属于此原始信封。
    local request = vulcan.json.decode(envelope.payload_json)
    assert(request.op == 'run')
    if tenant == nil then tenant, workspace = request.tenant, request.workspace end
    assert(tenant == request.tenant and workspace == request.workspace, 'partition contamination')
    assert(request.sequence > sequence, 'non-FIFO worker sequence')
    sequence, active, count = request.sequence, request.sequence, count + 1
    -- Compute result uses deterministic integer-safe arithmetic.
    -- 计算结果使用确定性的整数安全算术。
    local total = 0
    -- Bind the concrete local value for this exact host operation.
    -- 为此精确宿主操作绑定具体本地值。
    for i = 1, request.iterations do total = (total + i) % 1000003 end
    if request.workload == 'slow_host' then
        vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='slow_host', revision=request.sequence})
    end
    if request.workload == 'io' then
        vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='io', revision=request.sequence})
    end
    return vulcan.json.encode({ok=true,result={tenant=tenant,workspace=workspace,sequence=sequence,count=count,total=total,initialization_total=initialization_total,blob=request.blob}})
end
-- Check the exact finished envelope and clear state; succeeded reports Lua completion.
-- 检查精确完成信封并清除状态；succeeded 表示 Lua 完成。
function M.after_call(envelope, succeeded)
    assert(succeeded and active == vulcan.json.decode(envelope.payload_json).sequence)
    active = nil
    return true
end
-- Return the actual hygiene state without synthesizing a business response.
-- 返回真实卫生状态，不合成业务响应。
function M.reuse_check() return active == nil end
-- Validate host shutdown envelope and return a normal graceful-close response.
-- 验证宿主关闭信封并返回正常优雅关闭响应。
function M.shutdown(envelope)
    assert(vulcan.json.decode(envelope.payload_json).op == 'shutdown')
    return '{"ok":true,"result":{}}'
end
return M
"#;

/// Execute frozen native delays and exact fixture-file I/O through the existing authorized UI callback.
/// 通过现有授权 UI 回调执行冻结原生延迟及精确夹具文件 I/O。
struct SlowHost {
    /// Frozen delay for this run's real callback.
    /// 本轮真实回调的冻结延迟。
    delay: Duration,
    /// Exact fixture file chosen by the host before invoking any plugin business.
    /// 调用任何插件业务前由宿主选择的精确夹具文件。
    io_path: PathBuf,
    /// Frozen bytes compared with every actual native filesystem read.
    /// 与每次实际原生文件系统读取比较的冻结字节。
    io_expected: Vec<u8>,
}
impl VulcanPluginUiEventPublisher for SlowHost {
    /// Validate event identity; execute its declared native delay or fixture-file read, returning acknowledgement/error.
    /// 验证 event 身份；执行其声明的原生延迟或夹具文件读取，返回确认或错误。
    fn publish(&self, event: VulcanPluginUiInvalidation) -> Result<(), String> {
        if event.plugin_id != "validation-plugin" {
            return Err("Unexpected validation host event.".into());
        }
        match event.topic.as_str() {
            "slow_host" => std::thread::sleep(self.delay),
            "io" => {
                // Read this exact host-owned fixture on every callback; OS cache behavior remains real.
                // 每次回调读取此精确宿主持有夹具；操作系统缓存行为保持真实。
                let bytes = fs::read(&self.io_path).map_err(|error| error.to_string())?;
                if bytes != self.io_expected {
                    return Err(
                        "Native fixture-file bytes differ from the frozen I/O input.".into(),
                    );
                }
            }
            _ => return Err("Unexpected validation host event topic.".into()),
        }
        Ok(())
    }
}

/// Build engine from cfg and root using the verified native path helper; return the actual engine.
/// 使用已核实原生路径辅助函数从 cfg 及 root 构建引擎；返回真实引擎。
pub fn engine(cfg: &Config, root: &Path) -> Result<LuaEngine, Failure> {
    // The existing engine constructor returns a non-Send boxed error; retain its actual diagnostic before worker transfer.
    // 既有引擎构造器返回不可发送的盒装错误；在工作者传递前保留其真实诊断。
    Ok(LuaEngine::new(LuaEngineOptions::new(
        cfg.engine_pool,
        LuaRuntimeHostOptions::with_runtime_root(vulcan_runtime_assets::native_host_path(
            &root.canonicalize()?,
        )),
    ))
    .map_err(|error| error.to_string())?)
}

/// Write the common package under root, varying only required schema version and reuse declaration.
/// 在 root 下写入共同包，仅改变必需的 Schema 版本及复用声明。
/// cfg freezes limits; root owns the package; formal_protocol_version declares the caller-verified protocol.
/// cfg 冻结上限，root 持有包目录，formal_protocol_version 声明调用方已核实的协议版本。
/// Return the trusted package root, or propagate package creation and serialization errors.
/// 返回受信任包根，或传递包创建及序列化错误。
pub fn package(
    cfg: &Config,
    root: &Path,
    formal_protocol_version: Option<u32>,
) -> Result<PathBuf, Failure> {
    if cfg.workload == "settings_get_action" {
        return settings_get_package(cfg, root);
    }
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let bundled = root.join("bundled");
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let package = bundled.join("validation-plugin");
    fs::create_dir_all(package.join("runtime"))?;
    fs::write(
        package.join("runtime/plugin.bundle.lua"),
        BUSINESS.replace(
            "__VALIDATION_INITIALIZATION_ITERATIONS__",
            &cfg.initialization_iterations.to_string(),
        ),
    )?;
    fs::write(package.join("dependencies.yaml"), "{}\n")?;
    fs::write(
        package.join("input.json"),
        r#"{"type":"object","additionalProperties":false,"required":["tenant","workspace","sequence","workload","iterations","blob"],"properties":{"tenant":{"type":"string"},"workspace":{"type":"string"},"sequence":{"type":"integer"},"workload":{"type":"string"},"iterations":{"type":"integer"},"blob":{"type":"string"}}}"#,
    )?;
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let mut manifest = json!({"schema_version":vulcan_plugin_runtime::VULCAN_PLUGIN_MANIFEST_SCHEMA_VERSION,"id":"validation-plugin","version":"1.0.0","default_enabled":true,"display":{"name":"Validation Plugin","capability":"Complete host validation"},"permissions":["ui_host_context"],"execution":{"dependencies":{"manifest":"dependencies.yaml","install_mode":"bundled"}},"lua_runtime":{"bundle":"runtime/plugin.bundle.lua","invoke_function":"dispatch","shutdown_function":"shutdown","max_active_sessions":cfg.max_active_sessions,"tools":[{"id":"validation_run","group":"validation_tools","description":"Execute the frozen complete-host validation workload.","parameters_file":"input.json","execution":{"kind":"stateless","operation":"run","timeout_ms":cfg.timeout_ms}}]}});
    if let Some(protocol_version) = formal_protocol_version {
        manifest["runtime_requirements"] = json!({"protocol_version":protocol_version,"backend":"in_process","core_capabilities":["capacity_groups_v1"],"capacities":[]});
        if cfg.group != "public_single" {
            manifest["lua_runtime"]["tools"][0]["execution"]["reusable"] = json!({"before_call":"before_call","after_call":"after_call","reuse_check":"reuse_check","ordering":"key_serial","max_resident_vms":1,"max_running_calls":1,"idle_ttl_ms":cfg.reuse_idle_ttl_ms,"max_uses":cfg.reuse_max_uses});
        }
    }
    fs::write(
        package.join("plugin.yaml"),
        serde_yaml::to_string(&manifest)?,
    )?;
    Ok(bundled)
}

/// Copy cfg's exact production package into root without rewriting its manifest or executing source.
/// 将 cfg 的精确生产包复制到 root，不重写清单或执行源码。
/// Return private bundled ownership; reject missing inputs, escaping files and mismatched declared limits.
/// 返回私有内建包归属；拒绝输入缺失、文件逃逸及声明上限不匹配。
fn settings_get_package(cfg: &Config, root: &Path) -> Result<PathBuf, Failure> {
    // This optional field is selected exclusively by the explicit workload, never by path discovery.
    // 此可选字段仅由显式负载选择，绝不根据路径探测选择。
    let inputs = cfg
        .settings_get_action
        .as_ref()
        .ok_or("Missing settings_get_action inputs.")?;
    if !inputs.package_source.is_absolute() || !inputs.bundle_source.is_absolute() {
        return Err("Production package and common bundle require explicit absolute paths.".into());
    }
    // Resolve the one declared package so every required original asset has a checked physical owner.
    // 解析单个声明包，使每个必需原资产具有已检查物理所有者。
    let source = inputs.package_source.canonicalize()?;
    // Parse the same public manifest type that the actual catalog subsequently validates.
    // 解析随后实际目录校验的相同公开清单类型。
    let manifest: VulcanPluginManifest =
        serde_yaml::from_slice(&fs::read(source.join("plugin.yaml"))?)?;
    // The production declaration determines its real bundle, session limit and action timeout.
    // 生产声明决定真实 Bundle、会话上限及 Action 超时。
    let lua = manifest
        .lua_runtime
        .as_ref()
        .ok_or("Production Lua declaration is absent.")?;
    // Select only the known actual action; no other function or schema is a fallback.
    // 仅选择已知实际 Action；其他函数或 Schema 均不是回退。
    let action = lua
        .actions
        .iter()
        .find(|action| action.id == "settings-get")
        .ok_or("Production settings-get declaration is absent.")?;
    if manifest.id != SETTINGS_GET_PLUGIN
        || lua.bundle != "runtime/plugin.bundle.lua"
        || lua.max_active_sessions != cfg.max_active_sessions
        || action.timeout_ms != cfg.timeout_ms
        || manifest.execution.dependencies.manifest.as_deref() != Some("dependencies.yaml")
    {
        return Err(
            "Frozen production identity, bundle or limits differ from its actual manifest.".into(),
        );
    }
    // Copy only actual required assets and original admission inputs into the newly private package.
    // 仅将实际必需资产及原入场输入复制到新私有包。
    let mut files: BTreeSet<_> = manifest.required_files.iter().map(String::as_str).collect();
    files.extend(["plugin.yaml", "dependencies.yaml"]);
    // Both consumers expose only this real production package to their catalog.
    // 两个消费者仅将此真实生产包暴露给各自目录。
    let bundled = root.join("bundled");
    // Package identity stays unchanged while every writable location belongs to this temporary root.
    // 包身份保持不变，同时每个可写位置均属于此临时根。
    let package = bundled.join(SETTINGS_GET_PLUGIN);
    for name in files {
        if Path::new(name)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err("Production required file is not package-relative.".into());
        }
        // Reject symbolic-link escape before reading any original required file.
        // 读取任何原必需文件前拒绝符号链接逃逸。
        let original = source.join(name).canonicalize()?;
        if !original.starts_with(&source) || !original.is_file() {
            return Err("Production required file escapes its original package.".into());
        }
        // Preserve original bytes instead of serializing or adapting the actual manifest and schemas.
        // 保留原字节，不序列化或适配实际清单及 Schema。
        let destination = package.join(name);
        fs::create_dir_all(
            destination
                .parent()
                .ok_or("Required file parent is absent.")?,
        )?;
        fs::write(destination, fs::read(original)?)?;
    }
    fs::create_dir_all(package.join("runtime"))?;
    fs::write(package.join(&lua.bundle), fs::read(&inputs.bundle_source)?)?;
    Ok(bundled)
}

/// Prepare root's fixed I/O file, refresh manager and install cfg's shared native callback; reject invalid state.
/// 准备 root 的固定 I/O 文件，刷新 manager 并安装 cfg 的共用原生回调；拒绝无效状态。
/// Returns unit only after the public tool is enabled.
/// 仅在公开工具启用后返回空值。
pub fn activate(
    manager: &VulcanPluginRuntimeManager,
    cfg: &Config,
    root: &Path,
) -> Result<(), Failure> {
    // Freeze exact host-owned bytes before any measured Lua business enters the native callback.
    // 在任何测量 Lua 业务进入原生回调之前冻结精确宿主持有字节。
    let io_expected = vec![b'x'; cfg.io_bytes];
    // Resolve one fixture path from the already-created validation root, never from plugin arguments.
    // 从已经创建的验证根解析单个夹具路径，绝不从插件参数解析。
    let io_path = root.join("host-io.bin");
    fs::write(&io_path, &io_expected)?;
    manager.install_host_services(VulcanPluginHostServices {
        storage: None,
        vldb: None,
        ui_events: Some(Arc::new(SlowHost {
            delay: Duration::from_millis(cfg.host_delay_ms),
            io_path,
            io_expected,
        })),
    })?;
    manager.refresh_catalog(&BTreeSet::new(), &BTreeSet::new())?;
    if !manager
        .snapshot()
        .enabled_tools
        .contains_key("validation_run")
    {
        return Err(format!("Validation package rejected: {:?}", manager.snapshot()).into());
    }
    Ok(())
}

/// Invoke the full public manager route with cfg/owner/workspace/sequence; return measured result.
/// 使用 cfg、owner、workspace、sequence 调用完整公开管理器路由；返回测量结果。
/// The clock surrounds only manager.invoke; correctness validation and record I/O occur afterwards.
/// 计时仅包围 manager.invoke；正确性验证及记录 I/O 位于之后。
async fn call(
    manager: &VulcanPluginRuntimeManager,
    cfg: &Config,
    owner: &str,
    workspace: &Path,
    sequence: usize,
) -> Result<(u128, Value), Failure> {
    // Every stable worker rotates the same declared four workloads by its real one-based sequence.
    // 每个稳定工作者按真实从一开始的序号轮转相同声明的四种负载。
    let workload = if cfg.workload == "mixed" {
        MIXED_WORKLOADS[(sequence - 1) % MIXED_WORKLOADS.len()]
    } else {
        cfg.workload.as_str()
    };
    // Allocate arithmetic work only for the actual selected workload, including mixed calls.
    // 仅为实际选定负载分配算术工作，包含混合调用。
    let iterations = if workload == "compute" {
        cfg.compute_iterations
    } else {
        0
    };
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let blob = if workload == "io" {
        "x".repeat(cfg.io_bytes)
    } else {
        String::new()
    };
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let workspace_marker = workspace.to_string_lossy().into_owned();
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let request = VulcanPluginInvocationRequest {
        tool_id: "validation_run".into(),
        arguments: json!({"tenant":owner,"workspace":workspace_marker,"sequence":sequence,"workload":workload,"iterations":iterations,"blob":blob}),
        owner_session_id: owner.into(),
        workspace_root: workspace.into(),
    };
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let started = Instant::now();
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let result = manager.invoke(request).await;
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let elapsed = started.elapsed().as_micros();
    match result {
        Ok(value) => {
            // Bind the concrete local value for this exact host operation.
            // 为此精确宿主操作绑定具体本地值。
            let expected = ((iterations as u128 * (iterations as u128 + 1) / 2) % 1_000_003) as u64;
            // Validate the module's real initialization independently of request-time arithmetic.
            // 独立于请求阶段算术校验模块真实初始化。
            let initialization_expected = ((cfg.initialization_iterations as u128
                * (cfg.initialization_iterations as u128 + 1)
                / 2)
                % 1_000_003) as u64;
            if value["tenant"] != owner
                || value["workspace"] != workspace_marker
                || value["sequence"] != sequence
                || value["total"] != expected
                || value["initialization_total"] != initialization_expected
                || value["blob"] != blob
            {
                return Ok((
                    elapsed,
                    json!({"ok":false,"workload":workload,"classification":"validation","error":"Business identity/FIFO/initialization/output mismatch.","result":value}),
                ));
            }
            Ok((
                elapsed,
                json!({"ok":true,"workload":workload,"result":value}),
            ))
        }
        Err(error) => Ok((
            elapsed,
            json!({"ok":false,"workload":workload,"classification":"unclassified","error":error.to_string()}),
        )),
    }
}

/// Build the exact production PluginUi request for workspace; return no synthetic business input.
/// 为 workspace 构造精确生产 PluginUi 请求；不返回合成业务输入。
fn settings_get_request(workspace: &Path) -> VulcanPluginActionRequest {
    VulcanPluginActionRequest {
        plugin_id: SETTINGS_GET_PLUGIN.into(),
        action_id: "settings-get".into(),
        arguments: json!({}),
        caller: VulcanPluginActionCaller::PluginUi,
        workspace_root: workspace.into(),
        reuse_enabled_runtime: true,
    }
}

/// Inspect actual value's real modelId order after the original public action schema validation.
/// 原公开 Action Schema 校验后检查实际 value 的真实 modelId 顺序。
/// Return unit only for a nonempty strictly ascending production catalog.
/// 仅生产目录非空且严格升序时返回空值。
fn validate_settings_catalog(value: &Value) -> Result<(), Failure> {
    // Inspect actual returned rows before capture or comparison, without inventing a catalog result.
    // 在捕获或比较前检查实际返回行，不编造目录结果。
    let rows = value
        .get("catalogModels")
        .and_then(Value::as_array)
        .ok_or("Actual catalogModels is not an array.")?;
    // Preserve every actual model identity; malformed rows fail without choosing alternative fields.
    // 保留每个实际模型身份；畸形行直接失败，不选择替代字段。
    let ids = rows
        .iter()
        .map(|row| {
            row.get("modelId")
                .and_then(Value::as_str)
                .ok_or("Actual modelId is not a string.")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if ids.is_empty() || ids.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(
            "Actual production modelId catalog must be nonempty and strictly ascending.".into(),
        );
    }
    Ok(())
}

/// Run cfg's real settings-get action using raw configuration and a new output file.
/// 使用原配置 raw 及新输出文件运行 cfg 的真实 settings-get Action。
/// Return failure only after attempting public plugin drain and original parent close; initialization is untimed.
/// 仅在尝试公开插件排空及原父级关闭后返回失败；初始化不计时。
async fn run_settings_get_action(
    cfg: &Config,
    raw: Value,
    output_path: &Path,
) -> Result<(), Failure> {
    if cfg.concurrency != 1 || cfg.minimum_duration_ms != 0 || cfg.owner_cycles != 0 || cfg.soak {
        return Err("Production UI action requires one worker, zero minimum duration and no owner cycles/soak.".into());
    }
    // Required business sources are explicit and cannot be inherited from the synthetic fixture.
    // 必需业务来源明确，不能从合成夹具继承。
    let inputs = cfg
        .settings_get_action
        .as_ref()
        .ok_or("Missing production action inputs.")?;
    match &inputs.validation {
        SettingsGetActionValidation::Capture => {
            if cfg.rounds != 1 || cfg.calls_per_worker != 1 || cfg.minimum_calls != 0 {
                return Err(
                    "Capture requires one untimed initialization and no timed minimum calls."
                        .into(),
                );
            }
        }
        SettingsGetActionValidation::Measure { expected_result } => {
            if cfg.rounds != SETTINGS_GET_ROUNDS
                || cfg.calls_per_worker != SETTINGS_GET_CALLS
                || cfg.minimum_calls != SETTINGS_GET_ROUNDS * SETTINGS_GET_CALLS
            {
                return Err(
                    "Measurement requires exactly three rounds of 1000 hot action calls.".into(),
                );
            }
            validate_settings_catalog(expected_result)?;
        }
    }
    // The existing engine adapter and all manager-writable roots stay inside this newly owned directory.
    // 既有引擎适配器及全部管理器可写根均保持在此新拥有目录内。
    let root = tempfile::tempdir()?;
    // Supply an actual workspace to the unmodified public action admission path.
    // 向未修改公开 Action 入场路径提供实际工作区。
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace)?;
    // Construct the same original public manager used by the existing independent consumers.
    // 构造既有独立消费者使用的相同原公开管理器。
    let manager = crate::build(cfg, root.path())?;
    // Raw records preserve complete actual results outside the invocation clock.
    // 原记录在调用计时之外保留完整实际结果。
    let mut output = BufWriter::new(
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output_path)?,
    );
    // All samples belong to complete manager.invoke_action calls, never a direct Lua helper.
    // 全部样本属于完整 manager.invoke_action 调用，绝不是直接 Lua 辅助函数。
    let mut latencies = Vec::with_capacity(SETTINGS_GET_ROUNDS * SETTINGS_GET_CALLS);
    // Actual attempts remain distinct from schema-valid, business-validated successes.
    // 实际尝试与通过 Schema、业务校验的成功保持不同。
    let mut attempted = 0usize;
    // Only the untimed real invoke_action can prove that the UiAction instance was initialized.
    // 仅未计时真实 invoke_action 能证明 UiAction 实例已初始化。
    let mut initialized = false;
    // Only capture mode records this actual validated result; failed cleanup still makes the process fail.
    // 仅捕获模式记录此实际已校验结果；清理失败仍使进程失败。
    let mut captured_result = None;
    // Only measurement mode starts a hot-workload clock; capture has no benchmark duration.
    // 仅测量模式启动热态负载时钟；捕获不存在基准持续时间。
    let mut measurement_started = None;
    // Count rounds only after all their real hot actions pass complete result validation.
    // 仅在整轮全部真实热态 Action 通过完整结果校验后计数。
    let mut completed_rounds = 0usize;
    // Preserve the first business failure while still entering the original physical cleanup path.
    // 保留首个业务失败，同时仍进入原物理清理路径。
    let workload = async {
        manager.refresh_catalog(&BTreeSet::new(), &BTreeSet::new())?;
        writeln!(output, "{}", json!({"event":"configuration","config":raw,"runtime_kind":"UiAction","initial_status":crate::observe_action(&manager)?}))?;
        // Hook prewarm does not initialize UiAction; this is the original complete public first call.
        // Hook 预热不初始化 UiAction；此处执行原完整公开首次调用。
        let warmup = manager.invoke_action(settings_get_request(&workspace)).await?;
        initialized = true;
        validate_settings_catalog(&warmup)?;
        writeln!(output, "{}", json!({"event":"action_initialization","timed":false,"response":warmup,"status":crate::observe_action(&manager)?}))?;
        // These explicit stages have different acceptance inputs; neither uses a missing-value fallback.
        // 此两个显式阶段拥有不同验收输入；均不使用缺值回退。
        let SettingsGetActionValidation::Measure { expected_result } = &inputs.validation else {
            captured_result = Some(warmup);
            return Ok(());
        };
        if warmup != *expected_result {
            return Err("Initial production action differs from the actual captured expected_result.".into());
        }
        // The frozen maximum bounds only the declared hot rounds after actual initialization.
        // 冻结最长时间仅约束实际初始化后的声明热态轮次。
        let started = Instant::now();
        measurement_started = Some(started);
        // Each declared round uses the same actual retained UiAction and full production result.
        // 每个声明轮次使用相同实际保留 UiAction 及完整生产结果。
        for round in 0..SETTINGS_GET_ROUNDS {
            // Submit sequential hot actions without changing the original caller, workspace or initialization.
            // 顺序提交热态 Action，不改变原调用方、工作区或初始化。
            for ordinal in 0..SETTINGS_GET_CALLS {
                if started.elapsed() >= Duration::from_millis(cfg.maximum_duration_ms) {
                    return Err::<(), Failure>("Production action exceeded its frozen maximum duration.".into());
                }
                // Request construction precedes the clock; every production admission/callback/output check remains inside it.
                // 请求构造先于计时；每次生产入场、回调及输出检查均在计时内。
                let request = settings_get_request(&workspace);
                // Measure the complete original public route, including its real native config.get callback.
                // 测量完整原公开路由，包含真实原生 config.get 回调。
                let invoked = Instant::now();
                // Retain the actual result before measuring its complete public return.
                // 在测量完整公开返回之前保留实际结果。
                let result = manager.invoke_action(request).await;
                // Output recording and independent result comparison are outside this latency.
                // 输出记录及独立结果比较位于此时延之外。
                let elapsed = invoked.elapsed().as_micros();
                attempted += 1;
                match result {
                    Ok(value) => {
                        // Preserve invalid actual results while keeping them outside successful samples and accounting.
                        // 保留无效实际结果，同时将其排除于成功样本及计数之外。
                        let validation = validate_settings_catalog(&value).and_then(|()| {
                            if value == *expected_result {
                                Ok(())
                            } else {
                                Err("Complete production action differs from the actual captured expected_result.".into())
                            }
                        });
                        // Record failure is secondary to the original validation error, but still rejects an otherwise valid call.
                        // 记录失败次于原校验错误，但仍拒绝其他方面有效的调用。
                        let record = writeln!(output, "{}", json!({"event":"invocation","round":round,"ordinal":ordinal,"runtime_kind":"UiAction","workload":"settings_get_action","complete_invoke_us":elapsed,"response":{"ok":validation.is_ok(),"result":value,"classification":if validation.is_err() {Some("validation")} else {None},"error":validation.as_ref().err().map(ToString::to_string)}}));
                        // Return the retained original validation failure without substituting diagnostic I/O.
                        // 返回保留的原校验失败，不以诊断 I/O 替换。
                        if let Err(error) = validation {
                            // Report an independent record failure while preserving the primary error.
                            // 在保留主要错误的同时报告独立记录失败。
                            if let Err(record_error) = record {
                                eprintln!("Production action validation record failed: {record_error}");
                            }
                            return Err(error);
                        }
                        record?;
                        latencies.push(elapsed);
                    }
                    Err(error) => {
                        // Preserve the actual invocation error even if its independent diagnostic record cannot be written.
                        // 即使独立诊断记录无法写入，也保留实际调用错误。
                        let record = writeln!(output, "{}", json!({"event":"invocation","round":round,"ordinal":ordinal,"runtime_kind":"UiAction","workload":"settings_get_action","complete_invoke_us":elapsed,"response":{"ok":false,"error":error.to_string()}}));
                        // Preserve the secondary I/O diagnostic independently of the real invocation result.
                        // 独立于真实调用结果保留次要 I/O 诊断。
                        if let Err(record_error) = record {
                            eprintln!("Production action invocation record failed: {record_error}");
                        }
                        return Err(error.into());
                    }
                }
            }
            completed_rounds += 1;
            writeln!(output, "{}", json!({"event":"round","round":round + 1,"status":crate::observe_action(&manager)?}))?;
            output.flush()?;
        }
        if started.elapsed() > Duration::from_millis(cfg.maximum_duration_ms) {
            return Err("Production action crossed its frozen maximum duration.".into());
        }
        Ok(())
    }.await;
    // End the hot wall duration before cleanup; per-call percentiles remain the complete public invocation clock.
    // 清理前结束热态墙钟持续时间；单次百分位仍使用完整公开调用时钟。
    let duration = measurement_started.map(|started| started.elapsed());
    // Execute both original cleanup stages even after a call, observation or output-record failure.
    // 即使调用、观察或输出记录失败，也执行两个原清理阶段。
    let drain = crate::drain_action(&manager, cfg).await;
    // Parent close is a separate physical acknowledgement and cannot be inferred from drain intent.
    // 父级关闭是独立物理确认，不能从排空意图推断。
    let close = crate::close(&manager, cfg).await;
    // Preserve cleanup observations before emitting the terminal complete-action result.
    // 发出终态完整 Action 结果前保留清理观察。
    let final_status = crate::observe_action(&manager);
    latencies.sort_unstable();
    // Percentiles use every validated hot action in the same order-statistic convention as existing workloads.
    // 百分位使用每个已校验热态 Action，采用与既有负载相同的顺序统计约定。
    let percentile = |percent: usize| {
        latencies
            .get(latencies.len().saturating_sub(1) * percent / 100)
            .copied()
    };
    // Actual UiAction retirement counts and original parent closure jointly authorize success.
    // 实际 UiAction 退役计数及原父级关闭共同允许成功。
    let cleanup_confirmed = drain
        .as_ref()
        .is_ok_and(|report| !initialized || report["closed_ui_action_runtimes"] == 1)
        && close.is_ok()
        && final_status.is_ok();
    // Group identifies the consumer; the separate explicit purpose prevents reporting this session as ToolCall/ToolReusable.
    // group 标识消费者；独立显式用途防止将此会话报告为 ToolCall／ToolReusable。
    let summary = json!({"event":"summary","group":cfg.group,"workload":"settings_get_action","runtime_kind":"UiAction","measurement":matches!(&inputs.validation, SettingsGetActionValidation::Measure { .. }),"captured_result":captured_result,"initialization_completed":initialized,"rounds":completed_rounds,"duration_ms":duration.map(|value| value.as_millis()),"attempted":attempted,"successful":latencies.len(),"failed":attempted-latencies.len(),"sample_count":latencies.len(),"percentile_scope":if latencies.is_empty() {None} else {Some("all_successful_hot_ui_actions")},"p50_us":percentile(50),"p95_us":percentile(95),"p99_us":percentile(99),"workload_failure":workload.as_ref().err().map(ToString::to_string),"drain":drain.as_ref().ok(),"drain_error":drain.as_ref().err().map(ToString::to_string),"close_error":close.as_ref().err().map(ToString::to_string),"cleanup_confirmed":cleanup_confirmed,"final_status":final_status.as_ref().ok(),"final_status_error":final_status.as_ref().err().map(ToString::to_string),"phase_timings":null,"lua_heap_bytes":null});
    // Keep terminal recording independent of the original workload and completed physical cleanup results.
    // 将终态记录与原负载及已完成物理清理结果保持独立。
    let record = writeln!(output, "{summary}").and_then(|()| output.flush());
    // Workload failure wins first, then actual cleanup errors, then the original exact-retirement confirmation guard.
    // 负载失败首先保留，然后保留实际清理错误，最后检查原精确退役确认保护条件。
    let terminal = workload.and_then(|()| {
        drain?;
        close?;
        final_status?;
        if !cleanup_confirmed {
            return Err(
                "Production UiAction lacks exact physical drain/close confirmation.".into(),
            );
        }
        Ok(())
    });
    println!("{summary}");
    // Return the primary completed-workload or cleanup failure before considering record delivery.
    // 在考虑记录投递前返回主要已完成负载或清理失败。
    if let Err(error) = terminal {
        // Retain terminal I/O failure as an additional diagnostic rather than replacing the original outcome.
        // 将终态 I/O 失败保留为附加诊断，不替换原结果。
        if let Err(record_error) = record {
            eprintln!("Production action terminal record failed: {record_error}");
        }
        return Err(error);
    }
    record?;
    Ok(())
}

/// Run required configuration and output arguments, returning failure for incomplete soak or cleanup.
/// 运行必需的配置及输出参数，长稳或清理未完成时返回失败。
/// Consumer-specific build/observe/drain/close functions retain each verified public API boundary.
/// 消费者特定 build、observe、drain、close 函数保留各自已核实公开 API 边界。
pub async fn run() -> Result<(), Failure> {
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("Expected CONFIG_JSON OUTPUT_JSONL.".into());
    }
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let cfg: Config = serde_json::from_slice(&fs::read(&args[0])?)?;
    if cfg.concurrency == 0
        || cfg.rounds == 0
        || cfg.calls_per_worker == 0
        || cfg.timeout_ms == 0
        || cfg.cleanup_timeout_ms == 0
        || cfg.maximum_duration_ms == 0
        || (cfg.workload == "mixed"
            && (cfg.host_delay_ms == 0 || cfg.compute_iterations == 0 || cfg.io_bytes == 0))
        || ![
            "hot",
            "cold",
            "burst",
            "slow_host",
            "compute",
            "io",
            "mixed",
            "settings_get_action",
        ]
        .contains(&cfg.workload.as_str())
    {
        return Err("Invalid frozen workload limits.".into());
    }
    if cfg.workload == "settings_get_action" {
        return run_settings_get_action(
            &cfg,
            serde_json::from_slice(&fs::read(&args[0])?)?,
            Path::new(&args[1]),
        )
        .await;
    }
    if cfg.settings_get_action.is_some() {
        return Err("Production action inputs require workload settings_get_action.".into());
    }
    if cfg.soak
        && (cfg.minimum_duration_ms < 7_200_000
            || cfg.minimum_calls < 100_000
            || cfg.owner_cycles < 1000
            || cfg.workload != "mixed")
    {
        return Err(
            "Soak requires mixed workloads, two hours, 100000 successful calls and 1000 physical owner cycles."
                .into(),
        );
    }
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let root = tempfile::tempdir()?;
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let manager = crate::build(&cfg, root.path())?;
    activate(&manager, &cfg, root.path())?;
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let mut output = BufWriter::new(
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args[1])?,
    );
    writeln!(
        output,
        "{}",
        json!({"event":"configuration","config":serde_json::from_slice::<Value>(&fs::read(&args[0])?)?,"initial_status":crate::observe(&manager)?})
    )?;
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let mut latencies = Vec::with_capacity(if cfg.soak {
        SOAK_LATENCY_SAMPLE_LIMIT
    } else {
        0
    });
    // Count every real successful invocation independently of retained latency samples.
    // 独立于保留时延样本统计每次真实成功调用。
    let mut successful = 0usize;
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let mut attempted = 0usize;
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let mut failed = 0usize;
    // Preserve a failed workload gate until after explicit physical drain and parent close.
    // 将失败负载门禁保留至显式物理排空及父级关闭之后。
    let mut workload_failure: Option<String> = None;
    // Record actual cleanup errors without reclassifying them as native admission pressure.
    // 记录真实清理错误，不将它们重新分类为原生准入压力。
    let mut cleanup_failures = Vec::new();
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let mut round = 0usize;
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let started = Instant::now();
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let owners: Vec<_> = (0..cfg.concurrency)
        .map(|index| format!("tenant-{index}"))
        .collect();
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let workspaces: Vec<_> = (0..cfg.concurrency)
        .map(|index| root.path().join(format!("workspace-{index}")))
        .collect();
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    for workspace in &workspaces {
        fs::create_dir(workspace)?;
    }
    while round < cfg.rounds
        || started.elapsed() < Duration::from_millis(cfg.minimum_duration_ms)
        || successful < cfg.minimum_calls
    {
        if started.elapsed() >= Duration::from_millis(cfg.maximum_duration_ms) {
            workload_failure = Some("Maximum duration reached before all frozen minima.".into());
            break;
        }
        // Bind the concrete local value for this exact host operation.
        // 为此精确宿主操作绑定具体本地值。
        let mut workers = tokio::task::JoinSet::new();
        // Bind the concrete local value for this exact host operation.
        // 为此精确宿主操作绑定具体本地值。
        for index in 0..cfg.concurrency {
            // Bind the concrete local value for this exact host operation.
            // 为此精确宿主操作绑定具体本地值。
            let worker_manager = manager.clone();
            // Bind the concrete local value for this exact host operation.
            // 为此精确宿主操作绑定具体本地值。
            let worker_cfg = cfg.clone();
            // Bind the concrete local value for this exact host operation.
            // 为此精确宿主操作绑定具体本地值。
            let owner = owners[index].clone();
            // Bind the concrete local value for this exact host operation.
            // 为此精确宿主操作绑定具体本地值。
            let workspace = workspaces[index].clone();
            workers.spawn(async move {
                // Bind the concrete local value for this exact host operation.
                // 为此精确宿主操作绑定具体本地值。
                let mut records = Vec::new();
                // Retain any actual acknowledgement/drain failure while still returning original invocation records.
                // 保留实际确认或排空失败，同时仍返回原始调用记录。
                let mut cleanup_error = None;
                // Bind the concrete local value for this exact host operation.
                // 为此精确宿主操作绑定具体本地值。
                for ordinal in 0..worker_cfg.calls_per_worker {
                    // Stop new submissions at the frozen wall deadline while retaining already-started actual calls.
                    // 在冻结墙钟截止时间停止新提交，同时保留已经启动的真实调用。
                    if started.elapsed() >= Duration::from_millis(worker_cfg.maximum_duration_ms) {
                        break;
                    }
                    // Bind the concrete local value for this exact host operation.
                    // 为此精确宿主操作绑定具体本地值。
                    let sequence = round
                        .checked_mul(worker_cfg.calls_per_worker)
                        .and_then(|value| value.checked_add(ordinal + 1))
                        .ok_or("Sequence overflow.")?;
                    // Bind the concrete local value for this exact host operation.
                    // 为此精确宿主操作绑定具体本地值。
                    let (elapsed, result) =
                        call(&worker_manager, &worker_cfg, &owner, &workspace, sequence).await?;
                    records.push((elapsed, result));
                    if let Err(error) = crate::acknowledge(&worker_manager, &owner) {
                        cleanup_error = Some(error.to_string());
                        break;
                    }
                    if worker_cfg.workload == "cold" {
                        if let Err(error) = crate::drain(&worker_manager, &owner, &worker_cfg).await
                        {
                            cleanup_error = Some(error.to_string());
                            break;
                        }
                    }
                }
                Ok::<_, Failure>((owner, records, cleanup_error))
            });
        }
        // Bind the concrete local value for this exact host operation.
        // 为此精确宿主操作绑定具体本地值。
        while let Some(worker) = workers.join_next().await {
            // Bind the concrete local value for this exact host operation.
            // 为此精确宿主操作绑定具体本地值。
            let (owner, records, cleanup_error) = worker??;
            // Bind the concrete local value for this exact host operation.
            // 为此精确宿主操作绑定具体本地值。
            for (elapsed, result) in records {
                attempted += 1;
                if result["ok"] == true {
                    successful += 1;
                    if cfg.soak && latencies.len() == SOAK_LATENCY_SAMPLE_LIMIT {
                        // Replace one exact oldest retained sample; raw JSONL still contains every invocation.
                        // 替换单个精确最旧保留样本；原始 JSONL 仍包含每次调用。
                        latencies[(successful - 1) % SOAK_LATENCY_SAMPLE_LIMIT] = elapsed;
                    } else {
                        latencies.push(elapsed);
                    }
                } else {
                    failed += 1;
                }
                writeln!(
                    output,
                    "{}",
                    json!({"event":"invocation","round":round,"owner":owner,"workload":result["workload"],"complete_invoke_us":elapsed,"response":result})
                )?;
            }
            if let Some(error) = cleanup_error {
                writeln!(
                    output,
                    "{}",
                    json!({"event":"worker_cleanup_failure","round":round,"owner":owner,"error":error})
                )?;
                cleanup_failures.push(error);
            }
        }
        round += 1;
        // Preserve an exact public observation failure, then enter the same explicit cleanup/close path.
        // 保留精确公开观测失败，然后进入相同显式清理及关闭路径。
        let round_status = match crate::observe(&manager) {
            Ok(status) => json!({"ok":true,"value":status}),
            Err(error) => {
                cleanup_failures.push(format!("Round {round} observation failed: {error}"));
                json!({"ok":false,"error":error.to_string()})
            }
        };
        writeln!(
            output,
            "{}",
            json!({"event":"round","round":round,"status":round_status})
        )?;
        output.flush()?;
        // A round can satisfy every minimum after crossing the maximum; that remains a failed timing gate.
        // 一轮可能越过最长时限后才满足所有最小要求；这仍然属于时间门禁失败。
        if started.elapsed() > Duration::from_millis(cfg.maximum_duration_ms) {
            workload_failure = Some("Timed workload exceeded its frozen maximum duration.".into());
            break;
        }
        if cfg.soak && failed > 0 {
            workload_failure =
                Some("Soak encountered an invocation or business validation failure.".into());
            break;
        }
        if !cleanup_failures.is_empty() || workload_failure.is_some() {
            break;
        }
    }
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let duration = started.elapsed();
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    for owner in &owners {
        if let Err(error) = crate::drain(&manager, owner, &cfg).await {
            cleanup_failures.push(format!("Owner {owner} drain failed: {error}"));
        }
    }
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let baseline = match crate::observe(&manager) {
        Ok(baseline) => baseline,
        Err(error) => {
            // A missing baseline cannot authorize cycles; still attempt the original parent's public physical close.
            // 缺失基线不能授权循环；仍尝试原父级的公开物理关闭。
            let closed = crate::close(&manager, &cfg).await;
            writeln!(
                output,
                "{}",
                json!({"event":"baseline_observation_failure","error":error.to_string(),"close_error":closed.as_ref().err().map(ToString::to_string)})
            )?;
            output.flush()?;
            return Err(format!(
                "Baseline observation failed after public close attempt: {error}; close={closed:?}"
            )
            .into());
        }
    };
    // Count only physically verified completed cycles, never configured attempts or closing requests.
    // 仅统计经过物理验证的已完成循环，绝不统计配置尝试数或关闭请求。
    let mut completed_owner_cycles = 0usize;
    // Keep cycle invocation errors separate from timed workload throughput and success accounting.
    // 将循环调用错误与计时负载吞吐及成功记账分开。
    let mut cycle_failed = 0usize;
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    for cycle in 0..cfg.owner_cycles {
        if !cleanup_failures.is_empty() || workload_failure.is_some() {
            break;
        }
        // Bind the concrete local value for this exact host operation.
        // 为此精确宿主操作绑定具体本地值。
        let owner = format!("cycle-{cycle}");
        // Bind the concrete local value for this exact host operation.
        // 为此精确宿主操作绑定具体本地值。
        let workspace = root.path().join("cycle-workspace");
        fs::create_dir_all(&workspace)?;
        // Bind the concrete local value for this exact host operation.
        // 为此精确宿主操作绑定具体本地值。
        let (elapsed, result) = call(&manager, &cfg, &owner, &workspace, 1).await?;
        if result["ok"] != true {
            cycle_failed += 1;
            writeln!(
                output,
                "{}",
                json!({"event":"owner_cycle_invocation_failure","cycle":cycle,"owner":owner,"workload":result["workload"],"complete_invoke_us":elapsed,"response":result})
            )?;
            if let Err(error) = crate::drain(&manager, &owner, &cfg).await {
                cleanup_failures.push(format!("Failed cycle owner {owner} drain failed: {error}"));
            }
            break;
        }
        // Bind the concrete local value for this exact host operation.
        // 为此精确宿主操作绑定具体本地值。
        let live = match crate::owner_live(&manager, &owner) {
            Ok(live) => live,
            Err(error) => {
                cleanup_failures.push(format!(
                    "Cycle owner {owner} live domain verification failed: {error}"
                ));
                if let Err(error) = crate::drain(&manager, &owner, &cfg).await {
                    cleanup_failures.push(format!(
                        "Unverified cycle owner {owner} drain failed: {error}"
                    ));
                }
                break;
            }
        };
        if let Err(error) = crate::drain(&manager, &owner, &cfg).await {
            cleanup_failures.push(format!("Cycle owner {owner} drain failed: {error}"));
            break;
        }
        // Bind the concrete local value for this exact host operation.
        // 为此精确宿主操作绑定具体本地值。
        let after = match crate::observe(&manager) {
            Ok(after) => after,
            Err(error) => {
                cleanup_failures.push(format!(
                    "Cycle owner {owner} resource observation failed: {error}"
                ));
                break;
            }
        };
        if after != baseline {
            cleanup_failures.push(format!(
                "Owner cycle resources/retained counts did not return to baseline: {after}"
            ));
            break;
        }
        completed_owner_cycles += 1;
        writeln!(
            output,
            "{}",
            json!({"event":"owner_cycle","cycle":cycle,"workload":result["workload"],"complete_invoke_us":elapsed,"response":result,"live":live,"after":after})
        )?;
    }
    if let Err(error) = crate::close(&manager, &cfg).await {
        cleanup_failures.push(format!("Original parent physical close failed: {error}"));
    }
    latencies.sort_unstable();
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let percentile = |percent: usize| -> Option<u128> {
        if latencies.is_empty() {
            None
        } else {
            Some(latencies[((latencies.len() - 1) * percent) / 100])
        }
    };
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let summary = json!({"event":"summary","group":cfg.group,"workload":cfg.workload,"rounds":round,"attempted":attempted,"successful":successful,"failed":failed,"cycle_failed":cycle_failed,"duration_ms":duration.as_millis(),"successful_per_second":successful as f64/duration.as_secs_f64(),"sample_count":latencies.len(),"sample_limit":if cfg.soak {Some(SOAK_LATENCY_SAMPLE_LIMIT)} else {None},"percentile_scope":if cfg.soak {"recent_bounded_success_samples"} else {"all_successful_invocations"},"p50_us":if cfg.soak {None} else {percentile(50)},"p95_us":if cfg.soak {None} else {percentile(95)},"p99_us":if cfg.soak {None} else {percentile(99)},"sampled_p50_us":if cfg.soak {percentile(50)} else {None},"sampled_p95_us":if cfg.soak {percentile(95)} else {None},"sampled_p99_us":if cfg.soak {percentile(99)} else {None},"owner_cycles":completed_owner_cycles,"configured_owner_cycles":cfg.owner_cycles,"workload_failure":workload_failure,"cleanup_failures":cleanup_failures,"final_status":crate::observe(&manager)?,"phase_timings":null,"lua_heap_bytes":null});
    writeln!(output, "{summary}")?;
    output.flush()?;
    println!("{summary}");
    if workload_failure.is_some()
        || !cleanup_failures.is_empty()
        || cycle_failed > 0
        || (cfg.soak && failed > 0)
    {
        return Err(format!("Validation gate failed after explicit physical cleanup/close attempts: workload failures={failed}, cycle failures={cycle_failed}, completed cycles={completed_owner_cycles}, workload gate={workload_failure:?}, cleanup={cleanup_failures:?}").into());
    }
    Ok(())
}
