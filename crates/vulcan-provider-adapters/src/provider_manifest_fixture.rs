//! Executable JSON-only provider-manifest fixture harness.
//! 可执行的纯 JSON Provider Manifest fixture 验证器。

use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use crate::provider_task::ProviderAsyncTaskPoll;
use crate::{
    CredentialSecretEnvelope, ManifestOwnership, MediaUploadCancellation,
    ProviderToolExecutionKind, ProviderToolNormalizedOutput,
};
use vulcan_llm_core::UsageRecordKind;

/// Contract revision accepted by the executable fixture harness.
/// 可执行 fixture 验证器接受的契约修订号。
const EXECUTION_FIXTURE_SCHEMA_VERSION: &str = "provider-manifest-execution-fixture.v1";
/// Contract revision accepted for request/response/error/usage/discovery profile fixtures.
/// request/response/error/usage/discovery Profile fixture 接受的契约修订号。
const PROFILE_FIXTURE_SCHEMA_VERSION: &str = "provider-manifest-profile-fixture.v1";
/// Maximum fixture document size accepted by the harness.
/// 验证器接受的 fixture 文档最大尺寸。
const MAX_FIXTURE_BYTES: u64 = 2 * 1024 * 1024;
/// Maximum bundled baseline bytes accepted by the fixture compiler.
/// fixture 编译器接受的内置基线最大字节数。
///
/// This bounds manifest documents the harness compiles, so it derives from the single manifest
/// byte bound instead of restating it.
/// 它限制的是验证器要编译的清单文档，因此由唯一的清单字节上限派生，而不是重述字面量。
const MAX_FIXTURE_BASE_MANIFEST_BYTES: u64 = crate::MAX_MANIFEST_BYTES as u64;
/// Maximum HTTP interactions allowed in one fixture.
/// 单个 fixture 允许的最大 HTTP 交互次数。
const MAX_FIXTURE_INTERACTIONS: usize = 16;

/// One successful fixture result reported to the CLI.
/// 向 CLI 报告的一条成功 fixture 结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderManifestFixtureResult {
    /// Stable fixture identifier.
    /// 稳定 fixture 标识。
    pub fixture_id: String,
    /// Exact normalized output family observed at runtime.
    /// 运行时观测到的精确规范输出族。
    pub output_kind: String,
    /// Number of verified local HTTP interactions.
    /// 已验证的本地 HTTP 交互数量。
    pub interaction_count: usize,
}

/// One successful static audit over manifests, adapters, and production-target fixture coverage.
/// 一次针对 Manifest、adapter 与生产目标 fixture 覆盖的成功静态审计。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderManifestFixtureCoverageAudit {
    /// Number of compiled bundled system providers.
    /// 已编译内置系统供应商数量。
    pub provider_count: usize,
    /// Number of concrete production provider-tool targets, including template instances.
    /// 具体生产供应商工具目标数量，包含模板实例。
    pub target_count: usize,
    /// Production adapter counts keyed by their serialized stable identity.
    /// 按序列化稳定标识索引的生产 adapter 数量。
    pub adapter_counts: BTreeMap<String, usize>,
}

/// One JSON-only execution fixture layered over a bundled baseline manifest.
/// 叠加在内置基线清单上的一份纯 JSON 执行 fixture。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderManifestExecutionFixture {
    /// Fixed fixture schema revision.
    /// 固定 fixture Schema 修订号。
    schema_version: String,
    /// Stable fixture identifier.
    /// 稳定 fixture 标识。
    fixture_id: String,
    /// Bundled manifest file used only as a complete validated provider shell.
    /// 仅作为完整可校验供应商外壳使用的内置清单文件。
    base_manifest: String,
    /// Exact production targets whose closed execution signature is exercised by this fixture.
    /// 当前 fixture 覆盖其封闭执行签名的精确生产目标。
    #[serde(default)]
    production_target_refs: Vec<String>,
    /// JSON tool-target declaration appended without Rust source changes.
    /// 无需修改 Rust 源码即可追加的 JSON 工具目标声明。
    tool_target: Value,
    /// JSON operation declaration owning the appended target.
    /// 持有新增目标的 JSON operation 声明。
    operation: Value,
    /// Optional JSON upload channels required by the operation.
    /// operation 所需的可选 JSON 上传通道。
    #[serde(default)]
    upload_channels: Vec<Value>,
    /// Optional JSON request profiles referenced by the operation.
    /// operation 引用的可选 JSON 请求 Profile。
    #[serde(default)]
    request_profiles: Vec<Value>,
    /// Optional JSON response profiles referenced by the operation.
    /// operation 引用的可选 JSON 响应 Profile。
    #[serde(default)]
    response_profiles: Vec<Value>,
    /// Required execution primitives added to the fixture engine contract.
    /// 加入 fixture 引擎契约的必需执行原语。
    required_execution_primitives: Vec<String>,
    /// Required upload primitives added to the fixture engine contract.
    /// 加入 fixture 引擎契约的必需上传原语。
    #[serde(default)]
    required_upload_primitives: Vec<String>,
    /// Required mapping transformers added to the fixture engine contract.
    /// 加入 fixture 引擎契约的必需映射转换器。
    #[serde(default)]
    required_transformers: Vec<String>,
    /// Canonical tool arguments before upload materialization.
    /// 上传物化前的规范工具参数。
    arguments: Value,
    /// Ordered local HTTP interactions expected from actual execution.
    /// 实际执行预期产生的有序本地 HTTP 交互。
    interactions: Vec<FixtureHttpInteraction>,
    /// Optional expected normalized output family.
    /// 可选预期规范输出族。
    #[serde(default)]
    expected_output_kind: Option<String>,
    /// Optional exact normalized text required when the expected output family is text.
    /// 预期输出族为文本时要求的可选精确规范文本。
    #[serde(default)]
    expected_text_output: Option<String>,
    /// Optional exact normalized search object required for a search output fixture.
    /// 搜索输出 fixture 要求的可选精确规范搜索对象。
    #[serde(default)]
    expected_search_output: Option<Value>,
    /// Optional controlled execution error substring expected by a negative fixture.
    /// 负向 fixture 预期的可选受控执行错误子串。
    #[serde(default)]
    expected_error_contains: Option<String>,
    /// Whether the harness must cancel immediately after task creation.
    /// 验证器是否必须在任务创建后立即取消。
    #[serde(default)]
    cancel_after_start: bool,
    /// Whether provider-confirmed cancellation is the expected terminal outcome.
    /// 供应商确认取消是否为预期终态。
    #[serde(default)]
    expected_cancellation: bool,
}

/// One executable JSON profile or discovery fixture layered over a bundled manifest.
/// 叠加在内置 Manifest 上的一份可执行 JSON Profile 或发现 fixture。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderManifestProfileFixture {
    /// Fixed profile-fixture schema revision.
    /// 固定的 Profile fixture Schema 修订号。
    schema_version: String,
    /// Stable fixture identifier.
    /// 稳定 fixture 标识。
    fixture_id: String,
    /// Bundled manifest file used as the validated provider shell.
    /// 用作已校验供应商外壳的内置 Manifest 文件。
    base_manifest: String,
    /// Exact production subsystem exercised by this fixture.
    /// 此 fixture 执行的精确生产子系统。
    fixture_kind: ProfileFixtureKind,
    /// Optional JSON profile declaration appended before compilation.
    /// 编译前追加的可选 JSON Profile 声明。
    #[serde(default)]
    declaration: Option<Value>,
    /// Required mapping transformers added when the fixture appends a projection profile.
    /// fixture 追加投影 Profile 时加入的必需映射转换器。
    #[serde(default)]
    required_transformers: Vec<String>,
    /// Optional compiled profile identifier selected for execution.
    /// 为执行选择的可选已编译 Profile 标识。
    #[serde(default)]
    profile_id: Option<String>,
    /// Untouched provider input or already encoded request body.
    /// 未经修改的供应商输入或已编码请求正文。
    input: Value,
    /// Canonical request model identity used by request mappings.
    /// 请求映射使用的规范请求模型身份。
    #[serde(default)]
    model_id: Option<String>,
    /// Raw transport record kind used by usage extraction.
    /// usage 提取使用的原始传输记录类型。
    #[serde(default)]
    record_kind: Option<UsageRecordKind>,
    /// Closed discovery parser selected by a discovery fixture.
    /// discovery fixture 选择的封闭发现解析器。
    #[serde(default)]
    discovery_kind: Option<FixtureDiscoveryKind>,
    /// Exact output JSON Pointer expectations.
    /// 精确的输出 JSON Pointer 预期。
    expectations: BTreeMap<String, Value>,
}

/// Closed executable profile-fixture family.
/// 封闭的可执行 Profile fixture 类型族。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProfileFixtureKind {
    /// Request projection over an encoded JSON body.
    /// 对已编码 JSON 正文执行请求投影。
    Request,
    /// Successful response projection before protocol normalization.
    /// 协议归一化前的成功响应投影。
    Response,
    /// HTTP error response projection before error normalization.
    /// 错误归一化前的 HTTP 错误响应投影。
    Error,
    /// Usage extraction from untouched provider evidence.
    /// 从未经修改的供应商证据提取 usage。
    Usage,
    /// Model discovery response parsing.
    /// 模型发现响应解析。
    Discovery,
}

/// Closed discovery parsers available to JSON-only fixtures.
/// 纯 JSON fixture 可使用的封闭发现解析器。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FixtureDiscoveryKind {
    /// OpenAI-compatible `/models` parser.
    /// OpenAI 兼容 `/models` 解析器。
    OpenAi,
}

/// One terminal fixture execution outcome before expectation matching.
/// 预期匹配前的一种 fixture 执行终态。
enum FixtureExecutionOutcome {
    /// One normalized provider-tool result.
    /// 一份规范供应商工具结果。
    Output(crate::ProviderToolExecutionResult),
    /// Provider-confirmed cancellation.
    /// 供应商确认取消。
    Cancelled,
}

/// One exact local HTTP request and response expectation.
/// 一次精确本地 HTTP 请求与响应预期。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureHttpInteraction {
    /// Exact uppercase HTTP method.
    /// 精确大写 HTTP 方法。
    method: String,
    /// Exact request target including any query string.
    /// 包含查询字符串的精确请求目标。
    path: String,
    /// Optional exact JSON request body.
    /// 可选精确 JSON 请求正文。
    #[serde(default)]
    expected_json_body: Option<Value>,
    /// UTF-8 fragments that must occur in the raw request body, including multipart metadata.
    /// 必须出现在原始请求正文中的 UTF-8 分片，包含 Multipart 元数据。
    #[serde(default)]
    expected_body_contains: Vec<String>,
    /// Case-insensitive request header names with exact trimmed expected values.
    /// 大小写不敏感的请求头名称及精确裁剪后预期值。
    #[serde(default)]
    expected_headers: BTreeMap<String, String>,
    /// Case-insensitive request header names that must be absent.
    /// 必须缺失的大小写不敏感请求头名称。
    #[serde(default)]
    absent_headers: Vec<String>,
    /// HTTP response status.
    /// HTTP 响应状态。
    response_status: u16,
    /// Optional JSON response body; exactly one response body form must be declared.
    /// 可选 JSON 响应正文；必须且只能声明一种响应正文形式。
    #[serde(default)]
    response_json_body: Option<Value>,
    /// Optional exact UTF-8 response body for streaming or non-JSON protocols.
    /// 用于流式或非 JSON 协议的可选精确 UTF-8 响应正文。
    #[serde(default)]
    response_text_body: Option<String>,
    /// Optional exact response content type; defaults by selected body form.
    /// 可选精确响应内容类型；默认值由所选正文形式决定。
    #[serde(default)]
    response_content_type: Option<String>,
    /// Optional exact non-framing response headers emitted by the loopback server.
    /// 回环服务器发送的可选精确非分帧响应头。
    #[serde(default)]
    response_headers: BTreeMap<String, String>,
}

/// Closed execution signature used to bind one representative fixture to production targets.
/// 用于把一个代表性 fixture 绑定到生产目标的封闭执行签名。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderManifestFixtureTargetSignature {
    /// Provider-neutral capability family.
    /// 供应商无关能力族。
    capability: String,
    /// Normalized output family.
    /// 规范输出族。
    output_kind: String,
    /// Optional normalized resource family.
    /// 可选规范资源族。
    resource_kind: Option<String>,
    /// Closed transport execution family.
    /// 封闭传输执行族。
    execution_kind: String,
    /// Closed request/response adapter identity.
    /// 封闭请求/响应 adapter 标识。
    adapter: String,
}

/// Audits all bundled production targets against explicit executable-fixture coverage.
/// 根据显式可执行 fixture 覆盖审计全部内置生产目标。
///
/// # Parameters
/// 参数
/// `fixtures_directory` owns checked-in fixture documents and `manifests_directory` owns the
/// authoritative bundled system manifests.
/// `fixtures_directory` 持有签入的 fixture 文档，`manifests_directory` 持有权威内置系统
/// Manifest。
///
/// # Returns
/// 返回值
/// Provider, target, and adapter counts only when every concrete target is covered exactly once
/// by a fixture with the same closed execution signature.
/// 仅当每个具体目标都被具有相同封闭执行签名的 fixture 精确覆盖一次时，返回供应商、目标与
/// adapter 计数。
pub fn audit_provider_manifest_fixture_coverage(
    fixtures_directory: &Path,
    manifests_directory: &Path,
) -> Result<ProviderManifestFixtureCoverageAudit, String> {
    let mut production_targets = BTreeMap::new();
    let mut provider_ids = BTreeSet::new();
    let mut adapter_counts = BTreeMap::new();
    let manifest_paths = json_files(manifests_directory)?;
    if manifest_paths.is_empty() {
        return Err(format!(
            "No bundled provider manifests exist under `{}`.",
            manifests_directory.display()
        ));
    }
    for manifest_path in manifest_paths {
        let manifest_text = read_bounded_fixture_utf8(
            manifest_path.as_path(),
            MAX_FIXTURE_BASE_MANIFEST_BYTES,
            "bundled fixture-coverage manifest",
        )?;
        let resolved = crate::compile_manifest_text(&manifest_text, ManifestOwnership::System)
            .map_err(|error| {
                format!(
                    "Bundled manifest `{}` failed fixture coverage compilation: {error}",
                    manifest_path.display()
                )
            })?;
        let provider_id = resolved.descriptor.id.as_str();
        if provider_id.starts_with("custom:") {
            return Err(format!(
                "Bundled system provider `{provider_id}` collides with the reserved custom-provider namespace."
            ));
        }
        if !provider_ids.insert(provider_id.to_string()) {
            return Err(format!(
                "Bundled provider identity `{provider_id}` appears more than once."
            ));
        }
        for target in &resolved.tool_targets {
            let target_value = serde_json::to_value(target).map_err(|error| {
                format!(
                    "Production target `{provider_id}::{}` could not be inspected: {error}",
                    target.target_id
                )
            })?;
            let target_ref = format!("{provider_id}::{}", target.target_id);
            let signature =
                provider_manifest_production_target_signature(&target_value, target_ref.as_str())?;
            if signature.adapter != "generic_json" {
                return Err(format!(
                    "Production target `{target_ref}` uses provider-named or unregistered adapter `{}`.",
                    signature.adapter
                ));
            }
            *adapter_counts.entry(signature.adapter.clone()).or_insert(0) += 1;
            if production_targets
                .insert(target_ref.clone(), signature)
                .is_some()
            {
                return Err(format!(
                    "Production target identity `{target_ref}` appears more than once."
                ));
            }
        }
    }

    let mut covered_targets = BTreeSet::new();
    for fixture_path in json_files(fixtures_directory)? {
        let fixture_text = read_bounded_fixture(fixture_path.as_path())?;
        let document = parse_bounded_fixture_json(
            fixture_text.as_str(),
            &format!("Fixture `{}`", fixture_path.display()),
        )?;
        if document.get("schema_version").and_then(Value::as_str)
            != Some(EXECUTION_FIXTURE_SCHEMA_VERSION)
        {
            continue;
        }
        let fixture: ProviderManifestExecutionFixture = serde_json::from_value(document)
            .map_err(|error| format!("Fixture `{}` is invalid: {error}", fixture_path.display()))?;
        let fixture_signature = provider_manifest_fixture_target_signature(
            &fixture.tool_target,
            fixture.fixture_id.as_str(),
        )?;
        for target_ref in &fixture.production_target_refs {
            let production_signature = production_targets.get(target_ref).ok_or_else(|| {
                format!(
                    "Fixture `{}` references unknown production target `{target_ref}`.",
                    fixture.fixture_id
                )
            })?;
            if production_signature != &fixture_signature {
                return Err(format!(
                    "Fixture `{}` signature {:?} does not match production target `{target_ref}` signature {:?}.",
                    fixture.fixture_id, fixture_signature, production_signature
                ));
            }
            if !covered_targets.insert(target_ref.clone()) {
                return Err(format!(
                    "Production target `{target_ref}` is covered by more than one executable fixture."
                ));
            }
        }
    }

    let missing_targets = production_targets
        .keys()
        .filter(|target_ref| !covered_targets.contains(*target_ref))
        .cloned()
        .collect::<Vec<_>>();
    if !missing_targets.is_empty() {
        return Err(format!(
            "Production targets without executable fixture coverage: {}",
            missing_targets.join(", ")
        ));
    }
    Ok(ProviderManifestFixtureCoverageAudit {
        provider_count: provider_ids.len(),
        target_count: production_targets.len(),
        adapter_counts,
    })
}

/// Reads one closed target signature from a fixture or serialized production target.
/// 从 fixture 或序列化生产目标读取一个封闭目标签名。
///
/// # Parameters
/// 参数
/// `target` is the exact target JSON and `context` identifies errors without guessing ownership.
/// `target` 是精确目标 JSON，`context` 用于在不猜测归属的情况下标识错误。
///
/// # Returns
/// 返回值
/// The normalized signature, or an exact missing-field error.
/// 返回规范化签名，或精确的字段缺失错误。
fn provider_manifest_fixture_target_signature(
    target: &Value,
    context: &str,
) -> Result<ProviderManifestFixtureTargetSignature, String> {
    let required_string = |pointer: &str| {
        target
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("Target `{context}` has no string at `{pointer}`."))
    };
    Ok(ProviderManifestFixtureTargetSignature {
        capability: required_string("/capability")?,
        output_kind: required_string("/output/kind")?,
        resource_kind: target
            .pointer("/output/resource_kind")
            .and_then(Value::as_str)
            .map(str::to_string),
        execution_kind: required_string("/execution/execution_kind")?,
        adapter: required_string("/execution/adapter")?,
    })
}

/// Reads one closed target signature from the compiled production target shape.
/// 从已编译生产目标结构读取一个封闭目标签名。
///
/// # Parameters
/// 参数
/// `target` is the exact serialized compiled target and `context` identifies its Manifest owner.
/// `target` 是精确序列化的已编译目标，`context` 标识其 Manifest 所有者。
///
/// # Returns
/// 返回值
/// The normalized signature, or an exact compiled-contract field error.
/// 返回规范化签名，或精确的已编译契约字段错误。
fn provider_manifest_production_target_signature(
    target: &Value,
    context: &str,
) -> Result<ProviderManifestFixtureTargetSignature, String> {
    let required_string = |pointer: &str| {
        target
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("Compiled target `{context}` has no string at `{pointer}`."))
    };
    Ok(ProviderManifestFixtureTargetSignature {
        capability: required_string("/contract/capability")?,
        output_kind: required_string("/contract/output/kind")?,
        resource_kind: target
            .pointer("/contract/output/resource_kind")
            .and_then(Value::as_str)
            .map(str::to_string),
        execution_kind: required_string("/execution/execution_kind")?,
        adapter: required_string("/execution/adapter")?,
    })
}

/// Executes every JSON-only provider fixture against a strict local HTTP server.
/// 在严格本地 HTTP 服务器上执行每一份纯 JSON 供应商 fixture。
///
/// # Parameters
/// 参数
/// `fixtures_directory` contains fixture documents and `manifests_directory` contains bundled baselines.
/// `fixtures_directory` 包含 fixture 文档，`manifests_directory` 包含内置基线。
///
/// # Returns
/// 返回值
/// Ordered successful results, or the first exact compile, transport, or expectation failure.
/// 返回有序成功结果，或首个精确编译、传输或预期失败。
pub async fn execute_provider_manifest_fixture_suite(
    fixtures_directory: &Path,
    manifests_directory: &Path,
) -> Result<Vec<ProviderManifestFixtureResult>, String> {
    audit_provider_manifest_fixture_coverage(fixtures_directory, manifests_directory)?;
    let mut fixture_paths = json_files(fixtures_directory)?;
    fixture_paths.sort();
    if fixture_paths.is_empty() {
        return Err(format!(
            "No provider execution fixtures exist under `{}`.",
            fixtures_directory.display()
        ));
    }
    let mut results = Vec::with_capacity(fixture_paths.len());
    for fixture_path in fixture_paths {
        // fixtureFuture keeps each expanded execution state off the fixture-check caller stack.
        // fixtureFuture 使每份扩展执行状态脱离fixture-check调用栈。
        let fixture_future = Box::pin(execute_fixture_document(
            fixture_path.as_path(),
            manifests_directory,
        ));
        results.push(fixture_future.await?);
    }
    Ok(results)
}

/// Dispatches one bounded fixture document by its explicit closed schema revision.
/// 按显式封闭 Schema 修订号分发一份有界 fixture 文档。
async fn execute_fixture_document(
    fixture_path: &Path,
    manifests_directory: &Path,
) -> Result<ProviderManifestFixtureResult, String> {
    let fixture_text = read_bounded_fixture(fixture_path)?;
    let document = parse_bounded_fixture_json(
        fixture_text.as_str(),
        &format!("Fixture `{}`", fixture_path.display()),
    )?;
    let schema_version = document
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            format!(
                "Fixture `{}` has no schema_version.",
                fixture_path.display()
            )
        })?;
    match schema_version {
        EXECUTION_FIXTURE_SCHEMA_VERSION => {
            let fixture: ProviderManifestExecutionFixture = serde_json::from_value(document)
                .map_err(|error| {
                    format!("Fixture `{}` is invalid: {error}", fixture_path.display())
                })?;
            // executionFuture isolates the full upload/async execution state from the document dispatcher.
            // executionFuture 将完整上传/异步执行状态与文档分派器隔离。
            Box::pin(execute_provider_manifest_execution_fixture(
                fixture,
                fixture_path,
                manifests_directory,
            ))
            .await
        }
        PROFILE_FIXTURE_SCHEMA_VERSION => {
            let fixture: ProviderManifestProfileFixture = serde_json::from_value(document)
                .map_err(|error| {
                    format!("Fixture `{}` is invalid: {error}", fixture_path.display())
                })?;
            execute_provider_manifest_profile_fixture(fixture, fixture_path, manifests_directory)
        }
        other => Err(format!(
            "Fixture `{}` uses unsupported schema `{other}`.",
            fixture_path.display()
        )),
    }
}

/// Executes one fixture from manifest compilation through normalized output decoding.
/// 从 Manifest 编译到规范输出解码执行单个 fixture。
async fn execute_provider_manifest_execution_fixture(
    fixture: ProviderManifestExecutionFixture,
    fixture_path: &Path,
    manifests_directory: &Path,
) -> Result<ProviderManifestFixtureResult, String> {
    validate_fixture_contract(&fixture, fixture_path)?;
    let base_path = safe_baseline_path(manifests_directory, fixture.base_manifest.as_str())?;
    let base_text = read_bounded_fixture_utf8(
        base_path.as_path(),
        MAX_FIXTURE_BASE_MANIFEST_BYTES,
        "fixture baseline manifest",
    )?;
    let mut document = parse_bounded_fixture_json(
        base_text.as_str(),
        &format!("Fixture baseline `{}`", base_path.display()),
    )?;
    append_fixture_declarations(&mut document, &fixture)?;
    let manifest_text = serde_json::to_string(&document)
        .map_err(|error| format!("Fixture manifest could not serialize: {error}"))?;
    let resolved = crate::compile_manifest_text(&manifest_text, ManifestOwnership::System)
        .map_err(|error| format!("Fixture `{}` did not compile: {error}", fixture.fixture_id))?;
    let target_id = fixture
        .tool_target
        .get("target_id")
        .and_then(Value::as_str)
        .ok_or_else(|| String::from("Fixture tool target has no target_id."))?;
    let mut target = resolved
        .tool_targets
        .iter()
        .find(|target| target.target_id == target_id)
        .cloned()
        .ok_or_else(|| format!("Fixture target `{target_id}` was not compiled."))?;
    let operation = resolved
        .operations
        .iter()
        .find(|operation| {
            operation
                .tool_target_refs
                .iter()
                .any(|reference| reference == target_id)
        })
        .cloned()
        .ok_or_else(|| format!("Fixture target `{target_id}` has no compiled operation."))?;
    let request_profiles = resolved.request_profiles_for_operation(Some(&operation))?;
    let response_profiles = resolved.response_profiles_for_operation(Some(&operation))?;
    let mut entry = resolved
        .descriptor
        .service_entries
        .iter()
        .find(|entry| entry.id == target.service_entry_id)
        .cloned()
        .ok_or_else(|| {
            format!(
                "Fixture target `{target_id}` references missing entry `{}`.",
                target.service_entry_id
            )
        })?;
    let (origin, server) = spawn_fixture_server(fixture.interactions.clone())?;
    entry.base_url = origin.clone();
    target.execution.endpoint_base_url = Some(origin.clone());
    let secret = CredentialSecretEnvelope::new_api_key(String::from("fixture-secret"))?
        .execution_secret()?;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("Fixture HTTP client could not be built: {error}"))?;
    // Validate and materialize defaults before upload bindings remove or replace AI-visible fields.
    // 在上传绑定移除或替换 AI 可见字段前完成校验与默认值物化。
    let canonical_arguments = crate::provider_tool_execution::canonicalize_provider_tool_arguments(
        &target,
        fixture.arguments,
    )?;
    let arguments = if operation.upload_bindings.is_empty() {
        canonical_arguments
    } else {
        let credential_profile = entry.credential_profiles.first().ok_or_else(|| {
            format!(
                "Fixture target `{target_id}` service entry `{}` has no credential profile.",
                entry.id
            )
        })?;
        Box::pin(crate::upload_engine::materialize_operation_uploads(
            crate::upload_engine::OperationUploadExecutionContext {
                media_sources: &[],
                media_delivery_policy: crate::MediaDeliveryPolicy::default(),
                http: &http,
                entry: &entry,
                secret: &secret,
                credential_profile,
                channels: &resolved.upload_channels,
                progress: None,
                cancellation: &MediaUploadCancellation::new(),
                model_id: None,
                workspace_root: fixture_path.parent(),
                fixture_loopback_origin: Some(origin.as_str()),
            },
            &operation,
            canonical_arguments,
        ))
        .await?
    };
    let execution: Result<FixtureExecutionOutcome, String> = async {
        if target.execution.execution_kind == ProviderToolExecutionKind::HttpJsonAsyncPoll {
            let async_execution = operation.async_execution.as_ref().ok_or_else(|| {
                format!(
                    "Fixture operation `{}` has no async declaration.",
                    operation.operation_id
                )
            })?;
            let started = Box::pin(crate::provider_tool_execution::start_provider_async_task(
                crate::provider_tool_execution::ProviderAsyncExecutionContext {
                    http: &http,
                    entry: &entry,
                    target: &target,
                    request_profiles: &request_profiles,
                    response_profiles: &response_profiles,
                    secret: &secret,
                },
                arguments,
                Some(async_execution),
            ))
            .await?;
            let recovery = started.recovery.clone();
            if fixture.cancel_after_start {
                Box::pin(crate::provider_tool_execution::cancel_provider_async_task(
                    &http,
                    &entry,
                    &target,
                    started.provider_task_id.as_str(),
                    &secret,
                    Some(async_execution),
                ))
                .await?;
                return Ok(FixtureExecutionOutcome::Cancelled);
            }
            for _ in 0..fixture.interactions.len() {
                match Box::pin(crate::provider_tool_execution::poll_provider_async_task(
                    crate::provider_tool_execution::ProviderAsyncExecutionContext {
                        http: &http,
                        entry: &entry,
                        target: &target,
                        request_profiles: &request_profiles,
                        response_profiles: &response_profiles,
                        secret: &secret,
                    },
                    started.provider_task_id.as_str(),
                    &recovery,
                    Some(async_execution),
                ))
                .await?
                {
                    ProviderAsyncTaskPoll::Pending(_) => continue,
                    ProviderAsyncTaskPoll::Completed(result) => {
                        return Ok(FixtureExecutionOutcome::Output(result));
                    }
                    ProviderAsyncTaskPoll::Failed(error) => return Err(error),
                    ProviderAsyncTaskPoll::Cancelled => {
                        return Ok(FixtureExecutionOutcome::Cancelled);
                    }
                }
            }
            Err(String::from(
                "Fixture task did not reach a terminal result.",
            ))
        } else {
            Box::pin(crate::provider_tool_execution::execute_provider_tool(
                &http,
                &entry,
                &target,
                &request_profiles,
                &response_profiles,
                arguments,
                &secret,
            ))
            .await
            .map(FixtureExecutionOutcome::Output)
        }
    }
    .await;
    // Join the bounded server before interpreting output so request mismatches remain deterministic.
    // 在解释输出前连接有界服务器，使请求不匹配保持确定性。
    let server_result = server
        .join()
        .map_err(|_| String::from("Fixture HTTP server panicked."))?;
    if let Err(server_error) = server_result {
        if let Err(execution_error) = execution.as_ref() {
            return Err(format!(
                "Fixture `{}` execution failed: {execution_error} | loopback verification failed: {server_error}",
                fixture.fixture_id
            ));
        }
        return Err(format!(
            "Fixture `{}` loopback verification failed: {server_error}",
            fixture.fixture_id
        ));
    }
    let output_kind = match execution {
        Ok(FixtureExecutionOutcome::Output(execution)) => {
            let output_kind = normalized_output_kind(&execution.output).to_string();
            let expected = fixture.expected_output_kind.as_deref().ok_or_else(|| {
                format!(
                    "Fixture `{}` produced output `{output_kind}` without an output expectation.",
                    fixture.fixture_id
                )
            })?;
            if output_kind != expected {
                return Err(format!(
                    "Fixture `{}` expected output `{expected}` but observed `{output_kind}`.",
                    fixture.fixture_id
                ));
            }
            if let Some(expected_text) = fixture.expected_text_output.as_deref() {
                let ProviderToolNormalizedOutput::Text(actual_text) = &execution.output else {
                    return Err(format!(
                        "Fixture `{}` declares exact text but produced `{output_kind}`.",
                        fixture.fixture_id
                    ));
                };
                if actual_text != expected_text {
                    return Err(format!(
                        "Fixture `{}` expected exact text `{expected_text}` but observed `{actual_text}`.",
                        fixture.fixture_id
                    ));
                }
            }
            if let Some(expected_search) = fixture.expected_search_output.as_ref() {
                let ProviderToolNormalizedOutput::Search(actual_search) = &execution.output else {
                    return Err(format!(
                        "Fixture `{}` declares exact search output but produced `{output_kind}`.",
                        fixture.fixture_id
                    ));
                };
                let actual_search = serde_json::json!({
                    "query": actual_search.query.as_str(),
                    "results": actual_search.results.iter().map(|result| serde_json::json!({
                        "rank": result.rank,
                        "title": result.title.as_str(),
                        "url": result.url.as_str(),
                        "source_domain": result.source_domain.as_str(),
                        "snippet": result.snippet.as_str()
                    })).collect::<Vec<_>>()
                });
                if &actual_search != expected_search {
                    return Err(format!(
                        "Fixture `{}` expected exact search output {} but observed {}.",
                        fixture.fixture_id, expected_search, actual_search
                    ));
                }
            }
            output_kind
        }
        Ok(FixtureExecutionOutcome::Cancelled) if fixture.expected_cancellation => {
            String::from("cancelled")
        }
        Ok(FixtureExecutionOutcome::Cancelled) => {
            return Err(format!(
                "Fixture `{}` was unexpectedly cancelled.",
                fixture.fixture_id
            ));
        }
        Err(error) => {
            let expected = fixture
                .expected_error_contains
                .as_deref()
                .ok_or_else(|| error.clone())?;
            if !error.contains(expected) {
                return Err(format!(
                    "Fixture `{}` expected error containing `{expected}` but observed `{error}`.",
                    fixture.fixture_id
                ));
            }
            String::from("expected_error")
        }
    };
    Ok(ProviderManifestFixtureResult {
        fixture_id: fixture.fixture_id,
        output_kind,
        interaction_count: fixture.interactions.len(),
    })
}

/// Executes one request, response, error, usage, or discovery fixture through production code.
/// 通过生产代码执行一份 request、response、error、usage 或 discovery fixture。
fn execute_provider_manifest_profile_fixture(
    fixture: ProviderManifestProfileFixture,
    fixture_path: &Path,
    manifests_directory: &Path,
) -> Result<ProviderManifestFixtureResult, String> {
    validate_profile_fixture_contract(&fixture, fixture_path)?;
    let base_path = safe_baseline_path(manifests_directory, fixture.base_manifest.as_str())?;
    let base_text = read_bounded_fixture_utf8(
        base_path.as_path(),
        MAX_FIXTURE_BASE_MANIFEST_BYTES,
        "fixture baseline manifest",
    )?;
    let mut document = parse_bounded_fixture_json(
        base_text.as_str(),
        &format!("Fixture baseline `{}`", base_path.display()),
    )?;
    if let Some(declaration) = fixture.declaration.as_ref() {
        let pointer = match fixture.fixture_kind {
            ProfileFixtureKind::Request => "/request_profiles",
            ProfileFixtureKind::Response | ProfileFixtureKind::Error => "/response_profiles",
            ProfileFixtureKind::Usage => "/usage_profiles",
            ProfileFixtureKind::Discovery => {
                return Err(format!(
                    "Fixture `{}` must not append a profile to discovery.",
                    fixture.fixture_id
                ));
            }
        };
        document
            .pointer_mut(pointer)
            .and_then(Value::as_array_mut)
            .ok_or_else(|| format!("Fixture baseline has no array at `{pointer}`."))?
            .push(declaration.clone());
    }
    append_unique_strings(
        &mut document,
        "/engine_contract/required_transformers",
        &fixture.required_transformers,
    )?;
    let manifest_text = serde_json::to_string(&document)
        .map_err(|error| format!("Fixture manifest could not serialize: {error}"))?;
    let resolved = crate::compile_manifest_text(&manifest_text, ManifestOwnership::System)
        .map_err(|error| format!("Fixture `{}` did not compile: {error}", fixture.fixture_id))?;
    let output = match fixture.fixture_kind {
        ProfileFixtureKind::Request => {
            let profile_id = fixture_profile_id(&fixture)?;
            let profile = resolved
                .request_profiles
                .iter()
                .find(|profile| profile.profile_id == profile_id)
                .and_then(|profile| profile.payload.as_ref())
                .ok_or_else(|| format!("Request profile `{profile_id}` has no payload."))?;
            crate::execution::execute_request_profile_fixture(
                profile,
                fixture.model_id.as_deref().unwrap_or("fixture/model"),
                fixture.input.clone(),
            )?
        }
        ProfileFixtureKind::Response | ProfileFixtureKind::Error => {
            let profile_id = fixture_profile_id(&fixture)?;
            let profile = resolved
                .response_profiles
                .iter()
                .find(|profile| profile.profile_id == profile_id)
                .and_then(|profile| profile.payload.as_ref())
                .cloned()
                .ok_or_else(|| format!("Response profile `{profile_id}` has no payload."))?;
            let input = serde_json::to_string(&fixture.input)
                .map_err(|error| format!("Fixture input could not serialize: {error}"))?;
            let output = crate::execution::project_response_record_text(&[profile], &input)
                .map_err(|error| error.message)?;
            serde_json::from_str(&output)
                .map_err(|error| format!("Projected response is invalid JSON: {error}"))?
        }
        ProfileFixtureKind::Usage => {
            let profile_id = fixture_profile_id(&fixture)?;
            let profile = resolved
                .usage_profiles
                .iter()
                .find(|profile| profile.profile_id == profile_id)
                .and_then(|profile| profile.payload.as_ref())
                .ok_or_else(|| format!("Usage profile `{profile_id}` has no payload."))?;
            let record_kind = fixture.record_kind.ok_or_else(|| {
                format!("Usage fixture `{}` has no record_kind.", fixture.fixture_id)
            })?;
            let observation = profile
                .extract(record_kind, &fixture.input)
                .map_err(|error| error.message)?
                .ok_or_else(|| {
                    format!(
                        "Usage fixture `{}` did not match its profile.",
                        fixture.fixture_id
                    )
                })?;
            serde_json::to_value(observation)
                .map_err(|error| format!("Usage observation could not serialize: {error}"))?
        }
        ProfileFixtureKind::Discovery => {
            let input = serde_json::to_string(&fixture.input)
                .map_err(|error| format!("Fixture input could not serialize: {error}"))?;
            let catalog = match fixture.discovery_kind.ok_or_else(|| {
                format!(
                    "Discovery fixture `{}` has no discovery_kind.",
                    fixture.fixture_id
                )
            })? {
                FixtureDiscoveryKind::OpenAi => {
                    crate::parse_openai_model_catalog("fixture-connection", 1, input.as_str())?
                }
            };
            serde_json::json!({
                "model_ids": catalog.models.into_iter().map(|model| model.id).collect::<Vec<_>>(),
                "page_count": catalog.page_count,
                "schema_revision": catalog.discovery_schema_revision
            })
        }
    };
    assert_json_pointer_expectations(&fixture, &output)?;
    Ok(ProviderManifestFixtureResult {
        fixture_id: fixture.fixture_id,
        output_kind: profile_fixture_kind_name(fixture.fixture_kind).to_string(),
        interaction_count: 0,
    })
}

/// Validates one profile fixture's closed cross-field contract.
/// 校验一份 Profile fixture 的封闭跨字段契约。
fn validate_profile_fixture_contract(
    fixture: &ProviderManifestProfileFixture,
    fixture_path: &Path,
) -> Result<(), String> {
    if fixture.schema_version != PROFILE_FIXTURE_SCHEMA_VERSION {
        return Err(format!(
            "Fixture `{}` uses unsupported schema `{}`.",
            fixture_path.display(),
            fixture.schema_version
        ));
    }
    if fixture.fixture_id.trim().is_empty() || fixture.expectations.is_empty() {
        return Err(format!(
            "Fixture `{}` must declare a non-empty id and expectations.",
            fixture_path.display()
        ));
    }
    let is_discovery = matches!(fixture.fixture_kind, ProfileFixtureKind::Discovery);
    if is_discovery
        != (fixture.discovery_kind.is_some()
            && fixture.declaration.is_none()
            && fixture.profile_id.is_none())
    {
        return Err(format!(
            "Fixture `{}` has an incoherent discovery/profile declaration.",
            fixture.fixture_id
        ));
    }
    if !is_discovery && (fixture.declaration.is_none() || fixture.profile_id.is_none()) {
        return Err(format!(
            "Fixture `{}` must declare one profile and profile_id.",
            fixture.fixture_id
        ));
    }
    if matches!(fixture.fixture_kind, ProfileFixtureKind::Usage) != fixture.record_kind.is_some() {
        return Err(format!(
            "Fixture `{}` has an incoherent usage record_kind.",
            fixture.fixture_id
        ));
    }
    if fixture
        .expectations
        .keys()
        .any(|pointer| !pointer.starts_with('/'))
    {
        return Err(format!(
            "Fixture `{}` contains a non-pointer expectation.",
            fixture.fixture_id
        ));
    }
    Ok(())
}

/// Returns the required profile id from one validated non-discovery fixture.
/// 从一份已校验的非 discovery fixture 返回必需 Profile 标识。
fn fixture_profile_id(fixture: &ProviderManifestProfileFixture) -> Result<&str, String> {
    fixture
        .profile_id
        .as_deref()
        .ok_or_else(|| format!("Fixture `{}` has no profile_id.", fixture.fixture_id))
}

/// Verifies every exact JSON Pointer expectation against one production output value.
/// 针对一份生产输出值验证每个精确 JSON Pointer 预期。
fn assert_json_pointer_expectations(
    fixture: &ProviderManifestProfileFixture,
    output: &Value,
) -> Result<(), String> {
    for (pointer, expected) in &fixture.expectations {
        let actual = output.pointer(pointer).ok_or_else(|| {
            format!(
                "Fixture `{}` output has no value at `{pointer}`: {output}",
                fixture.fixture_id
            )
        })?;
        if actual != expected {
            return Err(format!(
                "Fixture `{}` expected `{pointer}` = {expected}, observed {actual}.",
                fixture.fixture_id
            ));
        }
    }
    Ok(())
}

/// Returns the stable fixture-family name used by CLI reports.
/// 返回 CLI 报告使用的稳定 fixture 类型名称。
fn profile_fixture_kind_name(kind: ProfileFixtureKind) -> &'static str {
    match kind {
        ProfileFixtureKind::Request => "request",
        ProfileFixtureKind::Response => "response",
        ProfileFixtureKind::Error => "error",
        ProfileFixtureKind::Usage => "usage",
        ProfileFixtureKind::Discovery => "discovery",
    }
}

/// Validates bounded fixture-only contract rules before mutating a baseline document.
/// 在修改基线文档前校验有界的 fixture 专属契约规则。
fn validate_fixture_contract(
    fixture: &ProviderManifestExecutionFixture,
    fixture_path: &Path,
) -> Result<(), String> {
    if fixture.schema_version != EXECUTION_FIXTURE_SCHEMA_VERSION {
        return Err(format!(
            "Fixture `{}` uses unsupported schema `{}`.",
            fixture_path.display(),
            fixture.schema_version
        ));
    }
    if fixture.fixture_id.trim().is_empty() {
        return Err(format!(
            "Fixture `{}` has an empty id.",
            fixture_path.display()
        ));
    }
    let expectation_count = usize::from(fixture.expected_output_kind.is_some())
        + usize::from(fixture.expected_error_contains.is_some())
        + usize::from(fixture.expected_cancellation);
    if expectation_count != 1 || fixture.cancel_after_start != fixture.expected_cancellation {
        return Err(format!(
            "Fixture `{}` must declare exactly one coherent output, error, or cancellation expectation.",
            fixture.fixture_id
        ));
    }
    if fixture.expected_text_output.is_some()
        && fixture.expected_output_kind.as_deref() != Some("text")
    {
        return Err(format!(
            "Fixture `{}` may declare exact text only with expected_output_kind `text`.",
            fixture.fixture_id
        ));
    }
    if fixture.expected_search_output.is_some()
        && fixture.expected_output_kind.as_deref() != Some("search")
    {
        return Err(format!(
            "Fixture `{}` may declare exact search output only with expected_output_kind `search`.",
            fixture.fixture_id
        ));
    }
    if fixture.interactions.is_empty() || fixture.interactions.len() > MAX_FIXTURE_INTERACTIONS {
        return Err(format!(
            "Fixture `{}` must declare between one and {MAX_FIXTURE_INTERACTIONS} interactions.",
            fixture.fixture_id
        ));
    }
    for interaction in &fixture.interactions {
        if !matches!(interaction.method.as_str(), "GET" | "POST" | "DELETE")
            || !interaction.path.starts_with('/')
            || !(100..=599).contains(&interaction.response_status)
        {
            return Err(format!(
                "Fixture `{}` contains an invalid HTTP interaction.",
                fixture.fixture_id
            ));
        }
        if interaction.expected_body_contains.len() > 16
            || interaction
                .expected_body_contains
                .iter()
                .any(|fragment| fragment.is_empty() || fragment.len() > 1_024)
        {
            return Err(format!(
                "Fixture `{}` request-body fragment expectations exceed the bounded contract.",
                fixture.fixture_id
            ));
        }
    }
    Ok(())
}

/// Appends fixture declarations and required primitives to one baseline JSON document.
/// 向一份基线 JSON 文档追加 fixture 声明与所需原语。
fn append_fixture_declarations(
    document: &mut Value,
    fixture: &ProviderManifestExecutionFixture,
) -> Result<(), String> {
    document
        .get_mut("tool_targets")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| String::from("Fixture baseline has no tool_targets array."))?
        .push(fixture.tool_target.clone());
    document
        .get_mut("operations")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| String::from("Fixture baseline has no operations array."))?
        .push(fixture.operation.clone());
    document
        .get_mut("upload_channels")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| String::from("Fixture baseline has no upload_channels array."))?
        .extend(fixture.upload_channels.clone());
    document
        .get_mut("request_profiles")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| String::from("Fixture baseline has no request_profiles array."))?
        .extend(fixture.request_profiles.clone());
    document
        .get_mut("response_profiles")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| String::from("Fixture baseline has no response_profiles array."))?
        .extend(fixture.response_profiles.clone());
    append_unique_strings(
        document,
        "/engine_contract/required_execution_primitives",
        &fixture.required_execution_primitives,
    )?;
    append_unique_strings(
        document,
        "/engine_contract/required_upload_primitives",
        &fixture.required_upload_primitives,
    )?;
    append_unique_strings(
        document,
        "/engine_contract/required_transformers",
        &fixture.required_transformers,
    )
}

/// Appends unique string values to one exact JSON array pointer.
/// 向一个精确 JSON 数组指针追加唯一字符串值。
fn append_unique_strings(
    document: &mut Value,
    pointer: &str,
    additions: &[String],
) -> Result<(), String> {
    let values = document
        .pointer_mut(pointer)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| format!("Fixture baseline has no array at `{pointer}`."))?;
    for addition in additions {
        if !values.iter().any(|value| value.as_str() == Some(addition)) {
            values.push(Value::String(addition.clone()));
        }
    }
    Ok(())
}

/// Resolves a fixture baseline without allowing path traversal outside the manifest directory.
/// 解析 fixture 基线且不允许路径穿越 Manifest 目录之外。
fn safe_baseline_path(directory: &Path, file_name: &str) -> Result<PathBuf, String> {
    if file_name.is_empty()
        || Path::new(file_name).components().count() != 1
        || !file_name.ends_with(".json")
    {
        return Err(String::from(
            "Fixture base_manifest must be one local JSON file name.",
        ));
    }
    Ok(directory.join(file_name))
}

/// Lists direct JSON fixture files without recursively widening the trusted source set.
/// 列出直接 JSON fixture 文件且不递归扩大可信来源集合。
fn json_files(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "Fixture directory `{}` could not be read: {error}",
            directory.display()
        )
    })?;
    entries
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("Fixture directory entry failed: {error}"))
        })
        .filter_map(|result| match result {
            Ok(path) if path.extension().and_then(|value| value.to_str()) == Some("json") => {
                Some(Ok(path))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

/// Reads one bounded UTF-8 fixture document.
/// 读取一份有界 UTF-8 fixture 文档。
fn read_bounded_fixture(path: &Path) -> Result<String, String> {
    read_bounded_fixture_utf8(path, MAX_FIXTURE_BYTES, "provider fixture")
}

/// Reads one regular UTF-8 fixture artifact under an exact post-open byte boundary.
/// 在精确的打开后字节边界内读取一份普通 UTF-8 fixture 工件。
///
/// # Parameters
/// 参数
/// `path` is exact, `max_bytes` is the hard cap, and `kind` scopes diagnostics.
/// `path` 为精确路径，`max_bytes` 为硬上限，`kind` 限定诊断范围。
///
/// # Returns
/// 返回值
/// Exact text or a controlled type, I/O, replacement, size, or encoding failure.
/// 返回精确文本，或受控的类型、I/O、替换、大小或编码失败。
fn read_bounded_fixture_utf8(path: &Path, max_bytes: u64, kind: &str) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("{kind} `{}` could not be stat-ed: {error}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!(
            "{kind} `{}` is not a regular file.",
            path.display()
        ));
    }
    if metadata.len() > max_bytes {
        return Err(format!(
            "{kind} `{}` exceeds the {max_bytes} byte bound.",
            path.display()
        ));
    }
    let file = fs::File::open(path)
        .map_err(|error| format!("{kind} `{}` could not be opened: {error}", path.display()))?;
    let mut limited = file.take(max_bytes.saturating_add(1));
    let mut bytes = Vec::new();
    limited
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{kind} `{}` could not be read: {error}", path.display()))?;
    if bytes.len() as u64 > max_bytes {
        return Err(format!(
            "{kind} `{}` changed while reading and exceeds the {max_bytes} byte bound.",
            path.display()
        ));
    }
    String::from_utf8(bytes)
        .map_err(|error| format!("{kind} `{}` is not valid UTF-8: {error}", path.display()))
}

/// Parses one byte-bounded fixture JSON document under shared depth and node limits.
/// 在共享深度与节点数量限制下解析一份受字节约束的 fixture JSON 文档。
///
/// # Parameters
/// 参数
/// `text` is exact UTF-8 and `label` scopes diagnostics.
/// `text` 是精确 UTF-8，`label` 限定诊断范围。
///
/// # Returns
/// 返回值
/// A bounded JSON tree or a controlled syntax, depth, or node-count failure.
/// 返回有界 JSON 树，或受控的语法、深度或节点数量失败。
fn parse_bounded_fixture_json(text: &str, label: &str) -> Result<Value, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|error| format!("{label} is invalid: {error}"))?;
    crate::provider_manifest::validate_runtime_json_bounds(&value, label)?;
    Ok(value)
}

/// Starts one sequential loopback HTTP fixture server.
/// 启动一个顺序执行的回环 HTTP fixture 服务器。
fn spawn_fixture_server(
    mut interactions: Vec<FixtureHttpInteraction>,
) -> Result<(String, thread::JoinHandle<Result<(), String>>), String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("Fixture server could not bind: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("Fixture server could not enter nonblocking mode: {error}"))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("Fixture server address is unavailable: {error}"))?;
    let base_url = format!("http://{address}");
    for interaction in &mut interactions {
        if let Some(response) = interaction.response_json_body.as_mut() {
            replace_fixture_base_url(response, base_url.as_str());
        }
        if let Some(response) = interaction.response_text_body.as_mut() {
            *response = response.replace("{fixture_base_url}", base_url.as_str());
        }
    }
    let handle = thread::spawn(move || {
        for interaction in interactions {
            // Each expected request has its own deadline so a pre-dispatch failure cannot deadlock the CLI.
            // 每个预期请求都有独立截止时间，避免分派前失败使 CLI 死锁。
            let mut stream = accept_fixture_connection(&listener)?;
            serve_fixture_interaction(&mut stream, &interaction)?;
        }
        Ok(())
    });
    Ok((base_url, handle))
}

/// Replaces the fixture-only base URL token throughout one bounded response payload.
/// 在一份有界响应载荷中递归替换仅供夹具使用的基础 URL 标记。
///
/// # Parameters
/// 参数
/// `value` is fixture-authored response JSON and `base_url` is the loopback listener origin.
/// `value` 是夹具编写的响应 JSON，`base_url` 是回环监听器源地址。
fn replace_fixture_base_url(value: &mut Value, base_url: &str) {
    match value {
        Value::String(text) => {
            *text = text.replace("{fixture_base_url}", base_url);
        }
        Value::Array(values) => {
            for value in values {
                replace_fixture_base_url(value, base_url);
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                replace_fixture_base_url(value, base_url);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// Accepts one fixture connection through a bounded nonblocking loop.
/// 通过有界非阻塞循环接受一个 fixture 连接。
///
/// # Parameters
/// 参数
/// `listener` is the loopback-only server socket created for one fixture.
/// `listener` 是为单个 fixture 创建的仅回环服务器套接字。
///
/// # Returns
/// 返回值
/// One connected stream with bounded read/write timeouts, or a deterministic deadline error.
/// 返回一个带有界读写超时的已连接流，或一条确定性的截止时间错误。
fn accept_fixture_connection(listener: &TcpListener) -> Result<TcpStream, String> {
    // Five seconds is long enough for local scheduling and short enough to expose pre-dispatch failures.
    // 五秒足以覆盖本地调度，同时能及时暴露分派前失败。
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .and_then(|_| stream.set_write_timeout(Some(Duration::from_secs(5))))
                    .map_err(|error| {
                        format!("Fixture server could not bound connection I/O: {error}")
                    })?;
                return Ok(stream);
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(String::from(
                        "Fixture server timed out waiting for an expected loopback request.",
                    ));
                }
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(format!("Fixture server accept failed: {error}")),
        }
    }
}

/// Verifies one request and emits its configured bounded response.
/// 校验一个请求并发送其配置的有界响应。
fn serve_fixture_interaction(
    stream: &mut TcpStream,
    interaction: &FixtureHttpInteraction,
) -> Result<(), String> {
    let request = read_http_request(stream)?;
    let header_end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| String::from("Fixture request has no header terminator."))?;
    let headers = std::str::from_utf8(&request[..header_end])
        .map_err(|_| String::from("Fixture request headers are not UTF-8."))?;
    let request_line = headers
        .lines()
        .next()
        .ok_or_else(|| String::from("Fixture request line is missing."))?;
    let expected_line = format!("{} {} HTTP/1.1", interaction.method, interaction.path);
    if request_line != expected_line {
        return Err(format!(
            "Fixture expected request `{expected_line}` but received `{request_line}`."
        ));
    }
    for (expected_name, expected_value) in &interaction.expected_headers {
        let actual = headers.lines().skip(1).find_map(|line| {
            line.split_once(':').and_then(|(name, value)| {
                name.eq_ignore_ascii_case(expected_name)
                    .then(|| value.trim())
            })
        });
        if actual != Some(expected_value.as_str()) {
            return Err(format!(
                "Fixture expected request header `{expected_name}: {expected_value}` but received `{}`.",
                actual.unwrap_or("<missing>")
            ));
        }
    }
    for absent_name in &interaction.absent_headers {
        if headers.lines().skip(1).any(|line| {
            line.split_once(':')
                .is_some_and(|(name, _)| name.eq_ignore_ascii_case(absent_name))
        }) {
            return Err(format!(
                "Fixture request header `{absent_name}` must be absent."
            ));
        }
    }
    // requestBody is shared by exact JSON and bounded raw-fragment verification.
    // requestBody 由精确 JSON 校验与有界原始分片校验共享。
    let request_body = &request[header_end + 4..];
    if let Some(expected) = interaction.expected_json_body.as_ref() {
        let actual: Value = serde_json::from_slice(request_body)
            .map_err(|error| format!("Fixture request JSON is invalid: {error}"))?;
        if &actual != expected {
            return Err(format!(
                "Fixture request JSON mismatch: expected {expected}, received {actual}."
            ));
        }
    }
    for expected_fragment in &interaction.expected_body_contains {
        if !request_body
            .windows(expected_fragment.len())
            .any(|window| window == expected_fragment.as_bytes())
        {
            return Err(format!(
                "Fixture request body is missing expected fragment `{expected_fragment}`."
            ));
        }
    }
    let (body, default_content_type) = match (
        interaction.response_json_body.as_ref(),
        interaction.response_text_body.as_ref(),
    ) {
        (Some(value), None) => (
            serde_json::to_vec(value)
                .map_err(|error| format!("Fixture response could not serialize: {error}"))?,
            "application/json",
        ),
        (None, Some(value)) => (value.as_bytes().to_vec(), "text/plain; charset=utf-8"),
        _ => {
            return Err(String::from(
                "Fixture interaction must declare exactly one response body form.",
            ));
        }
    };
    let content_type = interaction
        .response_content_type
        .as_deref()
        .unwrap_or(default_content_type);
    let reason = if (200..300).contains(&interaction.response_status) {
        "OK"
    } else {
        "ERROR"
    };
    if interaction.response_headers.len() > 16 {
        return Err(String::from(
            "Fixture response headers exceed the 16 entry limit.",
        ));
    }
    let mut response_headers = String::new();
    for (name, value) in &interaction.response_headers {
        if matches!(
            name.to_ascii_lowercase().as_str(),
            "content-type" | "content-length" | "connection" | "transfer-encoding"
        ) || reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_err()
            || reqwest::header::HeaderValue::from_str(value).is_err()
        {
            return Err(format!(
                "Fixture response header `{name}` is invalid or framing-owned."
            ));
        }
        response_headers.push_str(name);
        response_headers.push_str(": ");
        response_headers.push_str(value);
        response_headers.push_str("\r\n");
    }
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n",
        interaction.response_status,
        reason,
        content_type,
        response_headers,
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .and_then(|_| stream.write_all(body.as_slice()))
        .map_err(|error| format!("Fixture response write failed: {error}"))
}

/// Reads one bounded HTTP/1.1 request including its declared body.
/// 读取一份包含声明正文的有界 HTTP/1.1 请求。
fn read_http_request(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut expected_length = None;
    loop {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| format!("Fixture request read failed: {error}"))?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        if request.len() > MAX_FIXTURE_BYTES as usize {
            return Err(String::from("Fixture request exceeds 2 MiB."));
        }
        if expected_length.is_none()
            && let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
        {
            let headers = std::str::from_utf8(&request[..header_end])
                .map_err(|_| String::from("Fixture request headers are not UTF-8."))?;
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                })
                .unwrap_or(0);
            expected_length = Some(header_end + 4 + content_length);
        }
        if expected_length.is_some_and(|length| request.len() >= length) {
            break;
        }
    }
    Ok(request)
}

/// Returns the stable name of one normalized output variant.
/// 返回一个规范输出变体的稳定名称。
fn normalized_output_kind(output: &ProviderToolNormalizedOutput) -> &'static str {
    match output {
        ProviderToolNormalizedOutput::Text(_) => "text",
        ProviderToolNormalizedOutput::Resources(_) => "resources",
        ProviderToolNormalizedOutput::Embedding(_) => "embedding",
        ProviderToolNormalizedOutput::Rerank(_) => "rerank",
        ProviderToolNormalizedOutput::Search(_) => "search",
        ProviderToolNormalizedOutput::Extract(_) => "extract",
        ProviderToolNormalizedOutput::Composite(_) => "composite",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds the executable VideoExtend fixture as one ordinary manifest document.
    /// 把可执行 VideoExtend fixture 构建为一份普通 Manifest 文档。
    ///
    /// # Returns
    /// 返回值
    /// The baseline manifest with every fixture-owned declaration appended.
    /// 返回已追加全部 fixture 声明的基线 Manifest。
    fn video_extend_fixture_document() -> Value {
        // assetsRoot is the checked-in provider-manifest resource root.
        // assetsRoot 是签入仓库的供应商清单资源根目录。
        let assets_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("vulcan-runtime-assets")
            .join("resources")
            .join("provider-manifests");
        // fixturePath identifies the independent VideoExtend executable contract.
        // fixturePath 标识独立的 VideoExtend 可执行契约。
        let fixture_path = assets_root
            .join("fixtures")
            .join("json-only-video-extend.json");
        // fixtureText contains the exact checked-in fixture bytes as UTF-8 text.
        // fixtureText 包含签入 fixture 精确字节对应的 UTF-8 文本。
        let fixture_text = fs::read_to_string(&fixture_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", fixture_path.display()));
        // fixture is the closed typed fixture used by the production fixture executor.
        // fixture 是生产 fixture 执行器使用的封闭类型化 fixture。
        let fixture: ProviderManifestExecutionFixture =
            serde_json::from_str(&fixture_text).expect("VideoExtend fixture must parse");
        // basePath is the exact bundled baseline named by the fixture.
        // basePath 是 fixture 指定的精确内置基线。
        let base_path = assets_root
            .join("manifests")
            .join(fixture.base_manifest.as_str());
        // baseText contains the immutable bundled authority extended only for the test.
        // baseText 包含仅供测试扩展的不可变内置权限。
        let base_text = fs::read_to_string(&base_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", base_path.display()));
        // document receives the exact fixture declarations before capability mismatch injection.
        // document 在注入能力不匹配前接收精确 fixture 声明。
        let mut document: Value =
            serde_json::from_str(&base_text).expect("fixture baseline must parse");
        append_fixture_declarations(&mut document, &fixture)
            .expect("VideoExtend declarations must append");
        document
    }

    /// Proves a VideoExtend target cannot be owned by a VideoEdit operation.
    /// 证明 VideoExtend 目标不能由 VideoEdit operation 持有。
    #[test]
    fn video_edit_operation_cannot_own_video_extend_target() {
        // document starts from the executable VideoExtend fixture.
        // document 从可执行 VideoExtend fixture 开始。
        let mut document = video_extend_fixture_document();
        // operation is the fixture-owned declaration whose semantic identity is corrupted.
        // operation 是被破坏语义身份的 fixture 自有声明。
        let operation = document["operations"]
            .as_array_mut()
            .and_then(|operations| operations.last_mut())
            .expect("fixture operation must exist");
        operation["operation"] = Value::String(String::from("video_edit"));
        // manifestText is the exact malformed document submitted to the normal compiler.
        // manifestText 是提交给常规编译器的精确畸形文档。
        let manifest_text = serde_json::to_string(&document).expect("document must serialize");
        // error must retain the stable mismatch classification rather than accepting aliasing.
        // error 必须保留稳定的不匹配分类，而不是接受能力别名。
        let error = crate::compile_manifest_text(&manifest_text, ManifestOwnership::System)
            .expect_err("VideoEdit must not own a VideoExtend target");
        assert!(
            error
                .to_string()
                .contains("tool_target_operation_capability_mismatch"),
            "unexpected validation error: {error}"
        );
    }

    /// Proves a VideoEdit target cannot masquerade behind a VideoExtend operation.
    /// 证明 VideoEdit 目标不能伪装在 VideoExtend operation 之后。
    #[test]
    fn video_edit_target_cannot_masquerade_as_video_extend() {
        // document starts from the executable VideoExtend fixture.
        // document 从可执行 VideoExtend fixture 开始。
        let mut document = video_extend_fixture_document();
        // target is the fixture-owned declaration whose semantic capability is corrupted.
        // target 是被破坏语义能力的 fixture 自有声明。
        let target = document["tool_targets"]
            .as_array_mut()
            .and_then(|targets| targets.last_mut())
            .expect("fixture target must exist");
        target["capability"] = Value::String(String::from("video_edit"));
        // manifestText is the exact malformed document submitted to the normal compiler.
        // manifestText 是提交给常规编译器的精确畸形文档。
        let manifest_text = serde_json::to_string(&document).expect("document must serialize");
        // error must retain the stable mismatch classification rather than accepting aliasing.
        // error 必须保留稳定的不匹配分类，而不是接受能力别名。
        let error = crate::compile_manifest_text(&manifest_text, ManifestOwnership::System)
            .expect_err("VideoEdit target must not masquerade as VideoExtend");
        assert!(
            error
                .to_string()
                .contains("tool_target_operation_capability_mismatch"),
            "unexpected validation error: {error}"
        );
    }

    /// Executes the MiniMax speech fixture in isolation to prove URL audio survives projection.
    /// 独立执行 MiniMax 语音 fixture，证明 URL 音频能够通过响应投影。
    ///
    /// # Returns
    /// 返回值
    /// Completes normally only when the generic executor produces one resources result.
    /// 仅当通用执行器生成一份资源结果时正常完成。
    #[tokio::test]
    async fn minimax_speech_fixture_executes_with_url_audio() {
        // assets_root is the checked-in provider-manifest resource root.
        // assets_root 是签入仓库的供应商清单资源根目录。
        let assets_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("vulcan-runtime-assets")
            .join("resources")
            .join("provider-manifests");
        // fixture_path identifies only the MiniMax synchronous speech executable contract.
        // fixture_path 仅标识 MiniMax 同步语音可执行契约。
        let fixture_path = assets_root
            .join("fixtures")
            .join("minimax-speech-generic.json");
        // result proves request projection, response projection, and URL resource decoding together.
        // result 联合证明请求投影、响应投影与 URL 资源解码。
        let result = execute_fixture_document(
            fixture_path.as_path(),
            assets_root.join("manifests").as_path(),
        )
        .await
        .expect("MiniMax speech fixture must execute");
        assert_eq!(result.fixture_id, "minimax-speech-generic");
        assert_eq!(result.output_kind, "resources");
        assert_eq!(result.interaction_count, 1);
    }
}
