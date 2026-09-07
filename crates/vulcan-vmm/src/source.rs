//! Exact host-owned identities for memory caches and asynchronous results.
//! 记忆缓存及异步结果使用的准确宿主来源身份。

use crate::{VmmClientError, VmmTransportConfig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Identifies the actual memory owner without persisting remote credentials or display names.
/// 标识实际记忆归属，不持久化远端凭据或显示名称。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case", deny_unknown_fields)]
pub enum VmmMemorySource {
    /// One host-owned native memory project; the single user is implicit in native mode.
    /// 一个宿主拥有的原生记忆项目；原生模式隐含唯一用户。
    Native {
        /// Actual bound memory project, never the fixed native request envelope ID.
        /// 实际绑定记忆项目，绝非原生请求封套的固定 ID。
        project_id: u64,
    },
    /// One remote service/credential namespace and exact user/project pair.
    /// 一个远端服务及凭据命名空间，以及准确用户/项目组合。
    ExternalGrpc {
        /// Opaque digest of the normalized endpoint and authentication identity.
        /// 规范端点及鉴权身份的不透明摘要。
        route_hash: String,
        /// Exact remote user ID.
        /// 准确远端用户 ID。
        user_id: u64,
        /// Exact remote project ID.
        /// 准确远端项目 ID。
        project_id: u64,
    },
}

impl VmmMemorySource {
    /// Rejects delivery to another owner; omission is reserved for direct non-queued callers.
    /// 拒绝向其他归属投递；省略仅供直接非排队调用方使用。
    /// Takes an optional frozen expectation and returns success only for an exact validated match.
    /// 接收可选冻结预期，仅对准确且有效的匹配返回成功。
    pub fn verify_expected(&self, expected: Option<&Self>) -> Result<(), VmmClientError> {
        self.validate()?;
        if let Some(expected) = expected {
            expected.validate()?;
            if self != expected {
                return Err(VmmClientError::new(
                    "memory_source_changed",
                    "The memory source changed before delivery; the old writeback is cancelled.",
                ));
            }
        }
        Ok(())
    }

    /// Builds the native owner from a resolved binding; rejects reserved project zero.
    /// 根据已解析绑定构建原生归属；拒绝保留项目零。
    /// Takes the actual project ID and returns a validated source or explicit identity error.
    /// 接收实际项目 ID，返回已校验来源或明确身份错误。
    pub fn native(project_id: u64) -> Result<Self, VmmClientError> {
        // Validate the constructed value through the same boundary used for persisted sources.
        // 通过与持久来源相同的边界校验构造值。
        let source = Self::Native { project_id };
        source.validate()?;
        Ok(source)
    }

    /// Builds a remote identity using the same endpoint normalization as the connected transport.
    /// 使用与已连接传输相同的端点规范化规则构建远端身份。
    /// Takes frozen transport options and exact remote IDs; returns no credential-bearing fields.
    /// 接收冻结传输选项及准确远端 ID；不返回含凭据字段。
    pub fn external(
        config: &VmmTransportConfig,
        user_id: u64,
        project_id: u64,
    ) -> Result<Self, VmmClientError> {
        // Length-delimited JSON avoids collisions between concatenated endpoint and credential strings.
        // 长度明确的 JSON 避免端点及凭据字符串直接拼接产生碰撞。
        let material = serde_json::to_vec(&(
            "vmm-memory-source-v1",
            crate::transport::normalize_endpoint(&config.grpc_target)?,
            &config.grpc_api_key,
        ))
        .map_err(|error| VmmClientError::new("invalid_memory_source", error.to_string()))?;
        // Persist only the digest; timeout, keepalive and display changes do not change ownership.
        // 只持久化摘要；超时、保活及显示变化不改变归属。
        let source = Self::ExternalGrpc {
            route_hash: hex::encode(Sha256::digest(material)),
            user_id,
            project_id,
        };
        source.validate()?;
        Ok(source)
    }

    /// Validates a constructed or deserialized source without probing a service or changing state.
    /// 校验构造或反序列化的来源，不探测服务或改变状态。
    /// Returns success only for positive owner IDs and a canonical opaque remote digest.
    /// 仅对正数归属 ID 及规范不透明远端摘要返回成功。
    pub fn validate(&self) -> Result<(), VmmClientError> {
        // Native identity has no configurable user or external route fields by construction.
        // 原生身份在结构上不存在可配置用户或外部路由字段。
        let valid = match self {
            Self::Native { project_id } => *project_id > 0,
            Self::ExternalGrpc {
                route_hash,
                user_id,
                project_id,
            } => {
                *user_id > 0
                    && *project_id > 0
                    && route_hash.len() == 64
                    && route_hash
                        .bytes()
                        .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value))
            }
        };
        if valid {
            Ok(())
        } else {
            Err(VmmClientError::new(
                "invalid_memory_source",
                "Memory source identity is invalid.",
            ))
        }
    }
}

/// Exact local read identity, including native linked scopes but no externally managed query policy.
/// 准确本地读取身份，包含原生连带范围，但不包含外部服务自行管理的查询策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmmMemoryReadSource {
    /// Bound owner and transport identity; never a display name or plaintext credential.
    /// 绑定归属和传输身份；绝非显示名称或明文凭据。
    pub owner: VmmMemorySource,
    /// Ordered native visibility with the owner first; always empty for external gRPC.
    /// 归属项目在首位的有序原生可见范围；外置 gRPC 始终为空。
    pub visible_project_ids: Vec<u64>,
}

impl VmmMemoryReadSource {
    /// Freezes a validated owner and its declared native visibility without guessing remote policy.
    /// 冻结已校验归属及其声明的原生可见范围，不猜测远端策略。
    /// Takes exact ordered IDs; returns an identity or an explicit malformed-scope error.
    /// 接收准确有序 ID；返回身份或明确的范围格式错误。
    pub fn new(
        owner: VmmMemorySource,
        visible_project_ids: Vec<u64>,
    ) -> Result<Self, VmmClientError> {
        // Keep ordering because native profile section labels use the declared scope order.
        // 保留顺序，因为原生画像分区标签使用声明的作用域顺序。
        let source = Self {
            owner,
            visible_project_ids,
        };
        source.validate()?;
        Ok(source)
    }

    /// Validates constructed and decoded identities using the same backend-specific scope rules.
    /// 使用相同的后端专属范围规则校验构造及解码的身份。
    /// Returns an error for zero/duplicate native scopes, missing owner, or guessed remote scopes.
    /// 原生范围为零、重复、缺少归属或猜测远端范围时返回错误。
    pub fn validate(&self) -> Result<(), VmmClientError> {
        self.owner.validate()?;
        // Native scope access is owner-first and unique; external visibility belongs to the remote service.
        // 原生范围归属在首且唯一；外置可见性属于远端服务。
        let valid = match &self.owner {
            VmmMemorySource::Native { project_id } => {
                self.visible_project_ids.first() == Some(project_id)
                    && self.visible_project_ids.iter().all(|id| *id > 0)
                    && self
                        .visible_project_ids
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        == self.visible_project_ids.len()
            }
            VmmMemorySource::ExternalGrpc { .. } => self.visible_project_ids.is_empty(),
        };
        if !valid {
            return Err(VmmClientError::new(
                "invalid_memory_read_source",
                "Memory read source has an invalid backend-specific visibility scope.",
            ));
        }
        Ok(())
    }

    /// Checks a frozen read expectation; direct management callers may omit it.
    /// 核对冻结读取预期；直接管理调用方可以省略。
    /// Returns success only for an exact owner, route and native visibility match.
    /// 仅在归属、路由及原生可见范围准确匹配时返回成功。
    pub fn verify_expected(&self, expected: Option<&Self>) -> Result<(), VmmClientError> {
        self.validate()?;
        if let Some(expected) = expected {
            expected.validate()?;
            if self != expected {
                return Err(VmmClientError::new(
                    "memory_read_source_changed",
                    "The memory read source changed before the request was evaluated.",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Freezes ordered native read visibility independently from write ownership and remote service policy.
    /// 独立于写入归属和远端服务策略，冻结有序原生读取可见性。
    #[test]
    fn read_identity_includes_linked_scopes_without_inventing_remote_scopes() {
        // Reuse a numeric owner to prove linked visibility still changes the read identity.
        // 复用数字归属，以证明连带可见性仍会改变读取身份。
        let owner = VmmMemorySource::native(7).expect("owner");
        let first = VmmMemoryReadSource::new(owner.clone(), vec![7, 8]).expect("first");
        let changed = VmmMemoryReadSource::new(owner.clone(), vec![7, 9]).expect("changed");
        assert_eq!(first.owner, changed.owner);
        assert_eq!(
            changed
                .verify_expected(Some(&first))
                .expect_err("changed scope")
                .kind,
            "memory_read_source_changed"
        );
        assert!(first.verify_expected(Some(&first)).is_ok());
        for invalid in [vec![], vec![8, 7], vec![7, 7], vec![7, 0]] {
            assert!(VmmMemoryReadSource::new(owner.clone(), invalid).is_err());
        }
        let remote = VmmMemorySource::ExternalGrpc {
            route_hash: "a".repeat(64),
            user_id: 1,
            project_id: 7,
        };
        assert!(VmmMemoryReadSource::new(remote.clone(), vec![7]).is_err());
        let remote_read = VmmMemoryReadSource::new(remote, vec![]).expect("opaque remote policy");
        assert_ne!(first, remote_read);
        let encoded = serde_json::to_string(&first).expect("encode");
        let decoded: VmmMemoryReadSource = serde_json::from_str(&encoded).expect("decode");
        assert_eq!(decoded, first);
        decoded.validate().expect("decoded identity");
    }

    /// Proves endpoint normalization is shared, credential changes isolate IDs, and secrets stay private.
    /// 证明端点规范化一致、凭据变化隔离 ID，且密钥保持私有。
    #[test]
    fn remote_identity_is_exact_and_credential_free() {
        // Synthetic credentials never originate from real configuration.
        // 合成凭据绝不来自真实配置。
        let mut config = VmmTransportConfig::from_effective(&crate::VmmEffectiveConfig {
            grpc_target: "127.0.0.1:50051".to_owned(),
            grpc_api_key: Some("synthetic-secret".to_owned()),
            ..Default::default()
        });
        // Frozen source survives non-identity tuning.
        // 冻结来源不受非身份参数调整影响。
        let source = VmmMemorySource::external(&config, 1, 7).expect("source");
        config.grpc_target = "http://127.0.0.1:50051".to_owned();
        config.precheck_timeout = std::time::Duration::from_secs(99);
        assert_eq!(
            source,
            VmmMemorySource::external(&config, 1, 7).expect("same route")
        );
        assert_ne!(source, VmmMemorySource::native(7).expect("native"));
        assert_ne!(
            source,
            VmmMemorySource::external(&config, 2, 7).expect("other user")
        );
        assert_ne!(
            source,
            VmmMemorySource::external(&config, 1, 8).expect("other project")
        );
        config.grpc_api_key = Some("another-synthetic-secret".to_owned());
        assert_ne!(
            source,
            VmmMemorySource::external(&config, 1, 7).expect("other credentials")
        );
        // Neither debug nor serialized persistence exposes the credential or endpoint.
        // 调试及序列化持久化均不暴露凭据或端点。
        let encoded = serde_json::to_string(&source).expect("encode source");
        assert!(!encoded.contains("synthetic-secret"));
        assert!(!format!("{source:?}").contains("127.0.0.1"));
        assert_eq!(
            serde_json::from_str::<VmmMemorySource>(&encoded).expect("decode"),
            source
        );
    }

    /// Rejects reserved IDs and malformed persisted identities rather than inventing fallback owners.
    /// 拒绝保留 ID 及格式错误的持久身份，不虚构回退归属。
    #[test]
    fn invalid_source_never_defaults_to_project_one() {
        assert!(VmmMemorySource::native(0).is_err());
        assert!(
            VmmMemorySource::ExternalGrpc {
                route_hash: "bad".to_owned(),
                user_id: 1,
                project_id: 1
            }
            .validate()
            .is_err()
        );
        assert!(
            serde_json::from_str::<VmmMemorySource>(
                r#"{"backend":"native","project_id":1,"user_id":1}"#
            )
            .is_err()
        );
    }

    /// Verifies optional direct admission and strict frozen delivery use distinct explicit semantics.
    /// 验证可选直接准入与严格冻结投递使用明确不同的语义。
    #[test]
    fn frozen_delivery_rejects_rebinding_and_malformed_expectations() {
        // The source already came from the actual binding, not a fallback envelope.
        // 来源已经来自实际绑定，而非回退封套。
        let source = VmmMemorySource::native(7).expect("source");
        assert!(source.verify_expected(None).is_ok());
        assert!(source.verify_expected(Some(&source)).is_ok());
        assert!(
            source
                .verify_expected(Some(&VmmMemorySource::native(8).expect("other")))
                .is_err()
        );
        assert!(
            source
                .verify_expected(Some(&VmmMemorySource::Native { project_id: 0 }))
                .is_err()
        );
    }
}
