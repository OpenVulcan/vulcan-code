//! Password-protected provider and credential transfer between Vulcan Code installations.
//! 提供在 Vulcan Code 安装之间使用密码保护的供应商及凭据迁移。

use super::publication::{
    compensated_operation_error, mutate_public_ai_configuration_and_publish_in_transaction,
};
use super::{
    PublicAiSettingsSnapshot, build_public_ai_settings_snapshot,
    public_ai_runtime_and_configuration,
};
use crate::{SharedServiceBridgeState, begin_public_ai_configuration_transaction};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ring::{
    aead, pbkdf2,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{Cursor, Read, Write},
    num::NonZeroU32,
};
use vulcan_provider_adapters::{
    ConnectionModelAccessPolicy, CredentialConnection, CredentialSecretEnvelope,
    CredentialSecretMaterial, CredentialSecretStore, PersistedAiConfiguration,
    is_openai_chatgpt_plan, is_openai_codex_subscription, validate_custom_provider_identifier,
};
use vulcan_provider_config::custom_manifest::{
    CustomProviderManifestDocumentV1, document_from_configuration, install_portable_custom_provider,
};
use vulcan_service_api::ServiceEventEmitter;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

/// Authenticated format identifier. A format change requires a new identifier.
/// 经过认证的格式标识。格式变更时必须使用新标识。
const MAGIC: &[u8; 8] = b"VCMCFG\0\x01";
/// The single ZIP entry holds the authenticated header and encrypted configuration.
/// ZIP 中唯一的条目保存认证过的文件头及加密配置。
const ENTRY: &str = "configuration.aes256gcm";
/// Caps both the RPC transfer and the decoded archive, before any allocation or password work.
/// 在分配内存和处理密码前限制 RPC 传输及解码后的归档大小。
const MAX_ARCHIVE_BYTES: usize = 8 * 1024 * 1024;
/// Version-one key derivation and authenticated encryption parameters.
/// 第一版密钥派生和认证加密参数。
const PASSWORD_ROUNDS: NonZeroU32 = NonZeroU32::new(600_000).unwrap();
const SALT_BYTES: usize = 16;
const NONCE_BYTES: usize = 12;
const HEADER_BYTES: usize = MAGIC.len() + SALT_BYTES + NONCE_BYTES;
const ZIP_OVERHEAD_RESERVE: usize = 1024;

/// Carries the selected connection IDs, provider option and password for a single export.
/// 保存一次导出的连接 ID、供应商选项和密码。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ExportRequest {
    password: String,
    connection_ids: BTreeSet<String>,
    include_other_providers: bool,
}

/// Carries the ciphertext and password needed to inspect one file.
/// 保存预览一份文件所需的密文与密码。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreviewImportRequest {
    password: String,
    archive: String,
}

/// Selects records by their positions in the authenticated file used for preview.
/// 按已认证预览文件中的位置选择要导入的记录。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ImportRequest {
    password: String,
    archive: String,
    account_indexes: BTreeSet<usize>,
    include_other_providers: bool,
}

/// Contains only provider-owned configuration, never session or workspace state.
/// 只包含供应商关联配置，不含会话和工作区状态。
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ConfigurationArchive {
    providers: Vec<CustomProviderManifestDocumentV1>,
    accounts: Vec<ArchivedAccount>,
}

/// Transfers a credential or records the authorization needed on another installation.
/// 迁移凭据，或记录在另一台设备上所需的授权。
#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ArchivedAccount {
    // Keep the credential payload out of each enum slot while retaining the flat tagged wire format.
    // 将凭据载荷移出每个枚举槽位，同时保留扁平的带标签归档格式。
    Portable(Box<PortableAccount>),
    SignInRequired { account: ArchiveAccountIdentity },
}

/// Credentials and settings that may be transferred to another installation.
/// 可迁移到其他安装的凭据与设置。
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PortableAccount {
    /// Account settings without local secret-store references.
    /// 去除本机秘密存储引用的账号设置。
    connection: CredentialConnection,
    /// Credential used by the model service.
    /// 模型服务使用的凭据。
    material: CredentialSecretMaterial,
    /// Optional credential for account management operations.
    /// 可选的账号管理操作凭据。
    management_material: Option<CredentialSecretMaterial>,
    /// Account-specific model access policy, when configured.
    /// 已配置时保存该账号专属的模型访问策略。
    model_access: Option<ConnectionModelAccessPolicy>,
    /// Whether the account was enabled when exported.
    /// 导出时账号是否已启用。
    enabled: bool,
    /// Whether the account was still pending validation when exported.
    /// 导出时账号是否仍待验证。
    pending_validation: bool,
}

/// Identifies an account for selection or sign-in using user-visible metadata.
/// 用用户可见的元数据标识可选择或需重新登录的账号。
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArchiveAccountIdentity {
    provider_id: String,
    service_entry_id: String,
    display_name: String,
}

impl From<&CredentialConnection> for ArchiveAccountIdentity {
    /// Copies a connection's visible account identity for previews and sign-in notices.
    /// 复制连接的可见账号身份，供预览和重新登录提示使用。
    fn from(connection: &CredentialConnection) -> Self {
        Self {
            provider_id: connection.provider_id.clone(),
            service_entry_id: connection.service_entry_id.clone(),
            display_name: connection.display_name.clone(),
        }
    }
}

/// Describes one selectable archive account without secret material.
/// 描述归档中一条可选择的账号，不含秘密材料。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewAccount {
    /// Position in this exact authenticated archive.
    /// 在此份认证归档中的位置。
    index: usize,
    /// Portable connection ID, or null for a sign-in notice.
    /// 可迁移连接的 ID，重新登录记录则为空。
    connection_id: Option<String>,
    #[serde(flatten)]
    account: ArchiveAccountIdentity,
}

/// Lists credential metadata and the number of custom providers available in a file.
/// 列出文件中的凭据元数据和可选的自定义供应商数量。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportPreview {
    accounts: Vec<PreviewAccount>,
    provider_count: usize,
}

/// Returns only a fresh settings projection and the accounts needing sign-in.
/// 仅返回更新后的设置投影及需要重新登录的账号。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportResult {
    snapshot: PublicAiSettingsSnapshot,
    sign_in_required: Vec<ArchiveAccountIdentity>,
    provider_count: usize,
    account_count: usize,
}

/// Exports a consistent provider snapshot with matching portable credential records.
/// 导出一致的供应商快照及对应的可迁移凭据。
pub(crate) async fn export(
    state: SharedServiceBridgeState,
    request: ExportRequest,
) -> Result<String, String> {
    validate_password(&request.password)?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, config) = public_ai_runtime_and_configuration(&state)?;
    let connections = select_export_connections(&config, &request.connection_ids)?;
    let providers =
        select_export_providers(&config, &connections, request.include_other_providers)?;
    require_selection(connections.len(), providers.len())?;
    let store = runtime.credential_secret_store();
    let mut accounts = Vec::new();
    // Resolve the explicit selection before reading any secrets from the store.
    // 在读取任何凭据前先确定用户明确选择的范围。
    for connection in connections {
        let record_sign_in = || ArchivedAccount::SignInRequired {
            account: ArchiveAccountIdentity::from(connection),
        };
        if is_openai_chatgpt_plan(&connection.provider_id, &connection.service_entry_id)
            || is_openai_codex_subscription(&connection.provider_id, &connection.service_entry_id)
        {
            accounts.push(record_sign_in());
            continue;
        }
        let secret = store.read_secret(&connection.secret_reference).await?;
        if secret.is_renewable_oauth() {
            accounts.push(record_sign_in());
            continue;
        }
        let management_material = match connection.management_secret_reference.as_deref() {
            Some(reference) => Some(store.read_secret(reference).await?.material),
            None => None,
        };
        let mut portable_connection = connection.clone();
        portable_connection.secret_reference.clear();
        portable_connection.management_secret_reference = None;
        accounts.push(ArchivedAccount::Portable(Box::new(PortableAccount {
            connection: portable_connection,
            material: secret.material,
            management_material,
            model_access: config
                .connection_model_access_policies
                .get(&connection.id)
                .cloned(),
            enabled: config.connection_is_enabled(&connection.id),
            pending_validation: config.connection_is_pending_validation(&connection.id),
        })));
    }
    drop(transaction);
    tokio::task::spawn_blocking(move || {
        let document = ConfigurationArchive {
            providers,
            accounts,
        };
        encrypt(&document, &request.password).map(|archive| STANDARD.encode(archive))
    })
    .await
    .map_err(|_| String::from("Configuration export could not complete."))?
}

/// Resolves every selected ID against the current snapshot, failing if a selection disappeared.
/// 在当前快照中解析每个已选 ID，已选连接消失时明确失败。
fn select_export_connections<'a>(
    configuration: &'a PersistedAiConfiguration,
    connection_ids: &BTreeSet<String>,
) -> Result<Vec<&'a CredentialConnection>, String> {
    connection_ids
        .iter()
        .map(|id| {
            configuration
                .connection_graph
                .connections
                .get(id)
                .ok_or_else(|| format!("Credential connection `{id}` is no longer available."))
        })
        .collect()
}

/// Includes the required custom definitions and optionally the rest of the source definitions.
/// 选入所需的自定义供应商定义，并可选择附带来源的其余定义。
fn select_export_providers(
    configuration: &PersistedAiConfiguration,
    connections: &[&CredentialConnection],
    include_other_providers: bool,
) -> Result<Vec<CustomProviderManifestDocumentV1>, String> {
    let selected_provider_ids = connections
        .iter()
        .map(|connection| connection.provider_id.as_str())
        .collect::<BTreeSet<_>>();
    require_custom_provider_definitions(
        selected_provider_ids.iter().copied(),
        configuration.custom_providers.keys().map(String::as_str),
    )?;
    if include_other_providers && configuration.custom_provider_manifest_failure_total_count != 0 {
        return Err(String::from(
            "Repair or remove unavailable custom providers before exporting all custom providers.",
        ));
    }
    configuration
        .custom_providers
        .iter()
        .filter(|(id, _)| include_other_providers || selected_provider_ids.contains(id.as_str()))
        .map(|(id, descriptor)| document_from_configuration(configuration, id, descriptor))
        .collect()
}

/// Decrypts a file and returns only the metadata used to make its import selection.
/// 解密文件并仅返回做出导入选择所需的元数据。
pub(crate) async fn preview_import(request: PreviewImportRequest) -> Result<ImportPreview, String> {
    let archive = decode_import(request.archive, request.password).await?;
    Ok(archive.into_preview())
}

/// Authenticates the whole archive and validates the selected records before writing any state.
/// 认证整个归档并校验已选记录，完成后才写入状态。
pub(crate) async fn import(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: ImportRequest,
) -> Result<ImportResult, String> {
    let input = decode_import(request.archive, request.password)
        .await?
        .into_selection(&request.account_indexes, request.include_other_providers)?;
    let _transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, before) = public_ai_runtime_and_configuration(&state)?;
    let mut candidate = before.clone();
    let mut sign_in_required = Vec::new();
    let mut secrets = Vec::new();
    let mut provider_count = 0;
    let mut account_count = 0;
    for document in input.providers {
        provider_count += usize::from(install_portable_custom_provider(&mut candidate, document)?);
    }
    let registry = runtime.effective_providers(&candidate)?;
    for account in input.accounts {
        match account {
            ArchivedAccount::SignInRequired { account } => sign_in_required.push(account),
            ArchivedAccount::Portable(portable) => {
                // Consume the payload without cloning credential material.
                // 直接取出载荷，避免复制凭据内容。
                let PortableAccount {
                    mut connection,
                    material,
                    management_material,
                    model_access,
                    enabled,
                    pending_validation,
                } = *portable;
                if is_openai_chatgpt_plan(&connection.provider_id, &connection.service_entry_id)
                    || is_openai_codex_subscription(
                        &connection.provider_id,
                        &connection.service_entry_id,
                    )
                    || !connection.secret_reference.is_empty()
                    || connection.management_secret_reference.is_some()
                {
                    return Err(String::from("Invalid portable account binding."));
                }
                let secret = CredentialSecretEnvelope::new(material)?;
                if secret.is_renewable_oauth()
                    || secret.credential_mode() != connection.credential_mode
                {
                    return Err(String::from(
                        "Portable credential does not match this account.",
                    ));
                }
                connection.secret_reference =
                    format!("ai_secret_{}", uuid::Uuid::new_v4().simple());
                secrets.push((connection.secret_reference.clone(), secret));
                if let Some(material) = management_material {
                    if !matches!(material, CredentialSecretMaterial::ApiKey { .. }) {
                        return Err(String::from(
                            "Portable management credential must be an API key.",
                        ));
                    }
                    let reference = format!("ai_secret_{}", uuid::Uuid::new_v4().simple());
                    secrets.push((reference.clone(), CredentialSecretEnvelope::new(material)?));
                    connection.management_secret_reference = Some(reference);
                }
                let id = connection.id.clone();
                candidate
                    .connection_graph
                    .add_connection(&registry, connection)?;
                if let Some(policy) = model_access {
                    candidate
                        .connection_model_access_policies
                        .insert(id.clone(), policy);
                }
                candidate.set_connection_enabled(&id, enabled)?;
                if pending_validation {
                    candidate.pending_validation_connection_ids.insert(id);
                }
                account_count += 1;
            }
        }
    }
    let store = runtime.credential_secret_store();
    let mut created = Vec::new();
    for (reference, envelope) in secrets {
        if let Err(primary) = store.create_secret(&reference, &envelope).await {
            return Err(rollback_import(store.as_ref(), &created, primary).await);
        }
        created.push(reference);
    }
    let result = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &before,
        "Could not save imported provider configuration",
        |configuration| {
            *configuration = candidate;
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    );
    match result {
        Ok((_, snapshot)) => Ok(ImportResult {
            snapshot,
            sign_in_required,
            provider_count,
            account_count,
        }),
        Err(primary) => Err(rollback_import(store.as_ref(), &created, primary).await),
    }
}

impl ConfigurationArchive {
    /// Projects safe account information and drops the decrypted secrets and manifests.
    /// 投影可展示的账号信息，丢弃解密后的秘密和清单。
    fn into_preview(self) -> ImportPreview {
        let accounts = self
            .accounts
            .into_iter()
            .enumerate()
            .map(|(index, archived)| {
                let (connection_id, account) = match archived {
                    ArchivedAccount::Portable(portable) => {
                        let account = ArchiveAccountIdentity::from(&portable.connection);
                        (Some(portable.connection.id), account)
                    }
                    ArchivedAccount::SignInRequired { account } => (None, account),
                };
                PreviewAccount {
                    index,
                    connection_id,
                    account,
                }
            })
            .collect();
        ImportPreview {
            accounts,
            provider_count: self.providers.len(),
        }
    }

    /// Retains only confirmed accounts and provider definitions included by their selection.
    /// 仅保留已确认的账号以及本次选择涵盖的供应商定义。
    fn into_selection(
        mut self,
        account_indexes: &BTreeSet<usize>,
        include_other_providers: bool,
    ) -> Result<Self, String> {
        if account_indexes
            .last()
            .is_some_and(|&index| index >= self.accounts.len())
        {
            return Err(String::from(
                "A selected account is absent from this archive.",
            ));
        }
        self.accounts = self
            .accounts
            .into_iter()
            .enumerate()
            .filter_map(|(index, account)| account_indexes.contains(&index).then_some(account))
            .collect();
        let selected_provider_ids = self
            .accounts
            .iter()
            .map(|account| match account {
                ArchivedAccount::Portable(portable) => portable.connection.provider_id.as_str(),
                ArchivedAccount::SignInRequired { account } => account.provider_id.as_str(),
            })
            .collect::<BTreeSet<_>>();
        require_custom_provider_definitions(
            selected_provider_ids.iter().copied(),
            self.providers
                .iter()
                .map(|provider| provider.provider_manifest.provider_id.as_str()),
        )?;
        self.providers.retain(|provider| {
            include_other_providers
                || selected_provider_ids.contains(provider.provider_manifest.provider_id.as_str())
        });
        require_selection(self.accounts.len(), self.providers.len())?;
        Ok(self)
    }
}

/// Refuses transfer of a custom account without its portable provider definition.
/// 拒绝迁移缺少可迁移供应商定义的自定义账号。
fn require_custom_provider_definitions<'a>(
    selected: impl Iterator<Item = &'a str>,
    available: impl Iterator<Item = &'a str>,
) -> Result<(), String> {
    let definitions = available.collect::<BTreeSet<_>>();
    for id in selected {
        if validate_custom_provider_identifier(id).is_ok() && !definitions.contains(id) {
            return Err(format!(
                "Custom provider `{id}` is required by a selected account."
            ));
        }
    }
    Ok(())
}

/// Rejects a transfer with neither a credential nor a provider definition.
/// 拒绝既未选择凭据也未选择供应商定义的迁移。
fn require_selection(accounts: usize, providers: usize) -> Result<(), String> {
    if accounts == 0 && providers == 0 {
        return Err(String::from(
            "Select at least one credential or custom provider.",
        ));
    }
    Ok(())
}

/// Bounds and authenticates a supplied file on the blocking pool for preview and import.
/// 在阻塞线程池中限制并认证提供的文件，供预览和导入共用。
async fn decode_import(archive: String, password: String) -> Result<ConfigurationArchive, String> {
    validate_password(&password)?;
    if archive.len() > 4 * MAX_ARCHIVE_BYTES.div_ceil(3) {
        return Err(String::from("Configuration archive is too large."));
    }
    tokio::task::spawn_blocking(move || {
        let zip = STANDARD
            .decode(archive)
            .map_err(|_| String::from("Invalid configuration archive encoding."))?;
        decrypt(&zip, &password)
    })
    .await
    .map_err(|_| String::from("Configuration import could not complete."))?
}

/// Removes only credentials created by the failed import, then returns its safe error.
/// 仅移除失败导入新建的凭据，然后返回不含秘密的错误。
async fn rollback_import(
    store: &dyn CredentialSecretStore,
    created: &[String],
    primary: String,
) -> String {
    let mut cleanup = Ok(());
    for reference in created.iter().rev() {
        if let Err(error) = store.delete_secret(reference).await {
            cleanup = Err(error);
        }
    }
    compensated_operation_error(primary, Ok(()), cleanup)
}

/// Bounds password work and forbids an empty or whitespace-only password.
/// 限制密码处理开销，并拒绝空密码或仅含空白的密码。
fn validate_password(password: &str) -> Result<(), String> {
    if password.trim().is_empty() || password.len() > 1024 {
        return Err(String::from("Enter a valid configuration password."));
    }
    Ok(())
}

/// Derives the AES key from the password and the unique file salt.
/// 用配置密码和文件独立盐值派生 AES 密钥。
fn encryption_key(password: &str, salt: &[u8]) -> Result<aead::LessSafeKey, String> {
    let mut key = [0; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        PASSWORD_ROUNDS,
        salt,
        password.as_bytes(),
        &mut key,
    );
    aead::UnboundKey::new(&aead::AES_256_GCM, &key)
        .map(aead::LessSafeKey::new)
        .map_err(|_| String::from("Could not initialize configuration encryption."))
}

/// Authenticates the version, salt and nonce along with the encrypted JSON in a single ZIP entry.
/// 在唯一的 ZIP 条目内认证版本、盐值及随机数，并加密 JSON。
fn encrypt(document: &ConfigurationArchive, password: &str) -> Result<Vec<u8>, String> {
    let mut ciphertext = serde_json::to_vec(document)
        .map_err(|_| String::from("Could not encode provider configuration."))?;
    if ciphertext.len() > MAX_ARCHIVE_BYTES - ZIP_OVERHEAD_RESERVE {
        return Err(String::from("Configuration archive is too large."));
    }
    let mut header = [0; HEADER_BYTES];
    header[..MAGIC.len()].copy_from_slice(MAGIC);
    SystemRandom::new()
        .fill(&mut header[MAGIC.len()..])
        .map_err(|_| {
            String::from("Could not create the configuration encryption salt and nonce.")
        })?;
    let (salt, nonce) = header[MAGIC.len()..].split_at(SALT_BYTES);
    let key = encryption_key(password, salt)?;
    let nonce = aead::Nonce::try_assume_unique_for_key(nonce)
        .map_err(|_| String::from("Invalid configuration nonce."))?;
    key.seal_in_place_append_tag(nonce, aead::Aad::from(header), &mut ciphertext)
        .map_err(|_| String::from("Could not encrypt provider configuration."))?;
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(
        ENTRY,
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
    )
    .map_err(|_| String::from("Could not create configuration archive."))?;
    zip.write_all(&header)
        .and_then(|()| zip.write_all(&ciphertext))
        .map_err(|_| String::from("Could not write configuration archive."))?;
    Ok(zip
        .finish()
        .map_err(|_| String::from("Could not finish configuration archive."))?
        .into_inner())
}

/// Opens the one bounded ciphertext entry and accepts JSON only after successful authentication.
/// 读取唯一的有界密文条目，并仅在认证通过后解析 JSON。
fn decrypt(bytes: &[u8], password: &str) -> Result<ConfigurationArchive, String> {
    let invalid = || String::from("Invalid or unsupported Vulcan Code configuration archive.");
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(String::from("Configuration archive is too large."));
    }
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|_| invalid())?;
    if zip.len() != 1 {
        return Err(invalid());
    }
    let mut entry = zip.by_index(0).map_err(|_| invalid())?;
    if entry.name().map_err(|_| invalid())? != ENTRY
        || entry.compression() != CompressionMethod::Stored
        || entry.size() > MAX_ARCHIVE_BYTES as u64
    {
        return Err(invalid());
    }
    let mut header = [0; HEADER_BYTES];
    entry.read_exact(&mut header).map_err(|_| invalid())?;
    if &header[..MAGIC.len()] != MAGIC {
        return Err(invalid());
    }
    let mut ciphertext = Vec::new();
    entry
        .take(MAX_ARCHIVE_BYTES as u64)
        .read_to_end(&mut ciphertext)
        .map_err(|_| invalid())?;
    let (salt, nonce) = header[MAGIC.len()..].split_at(SALT_BYTES);
    let key = encryption_key(password, salt)?;
    let nonce = aead::Nonce::try_assume_unique_for_key(nonce).map_err(|_| invalid())?;
    let plaintext = key
        .open_in_place(nonce, aead::Aad::from(header), &mut ciphertext)
        .map_err(|_| String::from("Incorrect configuration password or damaged archive."))?;
    serde_json::from_slice(plaintext).map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ai_credential_secret_store::repair_tests::CredentialRepairFixture,
        tests::{
            public_ai_persistence_scope::PUBLIC_AI_CONFIGURATION_ROOT,
            test_support::{test_agent_runtime_bridge, test_service_state_with_parent_session},
        },
    };
    use std::sync::{Arc, Mutex};
    use vulcan_provider_adapters::{
        CredentialMode, CredentialSecretResolver, CustomProviderAuthentication,
        CustomProviderDefinition, CustomProviderModelDiscoveryMode,
        CustomProviderModelDiscoveryProtocol, CustomProviderProtocol, PublicAiRuntime,
    };

    /// Verifies credential records survive encryption and tampering or a wrong password fails.
    /// 验证凭据记录能加密往返，并拒绝篡改或错误密码。
    #[test]
    fn fixture_configuration_requires_the_correct_password_and_valid_tag() {
        let portable = ArchivedAccount::Portable(Box::new(PortableAccount {
            connection: CredentialConnection {
                id: "fixture-connection".into(),
                provider_id: "custom:fixture".into(),
                service_entry_id: "compatible-api".into(),
                credential_profile_id: "api-key".into(),
                credential_mode: vulcan_provider_adapters::CredentialMode::Token,
                credential_id: "fixture-credential".into(),
                display_name: "Fixture credential".into(),
                secret_reference: String::new(),
                management_secret_reference: None,
                service_origin_override: None,
            },
            material: CredentialSecretMaterial::ApiKey {
                value: "fixture-private-key".into(),
            },
            management_material: None,
            model_access: None,
            enabled: false,
            pending_validation: true,
        }));
        // Tagging the boxed payload must keep the credential fields at the same wire level.
        // 给装箱后的载荷添加标签时，凭据字段仍应位于相同的序列化层级。
        let wire = serde_json::to_value(&portable).unwrap();
        assert_eq!(wire["kind"], "portable");
        assert!(wire.get("connection").is_some() && wire.get("material").is_some());
        let archive = ConfigurationArchive {
            providers: Vec::new(),
            accounts: vec![
                portable,
                ArchivedAccount::SignInRequired {
                    account: ArchiveAccountIdentity {
                        provider_id: "openai".into(),
                        service_entry_id: "chatgpt-plan".into(),
                        display_name: "fixture-private-account".into(),
                    },
                },
            ],
        };
        let bytes = encrypt(&archive, "test password").unwrap();
        assert!(bytes.starts_with(b"PK"));
        assert!(
            !bytes
                .windows(b"fixture-private-account".len())
                .any(|slice| slice == b"fixture-private-account")
        );
        assert!(
            !bytes
                .windows(b"fixture-private-key".len())
                .any(|slice| slice == b"fixture-private-key")
        );
        assert_ne!(bytes, encrypt(&archive, "test password").unwrap());
        assert!(decrypt(&bytes, "different password").is_err());
        let restored = decrypt(&bytes, "test password").unwrap();
        assert!(matches!(
            &restored.accounts[..],
            [ArchivedAccount::Portable(portable), ArchivedAccount::SignInRequired { account }]
                if matches!(&portable.material, CredentialSecretMaterial::ApiKey { value } if value == "fixture-private-key")
                    && !portable.enabled
                    && portable.pending_validation
                    && account.display_name == "fixture-private-account"
        ));
        let mut original = ZipArchive::new(Cursor::new(&bytes)).unwrap();
        let mut entry = Vec::new();
        original
            .by_index(0)
            .unwrap()
            .read_to_end(&mut entry)
            .unwrap();
        entry[HEADER_BYTES] ^= 1;
        let mut modified = ZipWriter::new(Cursor::new(Vec::new()));
        modified
            .start_file(
                ENTRY,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        modified.write_all(&entry).unwrap();
        let tampered = modified.finish().unwrap().into_inner();
        assert!(decrypt(&tampered, "test password").is_err());
    }

    /// Binds a production settings bridge to the fixture's isolated credential database.
    /// 将生产设置桥接绑定到夹具的独立凭据数据库。
    fn transfer_state(fixture: &CredentialRepairFixture) -> SharedServiceBridgeState {
        let mut bridge = test_agent_runtime_bridge();
        bridge.public_ai_runtime =
            Arc::new(PublicAiRuntime::first_batch(fixture.store.clone()).unwrap());
        let mut state = test_service_state_with_parent_session(uuid::Uuid::new_v4());
        state.agent_runtime = Some(bridge);
        state.workspace_snapshot = None;
        Arc::new(Mutex::new(state))
    }

    /// Transfers two selected credentials in separate writes while never reading an unchecked secret.
    /// 分两次写入选定凭据，且始终不读取未勾选的秘密。
    #[tokio::test]
    async fn selected_credentials_can_be_transferred_in_separate_imports() {
        const PASSWORD: &str = "synthetic transfer password";
        const SELECTED_PROVIDER: &str = "custom:archive-transfer";
        const OTHER_PROVIDER: &str = "custom:archive-unselected";
        let source = CredentialRepairFixture::new().await;
        let source_state = transfer_state(&source);
        let (source_runtime, mut configuration) =
            public_ai_runtime_and_configuration(&source_state).unwrap();
        for id in [SELECTED_PROVIDER, OTHER_PROVIDER] {
            let descriptor = CustomProviderDefinition {
                id: id.into(),
                display_name: id.into(),
                base_url: "https://transfer.example.invalid/v1".into(),
                protocols: vec![CustomProviderProtocol::OpenAiChat],
                authentication: CustomProviderAuthentication::BearerAuthorization,
                model_discovery_mode: CustomProviderModelDiscoveryMode::Manual,
                model_discovery_protocol: CustomProviderModelDiscoveryProtocol::OpenAi,
                model_discovery_path: None,
                protocol_routes: Vec::new(),
                request_headers: Default::default(),
                session_header: None,
            }
            .into_descriptor()
            .unwrap();
            let document = document_from_configuration(&configuration, id, &descriptor).unwrap();
            assert!(install_portable_custom_provider(&mut configuration, document).unwrap());
        }
        let registry = source_runtime.effective_providers(&configuration).unwrap();
        for (id, provider_id) in [
            ("first", SELECTED_PROVIDER),
            ("second", SELECTED_PROVIDER),
            ("unchecked", OTHER_PROVIDER),
        ] {
            let secret_reference = format!("source-secret-{id}");
            let connection = CredentialConnection {
                id: id.into(),
                provider_id: provider_id.into(),
                service_entry_id: "default".into(),
                credential_profile_id: "api-key".into(),
                credential_mode: CredentialMode::Token,
                credential_id: format!("credential-{id}"),
                display_name: id.into(),
                secret_reference: secret_reference.clone(),
                management_secret_reference: None,
                service_origin_override: None,
            };
            configuration
                .connection_graph
                .add_connection(&registry, connection)
                .unwrap();
            if id != "unchecked" {
                let envelope = CredentialSecretEnvelope::new(CredentialSecretMaterial::ApiKey {
                    value: format!("synthetic-api-key-{id}"),
                })
                .unwrap();
                source
                    .store
                    .create_secret(&secret_reference, &envelope)
                    .await
                    .unwrap();
            }
        }
        source_runtime
            .reapply_model_catalog_overlays(&configuration)
            .unwrap();
        let archive = export(
            source_state,
            ExportRequest {
                password: PASSWORD.into(),
                connection_ids: BTreeSet::from(["first".into(), "second".into()]),
                include_other_providers: false,
            },
        )
        .await
        .unwrap();
        let preview = preview_import(PreviewImportRequest {
            archive: archive.clone(),
            password: PASSWORD.into(),
        })
        .await
        .unwrap();
        assert_eq!(preview.provider_count, 1);
        assert_eq!(preview.accounts.len(), 2);
        let serialized_preview = serde_json::to_string(&preview).unwrap();
        assert!(!serialized_preview.contains("synthetic-api-key"));
        assert!(!serialized_preview.contains("source-secret"));
        let target = CredentialRepairFixture::new().await;
        let target_state = transfer_state(&target);
        let app = ServiceEventEmitter::new(Vec::new());
        PUBLIC_AI_CONFIGURATION_ROOT
            .scope(target.directory.path().to_path_buf(), async {
                assert!(
                    import(
                        app.clone(),
                        target_state.clone(),
                        ImportRequest {
                            archive: archive.clone(),
                            password: PASSWORD.into(),
                            account_indexes: BTreeSet::from([preview.accounts.len()]),
                            include_other_providers: false,
                        }
                    )
                    .await
                    .is_err()
                );
                for (id, expected_provider_count, expected_accounts) in
                    [("first", 1, 1), ("second", 0, 2)]
                {
                    let index = preview
                        .accounts
                        .iter()
                        .find(|account| account.connection_id.as_deref() == Some(id))
                        .expect("selected account should be in preview")
                        .index;
                    let result = import(
                        app.clone(),
                        target_state.clone(),
                        ImportRequest {
                            archive: archive.clone(),
                            password: PASSWORD.into(),
                            account_indexes: BTreeSet::from([index]),
                            include_other_providers: false,
                        },
                    )
                    .await
                    .unwrap();
                    assert_eq!(result.account_count, 1);
                    assert_eq!(result.provider_count, expected_provider_count);
                    let (_, persisted) =
                        public_ai_runtime_and_configuration(&target_state).unwrap();
                    assert_eq!(
                        persisted.connection_graph.connections.len(),
                        expected_accounts
                    );
                    assert!(!persisted.custom_providers.contains_key(OTHER_PROVIDER));
                    let secret_ref = &persisted.connection_graph.connections[id].secret_reference;
                    assert_ne!(secret_ref, &format!("source-secret-{id}"));
                    let secret = target.store.read_secret(secret_ref).await.unwrap();
                    assert!(
                        matches!(secret.material, CredentialSecretMaterial::ApiKey { value }
                    if value == format!("synthetic-api-key-{id}"))
                    );
                }
            })
            .await;
    }
}
