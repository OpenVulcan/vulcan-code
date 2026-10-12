//! In-process SQLite storage for public AI credentials.
//! 公共 AI 凭据的进程内 SQLite 存储。

use async_trait::async_trait;

#[cfg(test)]
pub(crate) mod repair_tests;
#[cfg(test)]
mod sqlite_tests;
use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_provider_adapters::{
    BrowserCallbackSecretProjection, CredentialSecretEnvelope, CredentialSecretMaterial,
    CredentialSecretResolver, CredentialSecretSnapshot, CredentialSecretStore, MiniMaxRegion,
    OAuthTokenSet,
};
use vulcan_workspace_store::{
    data_plane::HostRuntimeHandle,
    workspace_store::{
        AiCredentialSecretRecord, create_ai_credential_secret_record,
        delete_ai_credential_secret_record, load_ai_credential_secret_record,
        replace_ai_credential_secret_record, restore_ai_credential_secret_record,
    },
};

/// Stores all public AI credentials in host-owned in-process SQLite.
/// 将全部公共 AI 凭据保存到宿主持有的进程内 SQLite。
#[derive(Clone)]
pub(crate) struct SqliteAiCredentialSecretStore {
    /// Shared application data-plane handle.
    /// 共享应用数据平面句柄。
    controller: HostRuntimeHandle,
}

impl SqliteAiCredentialSecretStore {
    /// Creates one SQLite credential store from the initialized application data plane.
    /// 从已初始化应用数据平面创建 SQLite 凭据仓库。
    ///
    /// # Parameters
    /// 参数
    /// `controller` is the exact Controller runtime handle published during bootstrap.
    /// `controller` 是启动期间发布的精确 Controller 运行时句柄。
    ///
    /// # Returns
    /// 返回值
    /// One store bound to the current application SQLite binding.
    /// 返回绑定当前应用 SQLite 的仓库。
    pub(crate) fn new(controller: HostRuntimeHandle) -> Self {
        Self { controller }
    }
}

#[async_trait]
impl CredentialSecretResolver for SqliteAiCredentialSecretStore {
    /// Reads one optional credential exclusively from the current SQLite store.
    /// 仅从当前 SQLite 仓库读取一个可选凭据。
    async fn read_optional_secret(
        &self,
        secret_reference: &str,
    ) -> Result<Option<CredentialSecretEnvelope>, String> {
        load_ai_credential_secret_record(&self.controller, secret_reference)
            .await?
            .map(credential_envelope_from_record)
            .transpose()
    }
}

#[async_trait]
impl CredentialSecretStore for SqliteAiCredentialSecretStore {
    /// Captures the exact optional row for `secret_reference` without decoding its credential format.
    /// 为 `secret_reference` 捕获精确可选记录，不解码其凭据格式。
    /// Returns an opaque repair snapshot or an explicit database/revision error.
    /// 返回不透明修复快照，或明确的数据库／修订号错误。
    async fn capture_secret(
        &self,
        secret_reference: &str,
    ) -> Result<Option<CredentialSecretSnapshot>, String> {
        load_ai_credential_secret_record(&self.controller, secret_reference)
            .await?
            .map(|record| {
                // Only relational identity is needed to replace or remove an unsupported envelope.
                // 替换或移除不受支持的封装仅需关系化身份。
                let revision = u64::try_from(record.revision)
                    .map_err(|_| String::from("Invalid stored AI credential revision."))?;
                CredentialSecretSnapshot::new(secret_reference, revision, record)
            })
            .transpose()
    }

    /// Restores `snapshot` at an absent `secret_reference` without interpreting its material.
    /// 在缺失的 `secret_reference` 恢复 `snapshot`，不解释其材料。
    /// Returns the exact row restoration result; regular reads still enforce the current format.
    /// 返回精确记录恢复结果；常规读取仍执行当前格式校验。
    async fn restore_snapshot(
        &self,
        secret_reference: &str,
        snapshot: &CredentialSecretSnapshot,
    ) -> Result<(), String> {
        restore_ai_credential_secret_record(&self.controller, snapshot.record(secret_reference)?)
            .await
    }

    /// Rolls back a replacement at `expected_revision` using the raw `snapshot` for `secret_reference`.
    /// 使用 `secret_reference` 的原始 `snapshot` 回滚 `expected_revision` 的替换。
    /// Returns a revision-checked restore with the original format and monotonically advanced revision.
    /// 返回保留原始格式且修订号单调递增的受修订号保护恢复结果。
    async fn rollback_replacement(
        &self,
        secret_reference: &str,
        expected_revision: u64,
        snapshot: &CredentialSecretSnapshot,
    ) -> Result<(), String> {
        // Raw restoration is compensation, not credential migration or permission to execute it.
        // 原始恢复属于补偿，不是凭据迁移，也不授予执行权限。
        let mut record = snapshot
            .record::<AiCredentialSecretRecord>(secret_reference)?
            .clone();
        // SQLite owns the actual revision range used by compare-and-replace.
        // SQLite 持有比较替换实际使用的修订号范围。
        let expected_revision = i64::try_from(expected_revision)
            .map_err(|_| String::from("AI credential revision exceeds SQLite range."))?;
        record.revision = expected_revision
            .checked_add(1)
            .ok_or_else(|| String::from("AI credential secret revision is exhausted."))?;
        record.updated_at_unix_ms = current_unix_millis()?;
        replace_ai_credential_secret_record(&self.controller, expected_revision, &record).await
    }

    /// Creates one new revision-one credential only in SQLite.
    /// 仅在 SQLite 中创建一个修订号为一的新凭据。
    async fn create_secret(
        &self,
        secret_reference: &str,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        secret.validate()?;
        let record = credential_record_from_envelope(secret_reference, secret)?;
        create_ai_credential_secret_record(&self.controller, &record).await
    }

    /// Replaces one SQLite credential through revision compare-and-replace.
    /// 通过修订号比较替换一个 SQLite 凭据。
    async fn replace_secret(
        &self,
        secret_reference: &str,
        expected_revision: u64,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        secret.validate()?;
        let expected_revision = i64::try_from(expected_revision)
            .map_err(|_| String::from("AI credential revision exceeds SQLite range."))?;
        let record = credential_record_from_envelope(secret_reference, secret)?;
        replace_ai_credential_secret_record(&self.controller, expected_revision, &record).await
    }

    /// Deletes one SQLite credential idempotently.
    /// 幂等删除一个 SQLite 凭据。
    async fn delete_secret(&self, secret_reference: &str) -> Result<(), String> {
        delete_ai_credential_secret_record(&self.controller, secret_reference).await
    }

    /// Restores one exact deleted SQLite credential for transaction compensation.
    /// 为事务补偿恢复一个精确删除的 SQLite 凭据。
    async fn restore_secret(
        &self,
        secret_reference: &str,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        secret.validate()?;
        let record = credential_record_from_envelope(secret_reference, secret)?;
        restore_ai_credential_secret_record(&self.controller, &record).await
    }
}

/// Converts one strongly typed credential envelope into relational SQLite fields.
/// 将一个强类型凭据封装转换为关系化 SQLite 字段。
fn credential_record_from_envelope(
    secret_reference: &str,
    secret: &CredentialSecretEnvelope,
) -> Result<AiCredentialSecretRecord, String> {
    secret.validate()?;
    let format_version = i64::from(secret.version);
    let revision = i64::try_from(secret.revision)
        .map_err(|_| String::from("AI credential revision exceeds SQLite range."))?;
    let timestamp = current_unix_millis()?;
    let mut record = AiCredentialSecretRecord {
        secret_reference: secret_reference.to_string(),
        format_version,
        revision,
        material_kind: String::new(),
        secret_value: None,
        access_token: None,
        refresh_token: None,
        issued_at_unix_ms: None,
        expires_at_unix_ms: None,
        scope_text: String::new(),
        account_subject: None,
        authorization_kind: None,
        provider_username: None,
        credential_label: None,
        email: None,
        id_token: None,
        account_id: None,
        oauth_client_id: None,
        oauth_client_secret: None,
        project_id: None,
        quota_project_id: None,
        provider_region: None,
        resource_url: None,
        provider_device_id: None,
        created_at_unix_ms: timestamp,
        updated_at_unix_ms: timestamp,
    };
    match &secret.material {
        CredentialSecretMaterial::OpenVulcanAuth {
            access_token,
            authorization_id,
            region,
            expires_at_unix_ms,
            rotation_pending,
        } => {
            record.material_kind = String::from("openvulcan_auth");
            record.authorization_kind = rotation_pending.then(|| String::from("rotation_pending"));
            record.access_token = Some(access_token.clone());
            // The device authorization UUID is the provider-visible installation identity.
            // 设备授权 UUID 是供应商可见的安装身份。
            record.provider_device_id = Some(authorization_id.clone());
            record.provider_region = Some(match region {
                vulcan_provider_adapters::OpenVulcanRegion::China => String::from("china"),
                vulcan_provider_adapters::OpenVulcanRegion::Global => String::from("global"),
            });
            record.expires_at_unix_ms = Some(
                i64::try_from(*expires_at_unix_ms)
                    .map_err(|_| String::from("OpenVulcan expiry exceeds SQLite range."))?,
            );
        }
        CredentialSecretMaterial::ApiKey { value } => {
            record.material_kind = String::from("api_key");
            record.secret_value = Some(value.clone());
        }
        CredentialSecretMaterial::StaticBearer { access_token } => {
            record.material_kind = String::from("static_bearer");
            record.access_token = Some(access_token.clone());
        }
        CredentialSecretMaterial::OpenAiCodexOAuth {
            tokens,
            id_token,
            account_id,
        } => {
            record.material_kind = String::from("open_ai_codex_oauth");
            apply_oauth_tokens(&mut record, tokens)?;
            record.id_token = id_token.clone();
            record.account_id = account_id.clone();
        }
        CredentialSecretMaterial::ClaudeCodeOAuth { tokens } => {
            record.material_kind = String::from("claude_code_oauth");
            apply_oauth_tokens(&mut record, tokens)?;
        }
        CredentialSecretMaterial::GoogleOAuth {
            tokens,
            client_id,
            client_secret,
            project_id,
            quota_project_id,
        } => {
            record.material_kind = String::from("google_oauth");
            apply_oauth_tokens(&mut record, tokens)?;
            record.oauth_client_id = Some(client_id.clone());
            record.oauth_client_secret = client_secret.clone();
            record.project_id = project_id.clone();
            record.quota_project_id = quota_project_id.clone();
        }
        CredentialSecretMaterial::MiniMaxOAuth {
            tokens,
            region,
            resource_url,
        } => {
            record.material_kind = String::from("mini_max_oauth");
            apply_oauth_tokens(&mut record, tokens)?;
            record.provider_region = Some(match region {
                MiniMaxRegion::Global => String::from("global"),
                MiniMaxRegion::China => String::from("china"),
            });
            record.resource_url = Some(resource_url.clone());
        }
        CredentialSecretMaterial::KimiCodingPlanOAuth { tokens, device_id } => {
            record.material_kind = String::from("kimi_coding_plan_oauth");
            apply_oauth_tokens(&mut record, tokens)?;
            record.provider_device_id = Some(device_id.clone());
        }
        CredentialSecretMaterial::BrowserCallbackAuthorization {
            secret,
            projection,
            provider_subject,
            provider_username,
            credential_label,
        } => {
            match projection {
                BrowserCallbackSecretProjection::ApiKey => {
                    record.material_kind = String::from("api_key");
                    record.secret_value = Some(secret.clone());
                }
                BrowserCallbackSecretProjection::Bearer => {
                    record.material_kind = String::from("static_bearer");
                    record.access_token = Some(secret.clone());
                }
            }
            record.authorization_kind = Some(String::from("browser_callback"));
            record.account_subject = Some(provider_subject.clone());
            record.provider_username = Some(provider_username.clone());
            record.credential_label = Some(credential_label.clone());
        }
    }
    Ok(record)
}

/// Converts one relational SQLite record back into a validated credential envelope.
/// 将一条关系化 SQLite 记录转换回已校验凭据封装。
fn credential_envelope_from_record(
    record: AiCredentialSecretRecord,
) -> Result<CredentialSecretEnvelope, String> {
    let version = u32::try_from(record.format_version)
        .map_err(|_| String::from("Stored AI credential format version is invalid."))?;
    let revision = u64::try_from(record.revision)
        .map_err(|_| String::from("Stored AI credential revision is invalid."))?;
    let material = match (
        record.material_kind.as_str(),
        record.authorization_kind.as_deref(),
    ) {
        ("openvulcan_auth", None | Some("rotation_pending")) => {
            CredentialSecretMaterial::OpenVulcanAuth {
                rotation_pending: record.authorization_kind.is_some(),
                access_token: required_record_text(
                    &record.access_token,
                    "OpenVulcan Auth credential",
                )?,
                authorization_id: required_record_text(
                    &record.provider_device_id,
                    "OpenVulcan authorization id",
                )?,
                region: match required_record_text(&record.provider_region, "OpenVulcan region")?
                    .as_str()
                {
                    "china" => vulcan_provider_adapters::OpenVulcanRegion::China,
                    "global" => vulcan_provider_adapters::OpenVulcanRegion::Global,
                    _ => return Err(String::from("Stored OpenVulcan region is invalid.")),
                },
                expires_at_unix_ms: required_record_u64(
                    record.expires_at_unix_ms,
                    "OpenVulcan expiry",
                )?,
            }
        }
        ("api_key", Some("browser_callback")) => {
            CredentialSecretMaterial::BrowserCallbackAuthorization {
                secret: required_record_text(&record.secret_value, "browser callback API key")?,
                projection: BrowserCallbackSecretProjection::ApiKey,
                provider_subject: required_record_text(
                    &record.account_subject,
                    "browser callback provider subject",
                )?,
                provider_username: required_record_text(
                    &record.provider_username,
                    "browser callback provider user name",
                )?,
                credential_label: required_record_text(
                    &record.credential_label,
                    "browser callback credential label",
                )?,
            }
        }
        ("static_bearer", Some("browser_callback")) => {
            CredentialSecretMaterial::BrowserCallbackAuthorization {
                secret: required_record_text(
                    &record.access_token,
                    "browser callback bearer token",
                )?,
                projection: BrowserCallbackSecretProjection::Bearer,
                provider_subject: required_record_text(
                    &record.account_subject,
                    "browser callback provider subject",
                )?,
                provider_username: required_record_text(
                    &record.provider_username,
                    "browser callback provider user name",
                )?,
                credential_label: required_record_text(
                    &record.credential_label,
                    "browser callback credential label",
                )?,
            }
        }
        ("api_key", None) => CredentialSecretMaterial::ApiKey {
            value: required_record_text(&record.secret_value, "API key")?,
        },
        ("static_bearer", None) => CredentialSecretMaterial::StaticBearer {
            access_token: required_record_text(&record.access_token, "static bearer token")?,
        },
        ("open_ai_codex_oauth", None) => CredentialSecretMaterial::OpenAiCodexOAuth {
            tokens: oauth_tokens_from_record(&record)?,
            id_token: record.id_token.clone(),
            account_id: record.account_id.clone(),
        },
        ("claude_code_oauth", None) => CredentialSecretMaterial::ClaudeCodeOAuth {
            tokens: oauth_tokens_from_record(&record)?,
        },
        ("google_oauth", None) => CredentialSecretMaterial::GoogleOAuth {
            tokens: oauth_tokens_from_record(&record)?,
            client_id: required_record_text(&record.oauth_client_id, "Google OAuth client id")?,
            client_secret: record.oauth_client_secret.clone(),
            project_id: record.project_id.clone(),
            quota_project_id: record.quota_project_id.clone(),
        },
        ("mini_max_oauth", None) => CredentialSecretMaterial::MiniMaxOAuth {
            tokens: oauth_tokens_from_record(&record)?,
            region: match required_record_text(&record.provider_region, "MiniMax region")?.as_str()
            {
                "global" => MiniMaxRegion::Global,
                "china" => MiniMaxRegion::China,
                _ => return Err(String::from("Stored MiniMax region is invalid.")),
            },
            resource_url: required_record_text(&record.resource_url, "MiniMax resource URL")?,
        },
        ("kimi_coding_plan_oauth", None) => CredentialSecretMaterial::KimiCodingPlanOAuth {
            tokens: oauth_tokens_from_record(&record)?,
            device_id: required_record_text(&record.provider_device_id, "Kimi device id")?,
        },
        _ => {
            return Err(String::from(
                "Stored AI credential material kind is unsupported.",
            ));
        }
    };
    let envelope = CredentialSecretEnvelope {
        version,
        revision,
        material,
    };
    envelope.validate()?;
    Ok(envelope)
}

/// Copies common OAuth token fields into one relational record.
/// 将公共 OAuth Token 字段复制到一条关系化记录。
fn apply_oauth_tokens(
    record: &mut AiCredentialSecretRecord,
    tokens: &OAuthTokenSet,
) -> Result<(), String> {
    tokens.validate()?;
    if tokens.scopes.iter().any(|scope| {
        scope
            .as_bytes()
            .iter()
            .any(|byte| byte.is_ascii_whitespace())
    }) {
        return Err(String::from(
            "OAuth scope values must not contain ASCII whitespace.",
        ));
    }
    record.access_token = Some(tokens.access_token.clone());
    record.refresh_token = Some(tokens.refresh_token.clone());
    record.issued_at_unix_ms = Some(
        i64::try_from(tokens.issued_at_unix_ms)
            .map_err(|_| String::from("OAuth issue time exceeds SQLite range."))?,
    );
    record.expires_at_unix_ms = Some(
        i64::try_from(tokens.expires_at_unix_ms)
            .map_err(|_| String::from("OAuth expiration time exceeds SQLite range."))?,
    );
    record.scope_text = tokens.scopes.iter().cloned().collect::<Vec<_>>().join(" ");
    record.account_subject = tokens.account_subject.clone();
    record.email = tokens.email.clone();
    Ok(())
}

/// Reconstructs common OAuth token fields from one relational record.
/// 从一条关系化记录重建公共 OAuth Token 字段。
fn oauth_tokens_from_record(record: &AiCredentialSecretRecord) -> Result<OAuthTokenSet, String> {
    let scopes = if record.scope_text.is_empty() {
        BTreeSet::new()
    } else {
        let values = record.scope_text.split(' ').collect::<Vec<_>>();
        if values.iter().any(|scope| scope.is_empty()) {
            return Err(String::from("Stored OAuth scope field is invalid."));
        }
        values.into_iter().map(str::to_string).collect()
    };
    Ok(OAuthTokenSet {
        access_token: required_record_text(&record.access_token, "OAuth access token")?,
        refresh_token: required_record_text(&record.refresh_token, "OAuth refresh token")?,
        issued_at_unix_ms: required_record_u64(record.issued_at_unix_ms, "OAuth issue time")?,
        expires_at_unix_ms: required_record_u64(
            record.expires_at_unix_ms,
            "OAuth expiration time",
        )?,
        scopes,
        account_subject: record.account_subject.clone(),
        email: record.email.clone(),
    })
}

/// Returns one required non-empty relational text field.
/// 返回一个必需的非空关系文本字段。
fn required_record_text(value: &Option<String>, label: &str) -> Result<String, String> {
    value
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| format!("Stored AI credential {label} is missing."))
}

/// Returns one required non-negative relational integer as an unsigned value.
/// 将一个必需的非负关系整数作为无符号值返回。
fn required_record_u64(value: Option<i64>, label: &str) -> Result<u64, String> {
    let value = value.ok_or_else(|| format!("Stored AI credential {label} is missing."))?;
    u64::try_from(value).map_err(|_| format!("Stored AI credential {label} is invalid."))
}

/// Returns the current Unix timestamp in SQLite's signed millisecond range.
/// 返回 SQLite 有符号毫秒范围内的当前 Unix 时间戳。
fn current_unix_millis() -> Result<i64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| String::from("System clock is earlier than the Unix epoch."))?
        .as_millis();
    i64::try_from(millis).map_err(|_| String::from("System clock exceeds SQLite time range."))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With no inputs, verifies real SQLite creation, rotation, reopening and deletion for both regions.
    /// 无需参数，验证两区域凭据的真实 SQLite 创建、轮换、重新打开和删除，成功时正常返回。
    #[tokio::test]
    async fn openvulcan_native_material_round_trips_with_rotation_fence() {
        for region in [
            vulcan_provider_adapters::OpenVulcanRegion::China,
            vulcan_provider_adapters::OpenVulcanRegion::Global,
        ] {
            // fixture owns each regional database through shutdown and reopening without user credentials.
            // fixture 持有各区域数据库，覆盖关闭和重新打开，完全不接触用户凭据。
            let fixture = repair_tests::CredentialRepairFixture::new().await;
            // original retains the device identity throughout all compare-and-replace operations.
            // original 在所有比较替换操作中保留设备身份。
            let original =
                CredentialSecretEnvelope::new(CredentialSecretMaterial::OpenVulcanAuth {
                    access_token: "a".repeat(43),
                    authorization_id: uuid::Uuid::new_v4().to_string(),
                    region,
                    expires_at_unix_ms: 2_000_000_000_000,
                    rotation_pending: false,
                })
                .expect("native fixture");
            // reference identifies the single synthetic account in this private database.
            // reference 标识当前私有数据库中唯一的合成账户。
            let reference = "native-fixture";
            fixture
                .store
                .create_secret(reference, &original)
                .await
                .expect("persist native credential");
            assert!(
                fixture
                    .store
                    .read_secret(reference)
                    .await
                    .expect("read native credential")
                    == original
            );

            // pending_material closes execution before a refresh request can consume the old token.
            // pending_material 在刷新请求可能消费旧令牌前关闭执行入口。
            let mut pending_material = original.material.clone();
            if let CredentialSecretMaterial::OpenVulcanAuth {
                rotation_pending, ..
            } = &mut pending_material
            {
                *rotation_pending = true;
            }
            // pending advances the revision used by the durable rotation fence.
            // pending 递增持久化轮换占用标记的修订号。
            let pending = original
                .advance_with_material(pending_material)
                .expect("rotation fence");
            fixture
                .store
                .replace_secret(reference, original.revision, &pending)
                .await
                .expect("persist rotation fence");
            assert!(
                fixture
                    .store
                    .replace_secret(reference, original.revision, &pending)
                    .await
                    .is_err()
            );

            // Release the host's exclusive ownership before recreating its database runtime.
            // 重建数据库运行时之前，先释放宿主的独占所有权。
            fixture.store.controller.local.shutdown();
            // reopened owns a new database runtime on the same private file, as a restarted host would.
            // reopened 为同一私有文件创建新数据库运行时，模拟宿主重启。
            let reopened = SqliteAiCredentialSecretStore::new(HostRuntimeHandle::new_for_test(
                vulcan_workspace_store::data_plane::HostRuntimeMetadata {
                    data_root: fixture.directory.path().to_path_buf(),
                    sqlite_path: fixture.store.controller.sqlite_path.clone(),
                    sqlite_binding_id: String::from("native-reopened"),
                    system_statistics_sqlite_path: fixture
                        .store
                        .controller
                        .system_statistics_sqlite_path
                        .clone(),
                    system_statistics_sqlite_binding_id: String::from("native-reopened-usage"),
                },
            ));
            // restored_pending must prevent replay after process-local state has been discarded.
            // restored_pending 必须在进程内状态丢弃后仍阻止重放。
            let restored_pending = reopened
                .read_secret(reference)
                .await
                .expect("read durable rotation fence");
            assert!(restored_pending == pending);
            assert!(restored_pending.execution_secret().is_err());

            // rotated_material records a new token while preserving the permanent regional identity.
            // rotated_material 保存新令牌并保留永久区域身份。
            let mut rotated_material = original.material.clone();
            if let CredentialSecretMaterial::OpenVulcanAuth {
                access_token,
                expires_at_unix_ms,
                ..
            } = &mut rotated_material
            {
                *access_token = "b".repeat(43);
                *expires_at_unix_ms += 1;
            }
            // rotated is the next revision after the pending checkpoint.
            // rotated 是待轮换检查点之后的下一修订。
            let rotated = pending
                .advance_with_material(rotated_material)
                .expect("rotated credential");
            reopened
                .replace_secret(reference, pending.revision, &rotated)
                .await
                .expect("persist rotated credential");
            assert!(
                reopened
                    .read_secret(reference)
                    .await
                    .expect("read rotated credential")
                    == rotated
            );
            assert!(rotated.execution_secret().is_ok());

            // snapshot exercises compensation against the same native table used by login.
            // snapshot 验证与登录使用同一原生凭据表的补偿过程。
            let snapshot = reopened
                .capture_secret(reference)
                .await
                .expect("capture native record")
                .expect("native record");
            reopened
                .delete_secret(reference)
                .await
                .expect("delete native credential");
            assert!(
                reopened
                    .read_optional_secret(reference)
                    .await
                    .expect("read deleted credential")
                    .is_none()
            );
            reopened
                .restore_snapshot(reference, &snapshot)
                .await
                .expect("restore native credential");
            assert!(
                reopened
                    .read_secret(reference)
                    .await
                    .expect("read restored credential")
                    == rotated
            );
            reopened
                .delete_secret(reference)
                .await
                .expect("final delete");
            reopened.controller.local.shutdown();
        }
    }

    /// Builds one reusable valid OAuth token fixture.
    /// 构建一份可复用的有效 OAuth Token 测试数据。
    fn oauth_tokens() -> OAuthTokenSet {
        OAuthTokenSet {
            access_token: String::from("access"),
            refresh_token: String::from("refresh"),
            issued_at_unix_ms: 100,
            expires_at_unix_ms: 200,
            scopes: BTreeSet::from([String::from("account"), String::from("inference")]),
            account_subject: Some(String::from("subject")),
            email: Some(String::from("user@example.com")),
        }
    }

    /// Verifies lossless Kimi relational conversion including its provider device identity.
    /// 验证 Kimi 关系化转换无损保留供应商设备身份。
    #[test]
    fn kimi_oauth_round_trips_through_relational_record() {
        let secret = CredentialSecretEnvelope::new(CredentialSecretMaterial::KimiCodingPlanOAuth {
            tokens: OAuthTokenSet {
                access_token: String::from("access"),
                refresh_token: String::from("refresh"),
                issued_at_unix_ms: 100,
                expires_at_unix_ms: 200,
                scopes: BTreeSet::from([String::from("account"), String::from("inference")]),
                account_subject: Some(String::from("subject")),
                email: Some(String::from("user@kimi.com")),
            },
            device_id: String::from("12345678-1234-1234-1234-123456789abc"),
        })
        .expect("Kimi fixture should be valid");

        let record = credential_record_from_envelope("secret-kimi", &secret)
            .expect("Kimi fixture should map to relational fields");
        let restored = credential_envelope_from_record(record)
            .expect("relational Kimi fields should reconstruct the envelope");

        assert!(restored == secret);
    }

    /// Verifies that OAuth scopes remain a deterministic non-JSON field.
    /// 验证 OAuth Scope 保持确定性的非 JSON 字段。
    #[test]
    fn oauth_scope_field_is_canonical_and_non_json() {
        let secret = CredentialSecretEnvelope::new(CredentialSecretMaterial::ClaudeCodeOAuth {
            tokens: OAuthTokenSet {
                access_token: String::from("access"),
                refresh_token: String::from("refresh"),
                issued_at_unix_ms: 100,
                expires_at_unix_ms: 200,
                scopes: BTreeSet::from([String::from("zeta"), String::from("alpha")]),
                account_subject: None,
                email: None,
            },
        })
        .expect("Claude fixture should be valid");

        let record = credential_record_from_envelope("secret-claude", &secret)
            .expect("Claude fixture should map to relational fields");

        assert_eq!(record.scope_text, "alpha zeta");
        assert!(
            ['[', ']', '{', '}']
                .iter()
                .all(|marker| !record.scope_text.contains(*marker))
        );
    }

    /// With no inputs, verifies all supported materials survive actual SQL writes; returns on success.
    /// 无需参数，验证所有受支持材料经过真实 SQL 写入后保持无损，成功时正常返回。
    #[tokio::test]
    async fn every_supported_credential_material_round_trips() {
        // secrets cover static, renewable and browser callback material projections.
        // secrets 覆盖静态、可续期及浏览器回调材料投影。
        let secrets = vec![
            CredentialSecretEnvelope::new_api_key(String::from("api-key"))
                .expect("API key fixture should be valid"),
            CredentialSecretEnvelope::new(CredentialSecretMaterial::StaticBearer {
                access_token: String::from("bearer"),
            })
            .expect("static bearer fixture should be valid"),
            CredentialSecretEnvelope::new(CredentialSecretMaterial::OpenAiCodexOAuth {
                tokens: oauth_tokens(),
                id_token: Some(String::from("id-token")),
                account_id: Some(String::from("account-id")),
            })
            .expect("OpenAI fixture should be valid"),
            CredentialSecretEnvelope::new(CredentialSecretMaterial::ClaudeCodeOAuth {
                tokens: oauth_tokens(),
            })
            .expect("Claude fixture should be valid"),
            CredentialSecretEnvelope::new(CredentialSecretMaterial::GoogleOAuth {
                tokens: oauth_tokens(),
                client_id: String::from("client-id"),
                client_secret: Some(String::from("client-secret")),
                project_id: Some(String::from("project-id")),
                quota_project_id: Some(String::from("quota-project-id")),
            })
            .expect("Google fixture should be valid"),
            CredentialSecretEnvelope::new(CredentialSecretMaterial::MiniMaxOAuth {
                tokens: oauth_tokens(),
                region: MiniMaxRegion::Global,
                resource_url: String::from("https://api.minimax.io"),
            })
            .expect("MiniMax fixture should be valid"),
            CredentialSecretEnvelope::new(CredentialSecretMaterial::KimiCodingPlanOAuth {
                tokens: oauth_tokens(),
                device_id: String::from("12345678-1234-1234-1234-123456789abc"),
            })
            .expect("Kimi fixture should be valid"),
            CredentialSecretEnvelope::new(CredentialSecretMaterial::BrowserCallbackAuthorization {
                secret: String::from("user_api-key"),
                projection: BrowserCallbackSecretProjection::ApiKey,
                provider_subject: String::from("provider-user-1"),
                provider_username: String::from("Tester"),
                credential_label: String::from("CommandCode CLI"),
            })
            .expect("browser API-key fixture should be valid"),
            CredentialSecretEnvelope::new(CredentialSecretMaterial::BrowserCallbackAuthorization {
                secret: String::from("user_bearer-token"),
                projection: BrowserCallbackSecretProjection::Bearer,
                provider_subject: String::from("provider-user-2"),
                provider_username: String::from("Tester Two"),
                credential_label: String::from("CommandCode Desktop"),
            })
            .expect("browser Bearer fixture should be valid"),
        ];

        // fixture exercises the complete production SQL boundary for every supported material.
        // fixture 为每一种受支持材料执行完整的生产 SQL 边界。
        let fixture = repair_tests::CredentialRepairFixture::new().await;
        for (index, secret) in secrets.into_iter().enumerate() {
            // reference isolates each credential independently of fixture ordering.
            // reference 将各凭据隔离，断言不依赖夹具顺序。
            let reference = format!("secret-{index}");
            fixture
                .store
                .create_secret(&reference, &secret)
                .await
                .expect("persist supported material");
            // restored comes from SQLite, including its actual material and authorization constraints.
            // restored 来自 SQLite，涵盖其实际的材料类别及授权约束。
            let restored = fixture
                .store
                .read_secret(&reference)
                .await
                .expect("read persisted material");
            assert!(restored == secret);
            fixture
                .store
                .delete_secret(&reference)
                .await
                .expect("delete supported material");
        }
    }
}
