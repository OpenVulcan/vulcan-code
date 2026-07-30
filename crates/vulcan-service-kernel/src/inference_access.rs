//! Ephemeral least-privilege access grants for the loopback inference service.
//! 回环推理服务使用的临时最小权限授权。

use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
use vulcan_ai_contract::{
    InferenceCallerBudget, InferenceCallerContext, InferencePermission, InferenceUsageScope,
};

/// Describes one host-owned inference access grant before issuance.
/// 描述一项签发前由宿主拥有的推理访问授权。
#[derive(Debug, Clone)]
pub struct InferenceAccessGrantSpec {
    /// Stable caller identity.
    /// 稳定调用方身份。
    pub caller_id: String,
    /// Consumer profiles accessible through the grant.
    /// 当前授权可访问的消费者配置档。
    pub consumer_profile_ids: BTreeSet<String>,
    /// Fine-grained operation permissions.
    /// 细粒度操作权限。
    pub permissions: BTreeSet<InferencePermission>,
    /// Consumer-owned LLM purpose identifiers accessible through the grant.
    /// 当前授权可访问的消费者自有 LLM 用途标识。
    pub llm_purpose_ids: BTreeSet<String>,
    /// Usage scopes the host permits this caller to consume.
    /// 宿主允许当前调用方使用的用途范围。
    pub usage_scopes: BTreeSet<InferenceUsageScope>,
    /// Host-owned execution budgets.
    /// 宿主拥有的执行预算。
    pub budget: InferenceCallerBudget,
    /// Absolute expiration timestamp in Unix milliseconds.
    /// Unix 毫秒绝对过期时间戳。
    pub expires_at_unix_ms: u64,
}

/// Returns an opaque token once together with its non-secret grant identity.
/// 一次性返回不透明令牌及其非秘密授权身份。
#[derive(Clone, PartialEq, Eq)]
pub struct IssuedInferenceAccessGrant {
    /// Stable grant identifier.
    /// 稳定授权标识。
    pub grant_id: String,
    /// Opaque bearer token returned only at issuance.
    /// 仅在签发时返回的不透明 Bearer Token。
    pub access_token: String,
    /// Absolute expiration timestamp in Unix milliseconds.
    /// Unix 毫秒绝对过期时间戳。
    pub expires_at_unix_ms: u64,
}

impl std::fmt::Debug for IssuedInferenceAccessGrant {
    /// Formats non-secret grant identity while redacting the one-time bearer token.
    /// 格式化非秘密授权身份，同时脱敏一次性 Bearer Token。
    ///
    /// # Parameters
    /// 参数
    /// `formatter` receives the grant identity, expiration, and one redaction marker.
    /// `formatter` 接收授权身份、过期时间与一个脱敏标记。
    ///
    /// # Returns
    /// 返回值
    /// Standard debug formatting completion.
    /// 标准调试格式化完成结果。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IssuedInferenceAccessGrant")
            .field("grant_id", &self.grant_id)
            .field("access_token", &"<redacted>")
            .field("expires_at_unix_ms", &self.expires_at_unix_ms)
            .finish()
    }
}

/// Retains one non-secret grant behind a token digest.
/// 在令牌摘要后保留一项非秘密授权。
#[derive(Debug, Clone)]
struct StoredInferenceAccessGrant {
    /// Stable grant identifier.
    /// 稳定授权标识。
    grant_id: String,
    /// Immutable host-owned authorization specification.
    /// 不可变宿主授权规格。
    spec: InferenceAccessGrantSpec,
}

/// Owns process-local inference grants without retaining raw access tokens.
/// 管理不保留原始访问令牌的进程内推理授权。
#[derive(Default)]
pub struct InferenceAccessRegistry {
    /// Grants keyed by SHA-256 token digest.
    /// 按 SHA-256 令牌摘要索引的授权。
    grants: Mutex<BTreeMap<[u8; 32], StoredInferenceAccessGrant>>,
}

impl InferenceAccessRegistry {
    /// Creates an empty process-local grant registry.
    /// 创建一个空的进程内授权注册表。
    ///
    /// # Returns
    /// 返回值
    /// A registry that owns no caller authority.
    /// 返回不拥有任何调用方权限的注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// Issues one opaque grant after validating every closed authorization field.
    /// 校验全部封闭授权字段后签发一项不透明授权。
    ///
    /// # Parameters
    /// 参数
    /// `spec` is trusted host configuration and never comes from an inference request.
    /// `spec` 是可信宿主配置，绝不来自推理请求。
    ///
    /// # Returns
    /// 返回值
    /// The one-time raw token and non-secret grant identity.
    /// 返回一次性原始令牌及非秘密授权身份。
    pub fn issue(
        &self,
        spec: InferenceAccessGrantSpec,
    ) -> Result<IssuedInferenceAccessGrant, String> {
        validate_grant_spec(&spec)?;
        let now = current_unix_millis()?;
        if spec.expires_at_unix_ms <= now {
            return Err(String::from(
                "Inference access grant expiration must be in the future.",
            ));
        }
        let grant_id = Uuid::new_v4().to_string();
        let access_token = format!("vci_{}_{}", Uuid::new_v4(), Uuid::new_v4());
        let digest = token_digest(access_token.as_str());
        let expires_at_unix_ms = spec.expires_at_unix_ms;
        self.grants
            .lock()
            .map_err(|_| String::from("Inference access registry lock is poisoned."))?
            .insert(
                digest,
                StoredInferenceAccessGrant {
                    grant_id: grant_id.clone(),
                    spec,
                },
            );
        Ok(IssuedInferenceAccessGrant {
            grant_id,
            access_token,
            expires_at_unix_ms,
        })
    }

    /// Revokes one grant immediately by its stable identifier.
    /// 按稳定标识立即撤销一项授权。
    ///
    /// # Parameters
    /// 参数
    /// `grant_id` is the non-secret identifier returned at issuance.
    /// `grant_id` 是签发时返回的非秘密标识。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when one current grant was removed.
    /// 仅在删除一项当前授权时返回 `true`。
    pub fn revoke(&self, grant_id: &str) -> Result<bool, String> {
        let mut grants = self
            .grants
            .lock()
            .map_err(|_| String::from("Inference access registry lock is poisoned."))?;
        let digest = grants
            .iter()
            .find_map(|(digest, grant)| (grant.grant_id == grant_id).then_some(*digest));
        Ok(digest.is_some_and(|digest| grants.remove(&digest).is_some()))
    }

    /// Replaces one grant with a new opaque token and specification.
    /// 使用新的不透明令牌与规格替换一项授权。
    ///
    /// # Parameters
    /// 参数
    /// `grant_id` identifies the grant to revoke and `replacement` is host-owned.
    /// `grant_id` 标识待撤销授权，`replacement` 由宿主拥有。
    ///
    /// # Returns
    /// 返回值
    /// A newly issued grant after the old one has been removed.
    /// 返回旧授权删除后新签发的授权。
    pub fn rotate(
        &self,
        grant_id: &str,
        replacement: InferenceAccessGrantSpec,
    ) -> Result<IssuedInferenceAccessGrant, String> {
        validate_grant_spec(&replacement)?;
        let now = current_unix_millis()?;
        if replacement.expires_at_unix_ms <= now {
            return Err(String::from(
                "Inference access grant expiration must be in the future.",
            ));
        }
        let replacement_grant_id = Uuid::new_v4().to_string();
        let access_token = format!("vci_{}_{}", Uuid::new_v4(), Uuid::new_v4());
        let replacement_digest = token_digest(access_token.as_str());
        let expires_at_unix_ms = replacement.expires_at_unix_ms;
        let mut grants = self
            .grants
            .lock()
            .map_err(|_| String::from("Inference access registry lock is poisoned."))?;
        let existing_digest = grants
            .iter()
            .find_map(|(digest, grant)| (grant.grant_id == grant_id).then_some(*digest))
            .ok_or_else(|| format!("Inference access grant `{grant_id}` was not found."))?;
        if grants.contains_key(&replacement_digest) {
            return Err(format!(
                "Inference access token collision prevented rotation of grant `{grant_id}`."
            ));
        }
        grants.remove(&existing_digest);
        grants.insert(
            replacement_digest,
            StoredInferenceAccessGrant {
                grant_id: replacement_grant_id.clone(),
                spec: replacement,
            },
        );
        Ok(IssuedInferenceAccessGrant {
            grant_id: replacement_grant_id,
            access_token,
            expires_at_unix_ms,
        })
    }

    /// Authorizes one exact operation and builds a trusted caller context.
    /// 授权一项精确操作并构建可信调用方上下文。
    ///
    /// # Parameters
    /// 参数
    /// The raw token is transport-provided; profile, permission, purpose, and usage scope
    /// are exact operation facts resolved by the host.
    /// 原始令牌由传输提供；配置档、权限、用途与用途范围是宿主解析的精确操作事实。
    ///
    /// # Returns
    /// 返回值
    /// A host-injected caller context or a stable authorization failure.
    /// 返回宿主注入的调用方上下文或稳定授权失败。
    pub fn authorize(
        &self,
        access_token: &str,
        consumer_profile_id: &str,
        permission: InferencePermission,
        purpose_id: Option<&str>,
        usage_scope: InferenceUsageScope,
    ) -> Result<InferenceCallerContext, String> {
        let digest = token_digest(access_token);
        let now = current_unix_millis()?;
        let mut grants = self
            .grants
            .lock()
            .map_err(|_| String::from("Inference access registry lock is poisoned."))?;
        let grant = grants
            .get(&digest)
            .cloned()
            .ok_or_else(|| String::from("Inference access token is invalid or revoked."))?;
        if grant.spec.expires_at_unix_ms <= now {
            grants.remove(&digest);
            return Err(String::from("Inference access token has expired."));
        }
        drop(grants);
        if !grant
            .spec
            .consumer_profile_ids
            .contains(consumer_profile_id)
        {
            return Err(format!(
                "Inference access grant does not authorize consumer profile `{consumer_profile_id}`."
            ));
        }
        if !grant.spec.permissions.contains(&permission) {
            return Err(String::from(
                "Inference access grant does not authorize the requested operation.",
            ));
        }
        if let Some(purpose_id) = purpose_id
            && !grant.spec.llm_purpose_ids.contains(purpose_id)
        {
            return Err(format!(
                "Inference access grant does not authorize LLM purpose `{purpose_id}`."
            ));
        }
        if !grant.spec.usage_scopes.contains(&usage_scope) {
            return Err(format!(
                "Inference access grant does not authorize usage scope `{:?}`.",
                usage_scope
            ));
        }
        let deadline_unix_ms = grant
            .spec
            .budget
            .max_execution_duration_ms
            .map(|duration| now.saturating_add(duration))
            .map(|deadline| deadline.min(grant.spec.expires_at_unix_ms));
        Ok(InferenceCallerContext {
            caller_id: grant.spec.caller_id,
            allowed_consumer_profile_ids: grant.spec.consumer_profile_ids,
            permissions: grant.spec.permissions,
            usage_scope,
            interactive_session_id: (usage_scope == InferenceUsageScope::InteractiveAgent)
                .then(|| format!("inference-grant:{}", grant.grant_id)),
            deadline_unix_ms,
            budget: grant.spec.budget,
        })
    }

    /// Returns the stable caller identifier bound to one valid token.
    /// 返回一个有效令牌绑定的稳定调用方标识。
    ///
    /// # Parameters
    /// 参数
    /// `access_token` is the opaque transport token.
    /// `access_token` 是不透明传输令牌。
    ///
    /// # Returns
    /// 返回值
    /// The caller identifier after expiry and revocation checks.
    /// 通过过期与撤销检查后返回调用方标识。
    pub fn caller_id(&self, access_token: &str) -> Result<String, String> {
        let digest = token_digest(access_token);
        let now = current_unix_millis()?;
        let mut grants = self
            .grants
            .lock()
            .map_err(|_| String::from("Inference access registry lock is poisoned."))?;
        let grant = grants
            .get(&digest)
            .cloned()
            .ok_or_else(|| String::from("Inference access token is invalid or revoked."))?;
        if grant.spec.expires_at_unix_ms <= now {
            grants.remove(&digest);
            return Err(String::from("Inference access token has expired."));
        }
        Ok(grant.spec.caller_id)
    }
}

/// Validates one grant specification before authority enters the registry.
/// 在权限进入注册表前校验一项授权规格。
///
/// # Parameters
/// 参数
/// `spec` contains only host-selected identity, scopes, and budgets.
/// `spec` 仅包含宿主选择的身份、范围与预算。
///
/// # Returns
/// 返回值
/// Success for a complete least-privilege specification.
/// 完整最小权限规格有效时返回成功。
fn validate_grant_spec(spec: &InferenceAccessGrantSpec) -> Result<(), String> {
    if spec.caller_id.trim().is_empty() {
        return Err(String::from(
            "Inference access grant caller identity must not be empty.",
        ));
    }
    if spec.consumer_profile_ids.is_empty() || spec.permissions.is_empty() {
        return Err(String::from(
            "Inference access grant requires consumer profiles and permissions.",
        ));
    }
    if spec.usage_scopes.is_empty() {
        return Err(String::from(
            "Inference access grant requires at least one usage scope.",
        ));
    }
    if spec.permissions.contains(&InferencePermission::Llm) && spec.llm_purpose_ids.is_empty() {
        return Err(String::from(
            "Inference access grant with LLM permission requires at least one LLM purpose.",
        ));
    }
    if !spec.permissions.contains(&InferencePermission::Llm) && !spec.llm_purpose_ids.is_empty() {
        return Err(String::from(
            "Inference access grant cannot declare LLM purposes without LLM permission.",
        ));
    }
    if spec.budget.max_concurrent_executions == 0 {
        return Err(String::from(
            "Inference access grant concurrency budget must be greater than zero.",
        ));
    }
    if spec.budget.max_input_units_per_request == Some(0) {
        return Err(String::from(
            "Inference access grant input budget must be greater than zero when configured.",
        ));
    }
    if spec.budget.max_output_tokens_per_request == Some(0) {
        return Err(String::from(
            "Inference access grant output-token budget must be greater than zero when configured.",
        ));
    }
    if spec.budget.max_requests_per_minute == Some(0) {
        return Err(String::from(
            "Inference access grant request-rate budget must be greater than zero when configured.",
        ));
    }
    if spec.budget.max_input_units_per_minute == Some(0) {
        return Err(String::from(
            "Inference access grant input-rate budget must be greater than zero when configured.",
        ));
    }
    if spec.budget.max_execution_duration_ms == Some(0) {
        return Err(String::from(
            "Inference access grant execution duration must be greater than zero.",
        ));
    }
    Ok(())
}

/// Computes the fixed-size token lookup key without retaining the raw token.
/// 计算固定长度令牌查找键且不保留原始令牌。
///
/// # Parameters
/// 参数
/// `access_token` is the caller-presented opaque value.
/// `access_token` 是调用方提交的不透明值。
///
/// # Returns
/// 返回值
/// A SHA-256 digest used only for process-local lookup.
/// 返回仅用于进程内查找的 SHA-256 摘要。
fn token_digest(access_token: &str) -> [u8; 32] {
    Sha256::digest(access_token.as_bytes()).into()
}

/// Returns the current Unix time in milliseconds.
/// 返回当前 Unix 毫秒时间。
///
/// # Returns
/// 返回值
/// A supported timestamp or a controlled clock-range failure.
/// 返回受支持时间戳或受控时钟范围失败。
fn current_unix_millis() -> Result<u64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| String::from("System clock is earlier than the Unix epoch."))?
        .as_millis();
    u64::try_from(millis)
        .map_err(|_| String::from("System time exceeds the supported inference grant range."))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one complete short-lived test grant.
    /// 构建一项完整短期测试授权。
    fn test_spec() -> InferenceAccessGrantSpec {
        InferenceAccessGrantSpec {
            caller_id: String::from("vmm-local"),
            consumer_profile_ids: BTreeSet::from([String::from("vmm")]),
            permissions: BTreeSet::from([InferencePermission::Llm, InferencePermission::Status]),
            llm_purpose_ids: BTreeSet::from([String::from("precheck_l1")]),
            usage_scopes: BTreeSet::from([InferenceUsageScope::InteractiveAgent]),
            budget: InferenceCallerBudget {
                max_concurrent_executions: 2,
                max_input_units_per_request: Some(1024),
                max_output_tokens_per_request: Some(512),
                max_requests_per_minute: Some(120),
                max_input_units_per_minute: Some(120_000),
                max_execution_duration_ms: Some(30_000),
            },
            expires_at_unix_ms: current_unix_millis().expect("clock") + 60_000,
        }
    }

    /// Verifies one-time issued grant debug formatting never exposes its bearer token.
    /// 验证一次性签发授权的调试格式绝不暴露其 Bearer Token。
    #[test]
    fn issued_grant_debug_redacts_access_token() {
        let registry = InferenceAccessRegistry::new();
        let issued = registry.issue(test_spec()).expect("issue grant");
        let debug = format!("{issued:?}");

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains(issued.access_token.as_str()));
    }

    /// Verifies unusable LLM grants and zero-valued optional budgets fail before issuance.
    /// 验证不可用的 LLM 授权与零值可选预算会在签发前失败。
    #[test]
    fn grant_issuance_rejects_incomplete_llm_scope_and_zero_budgets() {
        let registry = InferenceAccessRegistry::new();
        let mut missing_purpose = test_spec();
        missing_purpose.llm_purpose_ids.clear();
        assert!(
            registry.issue(missing_purpose).is_err(),
            "LLM grants must declare at least one purpose"
        );

        let mut orphan_purpose = test_spec();
        orphan_purpose.permissions.remove(&InferencePermission::Llm);
        assert!(
            registry.issue(orphan_purpose).is_err(),
            "non-LLM grants cannot retain LLM purposes"
        );

        let mut zero_request_rate = test_spec();
        zero_request_rate.budget.max_requests_per_minute = Some(0);
        assert!(
            registry.issue(zero_request_rate).is_err(),
            "zero-valued optional budgets must be rejected at issuance"
        );
    }

    /// Verifies grants are profile, operation, purpose, and scope constrained.
    /// 验证授权受配置档、操作、用途与用途范围约束。
    #[test]
    fn grant_authorization_is_fully_scoped() {
        let registry = InferenceAccessRegistry::new();
        let issued = registry.issue(test_spec()).expect("issue grant");
        let caller = registry
            .authorize(
                issued.access_token.as_str(),
                "vmm",
                InferencePermission::Llm,
                Some("precheck_l1"),
                InferenceUsageScope::InteractiveAgent,
            )
            .expect("authorized caller");
        assert_eq!(caller.caller_id, "vmm-local");
        assert!(caller.interactive_session_id.is_some());
        assert!(caller.deadline_unix_ms.is_some());
        assert!(
            caller
                .deadline_unix_ms
                .is_some_and(|deadline| deadline <= issued.expires_at_unix_ms)
        );
        assert!(
            registry
                .authorize(
                    issued.access_token.as_str(),
                    "vmm",
                    InferencePermission::Embedding,
                    None,
                    InferenceUsageScope::InteractiveAgent,
                )
                .is_err()
        );
        assert!(
            registry
                .authorize(
                    issued.access_token.as_str(),
                    "vmm",
                    InferencePermission::Cancel,
                    None,
                    InferenceUsageScope::InteractiveAgent,
                )
                .is_err(),
            "status permission must not imply cancellation authority"
        );
    }

    /// Verifies rotation invalidates the original opaque token immediately.
    /// 验证轮换立即使原始不透明令牌失效。
    #[test]
    fn grant_rotation_revokes_original_token() {
        let registry = InferenceAccessRegistry::new();
        let issued = registry.issue(test_spec()).expect("issue grant");
        let replacement = registry
            .rotate(issued.grant_id.as_str(), test_spec())
            .expect("rotate grant");
        assert!(registry.caller_id(issued.access_token.as_str()).is_err());
        assert_eq!(
            registry
                .caller_id(replacement.access_token.as_str())
                .expect("replacement caller"),
            "vmm-local"
        );
    }

    /// Verifies a rejected replacement leaves the original grant usable.
    /// 验证被拒绝的替换授权不会破坏原授权。
    #[test]
    fn failed_grant_rotation_preserves_original_token() {
        let registry = InferenceAccessRegistry::new();
        let issued = registry.issue(test_spec()).expect("issue grant");
        let mut invalid_replacement = test_spec();
        invalid_replacement.expires_at_unix_ms = current_unix_millis()
            .expect("clock should read")
            .saturating_sub(1);

        let error = registry
            .rotate(issued.grant_id.as_str(), invalid_replacement)
            .expect_err("expired replacement must fail");

        assert!(error.contains("expiration must be in the future"));
        assert_eq!(
            registry
                .caller_id(issued.access_token.as_str())
                .expect("original caller should remain authorized"),
            "vmm-local"
        );
    }
}
