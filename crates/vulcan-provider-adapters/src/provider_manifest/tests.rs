//! Schema drift tests: Rust DTO field sets versus the committed JSON Schema resources.
//! Schema 漂移测试：Rust DTO 字段集合与已提交 JSON Schema 资源的对比。

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::EnginePrimitiveRegistry;

/// Verifies the engine registry never advertises schema-reserved or nonexistent execution paths.
/// 验证引擎注册表绝不公布仅由 Schema 预留或根本不存在的执行路径。
#[test]
fn current_execution_registry_contains_only_runtime_backed_primitives() {
    let registry = EnginePrimitiveRegistry::current_protocol_baseline();

    assert_eq!(
        registry.execution_primitives,
        BTreeSet::from([
            String::from("conversation_media_analyze"),
            // Each DeepSeek replay contract owns projection, acceptance and its own wire tests.
            // 每个 DeepSeek 回传契约都具备投影、接受条件与各自的线路测试。
            String::from("deepseek_anthropic_thinking_v1"),
            String::from("deepseek_chat_reasoning_content_v1"),
            String::from("deepseek_responses_plaintext_v1"),
            String::from("http_json_async_poll"),
            String::from("http_json_sse"),
            String::from("http_json_sync"),
            String::from("http_multipart_sse"),
            String::from("mcp_http_call"),
        ])
    );
}

/// Verifies bundled Kimi video targets close target, operation, upload, and reference ownership.
/// 验证内置 Kimi 视频目标完整闭合目标、操作、上传与引用归属。
#[test]
fn bundled_video_understanding_targets_compile_with_exact_upload_references() {
    // expectedTargets enumerates only provider contracts proven by the Kimi reference implementation.
    // expectedTargets 只枚举由 Kimi 参考实现证实的供应商契约。
    let expected_targets = vec![
        (
            "platform-cn.kimi-k2.7-code.video-understanding",
            "platform-cn.video-file",
            "api.moonshot.cn",
            "/v1/files",
        ),
        (
            "platform-global.kimi-k2.7-code.video-understanding",
            "platform-global.video-file",
            "api.moonshot.ai",
            "/v1/files",
        ),
        (
            "coding-plan.kimi-for-coding.video-understanding",
            "coding-plan.video-file",
            "api.kimi.com",
            "/coding/v1/files",
        ),
    ];
    // manifestPath is the exact authoritative bundled source under test.
    // manifestPath 是本测试验证的精确权威内置源。
    let manifest_path =
        vulcan_runtime_assets::source_bundled_provider_manifests_directory().join("kimi.json");
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|_| panic!("missing bundled manifest at {}", manifest_path.display()));
    let compiled = super::compiler::compile_manifest_text(
        manifest_text.as_str(),
        super::ManifestOwnership::System,
    )
    .unwrap_or_else(|error| panic!("Kimi manifest must compile: {error}"));
    for (target_id, upload_channel_id, endpoint_host, endpoint_path) in expected_targets {
        // target owns the fixed facade projection while one unique operation owns upload.
        // target 持有固定外壳投影，而唯一操作持有上传。
        let target = compiled
            .tool_targets
            .iter()
            .find(|target| target.target_id == target_id)
            .unwrap_or_else(|| panic!("missing video target `{target_id}`"));
        assert_eq!(
            target.contract.capability,
            crate::ProviderToolCapabilityKind::VideoUnderstand
        );
        let projection = target
            .contract
            .media_input_projection
            .as_ref()
            .expect("video target must declare a fixed-facade projection");
        assert_eq!(projection.path_field, "video_path");
        assert_eq!(projection.prompt_field, "prompt");
        let operation = compiled
            .operations
            .iter()
            .find(|operation| operation.tool_target_refs.contains(&target_id.to_string()))
            .unwrap_or_else(|| panic!("missing operation for `{target_id}`"));
        assert_eq!(operation.upload_bindings.len(), 1);
        assert_eq!(operation.upload_bindings[0].source_path, "/video_path");
        assert_eq!(operation.upload_bindings[0].target_path, "/video_reference");
        assert_eq!(
            operation.upload_bindings[0].upload_channel_refs,
            [upload_channel_id.to_string()]
        );
        let channel = compiled
            .upload_channels
            .iter()
            .find(|channel| channel.upload_channel_id == upload_channel_id)
            .unwrap_or_else(|| panic!("missing upload channel `{upload_channel_id}`"));
        assert_eq!(
            channel.primitive,
            Some(crate::UploadPrimitiveKind::MultipartFile)
        );
        assert_eq!(channel.endpoint_host.as_deref(), Some(endpoint_host));
        assert_eq!(channel.endpoint_path.as_deref(), Some(endpoint_path));
        assert_eq!(channel.method.as_deref(), Some("POST"));
        assert_eq!(channel.file_field.as_deref(), Some("file"));
        assert_eq!(channel.max_bytes, Some(100_000_000));
        assert_eq!(channel.response_id_pointer.as_deref(), Some("/id"));
        assert_eq!(
            channel.fixed_fields.get("purpose").map(String::as_str),
            Some("video")
        );
        assert_eq!(channel.reference_template.as_deref(), Some("ms://{id}"));
        assert!(channel.progress_supported);
        assert!(channel.cancellation_supported);
    }
}

/// Verifies MiniMax M3 binds to the video-capable OpenAI route, rejects native structured output, and owns both regional upload channels.
/// 验证 MiniMax M3 固定绑定支持视频的 OpenAI 路由、不声明原生结构化输出，并拥有两个区域上传通道。
/// Verifies service tags follow entry kinds and standalone tool families, not generation features.
/// 验证服务标记遵循入口类型与独立工具类别，而非生成类功能。
#[test]
fn compiled_descriptor_derives_service_tags_from_entries_and_tool_families() {
    use crate::ProviderServiceTag::{Analysis, Model, Search};
    // compiled_tags compiles one bundled manifest and returns its derived tags.
    // compiled_tags 编译一份随包清单并返回其派生标记。
    let compiled_tags = |file: &str| {
        let path = vulcan_runtime_assets::source_bundled_provider_manifests_directory().join(file);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("missing bundled manifest at {}", path.display()));
        super::compiler::compile_manifest_text(text.as_str(), super::ManifestOwnership::System)
            .unwrap_or_else(|error| panic!("{file} must compile: {error:?}"))
            .descriptor
            .service_tags
    };
    // Tavily exposes only provider tools, so it carries no model tag.
    // Tavily 只公开供应商工具，因此没有模型标记。
    assert_eq!(compiled_tags("tavily.json"), vec![Search]);
    // Kimi's video understanding is a standalone analysis service beside its models.
    // Kimi 的视频理解是模型之外的独立分析服务。
    assert_eq!(compiled_tags("kimi.json"), vec![Model, Analysis]);
    // Image generation belongs to models and adds no tag of its own.
    // 图片生成归属于模型，不单独增加标记。
    assert_eq!(compiled_tags("apikeyfan.json"), vec![Model]);
}

#[test]
fn bundled_minimax_m3_declares_image_video_and_provider_upload_contract() {
    // manifestPath is the authoritative MiniMax source asset used by production compilation.
    // manifestPath 是生产编译使用的权威 MiniMax 源资源。
    let manifest_path =
        vulcan_runtime_assets::source_bundled_provider_manifests_directory().join("minimax.json");
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|_| panic!("missing MiniMax manifest at {}", manifest_path.display()));
    // compiled is the same validated manifest representation consumed by route resolution.
    // compiled 是路由解析实际消费的同一份已校验 Manifest 表示。
    let compiled = super::compiler::compile_manifest_text(
        manifest_text.as_str(),
        super::ManifestOwnership::System,
    )
    .expect("MiniMax manifest must compile");
    // m3 freezes the model-level multimodal declaration and the protocol used by normal chat.
    // m3 冻结模型级多模态声明以及普通对话使用的协议。
    let m3 = compiled
        .rule_for_model("cn", Some("token-plan-key"), "MiniMax-M3")
        .expect("MiniMax M3 must have a CN rule instance");
    assert_eq!(
        m3.selected_protocol.as_deref(),
        Some("openai-chat-completions.v1")
    );
    assert!(
        m3.supported_protocols
            .iter()
            .any(|protocol| protocol == "anthropic-messages-video.v1")
    );
    let capabilities = m3
        .capabilities
        .as_ref()
        .expect("MiniMax M3 must declare capabilities");
    assert_eq!(
        capabilities.input_resources,
        vec![
            vulcan_ai_contract::ResourceKind::Image,
            vulcan_ai_contract::ResourceKind::Video
        ]
    );
    assert_eq!(
        capabilities.structured_output,
        vulcan_ai_contract::StructuredOutputCapability::Unsupported
    );

    // Each regional upload channel must preserve the verified MiniMax file API semantics.
    // 每个区域上传通道都必须保留已验证的 MiniMax 文件 API 语义。
    for (channel_id, host) in [
        ("global.video-file", "api.minimax.io"),
        ("cn.video-file", "api.minimaxi.com"),
    ] {
        let channel = compiled
            .upload_channels
            .iter()
            .find(|channel| channel.upload_channel_id == channel_id)
            .unwrap_or_else(|| panic!("missing MiniMax upload channel `{channel_id}`"));
        assert_eq!(channel.endpoint_host.as_deref(), Some(host));
        assert_eq!(channel.endpoint_path.as_deref(), Some("/v1/files/upload"));
        assert_eq!(channel.file_field.as_deref(), Some("file"));
        assert_eq!(channel.max_bytes, Some(50_000_000));
        assert_eq!(
            channel.response_id_pointer.as_deref(),
            Some("/file/file_id")
        );
        assert_eq!(
            channel.response_id_encoding,
            Some(super::UploadResponseIdEncoding::UnsignedInteger)
        );
        assert_eq!(
            channel.reference_template.as_deref(),
            Some("mm_file://{id}")
        );
        assert_eq!(
            channel.fixed_fields.get("purpose").map(String::as_str),
            Some("video_understanding")
        );
        assert!(channel.progress_supported);
        assert!(channel.cancellation_supported);
        assert_eq!(channel.cache_duration_ms, Some(86_400_000));
    }
}

/// Verifies every bundled Kimi video route preserves the complete format set accepted by kimi-cli.
/// 验证每条内置 Kimi 视频路由均保留 kimi-cli 接受的完整格式集合。
#[test]
fn bundled_kimi_video_routes_preserve_reference_format_set() {
    // manifestPath points to the authoritative Kimi source asset.
    // manifestPath 指向权威 Kimi 源资源。
    let manifest_path =
        vulcan_runtime_assets::source_bundled_provider_manifests_directory().join("kimi.json");
    // manifestText is compiled through the production validator before assertions inspect it.
    // manifestText 在断言检查前通过生产校验器编译。
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|_| panic!("missing Kimi manifest at {}", manifest_path.display()));
    // compiled owns the normalized operations and channels used at runtime.
    // compiled 持有运行时使用的规范化操作与通道。
    let compiled = super::compiler::compile_manifest_text(
        manifest_text.as_str(),
        super::ManifestOwnership::System,
    )
    .expect("Kimi manifest must compile");
    // expectedFormats mirrors kimi-cli's closed video suffix table and MIME decisions.
    // expectedFormats 对应 kimi-cli 的封闭视频后缀表与 MIME 决策。
    let expected_formats = [
        ("mp4", "video/mp4"),
        ("mkv", "video/x-matroska"),
        ("avi", "video/x-msvideo"),
        ("mov", "video/quicktime"),
        ("wmv", "video/x-ms-wmv"),
        ("webm", "video/webm"),
        ("m4v", "video/x-m4v"),
        ("flv", "video/x-flv"),
        ("3gp", "video/3gpp"),
        ("3g2", "video/3gpp2"),
    ];

    for operation in compiled
        .operations
        .iter()
        .filter(|operation| operation.operation == "video_understand")
    {
        // binding is unique because each fixed video facade owns one source file.
        // binding 是唯一的，因为每个固定视频外壳只持有一个源文件。
        let binding = operation
            .upload_bindings
            .first()
            .expect("Kimi video operation must own one upload binding");
        // channel is the exact upload declaration referenced by this operation.
        // channel 是当前操作引用的精确上传声明。
        let channel_id = operation
            .upload_channel_refs
            .first()
            .expect("Kimi video operation must reference one upload channel");
        let channel = compiled
            .upload_channels
            .iter()
            .find(|channel| &channel.upload_channel_id == channel_id)
            .expect("Kimi video upload channel must exist");
        for (extension, mime_type) in expected_formats {
            assert_eq!(
                binding
                    .mime_types_by_extension
                    .get(extension)
                    .map(String::as_str),
                Some(mime_type),
                "operation `{}` must map .{extension}",
                operation.operation_id
            );
            assert!(
                channel
                    .accepted_mime_types
                    .iter()
                    .any(|accepted| accepted == mime_type),
                "channel `{channel_id}` must accept `{mime_type}`"
            );
        }
    }
}

/// Verifies media targets require reviewed projection metadata and non-media targets reject it.
/// 验证媒体目标必须具有已审核投影元数据，且非媒体目标拒绝该元数据。
#[test]
fn media_input_projection_is_closed_by_manifest_validation() {
    let manifest_path =
        vulcan_runtime_assets::source_bundled_provider_manifests_directory().join("kimi.json");
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|_| panic!("missing Kimi manifest at {}", manifest_path.display()));
    let mut manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("Kimi manifest must be valid JSON");
    let video_target = manifest["tool_targets"]
        .as_array_mut()
        .expect("tool targets")
        .iter_mut()
        .find(|target| target["capability"] == "video_understand")
        .expect("video target");
    video_target
        .as_object_mut()
        .expect("video target object")
        .remove("media_input_projection");
    let invalid_text = serde_json::to_string(&manifest).expect("manifest must serialize");
    assert!(
        super::compiler::compile_manifest_text(
            invalid_text.as_str(),
            super::ManifestOwnership::System,
        )
        .is_err(),
        "a media target without explicit projection must fail closed"
    );
}

/// Verifies APIKEY.FAN DeepSeek keeps one canonical reasoning channel without weakening Responses validation.
/// 验证 APIKEY.FAN DeepSeek 在不放宽 Responses 校验的前提下只保留一条规范思考通道。
#[test]
fn apikeyfan_deepseek_response_profile_filters_only_mirrored_reasoning_events() {
    // manifestPath owns the exact bundled provider definition under test.
    // manifestPath 指向本测试使用的精确内置供应商定义。
    let manifest_path =
        vulcan_runtime_assets::source_bundled_provider_manifests_directory().join("apikeyfan.json");
    // manifestText is compiled through the same system-manifest path used at publication.
    // manifestText 通过发布时使用的同一系统 Manifest 路径编译。
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|_| panic!("missing APIKEY.FAN manifest at {}", manifest_path.display()));
    // compiled freezes model-scoped response-profile ownership and executable payloads.
    // compiled 冻结模型作用域响应 Profile 的归属与可执行载荷。
    let compiled = super::compiler::compile_manifest_text(
        manifest_text.as_str(),
        super::ManifestOwnership::System,
    )
    .expect("APIKEY.FAN manifest must compile");
    // deepseekRule must own the relay-specific response projection while unrelated models do not.
    // deepseekRule 必须持有中转站专属响应投影，而无关模型不得继承该投影。
    let deepseek_rule = compiled
        .rule_instances
        .iter()
        .find(|rule| rule.rule_id == "deepseek-v4-flash")
        .expect("DeepSeek V4 Flash rule must compile");
    assert_eq!(
        deepseek_rule.response_profile_refs,
        vec![String::from(
            "apikeyfan.deepseek.responses.reasoning-mirror.v1"
        )]
    );
    let unrelated_rule = compiled
        .rule_instances
        .iter()
        .find(|rule| rule.rule_id == "gpt-5-6-sol")
        .expect("unrelated GPT rule must compile");
    assert!(unrelated_rule.response_profile_refs.is_empty());

    // responseProfile is the exact frozen payload selected by both DeepSeek rules.
    // responseProfile 是两条 DeepSeek 规则选择的精确冻结载荷。
    let response_profile = compiled
        .response_profiles
        .iter()
        .find(|profile| profile.profile_id == "apikeyfan.deepseek.responses.reasoning-mirror.v1")
        .and_then(|profile| profile.payload.clone())
        .expect("DeepSeek reasoning mirror profile must compile");
    // cases prove only the invalid duplicated channel is renamed into ignored lifecycle metadata.
    // cases 证明只有无效重复通道会被改名为可忽略的生命周期元数据。
    let cases = [
        (
            "response.reasoning_text.delta",
            "response.apikeyfan.reasoning_text_mirror.delta",
        ),
        (
            "response.reasoning_text.done",
            "response.apikeyfan.reasoning_text_mirror.done",
        ),
        (
            "response.reasoning_summary_text.delta",
            "response.reasoning_summary_text.delta",
        ),
        ("response.output_text.delta", "response.output_text.delta"),
    ];
    for (wire_type, expected_type) in cases {
        let projected = crate::execution::project_json_response_profiles(
            std::slice::from_ref(&response_profile),
            serde_json::json!({"type": wire_type, "delta": "verified"}),
        )
        .expect("APIKEY.FAN response projection must succeed");
        assert_eq!(projected["type"], expected_type);
        assert_eq!(projected["delta"], "verified");
    }
}

/// Resolves one committed JSON Schema resource in the runtime-assets crate.
/// 解析 runtime-assets crate 中一份已提交 JSON Schema 资源。
fn schema_resource(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("vulcan-runtime-assets")
        .join("resources")
        .join("provider-manifests")
        .join("schema")
        .join(relative)
}

/// Reads and parses one committed JSON Schema resource.
/// 读取并解析一份已提交 JSON Schema 资源。
fn load_schema(relative: &str) -> serde_json::Value {
    let path = schema_resource(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing schema resource at {}", path.display()));
    serde_json::from_str(&text).expect("schema resource must be valid JSON")
}

/// Collects RFC 6901 pointers for every object contained in one JSON document.
/// 收集一份 JSON 文档内每个对象的 RFC 6901 指针。
///
/// # Parameters
/// 参数
/// `value` is the current JSON subtree, `pointer` is its escaped pointer, and `output` receives
/// every object pointer including the root object.
/// `value` 是当前 JSON 子树，`pointer` 是其转义指针，`output` 接收包括根对象在内的每个对象指针。
fn collect_json_object_pointers(
    value: &serde_json::Value,
    pointer: &str,
    output: &mut Vec<String>,
) {
    match value {
        serde_json::Value::Object(object) => {
            output.push(pointer.to_string());
            for (key, child) in object {
                let escaped = key.replace('~', "~0").replace('/', "~1");
                collect_json_object_pointers(
                    child,
                    format!("{pointer}/{escaped}").as_str(),
                    output,
                );
            }
        }
        serde_json::Value::Array(array) => {
            for (index, child) in array.iter().enumerate() {
                collect_json_object_pointers(child, format!("{pointer}/{index}").as_str(), output);
            }
        }
        _ => {}
    }
}

/// Normalizes array indexes in one pointer so repeated instances share one structural test case.
/// 规范化一个指针中的数组索引，使重复实例共享同一个结构测试用例。
///
/// # Parameters
/// 参数
/// `pointer` is one escaped RFC 6901 object pointer.
/// `pointer` 是一个已转义的 RFC 6901 对象指针。
///
/// # Returns
/// 返回值
/// A stable path with every numeric array segment replaced by `*`.
/// 返回把每个数字数组段替换为 `*` 的稳定路径。
fn normalized_object_pointer(pointer: &str) -> String {
    pointer
        .split('/')
        .map(|segment| {
            if !segment.is_empty() && segment.bytes().all(|byte| byte.is_ascii_digit()) {
                "*"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Returns the exact top-level field set of the Rust manifest DTO.
/// 返回 Rust Manifest DTO 的精确顶层字段集合。
fn rust_top_level_fields() -> BTreeSet<&'static str> {
    [
        "schema_version",
        "provider_id",
        "revision",
        "published_at",
        "engine_contract",
        "display",
        "credential_registration",
        "services",
        "model_defaults",
        "model_rules",
        "usage_profiles",
        "request_profiles",
        "response_profiles",
        "upload_channels",
        "operations",
        "tool_targets",
        "tool_target_templates",
        "tool_target_template_bindings",
        "provenance",
    ]
    .into_iter()
    .collect()
}

/// Returns the exact serialized field set of the Rust service-entry DTO.
/// 返回 Rust 服务入口 DTO 的精确序列化字段集合。
fn rust_service_entry_fields() -> BTreeSet<&'static str> {
    [
        "service_entry_id",
        "display_name",
        "kind",
        "base_url",
        "credential_profiles",
        "default_authentication",
        "request_headers",
        "session_header",
        "service_origin_override_policy",
        "native_inference_bindings",
        "request_projector",
        "model_catalog",
        "protocol_routes",
        "model_defaults_override",
    ]
    .into_iter()
    .collect()
}

/// Returns the exact serialized field set of the Rust credential-profile DTO.
/// 返回 Rust 凭据配置档 DTO 的精确序列化字段集合。
fn rust_credential_profile_fields() -> BTreeSet<&'static str> {
    [
        "credential_profile_id",
        "display_name",
        "credential_mode",
        "authorization",
        "billing_kind",
        "account_usage_query",
        "account_model_access",
        "authentication",
        "key_prefix",
        "credential_role",
        "default_usage_scopes",
    ]
    .into_iter()
    .collect()
}

/// Returns the exact serialized field set of the Rust model-rule DTO.
/// 返回 Rust 模型规则 DTO 的精确序列化字段集合。
fn rust_model_rule_fields() -> BTreeSet<&'static str> {
    [
        "rule_id",
        "scope",
        "matcher",
        "display_name",
        "supported_protocols",
        "selected_protocol",
        "codex_behavior",
        "capabilities",
        "tool_schema_projection",
        "protocol_capabilities",
        "usage_profile_refs",
        "request_profile_refs",
        "response_profile_refs",
        "input_resource_constraints",
        "model_parameters",
        "operation_refs",
        "allowed_account_plan_names",
        "account_access_groups",
        "default_enabled",
        "provenance_ref",
    ]
    .into_iter()
    .collect()
}

/// Returns the exact serialized field set of the Rust model-catalog DTO.
/// 返回 Rust 模型目录 DTO 的精确序列化字段集合。
fn rust_model_catalog_fields() -> BTreeSet<&'static str> {
    ["source_policy", "discovery_kind", "path", "models"]
        .into_iter()
        .collect()
}

/// Verifies the manifest JSON Schema stays in sync with the Rust DTO top level.
/// 验证 Manifest JSON Schema 与 Rust DTO 顶层保持同步。
#[test]
fn manifest_schema_top_level_matches_rust_dto() {
    let schema = load_schema("provider-manifest-v2.schema.json");
    let properties = schema["properties"]
        .as_object()
        .expect("schema must declare properties");
    let schema_fields: BTreeSet<&str> = properties.keys().map(String::as_str).collect();
    let rust_fields = rust_top_level_fields();

    let missing_in_schema: Vec<_> = rust_fields.difference(&schema_fields).copied().collect();
    let missing_in_rust: Vec<_> = schema_fields.difference(&rust_fields).copied().collect();
    assert!(
        missing_in_schema.is_empty(),
        "Rust DTO fields missing from JSON Schema: {missing_in_schema:?}"
    );
    assert!(
        missing_in_rust.is_empty(),
        "JSON Schema fields missing from Rust DTO: {missing_in_rust:?}"
    );
    assert_eq!(
        properties["credential_registration"]["enum"],
        serde_json::json!(["direct", "mini_max_regional_api_key"]),
        "credential registration must remain a closed host primitive",
    );
}

/// Verifies the service-entry sub-fields match between schema and the schema's own declared contract.
/// 验证服务入口子字段在 Schema 声明契约内保持一致。
#[test]
fn manifest_schema_service_contract_is_consistent() {
    let schema = load_schema("provider-manifest-v2.schema.json");
    let services = &schema["properties"]["services"];
    assert_eq!(services["type"], "array");
    assert_eq!(services["maxItems"], 64);
    let item = &services["items"];
    let required = item["required"]
        .as_array()
        .expect("service items must declare required");
    for field in ["service_entry_id", "base_url", "protocol_routes"] {
        assert!(
            required.iter().any(|value| value == field),
            "service item required list must include `{field}`"
        );
    }
    let properties = item["properties"]
        .as_object()
        .expect("service items must declare properties");
    let schema_fields: BTreeSet<&str> = properties.keys().map(String::as_str).collect();
    assert_eq!(
        schema_fields,
        rust_service_entry_fields(),
        "service-entry JSON Schema fields must exactly match the Rust DTO"
    );
    assert_eq!(
        properties["request_projector"]["enum"],
        serde_json::json!(["direct", "compatible", "routed_compatible"]),
        "request-projector values must match RequestProjectorKind serialization"
    );
    let catalog_fields: BTreeSet<&str> = properties["model_catalog"]["properties"]
        .as_object()
        .expect("model catalog must declare properties")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        catalog_fields,
        rust_model_catalog_fields(),
        "model-catalog JSON Schema fields must exactly match the Rust DTO"
    );
    let credential_fields: BTreeSet<&str> =
        properties["credential_profiles"]["items"]["properties"]
            .as_object()
            .expect("credential profiles must declare properties")
            .keys()
            .map(String::as_str)
            .collect();
    assert_eq!(
        credential_fields,
        rust_credential_profile_fields(),
        "credential-profile JSON Schema fields must exactly match the Rust DTO"
    );
}

/// Verifies the committed JSON Schema keeps session headers exclusive to conversation entries.
/// 验证已提交 JSON Schema 将会话请求头限制在常规会话入口。
#[test]
fn manifest_schema_rejects_session_header_on_provider_tools() {
    let schema = load_schema("provider-manifest-v2.schema.json");
    let validator =
        jsonschema::draft202012::new(&schema).expect("provider manifest schema must compile");
    let manifest_path = schema_resource("provider-manifest-v2.schema.json")
        .parent()
        .expect("schema path must have a parent")
        .parent()
        .expect("schema directory must have a parent")
        .join("manifests")
        .join("tavily.json");
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|_| panic!("missing Tavily manifest at {}", manifest_path.display()));
    let mut manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("Tavily manifest must be valid JSON");
    // The fixture already contains a provider-tools entry, so this mutation isolates the schema rule.
    // 该 fixture 已包含 Provider Tools 入口，因此本变更只验证 Schema 规则本身。
    manifest["services"][0]["session_header"] =
        serde_json::Value::String(String::from("x-provider-session"));
    assert!(
        !validator.is_valid(&manifest),
        "provider-tools entries must not accept session_header"
    );
}

/// Verifies model-rule sub-fields match the Rust DTO exactly.
/// 验证模型规则子字段与 Rust DTO 精确一致。
#[test]
fn manifest_schema_model_rule_contract_is_consistent() {
    let schema = load_schema("provider-manifest-v2.schema.json");
    let properties = schema["properties"]["model_rules"]["items"]["properties"]
        .as_object()
        .expect("model-rule items must declare properties");
    let schema_fields: BTreeSet<&str> = properties.keys().map(String::as_str).collect();
    assert_eq!(
        schema_fields,
        rust_model_rule_fields(),
        "model-rule JSON Schema fields must exactly match the Rust DTO"
    );
}

/// Verifies omitted system-provider schema projection resolves to the lightweight first-version default.
/// 验证系统供应商省略 Schema 投影时解析为第一版轻量默认值。
#[test]
fn omitted_tool_schema_projection_defaults_to_lightweight() {
    let defaults: super::ModelDefaults = serde_json::from_value(serde_json::json!({
        "default_enabled": true
    }))
    .expect("omitted optional defaults must deserialize");

    assert_eq!(
        defaults.tool_schema_projection,
        super::ToolSchemaProjectionMode::Lightweight
    );
}

/// Verifies the committed schema rejects unknown fields at every model-parameter layer.
/// 验证已提交 Schema 在模型参数的每一层都拒绝未知字段。
#[test]
fn manifest_schema_closes_model_parameter_definitions() {
    let schema = load_schema("provider-manifest-v2.schema.json");
    jsonschema::draft202012::meta::validate(&schema)
        .expect("provider manifest schema must remain valid Draft 2020-12");
    let validator =
        jsonschema::draft202012::new(&schema).expect("provider manifest schema must compile");
    let manifest_path = schema_resource("provider-manifest-v2.schema.json")
        .parent()
        .expect("schema path must have a parent")
        .parent()
        .expect("schema directory must have a parent")
        .join("manifests")
        .join("minimax.json");
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|_| panic!("missing manifest resource at {}", manifest_path.display()));
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("MiniMax manifest must be valid JSON");
    assert!(
        validator.is_valid(&manifest),
        "the unchanged bundled MiniMax manifest must satisfy the committed schema"
    );

    for pointer in [
        "/model_rules/0/model_parameters/0",
        "/model_rules/0/model_parameters/0/schema",
        "/model_rules/0/model_parameters/0/default",
    ] {
        let mut invalid = manifest.clone();
        invalid
            .pointer_mut(pointer)
            .and_then(serde_json::Value::as_object_mut)
            .unwrap_or_else(|| panic!("test manifest must expose object at `{pointer}`"))
            .insert(String::from("unexpected"), serde_json::Value::Bool(true));
        assert!(
            !validator.is_valid(&invalid),
            "unknown field at `{pointer}` must be rejected by the committed schema"
        );
    }
}

/// One nested-contract probe case: the bundled document and the property trail that locates the
/// object to probe.
///
/// A trail never contains array positions. Regenerating a bundled manifest can insert, remove or
/// reorder entries, and a positional pointer would then probe a different object — or nothing at
/// all — with no contract change. Every segment selects the first object in the current subtree
/// that exposes the named property, so cases stay attached to document content.
type NestedContractCase = (&'static str, &'static [&'static str]);

/// Probe cases shared by the Rust-parser and JSON-Schema closure tests.
const NESTED_CONTRACT_CASES: &[NestedContractCase] = &[
    (
        "openai.json",
        &["services", "protocol_routes", "responses_features"],
    ),
    ("alibaba.json", &["usage_profiles", "payload"]),
    ("minimax.json", &["request_profiles", "payload", "steps"]),
    ("minimax.json", &["response_profiles", "payload", "steps"]),
    (
        "alibaba.json",
        &["services", "service_origin_override_policy"],
    ),
    ("alibaba.json", &["services", "native_inference_bindings"]),
    ("alibaba.json", &["tool_targets", "output"]),
    ("alibaba.json", &["tool_targets", "execution"]),
    (
        "alibaba.json",
        &["tool_targets", "output", "resource_decoders"],
    ),
];

/// One resolved location step; array positions are chosen by the resolver, never by a case.
#[derive(Clone)]
enum LocatedStep {
    Key(String),
    Index(usize),
}

/// Collects every descendant object exposing `key` in document order, with its path from `value`.
fn collect_holders_exposing_key<'a>(
    value: &'a serde_json::Value,
    key: &str,
    prefix: &[LocatedStep],
    found: &mut Vec<(Vec<LocatedStep>, &'a serde_json::Value)>,
) {
    match value {
        serde_json::Value::Object(map) => {
            if map.contains_key(key) {
                found.push((prefix.to_vec(), value));
            }
            for (name, child) in map {
                let mut next = prefix.to_vec();
                next.push(LocatedStep::Key(name.clone()));
                collect_holders_exposing_key(child, key, &next, found);
            }
        }
        serde_json::Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                let mut next = prefix.to_vec();
                next.push(LocatedStep::Index(index));
                collect_holders_exposing_key(item, key, &next, found);
            }
        }
        _ => {}
    }
}

/// Resolves one property trail, backtracking until the remaining trail also resolves.
///
/// Backtracking matters: an intermediate property can appear in several entries while only one of
/// them carries the nested contract, so the first candidate is not necessarily the right one.
/// A trail that ends at an array of objects resolves to that array's first object element.
fn locate_trail(value: &serde_json::Value, trail: &[&str]) -> Option<Vec<LocatedStep>> {
    let Some((key, rest)) = trail.split_first() else {
        return match value {
            serde_json::Value::Object(_) => Some(Vec::new()),
            serde_json::Value::Array(items) => items
                .iter()
                .position(|item| item.is_object())
                .map(|index| vec![LocatedStep::Index(index)]),
            _ => None,
        };
    };
    let mut holders = Vec::new();
    collect_holders_exposing_key(value, key, &[], &mut holders);
    for (mut path, holder) in holders {
        let Some(child) = holder.get(key) else {
            continue;
        };
        path.push(LocatedStep::Key(String::from(*key)));
        if let Some(mut suffix) = locate_trail(child, rest) {
            path.append(&mut suffix);
            return Some(path);
        }
    }
    None
}

/// Walks one previously located path from the document root.
fn walk_located_path<'a>(
    document: &'a mut serde_json::Value,
    path: &[LocatedStep],
) -> Option<&'a mut serde_json::Value> {
    let mut current = document;
    for step in path {
        current = match step {
            LocatedStep::Key(name) => current.get_mut(name.as_str())?,
            LocatedStep::Index(index) => current.get_mut(*index)?,
        };
    }
    Some(current)
}

/// Locates one nested contract object by property trail instead of by array position.
fn nested_contract_target<'a>(
    document: &'a mut serde_json::Value,
    trail: &[&str],
) -> Option<&'a mut serde_json::Value> {
    let path = locate_trail(document, trail)?;
    walk_located_path(document, &path)
}

/// Verifies trail resolution backtracks instead of committing to the first partial match.
///
/// The first service also exposes `protocol_routes`, but only the second one carries the nested
/// contract; a resolver that stops at the first candidate would look in the wrong route list. This
/// is the shape that made the previous positional case fail silently when bundled data changed.
#[test]
fn nested_contract_trail_resolution_backtracks() {
    let mut document = serde_json::json!({
        "services": [
            { "protocol_routes": [{ "protocol_profile_id": "without-contract" }] },
            { "protocol_routes": [{ "responses_features": {} }] },
        ]
    });
    {
        let target = nested_contract_target(
            &mut document,
            &["services", "protocol_routes", "responses_features"],
        )
        .expect("resolver must skip the route list that lacks the nested contract");
        assert_eq!(*target, serde_json::json!({}));
    }
    assert_eq!(
        nested_contract_target(&mut document, &["services", "absent_contract"]),
        None
    );
}

/// Verifies optional prompt-cache contracts close unknown fields without requiring catalog content.
/// 验证可选提示词缓存契约拒绝未知字段，且不依赖目录内容。
#[test]
fn optional_prompt_cache_contract_closes_unknown_fields() {
    // A synthetic cache declaration keeps this format probe independent of model additions/removals.
    // 合成缓存声明使此格式验证独立于模型增删。
    let mut cache = serde_json::json!({
        "provider_default_behavior": "probe",
        "supports_managed_explicit": false,
        "supported_ttl_options": []
    });
    // The same committed definition and imported Rust type must agree on both shapes.
    // 同一已提交定义与导入的 Rust 类型必须对两种形状保持一致。
    let schema = load_schema("provider-manifest-v2.schema.json");
    let cache_schema = serde_json::json!({
        "$defs": schema["$defs"],
        "$ref": "#/$defs/prompt_cache_capability"
    });
    let validator =
        jsonschema::draft202012::new(&cache_schema).expect("cache contract schema must compile");
    assert!(validator.is_valid(&cache));
    assert!(
        serde_json::from_value::<vulcan_ai_contract::PromptCacheCapability>(cache.clone()).is_ok()
    );
    cache["unexpected"] = serde_json::json!(true);
    assert!(!validator.is_valid(&cache));
    assert!(serde_json::from_value::<vulcan_ai_contract::PromptCacheCapability>(cache).is_err());
}

/// Verifies production manifest parsing rejects unknown fields in imported nested contracts.
/// 验证生产 Manifest 解析拒绝导入嵌套契约中的未知字段。
#[test]
fn manifest_parser_closes_imported_nested_contracts() {
    let manifests_directory = schema_resource("provider-manifest-v2.schema.json")
        .parent()
        .expect("schema path must have a parent")
        .parent()
        .expect("schema directory must have a parent")
        .join("manifests");

    for &(file_name, trail) in NESTED_CONTRACT_CASES {
        let manifest_path = manifests_directory.join(file_name);
        let manifest_text = std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|_| panic!("missing manifest resource at {}", manifest_path.display()));
        let mut manifest: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("bundled manifest must be valid JSON");
        nested_contract_target(&mut manifest, trail)
            .and_then(serde_json::Value::as_object_mut)
            .unwrap_or_else(|| {
                panic!(
                    "`{file_name}` no longer exposes a nested contract object at trail {trail:?}; \
                     the bundled document changed, so this expectation may be stale and must be \
                     re-verified against the current manifest"
                )
            })
            .insert(String::from("unexpected"), serde_json::Value::Bool(true));
        let invalid_text = serde_json::to_string(&manifest).expect("test document must serialize");
        assert!(
            super::compiler::compile_manifest_text(
                invalid_text.as_str(),
                super::ManifestOwnership::System,
            )
            .is_err(),
            "unknown nested field in `{file_name}` at trail {trail:?} must be rejected"
        );
    }
}

/// Verifies the committed JSON Schema closes the same imported nested contracts as Rust parsing.
/// 验证已提交 JSON Schema 与 Rust 解析一致地封闭导入嵌套契约。
#[test]
fn manifest_schema_closes_imported_nested_contracts() {
    let schema = load_schema("provider-manifest-v2.schema.json");
    jsonschema::draft202012::meta::validate(&schema)
        .expect("provider manifest schema must remain valid Draft 2020-12");
    let validator =
        jsonschema::draft202012::new(&schema).expect("provider manifest schema must compile");
    let manifests_directory = schema_resource("provider-manifest-v2.schema.json")
        .parent()
        .expect("schema path must have a parent")
        .parent()
        .expect("schema directory must have a parent")
        .join("manifests");

    for &(file_name, trail) in NESTED_CONTRACT_CASES {
        let manifest_path = manifests_directory.join(file_name);
        let manifest_text = std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|_| panic!("missing manifest resource at {}", manifest_path.display()));
        let mut manifest: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("bundled manifest must be valid JSON");
        assert!(
            validator.is_valid(&manifest),
            "unchanged bundled manifest `{file_name}` must satisfy the committed schema"
        );
        nested_contract_target(&mut manifest, trail)
            .and_then(serde_json::Value::as_object_mut)
            .unwrap_or_else(|| {
                panic!(
                    "`{file_name}` no longer exposes a nested contract object at trail {trail:?}; \
                     the bundled document changed, so this expectation may be stale and must be \
                     re-verified against the current manifest"
                )
            })
            .insert(String::from("unexpected"), serde_json::Value::Bool(true));
        assert!(
            !validator.is_valid(&manifest),
            "unknown nested field in `{file_name}` at trail {trail:?} must be rejected by the schema"
        );
    }
}

/// Verifies Rust parsing rejects every unknown object field rejected by the committed schema.
/// 验证 Rust 解析拒绝已提交 Schema 所拒绝的每一个未知对象字段。
#[test]
fn manifest_parser_matches_schema_unknown_field_closure() {
    let schema = load_schema("provider-manifest-v2.schema.json");
    let validator =
        jsonschema::draft202012::new(&schema).expect("provider manifest schema must compile");
    let manifests_directory = schema_resource("provider-manifest-v2.schema.json")
        .parent()
        .expect("schema path must have a parent")
        .parent()
        .expect("schema directory must have a parent")
        .join("manifests");
    let mut manifest_paths = std::fs::read_dir(&manifests_directory)
        .expect("bundled manifests directory must be readable")
        .map(|entry| {
            entry
                .expect("manifest directory entry must be readable")
                .path()
        })
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    manifest_paths.sort();
    let mut exercised_shapes = BTreeSet::new();

    for manifest_path in manifest_paths {
        let manifest_text = std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|_| panic!("missing manifest resource at {}", manifest_path.display()));
        let manifest: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("bundled manifest must be valid JSON");
        assert!(
            validator.is_valid(&manifest),
            "unchanged bundled manifest must satisfy the committed schema: {}\n{}",
            manifest_path.display(),
            validator
                .iter_errors(&manifest)
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
        let mut pointers = Vec::new();
        collect_json_object_pointers(&manifest, "", &mut pointers);
        for pointer in pointers {
            let shape = normalized_object_pointer(pointer.as_str());
            if !exercised_shapes.insert(shape) {
                continue;
            }
            let mut invalid = manifest.clone();
            invalid
                .pointer_mut(pointer.as_str())
                .and_then(serde_json::Value::as_object_mut)
                .unwrap_or_else(|| panic!("test manifest must expose object at `{pointer}`"))
                .insert(
                    String::from("__vulcan_unknown_field_test__"),
                    serde_json::Value::Bool(true),
                );
            if validator.is_valid(&invalid) {
                continue;
            }
            let invalid_text =
                serde_json::to_string(&invalid).expect("test document must serialize");
            assert!(
                super::compiler::compile_manifest_text(
                    invalid_text.as_str(),
                    super::ManifestOwnership::System,
                )
                .is_err(),
                "Rust parser accepted a field rejected by JSON Schema at `{}{pointer}`",
                manifest_path.display()
            );
        }
    }
}

/// Verifies malformed parameter domains and post-inheritance dependencies block compilation.
/// 验证畸形参数值域及继承后的依赖关系会阻止编译。
#[test]
fn manifest_compiler_rejects_invalid_model_parameter_contracts() {
    let manifest_path = schema_resource("provider-manifest-v2.schema.json")
        .parent()
        .expect("schema path must have a parent")
        .parent()
        .expect("schema directory must have a parent")
        .join("manifests")
        .join("deepseek.json");
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|_| panic!("missing manifest resource at {}", manifest_path.display()));
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("DeepSeek manifest must be valid JSON");

    let mut invalid_range = manifest.clone();
    invalid_range["model_rules"][0]["model_parameters"][0]["schema"]["minimum"] =
        serde_json::Value::String(String::from("3"));
    let invalid_range_text =
        serde_json::to_string(&invalid_range).expect("range probe must serialize");
    assert!(
        super::compiler::compile_manifest_text(
            invalid_range_text.as_str(),
            super::ManifestOwnership::System,
        )
        .is_err(),
        "an empty decimal domain must fail activation"
    );

    let mut missing_dependency = manifest;
    missing_dependency["model_rules"][0]["model_parameters"][0]["dependent_required"] =
        serde_json::json!(["missing.parameter"]);
    let missing_dependency_text =
        serde_json::to_string(&missing_dependency).expect("dependency probe must serialize");
    assert!(
        super::compiler::compile_manifest_text(
            missing_dependency_text.as_str(),
            super::ManifestOwnership::System,
        )
        .is_err(),
        "an unresolved post-inheritance dependency must fail activation"
    );
}

/// Verifies capability-patch round trips preserve omitted versus explicit-null semantics.
/// 验证能力补丁往返序列化保留省略与显式 null 语义。
#[test]
fn capability_patch_serialization_preserves_presence_semantics() {
    let omitted = super::ManifestCapabilityPatch::default();
    assert_eq!(
        serde_json::to_value(&omitted).expect("omitted patch must serialize"),
        serde_json::json!({}),
        "omitted fields must not become invalid serialized null values"
    );

    let explicit_null: super::ManifestCapabilityPatch =
        serde_json::from_value(serde_json::json!({ "context_limit": null }))
            .expect("nullable capability must parse");
    assert_eq!(explicit_null.context_limit, Some(None));
    assert_eq!(
        serde_json::to_value(explicit_null).expect("explicit null must serialize"),
        serde_json::json!({ "context_limit": null })
    );
}

/// Verifies the index schema stays parseable with its frozen contract.
/// 验证 Index Schema 保持可解析并符合冻结契约。
#[test]
fn index_schema_contract_is_stable() {
    let schema = load_schema("provider-index-v1.schema.json");
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        "vulcan.provider-index.v1"
    );
    assert_eq!(schema["properties"]["providers"]["type"], "object");
    assert_eq!(schema["properties"]["providers"]["maxProperties"], 256);
    assert_eq!(
        schema["properties"]["providers"]["propertyNames"]["maxLength"],
        super::MAX_PROVIDER_IDENTIFIER_LEN
    );
    let providers = &schema["properties"]["providers"]["additionalProperties"];
    let required = providers["required"]
        .as_array()
        .expect("index provider items must declare required");
    for field in [
        "provider_id",
        "revision",
        "relative_url",
        "sha256",
        "bytes",
        "minimum_engine_contract",
    ] {
        assert!(
            required.iter().any(|value| value == field),
            "index provider required list must include `{field}`"
        );
    }
    assert_eq!(
        providers["properties"]["sha256"]["pattern"],
        "^[0-9a-f]{64}$"
    );
    assert_eq!(
        providers["properties"]["provider_id"]["maxLength"],
        super::MAX_PROVIDER_IDENTIFIER_LEN
    );
    assert_eq!(
        providers["properties"]["provider_id"]["pattern"],
        "^[A-Za-z0-9._-]+$"
    );
    assert_eq!(schema["properties"]["issued_at_unix_ms"]["type"], "integer");
    assert_eq!(
        schema["properties"]["expires_at_unix_ms"]["type"],
        "integer"
    );
}

/// Verifies the schema fixture validates the frozen example manifest shape.
/// 验证 Schema fixture 能接受冻结的示例清单形态。
#[test]
fn manifest_schema_accepts_minimal_shape() {
    let schema = load_schema("provider-manifest-v2.schema.json");
    let properties = schema["properties"]
        .as_object()
        .expect("schema must declare properties");
    // A minimal document carries every required top-level property name; structural
    // deep validation is owned by the Rust validator, not duplicated in this test.
    // 最小文档携带全部必填顶层属性名；深度结构验证由 Rust 验证器负责，不在本测试重复。
    for field in [
        "schema_version",
        "provider_id",
        "revision",
        "published_at",
        "engine_contract",
        "display",
        "services",
        "model_defaults",
        "provenance",
    ] {
        assert!(
            properties.contains_key(field),
            "schema must declare required property `{field}`"
        );
    }
    assert_eq!(
        properties["provider_id"]["maxLength"],
        super::MAX_PROVIDER_IDENTIFIER_LEN
    );
    assert_eq!(properties["provider_id"]["pattern"], "^[A-Za-z0-9._:+-]+$");
}

/// DeepSeek manifest fixture mirroring the code-owned descriptor shape.
/// 镜像代码持有描述形态的 DeepSeek 清单 fixture。
const DEEPSEEK_MANIFEST: &str = r#"{
  "schema_version": "vulcan.provider-manifest.v2",
  "provider_id": "deepseek",
  "revision": 1,
  "published_at": "2026-08-06T00:00:00Z",
  "engine_contract": {
    "minimum_manifest_engine": 1,
    "required_protocol_profiles": [
      "openai-chat-completions.v1",
      "anthropic-messages.v1",
      "openai-responses.v1"
    ]
  },
  "display": {
    "name": "DeepSeek",
    "category": "official",
    "credential_directory_visibility": "configured_only"
  },
  "services": [
    {
      "service_entry_id": "official-api",
      "display_name": "DeepSeek API",
      "base_url": "https://api.deepseek.com",
      "request_projector": "compatible",
      "credential_profiles": [
        {
          "credential_profile_id": "api-key",
          "display_name": "API Key",
          "credential_mode": "token",
          "billing_kind": "pay_as_you_go",
          "account_usage_query": "deep_seek_balance",
          "key_prefix": "sk-",
          "default_usage_scopes": [
            "interactive_agent",
            "internal_background",
            "external_service"
          ]
        }
      ],
      "default_authentication": "bearer_authorization",
      "model_catalog": {
        "source_policy": "manifest_and_discovery",
        "discovery_kind": "open_ai",
        "path": "/models"
      },
      "protocol_routes": [
        {
          "protocol_profile_id": "openai-chat-completions.v1",
          "endpoint_path": "/chat/completions",
          "verified": true
        },
        {
          "protocol_profile_id": "anthropic-messages.v1",
          "endpoint_path": "/anthropic/v1/messages",
          "authentication": "x_api_key",
          "request_headers": {
            "anthropic-version": "2023-06-01"
          },
          "verified": true
        },
        {
          "protocol_profile_id": "openai-responses.v1",
          "endpoint_path": "/responses",
          "verified": true
        }
      ]
    }
  ],
  "model_defaults": {
    "default_enabled": true,
    "unknown_model_policy": "declared_rule_required",
    "supported_protocols": ["openai-responses.v1"],
    "selected_protocol": "openai-responses.v1"
  },
  "model_rules": [
    {
      "rule_id": "flash",
      "matcher": {"kind": "prefix", "value": "deepseek-v4-flash"},
      "supported_protocols": ["openai-responses.v1"],
      "selected_protocol": "openai-responses.v1"
    },
    {
      "rule_id": "pro",
      "matcher": {"kind": "prefix", "value": "deepseek-v4-pro"},
      "supported_protocols": ["anthropic-messages.v1"],
      "selected_protocol": "anthropic-messages.v1"
    }
  ],
  "provenance": {
    "status": "verified",
    "sources": [{"url": "https://api-docs.deepseek.com", "note": "official docs"}]
  }
}"#;

/// Verifies rule instances inherit provider defaults and expand per service entry.
/// 验证规则实例继承供应商默认并按服务入口展开。
#[test]
fn rule_instances_inherit_defaults_and_expand_per_entry() {
    let compiled =
        super::compiler::compile_manifest_text(DEEPSEEK_MANIFEST, super::ManifestOwnership::System)
            .expect("deepseek manifest must compile");
    assert_eq!(compiled.rule_instances.len(), 2);
    let flash = compiled
        .rule_instances
        .iter()
        .find(|instance| instance.rule_id == "flash")
        .expect("flash rule must exist");
    assert_eq!(flash.service_entry_id, "official-api");
    assert_eq!(
        flash.selected_protocol.as_deref(),
        Some("openai-responses.v1")
    );
    assert!(flash.default_enabled);
    assert_eq!(flash.provenance.manifest_revision, 1);
}

/// Verifies profile and operation reference collections preserve omission versus explicit-empty
/// presence through deserialization and inheritance compilation.
/// 验证 Profile 与操作引用集合在反序列化和继承编译中保留省略与显式空列表的 presence 差异。
#[test]
fn rule_reference_collections_distinguish_inherit_from_explicit_clear() {
    // manifestValue starts from a validated structural fixture and injects provider defaults plus
    // one explicit-empty rule without requiring executable declaration payloads for this pure
    // inheritance test.
    // manifestValue 从已验证结构 fixture 起步，注入供应商默认与一条显式空规则；本测试只验证
    // 继承，因此无需可执行声明载荷。
    let mut manifest_value: serde_json::Value =
        serde_json::from_str(DEEPSEEK_MANIFEST).expect("fixture must parse as JSON");
    manifest_value["model_defaults"]["default_usage_profile_refs"] =
        serde_json::json!(["provider.usage.v1"]);
    manifest_value["model_defaults"]["default_request_profile_refs"] =
        serde_json::json!(["provider.request.v1"]);
    manifest_value["model_defaults"]["default_response_profile_refs"] =
        serde_json::json!(["provider.response.v1"]);
    manifest_value["model_defaults"]["default_operation_refs"] =
        serde_json::json!(["provider.operation.v1"]);
    // flashRule explicitly clears every inherited collection; the pro rule remains omitted.
    // flashRule 显式清除全部继承集合；pro 规则继续保持字段省略。
    let flash_rule = manifest_value["model_rules"][0]
        .as_object_mut()
        .expect("flash rule must be an object");
    for field in [
        "usage_profile_refs",
        "request_profile_refs",
        "response_profile_refs",
        "operation_refs",
    ] {
        flash_rule.insert(field.to_string(), serde_json::json!([]));
    }
    // manifest preserves JSON field presence and instances freeze the resulting winners.
    // manifest 保留 JSON 字段 presence，instances 冻结最终胜者。
    let manifest: super::ProviderManifestV1 =
        serde_json::from_value(manifest_value).expect("presence fixture must deserialize");
    let instances = super::inheritance::resolve_rule_instances(&manifest);
    let cleared = instances
        .iter()
        .find(|instance| instance.rule_id == "flash")
        .expect("explicit-clear rule must resolve");
    let inherited = instances
        .iter()
        .find(|instance| instance.rule_id == "pro")
        .expect("inheriting rule must resolve");
    for (field, cleared_refs, inherited_refs) in [
        (
            "usage_profile_refs",
            cleared.usage_profile_refs.as_slice(),
            inherited.usage_profile_refs.as_slice(),
        ),
        (
            "request_profile_refs",
            cleared.request_profile_refs.as_slice(),
            inherited.request_profile_refs.as_slice(),
        ),
        (
            "response_profile_refs",
            cleared.response_profile_refs.as_slice(),
            inherited.response_profile_refs.as_slice(),
        ),
        (
            "operation_refs",
            cleared.operation_refs.as_slice(),
            inherited.operation_refs.as_slice(),
        ),
    ] {
        assert!(
            cleared_refs.is_empty(),
            "`{field}` must be explicitly cleared"
        );
        assert_eq!(inherited_refs.len(), 1, "`{field}` must inherit defaults");
        assert_eq!(
            cleared.field_provenance[field].source,
            super::FieldSourceKind::ModelRulePrefix,
            "the explicit empty list must remain rule-owned"
        );
        assert_eq!(
            inherited.field_provenance[field].source,
            super::FieldSourceKind::ProviderDefaults,
            "the omitted field must remain provider-default-owned"
        );
    }
}

/// Verifies rule resolution picks exact over prefix and longest prefix over shorter.
/// 验证规则解析优先精确、再最长前缀。
#[test]
fn rule_for_model_picks_most_specific() {
    let compiled =
        super::compiler::compile_manifest_text(DEEPSEEK_MANIFEST, super::ManifestOwnership::System)
            .expect("deepseek manifest must compile");
    assert_eq!(
        compiled
            .rule_for_model("official-api", None, "deepseek-v4-flash-0713")
            .map(|rule| rule.rule_id.as_str()),
        Some("flash")
    );
    assert_eq!(
        compiled
            .rule_for_model("official-api", None, "deepseek-v4-pro")
            .map(|rule| rule.rule_id.as_str()),
        Some("pro")
    );
    assert_eq!(
        compiled
            .rule_for_model("official-api", None, "deepseek-chat")
            .map(|rule| rule.rule_id.as_str()),
        None,
        "unmatched model must have no rule instance"
    );
}

/// Verifies capability templates merge field-by-field with fixed presence semantics:
/// omitted list fields inherit while explicitly empty list fields clear the provider base.
/// 验证能力模板按固定 presence 语义逐字段合并：省略列表字段继承，显式空列表字段清除
/// 供应商基座。
#[test]
fn capability_templates_merge_with_presence_semantics() {
    let manifest: super::ProviderManifestV1 = serde_json::from_str(
        r#"{
          "schema_version": "vulcan.provider-manifest.v2",
          "provider_id": "probe-merge",
          "revision": 1,
          "published_at": "2026-08-07T00:00:00Z",
          "engine_contract": { "minimum_manifest_engine": 1, "required_protocol_profiles": ["openai-chat-completions.v1", "openai-responses.v1"] },
          "display": { "name": "Probe Merge", "category": "official" },
          "services": [
            {
              "service_entry_id": "official-api",
              "display_name": "Official API",
              "kind": "conversation",
              "base_url": "https://api.probe.example.com",
              "credential_profiles": [
                {
                  "credential_profile_id": "api-key",
                  "display_name": "API Key",
                  "credential_mode": "token"
                }
              ],
              "default_authentication": "bearer_authorization",
              "model_catalog": { "source_policy": "manual_only", "discovery_kind": "manual" },
              "protocol_routes": [
                {
                  "protocol_profile_id": "openai-chat-completions.v1",
                  "endpoint_path": "/v1/chat/completions",
                  "verified": true
                }
              ]
            }
          ],
          "model_defaults": {
            "default_enabled": true,
            "catalog_membership_policy": "declared_only",
            "unknown_model_policy": "declared_rule_required",
            "tool_schema_projection": "strict",
            "capabilities": {
              "operations": ["conversation_respond"],
              "provider_native_tools": ["web_search"],
              "reasoning": "native",
              "prompt_cache": {"provider_default_behavior": "probe", "supports_managed_explicit": false, "supported_ttl_options": []},
              "evidence_revision": "defaults.v1",
              "reasoning_efforts": ["none", "medium"]
            }
          },
          "model_rules": [
            {
              "rule_id": "partial-overlay",
              "matcher": { "kind": "exact", "value": "merge/model" },
              "selected_protocol": "openai-chat-completions.v1",
              "capabilities": {
                "operations": ["conversation_respond", "embedding_create"],
                "prompt_cache": {"provider_default_behavior": "probe", "supports_managed_explicit": false, "supported_ttl_options": []},
                "evidence_revision": "rule.v1"
              }
            },
            {
              "rule_id": "tools-cleared",
              "matcher": { "kind": "exact", "value": "cleared/model" },
              "selected_protocol": "openai-chat-completions.v1",
              "tool_schema_projection": "passthrough",
              "capabilities": {
                "operations": [],
                "provider_native_tools": [],
                "prompt_cache": {"provider_default_behavior": "probe", "supports_managed_explicit": false, "supported_ttl_options": []},
                "evidence_revision": "rule.v1"
              }
            }
          ],
          "provenance": { "status": "migrated", "sources": [] }
        }"#,
    )
    .expect("merge probe must parse");
    let compiled = super::compiler::compile_manifest(&manifest, super::ManifestOwnership::System)
        .expect("merge probe must compile");

    // The partial overlay replaces its declared operations list and inherits omitted list fields
    // from the provider default base with real presence semantics (plan §6.4).
    // 部分覆盖替换其声明的 operations 列表，并以真实 presence 语义（方案 §6.4）
    // 从供应商默认基座继承省略的列表字段。
    let partial = compiled
        .rule_for_model("official-api", Some("api-key"), "merge/model")
        .expect("partial overlay rule must match");
    let capabilities = partial.capabilities.as_ref().expect("merged capabilities");
    assert_eq!(
        partial.tool_schema_projection,
        super::ToolSchemaProjectionMode::Strict,
        "an omitted rule field must inherit the provider default"
    );
    assert!(
        capabilities
            .operations
            .contains(&vulcan_ai_contract::AiOperation::EmbeddingCreate),
        "the rule overlay must replace the operations list"
    );
    assert_eq!(
        capabilities.provider_native_tools,
        vec![vulcan_ai_contract::NativeToolKind::WebSearch],
        "an undeclared list field must inherit the provider default base"
    );
    let provenance = &partial.capability_provenance;
    assert_eq!(
        provenance["operations"].rule_id.as_deref(),
        Some("partial-overlay"),
        "the rule must own its declared operations field"
    );
    // The explicit empty tools list is the authoritative no-tools contract.
    // 显式空工具列表是权威的无工具契约。
    let cleared = compiled
        .rule_for_model("official-api", Some("api-key"), "cleared/model")
        .expect("tools-cleared rule must match");
    assert_eq!(
        cleared.tool_schema_projection,
        super::ToolSchemaProjectionMode::Passthrough,
        "an explicit rule field must replace the provider default"
    );
    assert!(
        cleared
            .capabilities
            .as_ref()
            .expect("cleared capabilities")
            .provider_native_tools
            .is_empty(),
        "an explicit empty tools list must clear the base tools"
    );
}

/// Verifies manifest provider identities fail before exceeding safe persisted filenames.
/// 验证 Manifest 供应商身份在超过安全持久化文件名边界前失败。
#[test]
fn manifest_validator_rejects_filesystem_unsafe_provider_identity_length() {
    // document starts from one otherwise valid system fixture so the identity error is factual.
    // document 从一份其他字段均有效的系统夹具开始，使身份错误具有唯一事实来源。
    let mut document: serde_json::Value =
        serde_json::from_str(DEEPSEEK_MANIFEST).expect("fixture must parse");
    document["provider_id"] =
        serde_json::Value::String("a".repeat(super::MAX_PROVIDER_IDENTIFIER_LEN + 1));
    let text = serde_json::to_string(&document).expect("fixture must serialize");
    let errors = super::parse_and_validate(
        text.as_str(),
        super::ManifestOwnership::System,
        &EnginePrimitiveRegistry::current_protocol_baseline(),
    )
    .expect_err("oversized provider identity must fail validation");
    assert!(
        errors
            .iter()
            .any(|error| { error.code == "identifier_too_long" && error.path == "provider_id" })
    );
}

/// Verifies text-compiled snapshot provenance carries the content-addressed digest.
/// 验证文本编译的快照来源携带内容寻址摘要。
#[test]
fn compile_manifest_text_records_snapshot_provenance() {
    let compiled =
        super::compiler::compile_manifest_text(DEEPSEEK_MANIFEST, super::ManifestOwnership::System)
            .expect("deepseek manifest must compile");
    use sha2::Digest;
    let expected = hex::encode(sha2::Sha256::digest(DEEPSEEK_MANIFEST.as_bytes()));
    assert_eq!(
        compiled.snapshot_provenance.sha256.as_deref(),
        Some(expected.as_str()),
        "snapshot provenance must pin the exact source digest"
    );
    assert_eq!(compiled.snapshot_provenance.revision, compiled.revision);
}

/// Verifies non-capability winners carry field-level manifest provenance.
/// 验证非能力最终胜者携带字段级 Manifest 来源。
#[test]
fn resolved_non_capability_fields_record_winner_provenance() {
    let compiled =
        super::compiler::compile_manifest_text(DEEPSEEK_MANIFEST, super::ManifestOwnership::System)
            .expect("deepseek manifest must compile");
    let rule = compiled
        .rule_instances
        .first()
        .expect("deepseek must expose one resolved rule");

    for field in [
        "supported_protocols",
        "selected_protocol",
        "default_enabled",
        "usage_profile_refs",
        "request_profile_refs",
        "response_profile_refs",
        "operation_refs",
    ] {
        let provenance = rule
            .field_provenance
            .get(field)
            .unwrap_or_else(|| panic!("missing provenance for {field}"));
        assert_eq!(provenance.manifest_revision, compiled.revision);
    }
}

/// Verifies an explicit rule winner retains the exact lower-precedence provider origin.
/// 验证显式规则胜者保留精确的低优先级供应商来源。
#[test]
fn resolved_field_provenance_retains_overridden_origins() {
    let compiled =
        super::compiler::compile_manifest_text(DEEPSEEK_MANIFEST, super::ManifestOwnership::System)
            .expect("overridden manifest must compile");
    let provenance = compiled.rule_instances[0]
        .field_provenance
        .get("supported_protocols")
        .expect("supported protocol provenance");
    assert_eq!(provenance.overridden_sources.len(), 1);
    assert_eq!(
        provenance.overridden_sources[0].source,
        super::FieldSourceKind::ProviderDefaults
    );
}

/// Verifies executable declarations carry provider, revision, source layer, and source digest.
/// 验证可执行声明携带供应商、修订号、来源层与源摘要。
#[test]
fn executable_declarations_record_complete_manifest_provenance() {
    let path =
        vulcan_runtime_assets::source_bundled_provider_manifests_directory().join("tavily.json");
    let text = std::fs::read_to_string(path).expect("tavily manifest");
    let compiled =
        super::compiler::compile_manifest_text(text.as_str(), super::ManifestOwnership::System)
            .expect("tavily manifest must compile");
    for key in [
        "operation.tavily.tavily_search",
        "tool_target.public-api.search",
    ] {
        let provenance = compiled
            .declaration_provenance
            .get(key)
            .unwrap_or_else(|| panic!("missing declaration provenance `{key}`"));
        assert_eq!(provenance.provider_id, "tavily");
        assert_eq!(provenance.manifest_revision, compiled.revision);
        assert_eq!(provenance.source_layer, super::ManifestSourceLayer::Bundled);
        assert!(provenance.sha256.is_some());
    }
}

/// Verifies immutable target templates expand into executable targets with complete provenance.
/// 验证不可变目标模板会展开为带完整来源的可执行目标。
#[test]
fn tool_target_templates_compile_into_provenanced_targets() {
    let path =
        vulcan_runtime_assets::source_bundled_provider_manifests_directory().join("alibaba.json");
    let text = std::fs::read_to_string(path).expect("Alibaba manifest");
    let compiled =
        super::compiler::compile_manifest_text(text.as_str(), super::ManifestOwnership::System)
            .expect("Alibaba manifest must compile");
    let instance = compiled
        .tool_target_template_instances
        .values()
        .find(|instance| {
            instance.service_entry_id == "model-studio-cn"
                && instance.model_id == "qwen-image-2.0-pro"
        })
        .expect("the bundled image model must instantiate its reviewed template");
    assert_eq!(
        instance.template_id,
        "alibaba.model-studio-cn.qwen-image-generate.v1"
    );
    let target = compiled
        .tool_targets
        .iter()
        .find(|target| target.target_id == instance.target_id)
        .expect("the template instance must compile into one concrete target");
    assert_eq!(target.upstream_target_id, "qwen-image-2.0-pro");
    assert_eq!(target.service_entry_id, "model-studio-cn");
    let operation = compiled
        .operations
        .iter()
        .find(|operation| {
            operation
                .tool_target_template_refs
                .contains(&instance.template_id)
        })
        .expect("one operation must own the reviewed template");
    assert!(
        operation.tool_target_refs.contains(&instance.target_id),
        "the owning operation must receive the generated concrete target"
    );
    for key in [
        format!("tool_target_template.{}", instance.template_id),
        format!("tool_target_template_instance.{}", instance.target_id),
    ] {
        let provenance = compiled
            .declaration_provenance
            .get(&key)
            .unwrap_or_else(|| panic!("missing declaration provenance `{key}`"));
        assert_eq!(provenance.provider_id, "alibaba");
        assert_eq!(provenance.manifest_revision, compiled.revision);
        assert_eq!(provenance.source_layer, super::ManifestSourceLayer::Bundled);
        assert!(provenance.sha256.is_some());
    }
}

/// Verifies every bundled rule and executable reference has a complete provenance closure.
/// 验证每条内置规则与可执行引用都具备完整来源闭包。
#[test]
fn bundled_manifest_provenance_closure_is_complete() {
    let manifests = super::compiler::load_bundled_resolved_manifests();
    for (provider_id, manifest) in manifests {
        for rule in &manifest.rule_instances {
            for field in [
                "supported_protocols",
                "default_enabled",
                "usage_profile_refs",
                "request_profile_refs",
                "response_profile_refs",
                "operation_refs",
            ] {
                assert!(
                    rule.field_provenance.contains_key(field),
                    "provider `{provider_id}` rule `{}` is missing `{field}` provenance",
                    rule.rule_id
                );
            }
            if rule.selected_protocol.is_some() {
                assert!(
                    rule.field_provenance.contains_key("selected_protocol"),
                    "provider `{provider_id}` rule `{}` is missing selected protocol provenance",
                    rule.rule_id
                );
            }
            if !rule.model_parameters.is_empty() {
                assert!(
                    rule.field_provenance.contains_key("model_parameters"),
                    "provider `{provider_id}` rule `{}` is missing model parameter provenance",
                    rule.rule_id
                );
            }
            if !rule.input_resource_constraints.is_empty() {
                assert!(
                    rule.field_provenance
                        .contains_key("input_resource_constraints"),
                    "provider `{provider_id}` rule `{}` is missing resource constraint provenance",
                    rule.rule_id
                );
            }
            for provenance in rule
                .field_provenance
                .values()
                .chain(rule.capability_provenance.values())
            {
                assert_eq!(
                    provenance.manifest_revision, manifest.revision,
                    "provider `{provider_id}` rule `{}` contains a stale provenance revision",
                    rule.rule_id
                );
            }
            for (kind, references) in [
                ("usage_profile", rule.usage_profile_refs.as_slice()),
                ("request_profile", rule.request_profile_refs.as_slice()),
                ("response_profile", rule.response_profile_refs.as_slice()),
                ("operation", rule.operation_refs.as_slice()),
            ] {
                for reference in references {
                    let key = format!("{kind}.{reference}");
                    assert!(
                        manifest.declaration_provenance.contains_key(&key),
                        "provider `{provider_id}` rule `{}` references `{key}` without provenance",
                        rule.rule_id
                    );
                }
            }
        }
        for operation in &manifest.operations {
            let operation_key = format!("operation.{}", operation.operation_id);
            assert!(
                manifest.declaration_provenance.contains_key(&operation_key),
                "provider `{provider_id}` operation `{}` has no provenance",
                operation.operation_id
            );
            for upload_channel_id in &operation.upload_channel_refs {
                assert!(
                    manifest
                        .declaration_provenance
                        .contains_key(&format!("upload_channel.{upload_channel_id}")),
                    "provider `{provider_id}` operation `{}` upload `{upload_channel_id}` has no provenance",
                    operation.operation_id
                );
            }
            for target_id in &operation.tool_target_refs {
                assert!(
                    manifest
                        .declaration_provenance
                        .contains_key(&format!("tool_target.{target_id}")),
                    "provider `{provider_id}` operation `{}` target `{target_id}` has no provenance",
                    operation.operation_id
                );
            }
        }
    }
}

/// Verifies a provider with two entries expands unscoped rules to every entry.
/// 验证双入口供应商将无作用域规则展开到每个入口。
#[test]
fn unscoped_rules_expand_to_every_entry() {
    let mut manifest: super::ProviderManifestV1 =
        serde_json::from_str(DEEPSEEK_MANIFEST).expect("deepseek manifest must parse");
    manifest.services.push(
        serde_json::from_str(
            r#"{
              "service_entry_id": "second-api",
              "display_name": "Second API",
              "base_url": "https://api.deepseek.com/v2",
              "credential_profiles": [
                {
                  "credential_profile_id": "api-key",
                  "display_name": "API Key",
                  "credential_mode": "token"
                }
              ],
              "protocol_routes": [
                {
                  "protocol_profile_id": "openai-responses.v1",
                  "endpoint_path": "/responses",
                  "verified": true
                }
              ]
            }"#,
        )
        .expect("second entry must parse"),
    );
    let instances = super::inheritance::resolve_rule_instances(&manifest);
    assert_eq!(
        instances.len(),
        4,
        "two rules must expand to two entries each"
    );
    for instance in &instances {
        assert!(
            matches!(
                instance.service_entry_id.as_str(),
                "official-api" | "second-api"
            ),
            "instances must cover every service entry"
        );
    }
}

/// R0/P0-01: strict subset scopes coexist and win, independent of JSON array order.
/// R0/P0-01：严格子集作用域共存且胜出，且与 JSON 数组顺序无关。
///
/// Business invariant (plan §6.5): under one matcher rank the strict subset of a target
/// set always wins; intersecting but incomparable scopes are rejected; the JSON rule
/// array order is never a tie-breaker.
/// 业务不变量（方案 §6.5）：同一匹配器排名下目标集合的严格子集始终胜出；相交但
/// 不可比较的作用域被拒绝；JSON 规则数组顺序绝不充当决胜依据。
#[test]
fn rule_scope_strict_subset_wins_regardless_of_json_order() {
    let manifest_text = r#"{
          "schema_version": "vulcan.provider-manifest.v2",
          "provider_id": "probe-scope",
          "revision": 1,
          "published_at": "2026-08-07T00:00:00Z",
          "engine_contract": { "minimum_manifest_engine": 1, "required_protocol_profiles": ["openai-chat-completions.v1", "openai-responses.v1"] },
          "display": { "name": "Probe Scope", "category": "official" },
          "services": [
            {
              "service_entry_id": "official-api",
              "display_name": "Official API",
              "kind": "conversation",
              "base_url": "https://api.probe.example.com",
              "credential_profiles": [
                { "credential_profile_id": "p1", "display_name": "P1", "credential_mode": "token" },
                { "credential_profile_id": "p2", "display_name": "P2", "credential_mode": "token" }
              ],
              "default_authentication": "bearer_authorization",
              "model_catalog": { "source_policy": "manual_only", "discovery_kind": "manual" },
              "protocol_routes": [
                {
                  "protocol_profile_id": "openai-chat-completions.v1",
                  "endpoint_path": "/v1/chat/completions",
                  "verified": true
                },
                {
                  "protocol_profile_id": "openai-responses.v1",
                  "endpoint_path": "/v1/responses",
                  "verified": true
                }
              ]
            }
          ],
          "model_defaults": {
            "default_enabled": true,
            "catalog_membership_policy": "declared_only",
            "unknown_model_policy": "declared_rule_required"
          },
          "model_rules": [
            {
              "rule_id": "wide",
              "scope": { "credential_profile_ids": ["p1", "p2"] },
              "matcher": { "kind": "exact", "value": "m1" },
              "selected_protocol": "openai-chat-completions.v1",
              "default_enabled": false
            },
            {
              "rule_id": "narrow",
              "scope": { "credential_profile_ids": ["p1"] },
              "matcher": { "kind": "exact", "value": "m1" },
              "selected_protocol": "openai-responses.v1",
              "default_enabled": true
            }
          ],
          "provenance": { "status": "migrated", "sources": [] }
        }"#;
    let compiled =
        super::compiler::compile_manifest_text(manifest_text, super::ManifestOwnership::System)
            .expect("overlapping subset scopes must compile");
    // Under p1 the strict subset rule wins; under p2 only the wide rule applies.
    // p1 下严格子集规则胜出；p2 下只有宽规则适用。
    let p1 = compiled
        .rule_for_model("official-api", Some("p1"), "m1")
        .expect("m1 under p1 must match");
    assert_eq!(p1.rule_id, "narrow", "the strict subset scope must win");
    assert_eq!(p1.selected_protocol.as_deref(), Some("openai-responses.v1"));
    assert!(super::compiler::manifest_default_enabled(
        &compiled,
        "official-api",
        Some("p1"),
        "m1"
    ));
    let p2 = compiled
        .rule_for_model("official-api", Some("p2"), "m1")
        .expect("m1 under p2 must match");
    assert_eq!(p2.rule_id, "wide");
    assert!(!super::compiler::manifest_default_enabled(
        &compiled,
        "official-api",
        Some("p2"),
        "m1"
    ));
    // Swapping the JSON rule array order must produce the identical resolution.
    // 交换 JSON 规则数组顺序必须产生完全相同的解析结果。
    let mut swapped: serde_json::Value =
        serde_json::from_str(manifest_text).expect("manifest must parse");
    let rules = swapped["model_rules"]
        .as_array_mut()
        .expect("model_rules must be an array");
    rules.reverse();
    let swapped_text = serde_json::to_string(&swapped).expect("manifest must serialize");
    let swapped_compiled =
        super::compiler::compile_manifest_text(&swapped_text, super::ManifestOwnership::System)
            .expect("swapped manifest must compile");
    let swapped_p1 = swapped_compiled
        .rule_for_model("official-api", Some("p1"), "m1")
        .expect("m1 under p1 must match after swap");
    assert_eq!(
        swapped_p1.rule_id, "narrow",
        "JSON array order must never be a tie-breaker"
    );
}

/// R0/P0-01: identical target sets under one matcher rank are duplicate authority.
/// R0/P0-01：同一匹配器排名下完全相同的目标集合属于重复权威。
#[test]
fn rule_scope_identical_targets_are_duplicate_authority() {
    let manifest_text = r#"{
          "schema_version": "vulcan.provider-manifest.v2",
          "provider_id": "probe-incomparable",
          "revision": 1,
          "published_at": "2026-08-07T00:00:00Z",
          "engine_contract": { "minimum_manifest_engine": 1, "required_protocol_profiles": ["openai-chat-completions.v1", "openai-responses.v1"] },
          "display": { "name": "Probe Incomparable", "category": "official" },
          "services": [
            {
              "service_entry_id": "official-api",
              "display_name": "Official API",
              "kind": "conversation",
              "base_url": "https://api.probe.example.com",
              "credential_profiles": [
                { "credential_profile_id": "p1", "display_name": "P1", "credential_mode": "token" },
                { "credential_profile_id": "p2", "display_name": "P2", "credential_mode": "token" }
              ],
              "default_authentication": "bearer_authorization",
              "model_catalog": { "source_policy": "manual_only", "discovery_kind": "manual" },
              "protocol_routes": [
                {
                  "protocol_profile_id": "openai-chat-completions.v1",
                  "endpoint_path": "/v1/chat/completions",
                  "verified": true
                }
              ]
            }
          ],
          "model_defaults": {
            "default_enabled": true,
            "catalog_membership_policy": "declared_only",
            "unknown_model_policy": "declared_rule_required"
          },
          "model_rules": [
            {
              "rule_id": "left",
              "scope": { "credential_profile_ids": ["p1", "p2"] },
              "matcher": { "kind": "exact", "value": "m1" },
              "selected_protocol": "openai-chat-completions.v1"
            },
            {
              "rule_id": "right",
              "scope": { "credential_profile_ids": ["p2", "p1"] },
              "matcher": { "kind": "exact", "value": "m1" },
              "selected_protocol": "openai-chat-completions.v1"
            }
          ],
          "provenance": { "status": "migrated", "sources": [] }
        }"#;
    let error =
        super::compiler::compile_manifest_text(manifest_text, super::ManifestOwnership::System)
            .expect_err("identical target sets must be rejected as duplicate authority");
    assert!(
        !error.message.is_empty(),
        "duplicate authority must produce a real validation error"
    );
}

/// R0/P1-04: a rule that omits a list field must inherit the provider default, not
/// clear it through the empty-value default (plan §6.4).
/// R0/P1-04：省略列表字段的规则必须继承供应商默认，而不是借空值默认清除（方案 §6.4）。
#[test]
fn capability_template_missing_list_field_inherits_default() {
    let manifest_text = r#"{
          "schema_version": "vulcan.provider-manifest.v2",
          "provider_id": "probe-presence",
          "revision": 1,
          "published_at": "2026-08-07T00:00:00Z",
          "engine_contract": { "minimum_manifest_engine": 1, "required_protocol_profiles": ["openai-chat-completions.v1", "openai-responses.v1"] },
          "display": { "name": "Probe Presence", "category": "official" },
          "services": [
            {
              "service_entry_id": "official-api",
              "display_name": "Official API",
              "kind": "conversation",
              "base_url": "https://api.probe.example.com",
              "credential_profiles": [
                { "credential_profile_id": "api-key", "display_name": "API Key", "credential_mode": "token" }
              ],
              "default_authentication": "bearer_authorization",
              "model_catalog": { "source_policy": "manual_only", "discovery_kind": "manual" },
              "protocol_routes": [
                {
                  "protocol_profile_id": "openai-chat-completions.v1",
                  "endpoint_path": "/v1/chat/completions",
                  "verified": true
                }
              ]
            }
          ],
          "model_defaults": {
            "default_enabled": true,
            "catalog_membership_policy": "declared_only",
            "unknown_model_policy": "declared_rule_required",
            "capabilities": {
              "operations": ["conversation_respond"],
              "provider_native_tools": ["web_search"],
              "reasoning": "native",
              "prompt_cache": { "provider_default_behavior": "probe", "supports_managed_explicit": false, "supported_ttl_options": [] },
              "evidence_revision": "defaults.v1",
              "reasoning_efforts": ["none", "medium"]
            }
          },
          "model_rules": [
            {
              "rule_id": "missing-tools",
              "matcher": { "kind": "exact", "value": "presence/model" },
              "selected_protocol": "openai-chat-completions.v1",
              "capabilities": {
                "operations": ["conversation_respond", "embedding_create"],
                "reasoning": "unknown",
                "prompt_cache": { "provider_default_behavior": "probe", "supports_managed_explicit": false, "supported_ttl_options": [] },
                "evidence_revision": "rule.v1"
              }
            }
          ],
          "provenance": { "status": "migrated", "sources": [] }
        }"#;
    let compiled =
        super::compiler::compile_manifest_text(manifest_text, super::ManifestOwnership::System)
            .expect("presence probe must compile");
    let instance = compiled
        .rule_for_model("official-api", Some("api-key"), "presence/model")
        .expect("rule must match");
    let capabilities = instance.capabilities.as_ref().expect("merged capabilities");
    assert_eq!(
        capabilities.provider_native_tools,
        vec![vulcan_ai_contract::NativeToolKind::WebSearch],
        "an omitted list field must inherit the provider default instead of clearing it"
    );
    assert!(
        capabilities
            .operations
            .contains(&vulcan_ai_contract::AiOperation::EmbeddingCreate),
        "a declared list field must still replace the base"
    );
}

/// R2/§6.4.2: a `null` on a nullable limit field clears the lower layer.
/// R2/§6.4.2：可空限制字段上的 `null` 清除下层值。
///
/// Business invariant: omitted fields inherit, `null` clears only nullable fields, concrete
/// values replace; the remaining omitted fields still inherit the provider default base.
/// 业务不变量：省略字段继承，`null` 仅清除可空字段，具体值替换；其余省略字段仍继承
/// 供应商默认基座。
#[test]
fn capability_patch_null_clears_nullable_limit() {
    let manifest: super::ProviderManifestV1 = serde_json::from_str(
        r#"{
          "schema_version": "vulcan.provider-manifest.v2",
          "provider_id": "probe-null",
          "revision": 1,
          "published_at": "2026-08-07T00:00:00Z",
          "engine_contract": { "minimum_manifest_engine": 1, "required_protocol_profiles": ["openai-chat-completions.v1", "openai-responses.v1"] },
          "display": { "name": "Probe Null", "category": "official" },
          "services": [
            {
              "service_entry_id": "official-api",
              "display_name": "Official API",
              "kind": "conversation",
              "base_url": "https://api.probe.example.com",
              "credential_profiles": [
                { "credential_profile_id": "api-key", "display_name": "API Key", "credential_mode": "token" }
              ],
              "default_authentication": "bearer_authorization",
              "model_catalog": { "source_policy": "manual_only", "discovery_kind": "manual" },
              "protocol_routes": [
                {
                  "protocol_profile_id": "openai-chat-completions.v1",
                  "endpoint_path": "/v1/chat/completions",
                  "verified": true
                }
              ]
            }
          ],
          "model_defaults": {
            "default_enabled": true,
            "catalog_membership_policy": "declared_only",
            "unknown_model_policy": "declared_rule_required",
            "capabilities": {
              "operations": ["conversation_respond"],
              "context_limit": 4096,
              "evidence_revision": "defaults.v1"
            }
          },
          "model_rules": [
            {
              "rule_id": "clears-limit",
              "matcher": { "kind": "exact", "value": "null/model" },
              "selected_protocol": "openai-chat-completions.v1",
              "capabilities": {
                "context_limit": null,
                "evidence_revision": "rule.v1"
              }
            }
          ],
          "provenance": { "status": "migrated", "sources": [] }
        }"#,
    )
    .expect("null probe must parse");
    let compiled = super::compiler::compile_manifest(&manifest, super::ManifestOwnership::System)
        .expect("null probe must compile");
    let instance = compiled
        .rule_for_model("official-api", Some("api-key"), "null/model")
        .expect("rule must match");
    let capabilities = instance.capabilities.as_ref().expect("merged capabilities");
    assert_eq!(
        capabilities.context_limit, None,
        "a nullable limit `null` must clear the lower layer"
    );
    assert_eq!(
        capabilities.evidence_revision, "rule.v1",
        "the rule's own evidence revision wins"
    );
    assert_eq!(
        instance.capability_provenance["context_limit"]
            .rule_id
            .as_deref(),
        Some("clears-limit")
    );
}

/// R2/§6.4.2: `null` on a non-nullable capability field is rejected before deserialization.
/// R2/§6.4.2：非可空能力字段上的 `null` 在反序列化前被拒绝。
#[test]
fn capability_patch_null_on_non_nullable_field_rejected() {
    let text = r#"{
      "schema_version": "vulcan.provider-manifest.v2",
      "provider_id": "probe-bad-null",
      "revision": 1,
      "published_at": "2026-08-07T00:00:00Z",
      "engine_contract": { "minimum_manifest_engine": 1, "required_protocol_profiles": ["openai-chat-completions.v1", "openai-responses.v1"] },
      "display": { "name": "Probe Bad Null", "category": "official" },
      "services": [
        {
          "service_entry_id": "official-api",
          "display_name": "Official API",
          "kind": "conversation",
          "base_url": "https://api.probe.example.com",
          "credential_profiles": [
            { "credential_profile_id": "api-key", "display_name": "API Key", "credential_mode": "token" }
          ],
          "default_authentication": "bearer_authorization",
          "model_catalog": { "source_policy": "manual_only", "discovery_kind": "manual" },
          "protocol_routes": [
            {
              "protocol_profile_id": "openai-chat-completions.v1",
              "endpoint_path": "/v1/chat/completions",
              "verified": true
            }
          ]
        }
      ],
      "model_defaults": {
        "default_enabled": true,
        "catalog_membership_policy": "declared_only",
        "unknown_model_policy": "declared_rule_required"
      },
      "model_rules": [
        {
          "rule_id": "bad-null",
          "matcher": { "kind": "exact", "value": "bad/null" },
          "selected_protocol": "openai-chat-completions.v1",
          "capabilities": {
            "reasoning": null,
            "evidence_revision": "rule.v1"
          }
        }
      ],
      "provenance": { "status": "migrated", "sources": [] }
    }"#;
    let error = super::compiler::compile_manifest_text(text, super::ManifestOwnership::System)
        .expect_err("a non-nullable field with `null` must be rejected");
    assert_eq!(error.code, "capability_null_not_allowed");
}

/// R2/§R2.6-R2.7: the exact source layer survives compilation and never degrades to Bundled.
/// R2/§R2.6-R2.7：精确来源层在编译后保留，绝不退化为 Bundled。
#[test]
fn compile_with_source_layer_preserves_exact_layer() {
    use super::{ManifestOwnership, ManifestSourceLayer};
    let text = DEEPSEEK_MANIFEST;
    let bundled = super::compiler::compile_manifest_text_with_layer(
        text,
        ManifestOwnership::System,
        ManifestSourceLayer::Bundled,
    )
    .expect("bundled compile must succeed");
    let active = super::compiler::compile_manifest_text_with_layer(
        text,
        ManifestOwnership::System,
        ManifestSourceLayer::Active,
    )
    .expect("active compile must succeed");
    let previous = super::compiler::compile_manifest_text_with_layer(
        text,
        ManifestOwnership::System,
        ManifestSourceLayer::Previous,
    )
    .expect("previous compile must succeed");
    assert_eq!(
        bundled.snapshot_provenance.source_layer,
        ManifestSourceLayer::Bundled
    );
    assert_eq!(
        active.snapshot_provenance.source_layer,
        ManifestSourceLayer::Active
    );
    assert_eq!(
        previous.snapshot_provenance.source_layer,
        ManifestSourceLayer::Previous
    );
    assert_eq!(
        bundled.snapshot_provenance.sha256, active.snapshot_provenance.sha256,
        "the same source bytes share one content digest across layers"
    );
}

/// R2/§6.4.5: a same-ID parameter overrides, other IDs inherit, and explicit empty clears.
/// R2/§6.4.5：相同 ID 参数覆盖、其他 ID 继承，显式空集合清除。
#[test]
fn model_parameter_presence_merges_by_id_and_explicit_empty_clears() {
    let manifest_text = r#"{
          "schema_version": "vulcan.provider-manifest.v2",
          "provider_id": "probe-params",
          "revision": 1,
          "published_at": "2026-08-07T00:00:00Z",
          "engine_contract": { "minimum_manifest_engine": 1, "required_protocol_profiles": ["openai-chat-completions.v1", "openai-responses.v1"] },
          "display": { "name": "Probe Params", "category": "official" },
          "services": [
            {
              "service_entry_id": "official-api",
              "display_name": "Official API",
              "kind": "conversation",
              "base_url": "https://api.probe.example.com",
              "credential_profiles": [
                { "credential_profile_id": "api-key", "display_name": "API Key", "credential_mode": "token" }
              ],
              "default_authentication": "bearer_authorization",
              "model_catalog": { "source_policy": "manual_only", "discovery_kind": "manual" },
              "protocol_routes": [
                {
                  "protocol_profile_id": "openai-chat-completions.v1",
                  "endpoint_path": "/v1/chat/completions",
                  "verified": true
                }
              ]
            }
          ],
          "model_defaults": {
            "default_enabled": true,
            "catalog_membership_policy": "declared_only",
            "unknown_model_policy": "declared_rule_required",
            "capabilities": {
              "operations": ["conversation_respond"],
              "model_parameters": [
                { "id": "temperature", "display_name": "Temperature", "description": "base temperature",
                  "schema": { "type": "decimal" }, "default": { "type": "explicit_value", "value": { "type": "decimal", "value": "1.0" } },
                  "evidence_revision": "defaults.v1" },
                { "id": "top_p", "display_name": "Top P", "description": "base top_p",
                  "schema": { "type": "decimal" }, "default": { "type": "explicit_value", "value": { "type": "decimal", "value": "1.0" } },
                  "evidence_revision": "defaults.v1" }
              ],
              "evidence_revision": "defaults.v1"
            }
          },
          "model_rules": [
            {
              "rule_id": "param-overlay",
              "matcher": { "kind": "exact", "value": "params/model" },
              "selected_protocol": "openai-chat-completions.v1",
              "capabilities": {
                "model_parameters": [
                  { "id": "temperature", "display_name": "Temperature", "description": "rule temperature",
                    "schema": { "type": "decimal" }, "default": { "type": "explicit_value", "value": { "type": "decimal", "value": "0.2" } },
                    "evidence_revision": "rule.v1" }
                ],
                "evidence_revision": "rule.v1"
              }
            },
            {
              "rule_id": "param-clear",
              "matcher": { "kind": "exact", "value": "params/cleared" },
              "selected_protocol": "openai-chat-completions.v1",
              "capabilities": {
                "model_parameters": [],
                "evidence_revision": "rule.v2"
              }
            }
          ],
          "provenance": { "status": "migrated", "sources": [] }
        }"#;
    eprintln!("DIAG line35: {:?}", manifest_text.lines().nth(34));
    eprintln!("DIAG line36: {:?}", manifest_text.lines().nth(35));
    let manifest: super::ProviderManifestV1 =
        serde_json::from_str(manifest_text).expect("parameter probe must parse");
    let compiled = super::compiler::compile_manifest(&manifest, super::ManifestOwnership::System)
        .expect("parameter probe must compile");
    let instance = compiled
        .rule_for_model("official-api", Some("api-key"), "params/model")
        .expect("rule must match");
    let capabilities = instance.capabilities.as_ref().expect("merged capabilities");
    let temperature = capabilities
        .model_parameters
        .iter()
        .find(|parameter| parameter.id == "temperature")
        .expect("temperature must exist");
    assert_eq!(
        temperature.default,
        vulcan_ai_contract::ModelParameterDefault::ExplicitValue {
            value: vulcan_ai_contract::ModelParameterValue::Decimal(String::from("0.2")),
        },
        "a same-ID parameter must be overridden by the rule"
    );
    assert!(
        capabilities
            .model_parameters
            .iter()
            .any(|parameter| parameter.id == "top_p"),
        "an undeclared parameter ID must be inherited from the base"
    );
    let cleared = compiled
        .rule_for_model("official-api", Some("api-key"), "params/cleared")
        .expect("clear rule must match")
        .capabilities
        .as_ref()
        .expect("cleared capabilities");
    assert!(
        cleared.model_parameters.is_empty(),
        "an explicit empty parameter collection must clear inherited definitions"
    );
}

/// Verifies every closed model-source policy exposes exactly its declared identity layers.
/// 验证每个封闭模型来源策略仅公开其声明的身份层。
#[test]
fn model_source_policy_membership_is_exact_for_all_variants() {
    use super::ModelSourcePolicy;

    // cases is the complete six-variant truth table; adding an enum variant must update it.
    // cases 是完整的六变体真值表；新增枚举变体时必须同步更新。
    let cases = [
        (ModelSourcePolicy::ManifestOnly, true, false, false),
        (ModelSourcePolicy::DiscoveryOnly, false, true, false),
        (ModelSourcePolicy::ManifestAndDiscovery, true, true, false),
        (ModelSourcePolicy::ManualOnly, false, false, true),
        (ModelSourcePolicy::DiscoveryAndManual, false, true, true),
        (
            ModelSourcePolicy::ManifestDiscoveryAndManual,
            true,
            true,
            true,
        ),
    ];
    for (policy, manifest, discovery, manual) in cases {
        assert_eq!(policy.includes_manifest(), manifest, "{policy:?}");
        assert_eq!(policy.includes_discovery(), discovery, "{policy:?}");
        assert_eq!(policy.includes_manual(), manual, "{policy:?}");
    }
}

/// Verifies compilation retains the service-scoped source policy as immutable runtime authority.
/// 验证编译会把服务作用域来源策略保留为不可变运行时权威。
#[test]
fn compiled_manifest_retains_exact_model_source_policy() {
    let manifest: super::ProviderManifestV1 =
        serde_json::from_str(DEEPSEEK_MANIFEST).expect("DeepSeek manifest must parse");
    let compiled = super::compiler::compile_manifest(&manifest, super::ManifestOwnership::System)
        .expect("DeepSeek manifest must compile");
    assert_eq!(
        compiled
            .model_source_policy("official-api")
            .expect("conversation entry must retain its source policy"),
        super::ModelSourcePolicy::ManifestAndDiscovery
    );
    assert!(
        compiled.model_source_policy("missing-entry").is_err(),
        "missing source authority must fail closed"
    );
}
