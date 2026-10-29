//! Explicit credential replacement and compensation independent of old envelope formats.
//! 独立于旧封装格式的显式凭据替换与补偿。

use vulcan_provider_adapters::{
    CredentialSecretEnvelope, CredentialSecretMaterial, CredentialSecretSnapshot,
    CredentialSecretStore,
};

/// Replaces `reference` with fresh `material` in `store`, retaining an opaque rollback snapshot.
/// 在 `store` 中使用新 `material` 替换 `reference`，保留不透明回滚快照。
/// Returns the optional prior record and validated replacement, or a storage/validation error.
/// 返回可选原始记录和已验证替换封装，或存储／校验错误。
pub(super) async fn replace_credential_material(
    store: &dyn CredentialSecretStore,
    reference: &str,
    material: CredentialSecretMaterial,
) -> Result<(Option<CredentialSecretSnapshot>, CredentialSecretEnvelope), String> {
    // Capture storage identity without authorizing, decoding, or migrating the old material.
    // 捕获存储身份，不授权、解码或迁移旧材料。
    let previous = store.capture_secret(reference).await?;
    // The replacement uses only newly supplied material and the stored monotonic revision.
    // 替换仅使用新提供的材料和已存单调修订号。
    let replacement = match previous.as_ref() {
        Some(previous) => previous.replacement(material)?,
        None => CredentialSecretEnvelope::new(material)?,
    };
    match previous.as_ref() {
        Some(previous) => {
            store
                .replace_secret(reference, previous.revision(), &replacement)
                .await?
        }
        None => store.create_secret(reference, &replacement).await?,
    }
    Ok((previous, replacement))
}

/// Compensates a failed edit of `reference` in `store` using `previous` and `replacement`.
/// 使用 `previous` 和 `replacement` 补偿 `store` 中失败的 `reference` 编辑。
/// Returns a revision-checked rollback or deletion of a newly created credential.
/// 返回受修订号保护的回滚结果，或新建凭据的删除结果。
pub(super) async fn rollback_credential_replacement(
    store: &dyn CredentialSecretStore,
    reference: &str,
    previous: Option<&CredentialSecretSnapshot>,
    replacement: &CredentialSecretEnvelope,
) -> Result<(), String> {
    match previous {
        Some(previous) => {
            store
                .rollback_replacement(reference, replacement.revision, previous)
                .await
        }
        None => store.delete_secret(reference).await,
    }
}

/// Restores a deleted `reference` from optional `previous` through its owning `store`.
/// 通过所属 `store`，从可选 `previous` 恢复已删除的 `reference`。
/// Returns success for original absence, or the raw record restoration result.
/// 原先不存在时返回成功，否则返回原始记录恢复结果。
pub(super) async fn restore_deleted_credential(
    store: &dyn CredentialSecretStore,
    reference: &str,
    previous: Option<&CredentialSecretSnapshot>,
) -> Result<(), String> {
    match previous {
        Some(previous) => store.restore_snapshot(reference, previous).await,
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_credential_secret_store::repair_tests::{
        CredentialRepairFixture, UNSUPPORTED_FORMAT_VERSION,
    };
    use vulcan_provider_adapters::CredentialSecretResolver;

    /// Exercises the actual settings update and delete services with rejected SQLite credentials.
    /// 使用被拒绝的 SQLite 凭据验证真实设置更新和删除服务。
    /// No inputs; asserts repair visibility, successful replacement, and raw rollback after a real save failure.
    /// 无输入；断言修复可见性、成功替换以及真实保存失败后的原始回滚。
    #[tokio::test]
    async fn credential_repair_settings_services_replace_delete_and_compensate_old_records() {
        use super::super::connections::{
            PublicAiConnectionRemoveRequest, PublicAiConnectionUpdateRequest,
            service_remove_public_ai_connection, service_update_public_ai_connection,
        };
        use std::sync::{Arc, Mutex};
        use vulcan_provider_adapters::{
            CredentialConnection, CredentialMode, PersistedAiConfiguration, PublicAiRuntime,
        };
        use vulcan_service_api::ServiceEventEmitter;
        // fixture bounds the database and configuration mutations to one private directory.
        // fixture 将数据库和配置变更限制到同一私有目录。
        let fixture = CredentialRepairFixture::new().await;
        // runtime uses real credential reads and signed static catalogs, without provider I/O.
        // runtime 使用真实凭据读取和签名静态目录，不进行供应商 I/O。
        let runtime =
            Arc::new(PublicAiRuntime::first_batch(fixture.store.clone()).expect("runtime"));
        // configuration retains two rejected accounts for independent replacement and removal.
        // configuration 保留两个被拒绝账号，分别用于替换和移除。
        let mut configuration = PersistedAiConfiguration::default();
        for id in ["replace-old", "delete-old"] {
            fixture.insert(id, UNSUPPORTED_FORMAT_VERSION).await;
            configuration.connection_graph.connections.insert(
                id.into(),
                CredentialConnection {
                    id: id.into(),
                    provider_id: "alibaba".into(),
                    service_entry_id: "token-plan-personal-cn".into(),
                    credential_profile_id: "token-plan-api-key".into(),
                    credential_mode: CredentialMode::Token,
                    credential_id: id.into(),
                    display_name: id.into(),
                    secret_reference: id.into(),
                    management_secret_reference: None,
                    service_origin_override: None,
                },
            );
        }
        runtime
            .activate_startup_manifest_catalogs(
                &configuration,
                &configuration
                    .connection_graph
                    .connections
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>(),
            )
            .await
            .expect("repairable startup");
        // snapshot proves the same projection consumed by settings exposes both failures.
        // snapshot 证明设置实际消费的投影公开两个失败。
        let snapshot =
            super::super::snapshot::build_public_ai_settings_snapshot(&runtime, &configuration)
                .expect("settings snapshot");
        // serialized is the actual browser RPC shape rather than private Rust projection fields.
        // serialized 是真实浏览器 RPC 形状，而非 Rust 私有投影字段。
        let serialized = serde_json::to_value(snapshot).expect("browser snapshot");
        assert_eq!(
            serialized["connections"]
                .as_array()
                .expect("connections")
                .len(),
            2
        );
        assert_eq!(
            serialized["catalogErrors"]
                .as_object()
                .expect("catalog errors")
                .len(),
            2
        );
        // bridge and state use the existing isolated service fixture with no session persistence work.
        // bridge 和 state 使用既有隔离服务夹具，不执行会话持久化工作。
        let mut bridge = crate::tests::test_support::test_agent_runtime_bridge();
        bridge.public_ai_runtime = runtime.clone();
        let mut service = crate::tests::test_support::test_service_state_with_parent_session(
            uuid::Uuid::new_v4(),
        );
        service.agent_runtime = Some(bridge);
        service.workspace_snapshot = None;
        let state = Arc::new(Mutex::new(service));
        // app has no external sink; all service publication still runs normally.
        // app 没有外部接收器；全部服务发布仍正常执行。
        let app = ServiceEventEmitter::new(Vec::new());
        crate::tests::public_ai_persistence_scope::PUBLIC_AI_CONFIGURATION_ROOT
            .scope(fixture.directory.path().to_path_buf(), async {
                vulcan_provider_config::config::save_public_ai_connections_with_custom_manifests(
                    &configuration,
                    fixture.directory.path(),
                )
                .expect("saved fixture");
                // request supplies a fresh key; no old credential material is reused.
                // request 提供新的 Key；不复用旧凭据材料。
                let request = PublicAiConnectionUpdateRequest {
                    connection_id: "replace-old".into(),
                    display_name: "Repaired account".into(),
                    service_entry_id: "token-plan-personal-cn".into(),
                    credential_profile_id: "token-plan-api-key".into(),
                    credential_mode: CredentialMode::Token,
                    secret: Some("fresh-key".into()),
                    management_key: None,
                    remove_management_key: false,
                    service_origin_override: None,
                };
                let repaired =
                    service_update_public_ai_connection(app.clone(), state.clone(), request)
                        .await
                        .expect("repair through settings");
                let repaired = serde_json::to_value(repaired).expect("repaired browser snapshot");
                assert!(
                    !repaired["catalogErrors"]
                        .as_object()
                        .expect("catalog errors")
                        .contains_key("replace-old")
                );
                assert!(fixture.store.read_secret("replace-old").await.is_ok());
                // original and saved bytes prove failed removal restores the exact rejected row and configuration.
                // original 和已存字节证明失败移除恢复精确被拒绝记录及配置。
                let original = fixture.record("delete-old").await.expect("old row");
                let path = fixture.directory.path().join("ai-connections.toml");
                let retained = fixture.directory.path().join("before-removal.toml");
                let saved = std::fs::read(&path).expect("saved configuration");
                std::fs::rename(&path, &retained).expect("retain private configuration");
                std::fs::create_dir(&path).expect("block private save target");
                assert!(
                    service_remove_public_ai_connection(
                        app.clone(),
                        state.clone(),
                        PublicAiConnectionRemoveRequest {
                            connection_id: "delete-old".into(),
                            replacement_default_connection_id: None,
                        }
                    )
                    .await
                    .is_err()
                );
                assert!(fixture.record("delete-old").await.as_ref() == Some(&original));
                assert!(fixture.store.read_secret("delete-old").await.is_err());
                assert!(
                    runtime
                        .configuration_snapshot()
                        .expect("retained configuration")
                        .connection_graph
                        .connections
                        .contains_key("delete-old")
                );
                std::fs::remove_dir(&path).expect("remove private empty obstruction");
                std::fs::rename(&retained, &path).expect("restore private save target");
                assert_eq!(std::fs::read(&path).expect("retained bytes"), saved);
                let removed = service_remove_public_ai_connection(
                    app.clone(),
                    state.clone(),
                    PublicAiConnectionRemoveRequest {
                        connection_id: "delete-old".into(),
                        replacement_default_connection_id: None,
                    },
                )
                .await
                .expect("remove unsupported account through settings");
                let removed = serde_json::to_value(removed).expect("removed browser snapshot");
                assert_eq!(
                    removed["connections"]
                        .as_array()
                        .expect("connections")
                        .len(),
                    1
                );
                assert!(fixture.record("delete-old").await.is_none());
                assert!(fixture.store.read_secret("replace-old").await.is_ok());
            })
            .await;
    }

    /// Checks explicit replacement and compensation against a real unsupported SQLite row.
    /// 针对真实不受支持 SQLite 记录检查显式替换与补偿。
    /// No inputs; assertions ensure old material is never migrated or accepted for execution.
    /// 无输入；断言确保旧材料绝不被迁移或获准执行。
    #[tokio::test]
    async fn credential_repair_replaces_without_decoding_and_restores_original_format() {
        // fixture isolates the real store from every user profile.
        // fixture 将真实仓库与全部用户配置隔离。
        let fixture = CredentialRepairFixture::new().await;
        // original provides exact evidence for compensation, including its rejected format.
        // original 提供包含被拒绝格式的精确补偿证据。
        let mut original = fixture
            .insert("repair-key", UNSUPPORTED_FORMAT_VERSION)
            .await;
        // edit retains the opaque record while storing fresh validated material.
        // edit 保留不透明记录，同时保存新验证的材料。
        let (previous, replacement) = replace_credential_material(
            fixture.store.as_ref(),
            "repair-key",
            CredentialSecretMaterial::ApiKey {
                value: String::from("new-key"),
            },
        )
        .await
        .expect("explicit repair");
        assert_eq!(replacement.revision, original.revision as u64 + 1);
        assert!(fixture.store.read_secret("repair-key").await.is_ok());
        rollback_credential_replacement(
            fixture.store.as_ref(),
            "repair-key",
            previous.as_ref(),
            &replacement,
        )
        .await
        .expect("compensate failed validation or publication");
        // restored must preserve every original field except the monotonic revision and update time.
        // restored 必须保留原始全部字段，单调修订号和更新时间除外。
        let restored = fixture.record("repair-key").await.expect("restored row");
        original.revision += 2;
        original.updated_at_unix_ms = restored.updated_at_unix_ms;
        assert!(restored == original);
        assert!(fixture.store.read_secret("repair-key").await.is_err());
    }

    /// Ensures stale repair compensation cannot overwrite a later credential refresh or edit.
    /// 确保过期修复补偿不能覆盖更晚的凭据刷新或编辑。
    /// No inputs; assertions check compare-and-replace ownership and missing-record recreation.
    /// 无输入；断言检查比较替换所有权和缺失记录重建。
    #[tokio::test]
    async fn credential_repair_preserves_concurrent_revision_and_handles_absence() {
        // fixture owns an isolated schema.
        // fixture 持有隔离结构。
        let fixture = CredentialRepairFixture::new().await;
        fixture
            .insert("repair-key", UNSUPPORTED_FORMAT_VERSION)
            .await;
        // first edit is the transaction whose later compensation will be stale.
        // first 是后续补偿将过期的事务编辑。
        let (previous, first) = replace_credential_material(
            fixture.store.as_ref(),
            "repair-key",
            CredentialSecretMaterial::ApiKey {
                value: String::from("first-key"),
            },
        )
        .await
        .expect("first edit");
        // later edit owns a newer revision that stale compensation must preserve.
        // later 持有更新修订号，过期补偿必须保留它。
        let (_, later) = replace_credential_material(
            fixture.store.as_ref(),
            "repair-key",
            CredentialSecretMaterial::StaticBearer {
                access_token: String::from("fresh-authorization"),
            },
        )
        .await
        .expect("later authorization");
        assert!(
            rollback_credential_replacement(
                fixture.store.as_ref(),
                "repair-key",
                previous.as_ref(),
                &first
            )
            .await
            .is_err()
        );
        assert!(
            fixture
                .store
                .read_secret("repair-key")
                .await
                .expect("current record")
                == later
        );
        // absent edit reconstructs only a newly supplied credential at revision one.
        // absent 编辑仅在修订号一重建新提供的凭据。
        let (absent, created) = replace_credential_material(
            fixture.store.as_ref(),
            "missing-key",
            CredentialSecretMaterial::ApiKey {
                value: String::from("new-key"),
            },
        )
        .await
        .expect("recreate missing credential");
        assert!(absent.is_none());
        assert_eq!(created.revision, 1);
        rollback_credential_replacement(
            fixture.store.as_ref(),
            "missing-key",
            absent.as_ref(),
            &created,
        )
        .await
        .expect("undo creation");
        assert!(fixture.record("missing-key").await.is_none());
    }
}
