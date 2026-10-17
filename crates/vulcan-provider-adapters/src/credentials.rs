//! Versioned credential-secret contracts.
//! 版本化凭据秘密契约。

use crate::{BrowserCallbackSecretProjection, CredentialMode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Current persisted credential-secret format version.
/// 当前持久化凭据秘密格式版本。
pub const CREDENTIAL_SECRET_FORMAT_VERSION: u32 = 1;

/// Identifies the isolated OpenVulcan authorization deployment.
/// 标识相互隔离的 OpenVulcan 授权部署。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenVulcanRegion {
    /// International account authority.
    /// 国际账户授权端。
    Global,
    /// China account authority.
    /// 中国账户授权端。
    China,
}

/// Identifies the trusted MiniMax account and API region.
/// 标识可信的 MiniMax 账号及 API 区域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MiniMaxRegion {
    /// International MiniMax service.
    /// MiniMax 国际服务。
    Global,
    /// Mainland China MiniMax service.
    /// MiniMax 中国大陆服务。
    China,
}

/// Describes one versioned credential secret managed by the host credential store.
/// 描述一个由宿主凭据仓库管理的版本化凭据秘密。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialSecretEnvelope {
    /// Exact secret format version.
    /// 精确秘密格式版本。
    pub version: u32,
    /// Monotonic revision used to reject stale refresh writes.
    /// 用于拒绝过期刷新写入的单调修订号。
    pub revision: u64,
    /// Strongly typed provider credential material.
    /// 强类型供应商凭据材料。
    pub material: CredentialSecretMaterial,
}

/// Identifies every supported credential-secret shape.
/// 标识每一种受支持的凭据秘密结构。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CredentialSecretMaterial {
    /// Renewable OpenCode Console authorization bound to one billing workspace.
    /// 绑定到单一计费工作区的可续期 OpenCode Console 授权。
    OpencodeConsoleOAuth {
        /// Account-owned access and refresh tokens, including verified user identity.
        /// 账号持有的访问与刷新令牌，包含已验证用户身份。
        tokens: OAuthTokenSet,
        /// Exact workspace selected by the device authorization service.
        /// 设备授权服务选定的精确工作区。
        organization_id: String,
    },
    /// OpenDesign's approved non-renewable credential pair, stored and replaced atomically.
    /// OpenDesign 获批且不可续期的凭据对，原子存储及替换。
    OpenDesignAuth {
        /// Runtime key used exclusively for model discovery and inference.
        /// 仅用于模型发现与推理的运行密钥。
        runtime_key: String,
        /// Control key used exclusively for account metadata.
        /// 仅用于账户元数据的控制密钥。
        control_key: String,
        /// Stable user identifier issued alongside both keys.
        /// 与两把密钥同时签发的稳定用户标识。
        user_id: String,
        /// Account email returned by the authorization service.
        /// 授权服务返回的账户邮箱。
        email: String,
    },
    /// Rotating OpenVulcan device authorization without a separate refresh token.
    /// 没有独立刷新令牌的 OpenVulcan 轮换设备授权。
    OpenVulcanAuth {
        /// Exact credential issued by the selected authority.
        /// 所选授权端签发的准确凭据。
        access_token: String,
        /// Permanent device authorization identity preserved across rotations.
        /// 轮换期间保持不变的永久设备授权身份。
        authorization_id: String,
        /// Region fixed at initial authorization.
        /// 初次授权时固定的区域。
        region: OpenVulcanRegion,
        /// Server-owned absolute expiry in Unix milliseconds.
        /// 服务端持有的 Unix 毫秒绝对过期时间。
        expires_at_unix_ms: u64,
        /// Durable single-use rotation fence; true requires a new authorization after interruption.
        /// 持久单次轮换栅栏；为 true 时，中断后必须重新授权。
        rotation_pending: bool,
    },
    /// User-managed API key.
    /// 用户管理的 API Key。
    ApiKey {
        /// Exact key value.
        /// 精确 Key 值。
        value: String,
    },
    /// User-managed static bearer token.
    /// 用户管理的静态 Bearer Token。
    StaticBearer {
        /// Exact bearer token.
        /// 精确 Bearer Token。
        access_token: String,
    },
    /// Renewable OpenAI Codex account authorization.
    /// 可续期 OpenAI Codex 账号授权。
    OpenAiCodexOAuth {
        /// Shared OAuth token set.
        /// 共享 OAuth Token 集合。
        tokens: OAuthTokenSet,
        /// Optional OpenID identity token returned by the authority.
        /// 授权端可选返回的 OpenID 身份 Token。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id_token: Option<String>,
        /// Optional OpenAI account identifier extracted from verified identity data.
        /// 从已验证身份数据提取的可选 OpenAI 账号标识。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account_id: Option<String>,
    },
    /// Renewable Claude Code account authorization.
    /// 可续期 Claude Code 账号授权。
    ClaudeCodeOAuth {
        /// Shared OAuth token set.
        /// 共享 OAuth Token 集合。
        tokens: OAuthTokenSet,
    },
    /// Renewable Google account authorization.
    /// 可续期 Google 账号授权。
    GoogleOAuth {
        /// Shared OAuth token set.
        /// 共享 OAuth Token 集合。
        tokens: OAuthTokenSet,
        /// User-provided Google desktop OAuth client identifier.
        /// 用户提供的 Google 桌面 OAuth 客户端标识。
        client_id: String,
        /// Optional user-provided Google desktop OAuth client secret.
        /// 用户可选提供的 Google 桌面 OAuth 客户端密钥。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_secret: Option<String>,
        /// Optional verified Google Cloud project identifier.
        /// 可选的已验证 Google Cloud 项目标识。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<String>,
        /// Optional verified quota project identifier.
        /// 可选的已验证配额项目标识。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        quota_project_id: Option<String>,
    },
    /// Renewable MiniMax Token Plan authorization.
    /// 可续期 MiniMax Token Plan 授权。
    MiniMaxOAuth {
        /// Shared OAuth token set.
        /// 共享 OAuth Token 集合。
        tokens: OAuthTokenSet,
        /// Region fixed by the selected Device Flow authority.
        /// 由所选 Device Flow 授权端固定的区域。
        region: MiniMaxRegion,
        /// Exact trusted API resource URL returned by MiniMax.
        /// MiniMax 返回的精确可信 API 资源 URL。
        resource_url: String,
    },
    /// Renewable Kimi Coding Plan device authorization.
    /// 可续期的 Kimi Coding Plan 设备授权。
    KimiCodingPlanOAuth {
        /// Shared OAuth token set.
        /// 共享 OAuth Token 集合。
        tokens: OAuthTokenSet,
        /// Stable provider-visible identity required by refresh and usage requests.
        /// 刷新与用量请求所需的稳定供应商可见身份。
        device_id: String,
    },
    /// Host-managed credential delivered by a reviewed browser loopback callback.
    /// 由已审核浏览器回环回调交付并由宿主管理的凭据。
    BrowserCallbackAuthorization {
        /// Delivered secret retained only in the secure credential store.
        /// 仅保留在安全凭据仓库中的交付秘密。
        secret: String,
        /// Request-authentication projection declared by the source Manifest.
        /// 源 Manifest 声明的请求鉴权投影。
        projection: BrowserCallbackSecretProjection,
        /// Stable provider subject returned by the authorization page.
        /// 授权页面返回的稳定供应商主体。
        provider_subject: String,
        /// Provider-visible user name returned by the authorization page.
        /// 授权页面返回的供应商可见用户名。
        provider_username: String,
        /// Provider-owned credential label returned by the authorization page.
        /// 授权页面返回的供应商凭据标签。
        credential_label: String,
    },
}

/// Stores common renewable OAuth token fields without provider guesses.
/// 保存不包含供应商猜测的公共可续期 OAuth Token 字段。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OAuthTokenSet {
    /// Current access token.
    /// 当前 Access Token。
    pub access_token: String,
    /// Refresh token used for the same account.
    /// 用于同一账号的 Refresh Token。
    pub refresh_token: String,
    /// Token issue time as Unix milliseconds.
    /// Token 签发时间，Unix 毫秒。
    pub issued_at_unix_ms: u64,
    /// Token expiration time as Unix milliseconds.
    /// Token 过期时间，Unix 毫秒。
    pub expires_at_unix_ms: u64,
    /// Exact granted OAuth scopes.
    /// 精确授予的 OAuth Scope。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub scopes: BTreeSet<String>,
    /// Optional stable account subject.
    /// 可选稳定账号 Subject。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_subject: Option<String>,
    /// Optional verified account email.
    /// 可选已验证账号邮箱。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// Identifies how resolved secret material must be projected into authentication.
/// 标识解析后的秘密材料必须如何投影到鉴权。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialExecutionSecretKind {
    /// OpenVulcan's exact Auth scheme, never interchangeable with Bearer.
    /// OpenVulcan 的准确 Auth 方案，不能与 Bearer 互换。
    OpenVulcanAuth,
    /// API-key material whose exact header is selected by the service entry.
    /// 由服务入口选择精确请求头的 API Key 材料。
    ApiKey,
    /// Bearer material used for static or renewable authorization.
    /// 用于静态或可续期授权的 Bearer 材料。
    Bearer,
}

/// Carries one validated execution secret without implementing debug serialization.
/// 承载一个已验证的执行秘密，且不实现调试序列化。
#[derive(Clone, PartialEq, Eq)]
pub struct CredentialExecutionSecret {
    /// OpenCode billing workspace, kept separate from the Codex account header.
    /// OpenCode 计费工作区，与 Codex 账号请求头分开保存。
    opencode_organization_id: Option<String>,
    /// Private OpenDesign control credential; never returned by the inference value accessor.
    /// 私有 OpenDesign 控制凭据；绝不由推理值访问器返回。
    open_design_control_key: Option<String>,
    /// Secret-store revision frozen for this execution.
    /// 为本次执行冻结的秘密存储修订号。
    revision: u64,
    /// Authentication projection category.
    /// 鉴权投影类别。
    kind: CredentialExecutionSecretKind,
    /// Exact secret value.
    /// 精确秘密值。
    value: String,
    /// Optional account identity required by the exact upstream route.
    /// 精确上游路由所需的可选账号身份。
    account_id: Option<String>,
    /// Optional Google quota project required by the exact upstream route.
    /// 精确上游路由所需的可选 Google 配额项目。
    quota_project_id: Option<String>,
    /// Optional Google Code Assist project required in the exact upstream request body.
    /// 精确上游请求正文所需的可选 Google Code Assist 项目。
    project_id: Option<String>,
    /// Optional provider-visible device identity required by account metadata requests.
    /// 账号元数据请求所需的可选供应商可见设备身份。
    provider_device_id: Option<String>,
    /// Native credential deployment, preventing cross-region credential forwarding.
    /// 原生凭据部署，防止跨区域转发凭据。
    openvulcan_region: Option<OpenVulcanRegion>,
}

impl CredentialSecretEnvelope {
    /// Creates the first revision for validated strongly typed credential material.
    /// 为已验证的强类型凭据材料创建第一版秘密。
    ///
    /// # Parameters
    /// 参数
    /// `material` is the complete result of one credential verification or OAuth exchange.
    /// `material` 是一次凭据验证或 OAuth 交换的完整结果。
    ///
    /// # Returns
    /// 返回值
    /// A validated version-one envelope.
    /// 返回已验证的第一版封装。
    pub fn new(material: CredentialSecretMaterial) -> Result<Self, String> {
        let envelope = Self {
            version: CREDENTIAL_SECRET_FORMAT_VERSION,
            revision: 1,
            material,
        };
        envelope.validate()?;
        Ok(envelope)
    }

    /// Creates the first revision for a user-managed API key.
    /// 为用户管理的 API Key 创建第一版秘密。
    ///
    /// # Parameters
    /// 参数
    /// `value` is the non-empty key supplied by the user.
    /// `value` 是用户提供的非空 Key。
    ///
    /// # Returns
    /// 返回值
    /// A validated version-one envelope.
    /// 返回已验证的第一版秘密封装。
    pub fn new_api_key(value: String) -> Result<Self, String> {
        Self::new(CredentialSecretMaterial::ApiKey { value })
    }

    /// Replaces user-managed API-key material with the next revision.
    /// 使用下一修订号替换用户管理的 API Key 材料。
    ///
    /// # Parameters
    /// 参数
    /// `value` is the newly validated key supplied by the user.
    /// `value` 是用户新提供且经过校验的 Key。
    ///
    /// # Returns
    /// 返回值
    /// A new envelope whose revision is exactly one greater than the current revision.
    /// 返回修订号严格递增一的新封装。
    pub fn replace_api_key(&self, value: String) -> Result<Self, String> {
        self.advance_with_material(CredentialSecretMaterial::ApiKey { value })
    }

    /// Advances the revision while replacing strongly typed credential material.
    /// 在替换强类型凭据材料时递增修订号。
    ///
    /// # Parameters
    /// 参数
    /// `material` is a complete replacement produced by validation or refresh.
    /// `material` 是由验证或刷新产生的完整替换材料。
    ///
    /// # Returns
    /// 返回值
    /// A validated envelope whose revision is exactly one greater.
    /// 返回修订号严格递增一的已验证封装。
    pub fn advance_with_material(
        &self,
        material: CredentialSecretMaterial,
    ) -> Result<Self, String> {
        self.validate()?;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| String::from("AI credential secret revision is exhausted."))?;
        let envelope = Self {
            version: CREDENTIAL_SECRET_FORMAT_VERSION,
            revision,
            material,
        };
        envelope.validate()?;
        Ok(envelope)
    }

    /// Deserializes and validates one strict credential-secret document.
    /// 反序列化并校验一个严格凭据秘密文档。
    ///
    /// # Parameters
    /// 参数
    /// `document` is one exact host-supplied serialized envelope.
    /// `document` 是宿主提供的一份精确序列化封装。
    ///
    /// # Returns
    /// 返回值
    /// A validated envelope, or an explicit unsupported-format error.
    /// 返回已验证封装，或明确的不受支持格式错误。
    pub fn from_json(document: &str) -> Result<Self, String> {
        let envelope = serde_json::from_str::<Self>(document).map_err(|_| {
            String::from(
                "Unsupported AI credential secret format; remove and reconnect this account.",
            )
        })?;
        envelope.validate()?;
        Ok(envelope)
    }

    /// Serializes one validated credential-secret document.
    /// 序列化一个已验证凭据秘密文档。
    ///
    /// # Returns
    /// 返回值
    /// The exact JSON document written only to the secure store.
    /// 返回仅写入安全仓库的精确 JSON 文档。
    pub fn to_json(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|_| String::from("Failed to encode the AI credential secret securely."))
    }

    /// Returns the credential acquisition mode represented by the material.
    /// 返回该材料表示的凭据获取方式。
    ///
    /// # Returns
    /// 返回值
    /// `Token` for user-managed material and `Auth` for host-managed authorization.
    /// 用户管理材料返回 `Token`，宿主管理授权返回 `Auth`。
    pub fn credential_mode(&self) -> CredentialMode {
        match &self.material {
            CredentialSecretMaterial::ApiKey { .. }
            | CredentialSecretMaterial::StaticBearer { .. } => CredentialMode::Token,
            CredentialSecretMaterial::OpenAiCodexOAuth { .. }
            | CredentialSecretMaterial::OpencodeConsoleOAuth { .. }
            | CredentialSecretMaterial::ClaudeCodeOAuth { .. }
            | CredentialSecretMaterial::GoogleOAuth { .. }
            | CredentialSecretMaterial::MiniMaxOAuth { .. }
            | CredentialSecretMaterial::KimiCodingPlanOAuth { .. }
            | CredentialSecretMaterial::OpenVulcanAuth { .. }
            | CredentialSecretMaterial::OpenDesignAuth { .. }
            | CredentialSecretMaterial::BrowserCallbackAuthorization { .. } => CredentialMode::Auth,
        }
    }

    /// Reports whether this credential owns a renewable OAuth token set.
    /// 判断当前凭据是否持有可续期 OAuth Token 集合。
    ///
    /// # Returns
    /// 返回值
    ///
    /// `true` only for credentials whose access token can be renewed by the OAuth runtime.
    /// 仅当 OAuth 运行时能够续期该凭据的 Access Token 时返回 `true`。
    pub fn is_renewable_oauth(&self) -> bool {
        matches!(
            self.material,
            CredentialSecretMaterial::OpenAiCodexOAuth { .. }
                | CredentialSecretMaterial::OpencodeConsoleOAuth { .. }
                | CredentialSecretMaterial::ClaudeCodeOAuth { .. }
                | CredentialSecretMaterial::GoogleOAuth { .. }
                | CredentialSecretMaterial::MiniMaxOAuth { .. }
                | CredentialSecretMaterial::KimiCodingPlanOAuth { .. }
                | CredentialSecretMaterial::OpenVulcanAuth { .. }
        )
    }

    /// Resolves the current access material for one immutable execution.
    /// 为一次不可变执行解析当前访问材料。
    ///
    /// # Returns
    /// 返回值
    /// A non-debuggable secret value and its frozen revision.
    /// 返回不可调试输出的秘密值及其冻结修订号。
    pub fn execution_secret(&self) -> Result<CredentialExecutionSecret, String> {
        self.validate()?;
        if matches!(
            &self.material,
            CredentialSecretMaterial::OpenVulcanAuth {
                rotation_pending: true,
                ..
            }
        ) {
            return Err(String::from(
                "OpenVulcan rotation did not complete; sign in again.",
            ));
        }
        let (kind, value, account_id, quota_project_id, project_id, provider_device_id) =
            match &self.material {
                CredentialSecretMaterial::OpenDesignAuth { runtime_key, .. } => (
                    CredentialExecutionSecretKind::Bearer,
                    runtime_key.clone(),
                    None,
                    None,
                    None,
                    None,
                ),
                CredentialSecretMaterial::OpenVulcanAuth {
                    access_token,
                    authorization_id,
                    ..
                } => (
                    CredentialExecutionSecretKind::OpenVulcanAuth,
                    access_token.clone(),
                    None,
                    None,
                    None,
                    Some(authorization_id.clone()),
                ),
                CredentialSecretMaterial::ApiKey { value } => (
                    CredentialExecutionSecretKind::ApiKey,
                    value.clone(),
                    None,
                    None,
                    None,
                    None,
                ),
                CredentialSecretMaterial::StaticBearer { access_token } => (
                    CredentialExecutionSecretKind::Bearer,
                    access_token.clone(),
                    None,
                    None,
                    None,
                    None,
                ),
                CredentialSecretMaterial::OpenAiCodexOAuth {
                    tokens, account_id, ..
                } => (
                    CredentialExecutionSecretKind::Bearer,
                    tokens.access_token.clone(),
                    account_id.clone(),
                    None,
                    None,
                    None,
                ),
                CredentialSecretMaterial::OpencodeConsoleOAuth { tokens, .. }
                | CredentialSecretMaterial::ClaudeCodeOAuth { tokens } => (
                    CredentialExecutionSecretKind::Bearer,
                    tokens.access_token.clone(),
                    None,
                    None,
                    None,
                    None,
                ),
                CredentialSecretMaterial::GoogleOAuth {
                    tokens,
                    project_id,
                    quota_project_id,
                    ..
                } => (
                    CredentialExecutionSecretKind::Bearer,
                    tokens.access_token.clone(),
                    None,
                    quota_project_id.clone(),
                    project_id.clone(),
                    None,
                ),
                CredentialSecretMaterial::MiniMaxOAuth { tokens, .. } => (
                    CredentialExecutionSecretKind::Bearer,
                    tokens.access_token.clone(),
                    None,
                    None,
                    None,
                    None,
                ),
                CredentialSecretMaterial::KimiCodingPlanOAuth { tokens, device_id } => (
                    CredentialExecutionSecretKind::Bearer,
                    tokens.access_token.clone(),
                    None,
                    None,
                    None,
                    Some(device_id.clone()),
                ),
                CredentialSecretMaterial::BrowserCallbackAuthorization {
                    secret,
                    projection,
                    ..
                } => (
                    match projection {
                        BrowserCallbackSecretProjection::ApiKey => {
                            CredentialExecutionSecretKind::ApiKey
                        }
                        BrowserCallbackSecretProjection::Bearer => {
                            CredentialExecutionSecretKind::Bearer
                        }
                    },
                    secret.clone(),
                    None,
                    None,
                    None,
                    None,
                ),
            };
        Ok(CredentialExecutionSecret {
            opencode_organization_id: match &self.material {
                CredentialSecretMaterial::OpencodeConsoleOAuth {
                    organization_id, ..
                } => Some(organization_id.clone()),
                _ => None,
            },
            open_design_control_key: match &self.material {
                CredentialSecretMaterial::OpenDesignAuth { control_key, .. } => {
                    Some(control_key.clone())
                }
                _ => None,
            },
            openvulcan_region: match &self.material {
                CredentialSecretMaterial::OpenVulcanAuth { region, .. } => Some(*region),
                _ => None,
            },
            revision: self.revision,
            kind,
            value,
            account_id,
            quota_project_id,
            project_id,
            provider_device_id,
        })
    }

    /// Reports whether renewable access material must be refreshed before a new request.
    /// 报告可续期访问材料在新请求前是否必须刷新。
    ///
    /// # Parameters
    /// 参数
    /// `now_unix_ms` is the current wall-clock time and `refresh_skew_ms` reserves enough time
    /// for the next provider request to start with a usable access token.
    /// `now_unix_ms` 是当前墙钟时间，`refresh_skew_ms` 为下一次供应商请求预留足够的
    /// 可用 Access Token 时间。
    ///
    /// # Returns
    /// 返回值
    /// `true` only for OAuth material at or inside the requested refresh window.
    /// 仅当 OAuth 材料已经进入指定刷新窗口时返回 `true`。
    pub fn requires_refresh(&self, now_unix_ms: u64, refresh_skew_ms: u64) -> bool {
        if let CredentialSecretMaterial::OpenVulcanAuth {
            expires_at_unix_ms,
            rotation_pending,
            ..
        } = &self.material
        {
            return *rotation_pending
                || *expires_at_unix_ms
                    <= now_unix_ms.saturating_add(crate::openvulcan::REFRESH_AHEAD_MS);
        }
        let expires_at_unix_ms = match &self.material {
            CredentialSecretMaterial::OpenVulcanAuth {
                expires_at_unix_ms, ..
            } => *expires_at_unix_ms,
            CredentialSecretMaterial::OpenAiCodexOAuth { tokens, .. }
            | CredentialSecretMaterial::OpencodeConsoleOAuth { tokens, .. }
            | CredentialSecretMaterial::ClaudeCodeOAuth { tokens }
            | CredentialSecretMaterial::GoogleOAuth { tokens, .. }
            | CredentialSecretMaterial::MiniMaxOAuth { tokens, .. }
            | CredentialSecretMaterial::KimiCodingPlanOAuth { tokens, .. } => {
                tokens.expires_at_unix_ms
            }
            CredentialSecretMaterial::ApiKey { .. }
            | CredentialSecretMaterial::OpenDesignAuth { .. }
            | CredentialSecretMaterial::StaticBearer { .. }
            | CredentialSecretMaterial::BrowserCallbackAuthorization { .. } => return false,
        };
        expires_at_unix_ms <= now_unix_ms.saturating_add(refresh_skew_ms)
    }

    /// Validates version, revision, and all secret invariants.
    /// 校验版本、修订号及全部秘密不变量。
    ///
    /// # Returns
    /// 返回值
    /// Success only for a complete currently supported envelope.
    /// 仅完整且当前受支持的封装返回成功。
    pub fn validate(&self) -> Result<(), String> {
        if self.version != CREDENTIAL_SECRET_FORMAT_VERSION {
            return Err(format!(
                "Unsupported AI credential secret version `{}`; remove and reconnect this account.",
                self.version
            ));
        }
        if self.revision == 0 {
            return Err(String::from(
                "AI credential secret revision must be greater than zero.",
            ));
        }
        match &self.material {
            CredentialSecretMaterial::OpencodeConsoleOAuth {
                tokens,
                organization_id,
            } => {
                tokens.validate()?;
                crate::opencode::validate_organization_id(organization_id)?;
                validate_bounded_identity(
                    "OpenCode user id",
                    tokens
                        .account_subject
                        .as_deref()
                        .ok_or("OpenCode user identity is missing.")?,
                )
            }
            CredentialSecretMaterial::OpenDesignAuth {
                runtime_key,
                control_key,
                user_id,
                email,
            } => {
                validate_non_empty_secret("OpenDesign runtime key", runtime_key)?;
                validate_non_empty_secret("OpenDesign control key", control_key)?;
                validate_bounded_identity("OpenDesign user id", user_id)?;
                validate_bounded_identity("OpenDesign email", email)
            }
            CredentialSecretMaterial::OpenVulcanAuth {
                access_token,
                authorization_id,
                expires_at_unix_ms,
                ..
            } => {
                if access_token.len() != 43
                    || !access_token
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
                {
                    return Err(String::from(
                        "OpenVulcan Auth credential has an invalid format.",
                    ));
                }
                if uuid::Uuid::parse_str(authorization_id).is_err() || *expires_at_unix_ms == 0 {
                    return Err(String::from(
                        "OpenVulcan device authorization metadata is invalid.",
                    ));
                }
                Ok(())
            }
            CredentialSecretMaterial::ApiKey { value } => {
                validate_non_empty_secret("API key", value)
            }
            CredentialSecretMaterial::StaticBearer { access_token } => {
                validate_non_empty_secret("static bearer token", access_token)
            }
            CredentialSecretMaterial::OpenAiCodexOAuth { tokens, .. }
            | CredentialSecretMaterial::ClaudeCodeOAuth { tokens } => tokens.validate(),
            CredentialSecretMaterial::GoogleOAuth {
                tokens,
                client_id,
                client_secret,
                ..
            } => {
                tokens.validate()?;
                validate_non_empty_secret("Google OAuth client id", client_id)?;
                if client_secret.as_deref().is_some_and(str::is_empty) {
                    return Err(String::from(
                        "Google OAuth client secret must be absent or non-empty.",
                    ));
                }
                Ok(())
            }
            CredentialSecretMaterial::MiniMaxOAuth {
                tokens,
                region,
                resource_url,
            } => {
                tokens.validate()?;
                validate_minimax_resource_url(*region, resource_url)
            }
            CredentialSecretMaterial::KimiCodingPlanOAuth { tokens, device_id } => {
                tokens.validate()?;
                validate_kimi_device_id(device_id)
            }
            CredentialSecretMaterial::BrowserCallbackAuthorization {
                secret,
                provider_subject,
                provider_username,
                credential_label,
                ..
            } => {
                validate_non_empty_secret("browser callback credential", secret)?;
                validate_bounded_identity("provider subject", provider_subject)?;
                validate_bounded_identity("provider user name", provider_username)?;
                validate_bounded_identity("credential label", credential_label)
            }
        }
    }
}

/// Validates one non-secret identity returned by a browser authorization callback.
/// 校验浏览器授权回调返回的一个非秘密身份值。
///
/// # Parameters
/// 参数
/// `label` identifies the field in diagnostics and `value` is the exact provider value.
/// `label` 在诊断中标识字段，`value` 是精确供应商值。
///
/// # Returns
/// 返回值
/// Success only for a non-empty value no longer than 2048 UTF-8 bytes.
/// 仅非空且不超过 2048 个 UTF-8 字节的值返回成功。
fn validate_bounded_identity(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 2048 {
        return Err(format!(
            "Browser callback {label} must contain between 1 and 2048 UTF-8 bytes."
        ));
    }
    Ok(())
}

/// Validates the persisted RFC 4122-shaped Kimi device identifier.
/// 校验持久化的 RFC 4122 形态 Kimi 设备标识。
///
/// # Parameters
/// 参数
/// `device_id` is the exact identity sent during Kimi authorization, refresh, and usage.
/// `device_id` 是 Kimi 授权、刷新与用量请求期间发送的精确身份。
///
/// # Returns
/// 返回值
/// Success only for a lowercase hexadecimal UUID shape.
/// 仅对小写十六进制 UUID 形态返回成功。
fn validate_kimi_device_id(device_id: &str) -> Result<(), String> {
    let bytes = device_id.as_bytes();
    if bytes.len() != 36 {
        return Err(String::from("Kimi OAuth device identifier is invalid."));
    }
    for (index, byte) in bytes.iter().copied().enumerate() {
        let separator = matches!(index, 8 | 13 | 18 | 23);
        if (separator && byte != b'-')
            || (!separator && !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
        {
            return Err(String::from("Kimi OAuth device identifier is invalid."));
        }
    }
    Ok(())
}

impl OAuthTokenSet {
    /// Validates one renewable OAuth token set.
    /// 校验一组可续期 OAuth Token。
    ///
    /// # Returns
    /// 返回值
    /// Success only when access, refresh, and expiration data are complete.
    /// 仅 Access、Refresh 与过期数据完整时返回成功。
    pub fn validate(&self) -> Result<(), String> {
        validate_non_empty_secret("OAuth access token", self.access_token.as_str())?;
        validate_non_empty_secret("OAuth refresh token", self.refresh_token.as_str())?;
        if self.expires_at_unix_ms <= self.issued_at_unix_ms {
            return Err(String::from(
                "OAuth token expiration must be later than its issue time.",
            ));
        }
        if self.scopes.iter().any(|scope| scope.trim().is_empty()) {
            return Err(String::from("OAuth scopes must not contain empty values."));
        }
        Ok(())
    }
}

impl CredentialExecutionSecret {
    /// Returns the exact OpenCode workspace required by inference and account queries.
    /// 返回推理与账号查询所需的精确 OpenCode 工作区。
    ///
    /// No parameters; returns no workspace for credentials from other providers.
    /// 无参数；其他供应商的凭据返回空工作区。
    pub(crate) fn opencode_organization_id(&self) -> Option<&str> {
        self.opencode_organization_id.as_deref()
    }

    /// Resolves the dedicated control key for the registered OpenDesign wallet primitive.
    /// 为已注册的 OpenDesign 钱包原语解析专用控制密钥。
    ///
    /// Returns the exact control key, or an explicit error for API keys and other credentials.
    /// 返回精确控制密钥，或针对 API Key 及其他凭据的明确错误。
    pub(crate) fn open_design_control_key(&self) -> Result<&str, String> {
        self.open_design_control_key.as_deref().ok_or_else(|| {
            String::from("OpenDesign wallet balance requires account authorization.")
        })
    }
    /// Returns the fixed OpenVulcan deployment when this is a native Auth credential.
    /// 当本凭据为原生 Auth 时返回固定的 OpenVulcan 部署。
    pub fn openvulcan_region(&self) -> Option<OpenVulcanRegion> {
        self.openvulcan_region
    }
    /// Returns the frozen secret-store revision.
    /// 返回冻结的秘密存储修订号。
    ///
    /// # Returns
    /// 返回值
    /// The monotonic revision loaded before dispatch.
    /// 返回分派前加载的单调修订号。
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the authentication projection category.
    /// 返回鉴权投影类别。
    ///
    /// # Returns
    /// 返回值
    /// The exact API-key or bearer category.
    /// 返回精确 API Key 或 Bearer 类别。
    pub fn kind(&self) -> CredentialExecutionSecretKind {
        self.kind
    }

    /// Exposes the secret only to the provider transport boundary.
    /// 仅向供应商传输边界公开秘密。
    ///
    /// # Returns
    /// 返回值
    /// The exact in-memory value.
    /// 返回精确内存值。
    pub fn expose_value(&self) -> &str {
        self.value.as_str()
    }

    /// Returns the optional exact upstream account identifier.
    /// 返回可选的精确上游账号标识。
    ///
    /// # Returns
    /// 返回值
    /// A borrowed account identifier only when verified credential material supplied one.
    /// 仅当已验证凭据材料提供时返回借用的账号标识。
    pub fn account_id(&self) -> Option<&str> {
        self.account_id.as_deref()
    }

    /// Returns the optional exact Google quota project identifier.
    /// 返回可选的精确 Google 配额项目标识。
    ///
    /// # Returns
    /// 返回值
    /// A borrowed quota project identifier only when credential validation supplied one.
    /// 仅当凭据验证提供时返回借用的配额项目标识。
    pub fn quota_project_id(&self) -> Option<&str> {
        self.quota_project_id.as_deref()
    }

    /// Returns the optional exact Google Code Assist project identifier.
    /// 返回可选的精确 Google Code Assist 项目标识。
    ///
    /// # Returns
    /// 返回值
    /// A borrowed project identifier only when authorization validation supplied one.
    /// 仅当授权验证提供时返回借用的项目标识。
    pub fn project_id(&self) -> Option<&str> {
        self.project_id.as_deref()
    }

    /// Returns the provider-visible device identity required by exact metadata routes.
    /// 返回精确元数据路由所需的供应商可见设备身份。
    ///
    /// # Returns
    /// 返回值
    /// The frozen device identity when the credential contract declares one.
    /// 凭据契约声明设备身份时返回冻结值。
    pub fn provider_device_id(&self) -> Option<&str> {
        self.provider_device_id.as_deref()
    }
}

/// Rejects empty secret fields without returning their contents.
/// 拒绝空秘密字段且不返回其内容。
fn validate_non_empty_secret(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("AI credential {label} must not be empty."));
    }
    Ok(())
}

/// Validates the MiniMax resource URL against the selected regional authority.
/// 根据所选区域授权端校验 MiniMax 资源 URL。
///
/// # Parameters
/// 参数
/// `region` is the fixed authorization region and `resource_url` is the provider-returned URL.
/// `region` 是固定授权区域，`resource_url` 是供应商返回的 URL。
///
/// # Returns
/// 返回值
/// Success only for the exact reviewed HTTPS API origin.
/// 仅精确匹配已审核 HTTPS API Origin 时返回成功。
fn validate_minimax_resource_url(region: MiniMaxRegion, resource_url: &str) -> Result<(), String> {
    let expected = match region {
        MiniMaxRegion::Global => "https://api.minimax.io",
        MiniMaxRegion::China => "https://api.minimaxi.com",
    };
    if resource_url.trim_end_matches('/') != expected {
        return Err(String::from(
            "MiniMax OAuth returned an untrusted regional resource URL.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies API keys round-trip only through the versioned envelope.
    /// 验证 API Key 只能通过版本化封装往返。
    #[test]
    fn api_key_round_trips_through_versioned_envelope() {
        let envelope =
            CredentialSecretEnvelope::new_api_key(String::from("secret")).expect("valid key");
        let encoded = envelope.to_json().expect("secret should serialize");
        let decoded =
            CredentialSecretEnvelope::from_json(encoded.as_str()).expect("secret should parse");
        assert_eq!(decoded.credential_mode(), CredentialMode::Token);
        assert_eq!(decoded.execution_secret().expect("secret").revision(), 1);
    }

    /// Verifies historical bare strings are rejected instead of migrated.
    /// 验证历史裸字符串会被拒绝而不会迁移。
    #[test]
    fn historical_bare_string_is_rejected() {
        let error = CredentialSecretEnvelope::from_json("\"legacy-token\"")
            .err()
            .expect("bare strings must be rejected");
        assert!(error.contains("Unsupported AI credential secret format"));
    }

    /// Verifies replacement increments the revision exactly once.
    /// 验证替换操作只将修订号精确递增一次。
    #[test]
    fn api_key_replacement_increments_revision() {
        let initial =
            CredentialSecretEnvelope::new_api_key(String::from("first")).expect("valid key");
        let replaced = initial
            .replace_api_key(String::from("second"))
            .expect("replacement should validate");
        assert_eq!(replaced.revision, 2);
    }

    /// Verifies only renewable material inside the skew window requests refresh.
    /// 验证仅进入提前刷新窗口的可续期材料要求刷新。
    #[test]
    fn oauth_refresh_window_is_explicit() {
        let tokens = OAuthTokenSet {
            access_token: String::from("access"),
            refresh_token: String::from("refresh"),
            issued_at_unix_ms: 1_000,
            expires_at_unix_ms: 10_000,
            scopes: BTreeSet::new(),
            account_subject: None,
            email: None,
        };
        let oauth =
            CredentialSecretEnvelope::new(CredentialSecretMaterial::ClaudeCodeOAuth { tokens })
                .expect("OAuth material should validate");
        assert!(!oauth.requires_refresh(5_000, 1_000));
        assert!(oauth.requires_refresh(9_000, 1_000));
        let api_key = CredentialSecretEnvelope::new_api_key(String::from("key"))
            .expect("API key should validate");
        assert!(!api_key.requires_refresh(u64::MAX, u64::MAX));
    }

    /// Verifies generic browser credentials do not impersonate protocol-specific account fields.
    /// 验证通用浏览器凭据不会冒充协议专用账号字段。
    #[test]
    fn browser_callback_identity_is_not_projected_as_codex_account_id() {
        let envelope =
            CredentialSecretEnvelope::new(CredentialSecretMaterial::BrowserCallbackAuthorization {
                secret: String::from("user_secret"),
                projection: BrowserCallbackSecretProjection::Bearer,
                provider_subject: String::from("provider-subject"),
                provider_username: String::from("provider-user"),
                credential_label: String::from("CLI Login"),
            })
            .expect("browser callback material should validate");

        let secret = envelope
            .execution_secret()
            .expect("browser callback execution secret");
        assert_eq!(secret.kind(), CredentialExecutionSecretKind::Bearer);
        assert!(secret.account_id().is_none());
    }
}
