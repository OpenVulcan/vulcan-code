use super::*;

/// Builds one valid per-provider sync state for conditional-Index tests.
/// 为条件式 Index 测试构建一份有效逐供应商同步状态。
///
/// # Parameters
/// 参数
/// `revision` and `digest_byte` create a distinct active identity; `index_etag` is the exact
/// shared remote Index identity under test.
/// `revision` 与 `digest_byte` 创建不同活动身份；`index_etag` 是待测的精确共享远端
/// Index 身份。
///
/// # Returns
/// 返回值
/// One structurally valid bounded state accepted by the manifest store.
/// 返回一份 Manifest Store 接受的结构有效有界状态。
fn conditional_index_state(
    revision: u64,
    digest_byte: char,
    index_etag: &str,
) -> vulcan_provider_config::manifest_store::ManifestSyncState {
    vulcan_provider_config::manifest_store::ManifestSyncState {
        revision,
        sha256: digest_byte.to_string().repeat(64),
        previous: None,
        etag: None,
        index_etag: Some(index_etag.to_string()),
        last_modified: None,
        synced_at_unix_ms: revision,
        last_error_kind: None,
    }
}

/// Verifies a provider 304 updates validators without inventing a new activation time.
/// 验证供应商 304 只更新验证器，不虚构新的激活时间。
#[test]
fn provider_not_modified_preserves_successful_activation_time() {
    let mut current = conditional_index_state(7, 'a', "\"index-old\"");
    current.synced_at_unix_ms = 123_456;
    current.last_error_kind = Some(String::from("transient_failure"));

    let next = provider_not_modified_sync_state(&current, Some(String::from("\"index-new\"")));

    assert_eq!(next.revision, current.revision);
    assert_eq!(next.sha256, current.sha256);
    assert_eq!(next.synced_at_unix_ms, 123_456);
    assert_eq!(next.index_etag.as_deref(), Some("\"index-new\""));
    assert!(next.last_error_kind.is_none());
}

/// Verifies batch synchronization uses a conditional ETag only for complete consensus.
/// 验证批量同步仅在状态完整一致时使用条件式 ETag。
#[test]
fn shared_index_etag_requires_complete_provider_consensus() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let service = ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        Some(String::from("https://example.test/index.json")),
    );
    let providers = [String::from("alpha"), String::from("beta")];
    service
        .store
        .save_sync_state("alpha", &conditional_index_state(1, 'a', "\"index-7\""))
        .expect("alpha state should save");
    assert_eq!(
        service.shared_index_etag(&providers),
        None,
        "an absent provider state must force an unconditional fetch"
    );
    service
        .store
        .save_sync_state("beta", &conditional_index_state(2, 'b', "\"index-7\""))
        .expect("beta state should save");
    assert_eq!(
        service.shared_index_etag(&providers).as_deref(),
        Some("\"index-7\"")
    );
    service
        .store
        .save_sync_state("beta", &conditional_index_state(2, 'b', "\"index-8\""))
        .expect("divergent beta state should save");
    assert_eq!(
        service.shared_index_etag(&providers),
        None,
        "divergent authenticated Index observations must not be guessed"
    );
}

/// Verifies a batch conditional request also requires every pinned Active layer to revalidate.
/// 验证批量条件请求还要求每个固定的 Active 层均可重新校验。
#[test]
fn shared_revalidated_index_etag_requires_complete_active_layers() {
    // directory isolates the durable manifest store used by this proof.
    // directory 隔离本次证明所使用的持久 Manifest 存储。
    let directory = tempfile::tempdir().expect("temp dir should create");
    // service has no live signing keys because this test exercises only local admission.
    // service 不需要实时签名密钥，因为本测试只执行本地准入判断。
    let service = ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        Some(String::from("https://example.test/index.json")),
    );
    // providerId selects one real bundled system manifest accepted by the strict compiler.
    // providerId 选择一份可被严格编译器接受的真实内置系统 Manifest。
    let provider_id = "alibaba";
    // activeText is the exact trusted local document whose revision and digest enter state.
    // activeText 是修订与摘要写入状态的精确可信本地文档。
    let active_text = std::fs::read_to_string(
        vulcan_runtime_assets::resolve_bundled_provider_manifests_directory()
            .join(format!("{provider_id}.json")),
    )
    .expect("bundled provider manifest should read");
    // revision is read from the authoritative document rather than duplicated in the test.
    // revision 从权威文档读取，而不是在测试中重复声明。
    let revision = serde_json::from_str::<serde_json::Value>(&active_text)
        .expect("bundled manifest should parse")["revision"]
        .as_u64()
        .expect("bundled manifest revision should be unsigned");
    // state pins the exact Active bytes and one shared authenticated Index validator.
    // state 固定精确 Active 字节与一个共享认证 Index 验证器。
    let mut state = conditional_index_state(revision, 'a', "\"index-7\"");
    state.sha256 = ManifestStoreLayout::sha256_hex(active_text.as_bytes());
    service
        .store
        .save_sync_state(provider_id, &state)
        .expect("sync state should save");
    let providers = [provider_id.to_string()];
    assert_eq!(
        service.shared_revalidated_index_etag(&providers),
        None,
        "state without its Active document must force one unconditional batch fetch"
    );

    service
        .store
        .stage_candidate(provider_id, revision, active_text.as_bytes())
        .expect("candidate should stage");
    service
        .store
        .activate(provider_id, revision)
        .expect("candidate should activate");
    assert_eq!(
        service.shared_revalidated_index_etag(&providers).as_deref(),
        Some("\"index-7\"")
    );

    // Corrupting the Active bytes after state persistence must disable the 304 optimization.
    // 在状态持久化后破坏 Active 字节必须禁用 304 优化。
    std::fs::write(service.store.active_path(provider_id), b"{}")
        .expect("active corruption should write");
    assert_eq!(service.shared_revalidated_index_etag(&providers), None);
}

/// Verifies sync accepts only the exact JSON media type with optional parameters.
/// 验证同步只接受带可选参数的精确 JSON 媒体类型。
#[test]
fn sync_content_type_rejects_json_prefix_lookalikes() {
    assert!(is_supported_sync_content_type("application/json"));
    assert!(is_supported_sync_content_type(
        "Application/JSON; charset=utf-8"
    ));
    assert!(!is_supported_sync_content_type("application/jsonp"));
    assert!(!is_supported_sync_content_type("application/json-seq"));
    assert!(!is_supported_sync_content_type("text/json"));
}

/// Verifies sync diagnostics never retain credentials, signed query material, or fragments.
/// 验证同步诊断绝不保留凭据、签名查询材料或片段。
#[test]
fn sync_url_diagnostics_remove_secret_bearing_components() {
    let redacted = redacted_sync_url(
        "https://user:secret@manifests.example.com/v1/index.json?signature=private#fragment",
    );
    assert_eq!(redacted, "https://manifests.example.com/v1/index.json");
    assert!(!redacted.contains("user"));
    assert!(!redacted.contains("secret"));
    assert!(!redacted.contains("signature"));
    assert_eq!(
        redacted_sync_url("not a URL?signature=private"),
        "<invalid-remote-url>"
    );
}

/// Verifies public manifest diagnostics redact credentials and machine-local paths.
/// 验证公开 Manifest 诊断会脱敏凭据与机器本地路径。
#[test]
fn public_manifest_diagnostics_are_secret_safe_and_utf8_bounded() {
    assert_eq!(
        bounded_public_manifest_diagnostic("failed to read C:\\Users\\operator\\manifest.json"),
        "<redacted-local-path>"
    );
    assert_eq!(
        bounded_public_manifest_diagnostic("request token=private-value failed"),
        "<redacted>"
    );
    assert_eq!(
        bounded_public_manifest_diagnostic(
            r#"provider rejected {"authorization":"private-value"}"#
        ),
        "<redacted>"
    );
    assert_eq!(
        bounded_public_manifest_diagnostic("failed to read /home/operator/manifest.json"),
        "<redacted-local-path>"
    );
    assert_eq!(
        bounded_public_manifest_diagnostic("https://user:secret@example.test/index.json"),
        "<redacted-url>"
    );
    let oversized = "清单".repeat(MANIFEST_PREVIEW_DIAGNOSTIC_MAX_BYTES);
    let bounded = bounded_public_manifest_diagnostic(oversized.as_str());
    assert!(bounded.len() <= MANIFEST_PREVIEW_DIAGNOSTIC_MAX_BYTES);
    assert!(bounded.ends_with("..."));
    assert!(std::str::from_utf8(bounded.as_bytes()).is_ok());
    // report proves the serialized sync DTO reuses the same public diagnostic boundary.
    // report 证明序列化同步 DTO 复用同一公开诊断边界。
    let report = provider_sync_failure_report(
        "deepseek",
        String::from("failed to read C:\\Users\\operator\\active.json"),
    );
    assert_eq!(
        report.failure_message.as_deref(),
        Some("<redacted-local-path>")
    );
}

/// Verifies activation and compensated rollback are structurally mutually exclusive terminals.
/// 验证激活与已补偿回滚在结构上是相互排斥的终态。
#[test]
fn activation_telemetry_terminal_states_are_mutually_exclusive() {
    // activated is the exact event emitted only after every projection commits.
    // activated 是仅在全部投影提交后发出的精确事件。
    let activated = manifest_activation_terminal_event(
        "deepseek",
        7,
        11,
        ManifestActivationTelemetryTerminal::Activated {
            source: "active",
            sha256: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        },
    );
    // rolledBack is the exact event emitted only after every before-image is restored.
    // rolledBack 是仅在全部前镜像恢复后发出的精确事件。
    let rolled_back = manifest_activation_terminal_event(
        "deepseek",
        7,
        12,
        ManifestActivationTelemetryTerminal::RolledBack {
            failure_kind: "workbench_projection_failed",
        },
    );

    activated
        .validate()
        .expect("activation event should validate");
    rolled_back
        .validate()
        .expect("rollback event should validate");
    assert_eq!(
        activated.event_name,
        ProviderOperationalTelemetryEventName::ProviderManifestActivated
    );
    assert_eq!(activated.manifest_source.as_deref(), Some("active"));
    assert!(activated.failure_kind.is_none());
    assert_eq!(
        rolled_back.event_name,
        ProviderOperationalTelemetryEventName::ProviderManifestPostActivationRolledBack
    );
    assert!(rolled_back.manifest_source.is_none());
    assert_eq!(
        rolled_back.failure_kind.as_deref(),
        Some("workbench_projection_failed")
    );
    assert_ne!(
        activated.event_name.as_str(),
        rolled_back.event_name.as_str()
    );
}

/// Verifies an early index failure closes with one stable typed failure and no configured URL.
/// 验证提前 Index 失败以一个稳定类型化失败闭合，且不包含配置 URL。
#[test]
fn index_telemetry_guard_closes_early_failure_without_remote_url() {
    // providerId isolates this process-wide history assertion from every production provider.
    // providerId 将本进程级历史断言与所有生产供应商隔离。
    let provider_id = "telemetry-index-guard-contract-test";
    {
        // guard deliberately drops before normal completion to exercise the error boundary.
        // guard 故意在正常完成前释放，用于验证错误边界。
        let _guard = ManifestIndexCheckTelemetryGuard::start(vec![provider_id.to_string()]);
    }
    // events are newest-first and contain completed, failed, then started.
    // events 按新到旧排列，依次包含 completed、failed 与 started。
    let events = provider_operational_telemetry_snapshot(provider_id, 8)
        .expect("telemetry snapshot should read");
    assert_eq!(events.len(), 3);
    assert_eq!(
        events[0].event_name,
        ProviderOperationalTelemetryEventName::ManifestIndexCheckCompleted
    );
    assert_eq!(
        events[0].failure_kind.as_deref(),
        Some("index_check_failed")
    );
    assert_eq!(
        events[1].event_name,
        ProviderOperationalTelemetryEventName::ManifestIndexCheckFailed
    );
    assert_eq!(
        events[1].failure_kind.as_deref(),
        Some("index_check_failed")
    );
    let serialized = serde_json::to_string(&events).expect("events should serialize");
    assert!(!serialized.contains("://"));
    assert!(!serialized.to_ascii_lowercase().contains("authorization"));
}

/// Verifies a later batch abort never reclassifies an already completed provider check.
/// 验证后续批次中止绝不会重新分类已经完成的供应商检查。
#[test]
fn index_telemetry_guard_preserves_completed_provider_terminal_state() {
    // completedProviderId and pendingProviderId isolate both terminal branches in global history.
    // completedProviderId 与 pendingProviderId 在全局历史中隔离两条终态分支。
    let completed_provider_id = "telemetry-completed-provider-contract-test";
    let pending_provider_id = "telemetry-pending-provider-contract-test";
    {
        // guard resolves the shared index, completes the first provider, then simulates abort.
        // guard 解析共享 Index、完成首个供应商，随后模拟批次中止。
        let mut guard = ManifestIndexCheckTelemetryGuard::start(vec![
            completed_provider_id.to_string(),
            pending_provider_id.to_string(),
        ]);
        guard.mark_index_resolved();
        guard.complete_provider(&ProviderSyncReport {
            provider_id: completed_provider_id.to_string(),
            outcome: "up_to_date",
            revision: 1,
            failure_kind: None,
            failure_message: None,
            activation_snapshot: None,
        });
    }
    let completed_events = provider_operational_telemetry_snapshot(completed_provider_id, 8)
        .expect("completed provider telemetry should read");
    assert!(completed_events.iter().all(|event| {
        event.event_name != ProviderOperationalTelemetryEventName::ManifestIndexCheckFailed
    }));
    assert!(completed_events.iter().any(|event| {
        event.event_name == ProviderOperationalTelemetryEventName::ManifestIndexCheckCompleted
            && event.failure_kind.is_none()
    }));

    let pending_events = provider_operational_telemetry_snapshot(pending_provider_id, 8)
        .expect("pending provider telemetry should read");
    assert!(pending_events.iter().any(|event| {
        event.event_name == ProviderOperationalTelemetryEventName::ManifestIndexCheckFailed
            && event.failure_kind.as_deref() == Some("provider_check_aborted")
    }));
    assert!(pending_events.iter().any(|event| {
        event.event_name == ProviderOperationalTelemetryEventName::ManifestIndexCheckCompleted
            && event.failure_kind.as_deref() == Some("provider_check_aborted")
    }));
}

/// Verifies candidate rejection telemetry retains the compiler-owned field path as data.
/// 验证候选拒绝遥测把编译器权威字段路径作为结构化数据保留。
#[test]
fn candidate_rejection_telemetry_retains_typed_compiler_path() {
    // providerId isolates this process-wide event from every production provider.
    // providerId 将这条进程级事件与所有生产供应商隔离。
    let provider_id = "telemetry-candidate-path-contract-test";
    emit_provider_candidate_event(
        ProviderOperationalTelemetryEventName::ProviderCandidateRejected,
        provider_id,
        19,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        Some("schema_semantic_failed"),
        Some("$.model_rules[0].selected_protocol"),
    );
    // event is the exact newest rejection emitted above.
    // event 是上方发出的精确最新拒绝事件。
    let event = provider_operational_telemetry_snapshot(provider_id, 1)
        .expect("candidate telemetry snapshot should read")
        .into_iter()
        .next()
        .expect("candidate rejection event should exist");
    assert_eq!(
        event.event_name,
        ProviderOperationalTelemetryEventName::ProviderCandidateRejected
    );
    assert_eq!(
        event.failure_kind.as_deref(),
        Some("schema_semantic_failed")
    );
    assert_eq!(
        event.failure_path.as_deref(),
        Some("$.model_rules[0].selected_protocol")
    );
}

/// Verifies status-facing operational history remains bounded under repeated observations.
/// 验证面向状态的运行历史在重复观测下保持有界。
#[test]
fn status_telemetry_window_is_bounded() {
    // providerId isolates the bounded ring query from other provider tests.
    // providerId 将有界环查询与其他供应商测试隔离。
    let provider_id = "telemetry-status-window-contract-test";
    for _ in 0..(MANIFEST_STATUS_TELEMETRY_MAX_ITEMS + 8) {
        emit_manifest_index_event(
            ProviderOperationalTelemetryEventName::ManifestIndexCheckNotModified,
            provider_id,
            None,
        );
    }
    // events applies the exact status response cap even when more observations exist.
    // events 即使存在更多观测，也应用精确状态响应上限。
    let events =
        provider_operational_telemetry_snapshot(provider_id, MANIFEST_STATUS_TELEMETRY_MAX_ITEMS)
            .expect("bounded telemetry snapshot should read");
    assert_eq!(events.len(), MANIFEST_STATUS_TELEMETRY_MAX_ITEMS);
}

/// Supplies one immutable API key to signed manifest catalog activation tests.
/// 为签名清单目录激活测试提供一个不可变 API Key。
#[derive(Clone)]
struct SignedE2eSecretStore {
    /// Exact versioned test secret returned for every reference.
    /// 为每个引用返回的精确版本化测试秘密。
    secret: vulcan_provider_adapters::CredentialSecretEnvelope,
    /// One-shot barrier armed immediately before an in-flight generation-freeze assertion.
    /// 在执行中代际冻结断言前立即启用的一次性屏障。
    block_next_read: Arc<std::sync::atomic::AtomicBool>,
    /// Signals that the request has resolved its immutable route and reached secret loading.
    /// 表示请求已解析不可变路由并到达秘密加载阶段。
    read_blocked: Arc<tokio::sync::Notify>,
    /// Releases the one request paused after immutable route resolution.
    /// 释放在不可变路由解析后暂停的唯一请求。
    read_release: Arc<tokio::sync::Notify>,
}

impl SignedE2eSecretStore {
    /// Builds one deterministic revision-one API key store.
    /// 构建一个确定性的第一修订版 API Key 存储。
    ///
    /// # Returns
    /// 返回值
    /// A store suitable for static provider discovery without real credentials.
    /// 返回适用于静态供应商发现且不含真实凭据的存储。
    fn new() -> Self {
        Self {
            secret: vulcan_provider_adapters::CredentialSecretEnvelope {
                version: 1,
                revision: 1,
                material: vulcan_provider_adapters::CredentialSecretMaterial::ApiKey {
                    value: String::from("sk-sp-signed-manifest-e2e"),
                },
            },
            block_next_read: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            read_blocked: Arc::new(tokio::sync::Notify::new()),
            read_release: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Arms the next secret read as the deterministic in-flight request boundary.
    /// 将下一次秘密读取设置为确定性的执行中请求边界。
    fn arm_next_read_barrier(&self) {
        self.block_next_read
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Waits until the armed request has captured its immutable provider generation.
    /// 等待已启用屏障的请求捕获其不可变供应商代际。
    async fn wait_until_read_blocked(&self) {
        self.read_blocked.notified().await;
    }

    /// Releases the request paused at the secret-read boundary.
    /// 释放暂停在秘密读取边界的请求。
    fn release_blocked_read(&self) {
        self.read_release.notify_one();
    }
}

#[async_trait::async_trait]
impl vulcan_provider_adapters::CredentialSecretResolver for SignedE2eSecretStore {
    /// Returns the immutable fixture secret for every opaque reference.
    /// 为每个不透明引用返回不可变夹具秘密。
    async fn read_optional_secret(
        &self,
        _secret_reference: &str,
    ) -> Result<Option<vulcan_provider_adapters::CredentialSecretEnvelope>, String> {
        if self
            .block_next_read
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            self.read_blocked.notify_one();
            self.read_release.notified().await;
        }
        Ok(Some(self.secret.clone()))
    }
}

#[async_trait::async_trait]
impl vulcan_provider_adapters::CredentialSecretStore for SignedE2eSecretStore {
    /// Rejects mutation because the E2E fixture is immutable.
    /// 拒绝变更，因为 E2E 夹具不可变。
    async fn create_secret(
        &self,
        _secret_reference: &str,
        _secret: &vulcan_provider_adapters::CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err(String::from("The signed E2E secret store is immutable."))
    }

    /// Rejects replacement because the E2E fixture is immutable.
    /// 拒绝替换，因为 E2E 夹具不可变。
    async fn replace_secret(
        &self,
        _secret_reference: &str,
        _expected_revision: u64,
        _secret: &vulcan_provider_adapters::CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err(String::from("The signed E2E secret store is immutable."))
    }

    /// Rejects deletion because the E2E fixture is immutable.
    /// 拒绝删除，因为 E2E 夹具不可变。
    async fn delete_secret(&self, _secret_reference: &str) -> Result<(), String> {
        Err(String::from("The signed E2E secret store is immutable."))
    }

    /// Rejects restoration because the E2E fixture is immutable.
    /// 拒绝恢复，因为 E2E 夹具不可变。
    async fn restore_secret(
        &self,
        _secret_reference: &str,
        _secret: &vulcan_provider_adapters::CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err(String::from("The signed E2E secret store is immutable."))
    }
}

/// Builds one Alibaba manifest revision with exact additional static models.
/// 构建一份包含精确新增静态模型的 Alibaba 清单修订版。
///
/// # Parameters
/// 参数
/// `revision` is the signed remote revision, `model_ids` and their bounded data-only rules are
/// appended to Model Studio, `media_model_ids` bind reviewed media templates, and
/// `unauthorized_host` injects
/// one trust-ceiling violation when true.
/// `revision` 是签名远端修订号，`model_ids` 及其有界纯数据规则被追加到 Model Studio，
/// `media_model_ids` 绑定已审核媒体模板；`unauthorized_host` 为真时注入一个信任上限违规项。
///
/// # Returns
/// 返回值
/// Canonical test JSON bytes ready for digest pinning and remote activation.
/// 返回可直接进行摘要固定与远端激活的规范测试 JSON 字节。
fn alibaba_manifest_e2e_bytes(
    revision: u64,
    model_ids: &[&str],
    media_model_ids: &[&str],
    unauthorized_host: bool,
) -> Vec<u8> {
    let text = std::fs::read_to_string(
        vulcan_runtime_assets::resolve_bundled_provider_manifests_directory().join("alibaba.json"),
    )
    .expect("bundled Alibaba manifest must exist");
    let mut document =
        serde_json::from_str::<serde_json::Value>(&text).expect("manifest should parse");
    document["revision"] = serde_json::Value::from(revision);
    let services = document["services"]
        .as_array_mut()
        .expect("Alibaba services must be an array");
    let model_studio = services
        .iter_mut()
        .find(|service| service["service_entry_id"] == "model-studio-cn")
        .expect("Alibaba Model Studio entry must exist");
    let models = model_studio["model_catalog"]["models"]
        .as_array_mut()
        .expect("Alibaba static model catalog must be an array");
    for model_id in model_ids {
        models.push(serde_json::json!({
            "model_id": model_id,
            "display_name": model_id
        }));
    }
    if unauthorized_host {
        model_studio["base_url"] =
            serde_json::Value::String(String::from("https://unauthorized.example.test"));
    }
    // base_rule is reviewed executable authority; each remote rule changes identity and exact
    // model matching data only.
    // base_rule 是已审核可执行权限；每条远端规则只修改身份与精确模型匹配数据。
    let base_rule = document["model_rules"]
        .as_array()
        .expect("Alibaba model rules must be an array")
        .iter()
        .find(|rule| {
            rule["matcher"]["value"] == "qwen3.7-plus"
                && rule["scope"]["service_entry_ids"]
                    .as_array()
                    .is_some_and(|services| {
                        services.iter().any(|service| service == "model-studio-cn")
                    })
        })
        .cloned()
        .expect("Alibaba reviewed Qwen conversation rule must exist");
    // rules receives one exact bounded rule for every newly published conversation model.
    // rules 为每个新发布对话模型接收一条精确有界规则。
    let rules = document["model_rules"]
        .as_array_mut()
        .expect("Alibaba model rules must be mutable");
    for model_id in model_ids {
        let mut rule = base_rule.clone();
        rule["rule_id"] = serde_json::Value::String(format!("signed-e2e-{model_id}"));
        rule["matcher"]["value"] = serde_json::Value::String((*model_id).to_string());
        rules.push(rule);
    }
    // bindings is the independent media/tool model catalog and intentionally does not
    // pollute the conversation model catalog.
    // bindings 是独立的媒体/工具模型目录，刻意不污染对话模型目录。
    let bindings = document["tool_target_template_bindings"]
        .as_array_mut()
        .expect("Alibaba template bindings must be an array");
    for model_id in media_model_ids {
        bindings.push(serde_json::json!({
            "service_entry_id": "model-studio-cn",
            "model_id": model_id,
            "display_name": format!("{model_id} Generate"),
            "template_refs": ["alibaba.model-studio-cn.qwen-image-generate.v1"]
        }));
    }
    let bytes = serde_json::to_vec(&document).expect("manifest should serialize");
    let text = std::str::from_utf8(&bytes).expect("manifest should remain UTF-8");
    vulcan_provider_adapters::compile_manifest_text_with_layer(
        text,
        vulcan_provider_adapters::ManifestOwnership::System,
        vulcan_provider_adapters::ManifestSourceLayer::Active,
    )
    .expect("generated signed E2E manifest must compile");
    bytes
}

/// Starts one two-request signed HTTPS endpoint and returns its configured sync service.
/// 启动一个处理两次请求的签名 HTTPS 端点，并返回已配置的同步服务。
///
/// # Parameters
/// 参数
/// `app_data_root` owns durable sync state, `provider_id` and `revision` identify the pin,
/// `provider_bytes` are the exact downloaded bytes, and `index_revision` distinguishes runs.
/// `app_data_root` 持有持久同步状态，`provider_id` 与 `revision` 标识固定版本，
/// `provider_bytes` 是精确下载字节，`index_revision` 区分各次运行。
///
/// # Returns
/// 返回值
/// The configured service plus the server task that completes after Index and provider reads.
/// 返回已配置服务，以及在 Index 与供应商读取后结束的服务器任务。
async fn signed_https_sync_service(
    app_data_root: &Path,
    provider_id: &str,
    revision: u64,
    provider_bytes: Vec<u8>,
    index_revision: u64,
) -> (ProviderManifestSyncService, tokio::task::JoinHandle<()>) {
    use sha2::Digest;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind signed E2E loopback");
    let index_url = format!(
        "https://{}/index.json",
        listener.local_addr().expect("loopback address")
    );
    let digest = sha2::Sha256::digest(&provider_bytes);
    let digest_hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let mut seed = [0_u8; 32];
    seed[..10].copy_from_slice(b"signed-e2e");
    let key_id = String::from("signed-e2e-key");
    let now = current_unix_millis() as i64;
    let index = vulcan_provider_config::manifest_sync::ManifestRemoteIndex {
        schema_version: vulcan_provider_config::manifest_sync::INDEX_SCHEMA_VERSION.to_string(),
        index_revision,
        issued_at_unix_ms: now - 1_000,
        expires_at_unix_ms: now + 60_000,
        signing_key_id: key_id.clone(),
        providers: std::collections::BTreeMap::from([(
            provider_id.to_string(),
            vulcan_provider_config::manifest_sync::RemoteProviderEntry {
                provider_id: provider_id.to_string(),
                revision,
                relative_url: format!("/{provider_id}.json"),
                sha256: digest_hex,
                bytes: provider_bytes.len() as u64,
                minimum_engine_contract: 1,
            },
        )]),
    };
    let index_bytes = serde_json::to_vec(&index).expect("index should encode");
    let signature = vulcan_provider_config::manifest_sync::sign_index_document(&index_bytes, &seed)
        .expect("index should sign");
    let signature_hex = signature
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    let mut cert_params =
        rcgen::CertificateParams::new(Vec::<String>::new()).expect("certificate params");
    cert_params.subject_alt_names = vec![rcgen::SanType::IpAddress(std::net::IpAddr::V4(
        std::net::Ipv4Addr::LOCALHOST,
    ))];
    let key_pair = rcgen::KeyPair::generate().expect("certificate key");
    let generated = cert_params.self_signed(&key_pair).expect("certificate");
    let tls_config = tokio_rustls::rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![generated.der().clone()],
            tokio_rustls::rustls::pki_types::PrivateKeyDer::try_from(key_pair.serialize_der())
                .expect("private key der"),
        )
        .expect("TLS config");
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls_config));
    let provider_path = format!("/{provider_id}.json");
    let server = tokio::spawn(async move {
        for _request_index in 0..2 {
            let (stream, _peer) = listener.accept().await.expect("accept signed request");
            let mut stream = acceptor.accept(stream).await.expect("TLS accept");
            let mut head = Vec::new();
            let mut byte = [0_u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                let read = stream.read(&mut byte).await.expect("request read");
                if read == 0 {
                    break;
                }
                head.push(byte[0]);
            }
            let head_text = String::from_utf8_lossy(&head);
            // Every index and provider request must advertise the same executable client
            // boundary so remote delivery cannot depend on an implicit release guess.
            // 每次 Index 与供应商请求都必须声明同一可执行客户端边界，远端分发不能依赖隐式版本猜测。
            let lower_head = head_text.to_ascii_lowercase();
            assert!(lower_head.contains(&format!(
                "{SYNC_CLIENT_VERSION_HEADER}: {}",
                env!("CARGO_PKG_VERSION")
            )));
            assert!(lower_head.contains(&format!(
                "{SYNC_MANIFEST_SCHEMA_HEADER}: {}",
                vulcan_provider_adapters::provider_manifest::MANIFEST_SCHEMA_VERSION
            )));
            assert!(lower_head.contains(&format!(
                "{SYNC_MANIFEST_ENGINE_HEADER}: {}",
                vulcan_provider_adapters::provider_manifest::CURRENT_MANIFEST_ENGINE
            )));
            let path = head_text.split_whitespace().nth(1).unwrap_or("/");
            let (headers, body) = if path == "/index.json" {
                (
                    format!(
                        "Content-Type: application/json\r\nx-vulcan-manifest-signature: {signature_hex}\r\nETag: \"index-{index_revision}\"\r\n"
                    ),
                    index_bytes.as_slice(),
                )
            } else if path == provider_path {
                (
                    format!("Content-Type: application/json\r\nETag: \"provider-{revision}\"\r\n"),
                    provider_bytes.as_slice(),
                )
            } else {
                (String::new(), &[][..])
            };
            let status = if path == "/index.json" || path == provider_path {
                "200 OK"
            } else {
                "404 Not Found"
            };
            stream
                    .write_all(
                        format!(
                            "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .expect("response head write");
            stream.write_all(body).await.expect("response body write");
            let _ = stream.shutdown().await;
        }
    });
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("signed E2E client");
    let trusted = TrustedSigningKey {
        key_id,
        public_key: vulcan_provider_config::manifest_sync::public_key_for_seed(&seed),
    };
    (
        ProviderManifestSyncService::new(app_data_root, client, vec![trusted], Some(index_url)),
        server,
    )
}

/// Starts a reviewed-host HTTPS loopback for two generated-image requests.
/// 为两次文生图请求启动使用已审核主机名的 HTTPS 回环服务。
///
/// The injected client resolves only `dashscope.aliyuncs.com` to the ephemeral listener and
/// trusts only this fixture certificate. The production manifest endpoint remains unchanged,
/// so the test exercises the exact authority admitted by the bundled trust ceiling.
/// 注入客户端只把 `dashscope.aliyuncs.com` 解析到临时监听器，并且只信任当前夹具证书。
/// 生产 Manifest 端点保持不变，因此测试执行的是内置信任上限准入的精确权限。
///
/// # Returns
/// 返回值
/// The bounded HTTP client, a receiver for exact request paths/bodies, and the server task.
/// 返回有界 HTTP 客户端、精确请求路径/正文接收器与服务器任务。
async fn provider_tool_https_fixture() -> (
    reqwest::Client,
    tokio::sync::mpsc::UnboundedReceiver<(String, serde_json::Value)>,
    tokio::task::JoinHandle<()>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
    // listener owns the only socket to which the reviewed production hostname is resolved.
    // listener 持有已审核生产主机名所解析到的唯一套接字。
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind provider-tool E2E loopback");
    // listener_address freezes the ephemeral port before the server task takes ownership.
    // listener_address 在服务器任务取得所有权前冻结临时端口。
    let listener_address = listener
        .local_addr()
        .expect("provider-tool loopback address");
    // certificate_parameters binds the certificate to the exact reviewed manifest hostname.
    // certificate_parameters 把证书绑定到 Manifest 已审核的精确主机名。
    let certificate_parameters =
        rcgen::CertificateParams::new(vec![String::from("dashscope.aliyuncs.com")])
            .expect("provider-tool certificate params");
    // certificate_key is unique fixture key material and never leaves this test process.
    // certificate_key 是唯一夹具密钥材料，绝不离开当前测试进程。
    let certificate_key = rcgen::KeyPair::generate().expect("provider-tool certificate key");
    // generated_certificate supplies both the rustls chain and the isolated client trust root.
    // generated_certificate 同时提供 rustls 证书链与隔离的客户端信任根。
    let generated_certificate = certificate_parameters
        .self_signed(&certificate_key)
        .expect("provider-tool certificate");
    // client_certificate is the sole additional trust anchor for the injected client.
    // client_certificate 是注入客户端唯一额外信任锚。
    let client_certificate = reqwest::Certificate::from_der(generated_certificate.der().as_ref())
        .expect("provider-tool client certificate");
    // tls_configuration rejects client certificates and serves only the generated chain.
    // tls_configuration 拒绝客户端证书且只提供生成的证书链。
    let tls_configuration = tokio_rustls::rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![generated_certificate.der().clone()],
            tokio_rustls::rustls::pki_types::PrivateKeyDer::try_from(
                certificate_key.serialize_der(),
            )
            .expect("provider-tool private key DER"),
        )
        .expect("provider-tool TLS config");
    // tls_acceptor is shared across the two deterministic request connections.
    // tls_acceptor 在两次确定性请求连接之间共享。
    let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls_configuration));
    // request channel preserves exact request order for old/new generation assertions.
    // 请求通道为旧/新代际断言保留精确请求顺序。
    let (request_sender, request_receiver) = tokio::sync::mpsc::unbounded_channel();
    // server terminates after exactly two provider-tool requests.
    // server 在恰好两次供应商工具请求后终止。
    let server = tokio::spawn(async move {
        for _request_index in 0..2 {
            // accepted_stream is one reviewed-host TLS transport.
            // accepted_stream 是一条已审核主机 TLS 传输。
            let (accepted_stream, _peer) = listener
                .accept()
                .await
                .expect("accept provider-tool request");
            // tls_stream owns the fully authenticated request/response exchange.
            // tls_stream 持有完成认证的请求/响应交换。
            let mut tls_stream = tls_acceptor
                .accept(accepted_stream)
                .await
                .expect("accept provider-tool TLS");
            // request_head accumulates bytes only through the HTTP header terminator.
            // request_head 只累积到 HTTP 头终止符为止。
            let mut request_head = Vec::new();
            // next_byte is the exact one-byte framing buffer used by this bounded fixture.
            // next_byte 是该有界夹具使用的精确单字节分帧缓冲区。
            let mut next_byte = [0_u8; 1];
            while !request_head.ends_with(b"\r\n\r\n") {
                // read_count distinguishes a closed connection from one received byte.
                // read_count 区分已关闭连接与收到的一个字节。
                let read_count = tls_stream
                    .read(&mut next_byte)
                    .await
                    .expect("read provider-tool request head");
                if read_count == 0 {
                    break;
                }
                request_head.push(next_byte[0]);
            }
            // request_head_text is used only for bounded request-line and content-length parsing.
            // request_head_text 仅用于有界解析请求行与 Content-Length。
            let request_head_text =
                String::from_utf8(request_head).expect("provider-tool request head must be UTF-8");
            // request_path captures the exact endpoint path selected from the frozen target.
            // request_path 捕获从冻结目标选择的精确端点路径。
            let request_path = request_head_text
                .split_whitespace()
                .nth(1)
                .expect("provider-tool request path")
                .to_string();
            // content_length is the exact JSON body length declared by reqwest.
            // content_length 是 reqwest 声明的精确 JSON 正文长度。
            let content_length = request_head_text
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .expect("provider-tool content length");
            // request_body_bytes receives exactly the declared request body and no next frame.
            // request_body_bytes 恰好接收声明的请求正文且不会读取下一帧。
            let mut request_body_bytes = vec![0_u8; content_length];
            tls_stream
                .read_exact(&mut request_body_bytes)
                .await
                .expect("read provider-tool request body");
            // request_body is the projected provider-native JSON body under test.
            // request_body 是被测的供应商原生投影 JSON 正文。
            let request_body = serde_json::from_slice::<serde_json::Value>(&request_body_bytes)
                .expect("provider-tool request body must be JSON");
            // response_model echoes the exact generation-owned upstream model into the result URL.
            // response_model 把代际持有的精确上游模型回显到结果 URL。
            let response_model = request_body["model"]
                .as_str()
                .expect("provider-tool request model")
                .to_string();
            request_sender
                .send((request_path, request_body))
                .expect("provider-tool request receiver must remain open");
            // response_body is a valid Alibaba image envelope consumed by the reviewed profile.
            // response_body 是由已审核 Profile 消费的有效阿里云图片响应封装。
            let response_body = serde_json::to_vec(&serde_json::json!({
                    "request_id": format!("request-{response_model}"),
                    "output": {
                        "choices": [{
                            "message": {
                                "content": [{
                                    "image": format!("https://dashscope.aliyuncs.com/{response_model}.png")
                                }]
                            }
                        }]
                    }
                }))
                .expect("provider-tool response should encode");
            // response_head closes the connection after one complete JSON response.
            // response_head 在一个完整 JSON 响应后关闭连接。
            let response_head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response_body.len()
            );
            tls_stream
                .write_all(response_head.as_bytes())
                .await
                .expect("write provider-tool response head");
            tls_stream
                .write_all(&response_body)
                .await
                .expect("write provider-tool response body");
            let _ = tls_stream.shutdown().await;
        }
    });
    // client disables ambient proxies and resolves only the reviewed host to this fixture.
    // client 禁用环境代理，并且只把已审核主机解析到当前夹具。
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .add_root_certificate(client_certificate)
        .resolve("dashscope.aliyuncs.com", listener_address)
        .build()
        .expect("provider-tool E2E client");
    (client, request_receiver, server)
}

/// Returns the revision carried by the currently bundled DeepSeek manifest.
/// 返回当前随包 DeepSeek 清单携带的修订号。
fn bundled_deepseek_revision() -> u64 {
    let text = std::fs::read_to_string(
        vulcan_runtime_assets::resolve_bundled_provider_manifests_directory().join("deepseek.json"),
    )
    .expect("bundled deepseek manifest must exist");
    serde_json::from_str::<serde_json::Value>(&text).expect("manifest should parse")["revision"]
        .as_u64()
        .expect("bundled revision must be an unsigned integer")
}

/// Loads the bundled DeepSeek document with one exact test revision.
/// 加载带有精确测试修订号的随包 DeepSeek 文档。
fn deepseek_manifest_with_revision(revision: u64) -> String {
    let text = std::fs::read_to_string(
        vulcan_runtime_assets::resolve_bundled_provider_manifests_directory().join("deepseek.json"),
    )
    .expect("bundled deepseek manifest must exist");
    let mut value: serde_json::Value = serde_json::from_str(&text).expect("manifest should parse");
    value["revision"] = serde_json::json!(revision);
    serde_json::to_string(&value).expect("manifest should serialize")
}

/// Loads one DeepSeek revision and appends an exact model rule for status-delta tests.
/// 加载一个 DeepSeek 修订版，并为状态差异测试追加一条精确模型规则。
///
/// # Parameters
/// 参数
/// `revision` is the candidate Manifest revision and `model_id` is the exact new identity.
/// `revision` 是候选 Manifest 修订号；`model_id` 是精确的新身份。
///
/// # Returns
/// 返回值
/// Valid system-manifest JSON containing the additional exact model rule.
/// 返回包含新增精确模型规则的有效系统 Manifest JSON。
fn deepseek_manifest_with_exact_model(revision: u64, model_id: &str) -> String {
    // value owns the revision-adjusted bundled document.
    // value 持有已调整修订号的随包文档。
    let mut value = serde_json::from_str::<serde_json::Value>(
        deepseek_manifest_with_revision(revision).as_str(),
    )
    .expect("revision-adjusted DeepSeek manifest should parse");
    // rules is the exact mutable model-rule inventory.
    // rules 是精确的可变模型规则清单。
    let rules = value["model_rules"]
        .as_array_mut()
        .expect("DeepSeek model rules must be an array");
    // rule clones a complete exact-rule contract and changes only its stable identity.
    // rule 克隆一份完整精确规则契约，并只改变其稳定身份。
    let mut rule = rules
        .iter()
        .find(|rule| rule["matcher"]["kind"] == "exact")
        .cloned()
        .expect("DeepSeek must include an exact model rule");
    rule["rule_id"] = serde_json::Value::String(format!("status-{model_id}"));
    rule["display_name"] = serde_json::Value::String(String::from("Status Delta Model"));
    rule["matcher"]["value"] = serde_json::Value::String(model_id.to_string());
    rules.push(rule);
    serde_json::to_string(&value).expect("status-delta manifest should serialize")
}

/// Activates one DeepSeek test revision through the authenticated sync transaction.
/// 通过可信同步事务激活一个 DeepSeek 测试修订。
fn activate_trusted_deepseek(store: &ManifestStoreLayout, revision: u64, text: &str) {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(text.as_bytes());
    let entry = vulcan_provider_config::manifest_sync::RemoteProviderEntry {
        provider_id: String::from("deepseek"),
        revision,
        relative_url: String::from("/deepseek.json"),
        sha256: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
        bytes: text.len() as u64,
        minimum_engine_contract: 1,
    };
    let outcome = activate_synced_provider(store, &entry, text.as_bytes(), None, None, None)
        .expect("trusted test manifest should activate");
    assert!(
        matches!(
            outcome,
            vulcan_provider_config::manifest_sync::SyncOutcome::Activated { .. }
        ),
        "test activation must publish a new revision"
    );
}

/// Builds one isolated store with a staged and activated provider manifest.
/// 构建一个隔离存储并 staging + 激活一份供应商清单。
fn store_with_activated_deepseek() -> (ManifestStoreLayout, tempfile::TempDir, u64, String) {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let store = ManifestStoreLayout::new(directory.path());
    let revision = bundled_deepseek_revision() + 1;
    let text = deepseek_manifest_with_revision(revision);
    activate_trusted_deepseek(&store, revision, &text);
    (store, directory, revision, text)
}

/// Verifies automatic synchronization starts only after both external inputs exist.
/// 验证自动同步仅在两个外部输入同时存在后启动。
#[test]
fn periodic_sync_configuration_gate_requires_url_and_key() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let unconfigured = ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        None,
    );
    assert!(!unconfigured.is_configured());
    let configured = ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        vec![TrustedSigningKey {
            key_id: String::from("test-key"),
            public_key: [7_u8; 32],
        }],
        Some(String::from("https://updates.example/index.json")),
    );
    assert!(configured.is_configured());
}

/// Verifies an untrusted RPC identity cannot allocate a retained provider flight.
/// 验证不可信 RPC 身份无法分配被保留的供应商 Flight。
#[tokio::test]
async fn single_provider_sync_rejects_unknown_identity_before_flight_allocation() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let service = ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        None,
    );
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");

    let error = service
        .sync_one(&runtime, "unknown-system-provider")
        .await
        .expect_err("unknown provider identity must fail before sync configuration checks");

    assert!(error.contains("trusted system provider identifier"));
    assert!(service.provider_flights.lock().await.is_empty());
}

/// Verifies success cadence and exponential retry remain inside their jitter bounds.
/// 验证成功周期与指数重试始终位于各自抖动边界内。
#[test]
fn periodic_sync_delay_is_bounded_and_capped() {
    let success_low = periodic_sync_delay(0, 0).as_secs();
    let success_high = periodic_sync_delay(0, 4_320).as_secs();
    assert_eq!(success_low, 19_440);
    assert_eq!(success_high, 23_760);

    let first_failure_low = periodic_sync_delay(1, 0).as_secs();
    let first_failure_high = periodic_sync_delay(1, 12).as_secs();
    assert_eq!(first_failure_low, 54);
    assert_eq!(first_failure_high, 66);

    let capped_low = periodic_sync_delay(20, 0).as_secs();
    let capped_high = periodic_sync_delay(20, 720).as_secs();
    assert_eq!(capped_low, 3_240);
    assert_eq!(capped_high, 3_960);
}

/// Verifies shutdown cancellation aborts the exact registered periodic task.
/// 验证关闭取消会中止精确注册的周期任务。
#[tokio::test]
async fn periodic_sync_task_is_cancellable() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let service = ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        None,
    );
    let handle = tokio::spawn(std::future::pending::<()>());
    service
        .install_periodic_task(handle.abort_handle())
        .expect("periodic task should register");
    service
        .stop_periodic_task()
        .expect("periodic task should stop");
    let join_error = handle
        .await
        .expect_err("registered periodic task must be cancelled");
    assert!(join_error.is_cancelled());
}

/// Verifies the activation guard spans the complete cross-layer projection transaction.
/// 验证激活守卫覆盖完整的跨层投影事务。
#[tokio::test]
async fn activation_transaction_guard_is_single_writer() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let service = std::sync::Arc::new(ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        None,
    ));
    let first_guard = service.begin_activation_transaction().await;
    let (acquired_tx, acquired_rx) = tokio::sync::oneshot::channel();
    let service_clone = std::sync::Arc::clone(&service);
    let waiter = tokio::spawn(async move {
        let _second_guard = service_clone.begin_activation_transaction().await;
        let _ = acquired_tx.send(());
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), acquired_rx)
            .await
            .is_err(),
        "a second activation must remain blocked through the first projection window"
    );
    drop(first_guard);
    tokio::time::timeout(std::time::Duration::from_secs(2), waiter)
        .await
        .expect("the waiting activation should resume")
        .expect("the waiting activation task should not panic");
}

/// Verifies the startup recovery chain restores a persisted active layer.
/// 验证启动恢复链恢复持久化 active 层。
#[tokio::test]
async fn startup_restores_persisted_active_layer() {
    let (_store, directory, expected_revision, text) = store_with_activated_deepseek();
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let reports = restore_system_manifests_from_store(&runtime, directory.path())
        .expect("restore should succeed");
    runtime
        .activate_staged_manifest_catalogs(&PersistedAiConfiguration::default(), &[])
        .await
        .expect("startup projection must publish the restored generation");
    let report = reports
        .iter()
        .find(|report| report.provider_id == "deepseek")
        .expect("deepseek must be restored");
    assert_eq!(report.layer, "active");
    let revision = serde_json::from_str::<serde_json::Value>(&text)
        .expect("manifest should parse")
        .get("revision")
        .and_then(|value| value.as_u64())
        .expect("revision must exist");
    assert_eq!(report.revision, revision);
    assert_eq!(revision, expected_revision);
    // The runtime catalog now carries the restored revision.
    // 运行时目录现在携带恢复的修订号。
    let runtime_revisions = runtime.system_manifest_revisions().expect("revisions");
    assert!(
        runtime_revisions
            .iter()
            .any(|(id, rev)| id == "deepseek" && *rev == revision),
        "runtime must observe the restored manifest"
    );
}

/// Verifies startup consumes an Applying journal before trusting Active or sync state.
/// 验证启动会在信任 Active 或同步状态前消费 Applying 日志。
#[tokio::test]
async fn startup_recovers_uncommitted_activation_before_layer_selection() {
    use sha2::Digest;

    let (store, directory, previous_revision, previous_text) = store_with_activated_deepseek();
    let candidate_revision = previous_revision + 1;
    let candidate_text = deepseek_manifest_with_revision(candidate_revision);
    // digest pins the exact uncommitted candidate bytes.
    // digest 固定精确的未提交候选字节。
    let digest = sha2::Sha256::digest(candidate_text.as_bytes());
    let entry = vulcan_provider_config::manifest_sync::RemoteProviderEntry {
        provider_id: String::from("deepseek"),
        revision: candidate_revision,
        relative_url: String::from("/deepseek.json"),
        sha256: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
        bytes: candidate_text.len() as u64,
        minimum_engine_contract: 1,
    };
    let outcome = stage_synced_provider_activation(
        &store,
        &entry,
        candidate_text.as_bytes(),
        None,
        None,
        None,
    )
    .expect("uncommitted activation should stage");
    assert!(matches!(
        outcome,
        vulcan_provider_config::manifest_sync::SyncOutcome::Activated { .. }
    ));
    assert_eq!(
        store.read_active("deepseek").expect("active should read"),
        Some(candidate_text),
        "candidate should be visible only until recovery consumes the Applying journal"
    );

    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let reports = restore_system_manifests_from_store(&runtime, directory.path())
        .expect("startup recovery should succeed");
    runtime
        .activate_staged_manifest_catalogs(&PersistedAiConfiguration::default(), &[])
        .await
        .expect("recovered startup generation should publish");
    let report = reports
        .iter()
        .find(|report| report.provider_id == "deepseek")
        .expect("recovered Active layer should report");
    assert_eq!(report.layer, "active");
    assert_eq!(report.revision, previous_revision);
    assert_eq!(
        store
            .read_active("deepseek")
            .expect("recovered Active should read"),
        Some(previous_text)
    );
}

/// Verifies the production single-provider path leaves its file transaction recoverable.
/// 验证生产单供应商路径会让文件事务保持可恢复状态。
#[tokio::test]
async fn production_single_sync_stays_applying_until_projection_finalization() {
    // directory owns the staged provider files and transaction journal.
    // directory 持有已暂存供应商文件与事务日志。
    let directory = tempfile::tempdir().expect("temp dir should create");
    // runtime owns the staged manifest catalog before full projection publication.
    // runtime 持有完整投影发布前的暂存 Manifest 目录。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    // revision is strictly newer than the immutable bundled baseline.
    // revision 严格晚于不可变内置基线。
    let revision = bundled_deepseek_revision() + 1;
    // candidate_bytes are the exact signed provider document served by the local fixture.
    // candidate_bytes 是本地夹具提供的精确签名供应商文档。
    let candidate_bytes = deepseek_manifest_with_revision(revision).into_bytes();
    // service and server provide the reviewed signed HTTPS synchronization boundary.
    // service 与 server 提供已审核的签名 HTTPS 同步边界。
    let (service, server) =
        signed_https_sync_service(directory.path(), "deepseek", revision, candidate_bytes, 1).await;

    // report must retain the exact before-image needed by the projection finalizer.
    // report 必须保留投影完成器所需的精确前镜像。
    let report = service
        .sync_one(&runtime, "deepseek")
        .await
        .expect("production single sync should stage");
    assert_eq!(report.outcome, "activated");
    assert!(report.activation_snapshot.is_some());

    // store emulates process-start recovery before any projection finalizer commits the journal.
    // store 模拟任何投影完成器提交日志前的进程启动恢复。
    let store = ManifestStoreLayout::new(directory.path());
    assert!(
        store
            .recover_system_transaction("deepseek")
            .expect("Applying journal should recover"),
        "the production path must retain an Applying journal"
    );
    assert_eq!(
        store.read_active("deepseek").expect("active should read"),
        None,
        "recovery must restore the exact absent Active before-image"
    );

    server.await.expect("signed server should finish");
}

/// Verifies the recovery chain quarantines an invalid active layer and restores previous.
/// 验证恢复链隔离无效 active 层并恢复 previous 层。
#[tokio::test]
async fn startup_restores_previous_after_invalid_active() {
    let (_store, directory, previous_revision, previous_text) = store_with_activated_deepseek();
    let store = ManifestStoreLayout::new(directory.path());
    let active_revision = previous_revision + 1;
    // Rotate the first authenticated revision into Previous by activating its successor.
    // 通过激活后一修订，把首个已认证修订轮换到 Previous。
    let second = deepseek_manifest_with_revision(active_revision);
    activate_trusted_deepseek(&store, active_revision, &second);
    // Corrupt the active layer.
    // 破坏 active 层。
    std::fs::write(store.active_path("deepseek"), br#"{"broken":true}"#)
        .expect("corrupt active should write");
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let reports = restore_system_manifests_from_store(&runtime, directory.path())
        .expect("restore should succeed");
    let report = reports
        .iter()
        .find(|report| report.provider_id == "deepseek")
        .expect("deepseek must be restored");
    assert_eq!(report.layer, "previous");
    assert_eq!(report.revision, previous_revision);
    assert!(
        store
            .quarantine_path("deepseek", active_revision, "active_invalid")
            .exists(),
        "invalid active bytes must be quarantined"
    );
    runtime
        .activate_staged_manifest_catalogs(&PersistedAiConfiguration::default(), &[])
        .await
        .expect("startup projection must publish the Previous generation");
    emit_published_startup_manifest_restore_events(&runtime, &reports)
        .expect("published Previous restore telemetry should emit");
    let expected_previous_sha256 = ManifestStoreLayout::sha256_hex(previous_text.as_bytes());
    let fallback_event = provider_operational_telemetry_snapshot("deepseek", usize::MAX)
        .expect("fallback telemetry should remain readable")
        .into_iter()
        .find(|event| {
            event.event_name
                == ProviderOperationalTelemetryEventName::ProviderManifestFallbackSelected
                && event.manifest_revision == Some(previous_revision)
                && event.failure_kind.as_deref() == Some("active_invalid")
        })
        .expect("previous recovery must emit one exact fallback event");
    assert_eq!(fallback_event.manifest_source.as_deref(), Some("previous"));
    assert_eq!(
        fallback_event.manifest_generation,
        Some(runtime.catalog_generation())
    );
    assert_eq!(
        fallback_event.manifest_sha256.as_deref(),
        Some(expected_previous_sha256.as_str()),
        "fallback telemetry must use the verified Previous bytes rather than the old published generation"
    );
}

/// Verifies corrupt generated sync state falls back per provider instead of aborting startup.
/// 验证损坏的生成同步状态按供应商回退，而不是中止启动。
#[test]
fn startup_isolates_invalid_sync_state_bookkeeping() {
    // directory isolates the generated state corruption from the user's application data.
    // directory 把生成状态损坏与用户应用数据隔离。
    let directory = tempfile::tempdir().expect("temp dir should create");
    // store owns the exact encoded state path used by production recovery.
    // store 持有生产恢复使用的精确编码状态路径。
    let store = ManifestStoreLayout::new(directory.path());
    // statePath is created under its fixed generated-state directory.
    // statePath 创建在其固定生成状态目录下。
    let state_path = store.sync_state_path("deepseek");
    std::fs::create_dir_all(state_path.parent().expect("state parent must exist"))
        .expect("state directory should create");
    std::fs::write(&state_path, br#"{"unknown":true}"#)
        .expect("invalid state fixture should write");
    // runtime starts from the independent trusted bundled baseline.
    // runtime 从独立可信内置基线启动。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let bundled_revision = runtime
        .system_manifest("deepseek")
        .expect("bundled DeepSeek must exist")
        .revision;

    let reports = restore_system_manifests_from_store(&runtime, directory.path())
        .expect("one corrupt provider state must not abort startup");
    assert!(
        reports
            .iter()
            .all(|report| report.provider_id != "deepseek"),
        "invalid state cannot authenticate a persisted restore"
    );
    assert!(
        !state_path.exists(),
        "invalid generated state must be removed"
    );
    assert_eq!(
        runtime
            .system_manifest("deepseek")
            .expect("bundled fallback must remain executable")
            .revision,
        bundled_revision
    );
}

/// Verifies unauthenticated Active bytes are removed and never restored during startup.
/// 验证未认证的 Active 字节会在启动时移除且绝不会被恢复。
#[test]
fn startup_removes_active_layer_without_authenticated_state() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let store = ManifestStoreLayout::new(directory.path());
    let active_path = store.active_path("deepseek");
    std::fs::create_dir_all(active_path.parent().expect("active parent must exist"))
        .expect("active directory should create");
    let unauthenticated_revision = bundled_deepseek_revision() + 1;
    std::fs::write(
        &active_path,
        deepseek_manifest_with_revision(unauthenticated_revision),
    )
    .expect("orphan active fixture should write");
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let bundled_revision = runtime
        .system_manifest("deepseek")
        .expect("bundled DeepSeek must exist")
        .revision;

    let reports = restore_system_manifests_from_store(&runtime, directory.path())
        .expect("orphan provider bytes must not abort startup");

    assert!(!active_path.exists());
    assert!(
        reports
            .iter()
            .all(|report| report.provider_id != "deepseek")
    );
    assert_eq!(
        runtime
            .system_manifest("deepseek")
            .expect("bundled fallback must remain executable")
            .revision,
        bundled_revision
    );
}

/// Verifies one corrupt provider status file cannot suppress every other provider status.
/// 验证一个损坏的供应商状态文件不能压制其他全部供应商状态。
#[test]
fn aggregate_status_contains_provider_local_projection_failure() {
    // directory contains one deliberately invalid independent check-state document.
    // directory 包含一份刻意无效的独立检查状态文档。
    let directory = tempfile::tempdir().expect("temp dir should create");
    let service = ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        None,
    );
    let check_state_path = service.store.sync_check_state_path("deepseek");
    std::fs::create_dir_all(
        check_state_path
            .parent()
            .expect("check-state parent must exist"),
    )
    .expect("check-state directory should create");
    std::fs::write(&check_state_path, br#"{"unknown":true}"#)
        .expect("invalid check state fixture should write");
    // runtime retains the executable bundled layer while detailed status projection fails.
    // runtime 在详细状态投影失败时仍保留可执行内置层。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");

    let statuses = service
        .provider_manifest_status_all(&runtime)
        .expect("aggregate status must remain available");
    let deepseek = statuses
        .iter()
        .find(|status| status.provider_id == "deepseek")
        .expect("DeepSeek status must remain present");
    assert_eq!(
        deepseek
            .status_error
            .as_ref()
            .map(|error| error.failure_kind.as_str()),
        Some("status_projection_failed")
    );
    assert!(deepseek.active.is_some());
    assert!(deepseek.critical_error.is_none());
    assert!(
        statuses
            .iter()
            .any(|status| status.provider_id == "openrouter" && status.status_error.is_none()),
        "an unrelated healthy provider must retain complete status"
    );
}

/// Verifies an unconfigured sync service reports `not_configured` without any network.
/// 验证未配置的同步服务报告 `not_configured` 且不发起任何网络请求。
#[test]
fn unconfigured_sync_reports_not_configured() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let http = build_sync_http_client().expect("sync client");
    let service = ProviderManifestSyncService::new(directory.path(), http, Vec::new(), None);
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let runtime = std::sync::Arc::new(runtime);
    let report = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("async runtime")
        .block_on(service.sync_provider(&runtime, "deepseek"))
        .expect("not configured must not fail");
    assert_eq!(report.outcome, "not_configured");
}

/// Verifies a non-HTTPS index URL is rejected before any request.
/// 验证非 HTTPS Index URL 在任何请求前被拒绝。
#[test]
fn sync_rejects_insecure_index_url() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let service = ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        Some(String::from("http://insecure.example.com/index.json")),
    );
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let runtime = std::sync::Arc::new(runtime);
    let error = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("async runtime")
        .block_on(service.sync_provider(&runtime, "deepseek"))
        .expect_err("insecure index URL must fail");
    assert!(error.contains("HTTPS"), "{error}");
}

/// Verifies concurrent unconfigured requests return safely without network or state writes.
/// 验证并发的未配置请求能在不触网、不写状态的前提下安全返回。
#[tokio::test]
async fn concurrent_unconfigured_syncs_are_side_effect_free() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let service = std::sync::Arc::new(ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        None,
    ));
    let runtime =
        std::sync::Arc::new(PublicAiRuntime::unavailable().expect("runtime should initialize"));
    let (first_tx, first_rx) = tokio::sync::oneshot::channel();
    let (second_tx, second_rx) = tokio::sync::oneshot::channel();
    let service_clone = std::sync::Arc::clone(&service);
    let runtime_clone = std::sync::Arc::clone(&runtime);
    let first = tokio::spawn(async move {
        let _ = first_tx.send(());
        service_clone
            .sync_provider(&runtime_clone, "deepseek")
            .await
    });
    let service_clone = std::sync::Arc::clone(&service);
    let runtime_clone = std::sync::Arc::clone(&runtime);
    let second = tokio::spawn(async move {
        let _ = second_tx.send(());
        service_clone
            .sync_provider(&runtime_clone, "deepseek")
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), first_rx)
        .await
        .expect("first sync should start")
        .expect("first signal should send");
    tokio::time::timeout(std::time::Duration::from_secs(2), second_rx)
        .await
        .expect("second sync should start")
        .expect("second signal should send");
    let first_report = tokio::time::timeout(std::time::Duration::from_secs(5), first)
        .await
        .expect("first sync should finish")
        .expect("first task should not panic")
        .expect("first sync should succeed");
    let second_report = tokio::time::timeout(std::time::Duration::from_secs(5), second)
        .await
        .expect("second sync should finish")
        .expect("second task should not panic")
        .expect("second sync should succeed");
    // Both syncs stop before any network, store, or activation boundary.
    // 两个同步都在任何网络、存储或激活边界之前停止。
    assert_eq!(first_report.outcome, "not_configured");
    assert_eq!(second_report.outcome, "not_configured");
}

/// Verifies a corrupted provider state cannot abort the remaining full-catalog sync.
/// 验证一个供应商状态损坏时不能中止其余供应商的全目录同步。
///
/// Business invariant (plan §R7.7): one report is returned for every requested provider;
/// a local failure is classified as `sync_failed`, while later providers still download,
/// verify, and activate from the same signed Index snapshot.
/// 业务不变量（方案 §R7.7）：每个请求的供应商都必须返回一份报告；本地失败分类为
/// `sync_failed`，后续供应商仍从同一签名 Index 快照完成下载、校验与激活。
#[tokio::test]
async fn sync_all_isolates_provider_state_failure_and_continues() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let store = ManifestStoreLayout::new(directory.path());
    let broken_state_path = store.sync_state_path("broken-provider");
    std::fs::create_dir_all(
        broken_state_path
            .parent()
            .expect("sync-state path must have a parent"),
    )
    .expect("sync-state directory should create");
    std::fs::write(&broken_state_path, b"{not-json")
        .expect("corrupted provider state should write");

    let bundled_text = std::fs::read_to_string(
        vulcan_runtime_assets::resolve_bundled_provider_manifests_directory().join("alibaba.json"),
    )
    .expect("bundled Alibaba manifest must exist");
    let bundled_revision = serde_json::from_str::<serde_json::Value>(&bundled_text)
        .expect("bundled Alibaba manifest should parse")["revision"]
        .as_u64()
        .expect("bundled Alibaba revision must be an unsigned integer");
    let remote_revision = bundled_revision + 1;
    let provider_bytes = alibaba_manifest_e2e_bytes(remote_revision, &[], &[], false);
    let (service, server) = signed_https_sync_service(
        directory.path(),
        "alibaba",
        remote_revision,
        provider_bytes,
        1,
    )
    .await;
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    // finalizedProviderIds records the exact production-handler order.
    // finalizedProviderIds 记录精确的生产处理器调用顺序。
    let finalized_provider_ids = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    // handlerProviderIds gives the move closure shared access to the ordered evidence.
    // handlerProviderIds 让 move 闭包共享访问有序证据。
    let handler_provider_ids = std::sync::Arc::clone(&finalized_provider_ids);
    // alibabaActivePath proves the next provider has not crossed its store boundary yet.
    // alibabaActivePath 证明下一个供应商尚未越过其存储边界。
    let alibaba_active_path = store.active_path("alibaba");
    let reports = service
            .sync_all_with_report_handler(
                &runtime,
                &[String::from("broken-provider"), String::from("alibaba")],
                move |report| {
                    // providerIds and activePath are request-local handler evidence.
                    // providerIds 与 activePath 是逐次处理器调用的局部证据。
                    let provider_ids = std::sync::Arc::clone(&handler_provider_ids);
                    let active_path = alibaba_active_path.clone();
                    async move {
                        if report.provider_id == "broken-provider" {
                            assert!(
                                !active_path.exists(),
                                "the next provider must not stage before the current report is finalized"
                            );
                        }
                        provider_ids
                            .lock()
                            .expect("handler order lock should remain available")
                            .push(report.provider_id.clone());
                        Ok(report)
                    }
                },
            )
            .await
            .expect("one provider failure must not abort the full sync");

    assert_eq!(reports.len(), 2, "every provider must receive one report");
    assert_eq!(reports[0].provider_id, "broken-provider");
    assert_eq!(reports[0].outcome, "rejected");
    assert_eq!(reports[0].failure_kind.as_deref(), Some("sync_failed"));
    assert!(
        reports[0]
            .failure_message
            .as_deref()
            .is_some_and(|message| message.contains("invalid JSON")),
        "the isolated report must retain the local state failure"
    );
    assert_eq!(reports[1].provider_id, "alibaba");
    assert_eq!(reports[1].outcome, "activated");
    assert_eq!(reports[1].revision, remote_revision);
    assert_eq!(
        *finalized_provider_ids
            .lock()
            .expect("handler order lock should remain available"),
        [String::from("broken-provider"), String::from("alibaba")],
        "every provider must finish its handler before the next provider is staged"
    );
    server.await.expect("signed server should finish");
}

/// Verifies candidate preview performs the full trust/compile/diff flow without any store write.
/// 验证候选预览执行完整信任/编译/差异流程且不产生任何存储写入。
#[tokio::test]
async fn signed_candidate_preview_is_strictly_read_only() {
    // directory is the exact app-data root that must remain untouched by preview.
    // directory 是预览必须保持不变的精确应用数据根目录。
    let directory = tempfile::tempdir().expect("temp dir should create");
    // store resolves every path whose absence proves no hidden staging/state mutation occurred.
    // store 解析每条路径，其缺失证明未发生隐藏 staging/状态修改。
    let store = ManifestStoreLayout::new(directory.path());
    // bundledText supplies the current trusted revision used to create a strictly newer candidate.
    // bundledText 提供用于创建严格较新候选的当前可信修订。
    let bundled_text = std::fs::read_to_string(
        vulcan_runtime_assets::resolve_bundled_provider_manifests_directory().join("alibaba.json"),
    )
    .expect("bundled Alibaba manifest must exist");
    // bundledRevision is the immutable current runtime baseline.
    // bundledRevision 是不可变当前运行时基线。
    let bundled_revision = serde_json::from_str::<serde_json::Value>(&bundled_text)
        .expect("bundled Alibaba manifest should parse")["revision"]
        .as_u64()
        .expect("bundled Alibaba revision must be an unsigned integer");
    // candidateRevision is strictly monotonic and therefore eligible for semantic preview.
    // candidateRevision 严格单调，因此适用于语义预览。
    let candidate_revision = bundled_revision + 1;
    // candidateBytes add one exact model/rule so the diff has a deterministic added binding.
    // candidateBytes 新增一个精确模型/规则，使差异包含确定性新增绑定。
    let candidate_bytes =
        alibaba_manifest_e2e_bytes(candidate_revision, &["qwen-preview-only"], &[], false);
    // service and server execute the same signed HTTPS index/provider reads as production.
    // service 与 server 执行和生产相同的签名 HTTPS Index/供应商读取。
    let (service, server) = signed_https_sync_service(
        directory.path(),
        "alibaba",
        candidate_revision,
        candidate_bytes,
        1,
    )
    .await;
    // runtime owns the bundled Alibaba comparison snapshot.
    // runtime 持有内置 Alibaba 比较快照。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    assert!(
        !store.manifests_root().exists(),
        "precondition: app-data manifest root must be absent"
    );
    // preview is the verified in-memory candidate diff.
    // preview 是已验证的内存候选差异。
    let baseline = service
        .capture_preview_baseline(&runtime, "alibaba")
        .expect("preview baseline should capture");
    let preview = service
        .preview_provider_manifest(baseline, "alibaba")
        .await
        .expect("candidate preview should succeed");
    assert_eq!(preview.outcome, ProviderManifestPreviewOutcome::Candidate);
    assert!(preview.executable);
    assert_eq!(preview.candidate_revision, Some(candidate_revision));
    assert!(
        preview.model_binding_changes.iter().any(|delta| {
            delta.change == ProviderManifestChangeKind::Added
                && delta
                    .candidate
                    .as_ref()
                    .is_some_and(|binding| binding.model_id == "qwen-preview-only")
        }),
        "the new exact model binding must be visible in the backend-owned diff"
    );
    assert!(
        !store.manifests_root().exists(),
        "preview must not create staging, Active, Previous, quarantine, sync, or check state"
    );
    server.await.expect("signed server should finish");
}

/// R7: full signed remote sync chain over a real HTTPS loopback signing server (plan §R7.14).
/// R7：在真实 HTTPS 回环签名服务器上执行完整签名远端同步链路（方案 §R7.14）。
///
/// The local signing server serves a signed index and a pinned provider manifest; the
/// sync service fetches both over TLS, verifies the Ed25519 signature, activates the
/// provider, hot-replaces the runtime catalog, and short-circuits on the second
/// conditional sync via ETag.
/// 本地签名服务器提供签名 Index 与固定供应商清单；同步服务通过 TLS 获取两者、验签
/// Ed25519 签名、激活供应商、热替换运行时目录，并在第二次条件同步时经 ETag 短路。
#[tokio::test]
async fn signed_remote_sync_e2e_over_https_loopback() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // The test build enables both rustls provider features; install one explicitly so
    // the TLS acceptor and the sync client share a deterministic provider.
    // 测试构建同时启用了两个 rustls provider 特性；显式安装一个，使 TLS 接受器与
    // 同步客户端共享确定性 provider。
    let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();

    let directory = tempfile::tempdir().expect("temp dir should create");

    // Deterministic trusted signing key pair.
    // 确定性受信签名密钥对。
    let mut seed = [0_u8; 32];
    seed[..5].copy_from_slice(b"e2e-1");
    let public_key = vulcan_provider_config::manifest_sync::public_key_for_seed(&seed);
    let trusted = TrustedSigningKey {
        key_id: String::from("e2e-key-1"),
        public_key,
    };

    let remote_revision = bundled_deepseek_revision() + 5;
    // A revision above the bundled trust ceiling is the exact pinned provider document.
    // 高于随包信任下限的修订作为精确固定的供应商文档。
    let provider_bytes = deepseek_manifest_with_revision(remote_revision).into_bytes();
    let digest = {
        use sha2::Digest;
        sha2::Sha256::digest(&provider_bytes)
    };
    let digest_hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let now = current_unix_millis() as i64;
    let index = vulcan_provider_config::manifest_sync::ManifestRemoteIndex {
        schema_version: vulcan_provider_config::manifest_sync::INDEX_SCHEMA_VERSION.to_string(),
        index_revision: 1,
        issued_at_unix_ms: now - 1000,
        expires_at_unix_ms: now + 1000,
        signing_key_id: String::from("e2e-key-1"),
        providers: std::collections::BTreeMap::from([(
            String::from("deepseek"),
            vulcan_provider_config::manifest_sync::RemoteProviderEntry {
                provider_id: String::from("deepseek"),
                revision: remote_revision,
                relative_url: String::from("/deepseek.json"),
                sha256: digest_hex,
                bytes: provider_bytes.len() as u64,
                minimum_engine_contract: 1,
            },
        )]),
    };
    let index_bytes = serde_json::to_vec(&index).expect("index should encode");
    let signature = vulcan_provider_config::manifest_sync::sign_index_document(&index_bytes, &seed)
        .expect("index should sign");
    let signature_hex = signature
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        signature_hex.len(),
        128,
        "signature header is 128 hex chars"
    );

    // Self-signed certificate for the loopback HTTPS server.
    // 回环 HTTPS 服务器的自签名证书。
    let mut cert_params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("cert params");
    cert_params.subject_alt_names = vec![rcgen::SanType::IpAddress(std::net::IpAddr::V4(
        std::net::Ipv4Addr::LOCALHOST,
    ))];
    let key_pair = rcgen::KeyPair::generate().expect("key pair");
    let generated = cert_params.self_signed(&key_pair).expect("certificate");
    let tls_config = tokio_rustls::rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![generated.der().clone()],
            tokio_rustls::rustls::pki_types::PrivateKeyDer::try_from(key_pair.serialize_der())
                .expect("private key der"),
        )
        .expect("TLS config");
    let acceptor = tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(tls_config));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let index_url = format!(
        "https://{}/index.json",
        listener.local_addr().expect("loopback addr")
    );

    // The signing server answers: signed index 200, provider 200, then a 304 for the
    // conditional second sync; the loop ends after the three expected connections.
    // 签名服务器应答：签名 Index 200、供应商 200，随后为第二次条件同步应答 304；
    // 三次预期连接后循环结束。
    let index_bytes_for_server = index_bytes.clone();
    let provider_bytes_for_server = provider_bytes.clone();
    let signature_hex_for_server = signature_hex.clone();
    let server = tokio::spawn(async move {
        let mut handled = 0_u32;
        while handled < 3 {
            let (stream, _peer) = listener.accept().await.expect("accept");
            let mut stream = acceptor.accept(stream).await.expect("TLS accept");
            // Read the HTTP request head until the blank line.
            // 读取 HTTP 请求头直到空行。
            let mut head = Vec::new();
            let mut byte = [0_u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                let read = stream.read(&mut byte).await.expect("request read");
                if read == 0 {
                    break;
                }
                head.push(byte[0]);
            }
            let head_text = String::from_utf8_lossy(&head);
            let path = head_text.split_whitespace().nth(1).unwrap_or("/");
            let conditional = head_text
                .lines()
                .any(|line| line.to_ascii_lowercase().starts_with("if-none-match:"));
            match (path, conditional) {
                ("/index.json", false) => {
                    stream
                            .write_all(
                                format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"e2e-index-v1\"\r\nx-vulcan-manifest-signature: {signature_hex_for_server}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    index_bytes_for_server.len()
                                )
                                .as_bytes(),
                            )
                            .await
                            .expect("index head write");
                    stream
                        .write_all(&index_bytes_for_server)
                        .await
                        .expect("index body write");
                }
                ("/index.json", true) => {
                    stream
                            .write_all(
                                b"HTTP/1.1 304 Not Modified\r\nETag: \"e2e-index-v1\"\r\nConnection: close\r\n\r\n",
                            )
                            .await
                            .expect("304 write");
                }
                ("/deepseek.json", _) => {
                    stream
                            .write_all(
                                format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"e2e-deepseek-v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    provider_bytes_for_server.len()
                                )
                                .as_bytes(),
                            )
                            .await
                            .expect("provider head write");
                    stream
                        .write_all(&provider_bytes_for_server)
                        .await
                        .expect("provider body write");
                }
                _ => {
                    stream
                            .write_all(
                                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                            )
                            .await
                            .expect("404 write");
                }
            }
            let _ = stream.shutdown().await;
            handled += 1;
        }
    });

    // The dedicated client accepts the self-signed loopback certificate but still
    // disables redirects and keeps the bounded timeouts of the production builder.
    // 专用客户端接受回环自签名证书，但仍禁用重定向并保持生产构建器的有界超时。
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("sync client");
    let service =
        ProviderManifestSyncService::new(directory.path(), client, vec![trusted], Some(index_url));
    let runtime = std::sync::Arc::new(PublicAiRuntime::unavailable().expect("runtime"));

    // First sync fetches, verifies, activates, and hot-replaces the runtime.
    // 第一次同步执行获取、验签、激活并热替换运行时。
    let first = service
        .sync_provider(&runtime, "deepseek")
        .await
        .expect("first sync should succeed");
    assert_eq!(first.outcome, "activated");
    assert_eq!(first.revision, remote_revision);
    runtime
        .activate_staged_manifest_catalogs(&PersistedAiConfiguration::default(), &[])
        .await
        .expect("the verified staged manifest must publish as one generation");
    assert_eq!(
        runtime
            .system_manifest_revisions()
            .expect("revisions")
            .into_iter()
            .find(|(provider_id, _)| provider_id == "deepseek")
            .expect("deepseek revision")
            .1,
        remote_revision,
        "the signed remote revision must become externally visible"
    );
    let store = ManifestStoreLayout::new(directory.path());
    assert!(
        store
            .read_active("deepseek")
            .expect("read active")
            .is_some(),
        "activated bytes must be persisted"
    );

    // Second sync reuses the persisted ETag and short-circuits on 304.
    // 第二次同步复用持久化 ETag 并在 304 时短路。
    let second = service
        .sync_provider(&runtime, "deepseek")
        .await
        .expect("second sync should succeed");
    assert_eq!(second.outcome, "up_to_date");
    assert_eq!(second.revision, remote_revision);

    tokio::time::timeout(std::time::Duration::from_secs(10), server)
        .await
        .expect("signing server should finish")
        .expect("server task should not panic");
}

/// Verifies signed JSON hot-adds conversation and media models, freezes in-flight execution,
/// rejects a bad successor, and rolls back atomically.
/// 验证签名 JSON 热新增对话与媒体模型、冻结执行中请求、拒绝坏后继版本并原子回滚。
#[tokio::test]
async fn signed_manifest_e2e_new_model_rejection_and_rollback() {
    // directory owns the authenticated Active/Previous/state transaction for this E2E.
    // directory 持有本次 E2E 的已认证 Active/Previous/state 事务。
    let directory = tempfile::tempdir().expect("temp dir should create");
    // provider transport preserves the reviewed production hostname while all traffic stays local.
    // 供应商传输保留已审核生产主机名，同时把全部流量约束在本地。
    let (provider_http, mut provider_requests, provider_server) =
        provider_tool_https_fixture().await;
    // secret_store exposes the deterministic request boundary used for the freeze assertion.
    // secret_store 暴露用于冻结断言的确定性请求边界。
    let secret_store = Arc::new(SignedE2eSecretStore::new());
    // runtime uses only the fixture-owned provider transport in this cross-crate E2E.
    // runtime 在本次跨 crate E2E 中只使用夹具持有的供应商传输。
    let runtime = Arc::new(
        PublicAiRuntime::first_batch_with_http_client_for_testing(
            secret_store.clone(),
            provider_http,
        )
        .expect("runtime should initialize"),
    );
    // configuration owns both the text route and the independent media-tool route.
    // configuration 同时持有文本路由与独立媒体工具路由。
    let mut configuration = PersistedAiConfiguration::default();
    // connection_id identifies the conversation catalog receiving JSON-only text models.
    // connection_id 标识接收纯 JSON 文本模型的对话目录。
    let connection_id = String::from("signed-alibaba-account");
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            vulcan_provider_adapters::CredentialConnection {
                id: connection_id.clone(),
                provider_id: String::from("alibaba"),
                service_entry_id: String::from("model-studio-cn"),
                credential_profile_id: String::from("api-key"),
                credential_mode: vulcan_provider_adapters::CredentialMode::Token,
                credential_id: String::from("signed-alibaba-credential"),
                display_name: String::from("Signed Alibaba Account"),
                secret_reference: String::from("signed-alibaba-secret"),
                management_secret_reference: None,
                service_origin_override: None,
            },
        )
        .expect("Alibaba connection should register");
    // media_connection_id identifies the reviewed Model Studio authority used by templates.
    // media_connection_id 标识模板使用的已审核 Model Studio 权限边界。
    let media_connection_id = String::from("signed-alibaba-media-account");
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            vulcan_provider_adapters::CredentialConnection {
                id: media_connection_id.clone(),
                provider_id: String::from("alibaba"),
                service_entry_id: String::from("model-studio-cn"),
                credential_profile_id: String::from("api-key"),
                credential_mode: vulcan_provider_adapters::CredentialMode::Token,
                credential_id: String::from("signed-alibaba-media-credential"),
                display_name: String::from("Signed Alibaba Media Account"),
                secret_reference: String::from("signed-alibaba-media-secret"),
                management_secret_reference: None,
                service_origin_override: None,
            },
        )
        .expect("Alibaba media connection should register");
    runtime
        .refresh_connection_catalog(&configuration, &connection_id)
        .await
        .expect("bundled static catalog should refresh");
    runtime
        .refresh_connection_catalog(&configuration, &media_connection_id)
        .await
        .expect("bundled media-service catalog should refresh");
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("bundled generation should publish");

    // affected_connection_ids are refreshed and published together for every revision.
    // affected_connection_ids 在每个修订版中一起刷新并发布。
    let affected_connection_ids = vec![connection_id.clone(), media_connection_id.clone()];

    let bundled_revision = runtime
        .system_manifest("alibaba")
        .expect("bundled Alibaba manifest should exist")
        .revision;
    let first_model = "qwen-signed-e2e-first";
    let second_model = "qwen-signed-e2e-second";
    // Media models belong to the independent template-binding catalog, not conversation discovery.
    // 媒体模型属于独立模板绑定目录，而非对话发现目录。
    let first_media_model = "qwen-image-signed-e2e-first";
    let second_media_model = "qwen-image-signed-e2e-second";
    let first_revision = bundled_revision + 1;
    let first_bytes =
        alibaba_manifest_e2e_bytes(first_revision, &[first_model], &[first_media_model], false);
    let first_digest = {
        use sha2::Digest;
        sha2::Sha256::digest(&first_bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    vulcan_provider_config::manifest_sync::verify_provider_bytes(
        &vulcan_provider_config::manifest_sync::RemoteProviderEntry {
            provider_id: String::from("alibaba"),
            revision: first_revision,
            relative_url: String::from("/alibaba.json"),
            sha256: first_digest,
            bytes: first_bytes.len() as u64,
            minimum_engine_contract: 1,
        },
        &first_bytes,
    )
    .expect("the first signed provider bytes must pass bundled verification");
    let (first_service, first_server) =
        signed_https_sync_service(directory.path(), "alibaba", first_revision, first_bytes, 1)
            .await;
    let first_report = first_service
        .sync_provider(&runtime, "alibaba")
        .await
        .expect("first signed sync should succeed");
    assert_eq!(
        first_report.outcome, "activated",
        "unexpected first sync report: {first_report:?}"
    );
    runtime
        .activate_staged_manifest_catalogs(&configuration, &affected_connection_ids)
        .await
        .expect("first signed generation should publish");
    first_server
        .await
        .expect("first signed server should finish");
    assert!(
        runtime.catalog_snapshot().expect("first catalog")[&connection_id]
            .models
            .iter()
            .any(|model| model.id == first_model),
        "the first signed JSON-only model must appear without a software update"
    );

    // first_text_selection proves direct declared-protocol routing without probing or downgrade.
    // first_text_selection 证明直接按声明协议路由，不存在探测或降级。
    let first_text_selection = runtime
        .resolve_connection_model_selection(&configuration, &connection_id, "alibaba", first_model)
        .expect("first signed conversation model must resolve");
    assert_eq!(
        first_text_selection
            .route
            .protocol_route
            .protocol_profile_id,
        vulcan_llm_core::OPENAI_CHAT_PROFILE_ID
    );
    // first_available_target is the deterministic instance expanded from the reviewed template.
    // first_available_target 是从已审核模板确定性展开的实例。
    let first_available_target = runtime
        .available_provider_tool_targets(&configuration)
        .expect("first signed tool targets should resolve")
        .into_iter()
        .find(|available| {
            available.connection_id == media_connection_id
                && available.target.upstream_target_id == first_media_model
        })
        .expect("first signed image-template instance must be available");
    // first_tool_selection freezes the exact credential and generated target identity.
    // first_tool_selection 冻结精确凭据与生成目标标识。
    let first_tool_selection = vulcan_provider_adapters::ProviderToolTargetSelection {
        connection_id: media_connection_id.clone(),
        target_id: first_available_target.target.target_id.clone(),
    };
    secret_store.arm_next_read_barrier();
    // old_request starts under revision one and pauses only after cloning generation-owned route data.
    // old_request 在第一修订版下启动，并且仅在克隆代际路由数据后暂停。
    let old_request = {
        // request_runtime retains the shared runtime while the task is in flight.
        // request_runtime 在任务执行期间持有共享运行时。
        let request_runtime = Arc::clone(&runtime);
        // request_configuration freezes the invocation's exact user configuration.
        // request_configuration 冻结调用的精确用户配置。
        let request_configuration = configuration.clone();
        // request_selection freezes the first generated target identifier.
        // request_selection 冻结第一份生成目标标识。
        let request_selection = first_tool_selection.clone();
        tokio::spawn(async move {
            request_runtime
                .execute_provider_tool_target_for_owner(
                    &request_configuration,
                    vulcan_provider_adapters::ProviderToolCapabilityKind::ImageGenerate,
                    &request_selection,
                    serde_json::json!({"prompt": "draw revision one"}),
                    None,
                )
                .await
        })
    };
    secret_store.wait_until_read_blocked().await;

    let second_revision = bundled_revision + 2;
    let second_bytes = alibaba_manifest_e2e_bytes(
        second_revision,
        &[first_model, second_model],
        &[first_media_model, second_media_model],
        false,
    );
    let (second_service, second_server) = signed_https_sync_service(
        directory.path(),
        "alibaba",
        second_revision,
        second_bytes,
        2,
    )
    .await;
    let second_report = second_service
        .sync_provider(&runtime, "alibaba")
        .await
        .expect("second signed sync should succeed");
    assert_eq!(second_report.outcome, "activated");
    runtime
        .activate_staged_manifest_catalogs(&configuration, &affected_connection_ids)
        .await
        .expect("second signed generation should publish");
    second_server
        .await
        .expect("second signed server should finish");
    assert!(
        runtime.catalog_snapshot().expect("second catalog")[&connection_id]
            .models
            .iter()
            .any(|model| model.id == second_model),
        "the signed successor model must appear in the successor generation"
    );
    assert!(
        runtime
            .available_provider_tool_targets(&configuration)
            .expect("second signed tool targets should resolve")
            .iter()
            .any(|available| {
                available.connection_id == media_connection_id
                    && available.target.upstream_target_id == second_media_model
            }),
        "the signed successor media model must appear as a template instance"
    );

    secret_store.release_blocked_read();
    // old_result must complete with revision one's frozen route after revision two is public.
    // old_result 必须在第二修订版发布后仍使用第一修订版冻结路由完成。
    let old_result = old_request
        .await
        .expect("old request task should join")
        .expect("old request should execute through its frozen generation");
    // old_wire_request is the exact provider-native request sent after successor publication.
    // old_wire_request 是后继版本发布后发送的精确供应商原生请求。
    let (old_path, old_wire_request) = provider_requests
        .recv()
        .await
        .expect("old provider request should be captured");
    assert_eq!(
        old_path,
        "/api/v1/services/aigc/multimodal-generation/generation"
    );
    assert_eq!(old_wire_request["model"], first_media_model);
    assert_eq!(
        old_result.upstream_response_id.as_deref(),
        Some("request-qwen-image-signed-e2e-first")
    );
    match &old_result.output {
        vulcan_provider_adapters::ProviderToolNormalizedOutput::Resources(resources) => {
            assert_eq!(resources.len(), 1);
            assert_eq!(
                resources[0].download_url.as_deref(),
                Some("https://dashscope.aliyuncs.com/qwen-image-signed-e2e-first.png")
            );
        }
        output => panic!("expected normalized image resources, got {output:?}"),
    }

    // second_available_target resolves only after revision two crosses activation.
    // second_available_target 仅在第二修订版越过激活边界后解析。
    let second_available_target = runtime
        .available_provider_tool_targets(&configuration)
        .expect("second signed tool targets should remain available")
        .into_iter()
        .find(|available| {
            available.connection_id == media_connection_id
                && available.target.upstream_target_id == second_media_model
        })
        .expect("second signed image-template instance must be available");
    // second_tool_selection targets only the successor media model.
    // second_tool_selection 只指向后继版本媒体模型。
    let second_tool_selection = vulcan_provider_adapters::ProviderToolTargetSelection {
        connection_id: media_connection_id.clone(),
        target_id: second_available_target.target.target_id.clone(),
    };
    // second_result proves a newly started request observes the successor generation.
    // second_result 证明新启动请求观察到后继代际。
    let second_result = runtime
        .execute_provider_tool_target_for_owner(
            &configuration,
            vulcan_provider_adapters::ProviderToolCapabilityKind::ImageGenerate,
            &second_tool_selection,
            serde_json::json!({"prompt": "draw revision two"}),
            None,
        )
        .await
        .expect("second request should execute through the successor generation");
    // second_wire_request must contain the successor model and the same reviewed path.
    // second_wire_request 必须包含后继模型与同一已审核路径。
    let (second_path, second_wire_request) = provider_requests
        .recv()
        .await
        .expect("second provider request should be captured");
    assert_eq!(second_path, old_path);
    assert_eq!(second_wire_request["model"], second_media_model);
    assert_eq!(
        second_result.upstream_response_id.as_deref(),
        Some("request-qwen-image-signed-e2e-second")
    );
    tokio::time::timeout(std::time::Duration::from_secs(10), provider_server)
        .await
        .expect("provider-tool server should finish")
        .expect("provider-tool server task should not panic");

    let bad_revision = bundled_revision + 3;
    let bad_bytes = alibaba_manifest_e2e_bytes(
        bad_revision,
        &[first_model, second_model],
        &[first_media_model, second_media_model],
        true,
    );
    let (bad_service, bad_server) =
        signed_https_sync_service(directory.path(), "alibaba", bad_revision, bad_bytes, 3).await;
    let bad_report = bad_service
        .sync_provider(&runtime, "alibaba")
        .await
        .expect("a verified-index candidate rejection should return a classified report");
    bad_server.await.expect("bad signed server should finish");
    assert_eq!(bad_report.outcome, "rejected");
    assert_eq!(
        bad_report.failure_kind.as_deref(),
        Some("trust_ceiling_failed")
    );
    assert_eq!(
        runtime
            .system_manifest("alibaba")
            .expect("second generation should remain published")
            .revision,
        second_revision,
        "a rejected candidate must not alter the published generation"
    );

    let rollback = bad_service
        .rollback_provider_manifest(&runtime, "alibaba")
        .expect("authenticated previous layer should stage for rollback");
    assert_eq!(rollback.restored_revision, first_revision);
    bad_service
        .finalize_rollback_projection(
            &runtime,
            &configuration,
            rollback,
            |commit| commit(),
            || Ok(()),
        )
        .await
        .expect("rollback catalogs and manifest should publish atomically");
    assert_eq!(
        runtime
            .system_manifest("alibaba")
            .expect("rolled-back manifest should exist")
            .revision,
        first_revision
    );
    let rolled_back = runtime.catalog_snapshot().expect("rolled-back catalog");
    assert!(
        rolled_back[&connection_id]
            .models
            .iter()
            .any(|model| model.id == first_model)
    );
    assert!(
        rolled_back[&connection_id]
            .models
            .iter()
            .all(|model| model.id != second_model),
        "rollback must remove the successor-only static model from the same generation"
    );
    // rolled_back_targets prove media instances roll back in the exact same generation.
    // rolled_back_targets 证明媒体实例在同一精确代际内回滚。
    let rolled_back_targets = runtime
        .available_provider_tool_targets(&configuration)
        .expect("rolled-back tool targets should resolve");
    assert!(rolled_back_targets.iter().any(|available| {
        available.connection_id == media_connection_id
            && available.target.upstream_target_id == first_media_model
    }));
    assert!(
        rolled_back_targets.iter().all(|available| {
            available.connection_id != media_connection_id
                || available.target.upstream_target_id != second_media_model
        }),
        "rollback must remove the successor-only media template instance"
    );
}

/// R7: a 304 index without a complete local active layer forces an unconditional refetch.
/// R7：本地 active 层不完整时，304 Index 强制无条件重取。
///
/// Business invariant (plan §R7.9): 304 is acceptable only when the local state pins a
/// complete active file that still re-verifies; a missing active file must refetch.
/// 业务不变量（方案 §R7.9）：仅当本地状态固定一份完整且仍可重新校验的 active 文件时
/// 304 才可接受；active 文件缺失时必须重取。
#[test]
fn index_304_without_local_active_forces_refetch() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let store = ManifestStoreLayout::new(directory.path());
    // The sync state claims an active revision but no active file exists.
    // 同步状态声明存在活动修订，但不存在 active 文件。
    store
        .save_sync_state(
            "deepseek",
            &vulcan_provider_config::manifest_store::ManifestSyncState {
                revision: 3,
                sha256: "a".repeat(64),
                previous: None,
                etag: Some(String::from("\"etag-3\"")),
                index_etag: Some(String::from("\"index-etag-3\"")),
                last_modified: None,
                synced_at_unix_ms: 1000,
                last_error_kind: None,
            },
        )
        .expect("state should save");
    let service = ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        None,
    );
    // With no remote URL the service stays not_configured; the local-active validation is
    // exercised directly to prove a missing file is not "current".
    // 没有远端 URL 时服务保持 not_configured；直接执行本地 active 校验，证明缺失文件
    // 不视为 "current"。
    let error = service
        .local_active_is_current("deepseek", &3, &"a".repeat(64))
        .expect_err("missing active file must not validate as current");
    assert!(error.contains("no active layer"), "{error}");
}

/// Spawns one loopback HTTP server that answers a fixed number of connections.
/// 启动一个应答固定连接次数的回环 HTTP 服务器。
///
/// # Parameters
/// 参数
/// `responses` are the raw HTTP response heads and bodies served in accept order; the
/// request is drained before each response so redirect-following clients can be served twice.
/// `responses` 按 accept 顺序提供的原始 HTTP 响应头与响应体；每次响应前都会排空请求，
/// 以便跟随重定向的客户端可被服务两次。
///
/// # Returns
/// 返回值
/// The loopback URL of the first accepted connection.
/// 返回首个被接受连接的回环 URL。
fn spawn_loopback_server(responses: Vec<(String, Vec<u8>)>) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind should succeed");
    let url = format!("http://{}/index.json", listener.local_addr().expect("addr"));
    std::thread::spawn(move || {
        for (head, body) in responses {
            let (mut stream, _peer) = listener.accept().expect("accept should succeed");
            let mut buffer = [0_u8; 8192];
            let _read = std::io::Read::read(&mut stream, &mut buffer);
            std::io::Write::write_all(&mut stream, head.as_bytes()).expect("head should write");
            std::io::Write::write_all(&mut stream, &body).expect("body should write");
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    });
    url
}

/// R7: the configured index endpoint must resolve to one unambiguous credential-free HTTPS origin.
/// R7：配置的 Index 端点必须解析为唯一且不含凭据的 HTTPS 源。
///
/// Business invariant (plan §R7.4): URL text that can change authority interpretation,
/// carry credentials, or preserve a client-only fragment is rejected before network I/O.
/// 业务不变量（方案 §R7.4）：可能改变 authority 解释、携带凭据或保留仅客户端片段的
/// URL 文本必须在网络 I/O 前被拒绝。
#[test]
fn index_origin_rejects_ambiguous_authority() {
    // origin is the canonical scheme-and-authority identity used for provider URL joining.
    // origin 是供应商 URL 拼接使用的规范 scheme 与 authority 身份。
    let origin = index_origin("https://manifests.example.com/v1/index.json?channel=stable")
        .expect("one credential-free HTTPS URL should be accepted");
    assert_eq!(origin, "https://manifests.example.com");

    // unsafe_urls enumerate every configured-endpoint ambiguity rejected by the parser guard.
    // unsafe_urls 枚举解析器守卫拒绝的每类配置端点歧义。
    let unsafe_urls = [
        "http://manifests.example.com/v1/index.json",
        "https://user:secret@manifests.example.com/v1/index.json",
        "https://manifests.example.com/v1/index.json#candidate",
        "https://manifests.example.com\\@evil.example/v1/index.json",
        "not a URL",
    ];
    for unsafe_url in unsafe_urls {
        assert!(
            index_origin(unsafe_url).is_err(),
            "unsafe index URL must be rejected: {unsafe_url}"
        );
    }
}

/// R0/P0-04: remote index reads must be bounded by an actual cumulative byte limit.
/// R0/P0-04：远端 Index 读取必须受真实累计字节上限约束。
///
/// Business invariant (plan §6.7/§11.3, R7): every network body is read with a real
/// cumulative byte cap that fails on the next byte after the limit; an oversized
/// Content-Length must be rejected before buffering the whole body.
/// 业务不变量（方案 §6.7/§11.3、阶段 R7）：每个网络响应体都按真实累计字节上限读取，
/// 超过上限的下一字节立即失败；超限 Content-Length 必须在缓冲完整响应体前被拒绝。
#[test]
fn fetch_conditional_rejects_oversized_content_length() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let service = ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        Some(String::from("https://example.test/index.json")),
    );
    let oversized = vulcan_provider_config::manifest_sync::REMOTE_INDEX_MAX_BYTES as usize + 1;
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {oversized}\r\nConnection: close\r\n\r\n"
    );
    let url = spawn_loopback_server(vec![(head, vec![b'x'; oversized])]);
    let error = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("async runtime")
        .block_on(service.fetch_conditional(
            &url,
            None,
            vulcan_provider_config::manifest_sync::REMOTE_INDEX_MAX_BYTES,
        ))
        .expect_err("an oversized remote body must be rejected by the byte bound");
    assert!(
        error.contains("bound") || error.contains("limit") || error.contains("bytes"),
        "unexpected size-limit error: {error}"
    );
}

/// R7: a chunked body without Content-Length still fails at the cumulative byte bound.
/// R7：无 Content-Length 的分块响应体仍在累计字节上限处失败。
///
/// Business invariant (plan §R7.5): the streaming cumulative cap is the final authority
/// when the remote omits or lies about Content-Length.
/// 业务不变量（方案 §R7.5）：远端省略或谎报 Content-Length 时，流式累计上限是最终权威。
#[test]
fn fetch_conditional_rejects_chunked_body_over_bound() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let service = ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        Some(String::from("https://example.test/index.json")),
    );
    let oversized = vulcan_provider_config::manifest_sync::REMOTE_INDEX_MAX_BYTES as usize + 1;
    // No Content-Length and no Content-Type: the media-type check runs first, so the
    // fixture carries the accepted media type and only the cumulative byte cap applies.
    // 无 Content-Length 且无 Content-Type：媒体类型校验先行，因此该装置携带受接受的
    // 媒体类型，只适用累计字节上限。
    let head = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n";
    let url = spawn_loopback_server(vec![(head.to_string(), vec![b'x'; oversized])]);
    let error = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("async runtime")
        .block_on(service.fetch_conditional(
            &url,
            None,
            vulcan_provider_config::manifest_sync::REMOTE_INDEX_MAX_BYTES,
        ))
        .expect_err("an oversized chunked body must be rejected by the cumulative bound");
    assert!(
        error.contains("bound") || error.contains("limit") || error.contains("bytes"),
        "unexpected size-limit error: {error}"
    );
}

/// R0/P0-04: remote manifest reads must never follow redirects.
/// R0/P0-04：远端清单读取绝不跟随重定向。
///
/// Business invariant (plan §6.7/§11.3, R7): the sync client is built with redirects
/// disabled and any 3xx answer fails the fetch; a redirect must never deliver the
/// response from a second origin.
/// 业务不变量（方案 §6.7/§11.3、阶段 R7）：同步客户端禁用重定向，任何 3xx 响应
/// 都必须使获取失败；重定向绝不能从第二个源返回响应。
#[test]
fn fetch_conditional_rejects_redirect() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let service = ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        Some(String::from("https://example.test/index.json")),
    );
    let redirect = "HTTP/1.1 302 Found\r\nLocation: /moved.json\r\nContent-Length: 0\r\n\r\n";
    let target = "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc";
    let url = spawn_loopback_server(vec![
        (redirect.to_string(), Vec::new()),
        (target.to_string(), Vec::new()),
    ]);
    let error = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("async runtime")
        .block_on(service.fetch_conditional(&url, None, REMOTE_PROVIDER_MAX_BYTES))
        .expect_err("a redirect answer must fail the fetch");
    assert!(
        error.contains("redirect") || error.contains("3"),
        "unexpected redirect error: {error}"
    );
}

/// R7: response URL identity is rechecked even when an injected client follows redirects.
/// R7：即使注入客户端跟随重定向，也必须重新校验响应 URL 身份。
///
/// Business invariant (plan §R7.4): the dedicated no-redirect client is not the only
/// authority boundary; constructor misuse must still fail closed after the response arrives.
/// 业务不变量（方案 §R7.4）：专用禁重定向客户端不是唯一 authority 边界；即使构造器
/// 被误用，也必须在响应到达后保持关闭失败。
#[test]
fn fetch_conditional_rejects_redirect_followed_by_injected_client() {
    // directory isolates the manifest store used by this constructor-level regression test.
    // directory 隔离此构造器级回归测试使用的清单存储。
    let directory = tempfile::tempdir().expect("temp dir should create");
    // redirecting_client deliberately violates the production constructor policy.
    // redirecting_client 故意违反生产构造器的禁重定向策略。
    let redirecting_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(2))
        .build()
        .expect("redirect-following test client should build");
    // service proves the response-identity guard survives dependency injection misuse.
    // service 证明响应身份守卫可抵御依赖注入误用。
    let service = ProviderManifestSyncService::new(
        directory.path(),
        redirecting_client,
        Vec::new(),
        Some(String::from("https://example.test/index.json")),
    );
    // redirect points the injected client at a distinct final path on the same origin.
    // redirect 将注入客户端指向同源但最终路径不同的地址。
    let redirect = "HTTP/1.1 302 Found\r\nLocation: /moved.json\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    // target is otherwise a valid JSON response so only URL identity can reject it.
    // target 是其他方面均有效的 JSON 响应，确保只有 URL 身份会拒绝它。
    let target = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n";
    // url is the exact initial endpoint captured before the client follows the redirect.
    // url 是客户端跟随重定向前捕获的精确初始端点。
    let url = spawn_loopback_server(vec![
        (redirect.to_string(), Vec::new()),
        (target.to_string(), b"{}".to_vec()),
    ]);
    // error must come from the post-response URL identity guard.
    // error 必须来自响应后的 URL 身份守卫。
    let error = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("async runtime")
        .block_on(service.fetch_conditional(&url, None, REMOTE_PROVIDER_MAX_BYTES))
        .expect_err("a followed redirect must fail the response identity check");
    assert!(
        error.contains("forbidden redirect"),
        "unexpected followed-redirect error: {error}"
    );
}

/// Verifies the typed status snapshot exposes bundled/active/previous layers, sync state,
/// and quarantine summaries for one provider (plan §R10).
/// 验证 typed 状态快照暴露一个供应商的 bundled/active/previous 层、同步状态与隔离摘要
/// （方案 §R10）。
#[tokio::test]
async fn provider_manifest_status_snapshot_exposes_layers() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let store = ManifestStoreLayout::new(directory.path());
    let active_revision = bundled_deepseek_revision() + 1;
    // addedModelId is absent from Bundled and must be marked after the first activation.
    // addedModelId 不存在于 Bundled，首次激活后必须被标记。
    let added_model_id = "deepseek-status-added";
    let text = deepseek_manifest_with_exact_model(active_revision, added_model_id);
    // Stage and activate a revision above bundled so Active and Bundled form a contrast.
    // 暂存并激活高于随包版本的修订，使 Active 与 Bundled 形成对照。
    activate_trusted_deepseek(&store, active_revision, &text);
    store
        .quarantine("deepseek", 2, "signature_invalid", b"candidate")
        .expect("quarantine should write");

    let http = build_sync_http_client().expect("sync client");
    let service = ProviderManifestSyncService::new(directory.path(), http, Vec::new(), None);
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    // The bundled manifest is the baseline; restore the persisted active layer so the
    // runtime carries a non-bundled active snapshot.
    // 内置清单是基线；恢复持久化 active 层使运行时携带非 bundled 的 active 快照。
    restore_system_manifests_from_store(&runtime, directory.path())
        .expect("restore should succeed");
    runtime
        .activate_staged_manifest_catalogs(&PersistedAiConfiguration::default(), &[])
        .await
        .expect("restored active layer must publish before status is observed");

    let status = service
        .provider_manifest_status(&runtime, "deepseek")
        .expect("status snapshot should build");
    // The snapshot is a typed serializable contract (plan §R10 RPC 要求 5).
    // 快照是 typed 可序列化契约（方案 §R10 RPC 要求 5）。
    let serialized = serde_json::to_value(&status).expect("status must serialize as typed value");
    assert_eq!(status.provider_id, "deepseek");
    assert!(!status.sync_configured, "sync must be unconfigured");
    let bundled = status
        .bundled
        .as_ref()
        .expect("bundled layer must be readable");
    assert!(bundled.revision >= 1, "bundled revision");
    assert_eq!(bundled.sha256.len(), 64, "sha256 hex length");
    let active = status
        .active
        .as_ref()
        .expect("active layer must be present");
    assert_eq!(active.layer, ManifestActiveLayer::Active);
    assert_eq!(active.revision, active_revision);
    // changeBaseline proves first activation compares against Bundled without inventing Previous.
    // changeBaseline 证明首次激活会与 Bundled 比较，而不会虚构 Previous。
    let change_baseline = status
        .active_model_binding_change_baseline
        .as_ref()
        .expect("first activation must retain a bundled change baseline");
    assert_eq!(change_baseline.layer, ManifestActiveLayer::Bundled);
    assert_eq!(change_baseline.revision, bundled.revision);
    assert_eq!(status.active_model_binding_change_total_count, 1);
    assert!(!status.active_model_binding_changes_truncated);
    assert!(status.active_model_binding_changes.iter().any(|delta| {
        delta.change == ProviderManifestChangeKind::Added
            && delta
                .candidate
                .as_ref()
                .is_some_and(|binding| binding.model_id == added_model_id)
    }));
    assert_eq!(
        status.quarantine.len(),
        1,
        "quarantine summary must list the rejected candidate"
    );
    assert_eq!(status.quarantine[0].failure_kind, "signature_invalid");
    assert!(
        serialized.get("active").is_some(),
        "typed serialized active"
    );
    assert!(
        !status.model_rules.is_empty(),
        "active model rules must be exposed with provenance"
    );
    assert!(
        status
            .model_rules
            .iter()
            .all(|rule| rule.field_provenance.contains_key("supported_protocols")),
        "every expanded rule must explain its effective protocol candidates"
    );
    assert_eq!(
        status.model_binding_total_count,
        status.model_bindings.len(),
        "the bounded DeepSeek inventory should be complete"
    );
    assert!(
        !status.model_bindings.is_empty()
            && status.model_bindings.iter().all(|binding| {
                binding.executable
                    && binding.effective_protocol.is_some()
                    && binding.binding_source.as_deref() == Some("system_manifest")
                    && binding.manifest_revision == active_revision
            }),
        "every concrete DeepSeek model/profile binding must be backend-resolved and executable"
    );
    assert!(
            status.operations.iter().all(|operation| {
                operation.timeout_ms.is_some()
                    || operation.carrier
                        == vulcan_provider_adapters::provider_manifest::OperationCarrier::ConversationProtocol
            }),
            "every non-conversation operation must expose its timeout contract"
        );
    // alibabaStatus exercises the asynchronous lifecycle fields that DeepSeek does not own.
    // alibabaStatus 覆盖 DeepSeek 不持有的异步生命周期字段。
    let alibaba_status = service
        .provider_manifest_status(&runtime, "alibaba")
        .expect("bundled Alibaba status should build");
    // asyncOperations are the exact backend-owned operation contracts rendered by the UI.
    // asyncOperations 是 UI 渲染的精确后端 operation 契约。
    let async_operations = alibaba_status
        .operations
        .iter()
        .filter(|operation| operation.asynchronous)
        .collect::<Vec<_>>();
    assert!(
        !async_operations.is_empty(),
        "Alibaba must expose asynchronous operations"
    );
    assert!(
        async_operations.iter().all(|operation| {
            operation
                .poll_interval_ms
                .is_some_and(|interval| interval > 0)
                && operation
                    .max_backoff_ms
                    .zip(operation.poll_interval_ms)
                    .is_some_and(|(backoff, interval)| backoff >= interval)
                && operation.timeout_ms.is_some()
        }),
        "every asynchronous operation must expose its polling and backoff contract"
    );
    assert!(
        async_operations
            .iter()
            .any(|operation| operation.cancellation_supported),
        "the status must preserve declared cancellation support"
    );
    assert!(
        status
            .declaration_provenance
            .values()
            .all(|provenance| provenance.manifest_revision == active_revision),
        "declaration provenance must use the exact active revision"
    );
    assert!(
        serialized.get("model_rules").is_some(),
        "typed serialized model rules"
    );
    assert!(
        serialized.get("model_bindings").is_some(),
        "typed serialized model bindings"
    );
    assert!(
        serialized
            .get("active_model_binding_change_baseline")
            .is_some(),
        "typed serialized active binding baseline"
    );
    assert!(
        serialized.get("active_model_binding_changes").is_some(),
        "typed serialized active binding changes"
    );
    assert!(
        serialized.get("operations").is_some(),
        "typed serialized operation status"
    );
    assert!(
        serialized.get("declaration_provenance").is_some(),
        "typed serialized declaration provenance"
    );

    // originalBinding supplies a complete effective status for semantic diff normalization.
    // originalBinding 为语义差异归一化提供一条完整有效状态。
    let original_binding = status
        .model_bindings
        .iter()
        .find(|binding| binding.model_id != added_model_id)
        .cloned()
        .expect("an existing DeepSeek binding must be present");
    // revisionOnlyBinding changes generation identity but no effective model behavior.
    // revisionOnlyBinding 只改变代际身份，不改变任何有效模型行为。
    let mut revision_only_binding = original_binding.clone();
    revision_only_binding.manifest_revision =
        revision_only_binding.manifest_revision.saturating_add(1);
    for provenance in revision_only_binding.capability_provenance.values_mut() {
        provenance.manifest_revision = provenance.manifest_revision.saturating_add(1);
        for overridden in &mut provenance.overridden_sources {
            overridden.manifest_revision = overridden.manifest_revision.saturating_add(1);
        }
    }
    // revisionOnlyDiff must stay empty so a pure revision bump cannot mark every model changed.
    // revisionOnlyDiff 必须保持为空，使纯修订升级不能把全部模型标成已变更。
    let (revision_only_diff, revision_only_total, revision_only_truncated) =
        diff_manifest_model_bindings(
            std::slice::from_ref(&original_binding),
            std::slice::from_ref(&revision_only_binding),
        );
    assert!(revision_only_diff.is_empty());
    assert_eq!(revision_only_total, 0);
    assert!(!revision_only_truncated);
    revision_only_binding.effective_protocol = Some(String::from("semantic-change.v1"));
    // semanticDiff proves effective routing still produces a changed marker.
    // semanticDiff 证明有效路由变化仍会产生 changed 标记。
    let (semantic_diff, semantic_total, semantic_truncated) = diff_manifest_model_bindings(
        std::slice::from_ref(&original_binding),
        std::slice::from_ref(&revision_only_binding),
    );
    assert_eq!(semantic_total, 1);
    assert!(!semantic_truncated);
    assert_eq!(semantic_diff[0].change, ProviderManifestChangeKind::Changed);
}

/// Verifies status includes discovery-only models and mirrors the runtime's exact
/// bound-protocol variant guard for each configured connection.
/// 验证状态包含仅发现模型，并针对每个已配置连接镜像运行时精确绑定协议变体护栏。
#[test]
fn provider_manifest_status_includes_discovered_models_and_available_protocols() {
    // directory isolates the status store; no remote synchronization participates.
    // directory 隔离状态存储；本测试不参与远端同步。
    let directory = tempfile::tempdir().expect("temp dir should create");
    let http = build_sync_http_client().expect("sync client");
    let service = ProviderManifestSyncService::new(directory.path(), http, Vec::new(), None);
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    // configuration owns one exact OpenAI discovery account.
    // configuration 持有一个精确 OpenAI 发现账号。
    let mut configuration = PersistedAiConfiguration::default();
    let connection_id = String::from("openai-discovery-status");
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            vulcan_provider_adapters::CredentialConnection {
                id: connection_id.clone(),
                provider_id: String::from("openai"),
                service_entry_id: String::from("public-api"),
                credential_profile_id: String::from("api-key"),
                credential_mode: vulcan_provider_adapters::CredentialMode::Token,
                credential_id: String::from("openai-discovery-credential"),
                display_name: String::from("OpenAI Discovery Status"),
                secret_reference: String::from("openai-discovery-secret"),
                management_secret_reference: None,
                service_origin_override: None,
            },
        )
        .expect("OpenAI connection should register");
    // readyModel proves an unknown discovered model can use the Manifest default binding.
    // readyModel 证明未知发现模型可使用 Manifest 默认绑定。
    let mut ready_model = vulcan_provider_adapters::ConnectionModel {
        id: String::from("openai-dynamic-ready"),
        display_name: String::from("OpenAI Dynamic Ready"),
        capabilities: vulcan_provider_adapters::ModelCapabilities::default(),
        capability_evidence: Default::default(),
        protocol_variants: BTreeMap::new(),
        resolved_protocol_binding: None,
    };
    ready_model.protocol_variants.insert(
        vulcan_llm_core::OPENAI_CHAT_PROFILE_ID.to_string(),
        vulcan_provider_adapters::ProtocolModelVariant {
            capabilities: vulcan_provider_adapters::ModelCapabilities::default(),
            capability_evidence: Default::default(),
            discovery_signature: String::from("openai-status-chat-signature"),
            catalog_generation: 1,
        },
    );
    // missingModel intentionally lacks the Manifest-selected Chat variant.
    // missingModel 有意缺少 Manifest 选择的 Chat 变体。
    let mut missing_model = vulcan_provider_adapters::ConnectionModel {
        id: String::from("openai-dynamic-missing"),
        display_name: String::from("OpenAI Dynamic Missing"),
        capabilities: vulcan_provider_adapters::ModelCapabilities::default(),
        capability_evidence: Default::default(),
        protocol_variants: BTreeMap::new(),
        resolved_protocol_binding: None,
    };
    missing_model.protocol_variants.insert(
        vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID.to_string(),
        vulcan_provider_adapters::ProtocolModelVariant {
            capabilities: vulcan_provider_adapters::ModelCapabilities::default(),
            capability_evidence: Default::default(),
            discovery_signature: String::from("openai-status-responses-signature"),
            catalog_generation: 1,
        },
    );
    runtime
        .stage_source_catalog_for_testing(vulcan_provider_adapters::ConnectionModelCatalog {
            connection_id: connection_id.clone(),
            models: vec![ready_model, missing_model],
            generation: 1,
            page_count: 1,
            discovery_schema_revision: String::from("openai-status-fixture-v1"),
        })
        .expect("discovered source catalog should stage");
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("discovered catalog should cross a publication boundary");
    runtime
        .replace_configuration(configuration)
        .expect("status configuration should publish");

    let status = service
        .provider_manifest_status(&runtime, "openai")
        .expect("status snapshot should build");
    let ready = status
        .model_bindings
        .iter()
        .find(|binding| {
            binding.connection_id.as_deref() == Some(connection_id.as_str())
                && binding.model_id == "openai-dynamic-ready"
        })
        .expect("discovery-only ready model must be present");
    assert_eq!(ready.model_source, ManifestModelSourceStatus::Discovery);
    assert_eq!(
        ready.available_protocols,
        vec![vulcan_llm_core::OPENAI_CHAT_PROFILE_ID.to_string()]
    );
    assert_eq!(
        ready.effective_protocol.as_deref(),
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID)
    );
    assert!(ready.catalog_observed && ready.catalog_enabled && ready.executable);
    assert!(
        ready.capabilities.is_some() && ready.capability_evidence.is_some(),
        "the effective route capability and evidence must be exposed"
    );

    let missing = status
        .model_bindings
        .iter()
        .find(|binding| {
            binding.connection_id.as_deref() == Some(connection_id.as_str())
                && binding.model_id == "openai-dynamic-missing"
        })
        .expect("discovery-only missing-variant model must be present");
    assert_eq!(missing.model_source, ManifestModelSourceStatus::Discovery);
    assert_eq!(
        missing.available_protocols,
        vec![vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID.to_string()]
    );
    assert!(
        !missing.executable,
        "missing bound variant must block execution"
    );
    assert_eq!(
        missing.unavailable_reason.as_deref(),
        Some(
            "Model `openai-dynamic-missing` has no capability variant for its bound protocol `openai-chat-completions.v1`; execution is blocked."
        )
    );
}

/// Verifies an unconfigured service reports `sync_configured=false` and `bundled` remains
/// the active layer when no store state exists (plan §R10 "未配置" 状态清晰可见).
/// 验证未配置服务报告 `sync_configured=false` 且无存储状态时 bundled 保持为 active 层
/// （方案 §R10 "未配置" 状态清晰可见）。
#[test]
fn provider_manifest_status_reports_unconfigured_bundled_active() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let http = build_sync_http_client().expect("sync client");
    let service = ProviderManifestSyncService::new(directory.path(), http, Vec::new(), None);
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let status = service
        .provider_manifest_status(&runtime, "deepseek")
        .expect("status snapshot should build");
    assert!(!status.sync_configured, "sync must be unconfigured");
    assert!(status.sync_state.is_none(), "no sync state yet");
    assert!(status.previous.is_none(), "no previous layer yet");
    let active = status.active.expect("bundled active must be present");
    assert_eq!(
        active.layer,
        ManifestActiveLayer::Bundled,
        "without a store layer the bundled baseline is the active manifest"
    );
}

/// Verifies APIKEY.FAN status reflects Manifest cutover while Alibaba template instances remain visible.
/// 验证 APIKEY.FAN 状态反映 Manifest 切换，同时保持 Alibaba 模板实例可见。
#[test]
fn provider_manifest_status_exposes_apikeyfan_cutover_and_template_instances() {
    // directory isolates manifest-store state so both statuses come only from bundled facts.
    // directory 隔离 Manifest 存储状态，使两份状态都只来自内置事实。
    let directory = tempfile::tempdir().expect("temp dir should create");
    // http is required by the service even though this status-only test performs no network I/O.
    // http 是服务构造所需依赖，尽管本状态测试不会执行网络 I/O。
    let http = build_sync_http_client().expect("sync client");
    // service reads status through the same production projection used by the typed RPC.
    // service 通过 typed RPC 使用的同一生产投影读取状态。
    let service = ProviderManifestSyncService::new(directory.path(), http, Vec::new(), None);
    // runtime owns the authoritative compiled bundled generation.
    // runtime 持有权威的已编译内置代际。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");

    // apikeyfan proves the bundled Manifest is now the active binding authority.
    // apikeyfan 证明内置 Manifest 现已成为活动绑定权威。
    let apikeyfan = service
        .provider_manifest_status(&runtime, "apikeyfan")
        .expect("APIKEY.FAN status should build");
    assert!(apikeyfan.bundled.is_some());
    assert!(apikeyfan.active.is_some());

    // alibaba exposes generated targets without requiring the UI to recompute templates.
    // alibaba 公开生成目标，无需 UI 重新计算模板。
    let alibaba = service
        .provider_manifest_status(&runtime, "alibaba")
        .expect("Alibaba status should build");
    assert!(
        alibaba
            .tool_target_template_instances
            .iter()
            .any(|instance| {
                instance.template_id == "alibaba.model-studio-cn.qwen-image-generate.v1"
                    && instance.service_entry_id == "model-studio-cn"
                    && instance.model_id == "qwen-image-2.0-pro"
            }),
        "the concrete Qwen Image template instance must be visible in typed status"
    );
}

/// Verifies an explicit rollback restores the previous layer and hot-replaces the runtime.
/// 验证显式回滚恢复 previous 层并热替换运行时。
#[tokio::test]
async fn explicit_rollback_restores_previous_layer() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let store = ManifestStoreLayout::new(directory.path());
    let previous_revision = bundled_deepseek_revision() + 1;
    let active_revision = previous_revision + 1;
    let text = deepseek_manifest_with_revision(previous_revision);
    activate_trusted_deepseek(&store, previous_revision, &text);
    // Rotate the first authenticated revision into Previous by activating its successor.
    // 通过激活后一修订，把首个已认证修订轮换到 Previous。
    let second = deepseek_manifest_with_revision(active_revision);
    activate_trusted_deepseek(&store, active_revision, &second);

    let http = build_sync_http_client().expect("sync client");
    let service = ProviderManifestSyncService::new(directory.path(), http, Vec::new(), None);
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    restore_system_manifests_from_store(&runtime, directory.path())
        .expect("restore should succeed");
    let configuration = PersistedAiConfiguration::default();
    runtime
        .activate_staged_manifest_catalogs(&configuration, &[])
        .await
        .expect("initial active generation must publish");
    assert_eq!(
        runtime
            .system_manifest_revisions()
            .expect("revisions")
            .into_iter()
            .find(|(id, _)| id == "deepseek")
            .expect("deepseek revision")
            .1,
        active_revision,
        "the active revision must be visible before rollback"
    );

    let report = service
        .rollback_provider_manifest(&runtime, "deepseek")
        .expect("rollback should succeed");
    assert_eq!(report.restored_revision, previous_revision);
    service
        .finalize_rollback_projection(
            &runtime,
            &configuration,
            report,
            |commit| commit(),
            || Ok(()),
        )
        .await
        .expect("first rollback projection should publish and commit");
    assert_eq!(
        runtime
            .system_manifest_revisions()
            .expect("revisions")
            .into_iter()
            .find(|(id, _)| id == "deepseek")
            .expect("deepseek revision")
            .1,
        previous_revision,
        "the runtime must observe the rolled-back revision"
    );

    // A second rollback swaps back to the authenticated successor; rollback is a
    // content-addressed version toggle, not a blind copy of stale Previous bytes.
    // 第二次回滚交换回已认证后一修订；回滚是内容寻址版本切换，而不是盲目复制旧 Previous。
    let again = service
        .rollback_provider_manifest(&runtime, "deepseek")
        .expect("idempotent rollback should succeed");
    assert_eq!(again.restored_revision, active_revision);
    service
        .finalize_rollback_projection(
            &runtime,
            &configuration,
            again,
            |commit| commit(),
            || Ok(()),
        )
        .await
        .expect("second rollback projection should publish and commit");
}

/// Verifies a service projection/event failure restores exact disk and runtime before-images.
/// 验证服务投影/事件失败会恢复精确磁盘与运行时前镜像。
#[tokio::test]
async fn rollback_projection_failure_restores_exact_before_image() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let store = ManifestStoreLayout::new(directory.path());
    let previous_revision = bundled_deepseek_revision() + 1;
    let active_revision = previous_revision + 1;
    let first = deepseek_manifest_with_revision(previous_revision);
    activate_trusted_deepseek(&store, previous_revision, &first);
    let second = deepseek_manifest_with_revision(active_revision);
    activate_trusted_deepseek(&store, active_revision, &second);

    let service = ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        None,
    );
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    restore_system_manifests_from_store(&runtime, directory.path())
        .expect("restore should succeed");
    let configuration = PersistedAiConfiguration::default();
    runtime
        .activate_staged_manifest_catalogs(&configuration, &[])
        .await
        .expect("initial generation should publish");
    let before = store
        .capture_system_snapshot("deepseek")
        .expect("before-image must capture");

    let report = service
        .rollback_provider_manifest(&runtime, "deepseek")
        .expect("rollback staging should succeed");
    let publish_calls = std::cell::Cell::new(0_u8);
    let error = service
        .finalize_rollback_projection(
            &runtime,
            &configuration,
            report,
            |_commit| {
                publish_calls.set(publish_calls.get() + 1);
                Err(String::from("injected event emission failure"))
            },
            || {
                publish_calls.set(publish_calls.get() + 1);
                Ok(())
            },
        )
        .await
        .expect_err("the injected projection failure must reject the rollback");
    assert!(error.contains("injected event emission failure"), "{error}");
    assert_eq!(
        publish_calls.get(),
        2,
        "the restored former generation must be republished exactly once"
    );
    assert_eq!(
        store
            .capture_system_snapshot("deepseek")
            .expect("restored snapshot must capture"),
        before,
        "Active, Previous, and state bytes must match the exact before-image"
    );
    assert_eq!(
        runtime
            .system_manifest_revisions()
            .expect("revisions")
            .into_iter()
            .find(|(id, _)| id == "deepseek")
            .expect("deepseek revision")
            .1,
        active_revision,
        "the staged runtime catalog must return to the exact active revision"
    );
}

/// Verifies rollback publication must invoke its durable commit callback exactly once.
/// 验证回滚发布必须恰好调用一次持久提交回调。
#[tokio::test]
async fn rollback_projection_requires_exactly_one_commit_callback() {
    // directory owns the authenticated active/previous layers exercised by both failures.
    // directory 持有两种失败共同使用的已认证 active/previous 层。
    let directory = tempfile::tempdir().expect("temp dir should create");
    // store provides exact before-image capture and rollback staging.
    // store 提供精确前镜像捕获与回滚暂存。
    let store = ManifestStoreLayout::new(directory.path());
    // previousRevision identifies the first authenticated layer.
    // previousRevision 标识首个已认证层。
    let previous_revision = bundled_deepseek_revision() + 1;
    // activeRevision identifies the currently published successor.
    // activeRevision 标识当前已发布的后继版本。
    let active_revision = previous_revision + 1;
    // previousText is the exact signed-test document rotated into Previous.
    // previousText 是被轮换到 Previous 的精确签名测试文档。
    let previous_text = deepseek_manifest_with_revision(previous_revision);
    activate_trusted_deepseek(&store, previous_revision, &previous_text);
    // activeText is the exact signed-test document visible before each rejected projection.
    // activeText 是每次被拒投影前可见的精确签名测试文档。
    let active_text = deepseek_manifest_with_revision(active_revision);
    activate_trusted_deepseek(&store, active_revision, &active_text);
    // service owns the rollback journal and compensation implementation under test.
    // service 持有本测试所验证的回滚日志与补偿实现。
    let service = ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        None,
    );
    // runtime mirrors the authenticated store before rollback staging begins.
    // runtime 在回滚暂存开始前镜像已认证存储。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    restore_system_manifests_from_store(&runtime, directory.path())
        .expect("restore should succeed");
    // configuration supplies the exact empty connection graph used by this isolated rollback.
    // configuration 提供该隔离回滚使用的精确空连接图。
    let configuration = PersistedAiConfiguration::default();
    runtime
        .activate_staged_manifest_catalogs(&configuration, &[])
        .await
        .expect("initial generation should publish");
    // before captures every durable layer before either invalid projection is attempted.
    // before 捕获两次无效投影尝试前的全部持久层。
    let before = store
        .capture_system_snapshot("deepseek")
        .expect("before-image must capture");

    // missingCommitReport stages the first rollback whose projection omits the callback.
    // missingCommitReport 暂存首个省略回调的回滚。
    let missing_commit_report = service
        .rollback_provider_manifest(&runtime, "deepseek")
        .expect("rollback staging should succeed");
    // missingCommitError must reject apparent projection success without journal commit.
    // missingCommitError 必须拒绝未提交日志却声称投影成功的结果。
    let missing_commit_error = service
        .finalize_rollback_projection(
            &runtime,
            &configuration,
            missing_commit_report,
            |_commit| Ok(()),
            || Ok(()),
        )
        .await
        .expect_err("projection success without commit must fail closed");
    assert!(
        missing_commit_error.contains("without committing its activation journal"),
        "{missing_commit_error}"
    );
    assert_eq!(
        store
            .capture_system_snapshot("deepseek")
            .expect("compensated snapshot must capture"),
        before,
        "a missing commit callback must restore the exact durable before-image"
    );

    // duplicateCommitReport stages a second rollback after exact first-attempt compensation.
    // duplicateCommitReport 在首次尝试精确补偿后暂存第二次回滚。
    let duplicate_commit_report = service
        .rollback_provider_manifest(&runtime, "deepseek")
        .expect("second rollback staging should succeed");
    // duplicateCommitError must reject a projection that crosses the commit seam twice.
    // duplicateCommitError 必须拒绝两次穿越提交接缝的投影。
    let duplicate_commit_error = service
        .finalize_rollback_projection(
            &runtime,
            &configuration,
            duplicate_commit_report,
            |commit| {
                commit()?;
                commit()
            },
            || Ok(()),
        )
        .await
        .expect_err("duplicate commit callback use must fail closed");
    assert!(
        duplicate_commit_error.contains("more than once"),
        "{duplicate_commit_error}"
    );
    assert_eq!(
        store
            .capture_system_snapshot("deepseek")
            .expect("twice-compensated snapshot must capture"),
        before,
        "duplicate commit use must restore the exact durable before-image"
    );
    assert_eq!(
        runtime
            .system_manifest_revisions()
            .expect("revisions")
            .into_iter()
            .find(|(id, _)| id == "deepseek")
            .expect("deepseek revision")
            .1,
        active_revision,
        "both invalid projections must leave the original runtime revision visible"
    );
}

/// Verifies cancellation before rollback publication restores the exact staged before-image.
/// 验证回滚发布前取消会恢复精确暂存前镜像。
#[tokio::test]
async fn cancelled_rollback_projection_restores_staged_before_image() {
    // directory owns the authenticated two-version rollback store.
    // directory 持有已认证双版本回滚存储。
    let directory = tempfile::tempdir().expect("temp dir should create");
    // store owns the exact Active, Previous, state, and transaction files.
    // store 持有精确 Active、Previous、状态与事务文件。
    let store = ManifestStoreLayout::new(directory.path());
    // previous_revision and active_revision establish a valid rollback pair.
    // previous_revision 与 active_revision 建立有效回滚版本对。
    let previous_revision = bundled_deepseek_revision() + 1;
    let active_revision = previous_revision + 1;
    // previous_text is the authenticated predecessor activated first.
    // previous_text 是先激活的已认证前驱版本。
    let previous_text = deepseek_manifest_with_revision(previous_revision);
    activate_trusted_deepseek(&store, previous_revision, &previous_text);
    // active_text rotates the predecessor into Previous.
    // active_text 把前驱版本轮换到 Previous。
    let active_text = deepseek_manifest_with_revision(active_revision);
    activate_trusted_deepseek(&store, active_revision, &active_text);
    // service owns rollback staging and cancellation compensation.
    // service 持有回滚暂存与取消补偿。
    let service = ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        None,
    );
    // runtime restores the authenticated Active authority before rollback staging.
    // runtime 在回滚暂存前恢复已认证 Active 权威。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    restore_system_manifests_from_store(&runtime, directory.path())
        .expect("active authority should restore");
    // configuration publishes the restored Active staging catalog before rollback begins.
    // configuration 在回滚开始前发布已恢复的 Active 暂存目录。
    let configuration = PersistedAiConfiguration::default();
    runtime
        .activate_staged_manifest_catalogs(&configuration, &[])
        .await
        .expect("active generation should publish");
    // before is the exact durable snapshot cancellation must recover.
    // before 是取消必须恢复的精确持久化快照。
    let before = store
        .capture_system_snapshot("deepseek")
        .expect("before-image should capture");
    // report owns the Applying transaction and staged runtime mutation.
    // report 持有 Applying 事务与暂存运行时变更。
    let report = service
        .rollback_provider_manifest(&runtime, "deepseek")
        .expect("rollback should stage");

    {
        // guard emulates cancellation while waiting for the catalog activation lock.
        // guard 模拟等待目录激活锁时发生取消。
        let _guard = ProviderRollbackCancellationGuard::new(&service, &runtime, &report);
    }

    assert_eq!(
        store
            .capture_system_snapshot("deepseek")
            .expect("restored snapshot should capture"),
        before
    );
    assert!(
        !store
            .recover_system_transaction("deepseek")
            .expect("journal probe should succeed")
    );
    assert_eq!(
        runtime
            .system_manifest("deepseek")
            .expect("restored active manifest should exist")
            .revision,
        active_revision
    );
}

/// Verifies runtime publication failure restores Active/state to the authenticated version.
/// 验证运行时发布失败会把 Active/state 恢复到可信版本。
#[test]
fn failed_runtime_publication_rolls_back_store_and_state() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let store = ManifestStoreLayout::new(directory.path());
    let previous_revision = bundled_deepseek_revision() + 1;
    let active_revision = previous_revision + 1;
    let first = deepseek_manifest_with_revision(previous_revision);
    activate_trusted_deepseek(&store, previous_revision, &first);
    let store_snapshot = store
        .capture_system_snapshot("deepseek")
        .expect("former store snapshot must capture");
    let second = deepseek_manifest_with_revision(active_revision);
    activate_trusted_deepseek(&store, active_revision, &second);

    let openrouter_text = std::fs::read_to_string(
        vulcan_runtime_assets::resolve_bundled_provider_manifests_directory()
            .join("openrouter.json"),
    )
    .expect("bundled openrouter manifest must exist");
    let wrong_manifest = Arc::new(
        vulcan_provider_adapters::compile_manifest_text_with_layer(
            &openrouter_text,
            vulcan_provider_adapters::ManifestOwnership::System,
            vulcan_provider_adapters::ManifestSourceLayer::Active,
        )
        .expect("openrouter manifest must compile"),
    );
    let service = ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        None,
    );
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let error = service
        .outcome_to_report(
            &runtime,
            "deepseek",
            active_revision,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vulcan_provider_config::manifest_sync::SyncOutcome::Activated {
                revision: active_revision,
                manifest: wrong_manifest,
                store_snapshot,
            },
        )
        .expect_err("identity-mismatched runtime publication must fail");
    assert!(error.contains("store rollback: Ok"), "{error}");
    let active = store
        .read_active("deepseek")
        .expect("active read must succeed")
        .expect("active layer must remain");
    let active_revision = serde_json::from_str::<serde_json::Value>(&active)
        .expect("active manifest must parse")["revision"]
        .as_u64();
    assert_eq!(active_revision, Some(previous_revision));
    let state = match store
        .load_sync_state("deepseek")
        .expect("state read must succeed")
    {
        vulcan_provider_config::manifest_store::SyncStateLoad::Present(state) => state,
        vulcan_provider_config::manifest_store::SyncStateLoad::Absent => {
            panic!("authenticated state must remain")
        }
    };
    assert_eq!(state.revision, previous_revision);
    assert!(state.previous.is_none());
}

/// Verifies a rollback with no previous layer fails closed without touching the runtime.
/// 验证无 previous 层的回滚失败关闭且不触碰运行时。
#[test]
fn rollback_without_previous_layer_fails_closed() {
    let directory = tempfile::tempdir().expect("temp dir should create");
    let http = build_sync_http_client().expect("sync client");
    let service = ProviderManifestSyncService::new(directory.path(), http, Vec::new(), None);
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let error = service
        .rollback_provider_manifest(&runtime, "deepseek")
        .expect_err("rollback without previous must fail");
    assert!(
        error.contains("no authenticated sync state"),
        "error: {error}"
    );
}

/// Verifies 304 and failed checks advance check state without replacing last successful activation.
/// 验证 304 与失败检查推进检查状态但不替换最近成功激活。
#[test]
fn check_state_distinguishes_checked_from_successful_activation() {
    // directory isolates the durable check-state files from the user's application data.
    // directory 把持久化检查状态文件与用户应用数据隔离。
    let directory = tempfile::tempdir().expect("temp dir should create");
    // service owns the exact production check-state transaction implementation.
    // service 持有精确的生产检查状态事务实现。
    let service = ProviderManifestSyncService::new(
        directory.path(),
        build_sync_http_client().expect("sync client"),
        Vec::new(),
        None,
    );
    // activated is recorded only after callers would have completed all projections.
    // activated 仅在调用方已完成全部投影后记录。
    let activated = ProviderSyncReport {
        provider_id: String::from("deepseek"),
        outcome: "activated",
        revision: 2,
        failure_kind: None,
        failure_message: None,
        activation_snapshot: None,
    };
    service
        .record_sync_check_reports(std::slice::from_ref(&activated))
        .expect("activation check should record");
    // firstState proves checked and successful initially share the activation boundary.
    // firstState 证明最近检查与最近成功最初共享激活边界。
    let first_state = match service
        .store
        .load_check_state("deepseek")
        .expect("first check state should load")
    {
        CheckStateLoad::Present(state) => state,
        CheckStateLoad::Absent => panic!("activated check state must exist"),
    };
    assert_eq!(first_state.outcome, ManifestCheckOutcome::Activated);
    assert_eq!(
        first_state.last_successful_at_unix_ms,
        Some(first_state.checked_at_unix_ms)
    );
    // upToDate models a successful conditional 304 observation with no new activation.
    // upToDate 模拟未产生新激活的成功条件 304 观测。
    let up_to_date = ProviderSyncReport {
        provider_id: String::from("deepseek"),
        outcome: "up_to_date",
        revision: 2,
        failure_kind: None,
        failure_message: None,
        activation_snapshot: None,
    };
    service
        .record_sync_check_reports(std::slice::from_ref(&up_to_date))
        .expect("304 check should record");
    // rejected models a later candidate failure with a stable non-secret classification.
    // rejected 模拟后续候选以稳定非秘密分类失败。
    let rejected = ProviderSyncReport {
        provider_id: String::from("deepseek"),
        outcome: "rejected",
        revision: 3,
        failure_kind: Some(String::from("schema_semantic_failed")),
        failure_message: None,
        activation_snapshot: None,
    };
    service
        .record_sync_check_reports(std::slice::from_ref(&rejected))
        .expect("rejected check should record");
    // finalState must advance outcome while retaining the proven activation timestamp.
    // finalState 必须推进结果并保留已证明激活时间戳。
    let final_state = match service
        .store
        .load_check_state("deepseek")
        .expect("final check state should load")
    {
        CheckStateLoad::Present(state) => state,
        CheckStateLoad::Absent => panic!("final check state must exist"),
    };
    assert_eq!(final_state.outcome, ManifestCheckOutcome::Rejected);
    assert_eq!(
        final_state.last_error_kind.as_deref(),
        Some("schema_semantic_failed")
    );
    assert_eq!(
        final_state.last_successful_at_unix_ms,
        first_state.last_successful_at_unix_ms
    );
    assert!(final_state.checked_at_unix_ms >= first_state.checked_at_unix_ms);
}
