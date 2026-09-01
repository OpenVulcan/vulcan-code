//! Receipt chronology, durable replay and failed-publication regressions.
//! 回执时序、持久重放及发布失败回归。

use super::*;

/// Binds pending drafts by exact intent and rejects a second unequal receipt for the same intent before publication or hydration.
/// 按精确意图绑定待完成草稿，并在发布或补水前拒绝同意图的第二份不相等回执。
#[test]
fn draft_receipt_identity_is_write_once_and_never_latest_cache()
-> Result<(), Box<dyn std::error::Error>> {
    use vulcan_session_core::v1::UserPromptResourceDeliveryKind;
    // Real isolated catalog publication is required, not just an in-memory predicate.
    // 要求真实隔离目录发布，而非只测试内存谓词。
    let project = tempfile::tempdir()?;
    let registry = ProjectResourceCatalogRegistry::default();
    let source = register(&registry, project.path())?;
    let intent = uuid::Uuid::new_v4().to_string();
    let mut first = binding("file-original", 10);
    first
        .upload_receipt
        .as_mut()
        .expect("typed receipt")
        .draft_intent_id = Some(intent.clone());
    let pending = UserPromptResourceDeliveryKind::UploadPending {
        draft_intent_id: Some(intent.clone().into_boxed_str()),
        upload_channel_id: "files".into(),
        upload_contract_fingerprint: format!("sha256:{}", "a".repeat(64)),
    };
    registry.record_provider_upload_receipt(
        project.path(),
        source.resource_id,
        &source.sha256,
        source.size,
        first.clone(),
    )?;
    assert!(first.matches_prompt_upload(&pending, &source.sha256, source.size));
    assert!(!binding("file-newer", 20).matches_prompt_upload(
        &pending,
        &source.sha256,
        source.size
    ));
    // Same intent cannot select a newer reference even if every route and source field is equal.
    // 即使全部路线及来源字段相同，同一意图也不能选择更新引用。
    let mut replacement = first.clone();
    replacement.media_id = "file-replacement".into();
    assert!(
        registry
            .record_provider_upload_receipt(
                project.path(),
                source.resource_id,
                &source.sha256,
                source.size,
                replacement.clone()
            )
            .is_err()
    );
    let mut catalog = load_project_resource_catalog(project.path())?;
    assert_eq!(catalog.entries[0].provider_bindings, vec![first]);
    catalog.entries[0].provider_bindings.push(replacement);
    assert!(validate_project_resource_catalog(&catalog).is_err());
    // Legacy omission remains byte-shape compatible and cannot masquerade as this linked draft.
    // 旧字段缺省保持字节形态兼容，且不能冒充本关联草稿。
    let legacy = binding("file-legacy", 1);
    assert!(!serde_json::to_string(&legacy)?.contains("draft_intent_id"));
    let mut invalid = legacy;
    invalid
        .upload_receipt
        .as_mut()
        .expect("typed receipt")
        .draft_intent_id = Some("not-an-intent".into());
    assert!(invalid.validate_upload_receipt().is_err());
    Ok(())
}

/// Builds a typed private receipt for exact test reference `reference` and creation time `created`.
/// 为精确测试引用`reference`及创建时间`created`构建强类型私有回执。
fn binding(reference: &str, created: u64) -> ProviderBinding {
    ProviderBinding {
        provider: "provider".into(),
        connection_id: Some("connection".into()),
        media_id: reference.into(),
        upload_receipt: Some(ProjectResourceUploadReceipt {
            draft_intent_id: None,
            version: 1,
            service_entry_id: "service".into(),
            upload_channel_id: "files".into(),
            upload_contract_fingerprint: format!("sha256:{}", "a".repeat(64)),
            resource_kind: ResourceKind::Video,
            mime_type: "video/mp4".into(),
            source_kind: ResourceSourceKind::ProviderFileId,
            session_scope_id: Some("session".into()),
            created_at_unix_millis: created,
            expires_at_unix_millis: created + 1000,
        }),
    }
}

/// Registers one source and returns its exact logical catalog entry for receipt tests.
/// 注册一个来源，并返回其精确逻辑目录条目供回执测试使用。
fn register(
    registry: &ProjectResourceCatalogRegistry,
    project: &Path,
) -> Result<ResourceCatalogEntry, ProjectResourceCatalogError> {
    registry.resolve_or_register_project_resources(
        project,
        &[ProjectResourceRegistration {
            original_name: "clip.mp4".into(),
            mime: Some("video/mp4".into()),
            content: b"original source".to_vec(),
            provider_bindings: Vec::new(),
        }],
    )?;
    Ok(load_project_resource_catalog(project)?.entries.remove(0))
}

/// Verifies later uploads never overwrite earlier receipts and reload preserves their exact order and fingerprints.
/// 验证后续上传绝不覆盖更早回执，重载保留精确顺序和指纹。
#[test]
fn chronological_receipts_are_append_only_idempotent_and_durable()
-> Result<(), Box<dyn std::error::Error>> {
    // An isolated temporary project owns every file touched by this test.
    // 隔离临时项目持有本测试涉及的全部文件。
    let project = tempfile::tempdir()?;
    // The existing project owner serializes resource catalog mutations.
    // 既有项目所有者串行化资源目录修改。
    let registry = ProjectResourceCatalogRegistry::default();
    // The exact source identity is retained independently of later receipt or file changes.
    // 精确来源身份独立于后续回执或文件变化保留。
    let source = register(&registry, project.path())?;
    // The earlier receipt remains the immutable reference for its original prompt.
    // 更早回执保持为原提示词的不可变引用。
    let first = binding("file-first", 10);
    // The later upload has its own receipt and cannot overwrite the earlier one.
    // 后续上传拥有独立回执，不能覆盖更早回执。
    let second = binding("file-second", 20);
    // The first receipt fingerprint identifies its original source and lifetime.
    // 首份回执指纹标识其原来源和生命周期。
    let first_fingerprint = registry.record_provider_upload_receipt(
        project.path(),
        source.resource_id,
        &source.sha256,
        source.size,
        first.clone(),
    )?;
    // A later receipt must receive a distinct opaque identity.
    // 后续回执必须获得不同的不透明身份。
    let second_fingerprint = registry.record_provider_upload_receipt(
        project.path(),
        source.resource_id,
        &source.sha256,
        source.size,
        second.clone(),
    )?;
    assert_ne!(first_fingerprint, second_fingerprint);
    assert_eq!(
        registry.record_provider_upload_receipt(
            project.path(),
            source.resource_id,
            &source.sha256,
            source.size,
            first.clone()
        )?,
        first_fingerprint
    );
    // Reload durable bytes instead of relying on the writer's in-memory copy.
    // 重载可靠字节，而不依赖写入器内存副本。
    let loaded = load_project_resource_catalog(project.path())?;
    assert_eq!(loaded.next_resource_id, 2);
    assert_eq!(loaded.entries[0].provider_bindings, vec![first, second]);
    assert_eq!(
        loaded.entries[0].provider_bindings[0]
            .upload_receipt_fingerprint(&source.sha256, source.size)?,
        first_fingerprint
    );
    Ok(())
}

/// Rejects source drift and malformed receipts without publishing private values or consuming ids.
/// 拒绝源漂移和损坏回执，不发布私有值也不消耗ID。
#[test]
fn receipt_rejection_preserves_catalog_and_hides_private_reference()
-> Result<(), Box<dyn std::error::Error>> {
    // An isolated temporary project owns every file touched by this test.
    // 隔离临时项目持有本测试涉及的全部文件。
    let project = tempfile::tempdir()?;
    // The existing project owner serializes resource catalog mutations.
    // 既有项目所有者串行化资源目录修改。
    let registry = ProjectResourceCatalogRegistry::default();
    // The exact source identity is retained independently of later receipt or file changes.
    // 精确来源身份独立于后续回执或文件变化保留。
    let source = register(&registry, project.path())?;
    // Preserve exact catalog bytes to verify failure atomicity.
    // 保留精确目录字节以验证失败原子性。
    let original = fs::read(project_resource_catalog_path(project.path()))?;
    assert!(
        registry
            .record_provider_upload_receipt(
                project.path(),
                source.resource_id,
                &source.sha256,
                source.size + 1,
                binding("file-secret", 10)
            )
            .is_err()
    );
    // This malformed receipt deliberately contains a private reference that errors must not expose.
    // 此无效回执故意包含错误不得暴露的私有引用。
    let mut invalid = binding("https://private.example/secret?token=secret", 10);
    invalid
        .upload_receipt
        .as_mut()
        .ok_or("fixture receipt missing")?
        .upload_contract_fingerprint = "a".repeat(64);
    // The failure remains inspectable without publishing the rejected receipt.
    // 失败可以检查，而被拒回执不会发布。
    let error = registry
        .record_provider_upload_receipt(
            project.path(),
            source.resource_id,
            &source.sha256,
            source.size,
            invalid,
        )
        .expect_err("invalid fingerprint must fail");
    assert!(!error.to_string().contains("private.example"));
    assert!(!error.to_string().contains("token="));
    // Different MIME cannot bind even when source bytes and route otherwise match.
    // 即使源字节和路线匹配，不同MIME也不能绑定。
    let mut different_mime = binding("file-secret", 10);
    different_mime
        .upload_receipt
        .as_mut()
        .ok_or("fixture receipt missing")?
        .mime_type = "video/webm".into();
    assert!(
        registry
            .record_provider_upload_receipt(
                project.path(),
                source.resource_id,
                &source.sha256,
                source.size,
                different_mime
            )
            .is_err()
    );
    assert_eq!(
        fs::read(project_resource_catalog_path(project.path()))?,
        original
    );
    Ok(())
}

/// Verifies fingerprint ownership covers every source identity and original lifetime, while legacy receipts cannot claim proof.
/// 验证指纹归属覆盖所有源身份和原生命周期，旧回执不能伪称已有证明。
#[test]
fn fingerprint_covers_source_and_lifetime_and_rejects_legacy()
-> Result<(), Box<dyn std::error::Error>> {
    // The earlier receipt remains the immutable reference for its original prompt.
    // 更早回执保持为原提示词的不可变引用。
    let first = binding("file", 10);
    // The exact source identity is retained independently of later receipt or file changes.
    // 精确来源身份独立于后续回执或文件变化保留。
    let source = "b".repeat(64);
    // The reference digest covers immutable source and receipt facts.
    // 引用摘要覆盖不可变来源和回执事实。
    let fingerprint = first.upload_receipt_fingerprint(&source, 4)?;
    assert_ne!(fingerprint, first.upload_receipt_fingerprint(&source, 5)?);
    assert_ne!(
        fingerprint,
        first.upload_receipt_fingerprint(&"c".repeat(64), 4)?
    );
    assert_ne!(
        fingerprint,
        binding("file", 11).upload_receipt_fingerprint(&source, 4)?
    );
    // A legacy record remains readable without claiming verified upload provenance.
    // 旧记录仍可读取，但不能声称具有已验证上传来源。
    let legacy: ProviderBinding =
        serde_json::from_str(r#"{"provider":"provider","media_id":"old-file"}"#)?;
    legacy.validate_upload_receipt()?;
    assert!(legacy.upload_receipt_fingerprint(&source, 4).is_err());
    Ok(())
}

/// Verifies a failed atomic receipt publication never contaminates the in-memory project owner.
/// 验证回执原子发布失败绝不污染内存项目所有者。
#[test]
fn failed_receipt_publication_keeps_prior_receipts() -> Result<(), Box<dyn std::error::Error>> {
    // An isolated temporary project owns every file touched by this test.
    // 隔离临时项目持有本测试涉及的全部文件。
    let project = tempfile::tempdir()?;
    // The existing project owner serializes resource catalog mutations.
    // 既有项目所有者串行化资源目录修改。
    let registry = ProjectResourceCatalogRegistry::default();
    // The exact source identity is retained independently of later receipt or file changes.
    // 精确来源身份独立于后续回执或文件变化保留。
    let source = register(&registry, project.path())?;
    // Only the fixture's exact catalog path is used for failure injection.
    // 故障注入仅使用夹具的精确目录文件路径。
    let catalog_path = project_resource_catalog_path(project.path());
    // The fixture preserves its catalog while constructing a publication failure.
    // 夹具在构造发布失败时保留目录文件。
    let saved_path = project.path().join("saved-catalog.json");
    // Only the fixture-owned catalog is moved to create a deterministic publication failure.
    // 仅移动夹具所有的目录文件，制造确定性的发布失败。
    fs::rename(&catalog_path, &saved_path)?;
    fs::create_dir(&catalog_path)?;
    assert!(
        registry
            .record_provider_upload_receipt(
                project.path(),
                source.resource_id,
                &source.sha256,
                source.size,
                binding("failed", 10)
            )
            .is_err()
    );
    fs::remove_dir(&catalog_path)?;
    fs::rename(&saved_path, &catalog_path)?;
    registry.record_provider_upload_receipt(
        project.path(),
        source.resource_id,
        &source.sha256,
        source.size,
        binding("success", 20),
    )?;
    // Read the authoritative persisted catalog for invariant assertions.
    // 读取权威持久化目录进行不变量断言。
    let catalog = load_project_resource_catalog(project.path())?;
    assert_eq!(
        catalog.entries[0].provider_bindings,
        vec![binding("success", 20)]
    );
    Ok(())
}
