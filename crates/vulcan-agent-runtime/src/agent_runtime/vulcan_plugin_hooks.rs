//! Vulcan Plugin Lua hook registration bridge for the shared host dispatcher.
//! 面向共享宿主调度器的 Vulcan 插件 Lua hook 注册桥。

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;
use sha2::{Digest, Sha256};
use vulcan_hooks::{
    FnHookHandler, HookActor, HookContext, HookDecision, HookDispatchReport, HookDispatchRequest,
    HookDispatcher, HookError, HookMode, HookPayload, HookPointDescriptor, HookRegistration,
    HookScope,
};
use vulcan_plugin_runtime::{
    VulcanPluginCatalogSnapshot, VulcanPluginHookDescriptor, VulcanPluginRuntimeManager,
    VulcanPluginRuntimeState, VulcanPluginToolHookSourceKind,
};
use vulcan_tools_manager::{
    VulcanToolCandidate, VulcanToolInvocationContext, VulcanToolInvocationResult, VulcanToolOrigin,
    VulcanToolSourceKind,
};

/// Stable registration prefix reserved for manifest-driven Vulcan Plugin hooks.
/// 为清单驱动 Vulcan 插件 hook 保留的稳定注册前缀。
const VULCAN_PLUGIN_HOOK_REGISTRATION_PREFIX: &str = "vulcan-plugin/";

/// Stable host point names for the generic tool lifecycle.
/// 通用工具生命周期使用的稳定宿主点位名称。
const TOOL_HOOK_POINTS: [&str; 3] = [
    vulcan_plugin_runtime::VULCAN_PLUGIN_HOOK_TOOL_REGISTRATION,
    vulcan_plugin_runtime::VULCAN_PLUGIN_HOOK_TOOL_BEFORE,
    vulcan_plugin_runtime::VULCAN_PLUGIN_HOOK_TOOL_AFTER,
];

/// Cache key for one immutable registration transformation.
/// 一次不可变注册变换使用的缓存键。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ToolRegistrationCacheKey {
    /// Canonical plugin root that owns the catalog revision.
    /// 拥有该目录代次的规范化插件根目录。
    root: PathBuf,
    /// Optional canonical user-plugin root merged into the catalog.
    /// 合并进目录的可选规范化用户插件根目录。
    user_root: Option<PathBuf>,
    /// Monotonic catalog revision that freezes enabled hooks and generations.
    /// 冻结已启用 Hook 与插件代次的单调目录修订号。
    catalog_revision: u64,
    /// Exact immutable source identity of the candidate.
    /// 候选工具的不可变精确来源身份。
    origin: VulcanToolOrigin,
    /// Digest of the candidate definition before registration transformation.
    /// 注册变换前候选定义的摘要。
    definition_digest: String,
    /// Digest of the matching registration hook declarations.
    /// 匹配注册 Hook 声明的摘要。
    registration_hooks_digest: String,
}

/// Cached public-definition fields produced by `tool.registration`.
/// `tool.registration` 产生的已缓存公开定义字段。
#[derive(Debug, Clone)]
struct CachedToolRegistrationDefinition {
    /// Host-facing identifier before final public-name normalization.
    /// 最终公开名称归一化前的宿主侧标识。
    id: String,
    /// Host-facing name before final public-name normalization.
    /// 最终公开名称归一化前的宿主侧名称。
    name: String,
    /// Model-visible description.
    /// 面向模型的工具描述。
    description: String,
    /// Optional model-visible help text.
    /// 可选的面向模型帮助文本。
    help_text: Option<String>,
    /// Argument JSON Schema.
    /// 参数 JSON Schema。
    parameters_schema: Value,
    /// Whether the candidate remains directly visible to the model.
    /// 候选是否继续直接对模型可见。
    ai_visible: bool,
}

/// One cached registration decision for a fixed plugin catalog generation.
/// 某个固定插件目录代次的一项已缓存工具注册决策。
#[derive(Clone)]
enum CachedToolRegistrationOutcome {
    /// Public definition after all registration transforms completed.
    /// 所有注册变换完成后的公开定义。
    Definition(CachedToolRegistrationDefinition),
    /// Exact declaration blocked this original tool before AI registration.
    /// 精确声明在 AI 注册前阻断了此原始工具。
    Omitted,
}

/// Process-local cache for catalog-generation registration transforms.
/// 用于目录代次注册变换的进程级缓存。
static TOOL_REGISTRATION_CACHE: OnceLock<
    Mutex<HashMap<ToolRegistrationCacheKey, CachedToolRegistrationOutcome>>,
> = OnceLock::new();

/// Per-definition locks prevent concurrent provider requests from running one registration Hook twice.
/// 按定义分配的锁防止并发 Provider 请求重复执行同一个注册 Hook。
static TOOL_REGISTRATION_FLIGHTS: OnceLock<
    Mutex<HashMap<ToolRegistrationCacheKey, Arc<Mutex<()>>>>,
> = OnceLock::new();

/// Returns the generic tool point descriptors accepted by the host dispatcher.
/// 返回宿主调度器接受的通用工具点位描述。
fn generic_tool_hook_point_descriptors() -> [HookPointDescriptor; 3] {
    [
        HookPointDescriptor::new(
            vulcan_plugin_runtime::VULCAN_PLUGIN_HOOK_TOOL_REGISTRATION,
            [HookMode::Transform, HookMode::Decision],
            "Transform one explicitly targeted AI-visible tool definition during catalog construction.",
        )
        .with_default_scope(HookScope::Global),
        HookPointDescriptor::new(
            vulcan_plugin_runtime::VULCAN_PLUGIN_HOOK_TOOL_BEFORE,
            [HookMode::Transform, HookMode::Decision],
            "Transform or reject one explicitly targeted tool invocation before host I/O.",
        )
        .with_default_scope(HookScope::Session),
        HookPointDescriptor::new(
            vulcan_plugin_runtime::VULCAN_PLUGIN_HOOK_TOOL_AFTER,
            [HookMode::Transform],
            "Transform one explicitly targeted model-visible tool result after host I/O.",
        )
        .with_default_scope(HookScope::Session),
    ]
}

/// Returns whether a point is one of the three generic tool lifecycle points.
/// 返回点位是否属于三个通用工具生命周期点位之一。
fn is_generic_tool_hook_point(point: &str) -> bool {
    TOOL_HOOK_POINTS.contains(&point)
}

/// Converts the manifest source kind into the manager's immutable source kind.
/// 将清单来源种类转换为管理器的不可变来源种类。
fn map_tool_hook_source_kind(source_kind: VulcanPluginToolHookSourceKind) -> VulcanToolSourceKind {
    match source_kind {
        VulcanPluginToolHookSourceKind::Native => VulcanToolSourceKind::Native,
        VulcanPluginToolHookSourceKind::VulcanManaged => VulcanToolSourceKind::VulcanManaged,
        VulcanPluginToolHookSourceKind::VulcanPlugin => VulcanToolSourceKind::VulcanPlugin,
        VulcanPluginToolHookSourceKind::LuaSkills => VulcanToolSourceKind::LuaSkills,
        VulcanPluginToolHookSourceKind::Mcp => VulcanToolSourceKind::Mcp,
        VulcanPluginToolHookSourceKind::AiHostBinding => VulcanToolSourceKind::AiHostBinding,
    }
}

/// Returns the stable serialized label for one immutable source kind.
/// 返回不可变来源种类的稳定序列化标签。
fn tool_source_kind_label(source_kind: VulcanToolSourceKind) -> &'static str {
    match source_kind {
        VulcanToolSourceKind::Native => "native",
        VulcanToolSourceKind::VulcanManaged => "vulcan_managed",
        VulcanToolSourceKind::VulcanPlugin => "vulcan_plugin",
        VulcanToolSourceKind::LuaSkills => "lua_skills",
        VulcanToolSourceKind::Mcp => "mcp",
        VulcanToolSourceKind::AiHostBinding => "ai_host_binding",
    }
}

/// Returns whether a manifest target exactly identifies one candidate origin.
/// 返回清单目标是否精确标识某个候选来源身份。
fn tool_hook_targets_origin(hook: &VulcanPluginHookDescriptor, origin: &VulcanToolOrigin) -> bool {
    let Some(target) = hook.manifest.tool_target.as_ref() else {
        return false;
    };
    map_tool_hook_source_kind(target.source_kind) == origin.source_kind()
        && target.namespace == origin.namespace()
        && target.original_name == origin.original_name()
}

/// Returns the enabled generic hooks that match one immutable origin and point.
/// 返回与不可变来源身份及点位精确匹配的已启用通用 Hook。
fn matching_tool_hooks(
    snapshot: &VulcanPluginCatalogSnapshot,
    origin: &VulcanToolOrigin,
    point: &str,
) -> Vec<VulcanPluginHookDescriptor> {
    snapshot
        .enabled_hooks
        .values()
        .filter(|hook| hook.manifest.point == point && tool_hook_targets_origin(hook, origin))
        .cloned()
        .collect()
}

/// Builds a stable host-owned JSON identity frame without consulting public names.
/// 构建稳定的宿主 JSON 身份帧，整个过程不读取公开名称。
fn tool_origin_json(origin: &VulcanToolOrigin) -> Value {
    serde_json::json!({
        "source_kind": tool_source_kind_label(origin.source_kind()),
        "namespace": origin.namespace(),
        "original_name": origin.original_name(),
    })
}

/// Builds the registration input frame sent to Lua.
/// 构建发送给 Lua 的注册输入帧。
fn registration_payload(candidate: &VulcanToolCandidate) -> Value {
    serde_json::json!({
        "tool": tool_origin_json(candidate.descriptor.origin()),
        "definition": {
            "id": candidate.descriptor.id,
            "name": candidate.descriptor.name,
            "description": candidate.descriptor.description,
            "help_text": candidate.descriptor.help_text,
            "parameters_schema": candidate.parameters_schema,
            "ai_visible": candidate.ai_visible,
        },
    })
}

/// Builds the invocation-before frame sent to Lua.
/// 构建发送给 Lua 的调用前输入帧。
fn before_payload(origin: &VulcanToolOrigin, public_name: &str, arguments: Value) -> Value {
    serde_json::json!({
        "tool": tool_origin_json(origin),
        "public_name": public_name,
        "arguments": arguments,
    })
}

/// Builds the invocation-after frame sent to Lua.
/// 构建发送给 Lua 的调用后输入帧。
fn after_payload(
    origin: &VulcanToolOrigin,
    public_name: &str,
    arguments: Value,
    result: Value,
) -> Value {
    serde_json::json!({
        "tool": tool_origin_json(origin),
        "public_name": public_name,
        "arguments": arguments,
        "result": result,
    })
}

/// Returns the digest used to distinguish one immutable JSON definition.
/// 返回区分不可变 JSON 定义所使用的摘要。
fn json_digest(value: &Value) -> String {
    let bytes = value.to_string().into_bytes();
    hex::encode(Sha256::digest(bytes))
}

/// Returns a digest of the matching registration declarations in execution order.
/// 返回按执行顺序排列的匹配注册声明摘要。
fn registration_hooks_digest(hooks: &[VulcanPluginHookDescriptor]) -> String {
    let value = hooks
        .iter()
        .map(|hook| {
            serde_json::json!({
                "plugin_id": hook.plugin_id,
                "plugin_generation": hook.plugin_generation,
                "registration_id": hook.registration_id,
                "point": hook.manifest.point,
                "mode": format!("{:?}", hook.manifest.mode),
                "function": hook.manifest.function,
            })
        })
        .collect::<Vec<_>>();
    json_digest(&Value::Array(value))
}

/// Returns a cache key for one registration transformation at one catalog revision.
/// 返回某个目录修订下单次注册变换的缓存键。
fn registration_cache_key(
    snapshot: &VulcanPluginCatalogSnapshot,
    candidate: &VulcanToolCandidate,
    hooks: &[VulcanPluginHookDescriptor],
) -> ToolRegistrationCacheKey {
    ToolRegistrationCacheKey {
        root: snapshot.root.clone(),
        user_root: snapshot.user_root.clone(),
        catalog_revision: snapshot.revision,
        origin: candidate.descriptor.origin().clone(),
        definition_digest: json_digest(&registration_payload(candidate)),
        registration_hooks_digest: registration_hooks_digest(hooks),
    }
}

/// Removes older revisions without letting a stale in-flight request evict a newer cache.
/// 移除旧修订，同时防止在途旧请求淘汰更新的缓存。
fn purge_stale_registration_cache_entries(snapshot: &VulcanPluginCatalogSnapshot) {
    let Some(cache) = TOOL_REGISTRATION_CACHE.get() else {
        return;
    };
    let Ok(mut cache) = cache.lock() else {
        return;
    };
    cache.retain(|key, _| {
        key.root != snapshot.root
            || key.user_root != snapshot.user_root
            || key.catalog_revision >= snapshot.revision
    });
}

/// Reads one cached registration decision when the catalog generation still matches.
/// 当目录代次仍匹配时读取一项缓存注册决策。
fn cached_registration_outcome(
    key: &ToolRegistrationCacheKey,
) -> Option<CachedToolRegistrationOutcome> {
    TOOL_REGISTRATION_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|cache| cache.get(key).cloned())
}

/// Stores one decision for every tool in the current catalog without arbitrary entry eviction.
/// 保留当前目录中每个工具的决策，不按任意条目上限淘汰。
/// Older catalog revisions are removed by `purge_stale_registration_cache_entries`.
/// 旧目录修订由 `purge_stale_registration_cache_entries` 移除。
fn cache_registration_outcome(
    key: ToolRegistrationCacheKey,
    outcome: CachedToolRegistrationOutcome,
) {
    if let Ok(mut cache) = TOOL_REGISTRATION_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        // A late completion from a retired catalog must not repopulate entries after a newer
        // request has already advanced the same catalog identity.
        // 已退役目录的迟到结果不得在同一目录身份进入新修订后重新填充旧条目。
        if cache.keys().any(|cached| {
            cached.root == key.root
                && cached.user_root == key.user_root
                && cached.catalog_revision > key.catalog_revision
        }) {
            return;
        }
        cache.insert(key, outcome);
    }
}

/// Computes one registration result once while other callers wait for that exact cache key.
/// 对同一缓存键只计算一次注册结果，其他调用方等待该结果。
///
/// `key` freezes catalog and source definitions; `compute` enters Lua only on a cache miss.
/// Returns a cached or new result, while failed and deferred hooks remain retryable.
/// `key` 冻结目录与来源定义；`compute` 仅在缓存缺失时进入 Lua。
/// 返回已缓存或新结果，失败与延迟的 Hook 保持可重试。
fn cached_or_compute_registration_outcome<F>(
    key: &ToolRegistrationCacheKey,
    compute: F,
) -> Result<CachedToolRegistrationOutcome, String>
where
    F: FnOnce() -> Result<CachedToolRegistrationOutcome, String>,
{
    if let Some(outcome) = cached_registration_outcome(key) {
        return Ok(outcome);
    }
    let flights = TOOL_REGISTRATION_FLIGHTS.get_or_init(|| Mutex::new(HashMap::new()));
    let flight = flights
        .lock()
        .map_err(|error| format!("failed to lock tool registration flights: {error}"))?
        .entry(key.clone())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    let result = (|| {
        let _flight_guard = flight
            .lock()
            .map_err(|error| format!("failed to lock tool registration flight: {error}"))?;
        // A preceding waiter may have published the result while this caller acquired the lock.
        // 当前调用方取得锁前，前一个等待者可能已经发布结果，因此必须再次查询缓存。
        if let Some(outcome) = cached_registration_outcome(key) {
            return Ok(outcome);
        }
        let outcome = compute()?;
        cache_registration_outcome(key.clone(), outcome.clone());
        Ok(outcome)
    })();
    // Drop this caller's strong reference before checking the map. Otherwise two callers can
    // both observe each other's reference, skip removal, then leave an unused flight behind.
    // 检查映射前先释放当前调用方的强引用；否则两个调用方可能互相看到对方的引用，均跳过清理，
    // 最终留下无人使用的飞行锁。
    // flight_identity keeps the allocation identity stable while the map may change concurrently.
    // flight_identity 在映射可能并发变化时维持该分配的稳定身份。
    let flight_identity = Arc::downgrade(&flight);
    drop(flight);
    if let Ok(mut active) = flights.lock()
        && active.get(key).is_some_and(|current| {
            flight_identity.ptr_eq(&Arc::downgrade(current)) && Arc::strong_count(current) == 1
        })
    {
        active.remove(key);
    }
    result
}

/// Creates a dispatcher containing built-in points and the generic tool points.
/// 创建包含内建点位与通用工具点位的调度器。
fn new_host_hook_dispatcher() -> Result<HookDispatcher, HookError> {
    let mut dispatcher = HookDispatcher::default();
    for descriptor in super::conversation_hooks::host_hook_point_descriptors() {
        dispatcher.register_point(descriptor)?;
    }
    for descriptor in generic_tool_hook_point_descriptors() {
        dispatcher.register_point(descriptor)?;
    }
    Ok(dispatcher)
}

/// Verifies that a cached hook descriptor is still enabled in the current plugin generation.
/// 验证缓存的 Hook 描述仍处于当前插件启用代次。
fn validate_tool_hook_lifecycle(
    runtime: &VulcanPluginRuntimeManager,
    hooks: &[VulcanPluginHookDescriptor],
) -> Result<(), HookError> {
    let snapshot = runtime.snapshot();
    for hook in hooks {
        let plugin = snapshot.plugins.get(&hook.plugin_id).ok_or_else(|| {
            HookError::handler(format!(
                "Vulcan Plugin `{}` is no longer installed.",
                hook.plugin_id
            ))
        })?;
        if plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            || plugin.generation != hook.plugin_generation
        {
            return Err(HookError::handler(format!(
                "Vulcan Plugin `{}` hook `{}` is from a stale lifecycle generation.",
                hook.plugin_id, hook.registration_id
            )));
        }
        let current = snapshot
            .enabled_hooks
            .get(&hook.registration_id)
            .ok_or_else(|| {
                HookError::handler(format!(
                    "Vulcan Plugin hook `{}` is no longer enabled.",
                    hook.registration_id
                ))
            })?;
        let target_matches = match (
            current.manifest.tool_target.as_ref(),
            hook.manifest.tool_target.as_ref(),
        ) {
            (None, None) => true,
            (Some(current), Some(previous)) => {
                current.source_kind == previous.source_kind
                    && current.namespace == previous.namespace
                    && current.original_name == previous.original_name
            }
            _ => false,
        };
        if current.plugin_id != hook.plugin_id
            || current.plugin_generation != hook.plugin_generation
            || current.manifest.point != hook.manifest.point
            || current.manifest.mode != hook.manifest.mode
            || current.manifest.function != hook.manifest.function
            || !target_matches
        {
            return Err(HookError::handler(format!(
                "Vulcan Plugin hook `{}` changed since the tool surface was prepared.",
                hook.registration_id
            )));
        }
    }
    Ok(())
}

/// Registers an explicitly selected set of plugin hooks into a dispatcher.
/// 将显式选中的插件 Hook 集合注册到调度器。
fn synchronize_vulcan_plugin_hook_descriptors(
    dispatcher: &mut HookDispatcher,
    runtime: VulcanPluginRuntimeManager,
    hooks: &[VulcanPluginHookDescriptor],
) -> Result<(), HookError> {
    validate_vulcan_plugin_hooks(dispatcher, hooks)?;
    validate_tool_hook_lifecycle(&runtime, hooks)?;

    let mut staged = dispatcher.clone();
    let existing_ids = staged
        .registry()
        .registration_snapshots(None)
        .into_iter()
        .map(|snapshot| snapshot.registration.id)
        .filter(|id| id.starts_with(VULCAN_PLUGIN_HOOK_REGISTRATION_PREFIX))
        .collect::<Vec<_>>();
    for registration_id in existing_ids {
        staged.unregister(&registration_id, None);
    }

    for hook in hooks.iter().cloned() {
        let mut registration = HookRegistration::new(
            hook.registration_id.clone(),
            hook.manifest.point.clone(),
            hook.manifest.mode,
        );
        registration.scope = hook.manifest.scope;
        registration.priority = hook.manifest.priority;
        registration.timeout_ms = hook.manifest.timeout_ms;
        registration.failure_policy = hook.manifest.failure_policy;
        registration.requested_capabilities = hook
            .manifest
            .requested_capabilities
            .iter()
            .cloned()
            .collect();
        registration.display_name = hook.manifest.display_name.clone();
        registration.progress_label = hook.manifest.progress_label.clone();
        let hook_runtime = runtime.clone();
        staged.register(
            registration,
            Arc::new(FnHookHandler::new(move |invocation| {
                let hook_runtime = hook_runtime.clone();
                let hook = hook.clone();
                async move {
                    hook_runtime
                        .invoke_hook(&hook, invocation)
                        .await
                        .map_err(|error| HookError::handler(error.to_string()))
                }
            })),
        )?;
    }
    *dispatcher = staged;
    Ok(())
}

/// Builds a trusted session-scoped context for a tool lifecycle hook.
/// 为工具生命周期 Hook 构建可信的会话作用域上下文。
fn tool_hook_context(
    context: &VulcanToolInvocationContext,
    point: &str,
    workspace_root: Option<&PathBuf>,
) -> HookContext {
    let mut hook_context = HookContext::new(context.controller_tool_call_id.clone(), point);
    hook_context.scope = HookScope::Session;
    hook_context.actor = HookActor::new("ai-tool");
    hook_context.session_id = Some(context.session_id.clone());
    if let Some(workspace_root) = workspace_root {
        hook_context.metadata.insert(
            String::from("workspace_root"),
            Value::String(workspace_root.to_string_lossy().into_owned()),
        );
    }
    hook_context
}

/// Builds a catalog-scoped context for a registration transformation.
/// 为注册变换构建目录作用域上下文。
fn registration_hook_context() -> HookContext {
    let mut context = HookContext::new(
        "tool-registration",
        vulcan_plugin_runtime::VULCAN_PLUGIN_HOOK_TOOL_REGISTRATION,
    );
    context.scope = HookScope::Global;
    context.actor = HookActor::new("tool-catalog");
    context
}

/// Dispatches explicitly matched tool hooks without exposing unrelated declarations.
/// 仅调度显式匹配的工具 Hook，绝不暴露无关声明。
async fn dispatch_tool_hook_descriptors(
    runtime: VulcanPluginRuntimeManager,
    hooks: Vec<VulcanPluginHookDescriptor>,
    context: HookContext,
    payload: Value,
) -> Result<HookDispatchReport, HookError> {
    if matches!(
        context.point.as_str(),
        vulcan_plugin_runtime::VULCAN_PLUGIN_HOOK_TOOL_BEFORE
            | vulcan_plugin_runtime::VULCAN_PLUGIN_HOOK_TOOL_AFTER
    ) && context
        .session_id
        .as_deref()
        .is_none_or(|session_id| session_id.trim().is_empty())
    {
        return Err(HookError::invalid_registration(
            "Generic tool invocation hooks require a trusted session context.",
        ));
    }
    let mut dispatcher = new_host_hook_dispatcher()?;
    synchronize_vulcan_plugin_hook_descriptors(&mut dispatcher, runtime, &hooks)?;
    dispatcher
        .dispatch(HookDispatchRequest::new(
            context,
            HookPayload::from_serializable(payload)?,
        ))
        .await
}

/// Runs one async hook chain from the synchronous candidate-construction boundary.
/// 从同步候选构建边界运行一次异步 Hook 链。
fn run_hook_future_from_sync<F>(future: F) -> F::Output
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tool hook runtime should build")
                .block_on(future)
        })
        .join()
        .expect("tool hook worker should complete")
    } else {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tool hook runtime should build")
            .block_on(future)
    }
}

/// Verifies that a Lua response still carries the immutable source identity.
/// 验证 Lua 响应仍携带不可变来源身份。
fn response_identity_matches_origin(value: &Value, origin: &VulcanToolOrigin) -> bool {
    value.get("source_kind").and_then(Value::as_str)
        == Some(tool_source_kind_label(origin.source_kind()))
        && value.get("namespace").and_then(Value::as_str) == Some(origin.namespace())
        && value.get("original_name").and_then(Value::as_str) == Some(origin.original_name())
}

/// Applies one cached registration result to a candidate while preserving its origin.
/// 将一份缓存注册结果应用到候选，同时保留其来源身份。
fn apply_cached_registration_definition(
    mut candidate: VulcanToolCandidate,
    definition: CachedToolRegistrationDefinition,
) -> VulcanToolCandidate {
    candidate.descriptor.id = definition.id;
    candidate.descriptor.name = definition.name;
    candidate.descriptor.description = definition.description;
    candidate.descriptor.help_text = definition.help_text;
    candidate.parameters_schema = definition.parameters_schema;
    candidate.ai_visible = definition.ai_visible;
    candidate
}

/// Parses a Lua registration response into host-owned definition fields.
/// 将 Lua 注册响应解析为宿主自有的定义字段。
fn parse_registration_definition(
    payload: &Value,
    candidate: &VulcanToolCandidate,
) -> Result<CachedToolRegistrationDefinition, String> {
    let payload = payload
        .as_object()
        .ok_or_else(|| String::from("tool.registration must return an object payload."))?;
    let tool = payload
        .get("tool")
        .and_then(Value::as_object)
        .ok_or_else(|| String::from("tool.registration payload must contain tool identity."))?;
    if !response_identity_matches_origin(
        &Value::Object(tool.clone()),
        candidate.descriptor.origin(),
    ) {
        return Err(String::from(
            "tool.registration cannot change the immutable tool origin.",
        ));
    }
    let definition = payload
        .get("definition")
        .and_then(Value::as_object)
        .ok_or_else(|| String::from("tool.registration payload must contain definition."))?;

    let string_field = |field: &str, fallback: &str| {
        definition
            .get(field)
            .map(|value| {
                value
                    .as_str()
                    .filter(|value| !value.trim().is_empty())
                    .map(String::from)
                    .ok_or_else(|| {
                        format!("tool.registration field `{field}` must be non-empty text.")
                    })
            })
            .transpose()
            .map(|value| value.unwrap_or_else(|| fallback.to_string()))
    };
    let help_text = definition
        .get("help_text")
        .map(|value| {
            if value.is_null() {
                Ok(None)
            } else {
                value
                    .as_str()
                    .map(|value| Some(value.to_string()))
                    .ok_or_else(|| {
                        String::from("tool.registration field `help_text` must be text or null.")
                    })
            }
        })
        .transpose()
        .map(|value| value.unwrap_or_else(|| candidate.descriptor.help_text.clone()))?;
    let parameters_schema = definition
        .get("parameters_schema")
        .cloned()
        .unwrap_or_else(|| candidate.parameters_schema.clone());
    if !parameters_schema.is_object() {
        return Err(String::from(
            "tool.registration field `parameters_schema` must be a JSON object.",
        ));
    }
    // A malformed transformed schema must be rejected before it reaches the provider request.
    // 变换后的无效 Schema 必须在进入 Provider 请求前拒绝。
    jsonschema::validator_for(&parameters_schema)
        .map_err(|error| format!("tool.registration produced an invalid tool schema: {error}"))?;
    let ai_visible = match definition.get("ai_visible") {
        None => candidate.ai_visible,
        Some(value) => value
            .as_bool()
            .ok_or_else(|| String::from("tool.registration field `ai_visible` must be boolean."))?,
    };

    Ok(CachedToolRegistrationDefinition {
        id: string_field("id", &candidate.descriptor.id)?,
        name: string_field("name", &candidate.descriptor.name)?,
        description: string_field("description", &candidate.descriptor.description)?,
        help_text,
        parameters_schema,
        ai_visible,
    })
}

/// Applies one registration hook chain exactly once for a cache key.
/// 按一个缓存键恰好执行一次注册 Hook 链。
fn apply_registration_hooks(
    runtime: VulcanPluginRuntimeManager,
    snapshot: &VulcanPluginCatalogSnapshot,
    candidate: VulcanToolCandidate,
    hooks: &[VulcanPluginHookDescriptor],
) -> Result<VulcanToolCandidate, String> {
    let key = registration_cache_key(snapshot, &candidate, hooks);
    let outcome = cached_or_compute_registration_outcome(&key, || {
        let payload = registration_payload(&candidate);
        let report = run_hook_future_from_sync(dispatch_tool_hook_descriptors(
            runtime.clone(),
            hooks.to_vec(),
            registration_hook_context(),
            payload,
        ))
        .map_err(|error| format!("tool.registration failed: {error}"))?;
        if report.decision == HookDecision::Block {
            // A declared Block is stable, while Defer and execution errors remain retryable.
            // 显式 Block 稳定有效，Defer 与执行错误则保持可重试。
            return Ok(CachedToolRegistrationOutcome::Omitted);
        }
        if report.decision != HookDecision::Continue {
            return Err(format!(
                "tool.registration returned {:?}; the tool is omitted.",
                report.decision
            ));
        }
        parse_registration_definition(&report.payload.value, &candidate)
            .map(CachedToolRegistrationOutcome::Definition)
    })?;
    // A cached definition can outlive a concurrent plugin disable. Validate the current
    // generation even on a cache hit before exposing that definition to the model.
    // 并发停用插件后缓存定义可能仍在；即使命中缓存，也要在暴露给模型前核对当前代次。
    validate_tool_hook_lifecycle(&runtime, hooks)
        .map_err(|error| format!("tool.registration lifecycle is stale: {error}"))?;
    match outcome {
        CachedToolRegistrationOutcome::Definition(definition) => {
            Ok(apply_cached_registration_definition(candidate, definition))
        }
        CachedToolRegistrationOutcome::Omitted => {
            Err(String::from("tool.registration omitted this tool."))
        }
    }
}

/// Extracts transformed arguments from a tool-before response.
/// 从 tool.before 响应提取变换后的参数。
fn parse_before_arguments(payload: &Value, origin: &VulcanToolOrigin) -> Result<Value, String> {
    let payload = payload
        .as_object()
        .ok_or_else(|| String::from("tool.before must return an object payload."))?;
    let tool = payload
        .get("tool")
        .ok_or_else(|| String::from("tool.before payload must contain tool identity."))?;
    if !response_identity_matches_origin(tool, origin) {
        return Err(String::from(
            "tool.before cannot change the immutable tool origin.",
        ));
    }
    payload
        .get("arguments")
        .cloned()
        .ok_or_else(|| String::from("tool.before payload must contain arguments."))
}

/// Validates arguments against one frozen public or original tool definition.
/// 按一份冻结的公开或原始工具定义校验参数。
///
/// `schema` is the exact definition for `phase`; `arguments` is the candidate payload.
/// Returns an error without echoing potentially sensitive argument values.
/// `schema` 是 `phase` 阶段的精确定义，`arguments` 是候选载荷；错误不会回显可能敏感的参数值。
fn validate_hook_tool_arguments(
    schema: &Value,
    arguments: &Value,
    phase: &str,
) -> Result<(), String> {
    let validator = jsonschema::validator_for(schema)
        .map_err(|error| format!("{phase} tool schema is invalid: {error}"))?;
    if validator.is_valid(arguments) {
        Ok(())
    } else {
        Err(format!(
            "{phase} tool arguments do not match the declared schema"
        ))
    }
}

/// Extracts the model-visible result from a tool-after response.
/// 从 tool.after 响应提取模型可见结果。
fn parse_after_result(payload: &Value, origin: &VulcanToolOrigin) -> Result<Value, String> {
    let payload = payload
        .as_object()
        .ok_or_else(|| String::from("tool.after must return an object payload."))?;
    let tool = payload
        .get("tool")
        .ok_or_else(|| String::from("tool.after payload must contain tool identity."))?;
    if !response_identity_matches_origin(tool, origin) {
        return Err(String::from(
            "tool.after cannot change the immutable tool origin.",
        ));
    }
    payload
        .get("result")
        .cloned()
        .ok_or_else(|| String::from("tool.after payload must contain result."))
}

/// Keeps a completed tool result visible when only post-processing fails.
/// 仅后置处理失败时保留已完成工具的结果可见性。
///
/// `result` is the already executed upstream result. Returns it with an explicit warning so the
/// model cannot mistake post-processing failure for an unexecuted action and retry blindly.
/// `result` 是已经执行的上游结果；返回附带明确警示的结果，避免模型把后置失败误认作动作未执行而盲目重试。
fn completed_with_post_hook_failure(
    result: VulcanToolInvocationResult,
) -> VulcanToolInvocationResult {
    VulcanToolInvocationResult {
        model_output: serde_json::json!({
            "status": "completed_with_postprocessing_error",
            "result": result.model_output,
            "warning": "The underlying tool already completed. A post-processing hook failed; do not automatically retry the action."
        }),
        host_result: result.host_result,
    }
}

/// Wraps one candidate with the explicitly matched before and after hook chains.
/// 用显式匹配的调用前与调用后 Hook 链包装单个候选。
fn wrap_candidate_with_tool_hooks(
    mut candidate: VulcanToolCandidate,
    runtime: VulcanPluginRuntimeManager,
    original_schema: Value,
    lifecycle_hooks: Vec<VulcanPluginHookDescriptor>,
    before_hooks: Vec<VulcanPluginHookDescriptor>,
    after_hooks: Vec<VulcanPluginHookDescriptor>,
    workspace_root: Option<PathBuf>,
) -> VulcanToolCandidate {
    if lifecycle_hooks.is_empty() && before_hooks.is_empty() && after_hooks.is_empty() {
        return candidate;
    }
    let origin = candidate.descriptor.origin().clone();
    let public_name = candidate.descriptor.name.clone();
    let public_schema = candidate.parameters_schema.clone();
    let base_invoke = candidate.invoke.clone();
    candidate.invoke = Arc::new(move |arguments, invocation_context| {
        let runtime = runtime.clone();
        let origin = origin.clone();
        let public_name = public_name.clone();
        let public_schema = public_schema.clone();
        let original_schema = original_schema.clone();
        let lifecycle_hooks = lifecycle_hooks.clone();
        let before_hooks = before_hooks.clone();
        let after_hooks = after_hooks.clone();
        let workspace_root = workspace_root.clone();
        let base_invoke = base_invoke.clone();
        Box::pin(async move {
            validate_hook_tool_arguments(&public_schema, &arguments, "public")?;
            if !lifecycle_hooks.is_empty() {
                validate_tool_hook_lifecycle(&runtime, &lifecycle_hooks)
                    .map_err(|error| format!("tool hook lifecycle is stale: {error}"))?;
            }
            let effective_arguments = if before_hooks.is_empty() {
                arguments
            } else {
                let report = dispatch_tool_hook_descriptors(
                    runtime.clone(),
                    before_hooks,
                    tool_hook_context(
                        &invocation_context,
                        vulcan_plugin_runtime::VULCAN_PLUGIN_HOOK_TOOL_BEFORE,
                        workspace_root.as_ref(),
                    ),
                    before_payload(&origin, &public_name, arguments),
                )
                .await
                .map_err(|error| format!("tool.before failed: {error}"))?;
                if report.decision != HookDecision::Continue {
                    return Err(format!(
                        "tool.before returned {:?}; tool invocation was rejected.",
                        report.decision
                    ));
                }
                parse_before_arguments(&report.payload.value, &origin)?
            };
            validate_hook_tool_arguments(&original_schema, &effective_arguments, "original")?;
            // Plugin disable can complete while Lua is transforming arguments. Recheck the
            // exact generation before the underlying tool receives the transformed request.
            // Lua 变换参数期间插件可能停用；在原工具收到变换请求前重新校验精确代次。
            if !lifecycle_hooks.is_empty() {
                validate_tool_hook_lifecycle(&runtime, &lifecycle_hooks)
                    .map_err(|error| format!("tool hook lifecycle is stale: {error}"))?;
            }
            let result =
                base_invoke(effective_arguments.clone(), invocation_context.clone()).await?;
            if after_hooks.is_empty() {
                return Ok(result);
            }
            let report = dispatch_tool_hook_descriptors(
                runtime,
                after_hooks,
                tool_hook_context(
                    &invocation_context,
                    vulcan_plugin_runtime::VULCAN_PLUGIN_HOOK_TOOL_AFTER,
                    workspace_root.as_ref(),
                ),
                after_payload(
                    &origin,
                    &public_name,
                    effective_arguments,
                    result.model_output.clone(),
                ),
            )
            .await;
            let Ok(report) = report else {
                return Ok(completed_with_post_hook_failure(result));
            };
            if report.decision != HookDecision::Continue {
                return Ok(completed_with_post_hook_failure(result));
            }
            let model_output = match parse_after_result(&report.payload.value, &origin) {
                Ok(model_output) => model_output,
                Err(_) => return Ok(completed_with_post_hook_failure(result)),
            };
            Ok(VulcanToolInvocationResult {
                model_output,
                host_result: result.host_result,
            })
        })
    });
    candidate
}

/// Applies catalog registration transforms and invocation hooks to all host candidates.
/// 对所有宿主候选应用目录注册变换与调用生命周期 Hook。
///
/// Registration hooks execute before public-name normalization and are cached by catalog revision,
/// plugin generation, immutable origin, hook declarations, and the source definition digest.
/// 注册 Hook 在公开名称归一化前执行，并按目录修订、插件代次、不可变来源、Hook 声明及来源定义摘要缓存。
pub fn prepare_vulcan_plugin_tool_candidates(
    runtime: Option<VulcanPluginRuntimeManager>,
    candidates: Vec<VulcanToolCandidate>,
    workspace_root: Option<PathBuf>,
) -> Vec<VulcanToolCandidate> {
    let Some(runtime) = runtime else {
        return candidates;
    };
    let snapshot = runtime.snapshot();
    if !snapshot
        .enabled_hooks
        .values()
        .any(|hook| is_generic_tool_hook_point(&hook.manifest.point))
    {
        return candidates;
    }
    purge_stale_registration_cache_entries(&snapshot);
    candidates
        .into_iter()
        .filter_map(|candidate| {
            let origin = candidate.descriptor.origin().clone();
            let original_schema = candidate.parameters_schema.clone();
            let registration_hooks = matching_tool_hooks(
                &snapshot,
                &origin,
                vulcan_plugin_runtime::VULCAN_PLUGIN_HOOK_TOOL_REGISTRATION,
            );
            // Registration definitions are catalog-wide. A session-scoped declaration cannot be
            // safely cached without a session-bound catalog, so it fails closed.
            // 注册定义属于目录级数据；会话作用域声明没有会话绑定目录，无法安全缓存，因此直接关闭。
            if registration_hooks
                .iter()
                .any(|hook| hook.manifest.scope == HookScope::Session)
            {
                return None;
            }
            let candidate = if registration_hooks.is_empty() {
                candidate
            } else {
                apply_registration_hooks(runtime.clone(), &snapshot, candidate, &registration_hooks)
                    .ok()?
            };
            let before_hooks = matching_tool_hooks(
                &snapshot,
                candidate.descriptor.origin(),
                vulcan_plugin_runtime::VULCAN_PLUGIN_HOOK_TOOL_BEFORE,
            );
            let after_hooks = matching_tool_hooks(
                &snapshot,
                candidate.descriptor.origin(),
                vulcan_plugin_runtime::VULCAN_PLUGIN_HOOK_TOOL_AFTER,
            );
            Some(wrap_candidate_with_tool_hooks(
                candidate,
                runtime.clone(),
                original_schema,
                registration_hooks
                    .iter()
                    .chain(before_hooks.iter())
                    .chain(after_hooks.iter())
                    .cloned()
                    .collect(),
                before_hooks,
                after_hooks,
                workspace_root.clone(),
            ))
        })
        .collect()
}

/// Dispatches one host-owned hook point through the current enabled plugin snapshot.
/// 通过当前已启用插件快照分发一个宿主持有 Hook 点。
///
/// # Parameters
/// 参数
/// `runtime` owns plugin leases, `context` identifies the host point, and `payload` is exact input.
/// `runtime` 持有插件租约，`context` 标识宿主点位，`payload` 是精确输入。
///
/// # Returns
/// 返回值
/// Returns the shared dispatcher report with transformed payload and generic contributions.
/// 返回包含转换后载荷与通用贡献的共享调度报告。
pub async fn dispatch_vulcan_plugin_hook(
    runtime: VulcanPluginRuntimeManager,
    context: HookContext,
    payload: Value,
) -> Result<HookDispatchReport, HookError> {
    if is_generic_tool_hook_point(&context.point) {
        return Err(HookError::invalid_registration(
            "Generic tool hook dispatch requires the candidate's immutable origin.",
        ));
    }
    let mut dispatcher = new_host_hook_dispatcher()?;
    let snapshot = runtime.snapshot();
    synchronize_vulcan_plugin_hooks(&mut dispatcher, runtime, &snapshot)?;
    dispatcher
        .dispatch(HookDispatchRequest::new(
            context,
            HookPayload::from_serializable(payload)?,
        ))
        .await
}

/// Dispatches one generic tool hook after exact immutable-origin matching.
/// 在按不可变来源身份精确匹配后调度一个通用工具 Hook。
///
/// # Parameters
/// 参数
/// `runtime` owns plugin leases, `origin` is captured before public-name normalization,
/// `context` carries the trusted invocation session, and `payload` is the point-specific frame.
/// `runtime` 持有插件租约，`origin` 是公开名称归一化前捕获的来源身份，`context` 携带可信调用会话，`payload` 是点位专用帧。
///
/// # Returns
/// 返回值
/// Returns the transformed report from hooks whose declared target exactly equals `origin`.
/// 返回声明目标与 `origin` 完全相等的 Hook 产生的变换报告。
pub async fn dispatch_vulcan_plugin_tool_hook(
    runtime: VulcanPluginRuntimeManager,
    origin: &VulcanToolOrigin,
    context: HookContext,
    payload: Value,
) -> Result<HookDispatchReport, HookError> {
    if !is_generic_tool_hook_point(&context.point) {
        return Err(HookError::invalid_registration(format!(
            "Point `{}` is not a generic tool hook point.",
            context.point
        )));
    }
    let snapshot = runtime.snapshot();
    let hooks = matching_tool_hooks(&snapshot, origin, &context.point);
    dispatch_tool_hook_descriptors(runtime, hooks, context, payload).await
}

/// Atomically synchronizes enabled manifest hooks into the existing host dispatcher.
/// 将已启用的清单 hook 原子同步到现有宿主调度器。
///
/// # Parameters
/// 参数
/// `dispatcher` is the existing host dispatcher, `runtime` executes Lua handlers, and `catalog` supplies the enabled hook snapshot.
/// `dispatcher` 是现有宿主调度器，`runtime` 执行 Lua 处理器，`catalog` 提供已启用 Hook 快照。
///
/// # Returns
/// 返回值
/// Returns success after atomically replacing only Vulcan Plugin registrations.
/// 在仅原子替换 Vulcan 插件注册项后返回成功。
pub fn synchronize_vulcan_plugin_hooks(
    dispatcher: &mut HookDispatcher,
    runtime: VulcanPluginRuntimeManager,
    catalog: &VulcanPluginCatalogSnapshot,
) -> Result<(), HookError> {
    // Service bootstrap owns the long-lived conversation dispatcher. Install the generic points
    // there before plugin lifecycle validation can enable a declared tool hook.
    // 服务启动拥有长寿命的会话调度器；在插件生命周期校验启用工具 Hook 前先注册通用点位。
    for descriptor in generic_tool_hook_point_descriptors() {
        if dispatcher
            .registry()
            .point_descriptor(&descriptor.point)
            .is_none()
        {
            dispatcher.register_point(descriptor)?;
        }
    }
    let enabled_hooks = catalog
        .enabled_hooks
        .values()
        .filter(|hook| !is_generic_tool_hook_point(&hook.manifest.point))
        .cloned()
        .collect::<Vec<_>>();
    synchronize_vulcan_plugin_hook_descriptors(dispatcher, runtime, &enabled_hooks)
}

/// Validates plugin hook declarations against the host-owned point registry without mutation.
/// 在不产生变更的前提下，依据宿主自有点位注册表校验插件 hook 声明。
///
/// # Parameters
/// 参数
/// `dispatcher` owns valid host point descriptors and `hooks` contains candidate plugin declarations.
/// `dispatcher` 拥有有效宿主点位描述，`hooks` 包含候选插件声明。
///
/// # Returns
/// 返回值
/// Returns success only when every point exists and accepts the declared mode.
/// 仅当每个点位存在且接受声明模式时返回成功。
pub fn validate_vulcan_plugin_hooks(
    dispatcher: &HookDispatcher,
    hooks: &[VulcanPluginHookDescriptor],
) -> Result<(), HookError> {
    for hook in hooks {
        let point = dispatcher
            .registry()
            .point_descriptor(&hook.manifest.point)
            .ok_or_else(|| {
                HookError::invalid_registration(format!(
                    "Vulcan Plugin hook `{}` references unknown host point `{}`.",
                    hook.registration_id, hook.manifest.point
                ))
            })?;
        if !point.modes.contains(&hook.manifest.mode) {
            return Err(HookError::invalid_registration(format!(
                "Vulcan Plugin hook `{}` mode {:?} is not accepted by host point `{}`.",
                hook.registration_id, hook.manifest.mode, hook.manifest.point
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use luaskills::{LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
    use serde_json::json;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::{Barrier, RwLock};
    use vulcan_hooks::{HookMode, HookPointDescriptor};
    use vulcan_tools_manager::{
        VulcanPlanToolPolicy, VulcanToolDescriptor, VulcanToolOrigin, VulcanToolSourceKind,
    };

    /// Verifies simultaneous cold misses invoke one registration transform and release its flight lock.
    /// 验证并发首次缓存未命中只执行一次注册变换，并在结束后释放飞行锁。
    #[test]
    fn concurrent_registration_cache_misses_compute_only_once() {
        let root = tempfile::tempdir().expect("create cache test root");
        let key = ToolRegistrationCacheKey {
            root: root.path().to_path_buf(),
            user_root: None,
            catalog_revision: 1,
            origin: VulcanToolOrigin::new(VulcanToolSourceKind::Mcp, "fixture", "read_page"),
            definition_digest: String::from("definition"),
            registration_hooks_digest: String::from("hooks"),
        };
        let started = Arc::new(Barrier::new(8));
        let computations = Arc::new(AtomicUsize::new(0));
        std::thread::scope(|scope| {
            let handles = (0..8)
                .map(|_| {
                    let key = key.clone();
                    let started = Arc::clone(&started);
                    let computations = Arc::clone(&computations);
                    scope.spawn(move || {
                        started.wait();
                        cached_or_compute_registration_outcome(&key, || {
                            computations.fetch_add(1, AtomicOrdering::SeqCst);
                            std::thread::sleep(std::time::Duration::from_millis(75));
                            Ok(CachedToolRegistrationOutcome::Definition(
                                CachedToolRegistrationDefinition {
                                    id: String::from("public_read"),
                                    name: String::from("public_read"),
                                    description: String::from("one registration"),
                                    help_text: None,
                                    parameters_schema: json!({"type": "object"}),
                                    ai_visible: true,
                                },
                            ))
                        })
                        .expect("one registration result")
                    })
                })
                .collect::<Vec<_>>();
            for handle in handles {
                let outcome = handle.join().expect("registration worker");
                assert!(matches!(
                    outcome,
                    CachedToolRegistrationOutcome::Definition(_)
                ));
            }
        });
        assert_eq!(computations.load(AtomicOrdering::SeqCst), 1);
        assert!(
            !TOOL_REGISTRATION_FLIGHTS
                .get()
                .expect("flight map")
                .lock()
                .expect("lock flight map")
                .contains_key(&key)
        );
    }

    /// Verifies a large current catalog keeps every registration result for later requests.
    /// 验证大型当前目录中的每个注册结果都保留供后续请求复用。
    #[test]
    fn current_catalog_registration_cache_does_not_evict_tools() {
        let root = tempfile::tempdir().expect("create large cache test root");
        let keys = (0..1050)
            .map(|index| ToolRegistrationCacheKey {
                root: root.path().to_path_buf(),
                user_root: None,
                catalog_revision: 1,
                origin: VulcanToolOrigin::new(
                    VulcanToolSourceKind::Mcp,
                    "fixture",
                    format!("tool_{index}"),
                ),
                definition_digest: String::from("definition"),
                registration_hooks_digest: String::from("hooks"),
            })
            .collect::<Vec<_>>();
        for key in &keys {
            cache_registration_outcome(key.clone(), CachedToolRegistrationOutcome::Omitted);
        }
        assert!(
            keys.iter()
                .all(|key| cached_registration_outcome(key).is_some()),
            "one current catalog must never re-enter Lua because another tool was registered"
        );
    }

    /// Verifies an older in-flight catalog cannot erase a newer registration result.
    /// 验证在途旧目录不能清除更新修订的注册结果。
    #[test]
    fn stale_catalog_purge_preserves_newer_registration_results() {
        let root = tempfile::tempdir().expect("create revision cache test root");
        let key = ToolRegistrationCacheKey {
            root: root.path().to_path_buf(),
            user_root: None,
            catalog_revision: 2,
            origin: VulcanToolOrigin::new(VulcanToolSourceKind::Mcp, "fixture", "read_page"),
            definition_digest: String::from("definition"),
            registration_hooks_digest: String::from("hooks"),
        };
        cache_registration_outcome(key.clone(), CachedToolRegistrationOutcome::Omitted);
        let mut snapshot = VulcanPluginCatalogSnapshot::empty(root.path().to_path_buf());
        snapshot.revision = 1;
        purge_stale_registration_cache_entries(&snapshot);
        assert!(cached_registration_outcome(&key).is_some());
        let mut retired_key = key.clone();
        retired_key.catalog_revision = 1;
        cache_registration_outcome(retired_key.clone(), CachedToolRegistrationOutcome::Omitted);
        assert!(cached_registration_outcome(&retired_key).is_none());
        snapshot.revision = 3;
        purge_stale_registration_cache_entries(&snapshot);
        assert!(cached_registration_outcome(&key).is_none());
    }

    /// Writes one minimal hook-only Vulcan Plugin package.
    /// 写入一个最小化的仅含 hook 的 Vulcan 插件包。
    fn write_hook_package(root: &Path) {
        let package = root.join("hook-plugin");
        fs::create_dir_all(package.join("runtime")).expect("create hook runtime directory");
        fs::write(package.join("dependencies.yaml"), "{}\n").expect("write dependencies");
        fs::write(
            package.join("runtime/plugin.bundle.lua"),
            r#"local M = {}
local count = 0
function M.dispatch() return '{"ok":true,"result":{}}' end
function M.shutdown() return '{"ok":true,"result":{}}' end
function M.on_precheck(_args)
    count = count + 1
    return '{"ok":true,"result":{"payload":{"value":{"count":' .. tostring(count) .. '}}}}'
end
return M
"#,
        )
        .expect("write hook Lua module");
        fs::write(
            package.join("plugin.yaml"),
            r#"schema_version: 1
id: hook-plugin
version: 1.0.0
default_enabled: true
display:
  name: Hook Plugin
  capability: Host hook bridge fixture
execution:
  dependencies:
    manifest: dependencies.yaml
    install_mode: bundled
lua_runtime:
  bundle: runtime/plugin.bundle.lua
  invoke_function: dispatch
  shutdown_function: shutdown
  max_active_sessions: 1
  hooks:
    - id: precheck
      point: conversation.turn.precheck
      mode: transform
      function: on_precheck
      timeout_ms: 20000
      failure_policy: stop
"#,
        )
        .expect("write hook manifest");
    }

    /// Builds one real LuaSkills engine rooted at the test runtime layout.
    /// 构建一个以测试运行布局为根的真实 LuaSkills 引擎。
    fn test_engine(runtime_root: &Path) -> LuaEngine {
        // native_runtime_root keeps the integration-test Lua package paths free of Windows verbatim prefixes.
        // native_runtime_root 使集成测试的 Lua package 路径不包含 Windows verbatim 前缀。
        let native_runtime_root = crate::runtime_assets::native_host_path(
            &runtime_root
                .canonicalize()
                .expect("canonicalize LuaSkills runtime root"),
        );
        LuaEngine::new(LuaEngineOptions::new(
            LuaVmPoolConfig {
                min_size: 1,
                max_size: 1,
                idle_ttl_secs: 60,
            },
            LuaRuntimeHostOptions::with_runtime_root(native_runtime_root),
        ))
        .expect("create LuaSkills test engine")
    }

    /// Verifies atomic registration, idempotent synchronization, and draining removal.
    /// 验证原子注册、幂等同步与 draining 移除。
    #[test]
    fn vulcan_plugin_hooks_share_existing_dispatcher_and_lifecycle() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime_root = temp.path().join("runtime");
        let plugin_root = runtime_root.join("plugins");
        fs::create_dir_all(&plugin_root).expect("create Vulcan Plugin root");
        write_hook_package(&plugin_root);
        let bridge_root = runtime_root
            .join("system_lua_lib")
            .join("vulcan-plugin-bridge");
        fs::create_dir_all(bridge_root.join("runtime")).expect("create bridge runtime directory");
        fs::write(bridge_root.join("dependencies.yaml"), "{}\n")
            .expect("write bridge dependencies");
        fs::write(
            bridge_root.join("runtime/bridge.lua"),
            include_str!("../../../../plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua"),
        )
        .expect("write bridge module");
        let user_plugin_root = runtime_root.join("user_plugins");
        let managed_runtime_root = runtime_root.join("managed_runtime");
        let plugin_state_root = runtime_root.join("plugin_state");
        let plugin_runtime_root = runtime_root.join("plugin_runtime");
        fs::create_dir_all(&user_plugin_root).expect("create user plugin root");
        fs::create_dir_all(&managed_runtime_root).expect("create managed runtime root");
        fs::create_dir_all(&plugin_state_root).expect("create plugin state root");
        fs::create_dir_all(&plugin_runtime_root).expect("create plugin runtime root");
        let manager = VulcanPluginRuntimeManager::new(
            Arc::new(RwLock::new(test_engine(&runtime_root))),
            plugin_root,
            user_plugin_root,
            bridge_root,
            managed_runtime_root,
            plugin_state_root,
            plugin_runtime_root,
        );
        let catalog = manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load hook catalog");
        assert_eq!(
            catalog.enabled_hooks.len(),
            1,
            "hook fixture must expose exactly one enabled registration"
        );
        let mut dispatcher = HookDispatcher::default();
        dispatcher
            .register_point(HookPointDescriptor::new(
                "conversation.turn.precheck",
                [HookMode::Transform],
                "Test precheck point.",
            ))
            .expect("register host point");

        let mut invalid_catalog = (*catalog).clone();
        for hook in invalid_catalog.enabled_hooks.values_mut() {
            hook.manifest.point = String::from("unknown.point");
        }
        for plugin in invalid_catalog.plugins.values_mut() {
            for hook in &mut plugin.hooks {
                hook.manifest.point = String::from("unknown.point");
            }
        }
        let invalid =
            synchronize_vulcan_plugin_hooks(&mut dispatcher, manager.clone(), &invalid_catalog);
        assert!(invalid.is_err());
        assert!(
            dispatcher
                .registry()
                .registration_snapshots(None)
                .is_empty()
        );

        synchronize_vulcan_plugin_hooks(&mut dispatcher, manager.clone(), &catalog)
            .expect("register plugin hook");
        synchronize_vulcan_plugin_hooks(&mut dispatcher, manager.clone(), &catalog)
            .expect("resynchronize plugin hook");
        assert_eq!(dispatcher.registry().registration_snapshots(None).len(), 1);
        manager
            .begin_draining("hook-plugin")
            .expect("begin plugin draining");
        synchronize_vulcan_plugin_hooks(&mut dispatcher, manager.clone(), &manager.snapshot())
            .expect("remove draining plugin hook");
        assert!(
            dispatcher
                .registry()
                .registration_snapshots(None)
                .is_empty()
        );
    }

    /// Builds a minimal candidate used by pure registration-frame tests.
    /// 构建纯注册帧测试使用的最小候选工具。
    fn test_candidate(origin: VulcanToolOrigin) -> VulcanToolCandidate {
        VulcanToolCandidate {
            descriptor: VulcanToolDescriptor::new(
                "read_file",
                "read_file",
                "Read a file",
                None,
                origin,
            ),
            parameters_schema: json!({"type": "object"}),
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::Visible,
            request_streaming: None,
            invoke: Arc::new(|_, _| {
                Box::pin(async { Ok(VulcanToolInvocationResult::from_model_output(json!("ok"))) })
            }),
        }
    }

    /// Verifies that registration changes cannot replace immutable origin metadata.
    /// 验证注册变换不能替换不可变来源元数据。
    #[test]
    fn registration_transform_preserves_origin_after_public_rename() {
        let origin = VulcanToolOrigin::new(VulcanToolSourceKind::Mcp, "server-a", "read_file");
        let candidate = test_candidate(origin.clone());
        let payload = json!({
            "tool": {
                "source_kind": "mcp",
                "namespace": "server-a",
                "original_name": "read_file"
            },
            "definition": {
                "id": "read_file_renamed",
                "name": "read_file_renamed",
                "description": "Transformed description",
                "parameters_schema": {"type": "object", "properties": {"path": {"type": "string"}}},
                "ai_visible": true
            }
        });
        let transformed = parse_registration_definition(&payload, &candidate)
            .expect("registration response should parse");
        let transformed_candidate = apply_cached_registration_definition(candidate, transformed);
        assert_eq!(transformed_candidate.descriptor.origin(), &origin);
        assert_eq!(transformed_candidate.descriptor.id, "read_file_renamed");
        assert_eq!(transformed_candidate.descriptor.name, "read_file_renamed");
    }

    /// Verifies malformed Lua schemas are omitted before a provider can receive them.
    /// 验证 Lua 产生的无效 Schema 在发给 Provider 前被拒绝。
    #[test]
    fn registration_transform_rejects_invalid_public_schema() {
        let candidate = test_candidate(VulcanToolOrigin::new(
            VulcanToolSourceKind::Mcp,
            "server-a",
            "read_file",
        ));
        let payload = json!({
            "tool": {
                "source_kind": "mcp",
                "namespace": "server-a",
                "original_name": "read_file"
            },
            "definition": {
                "parameters_schema": {"type": "not-a-json-schema-type"}
            }
        });
        assert!(
            parse_registration_definition(&payload, &candidate)
                .expect_err("invalid public schema must fail closed")
                .contains("invalid tool schema")
        );
    }

    /// Verifies exact target matching uses source identity instead of a renamed public name.
    /// 验证精确目标匹配使用来源身份，而不是改名后的公开名称。
    #[test]
    fn tool_hook_target_matching_ignores_public_name() {
        let manifest: vulcan_plugin_runtime::VulcanPluginHookManifest = serde_yaml::from_str(
            r#"id: before
point: tool.before
tool_target:
  source_kind: mcp
  namespace: server-a
  original_name: read_file
mode: transform
function: before
"#,
        )
        .expect("tool hook manifest should parse");
        let hook = VulcanPluginHookDescriptor {
            plugin_id: String::from("plugin-a"),
            plugin_generation: 7,
            registration_id: String::from("plugin-a/before"),
            manifest,
        };
        let origin = VulcanToolOrigin::new(VulcanToolSourceKind::Mcp, "server-a", "read_file");
        assert!(tool_hook_targets_origin(&hook, &origin));
        let renamed_origin = VulcanToolOrigin::new(VulcanToolSourceKind::Mcp, "server-a", "other");
        assert!(!tool_hook_targets_origin(&hook, &renamed_origin));
    }

    /// Verifies a hidden owner parameter is rejected publicly and required after the hook.
    /// 验证隐藏的所有者参数在公开入口被拒绝，并在 Hook 后成为原始入口的必填参数。
    #[test]
    fn hook_arguments_validate_public_and_original_schemas() {
        let public_schema = json!({
            "type": "object",
            "properties": { "url": { "type": "string" } },
            "required": ["url"],
            "additionalProperties": false
        });
        let original_schema = json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "session_id": { "type": "string" }
            },
            "required": ["url", "session_id"],
            "additionalProperties": false
        });
        let public_arguments = json!({ "url": "https://example.test" });
        assert!(validate_hook_tool_arguments(&public_schema, &public_arguments, "public").is_ok());
        assert!(
            validate_hook_tool_arguments(
                &public_schema,
                &json!({ "url": "https://example.test", "session_id": "forged" }),
                "public"
            )
            .is_err()
        );
        assert!(
            validate_hook_tool_arguments(&original_schema, &public_arguments, "original").is_err()
        );
        assert!(
            validate_hook_tool_arguments(
                &original_schema,
                &json!({ "url": "https://example.test", "session_id": "trusted" }),
                "original"
            )
            .is_ok()
        );
    }

    /// Verifies a post-hook failure cannot recast an executed action as a failed tool call.
    /// 验证后置 Hook 失败不会把已执行动作改写成失败的工具调用。
    #[test]
    fn post_hook_failure_preserves_completed_tool_result() {
        let result = VulcanToolInvocationResult::from_model_output(json!({ "clicked": true }));
        let projected = completed_with_post_hook_failure(result);
        assert_eq!(
            projected.model_output["status"],
            "completed_with_postprocessing_error"
        );
        assert_eq!(projected.model_output["result"]["clicked"], true);
    }

    /// Builds a real invocation identity for the product-plugin hook integration test.
    /// 为产品插件 Hook 集成测试构建真实的调用身份。
    fn browser_hook_test_context() -> VulcanToolInvocationContext {
        use vulcan_session_core::v1::ids::{ApiCallId, RequestStreamId, Seq, ToolCallId};

        VulcanToolInvocationContext {
            session_id: String::from("browser-hook-session"),
            request_stream_id: RequestStreamId::try_new("browser-hook-stream")
                .expect("test request stream id"),
            api_call_id: ApiCallId::try_new(1).expect("test API id"),
            tool_call_id: ToolCallId::try_new(2).expect("test tool id"),
            tool_seq: Seq::try_new(3).expect("test tool seq"),
            controller_tool_call_id: String::from("browser-hook-call"),
            provider_tool_call_id: None,
        }
    }

    /// Creates an isolated manager whose bundled root points at the product browser plugin.
    /// 创建一个隔离的插件管理器，并把内建根目录指向产品浏览器插件。
    fn browser_plugin_test_manager(runtime_root: &Path) -> VulcanPluginRuntimeManager {
        let bundled_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("plugins");
        plugin_hook_test_manager(runtime_root, bundled_root)
    }

    /// Creates an isolated manager with one explicit bundled plugin root.
    /// 使用显式内建插件根目录创建隔离管理器。
    fn plugin_hook_test_manager(
        runtime_root: &Path,
        bundled_root: PathBuf,
    ) -> VulcanPluginRuntimeManager {
        let bridge_root = runtime_root
            .join("system_lua_lib")
            .join("vulcan-plugin-bridge");
        fs::create_dir_all(bridge_root.join("runtime")).expect("create bridge runtime directory");
        fs::write(bridge_root.join("dependencies.yaml"), "{}\n")
            .expect("write bridge dependencies");
        fs::write(
            bridge_root.join("runtime/bridge.lua"),
            include_str!("../../../../plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua"),
        )
        .expect("write bridge module");
        let user_plugin_root = runtime_root.join("user_plugins");
        let managed_runtime_root = runtime_root.join("managed_runtime");
        let plugin_state_root = runtime_root.join("plugin_state");
        let plugin_runtime_root = runtime_root.join("plugin_runtime");
        fs::create_dir_all(&user_plugin_root).expect("create user plugin root");
        fs::create_dir_all(&managed_runtime_root).expect("create managed runtime root");
        fs::create_dir_all(&plugin_state_root).expect("create plugin state root");
        fs::create_dir_all(&plugin_runtime_root).expect("create plugin runtime root");
        VulcanPluginRuntimeManager::new(
            Arc::new(RwLock::new(test_engine(runtime_root))),
            bundled_root,
            user_plugin_root,
            bridge_root,
            managed_runtime_root,
            plugin_state_root,
            plugin_runtime_root,
        )
    }

    /// Writes a bundled fixture that hides a host-owned session argument before AI registration.
    /// 写入一个在 AI 注册前隐藏宿主会话参数的内建测试插件。
    fn write_session_owner_hook_package(plugin_root: &Path) {
        let package = plugin_root.join("session-owner-hook");
        fs::create_dir_all(package.join("runtime")).expect("create session hook runtime");
        fs::create_dir_all(package.join("config")).expect("create session hook config");
        fs::write(package.join("dependencies.yaml"), "{}\n")
            .expect("write session hook dependencies");
        fs::write(
            package.join("config/schema.json"),
            r#"{"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}"#,
        )
        .expect("write session hook config schema");
        fs::write(
            package.join("config/defaults.json"),
            r#"{"label":"default"}"#,
        )
        .expect("write session hook config defaults");
        fs::write(
            package.join("runtime/plugin.bundle.lua"),
            r#"-- M contains exact source-bound tool hooks.
-- M 包含精确绑定来源的工具 Hook。
local M = {}
local registration_count = 0

-- Return the managed plugin bridge envelope for a successful hook.
-- 返回成功 Hook 的托管插件桥接信封。
local function success(result)
    return vulcan.json.encode({ ok = true, result = result })
end

-- Hide the host-owned session argument and rename only the public tool.
-- 隐藏宿主持有的会话参数，并仅重命名公开工具。
function M.register(args)
    local invocation = vulcan.json.decode(args.invocation_json)
    local payload = invocation.payload.value
    local schema = payload.definition.parameters_schema
    if schema.properties.owner_session_id == nil then error("original owner argument missing") end
    registration_count = registration_count + 1
    schema.properties.owner_session_id = nil
    schema.required = { "url" }
    schema.additionalProperties = false
    payload.definition.id = "public_read"
    payload.definition.name = "public_read"
    local config = vulcan.host.call("vulcan.plugin.config.get", {})
    local suffix = config.label == "default" and "" or " " .. config.label
    payload.definition.description = "Registration run " .. tostring(registration_count) .. suffix
    return success({ payload = { value = payload } })
end

-- Insert the trusted invocation session after the model arguments are validated.
-- 在校验模型参数后插入可信调用会话。
function M.before(args)
    local invocation = vulcan.json.decode(args.invocation_json)
    local payload = invocation.payload.value
    if type(invocation.context.session_id) ~= "string" then error("trusted session missing") end
    if payload.arguments.url == "https://origin-switch.test" then
        payload.tool.original_name = "other_tool"
    end
    payload.arguments.owner_session_id = invocation.context.session_id
    return success({ payload = { value = payload } })
end

-- Fail selected post-processing after the upstream call has completed.
-- 在上游调用完成后，让指定的后置处理失败。
function M.after(args)
    local invocation = vulcan.json.decode(args.invocation_json)
    local payload = invocation.payload.value
    if payload.arguments.url == "https://post-fail.test" then
        error("post-processing fixture failure")
    end
    return success({ payload = { value = payload } })
end

-- Reject unrelated action dispatch because this fixture only declares hooks.
-- 拒绝无关的动作分发，因为本夹具仅声明 Hook。
function M.dispatch() return success({}) end

-- Complete the plugin runtime shutdown with a valid bridge result.
-- 以有效桥接结果完成插件运行时关闭。
function M.shutdown() return success({ closed = true }) end

return M
"#,
        )
        .expect("write session hook Lua module");
        fs::write(
            package.join("plugin.yaml"),
            r#"schema_version: 1
id: session-owner-hook
version: 1.0.0
default_enabled: false
display:
  name: Session Owner Hook
  capability: Exact session argument fixture
permissions:
  - config
config:
  schema_file: config/schema.json
  defaults_file: config/defaults.json
execution:
  dependencies:
    manifest: dependencies.yaml
    install_mode: bundled
lua_runtime:
  bundle: runtime/plugin.bundle.lua
  invoke_function: dispatch
  shutdown_function: shutdown
  max_active_sessions: 2
  hooks:
    - id: registration
      point: tool.registration
      tool_target:
        source_kind: mcp
        namespace: fixture-server
        original_name: read_page
      mode: transform
      scope: global
      function: register
      timeout_ms: 3000
      failure_policy: stop
    - id: before
      point: tool.before
      tool_target:
        source_kind: mcp
        namespace: fixture-server
        original_name: read_page
      mode: transform
      scope: session
      function: before
      timeout_ms: 3000
      failure_policy: stop
    - id: after
      point: tool.after
      tool_target:
        source_kind: mcp
        namespace: fixture-server
        original_name: read_page
      mode: transform
      scope: session
      function: after
      timeout_ms: 3000
      failure_policy: stop
"#,
        )
        .expect("write session hook manifest");
    }

    /// Moves the post-hook into a second plugin so the two hook stages use different dedicated Lua VMs.
    /// 把后置 Hook 移入第二个插件，使两个 Hook 阶段使用不同的专用 Lua VM。
    fn split_session_owner_hooks_across_plugins(plugin_root: &Path) {
        let first_manifest = plugin_root.join("session-owner-hook/plugin.yaml");
        let text = fs::read_to_string(&first_manifest).expect("read owner hook manifest");
        let (before_only, _) = text
            .split_once("    - id: after\n")
            .expect("exact owner post-hook declaration");
        fs::write(&first_manifest, format!("{before_only}\n"))
            .expect("retain owner registration and pre-hook");
        let package = plugin_root.join("session-after-hook");
        fs::create_dir_all(package.join("runtime")).expect("create post-hook runtime");
        fs::write(package.join("dependencies.yaml"), "{}\n").expect("write post-hook dependencies");
        fs::write(
            package.join("runtime/plugin.bundle.lua"),
            r#"-- M keeps only the post-hook in this plugin's dedicated Lua VM.
-- M 仅在本插件的专用 Lua VM 中保留后置 Hook。
local M = {}

-- Add the host-authenticated session to the completed target result.
-- 在已完成的目标结果中加入宿主认证的会话身份。
function M.after(args)
    local invocation = vulcan.json.decode(args.invocation_json)
    local payload = invocation.payload.value
    payload.result.after_session_id = invocation.context.session_id
    return vulcan.json.encode({ ok = true, result = { payload = { value = payload } } })
end

-- Reject unrelated action dispatch in this hook-only fixture.
-- 在仅声明 Hook 的夹具中拒绝无关动作分发。
function M.dispatch() return vulcan.json.encode({ ok = true, result = {} }) end

-- Close the isolated hook runtime with a valid bridge envelope.
-- 以有效桥接信封关闭隔离的 Hook 运行时。
function M.shutdown() return vulcan.json.encode({ ok = true, result = {} }) end

return M
"#,
        )
        .expect("write second plugin Lua runtime");
        fs::write(
            package.join("plugin.yaml"),
            r#"schema_version: 1
id: session-after-hook
version: 1.0.0
default_enabled: false
display:
  name: Session After Hook
  capability: Cross-VM post-hook fixture
execution:
  dependencies:
    manifest: dependencies.yaml
    install_mode: bundled
lua_runtime:
  bundle: runtime/plugin.bundle.lua
  invoke_function: dispatch
  shutdown_function: shutdown
  max_active_sessions: 1
  hooks:
    - id: after
      point: tool.after
      tool_target:
        source_kind: mcp
        namespace: fixture-server
        original_name: read_page
      mode: transform
      scope: session
      function: after
      timeout_ms: 3000
      failure_policy: stop
"#,
        )
        .expect("write second plugin manifest");
    }

    /// Verifies before and after hooks in separate plugin-owned Lua VMs retain one trusted call identity.
    /// 验证位于两个插件专用 Lua VM 中的前后 Hook 保持同一可信调用身份。
    #[tokio::test(flavor = "current_thread")]
    async fn split_plugin_hook_vms_preserve_one_session_invocation() {
        let temp = tempfile::tempdir().expect("cross-VM hook test directory");
        let runtime_root = temp.path().join("runtime");
        fs::create_dir_all(&runtime_root).expect("cross-VM runtime root");
        let bundled_root = runtime_root.join("bundled_plugins");
        write_session_owner_hook_package(&bundled_root);
        split_session_owner_hooks_across_plugins(&bundled_root);
        let plugins = plugin_hook_test_manager(&runtime_root, bundled_root);
        let enabled = BTreeSet::from([
            String::from("session-owner-hook"),
            String::from("session-after-hook"),
        ]);
        let catalog = plugins
            .refresh_catalog(&BTreeSet::new(), &enabled)
            .expect("enable both plugin-owned Hook VMs");
        assert_eq!(catalog.enabled_hooks.len(), 3);
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        let candidate = VulcanToolCandidate {
            descriptor: VulcanToolDescriptor::new(
                "read_page",
                "read_page",
                "Read a page.",
                None,
                VulcanToolOrigin::new(VulcanToolSourceKind::Mcp, "fixture-server", "read_page"),
            ),
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "owner_session_id": {"type": "string"}
                },
                "required": ["url", "owner_session_id"],
                "additionalProperties": false
            }),
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::Visible,
            request_streaming: None,
            invoke: Arc::new(move |arguments, _context| {
                observed.fetch_add(1, AtomicOrdering::SeqCst);
                Box::pin(
                    async move { Ok(VulcanToolInvocationResult::from_model_output(arguments)) },
                )
            }),
        };
        let prepared = prepare_vulcan_plugin_tool_candidates(Some(plugins), vec![candidate], None)
            .into_iter()
            .next()
            .expect("split plugin candidate");
        let mut context = browser_hook_test_context();
        context.session_id = String::from("cross-vm-conversation");
        let result = (prepared.invoke)(json!({"url": "https://cross-vm.test"}), context)
            .await
            .expect("cross-VM hook invocation");
        assert_eq!(
            result.model_output["owner_session_id"],
            "cross-vm-conversation"
        );
        assert_eq!(
            result.model_output["after_session_id"],
            "cross-vm-conversation"
        );
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
    }

    /// Verifies cached registration and trusted session injection across concurrent conversations.
    /// 验证并发会话中的注册缓存与可信会话参数注入。
    #[tokio::test(flavor = "current_thread")]
    async fn session_owner_hook_hides_and_restores_exact_mcp_argument() {
        let temp = tempfile::tempdir().expect("create hook test directory");
        let runtime_root = temp.path().join("runtime");
        fs::create_dir_all(&runtime_root).expect("create hook runtime root");
        let bundled_root = runtime_root.join("bundled_plugins");
        write_session_owner_hook_package(&bundled_root);
        let manager = plugin_hook_test_manager(&runtime_root, bundled_root);
        let enabled = BTreeSet::from([String::from("session-owner-hook")]);
        let catalog = manager
            .refresh_catalog(&BTreeSet::new(), &enabled)
            .expect("enable session owner fixture");
        assert_eq!(
            catalog
                .enabled_hooks
                .values()
                .filter(|hook| hook.plugin_id == "session-owner-hook")
                .count(),
            3,
            "session fixture catalog: {:?}",
            catalog.plugins.get("session-owner-hook")
        );
        let original_schema = json!({
            "type": "object",
            "properties": {
                "url": {"type": "string"},
                "owner_session_id": {"type": "string"}
            },
            "required": ["url", "owner_session_id"],
            "additionalProperties": false
        });
        let observed = Arc::new(Mutex::new(Vec::<Value>::new()));
        let observed_for_invoke = Arc::clone(&observed);
        let candidate = VulcanToolCandidate {
            descriptor: VulcanToolDescriptor::new(
                "read_page",
                "read_page",
                "Read a page.",
                None,
                VulcanToolOrigin::new(VulcanToolSourceKind::Mcp, "fixture-server", "read_page"),
            ),
            parameters_schema: original_schema,
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::Visible,
            request_streaming: None,
            invoke: Arc::new(move |arguments, _context| {
                observed_for_invoke
                    .lock()
                    .expect("record upstream arguments")
                    .push(arguments.clone());
                Box::pin(
                    async move { Ok(VulcanToolInvocationResult::from_model_output(arguments)) },
                )
            }),
        };
        let prepare = |candidate| {
            prepare_vulcan_plugin_tool_candidates(Some(manager.clone()), vec![candidate], None)
                .into_iter()
                .next()
                .expect("exact target must remain available")
        };
        let first = prepare(candidate.clone());
        let cached = prepare(candidate.clone());
        assert_eq!(first.descriptor.name, "public_read");
        assert_eq!(cached.descriptor.description, "Registration run 1");
        assert_eq!(
            first.descriptor.origin(),
            candidate.descriptor.origin(),
            "public rename must preserve the original MCP target"
        );
        assert!(
            first.parameters_schema["properties"]
                .get("owner_session_id")
                .is_none()
        );
        assert_eq!(first.parameters_schema["required"], json!(["url"]));
        let mut first_context = browser_hook_test_context();
        first_context.session_id = String::from("conversation-one");
        first_context.controller_tool_call_id = String::from("call-one");
        let mut second_context = browser_hook_test_context();
        second_context.session_id = String::from("conversation-two");
        second_context.controller_tool_call_id = String::from("call-two");
        let (first_result, second_result) = tokio::join!(
            (first.invoke)(json!({"url": "https://one.test"}), first_context.clone()),
            (cached.invoke)(json!({"url": "https://two.test"}), second_context),
        );
        assert_eq!(
            first_result.expect("first session call").model_output["owner_session_id"],
            "conversation-one"
        );
        assert_eq!(
            second_result.expect("second session call").model_output["owner_session_id"],
            "conversation-two"
        );
        assert_eq!(observed.lock().expect("upstream arguments").len(), 2);
        let forged = (first.invoke)(
            json!({"url": "https://one.test", "owner_session_id": "conversation-two"}),
            first_context.clone(),
        )
        .await;
        assert!(
            forged.is_err(),
            "model-supplied owner must fail public schema validation"
        );
        assert_eq!(observed.lock().expect("upstream arguments").len(), 2);
        let switched = (first.invoke)(
            json!({"url": "https://origin-switch.test"}),
            first_context.clone(),
        )
        .await;
        assert!(
            switched.is_err(),
            "a before hook cannot switch the immutable tool target"
        );
        assert_eq!(observed.lock().expect("upstream arguments").len(), 2);
        let completed = (first.invoke)(json!({"url": "https://post-fail.test"}), first_context)
            .await
            .expect("post-processing failure must preserve completed target result");
        assert_eq!(
            completed.model_output["status"],
            "completed_with_postprocessing_error"
        );
        assert_eq!(
            completed.model_output["result"]["owner_session_id"],
            "conversation-one"
        );
        assert_eq!(observed.lock().expect("upstream arguments").len(), 3);
        let unmatched = VulcanToolCandidate {
            descriptor: VulcanToolDescriptor::new(
                "read_page",
                "read_page",
                "Read a native page.",
                None,
                VulcanToolOrigin::new(VulcanToolSourceKind::Native, "fixture-server", "read_page"),
            ),
            ..candidate.clone()
        };
        let unmatched = prepare(unmatched);
        assert_eq!(unmatched.descriptor.name, "read_page");
        assert!(
            unmatched.parameters_schema["properties"]
                .get("owner_session_id")
                .is_some()
        );
        let original_candidate = candidate.clone();
        let changed_schema = VulcanToolCandidate {
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "owner_session_id": {"type": "string"},
                    "hint": {"type": "string"}
                },
                "required": ["url", "owner_session_id"],
                "additionalProperties": false
            }),
            ..candidate
        };
        let changed = prepare(changed_schema);
        assert_eq!(changed.descriptor.description, "Registration run 2");
        assert_eq!(
            changed.parameters_schema["properties"]["hint"]["type"],
            "string"
        );

        // Updating a plugin configuration must invalidate the registration transform while
        // repeated requests under the same revision still use its cached public definition.
        // 更新插件配置必须使注册变换失效；同一修订下的重复请求仍使用缓存的公开定义。
        let config_revision = manager.snapshot().revision;
        manager
            .update_plugin_config("session-owner-hook", json!({"label": "changed"}))
            .expect("update registration hook configuration");
        assert!(manager.snapshot().revision > config_revision);
        let config_changed = prepare(original_candidate.clone());
        assert_eq!(
            config_changed.descriptor.description,
            "Registration run 3 changed"
        );
        assert!(
            config_changed.parameters_schema["properties"]
                .get("owner_session_id")
                .is_none()
        );
        let config_cached = prepare(original_candidate.clone());
        assert_eq!(
            config_cached.descriptor.description,
            "Registration run 3 changed"
        );
        manager
            .update_plugin_config("session-owner-hook", json!({"label": "default"}))
            .expect("restore fixture configuration");

        // A changed package fingerprint creates a new plugin generation and invalidates the
        // public definition without exposing the original host-only argument in between.
        // 包内容指纹变化生成新插件代次并使公开定义失效，期间不能暴露原始宿主参数。
        let previous = manager.snapshot();
        let bundle =
            runtime_root.join("bundled_plugins/session-owner-hook/runtime/plugin.bundle.lua");
        let old_source = fs::read_to_string(&bundle).expect("read old registration module");
        let updated_source = old_source.replace("Registration run ", "Updated registration run ");
        assert_ne!(updated_source, old_source);
        fs::write(&bundle, updated_source).expect("update plugin registration behavior");
        let updated_catalog = manager
            .refresh_catalog(&BTreeSet::new(), &enabled)
            .expect("reload changed plugin generation");
        assert!(updated_catalog.revision > previous.revision);
        assert!(
            updated_catalog.plugins["session-owner-hook"].generation
                > previous.plugins["session-owner-hook"].generation
        );
        let updated = prepare(original_candidate);
        assert_eq!(updated.descriptor.description, "Updated registration run 1");
        assert!(
            updated.parameters_schema["properties"]
                .get("owner_session_id")
                .is_none()
        );
        let stale = (first.invoke)(
            json!({"url": "https://stale.test"}),
            browser_hook_test_context(),
        )
        .await;
        assert!(stale.is_err(), "old hook generation must be revoked");
    }

    /// Verifies registration, trusted session injection, and post-hook delivery through two real stdio MCP processes.
    /// 通过两个真实标准输入输出 MCP 进程验证注册、可信会话注入及后置 Hook 交付。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires VULCAN_W14_NODE_EXE to name a real Node.js executable"]
    async fn session_owner_hooks_reach_two_real_mcp_processes() {
        use crate::mcp::{
            McpRuntimeToolAllowlist, McpServerConfig, McpServerSource, McpServerTransportConfig,
            bootstrap_mcp_runtime_access,
        };
        use uuid::Uuid;
        use vulcan_tools_manager::VulcanToolsManager;

        // The explicit executable and source fixture make the child-process boundary reviewable.
        // 显式可执行文件与源码夹具使子进程边界可核查。
        let node_executable = std::env::var("VULCAN_W14_NODE_EXE")
            .expect("set VULCAN_W14_NODE_EXE to the exact Node.js executable");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/owner-mcp.mjs")
            .canonicalize()
            .expect("real MCP fixture path");
        let fixture = crate::runtime_assets::native_host_path(&fixture);
        let temp = tempfile::tempdir().expect("hook and MCP runtime root");
        let stall_marker = temp.path().join("in-flight-owner-call.json");
        let runtime_root = temp.path().join("runtime");
        fs::create_dir_all(&runtime_root).expect("create hook runtime root");
        let bundled_root = runtime_root.join("bundled_plugins");
        write_session_owner_hook_package(&bundled_root);
        let plugins = plugin_hook_test_manager(&runtime_root, bundled_root);
        let enabled = BTreeSet::from([String::from("session-owner-hook")]);
        plugins
            .refresh_catalog(&BTreeSet::new(), &enabled)
            .expect("enable exact tool hooks");

        // The MCP server declaration has the same immutable namespace targeted by the Lua hooks.
        // MCP 服务声明使用与 Lua Hook 精确匹配的不可变命名空间。
        let generation = 1_u64;
        let server = McpServerConfig {
            source: McpServerSource::Plugin {
                plugin_id: String::from("session-owner-hook"),
                generation,
            },
            id: String::from("fixture-server"),
            name: String::from("Owner MCP Fixture"),
            enabled: true,
            transport: McpServerTransportConfig::Stdio {
                command: node_executable,
                args: vec![fixture.to_string_lossy().into_owned()],
                cwd: None,
                env: std::collections::BTreeMap::from([(
                    String::from("VULCAN_W14_STALL_MARKER"),
                    crate::runtime_assets::native_host_path(&stall_marker)
                        .to_string_lossy()
                        .into_owned(),
                )]),
                env_passthrough: Vec::new(),
                inherit_environment: true,
            },
        };
        let owner_one = Uuid::from_u128(0x111);
        let owner_two = Uuid::from_u128(0x222);
        // An ordinary user MCP process shares the access registry but not this plugin lifecycle.
        // 普通用户 MCP 进程共享访问注册表，但不共享本插件的生命周期。
        let ordinary_server = McpServerConfig {
            source: McpServerSource::UserConfigured,
            id: String::from("ordinary-fixture"),
            name: String::from("Ordinary MCP Fixture"),
            ..server.clone()
        };
        let access = bootstrap_mcp_runtime_access(vec![ordinary_server])
            .await
            .expect("ordinary MCP runtime");
        let ordinary_binding = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if let Some(binding) = access
                    .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
                    .into_iter()
                    .find(|binding| binding.server_id() == Some("ordinary-fixture"))
                {
                    break binding;
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("ordinary MCP discovery");
        let partition_one = access
            .open_owned_stdio_partition(
                "session-owner-hook",
                generation,
                owner_one,
                vec![server.clone()],
            )
            .await
            .expect("first real MCP owner process");
        let partition_two = access
            .open_owned_stdio_partition(
                "session-owner-hook",
                generation,
                owner_two,
                vec![server.clone()],
            )
            .await
            .expect("second real MCP owner process");
        let other_owner = Uuid::from_u128(0x333);
        let other_server = McpServerConfig {
            source: McpServerSource::Plugin {
                plugin_id: String::from("unrelated-plugin"),
                generation,
            },
            id: String::from("unrelated-fixture"),
            name: String::from("Unrelated Plugin MCP Fixture"),
            ..server
        };
        let other_partition = access
            .open_owned_stdio_partition(
                "unrelated-plugin",
                generation,
                other_owner,
                vec![other_server],
            )
            .await
            .expect("unrelated plugin MCP process");
        let other_binding = other_partition
            .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .expect("unrelated plugin tool")
            .remove(0);

        // This candidate uses the same owner-checked MCP binding call as the production bridge.
        // 此候选使用与产品桥接层相同的所有者校验 MCP 绑定调用。
        let build_manager = |binding: crate::mcp::McpRuntimeToolBinding| {
            let descriptor = VulcanToolDescriptor::new(
                binding.id(),
                binding.name(),
                binding.description(),
                None,
                VulcanToolOrigin::new(
                    VulcanToolSourceKind::Mcp,
                    binding.server_id().expect("MCP server ID"),
                    binding.origin_tool_name().expect("original MCP tool name"),
                ),
            );
            let candidate = VulcanToolCandidate {
                descriptor,
                parameters_schema: binding.args_schema().clone(),
                ai_visible: true,
                plan_policy: VulcanPlanToolPolicy::Visible,
                request_streaming: None,
                invoke: Arc::new(move |arguments, context| {
                    let binding = binding.clone();
                    Box::pin(async move {
                        let owner = Uuid::parse_str(&context.session_id)
                            .map_err(|error| format!("invalid trusted owner: {error}"))?;
                        binding
                            .invoke_for_owner(owner, arguments)
                            .await
                            .map(VulcanToolInvocationResult::from_model_output)
                    })
                }),
            };
            VulcanToolsManager::new(prepare_vulcan_plugin_tool_candidates(
                Some(plugins.clone()),
                vec![candidate],
                None,
            ))
        };
        let binding_one = partition_one
            .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .expect("first owner tool")
            .remove(0);
        let binding_one_drain_probe = binding_one.clone();
        let binding_two = partition_two
            .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .expect("second owner tool")
            .remove(0);
        let binding_two_drain_probe = binding_two.clone();
        let manager_one = build_manager(binding_one.clone());
        let manager_two = build_manager(binding_two);
        let tools_one = manager_one.resolve_for_execution_mode("act");
        let tools_two = manager_two.resolve_for_execution_mode("act");
        assert_eq!(tools_one.definitions.len(), 1);
        assert_eq!(tools_two.definitions.len(), 1);
        for tool in [&tools_one.definitions[0], &tools_two.definitions[0]] {
            assert_eq!(tool.descriptor.name, "mcp_public_read");
            assert_eq!(tool.descriptor.description, "Registration run 1");
            assert!(
                tool.parameters_schema["properties"]
                    .get("owner_session_id")
                    .is_none()
            );
        }
        let mut context_one = browser_hook_test_context();
        context_one.session_id = owner_one.to_string();
        let mut context_two = browser_hook_test_context();
        context_two.session_id = owner_two.to_string();
        let (result_one, result_two) = tokio::join!(
            manager_one.invoke_tool(
                &tools_one.definitions[0],
                json!({"url": "https://one.test"}),
                context_one.clone(),
                "act",
            ),
            manager_two.invoke_tool(
                &tools_two.definitions[0],
                json!({"url": "https://two.test"}),
                context_two.clone(),
                "act",
            ),
        );
        let result_one = result_one.expect("first real MCP call");
        let result_two = result_two.expect("second real MCP call");
        // The MCP transport projects a single JSON text content block into a JSON result.
        // MCP 传输把单个 JSON 文本内容块投影为 JSON 结果。
        let decoded_one = result_one.model_output;
        let decoded_two = result_two.model_output;
        assert_eq!(decoded_one["owner_session_id"], owner_one.to_string());
        assert_eq!(decoded_two["owner_session_id"], owner_two.to_string());
        assert_eq!(decoded_one["url"], "https://one.test");
        assert_eq!(decoded_two["url"], "https://two.test");
        assert_ne!(decoded_one["process_id"], decoded_two["process_id"]);

        // A forged model argument never reaches either stdio process.
        // 模型伪造的参数不会进入任何一个标准输入输出进程。
        assert!(manager_one
            .invoke_tool(
                &tools_one.definitions[0],
                json!({"url": "https://forged.test", "owner_session_id": owner_two.to_string()}),
                context_one,
                "act",
            )
            .await
            .is_err());
        assert!(
            manager_one
                .invoke_tool(
                    &tools_one.definitions[0],
                    json!({"url": "https://replay.test"}),
                    context_two,
                    "act",
                )
                .await
                .is_err()
        );
        // Repeat the production tool-manager resolution against the same live stdio binding:
        // an unchanged catalog reuses the public definition, while a committed plugin config
        // publishes a fresh registration result without exposing the host-owned argument.
        // 针对同一实际标准输入输出绑定重复执行产品工具管理器解析：目录未变化时复用公开定义，
        // 插件配置提交后发布新的注册结果，同时继续隐藏宿主持有的参数。
        let repeated = build_manager(binding_one.clone()).resolve_for_execution_mode("act");
        assert_eq!(
            repeated.definitions[0].descriptor.description,
            "Registration run 1"
        );
        let before_config_revision = plugins.snapshot().revision;
        plugins
            .update_plugin_config("session-owner-hook", json!({"label": "stdio"}))
            .expect("commit live stdio registration configuration");
        assert!(plugins.snapshot().revision > before_config_revision);
        let changed_manager = build_manager(binding_one);
        let changed = changed_manager.resolve_for_execution_mode("act");
        assert_eq!(
            changed.definitions[0].descriptor.description,
            "Registration run 2 stdio"
        );
        assert!(
            changed.definitions[0].parameters_schema["properties"]
                .get("owner_session_id")
                .is_none()
        );
        let changed_cached =
            build_manager(binding_two_drain_probe.clone()).resolve_for_execution_mode("act");
        assert_eq!(
            changed_cached.definitions[0].descriptor.description,
            "Registration run 2 stdio"
        );
        let mut changed_context = browser_hook_test_context();
        changed_context.session_id = owner_one.to_string();
        let changed_result = changed_manager
            .invoke_tool(
                &changed.definitions[0],
                json!({"url": "https://updated.test"}),
                changed_context,
                "act",
            )
            .await
            .expect("updated public definition reaches the same real MCP process");
        assert_eq!(
            changed_result.model_output["owner_session_id"],
            owner_one.to_string()
        );
        assert_eq!(changed_result.model_output["url"], "https://updated.test");
        // The marker is written only after the first owner's MCP process admits this call.
        // 标记文件只在第一个所有者的 MCP 进程准入调用后写入。
        let stalled_call = tokio::spawn(async move {
            binding_one_drain_probe
                .invoke_for_owner(
                    owner_one,
                    json!({"url": "https://stall.test", "owner_session_id": owner_one.to_string()}),
                )
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if stall_marker.is_file() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("in-flight MCP request admitted before drain");
        let admitted: Value =
            serde_json::from_slice(&fs::read(&stall_marker).expect("read in-flight MCP marker"))
                .expect("decode in-flight MCP marker");
        assert_eq!(admitted["owner_session_id"], owner_one.to_string());
        // Draining this plugin revokes its owner bindings while two unrelated process families
        // remain live and callable through the same MCP access registry.
        // 排空本插件时撤销其所有者绑定，而同一 MCP 注册表中的两类无关进程仍存活且可调用。
        access
            .begin_plugin_owner_drain("session-owner-hook")
            .expect("drain exact plugin MCP owners");
        let stalled_result = tokio::time::timeout(std::time::Duration::from_secs(10), stalled_call)
            .await
            .expect("drain cancels in-flight MCP wait")
            .expect("in-flight task joined");
        assert!(
            stalled_result.is_err(),
            "drained child must not deliver a late result"
        );
        assert!(binding_two_drain_probe
            .invoke_for_owner(
                owner_two,
                json!({"url": "https://revoked.test", "owner_session_id": owner_two.to_string()}),
            )
            .await
            .is_err());
        let other_result = other_binding
            .invoke_for_owner(
                other_owner,
                json!({"url": "https://other.test", "owner_session_id": other_owner.to_string()}),
            )
            .await
            .expect("other plugin MCP remains callable");
        let ordinary_result = ordinary_binding
            .invoke(json!({"url": "https://ordinary.test", "owner_session_id": "ordinary"}))
            .await
            .expect("user MCP remains callable");
        assert_eq!(other_result["url"], "https://other.test");
        assert_eq!(ordinary_result["url"], "https://ordinary.test");
        partition_one
            .shutdown()
            .expect("stop first owner MCP process");
        partition_two
            .shutdown()
            .expect("stop second owner MCP process");
        other_partition
            .shutdown()
            .expect("stop unrelated plugin MCP process");
        access.shutdown();
    }

    /// Verifies the product browser plugin catalog and its screenshot hook end to end.
    /// 端到端验证产品浏览器插件目录及其截图 Hook。
    #[tokio::test(flavor = "current_thread")]
    async fn product_browser_screenshot_hooks_transform_and_guard_mcp_calls() {
        let temp = tempfile::tempdir().expect("create browser hook test directory");
        let runtime_root = temp.path().join("runtime");
        fs::create_dir_all(&runtime_root).expect("create browser hook runtime root");
        let manager = browser_plugin_test_manager(&runtime_root);
        let enabled = BTreeSet::from([String::from("vulcan-browser")]);
        let catalog = manager
            .refresh_catalog(&BTreeSet::new(), &enabled)
            .expect("load product browser plugin catalog");
        let plugin = catalog
            .plugins
            .get("vulcan-browser")
            .expect("product browser plugin must be discovered");
        assert!(
            plugin.diagnostics.is_empty(),
            "catalog diagnostics: {:?}",
            plugin.diagnostics
        );
        assert_eq!(
            plugin.hooks.len(),
            5,
            "browser plugin must declare screenshot hooks and exact unsafe-code guards"
        );
        assert!(
            catalog.enabled_hooks.len() >= 2,
            "product hooks must be present in the enabled hook surface"
        );
        let matching_hooks = catalog
            .enabled_hooks
            .values()
            .filter(|hook| {
                hook.manifest.tool_target.as_ref().is_some_and(|target| {
                    target.source_kind == VulcanPluginToolHookSourceKind::Mcp
                        && target.namespace == "vulcan-browser:playwright"
                        && target.original_name == "browser_take_screenshot"
                })
            })
            .count();
        assert_eq!(
            matching_hooks, 2,
            "both hooks must target the immutable MCP origin"
        );
        // Service bootstrap starts with the conversation dispatcher rather than the temporary
        // tool-hook dispatcher; plugin enablement must recognize the product's generic points.
        // 服务启动使用会话调度器而非临时工具 Hook 调度器；启用插件时必须识别产品通用点位。
        let mut service_dispatcher = HookDispatcher::default();
        for descriptor in super::super::conversation_hooks::host_hook_point_descriptors() {
            service_dispatcher
                .register_point(descriptor)
                .expect("register conversation host point");
        }
        synchronize_vulcan_plugin_hooks(&mut service_dispatcher, manager.clone(), &catalog)
            .expect("synchronize product hook points into service dispatcher");
        validate_vulcan_plugin_hooks(&service_dispatcher, &plugin.hooks)
            .expect("product browser hooks must validate in the service dispatcher");

        let origin = VulcanToolOrigin::new(
            VulcanToolSourceKind::Mcp,
            "vulcan-browser:playwright",
            "browser_take_screenshot",
        );
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls_for_invoke = Arc::clone(&calls);
        let candidate = VulcanToolCandidate {
            descriptor: VulcanToolDescriptor::new(
                "browser_take_screenshot",
                "browser_take_screenshot",
                "Take a browser screenshot.",
                None,
                origin.clone(),
            ),
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "filename": {"type": "string"},
                    "type": {"type": "string"}
                }
            }),
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::Visible,
            request_streaming: None,
            invoke: Arc::new(move |arguments, _context| {
                calls_for_invoke.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Box::pin(async move {
                    Ok(VulcanToolInvocationResult::from_model_output(json!({
                        "arguments": arguments,
                        "image": "preserved"
                    })))
                })
            }),
        };
        let prepared = prepare_vulcan_plugin_tool_candidates(Some(manager), vec![candidate], None);
        assert_eq!(
            prepared.len(),
            1,
            "matching MCP candidate must remain visible"
        );
        let prepared = prepared.into_iter().next().expect("prepared browser tool");
        assert_eq!(
            prepared.parameters_schema["properties"]["filename"]["type"],
            "string"
        );
        assert_eq!(prepared.parameters_schema["additionalProperties"], false);
        assert!(
            prepared
                .descriptor
                .description
                .contains("filename is ignored")
        );

        let context = browser_hook_test_context();
        let passed = (prepared.invoke)(
            json!({"filename": "should-not-reach-mcp", "type": "png"}),
            context,
        )
        .await
        .expect("compatible filename must be removed before the MCP call");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(passed.model_output["arguments"]["type"], "png");
        assert!(passed.model_output["arguments"].get("filename").is_none());
        assert_eq!(passed.model_output["image"], "preserved");
    }

    /// Verifies the product plugin removes unsafe Playwright code execution and blocks stale calls.
    /// 验证产品插件移除不安全的 Playwright 代码执行工具，并阻断旧目录调用。
    #[tokio::test(flavor = "current_thread")]
    async fn product_browser_plugin_omits_and_blocks_unsafe_run_code() {
        let temp = tempfile::tempdir().expect("create browser hook test directory");
        let runtime_root = temp.path().join("runtime");
        fs::create_dir_all(&runtime_root).expect("create browser hook runtime root");
        let manager = browser_plugin_test_manager(&runtime_root);
        let catalog = manager
            .refresh_catalog(
                &BTreeSet::new(),
                &BTreeSet::from([String::from("vulcan-browser")]),
            )
            .expect("load product browser plugin catalog");
        let origin = VulcanToolOrigin::new(
            VulcanToolSourceKind::Mcp,
            "vulcan-browser:playwright",
            "browser_run_code_unsafe",
        );
        let registration_hooks = matching_tool_hooks(
            &catalog,
            &origin,
            vulcan_plugin_runtime::VULCAN_PLUGIN_HOOK_TOOL_REGISTRATION,
        );
        let before_hooks = matching_tool_hooks(
            &catalog,
            &origin,
            vulcan_plugin_runtime::VULCAN_PLUGIN_HOOK_TOOL_BEFORE,
        );
        assert_eq!(registration_hooks.len(), 2);
        assert_eq!(before_hooks.len(), 1);

        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls_for_invoke = Arc::clone(&calls);
        let candidate = VulcanToolCandidate {
            descriptor: VulcanToolDescriptor::new(
                "browser_run_code_unsafe",
                "browser_run_code_unsafe",
                "Run unsafe Playwright code.",
                None,
                origin,
            ),
            parameters_schema: json!({
                "type": "object",
                "properties": { "code": { "type": "string" } }
            }),
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::Visible,
            request_streaming: None,
            invoke: Arc::new(move |_, _| {
                calls_for_invoke.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Box::pin(async { Ok(VulcanToolInvocationResult::from_model_output(json!("ran"))) })
            }),
        };
        let cache_key = registration_cache_key(&catalog, &candidate, &registration_hooks);
        let prepared = prepare_vulcan_plugin_tool_candidates(
            Some(manager.clone()),
            vec![candidate.clone()],
            None,
        );
        assert!(
            prepared.is_empty(),
            "unsafe tool must not enter the AI catalog"
        );
        assert!(matches!(
            cached_registration_outcome(&cache_key),
            Some(CachedToolRegistrationOutcome::Omitted)
        ));
        assert!(
            prepare_vulcan_plugin_tool_candidates(
                Some(manager.clone()),
                vec![candidate.clone()],
                None,
            )
            .is_empty(),
            "the cached omission must survive another tool catalog construction"
        );

        let stale = wrap_candidate_with_tool_hooks(
            candidate,
            manager,
            json!({
                "type": "object",
                "properties": { "code": { "type": "string" } }
            }),
            before_hooks.clone(),
            before_hooks,
            Vec::new(),
            None,
        );
        let rejected = (stale.invoke)(
            json!({"code": "async (page) => page.title()"}),
            browser_hook_test_context(),
        )
        .await;
        assert!(rejected.is_err(), "stale unsafe tool call must be rejected");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
}
