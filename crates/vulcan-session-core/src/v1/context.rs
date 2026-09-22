//! V1 runtime lock and context-compaction documents.
//! V1 运行时锁定与上下文压缩文档。
//!
//! `runtime.json` pins the variable sources that must stay stable across
//! requests inside one context generation; `compactions.json` records the
//! history of context-compaction nodes only.
//! `runtime.json` 锁定必须在一个上下文代次内跨请求保持稳定的可变来源；
//! `compactions.json` 只记录上下文压缩节点历史。

use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::v1::errors::DomainError;
use crate::v1::ids::{ApiCallId, CompactionId, Seq};

mod compaction_content;
pub use compaction_content::{CompactionContent, RemoteResponsesCheckpoint};

/// File contract of the V1 runtime lock document.
/// V1 运行时锁定文档的文件契约。
pub const RUNTIME_CONTRACT: &str = "vulcan.session-runtime.v1";

/// File contract of the V1 compactions document.
/// V1 压缩节点文档的文件契约。
pub const COMPACTIONS_CONTRACT: &str = "vulcan.session-compactions.v1";

/// Explicit initialization state of the current runtime baseline.
/// 当前运行时基线的明确初始化状态。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeInitializationState {
    /// No physical Provider API has durably locked its baseline yet.
    /// 尚无物理 Provider API 可靠锁定其基线。
    #[default]
    FreshUninitialized,
    /// Historical facts are valid, but the rewritten generation has not locked its next baseline.
    /// 历史事实有效，但重写后的 generation 尚未锁定下一份基线。
    BaselineRefreshPending,
    /// The baseline was explicitly initialized; an empty segment list is valid.
    /// 基线已被明确初始化；空 segment 列表同样合法。
    Initialized,
}

/// Kind of one locked source injected into the stable system prefix.
/// 注入稳定系统前缀的某个锁定来源种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockedSourceKind {
    /// Captured memory-profile snapshot.
    /// 记忆画像快照。
    MemoryProfile,
    /// One `AGENTS.md` layer that actually participates in rule composition.
    /// 实际参与规则组合的一层 `AGENTS.md`。
    AgentsMd,
    /// One Vulcan-flavored additional `Vulcan.md` layer in the composition.
    /// 参与组合的一层 Vulcan 专属附加 `Vulcan.md`。
    VulcanMd,
    /// The active agent-persona prompt body.
    /// 当前生效的 Agent 人格提示词正文。
    AgentPersona,
    /// The active output-style prompt body.
    /// 当前生效的输出风格提示词正文。
    OutputStyle,
    /// Fixed injected text generated from prompt-skill on/off selections.
    /// 由提示词技能启停选择生成的固定注入正文。
    PromptSkillInjection,
    /// Long-lived plugin instruction block.
    /// 长期插件指令块。
    PluginInstructions,
    /// Baseline hook fixed body.
    /// 基线 Hook 固定正文。
    BaselineHooks,
}

/// Final Provider prompt layer targeted by one locked baseline segment.
/// 一条锁定基线片段最终写入的 Provider 提示词层。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockedPromptTarget {
    /// System instruction stream.
    /// 系统指令流。
    System,
    /// Developer instruction stream.
    /// 开发者指令流。
    Developer,
    /// User contextual stream.
    /// 用户上下文流。
    User,
    /// Tool-metadata instruction stream.
    /// 工具元数据指令流。
    ToolMetadata,
}

/// One locked source snapshot with its source identity.
/// 一份带来源标识的锁定来源快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedSource {
    /// Stable final prompt-node identity; it is independent from the source resource id.
    /// 稳定的最终提示词节点身份；它与来源资源 id 相互独立。
    pub node_id: String,
    /// Explicit final order inside the locked baseline.
    /// 锁定基线内的明确最终顺序。
    pub order: u32,
    /// Final Provider prompt layer used by this exact segment.
    /// 该精确片段使用的最终 Provider 提示词层。
    pub target: LockedPromptTarget,
    /// Stable kind of the locked source.
    /// 锁定来源的稳定种类。
    pub kind: LockedSourceKind,
    /// Optional human-readable source label (for example the file path).
    /// 可选人类可读来源标签(例如文件路径)。
    pub source_label: Option<String>,
    /// Optional immutable source identity when content is referenced by id.
    /// 内容按 id 引用时的可选不可变来源身份。
    pub source_id: Option<String>,
    /// The exact pinned body used by the current context generation.
    /// 当前上下文代次使用的精确锁定正文。
    pub content: String,
}

/// One explicitly classified baseline segment produced by the prompt owner.
/// 一条由提示词 owner 显式分类的基线片段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedBaselineSegmentInput {
    /// Stable audit classification supplied by the authoritative prompt source.
    /// 由权威提示词来源提供的稳定审计分类。
    pub kind: LockedSourceKind,
    /// Stable final prompt-node identity supplied without name inference.
    /// 不经名称推断直接提供的稳定最终提示词节点身份。
    pub node_id: String,
    /// Exact final Provider layer of this baseline segment.
    /// 该基线片段的精确最终 Provider 层。
    pub target: LockedPromptTarget,
    /// Optional human-readable source label used only for diagnostics.
    /// 仅用于诊断的可选人类可读来源标签。
    pub source_label: Option<String>,
    /// Optional immutable identity of the exact source snapshot.
    /// 精确来源快照的可选不可变身份。
    pub source_id: Option<String>,
    /// Exact final body used by the physical Provider request.
    /// 物理 Provider 请求实际使用的精确最终正文。
    pub content: String,
}

/// Canonical ordered baseline projection accepted by runtime persistence.
/// 运行时持久化接收的规范有序基线投影。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LockedBaselineProjection {
    /// Canonical locked sources in their exact final request order.
    /// 按最终请求精确顺序排列的规范锁定来源。
    locked_sources: Vec<LockedSource>,
}

impl LockedBaselineProjection {
    /// Builds the canonical initialized baseline containing no locked segments.
    /// 构建不包含任何锁定片段的规范已初始化基线。
    pub fn empty() -> Self {
        Self {
            locked_sources: Vec::new(),
        }
    }

    /// Projects explicit prompt provenance into canonical contiguous locked sources.
    /// 将显式提示词来源事实投影为规范连续的锁定来源。
    ///
    /// # Parameters / 参数
    /// `segments` must already separate baseline semantics from request-dynamic semantics.
    /// `segments` 必须已经把基线语义与单请求动态语义分离。
    ///
    /// # Returns / 返回值
    /// Returns one validated projection whose order is assigned only from input order.
    /// 返回一份已校验投影，其顺序只由输入顺序分配。
    pub fn project(segments: Vec<LockedBaselineSegmentInput>) -> Result<Self, DomainError> {
        let mut locked_sources = Vec::with_capacity(segments.len());
        for (index, segment) in segments.into_iter().enumerate() {
            let order = u32::try_from(index).map_err(|_| DomainError::InvalidLockedSource {
                field: "order_overflow",
            })?;
            locked_sources.push(LockedSource {
                node_id: segment.node_id,
                order,
                target: segment.target,
                kind: segment.kind,
                source_label: segment.source_label,
                source_id: segment.source_id,
                content: segment.content,
            });
        }
        Self::from_locked_sources(locked_sources)
    }

    /// Returns the canonical locked-source slice without rebuilding or inference.
    /// 返回规范锁定来源切片，不进行重建或推断。
    pub fn locked_sources(&self) -> &[LockedSource] {
        &self.locked_sources
    }

    /// Consumes the projection and returns its canonical locked sources.
    /// 消耗投影并返回其中的规范锁定来源。
    pub fn into_locked_sources(self) -> Vec<LockedSource> {
        self.locked_sources
    }

    /// Restores the already initialized baseline from one authoritative runtime lock.
    /// 从一个权威运行时锁恢复已经初始化的基线。
    ///
    /// # Parameters / 参数
    /// `runtime_lock` is the exact persisted generation lock captured by the session owner.
    /// `runtime_lock` 是会话 owner 捕获的精确持久化 generation 锁。
    ///
    /// # Returns / 返回值
    /// Returns `Some` for an initialized generation, including an intentionally empty baseline;
    /// fresh or refresh-pending generations return `None` so the next request may establish it.
    /// 已初始化 generation 返回 `Some`，包括有意为空的基线；全新或待刷新 generation 返回
    /// `None`，允许下一请求建立基线。
    pub fn from_initialized_runtime_lock(
        runtime_lock: &RuntimeLock,
    ) -> Result<Option<Self>, DomainError> {
        match runtime_lock.initialization_state {
            RuntimeInitializationState::Initialized => {
                Self::from_locked_sources(runtime_lock.locked_sources.clone()).map(Some)
            }
            RuntimeInitializationState::FreshUninitialized
            | RuntimeInitializationState::BaselineRefreshPending => Ok(None),
        }
    }

    /// Validates one already materialized projection used by strict decoding.
    /// 校验一份由严格解码恢复的已物化投影。
    pub(crate) fn from_locked_sources(
        locked_sources: Vec<LockedSource>,
    ) -> Result<Self, DomainError> {
        validate_locked_sources(&locked_sources)?;
        Ok(Self { locked_sources })
    }
}

impl<'de> Deserialize<'de> for LockedBaselineProjection {
    /// Strictly decodes and validates one canonical baseline projection.
    /// 严格解码并校验一份规范基线投影。
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ProjectionWire {
            /// Canonical locked sources carried by the wire representation.
            /// 线格式携带的规范锁定来源。
            locked_sources: Vec<LockedSource>,
        }

        let wire = ProjectionWire::deserialize(deserializer)?;
        Self::from_locked_sources(wire.locked_sources).map_err(de::Error::custom)
    }
}

/// Validates the canonical durable locked-source sequence.
/// 校验规范持久化锁定来源序列。
pub(crate) fn validate_locked_sources(sources: &[LockedSource]) -> Result<(), DomainError> {
    let mut node_ids = BTreeSet::new();
    for (index, source) in sources.iter().enumerate() {
        if source.node_id.trim().is_empty() || source.node_id.trim() != source.node_id {
            return Err(DomainError::InvalidLockedSource { field: "node_id" });
        }
        if !node_ids.insert(source.node_id.as_str()) {
            return Err(DomainError::InvalidLockedSource {
                field: "duplicate_node_id",
            });
        }
        if source.order as usize != index {
            return Err(DomainError::InvalidLockedSource { field: "order" });
        }
        if source
            .source_label
            .as_ref()
            .is_some_and(|value| value.trim().is_empty() || value.trim() != value)
        {
            return Err(DomainError::InvalidLockedSource {
                field: "source_label",
            });
        }
        if source
            .source_id
            .as_ref()
            .is_some_and(|value| value.trim().is_empty() || value.trim() != value)
        {
            return Err(DomainError::InvalidLockedSource { field: "source_id" });
        }
        if source.content.trim().is_empty() {
            return Err(DomainError::InvalidLockedSource { field: "content" });
        }
    }
    Ok(())
}

/// Current context-generation lock: pinned sources plus the compaction pointer.
/// 当前上下文代次锁定：锁定来源 + 压缩指针。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeLock {
    /// Explicit baseline initialization state.
    /// 明确的基线初始化状态。
    pub initialization_state: RuntimeInitializationState,
    /// Current effective compaction node id, or `None` before first compaction.
    /// 当前生效压缩节点 id；首次压缩前为 `None`。
    pub current_compaction_id: Option<CompactionId>,
    /// Locked variable sources of the current generation.
    /// 当前代次的锁定可变来源。
    pub locked_sources: Vec<LockedSource>,
}

/// One context-compaction node record.
/// 一条上下文压缩节点记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompactionNode {
    /// Local compaction node id.
    /// 本地压缩节点 id。
    pub compaction_id: CompactionId,
    /// API call id of the compaction request itself.
    /// 压缩请求自身的 API 调用 id。
    pub api_call_id: ApiCallId,
    /// Terminal `api_stats` seq supplying the compaction usage statistics.
    /// 提供压缩用量统计的终态 `api_stats` seq。
    pub api_stats_seq: Seq,
    /// Main-event seq at which the compaction occurred.
    /// 压缩实际发生位置的主事件 seq。
    pub occurred_at_seq: Seq,
    /// Inclusive retained boundary: this seq and later provider-visible events
    /// enter the new active window.
    /// 包含保留边界：该 seq 及之后的 Provider 可见事件进入新活动窗口。
    pub retained_from_seq: Seq,
    /// Exclusive summary or opaque checkpoint defining this context generation.
    /// 定义此上下文代次的互斥摘要或不透明检查点。
    pub content: CompactionContent,
    /// Creation time in Unix milliseconds.
    /// 创建时间(Unix 毫秒)。
    pub created_at_ms: i64,
}

/// Serialized `runtime.json` document.
/// 序列化的 `runtime.json` 文档。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeDocument {
    /// File contract marking the V1 runtime format.
    /// 标记 V1 运行时格式的文件契约。
    pub contract: String,
    /// Schema version of the document.
    /// 文档的 schema 版本。
    pub schema_version: u32,
    /// Owning session id.
    /// 所属会话 id。
    pub session_id: String,
    /// Last write time in Unix milliseconds.
    /// 最后写入时间(Unix 毫秒)。
    pub updated_at_ms: i64,
    /// Explicit baseline initialization state.
    /// 明确的基线初始化状态。
    pub initialization_state: RuntimeInitializationState,
    /// Current effective compaction pointer.
    /// 当前生效的压缩指针。
    pub current_compaction_id: Option<CompactionId>,
    /// Locked variable sources of the current generation.
    /// 当前代次的锁定可变来源。
    pub locked_sources: Vec<LockedSource>,
}

impl RuntimeDocument {
    /// Builds a fresh empty runtime document with the current contract.
    /// 使用当前契约构建一份全新的空运行时文档。
    pub fn new(session_id: impl Into<String>, updated_at_ms: i64) -> Self {
        Self {
            contract: RUNTIME_CONTRACT.to_string(),
            schema_version: 1,
            session_id: session_id.into(),
            updated_at_ms,
            initialization_state: RuntimeInitializationState::FreshUninitialized,
            current_compaction_id: None,
            locked_sources: Vec::new(),
        }
    }
}

/// Serialized `compactions.json` document.
/// 序列化的 `compactions.json` 文档。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompactionsDocument {
    /// File contract marking the V1 compactions format.
    /// 标记 V1 压缩节点格式的文件契约。
    pub contract: String,
    /// Schema version of the document.
    /// 文档的 schema 版本。
    pub schema_version: u32,
    /// Owning session id.
    /// 所属会话 id。
    pub session_id: String,
    /// Compaction node history in occurrence order.
    /// 按发生顺序排列的压缩节点历史。
    pub nodes: Vec<CompactionNode>,
}

impl CompactionsDocument {
    /// Builds a fresh empty compactions document with the current contract.
    /// 使用当前契约构建一份全新的空压缩文档。
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            contract: COMPACTIONS_CONTRACT.to_string(),
            schema_version: 1,
            session_id: session_id.into(),
            nodes: Vec::new(),
        }
    }

    /// Builds a compactions document from existing nodes.
    /// 从既有节点构建一份压缩文档。
    pub fn new_with_nodes(session_id: impl Into<String>, nodes: Vec<CompactionNode>) -> Self {
        Self {
            contract: COMPACTIONS_CONTRACT.to_string(),
            schema_version: 1,
            session_id: session_id.into(),
            nodes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The runtime document serializes with the exact frozen contract values.
    /// 运行时文档使用精确冻结的契约值序列化。
    #[test]
    fn runtime_document_contract_round_trip() {
        let document = RuntimeDocument::new("session-uuid", 1_700_000_000_000);
        let json = serde_json::to_string(&document).unwrap();
        assert!(json.contains(r#""contract":"vulcan.session-runtime.v1""#));
        assert!(json.contains(r#""schema_version":1"#));
        let decoded: RuntimeDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, document);
    }

    /// Locked sources survive a full serialize/deserialize cycle.
    /// 锁定来源经过完整序列化/反序列化周期后保持不变。
    #[test]
    fn locked_source_round_trip() {
        let document = RuntimeDocument {
            locked_sources: vec![LockedSource {
                node_id: "agents-project".to_string(),
                order: 0,
                target: LockedPromptTarget::Developer,
                kind: LockedSourceKind::AgentsMd,
                source_label: Some("AGENTS.md".to_string()),
                source_id: None,
                content: "always speak truth".to_string(),
            }],
            ..RuntimeDocument::new("s", 1)
        };
        let decoded: RuntimeDocument =
            serde_json::from_str(&serde_json::to_string(&document).unwrap()).unwrap();
        assert_eq!(decoded, document);
    }

    /// Explicit provenance projects every baseline kind without name-based classification.
    /// 显式来源事实无需按名称分类即可投影全部基线种类。
    #[test]
    fn explicit_baseline_provenance_projects_all_kinds_in_input_order() {
        let kinds = [
            LockedSourceKind::MemoryProfile,
            LockedSourceKind::AgentsMd,
            LockedSourceKind::VulcanMd,
            LockedSourceKind::AgentPersona,
            LockedSourceKind::OutputStyle,
            LockedSourceKind::PromptSkillInjection,
            LockedSourceKind::PluginInstructions,
            LockedSourceKind::BaselineHooks,
        ];
        let segments = kinds
            .into_iter()
            .enumerate()
            .map(|(index, kind)| LockedBaselineSegmentInput {
                kind,
                node_id: format!("deliberately-misleading-node-{index}"),
                target: if index % 2 == 0 {
                    LockedPromptTarget::System
                } else {
                    LockedPromptTarget::Developer
                },
                source_label: Some(format!("source-{index}")),
                source_id: Some(format!("snapshot-{index}")),
                content: format!("baseline body {index}"),
            })
            .collect();
        let projection = LockedBaselineProjection::project(segments).unwrap();
        assert_eq!(projection.locked_sources().len(), kinds.len());
        for (index, source) in projection.locked_sources().iter().enumerate() {
            assert_eq!(source.order as usize, index);
            assert_eq!(source.kind, kinds[index]);
        }

        let json = serde_json::to_string(&projection).unwrap();
        let decoded: LockedBaselineProjection = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, projection);
    }

    /// Baseline projection rejects ambiguous identities, sparse order, empty bodies, and unknown fields.
    /// 基线投影拒绝含糊身份、稀疏顺序、空正文与未知字段。
    #[test]
    fn baseline_projection_is_strict_and_canonical() {
        let segment = |node_id: &str, content: &str| LockedBaselineSegmentInput {
            kind: LockedSourceKind::AgentsMd,
            node_id: node_id.to_string(),
            target: LockedPromptTarget::Developer,
            source_label: Some("AGENTS.md".to_string()),
            source_id: Some("resource-1".to_string()),
            content: content.to_string(),
        };
        assert!(matches!(
            LockedBaselineProjection::project(vec![segment("same", "a"), segment("same", "b")]),
            Err(DomainError::InvalidLockedSource {
                field: "duplicate_node_id"
            })
        ));
        assert!(matches!(
            LockedBaselineProjection::project(vec![segment("node", "   ")]),
            Err(DomainError::InvalidLockedSource { field: "content" })
        ));

        let projection = LockedBaselineProjection::project(vec![segment("node", "body")]).unwrap();
        let mut sparse = serde_json::to_value(&projection).unwrap();
        sparse["locked_sources"][0]["order"] = serde_json::json!(2);
        assert!(serde_json::from_value::<LockedBaselineProjection>(sparse).is_err());
        let mut unknown = serde_json::to_value(&projection).unwrap();
        unknown["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<LockedBaselineProjection>(unknown).is_err());
    }

    /// Runtime documents and nested locked sources reject unknown durable fields.
    /// runtime 文档与其嵌套 locked source 会拒绝未知持久化字段。
    #[test]
    fn runtime_document_rejects_unknown_fields_at_every_durable_layer() {
        let document = RuntimeDocument {
            locked_sources: vec![LockedSource {
                node_id: "agents-project".to_string(),
                order: 0,
                target: LockedPromptTarget::Developer,
                kind: LockedSourceKind::AgentsMd,
                source_label: Some("AGENTS.md".to_string()),
                source_id: None,
                content: "always speak truth".to_string(),
            }],
            ..RuntimeDocument::new("s", 1)
        };
        let mut top_level = serde_json::to_value(&document).unwrap();
        top_level["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<RuntimeDocument>(top_level).is_err());

        let mut nested = serde_json::to_value(&document).unwrap();
        nested["locked_sources"][0]["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<RuntimeDocument>(nested).is_err());
    }

    /// Compaction documents and nested nodes reject unknown durable fields.
    /// compactions 文档与其嵌套节点会拒绝未知持久化字段。
    #[test]
    fn compactions_document_rejects_unknown_fields_at_every_durable_layer() {
        let document = CompactionsDocument::new_with_nodes(
            "s",
            vec![CompactionNode {
                compaction_id: CompactionId::try_new(1).unwrap(),
                api_call_id: ApiCallId::try_new(2).unwrap(),
                api_stats_seq: Seq::try_new(4).unwrap(),
                occurred_at_seq: Seq::try_new(3).unwrap(),
                retained_from_seq: Seq::try_new(1).unwrap(),
                content: CompactionContent::Summary {
                    text: "summary".to_string(),
                },
                created_at_ms: 5,
            }],
        );
        let mut top_level = serde_json::to_value(&document).unwrap();
        top_level["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<CompactionsDocument>(top_level).is_err());

        let mut nested = serde_json::to_value(&document).unwrap();
        nested["nodes"][0]["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<CompactionsDocument>(nested).is_err());
    }
}
