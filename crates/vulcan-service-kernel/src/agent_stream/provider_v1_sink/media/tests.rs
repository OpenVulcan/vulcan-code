//! Real V1 media preparation against isolated catalogs and frozen offline execution fixtures.
//! 针对隔离目录及冻结离线执行夹具的真实V1媒体准备测试。

use super::*;
mod budget;
mod cancellation;
mod partial_completion;
mod readonly;
mod upload_lifecycle;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use vulcan_provider_adapters::{
    CredentialConnection, CredentialMode, CredentialSecretEnvelope, CredentialSecretResolver,
    CredentialSecretStore, PersistedAiConfiguration, ProtocolModelVariant, PublicAiRuntime,
    ResourceReferencePolicy, UploadChannelDeclaration, parse_openrouter_model_catalog,
};
use vulcan_session_core::v1::{
    UserPromptMediaDelivery, UserPromptParameters, projection::SourcedProjectedItem,
};
use vulcan_workspace_store::workspace_store::ProjectResourceRegistration;

/// Counts credential reads so history tests prove they never attempt a new authenticated upload.
/// 统计凭据读取，使历史测试证明绝未尝试新的认证上传。
#[derive(Default)]
struct OfflineSecrets {
    /// Injects an explicit credential failure after route selection, before any network upload.
    /// 在路线选择之后、任何网络上传之前注入明确凭据失败。
    reject_reads: AtomicBool,
    /// Number of exact secret resolutions; initial execution selection is the only expected read.
    /// 精确秘密解析次数；仅预期初始执行选择进行读取。
    reads: AtomicUsize,
}

#[async_trait::async_trait]
impl CredentialSecretResolver for OfflineSecrets {
    /// Returns one isolated test credential for `reference`; never reads a user credential store.
    /// 为`reference`返回隔离测试凭据；绝不读取用户凭据存储。
    async fn read_optional_secret(
        &self,
        reference: &str,
    ) -> Result<Option<CredentialSecretEnvelope>, String> {
        if reference != "test-media-secret" {
            return Err("unexpected test credential reference".into());
        }
        self.reads.fetch_add(1, Ordering::SeqCst);
        if self.reject_reads.load(Ordering::SeqCst) {
            return Err(String::from("isolated credential preparation failure"));
        }
        Ok(Some(CredentialSecretEnvelope::new_api_key(
            "offline-media-test".into(),
        )?))
    }
}

#[async_trait::async_trait]
impl CredentialSecretStore for OfflineSecrets {
    /// Rejects `reference`/`secret` writes because this fixture is read-only.
    /// 拒绝`reference`及`secret`写入，因为夹具为只读。
    async fn create_secret(
        &self,
        _reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err("read-only test store".into())
    }
    /// Rejects reference replacement and its supplied revision without touching real storage.
    /// 拒绝引用替换及其提供的修订，不触碰真实存储。
    async fn replace_secret(
        &self,
        _reference: &str,
        _revision: u64,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err("read-only test store".into())
    }
    /// Rejects deletion of `reference` from this immutable fixture.
    /// 拒绝从不可变夹具删除`reference`。
    async fn delete_secret(&self, _reference: &str) -> Result<(), String> {
        Err("read-only test store".into())
    }
    /// Rejects restoring `reference`/`secret`; the fixture owns no persistent mutation path.
    /// 拒绝恢复`reference`及`secret`；夹具没有持久化修改路径。
    async fn restore_secret(
        &self,
        _reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err("read-only test store".into())
    }
}

/// Builds a frozen test execution for `second_model`, optionally declaring a bounded upload for images.
/// 为`second_model`构建冻结测试执行，可选声明图片有界上传。
/// Runtime route selection is real; the test-only upload declaration has an unreachable host and is never dispatched.
/// 运行时路线选择真实执行；仅测试上传声明使用不可达宿主，绝不分派。
async fn context(
    second_model: bool,
    uploaded: bool,
) -> Result<(ProviderRequestMediaContext, Arc<OfflineSecrets>), String> {
    context_with_observer(second_model, uploaded, None).await
}

/// Returns the existing isolated inline context to V1 native-tool tests without exposing credential internals.
/// 向V1原生工具测试返回既有隔离内联上下文，不暴露凭据内部结构。
pub(in crate::agent_stream::provider_v1_sink) async fn native_input_context()
-> Result<ProviderRequestMediaContext, String> {
    context(false, false).await.map(|(context, _)| context)
}

/// Verifies original draft single-start and cancellation/observer lifetimes using the existing real frozen route fixture.
/// 使用既有真实冻结路线夹具验证原草稿单次启动及取消与观察者生命周期。
#[tokio::test]
async fn composer_draft_original_upload_has_one_physical_owner()
-> Result<(), Box<dyn std::error::Error>> {
    let (context, secrets) = context(false, true).await?;
    let reads = secrets.reads.load(Ordering::SeqCst);
    crate::media_upload::exercise_draft_upload_reservation_lifecycle(context.execution).await;
    assert_eq!(
        secrets.reads.load(Ordering::SeqCst),
        reads,
        "view changes do not read credentials"
    );
    Ok(())
}

/// Real pasted image staging is converted for the frozen model and retains exact draft ownership.
/// 真实粘贴图片暂存按冻结模型转换，并保留精确草稿归属。
#[tokio::test]
async fn composer_image_conversion_stages_png_before_delivery_intent() -> Result<(), String> {
    // Reuse the established offline route; this test performs no provider request.
    // 复用既有离线路线；此测试不执行供应商请求。
    let (context, _) = context(false, false).await?;
    crate::media_upload::exercise_composer_image_conversion(&context.execution).await
}

/// Builds the real frozen `second_model`/`uploaded` fixture with an optional existing runtime `observer`.
/// 使用可选既有运行时`observer`构建真实冻结`second_model`/`uploaded`夹具。
/// Returns the shared execution and isolated credential store without any external provider call.
/// 返回共享执行及隔离凭据存储，不执行任何外部供应商调用。
async fn context_with_observer(
    second_model: bool,
    uploaded: bool,
    observer: Option<vulcan_provider_adapters::RuntimeMediaUploadProgressObserver>,
) -> Result<(ProviderRequestMediaContext, Arc<OfflineSecrets>), String> {
    context_with_http(second_model, uploaded, observer, None).await
}

/// Builds the same frozen fixture with optional `http` transport, bounded to the HTTPS test's DNS and trust root.
/// 使用可选`http`传输构建相同冻结夹具，该传输限定于HTTPS测试的DNS及信任根。
/// `second_model`, `uploaded` and `observer` retain the existing fixture semantics; returns its execution and isolated secrets.
/// `second_model`、`uploaded`及`observer`保留既有夹具语义；返回其执行及隔离秘密存储。
async fn context_with_http(
    second_model: bool,
    uploaded: bool,
    observer: Option<vulcan_provider_adapters::RuntimeMediaUploadProgressObserver>,
    http: Option<reqwest::Client>,
) -> Result<(ProviderRequestMediaContext, Arc<OfflineSecrets>), String> {
    let secrets = Arc::new(OfflineSecrets::default());
    // Use the repository's existing test-only transport injection without changing production URL validation.
    // 使用仓库既有仅测试传输注入，不改变生产URL验证。
    let runtime = match http {
        Some(http) => {
            PublicAiRuntime::first_batch_with_http_client_for_testing(secrets.clone(), http)?
        }
        None => PublicAiRuntime::first_batch(secrets.clone())?,
    };
    runtime.set_runtime_media_upload_progress_observer(observer)?;
    let mut configuration = PersistedAiConfiguration::default();
    configuration.connection_graph.add_connection(
        &runtime.providers()?,
        CredentialConnection {
            id: "test-media-connection".into(),
            provider_id: "openrouter".into(),
            service_entry_id: "public-api".into(),
            credential_profile_id: "api-key".into(),
            credential_mode: CredentialMode::Token,
            credential_id: "test-media-credential".into(),
            display_name: "offline media".into(),
            secret_reference: "test-media-secret".into(),
            management_secret_reference: None,
            service_origin_override: None,
        },
    )?;
    configuration.set_model_enabled("openrouter", "openai/gpt-5.6-terra", true);
    configuration.set_model_enabled("openrouter", "openai/gpt-5.6-sol", true);
    let mut catalog = parse_openrouter_model_catalog(
        "test-media-connection",
        1,
        r#"{"data":[{"id":"openai/gpt-5.6-terra","name":"A","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text","image"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}},{"id":"openai/gpt-5.6-sol","name":"B","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text","image"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#,
    )?;
    for model in &mut catalog.models {
        model.protocol_variants.insert(
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID.into(),
            ProtocolModelVariant {
                capabilities: model.capabilities.clone(),
                capability_evidence: Default::default(),
                discovery_signature: "offline-media-variant".into(),
                catalog_generation: catalog.generation,
            },
        );
    }
    runtime.stage_source_catalog_for_testing(catalog)?;
    runtime.replace_configuration(configuration.clone())?;
    runtime.reapply_model_catalog_overlays(&configuration)?;
    let model = if second_model {
        "openai/gpt-5.6-sol"
    } else {
        "openai/gpt-5.6-terra"
    };
    let mut execution = runtime
        .resolve_connection_model_execution(
            &configuration,
            "test-media-connection",
            "openrouter",
            model,
        )
        .await?;
    if uploaded {
        // This exact closed declaration tests source/reference semantics without changing any bundled manifest.
        // 此精确封闭声明测试来源及引用语义，不改变任何内置清单。
        let channel: UploadChannelDeclaration = serde_json::from_value(serde_json::json!({
            "upload_channel_id":"offline-files", "primitive":"provider_file_create", "endpoint_host":"unreachable.invalid", "endpoint_path":"/files", "method":"POST", "file_field":"file", "accepted_mime_types":["image/png"], "max_bytes":1048576, "max_count":1, "timeout_ms":1000, "response_id_pointer":"/id", "reference_encoding":"provider_file_id", "reference_scope":"session", "cache_duration_ms":60000
        })).map_err(|error| error.to_string())?;
        execution.route.upload_channels = vec![channel];
        execution.route.protocol_route.input_resource_constraints =
            vec![vulcan_provider_adapters::InputResourceConstraint {
                resource_kind: ResourceKind::Image,
                accepted_mime_types: vec!["image/png".into()],
                max_bytes: Some(1048576),
                max_count: Some(4),
                reference_policy: ResourceReferencePolicy::Any,
                upload_channel_refs: vec!["offline-files".into()],
            }];
    }
    Ok((
        ProviderRequestMediaContext {
            execution: Arc::new(execution),
            session_scope_id: "v1-media-session".into(),
            tool_names: Vec::new(),
        },
        secrets,
    ))
}

/// Checks real composer planning against the same offline routes and immutable receipts used by V1 replay.
/// 根据V1重放使用的相同离线路线和不可变回执，检查真实输入区规划。
#[tokio::test]
async fn composer_draft_routes_preserve_upload_and_inline_intentions()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::media_upload::{test_composer_draft_decision, test_composer_project_decision};
    use vulcan_agent_runtime::attachment_asset_index::{
        AttachmentDraftMediaDelivery, AttachmentDraftMediaOrigin,
    };
    use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
    use vulcan_service_api::ComposerAttachmentDeliveryPlan;
    use vulcan_session_core::v1::UserPromptResourcePreparation;

    // No user data or provider network is used by the real frozen route fixture.
    // 真实冻结路线夹具不使用用户数据或供应商网络。
    let project = tempfile::tempdir()?;
    // A shared catalog holds only immutable source and receipt facts.
    // 共享目录仅持有不可变来源和回执事实。
    let registry = ProjectResourceCatalogRegistry::default();
    // Register a three-byte source; planner tests use its authoritative metadata without reading it.
    // 注册三字节来源；规划测试只使用其权威元数据，不读取来源。
    let entry = register(project.path(), &registry, b"abc")?;
    // Canonical staged shape has the same kind, MIME and digest as the registered source.
    // 规范暂存形状具有与已注册来源相同的类别、MIME及摘要。
    let attachment = ConversationAttachment {
        kind: ConversationAttachmentKind::Image,
        file_name: "image.png".into(),
        local_path: project
            .path()
            .join(&entry.entity_relative_path)
            .to_string_lossy()
            .into_owned(),
        resource_path: None,
        source_path: None,
        asset_id: Some("fixture-asset".into()),
        byte_len: Some(entry.size),
        source_sha256: Some(entry.sha256.clone()),
        delivery_mode: None,
        preview_local_path: None,
        mime_type: "image/png".into(),
    };
    // A has a real upload contract, while both B variants keep the same provider and connection.
    // A具有真实上传契约，而两个B变体保留相同供应商及连接。
    let (mut first, first_secrets) = context(false, true).await?;
    // Create a valid original receipt without performing an upload.
    // 不执行上传，创建一份有效原回执。
    seed_receipt(
        project.path(),
        &registry,
        &entry,
        &first,
        "file-composer-original",
        false,
    )?;
    // Read the fixture's exact only receipt, not a production latest-reference selection.
    // 读取夹具精确唯一回执，不作为生产最近引用选择。
    let binding =
        load_project_resource_catalog(project.path())?.entries[0].provider_bindings[0].clone();
    // The channel and contract remain those selected on A before the switch.
    // 通道及契约保持切换之前A所选值。
    let receipt = &binding.upload_receipt;
    // An admitted unfinished original upload can still complete on A, but never on B.
    // 已准入未完成原上传仍可在A完成，但绝不能在B完成。
    let pending = AttachmentDraftMediaOrigin::new(
        &attachment,
        original_route(&first),
        AttachmentDraftMediaDelivery::Upload {
            upload_channel_id: receipt.upload_channel_id.clone(),
            upload_contract_fingerprint: receipt.upload_contract_fingerprint.clone(),
            receipt: None,
        },
    )?;
    // A successful upload freezes the exact original private receipt.
    // 成功上传冻结精确原私有回执。
    let mut uploaded = pending.clone();
    if let AttachmentDraftMediaDelivery::Upload { receipt, .. } = &mut uploaded.delivery {
        *receipt = Some(binding);
    }
    // An independent inline intention must not acquire upload merely because B offers it.
    // 独立内联意图不能仅因B提供上传就转为上传。
    let inline = AttachmentDraftMediaOrigin::new(
        &attachment,
        original_route(&first),
        AttachmentDraftMediaDelivery::Inline,
    )?;
    // Project occurrences retain exact per-prompt proofs independently of staged leases.
    // 项目出现独立于暂存租约，保留精确的逐提示词证明。
    let project_entry = load_project_resource_catalog(project.path())?.entries[0].clone();
    // The project reference has no staged asset ownership.
    // 项目引用没有暂存资产所有权。
    let mut project_attachment = attachment.clone();
    project_attachment.asset_id = None;
    // Uploaded and inline occurrences of the same source retain distinct original intentions.
    // 同一来源的上传和内联出现保留各自不同的原始意图。
    let project_uploaded =
        UserPromptResourcePreparation::Prepared(uploaded.prompt_delivery(entry.resource_id)?);
    let project_inline =
        UserPromptResourcePreparation::Prepared(inline.prompt_delivery(entry.resource_id)?);
    // Both upload-capable and upload-incapable B must project original uploads as addresses.
    // 具备及不具备上传的B都必须将原上传投影为地址。
    for has_upload in [false, true] {
        // Freeze each exact B variant once, then count all subsequent credential reads.
        // 每个精确B变体只冻结一次，然后统计全部后续凭据读取。
        let (mut second, secrets) = context(true, has_upload).await?;
        // Route selection may read credentials; pure planning may not.
        // 路线选择可以读取凭据；纯规划不可以。
        let reads = secrets.reads.load(Ordering::SeqCst);
        for (preparation, expected) in [
            (
                &project_uploaded,
                ComposerAttachmentDeliveryPlan::MetadataOnly,
            ),
            (
                &project_inline,
                ComposerAttachmentDeliveryPlan::InlineOriginal,
            ),
            (
                &UserPromptResourcePreparation::Unproven,
                ComposerAttachmentDeliveryPlan::MetadataOnly,
            ),
            (
                &UserPromptResourcePreparation::Pending,
                if has_upload {
                    ComposerAttachmentDeliveryPlan::UploadReference
                } else {
                    ComposerAttachmentDeliveryPlan::InlineOriginal
                },
            ),
        ] {
            assert_eq!(
                test_composer_project_decision(
                    &second.execution,
                    &project_attachment,
                    &project_entry,
                    preparation,
                    &second.session_scope_id
                )?
                .0,
                expected
            );
        }
        for original in [&pending, &uploaded] {
            assert_eq!(
                test_composer_draft_decision(
                    &second.execution,
                    &attachment,
                    original,
                    &second.session_scope_id
                )?
                .0,
                ComposerAttachmentDeliveryPlan::MetadataOnly
            );
        }
        assert_eq!(
            test_composer_draft_decision(
                &second.execution,
                &attachment,
                &inline,
                &second.session_scope_id
            )?
            .0,
            ComposerAttachmentDeliveryPlan::InlineOriginal
        );
        // An unsupported B temporarily shows metadata without changing the original inline intention.
        // 不支持的B暂时显示元数据，不改变原内联意图。
        Arc::get_mut(&mut second.execution)
            .ok_or("exclusive B")?
            .model
            .capabilities
            .input_resources
            .clear();
        assert_eq!(
            test_composer_project_decision(
                &second.execution,
                &project_attachment,
                &project_entry,
                &project_inline,
                &second.session_scope_id
            )?
            .0,
            ComposerAttachmentDeliveryPlan::MetadataOnly
        );
        assert_eq!(
            test_composer_draft_decision(
                &second.execution,
                &attachment,
                &inline,
                &second.session_scope_id
            )?
            .0,
            ComposerAttachmentDeliveryPlan::MetadataOnly
        );
        assert_eq!(secrets.reads.load(Ordering::SeqCst), reads);
    }
    // Returning to A restores each original; pending upload remains upload even if its soft inline preference changed.
    // 返回A恢复各原方式；即使软内联偏好变化，待处理原上传仍保持上传。
    Arc::get_mut(&mut first.execution)
        .ok_or("exclusive A")?
        .media_delivery_policy
        .conversation
        .image
        .with_upload_max_bytes = 512 * 1024;
    // All repeated planning remains free of credential or network side effects.
    // 所有重复规划都不产生凭据读取或网络副作用。
    let reads = first_secrets.reads.load(Ordering::SeqCst);
    assert_eq!(
        test_composer_project_decision(
            &first.execution,
            &project_attachment,
            &project_entry,
            &project_uploaded,
            &first.session_scope_id
        )?,
        (
            ComposerAttachmentDeliveryPlan::UploadReference,
            "original_upload_reference_restored"
        )
    );
    assert_eq!(
        test_composer_project_decision(
            &first.execution,
            &project_attachment,
            &project_entry,
            &project_inline,
            &first.session_scope_id
        )?
        .0,
        ComposerAttachmentDeliveryPlan::InlineOriginal
    );
    assert_eq!(
        test_composer_draft_decision(
            &first.execution,
            &attachment,
            &pending,
            &first.session_scope_id
        )?
        .0,
        ComposerAttachmentDeliveryPlan::UploadReference
    );
    assert_eq!(
        test_composer_draft_decision(
            &first.execution,
            &attachment,
            &uploaded,
            &first.session_scope_id
        )?
        .1,
        "original_upload_reference_restored"
    );
    assert_eq!(
        test_composer_draft_decision(
            &first.execution,
            &attachment,
            &inline,
            &first.session_scope_id
        )?
        .0,
        ComposerAttachmentDeliveryPlan::InlineOriginal
    );
    // The exact receipt lifetime is authoritative; expiration does not select a replacement upload.
    // 精确回执生命周期具有权威性；过期不选择替代上传。
    if let AttachmentDraftMediaDelivery::Upload {
        receipt: Some(binding),
        ..
    } = &mut uploaded.delivery
    {
        binding.upload_receipt.expires_at_unix_millis = 2;
    }
    assert_eq!(
        test_composer_draft_decision(
            &first.execution,
            &attachment,
            &uploaded,
            &first.session_scope_id
        )?
        .0,
        ComposerAttachmentDeliveryPlan::MetadataOnly
    );
    assert_eq!(first_secrets.reads.load(Ordering::SeqCst), reads);
    Ok(())
}

/// Keeps inherited unknown media address-only even inside a newly admitted prompt on an upload-capable model.
/// 即使处于具备上传能力模型的新准入提示词中，继承的未知媒体也保持仅地址。
#[tokio::test]
async fn v1_copied_unknown_resource_never_gains_new_upload_eligibility()
-> Result<(), Box<dyn std::error::Error>> {
    // Exact immutable source bytes exist, so rejection proves provenance policy rather than missing-source behavior.
    // 精确不可变源字节实际存在，因此拒绝证明来源策略，而非来源缺失行为。
    let project = tempfile::tempdir()?;
    // The project owner remains the ordinary production catalog registry.
    // 项目所有者仍为普通生产目录注册表。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // Register a small inline/upload-capable image.
    // 注册可内联及上传的小图片。
    let entry = register(project.path(), &registry, b"abc")?;
    // This current target supports upload, but unknown history cannot authorize it.
    // 当前目标支持上传，但未知历史不能授权上传。
    let (context, secrets) = context(false, true).await?;
    // Record credential reads after route freezing; media preparation must add none.
    // 路线冻结后记录凭据读取；媒体准备不能新增读取。
    let reads = secrets.reads.load(Ordering::SeqCst);
    // The new prompt explicitly marks this copied source as unproven.
    // 新提示词显式标记此复制来源为未经证明。
    let mut parameters = UserPromptParameters::for_resource_preparations(
        &[entry.resource_id],
        vec![vulcan_session_core::v1::UserPromptResourcePreparation::Unproven],
    )?
    .ok_or("parameters")?;
    // Core grants first-request freezing, not permission to invent source history.
    // Core授予首次请求冻结资格，而非虚构来源历史的许可。
    let input = snapshot(
        project.path(),
        entry.resource_id,
        vec![Some(parameters.clone())],
    )?;
    // The actual preparation must preserve address-only semantics without consulting another upload.
    // 实际准备必须保持仅地址语义，不查询另一上传。
    let prepared = prepare_v1_user_media(&input, Some(&context), Arc::clone(&registry)).await?;
    assert!(metadata(&prepared, 0).contains("project-resource://"));
    assert_eq!(
        prepared.bindings[0].origin.resources[0].delivery,
        UserPromptResourceDeliveryKind::Attachment
    );
    parameters.media_delivery = UserPromptMediaDelivery::Frozen {
        origin: prepared.bindings[0].origin.clone(),
    };
    parameters.validate(&[entry.resource_id])?;
    // Later replay still carries the explicit unknown marker rather than promoting the first address view to proof.
    // 后续重放仍携带显式未知标记，不将首次地址视图提升为来源证明。
    let replay = snapshot(project.path(), entry.resource_id, vec![Some(parameters)])?;
    assert!(
        metadata(
            &prepare_v1_user_media(&replay, Some(&context), registry).await?,
            0
        )
        .contains("project-resource://")
    );
    assert_eq!(secrets.reads.load(Ordering::SeqCst), reads);
    Ok(())
}

/// Registers exact `bytes` in the existing project `registry`, returning its immutable resource entry.
/// 在既有项目`registry`注册精确`bytes`，返回不可变资源条目。
fn register(
    project: &std::path::Path,
    registry: &ProjectResourceCatalogRegistry,
    bytes: &[u8],
) -> Result<ResourceCatalogEntry, String> {
    registry
        .resolve_or_register_project_resources(
            project,
            &[ProjectResourceRegistration {
                original_name: "image.png".into(),
                mime: Some("image/png".into()),
                content: bytes.to_vec(),
                provider_bindings: Vec::new(),
            }],
        )
        .map_err(|error| error.to_string())?;
    load_project_resource_catalog(project)
        .map_err(|error| error.to_string())?
        .entries
        .into_iter()
        .last()
        .ok_or("registered entry missing".into())
}

/// Builds a sourced snapshot for ordered `parameters` occurrences of `resource`, with Core eligibility only for explicit Pending rows.
/// 为`resource`按有序`parameters`出现构建来源快照，仅对显式Pending行给予Core资格。
fn snapshot(
    project: &std::path::Path,
    resource: ResourceId,
    parameters: Vec<Option<UserPromptParameters>>,
) -> Result<SessionProviderProjectionSnapshot, String> {
    let mut preparing = Vec::new();
    let mut sourced_items = Vec::new();
    for (index, parameters) in parameters.into_iter().enumerate() {
        let seq = Seq::try_new(index as u64 + 1).map_err(|error| error.to_string())?;
        if parameters.as_ref().is_some_and(|parameters| {
            matches!(
                parameters.media_delivery,
                UserPromptMediaDelivery::Pending | UserPromptMediaDelivery::Preparing { .. }
            )
        }) {
            preparing.push(seq);
        }
        sourced_items.push(SourcedProjectedItem {
            user_prompt_parameters: parameters,
            source_from_seq: seq,
            source_to_seq: seq,
            item: ProjectedItem::UserInput {
                text: format!("prompt-{index}"),
                resource_ids: vec![resource],
            },
        });
    }
    Ok(SessionProviderProjectionSnapshot {
        unenveloped_assistant_requests: Default::default(),
        current_compaction_id: None,
        open_turn_id: None,
        child_business_execution_id: None,
        preparing_user_prompt_media_seqs: preparing,
        compaction_frontier_seq: None,
        image_replay_from_seq: None,
        segment_generation: 1,
        document_revision: 1,
        retained_from_seq: Seq::try_new(1).map_err(|error| error.to_string())?,
        through_seq: Seq::try_new(sourced_items.len() as u64).map_err(|error| error.to_string())?,
        items: sourced_items.iter().map(|item| item.item.clone()).collect(),
        sourced_items,
        project_directory: project.to_path_buf(),
        provider_usage_budget_anchor: None,
        initialized_locked_baseline: None,
    })
}

/// Encodes exact `origin` as frozen prompt parameters, never through a cache-derived default.
/// 将精确`origin`编码为冻结提示词参数，绝不使用缓存派生默认值。
fn frozen(origin: UserPromptMediaOrigin) -> Option<UserPromptParameters> {
    Some(UserPromptParameters {
        unproven_resource_positions: Vec::new(),
        version: 1,
        prepared_resources: Vec::new(),
        media_delivery: UserPromptMediaDelivery::Frozen { origin },
    })
}

/// Keeps an already prepared A upload when B owns the first physical request, then restores A without another upload.
/// B持有首次物理请求时保留已准备A上传，然后无需再次上传即可恢复A。
#[tokio::test]
async fn v1_pending_prompt_handoff_preserves_earlier_upload_origin()
-> Result<(), Box<dyn std::error::Error>> {
    // Both request contexts consume the same real project catalog.
    // 两个请求上下文消费同一真实项目目录。
    let project = tempfile::tempdir()?;
    // The actual catalog owner retains the private first receipt.
    // 实际目录所有者保留私有首次回执。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // Canonical source identity remains unchanged through the first request and model return.
    // 规范来源身份在首次请求及模型回切期间保持不变。
    let entry = register(project.path(), &registry, b"abc")?;
    // Model A prepared the resource before any user prompt was sent.
    // 模型A在发送任何用户提示词之前准备资源。
    let (first, first_secrets) = context(false, true).await?;
    // The user switches to B before submitting the prompt.
    // 用户在提交提示词之前切到B。
    let (second, second_secrets) = context(true, true).await?;
    // This is the exact completed receipt handed off by the draft resource owner.
    // 这是草稿资源所有者交接的精确已完成回执。
    let original = seed_receipt(
        project.path(),
        &registry,
        &entry,
        &first,
        "file-draft-A",
        false,
    )?;
    // Pending concerns first physical preparation, not absence of already proven draft media.
    // 待准备指首次物理准备，而非缺失已经证明的草稿媒体。
    let mut parameters = UserPromptParameters {
        unproven_resource_positions: Vec::new(),
        version: 1,
        prepared_resources: vec![Some(original.resources[0].clone())],
        media_delivery: UserPromptMediaDelivery::Pending,
    };
    // Core marks only this new prompt eligible for its first immutable binding.
    // Core只将本新提示词标记为有资格获得首次不可变绑定。
    let inputs = snapshot(
        project.path(),
        entry.resource_id,
        vec![Some(parameters.clone())],
    )?;
    // No additional credential lookup is permitted for either replay direction.
    // 两个重放方向都不允许额外凭据查询。
    let first_reads = first_secrets.reads.load(Ordering::SeqCst);
    // Capture B's baseline before first-request preparation.
    // 首次请求准备前捕获B基线。
    let second_reads = second_secrets.reads.load(Ordering::SeqCst);
    // B gets an address even though it supports the same upload interface.
    // 即使B支持相同上传接口，也只获得地址。
    let prepared = prepare_v1_user_media(&inputs, Some(&second), registry.clone()).await?;
    assert!(metadata(&prepared, 0).contains("another model"));
    assert_eq!(prepared.bindings[0].origin.route, original_route(&second));
    assert_eq!(prepared.bindings[0].origin.resources, original.resources);
    parameters.media_delivery = UserPromptMediaDelivery::Frozen {
        origin: prepared.bindings[0].origin.clone(),
    };
    // After the first request, A restores the exact original reference from prompt-owned proof.
    // 首次请求之后，A根据提示词所有证明恢复精确原引用。
    let restored = snapshot(project.path(), entry.resource_id, vec![Some(parameters)])?;
    assert_eq!(
        native(
            &prepare_v1_user_media(&restored, Some(&first), registry).await?,
            0
        )
        .id,
        "file-draft-A"
    );
    assert_eq!(first_secrets.reads.load(Ordering::SeqCst), first_reads);
    assert_eq!(second_secrets.reads.load(Ordering::SeqCst), second_reads);
    Ok(())
}

/// Returns the native resource at prompt `index`, failing if the production preparation chose metadata only.
/// 返回提示词`index`处的原生资源；生产准备选择仅元数据时失败。
fn native(prepared: &PreparedV1UserMedia, index: usize) -> &ResourceReference {
    let ContentItem::Resource { resource } = &prepared.prompts[&index].content[1] else {
        panic!("expected native resource");
    };
    resource
}

/// Returns attachment text at prompt `index`, proving no resource object was emitted.
/// 返回提示词`index`处的附件文本，证明没有产出资源对象。
fn metadata(prepared: &PreparedV1UserMedia, index: usize) -> &str {
    let ContentItem::Text { text } = &prepared.prompts[&index].content[1] else {
        panic!("expected metadata-only attachment");
    };
    text
}

/// Seeds a private receipt with `reference` and explicit `expired` lifetime, returning its prompt-safe original identity.
/// 播种具有`reference`和显式`expired`生命周期的私有回执，返回提示词安全原身份。
fn seed_receipt(
    project: &std::path::Path,
    registry: &ProjectResourceCatalogRegistry,
    entry: &ResourceCatalogEntry,
    context: &ProviderRequestMediaContext,
    reference: &str,
    expired: bool,
) -> Result<UserPromptMediaOrigin, String> {
    let execution = &context.execution;
    let contract = execution
        .client
        .conversation_media_upload_contract(&execution.route, ResourceKind::Image, "image/png")?
        .ok_or("test upload contract missing")?;
    let binding = ProviderBinding {
        provider: execution.route.provider.id.clone(),
        connection_id: execution.route.connection.id.clone(),
        media_id: reference.into(),
        upload_receipt: ProjectResourceUploadReceipt {
            draft_intent_id: None,
            version: 1,
            service_entry_id: execution.route.entry.id.clone(),
            upload_channel_id: contract.upload_channel_id.clone(),
            upload_contract_fingerprint: contract.contract_fingerprint,
            resource_kind: ResourceKind::Image,
            mime_type: "image/png".into(),
            source_kind: ResourceSourceKind::ProviderFileId,
            session_scope_id: Some(context.session_scope_id.clone()),
            created_at_unix_millis: 1,
            expires_at_unix_millis: if expired { 2 } else { u64::MAX },
        },
    };
    registry
        .record_provider_upload_receipt(
            project,
            entry.resource_id,
            &entry.sha256,
            entry.size,
            binding.clone(),
        )
        .map_err(|error| error.to_string())?;
    Ok(UserPromptMediaOrigin {
        route: original_route(context),
        resources: vec![UserPromptResourceDelivery {
            resource_id: entry.resource_id,
            original_route: original_route(context),
            delivery: uploaded_origin(&binding, entry).map_err(|error| error.to_string())?,
        }],
    })
}

/// Verifies proven inline history remains inline across supported models despite new upload preferences or zero soft thresholds.
/// 验证已证明内联历史跨受支持模型仍保持内联，不受新上传偏好或零软阈值改写。
#[tokio::test]
async fn v1_inline_history_is_reversible_and_does_not_upload()
-> Result<(), Box<dyn std::error::Error>> {
    let project = tempfile::tempdir()?;
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    let entry = register(project.path(), &registry, b"abc")?;
    let (mut first, first_secrets) = context(false, false).await?;
    let pending = snapshot(
        project.path(),
        entry.resource_id,
        vec![UserPromptParameters::for_new_resources(
            &[entry.resource_id],
        )],
    )?;
    let prepared = prepare_v1_user_media(&pending, Some(&first), registry.clone()).await?;
    assert_eq!(
        native(&prepared, 0).url.as_deref(),
        Some("data:image/png;base64,YWJj")
    );
    let origin = prepared.bindings[0].origin.clone();
    let history = snapshot(project.path(), entry.resource_id, vec![frozen(origin)])?;
    let (mut second, second_secrets) = context(true, true).await?;
    let second_reads = second_secrets.reads.load(Ordering::SeqCst);
    let on_second = prepare_v1_user_media(&history, Some(&second), registry.clone()).await?;
    assert_eq!(
        native(&on_second, 0).source_kind,
        ResourceSourceKind::InlineData
    );
    assert!(on_second.bindings.is_empty());
    Arc::get_mut(&mut second.execution)
        .ok_or("exclusive test execution")?
        .model
        .capabilities
        .input_resources
        .clear();
    assert!(
        metadata(
            &prepare_v1_user_media(&history, Some(&second), registry.clone()).await?,
            0
        )
        .contains("resource bytes were not provided")
    );
    Arc::get_mut(&mut first.execution)
        .ok_or("exclusive test execution")?
        .media_delivery_policy
        .conversation
        .image
        .without_upload_max_bytes = 0;
    let reads = first_secrets.reads.load(Ordering::SeqCst);
    assert_eq!(
        native(
            &prepare_v1_user_media(&history, Some(&first), registry).await?,
            0
        )
        .source_kind,
        ResourceSourceKind::InlineData
    );
    assert_eq!(first_secrets.reads.load(Ordering::SeqCst), reads);
    assert_eq!(second_secrets.reads.load(Ordering::SeqCst), second_reads);
    Ok(())
}

/// Verifies an original upload restores its exact older receipt and never cross-sends or reuploads on model changes.
/// 验证原上传恢复精确更早回执，且在模型变化时绝不跨发或重传。
#[tokio::test]
async fn v1_uploaded_history_uses_original_receipt_not_latest_binding()
-> Result<(), Box<dyn std::error::Error>> {
    let project = tempfile::tempdir()?;
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    let entry = register(project.path(), &registry, b"abc")?;
    let (first, secrets) = context(false, true).await?;
    let origin = seed_receipt(
        project.path(),
        &registry,
        &entry,
        &first,
        "file-original",
        false,
    )?;
    seed_receipt(
        project.path(),
        &registry,
        &entry,
        &first,
        "file-later",
        false,
    )?;
    let history = snapshot(project.path(), entry.resource_id, vec![frozen(origin)])?;
    let reads = secrets.reads.load(Ordering::SeqCst);
    assert_eq!(
        native(
            &prepare_v1_user_media(&history, Some(&first), registry.clone()).await?,
            0
        )
        .id,
        "file-original"
    );
    for uploaded in [false, true] {
        let (second, second_secrets) = context(true, uploaded).await?;
        let second_reads = second_secrets.reads.load(Ordering::SeqCst);
        let prepared = prepare_v1_user_media(&history, Some(&second), registry.clone()).await?;
        let text = metadata(&prepared, 0);
        assert!(text.contains("project-resource://1"));
        assert!(!text.contains("file-original"));
        assert!(!text.contains("file-later"));
        assert_eq!(second_secrets.reads.load(Ordering::SeqCst), second_reads);
    }
    assert_eq!(
        native(
            &prepare_v1_user_media(&history, Some(&first), registry).await?,
            0
        )
        .id,
        "file-original"
    );
    assert_eq!(secrets.reads.load(Ordering::SeqCst), reads);
    Ok(())
}

/// Verifies an expired original receipt cannot be replaced by a later valid receipt for the same source.
/// 验证过期原回执不能被同一来源的较新有效回执替换。
#[tokio::test]
async fn v1_expired_and_unknown_history_never_become_new_uploads()
-> Result<(), Box<dyn std::error::Error>> {
    let project = tempfile::tempdir()?;
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    let entry = register(project.path(), &registry, b"abc")?;
    let (context, secrets) = context(false, true).await?;
    let origin = seed_receipt(
        project.path(),
        &registry,
        &entry,
        &context,
        "file-expired",
        true,
    )?;
    seed_receipt(
        project.path(),
        &registry,
        &entry,
        &context,
        "file-valid",
        false,
    )?;
    let history = snapshot(
        project.path(),
        entry.resource_id,
        vec![frozen(origin), None],
    )?;
    let reads = secrets.reads.load(Ordering::SeqCst);
    let prepared = prepare_v1_user_media(&history, Some(&context), registry).await?;
    assert!(metadata(&prepared, 0).contains("expired"));
    assert!(metadata(&prepared, 1).contains("unknown"));
    assert!(prepared.bindings.is_empty());
    assert_eq!(secrets.reads.load(Ordering::SeqCst), reads);
    Ok(())
}

/// Verifies a new prompt may reuse a valid receipt without changing an earlier prompt's original model ownership.
/// 验证新提示词可复用有效回执，但不改变更早提示词的原模型归属。
#[tokio::test]
async fn v1_same_resource_in_two_prompts_keeps_distinct_origins()
-> Result<(), Box<dyn std::error::Error>> {
    let project = tempfile::tempdir()?;
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    let entry = register(project.path(), &registry, b"abc")?;
    let (first, _) = context(false, true).await?;
    let (second, secrets) = context(true, true).await?;
    let original = seed_receipt(
        project.path(),
        &registry,
        &entry,
        &first,
        "file-first",
        false,
    )?;
    seed_receipt(
        project.path(),
        &registry,
        &entry,
        &second,
        "file-second",
        false,
    )?;
    let inputs = snapshot(
        project.path(),
        entry.resource_id,
        vec![
            frozen(original.clone()),
            UserPromptParameters::for_new_resources(&[entry.resource_id]),
        ],
    )?;
    let reads = secrets.reads.load(Ordering::SeqCst);
    let prepared = prepare_v1_user_media(&inputs, Some(&second), registry).await?;
    assert!(metadata(&prepared, 0).contains("another model"));
    assert_eq!(prepared.bindings.len(), 1);
    assert_eq!(prepared.bindings[0].prompt_seq.value(), 2);
    assert_eq!(prepared.bindings[0].origin.route, original_route(&second));
    assert_eq!(
        native(&prepared, 1).source_kind,
        ResourceSourceKind::ProviderFileId
    );
    assert_eq!(
        inputs.sourced_items[0].user_prompt_parameters,
        frozen(original)
    );
    assert_eq!(secrets.reads.load(Ordering::SeqCst), reads);
    Ok(())
}

/// Restores each occurrence by its own original model when two uploads coexist in one prompt first sent on model B.
/// 同一首次在模型B发送的提示词含两次上传时，按每次出现各自原模型恢复。
#[tokio::test]
async fn v1_mixed_original_routes_in_one_prompt_restore_independently()
-> Result<(), Box<dyn std::error::Error>> {
    // One project stores both references for the same immutable file.
    // 一个项目存储同一不可变文件的两个引用。
    let project = tempfile::tempdir()?;
    // The real catalog owner appends receipts without replacing earlier records.
    // 真实目录所有者追加回执，不替换更早记录。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // Duplicate appearances intentionally share source identity but not delivery origin.
    // 重复出现故意共享来源身份，但不共享投递原身份。
    let entry = register(project.path(), &registry, b"abc")?;
    // Two exact model contexts share the provider and upload contract.
    // 两个精确模型上下文共享供应商及上传契约。
    let (first, first_secrets) = context(false, true).await?;
    // Changing only model still changes the original upload target.
    // 仅改变模型仍会改变原上传目标。
    let (second, second_secrets) = context(true, true).await?;
    // Capture the earlier model-A occurrence before appending the model-B receipt.
    // 追加模型B回执之前捕获更早模型A出现。
    let mut first_origin =
        seed_receipt(project.path(), &registry, &entry, &first, "file-A", false)?;
    // The first physical request is B, but its first resource occurrence still belongs to A.
    // 首次物理请求为B，但其第一个资源出现仍属于A。
    let mut origin = seed_receipt(project.path(), &registry, &entry, &second, "file-B", false)?;
    origin.resources.insert(0, first_origin.resources.remove(0));
    // Assemble the exact two-occurrence prompt; no other prompt or cache timestamp selects ownership.
    // 组装精确两次出现提示词；不使用其他提示词或缓存时间戳选择归属。
    let mut inputs = snapshot(
        project.path(),
        entry.resource_id,
        vec![frozen(origin.clone())],
    )?;
    inputs.sourced_items[0].item = ProjectedItem::UserInput {
        text: "mixed-origin".into(),
        resource_ids: vec![entry.resource_id, entry.resource_id],
    };
    inputs.items[0] = inputs.sourced_items[0].item.clone();
    // Original credential reads exclude all replay work from the expected baseline.
    // 原凭据读取次数为全部重放工作提供预期基线。
    let first_reads = first_secrets.reads.load(Ordering::SeqCst);
    // B must also perform no fresh authenticated upload on replay.
    // B重放时同样不能执行新的认证上传。
    let second_reads = second_secrets.reads.load(Ordering::SeqCst);
    // A restores only the first occurrence, preserving prompt resource order.
    // A只恢复第一个出现，保留提示词资源顺序。
    let on_first = prepare_v1_user_media(&inputs, Some(&first), registry.clone()).await?;
    assert!(
        matches!(&on_first.prompts[&0].content[1], ContentItem::Resource { resource } if resource.id == "file-A")
    );
    assert!(
        matches!(&on_first.prompts[&0].content[2], ContentItem::Text { text } if text.contains("another model"))
    );
    // B restores only the second occurrence, even though it owns the prompt's first physical request.
    // 即使B持有提示词首次物理请求，也只恢复第二个出现。
    let on_second = prepare_v1_user_media(&inputs, Some(&second), registry).await?;
    assert!(
        matches!(&on_second.prompts[&0].content[1], ContentItem::Text { text } if text.contains("another model"))
    );
    assert!(
        matches!(&on_second.prompts[&0].content[2], ContentItem::Resource { resource } if resource.id == "file-B")
    );
    assert_eq!(
        inputs.sourced_items[0].user_prompt_parameters,
        frozen(origin)
    );
    assert!(on_first.bindings.is_empty() && on_second.bindings.is_empty());
    assert_eq!(first_secrets.reads.load(Ordering::SeqCst), first_reads);
    assert_eq!(second_secrets.reads.load(Ordering::SeqCst), second_reads);
    Ok(())
}

/// Verifies shared encoded budget follows prompt/resource order and corrupt source bytes never reach the model.
/// 验证共享编码预算遵循提示词及资源顺序，损坏来源字节绝不进入模型。
#[tokio::test]
async fn v1_encoded_budget_and_source_integrity_apply_before_native_delivery()
-> Result<(), Box<dyn std::error::Error>> {
    let project = tempfile::tempdir()?;
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    let entry = register(project.path(), &registry, &vec![1; 512 * 1024])?;
    let (context, _) = context(false, false).await?;
    let pending = UserPromptParameters::for_new_resources(&[entry.resource_id]);
    let inputs = snapshot(
        project.path(),
        entry.resource_id,
        vec![pending.clone(), pending.clone(), pending],
    )?;
    let prepared = prepare_v1_user_media(&inputs, Some(&context), registry.clone()).await?;
    assert_eq!(
        native(&prepared, 0).source_kind,
        ResourceSourceKind::InlineData
    );
    assert_eq!(
        native(&prepared, 1).source_kind,
        ResourceSourceKind::InlineData
    );
    assert!(metadata(&prepared, 2).contains("combined encoded"));
    assert_eq!(prepared.bindings.len(), 3);
    let history = snapshot(
        project.path(),
        entry.resource_id,
        vec![frozen(prepared.bindings[0].origin.clone())],
    )?;
    std::fs::write(
        project.path().join(&entry.entity_relative_path),
        vec![2; entry.size as usize],
    )?;
    assert!(
        metadata(
            &prepare_v1_user_media(&history, Some(&context), registry).await?,
            0
        )
        .contains("could not be verified")
    );
    Ok(())
}

/// Verifies that historical pressure triggers only when retirement lets the new image fit.
/// 验证仅在退役历史图能让新图准入时触发历史图片压力。
#[tokio::test]
async fn v1_image_compaction_preserves_the_new_image_through_follow_up()
-> Result<(), Box<dyn std::error::Error>> {
    let project = tempfile::tempdir()?;
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    let entry = register(project.path(), &registry, &vec![1; 512 * 1024])?;
    let (context, _) = context(false, false).await?;
    let pending = UserPromptParameters::for_new_resources(&[entry.resource_id]);
    let initial = snapshot(
        project.path(),
        entry.resource_id,
        vec![pending.clone(), pending.clone()],
    )?;
    let first = prepare_v1_user_media(&initial, Some(&context), Arc::clone(&registry)).await?;
    let old = first
        .bindings
        .iter()
        .map(|binding| frozen(binding.origin.clone()))
        .collect::<Vec<_>>();
    let mut prompts = old.clone();
    prompts.push(pending.clone());
    let mut request = snapshot(project.path(), entry.resource_id, prompts)?;
    assert!(conversation_image_compaction_required(&request, &context).await?);
    let summary = prepare_image_summary_media(
        &request,
        Some(&context),
        Arc::clone(&registry),
        &[CompactedImageExcerpt {
            locator: project_resource_locator(entry.resource_id),
            source_seqs: request
                .sourced_items
                .iter()
                .take(2)
                .map(|item| item.source_from_seq)
                .collect(),
            sha256: entry.sha256.clone(),
            description: Some(String::from("Historical image")),
        }],
    )
    .await?;
    assert!(summary.bindings.is_empty());
    assert!(summary.prompts.values().all(|prompt| {
        prompt
            .content
            .iter()
            .all(|item| !matches!(item, ContentItem::Resource { .. }))
    }));

    // The pending sequence can predate the summary API while still owning its first business send.
    // 待发送序号可能早于摘要 API，但仍保有首次业务投递权。
    let pending_seq = request.preparing_user_prompt_media_seqs[0];
    request.compaction_frontier_seq = Some(pending_seq);
    request.image_replay_from_seq = Some(pending_seq);
    assert!(!conversation_image_compaction_required(&request, &context).await?);
    let resumed = prepare_v1_user_media(&request, Some(&context), Arc::clone(&registry)).await?;
    assert_eq!(
        native(&resumed, 2).source_kind,
        ResourceSourceKind::InlineData
    );
    assert!(!metadata(&resumed, 0).is_empty());
    assert!(!metadata(&resumed, 1).is_empty());

    // A tool follow-up must retain the new image after its first business send froze the origin.
    // 工具后续请求中，新图首次业务投递冻结原身份后仍保留。
    let mut prompts = old;
    prompts.push(frozen(resumed.bindings[0].origin.clone()));
    let mut follow_up = snapshot(project.path(), entry.resource_id, prompts)?;
    follow_up.compaction_frontier_seq = Some(pending_seq);
    follow_up.image_replay_from_seq = Some(pending_seq);
    let replayed = prepare_v1_user_media(&follow_up, Some(&context), registry).await?;
    assert_eq!(
        native(&replayed, 2).source_kind,
        ResourceSourceKind::InlineData
    );
    assert!(!metadata(&replayed, 0).is_empty());
    Ok(())
}

/// Distinguishes new upload failure from expired history and never falls back to Base64 or exposes the private error.
/// 区分新上传失败与历史过期，绝不回退Base64或暴露私有错误。
#[tokio::test]
async fn v1_failed_new_upload_keeps_explicit_safe_attachment()
-> Result<(), Box<dyn std::error::Error>> {
    // Only this temporary catalog is used by the failed preparation.
    // 失败准备只使用本临时目录。
    let project = tempfile::tempdir()?;
    // The real resource owner contains no earlier upload receipt.
    // 真实资源所有者不含任何更早上传回执。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // A small source would fit inline, but a failed planned upload must not change branches.
    // 小来源原本满足内联大小，但已规划上传失败不能改变分支。
    let entry = register(project.path(), &registry, b"abc")?;
    // Freeze a valid upload route before rejecting later credential preparation.
    // 在拒绝后续凭据准备之前冻结有效上传路线。
    let (context, secrets) = context(false, true).await?;
    secrets.reject_reads.store(true, Ordering::SeqCst);
    // Core explicitly identifies this as a new first-preparation prompt.
    // Core明确将其标识为新首次准备提示词。
    let inputs = snapshot(
        project.path(),
        entry.resource_id,
        vec![UserPromptParameters::for_new_resources(
            &[entry.resource_id],
        )],
    )?;
    // Credential rejection happens before the fixture's unreachable upload host could be contacted.
    // 凭据拒绝发生在夹具不可达上传宿主可能被联系之前。
    let prepared = prepare_v1_user_media(&inputs, Some(&context), registry).await?;
    // The model receives only safe location and an accurate unsent-content reason.
    // 模型只接收安全位置和准确未发送内容原因。
    let text = metadata(&prepared, 0);
    assert!(text.contains("selected upload could not be prepared"));
    assert!(text.contains("project-resource://1"));
    assert!(!text.contains("isolated credential"));
    assert!(!text.contains("base64,"));
    // Failed upload still returns metadata, but preserves the original selected upload rather than claiming address-only intent.
    // 上传失败仍返回元数据，但保留原所选上传，而非声称仅地址意图。
    assert!(matches!(
        prepared.bindings[0].origin.resources[0].delivery,
        UserPromptResourceDeliveryKind::UploadPending { .. }
    ));
    Ok(())
}

/// Keeps prompt media at or before the compaction frontier as an address only while later media stays native.
/// 压缩前沿及之前的提示词媒体仅保留地址，之后的媒体保持原生。
#[tokio::test]
async fn v1_media_before_compaction_frontier_keeps_only_its_address()
-> Result<(), Box<dyn std::error::Error>> {
    let project = tempfile::tempdir()?;
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    let entry = register(project.path(), &registry, b"abc")?;
    let (first, _first_secrets) = context(false, false).await?;
    let pending = snapshot(
        project.path(),
        entry.resource_id,
        vec![
            UserPromptParameters::for_new_resources(&[entry.resource_id]),
            UserPromptParameters::for_new_resources(&[entry.resource_id]),
        ],
    )?;
    let prepared = prepare_v1_user_media(&pending, Some(&first), registry.clone()).await?;
    let origins = prepared
        .bindings
        .iter()
        .map(|binding| frozen(binding.origin.clone()))
        .collect();
    let mut history = snapshot(project.path(), entry.resource_id, origins)?;
    history.compaction_frontier_seq = Some(Seq::try_new(1)?);
    let replayed = prepare_v1_user_media(&history, Some(&first), registry).await?;
    // The earlier prompt keeps attachment text instead of a resource object.
    // 较早提示词保留附件文字而非资源对象。
    assert!(!metadata(&replayed, 0).is_empty());
    assert_eq!(
        native(&replayed, 1).source_kind,
        ResourceSourceKind::InlineData
    );
    Ok(())
}
