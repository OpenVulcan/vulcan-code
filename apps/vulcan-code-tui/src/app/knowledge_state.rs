//! Exact process-local knowledge selectors and explicit unsaved human policy drafts.
//! 精确进程本地知识选择器及明确未保存人工策略草稿。

use crate::service_api::{ServiceCall, knowledge_base};
use crate::tr;
use serde_json::{Value, json};
use vulcan_service_api::{
    KnowledgeChangeDocumentReadRequest, KnowledgeChangeSideRequest, KnowledgeDocumentReadRequest,
    KnowledgeEmbeddingSettingsRequest, KnowledgeIndexModeRequest,
    KnowledgeLibraryDocumentListRequest, KnowledgeLibrarySettingsRequest,
    KnowledgeRouteReferenceRequest,
};

/// Viewport size shared by this client's bounded lists, never a second server safety limit.
/// 此客户端有界列表共享视口大小，绝非第二个服务安全上限。
pub const KNOWLEDGE_PAGE_SIZE: u32 = 20;

/// Exact current or immutable historical body identity; no corpus or alternate selector exists.
/// 精确当前或不可变历史正文身份；不存在语料或替代选择器。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnowledgeDocumentAddress {
    /// Current stable document identity.
    /// 当前稳定文档身份。
    Current(String),
    /// Immutable change, document and explicit side.
    /// 不可变变更、文档及明确侧别。
    Historical {
        seq: u64,
        document_id: String,
        side: KnowledgeChangeSideRequest,
    },
}

/// Human draft fields never persist until explicit save validates the complete selected mode.
/// 人工草稿字段在明确保存验证完整选择模式前绝不持久化。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeDraftField {
    /// New library display name.
    /// 新库展示名称。
    Name,
    /// Human purpose directing AI maintenance.
    /// 指导 AI 维护的人工用途。
    Purpose,
    /// Conscious declared mode choice.
    /// 主动声明模式选择。
    Mode,
    /// Exact candidate route identity.
    /// 精确候选路由身份。
    Route,
    /// Explicit output dimension text.
    /// 明确输出维度文本。
    Dimensions,
    /// Optional explicit batch text; blank asks the server for its default.
    /// 可选明确批量文本；空白请求服务默认值。
    Batch,
}

/// Unsaved creation or mode change, frozen to one optional existing library.
/// 未保存创建或模式变更，冻结至一个可选既有知识库。
#[derive(Debug, Clone, Default)]
pub struct KnowledgePolicyDraft {
    /// None means new library; Some is the immutable existing owner.
    /// 空值表示新库；非空为不可变既有所有者。
    pub library_id: Option<String>,
    /// Human name, editable during creation only.
    /// 人工名称，仅创建期间可编辑。
    pub name: String,
    /// Human purpose, editable during creation only.
    /// 人工用途，仅创建期间可编辑。
    pub purpose: String,
    /// Explicit selected mode; new drafts begin unselected.
    /// 明确选定模式；新草稿开始时未选择。
    pub mode: Option<KnowledgeIndexModeRequest>,
    /// Exact chosen catalog route key, never a display name.
    /// 精确选定目录路由键，绝非展示名称。
    pub route_key: String,
    /// Exact entered dimensions, preserving invalid input for correction.
    /// 精确输入维度，保留非法输入以便修正。
    pub dimensions: String,
    /// Exact optional batch input.
    /// 精确可选批量输入。
    pub batch: String,
}

/// Every local selector is reset when the selected physical library changes.
/// 所选物理知识库变化时重置每个本地选择器。
#[derive(Debug, Clone)]
pub struct KnowledgeUiState {
    /// Canonical logical directory, empty for root.
    /// 规范逻辑目录；空字符串表示根。
    pub folder: String,
    /// Exact service-issued cursor stack; first None denotes the initial page.
    /// 精确服务签发游标栈；首个空值表示初始页。
    pub cursors: Vec<Option<String>>,
    /// Stable current or immutable history document owner.
    /// 稳定当前或不可变历史文档所有者。
    pub document: Option<KnowledgeDocumentAddress>,
    /// Service-issued one-based body continuations.
    /// 服务签发从一开始正文续读值。
    pub starts: Vec<usize>,
    /// Human-submitted search text; changing it revokes old results.
    /// 人工提交检索文本；改变后撤销旧结果。
    pub query: String,
    /// Exclusive immutable history sequence boundaries.
    /// 排他不可变历史序号边界。
    pub history_before: Vec<Option<u64>>,
    /// Exact immutable selected change.
    /// 精确不可变选定变更。
    pub change_seq: Option<u64>,
    /// Service-issued immutable image offsets.
    /// 服务签发不可变镜像偏移。
    pub change_offsets: Vec<u64>,
    /// Service-issued live failure offsets.
    /// 服务签发实时失败偏移。
    pub failure_offsets: Vec<u64>,
    /// Native package browser directory; None requests host default roots.
    /// 原生套件浏览目录；空值请求宿主默认根。
    pub package_path: Option<String>,
    /// The only local human policy draft.
    /// 唯一本地人工策略草稿。
    pub draft: Option<KnowledgePolicyDraft>,
}

impl Default for KnowledgeUiState {
    /// Creates an empty root view with no selected mode or document.
    /// 创建没有选定模式或文档的空根视图。
    /// Parameters: none. Returns the initial read-only selection state.
    /// 参数：无。返回初始只读选择状态。
    fn default() -> Self {
        Self {
            folder: String::new(),
            cursors: vec![None],
            document: None,
            starts: vec![1],
            query: String::new(),
            history_before: vec![None],
            change_seq: None,
            change_offsets: vec![0],
            failure_offsets: vec![0],
            package_path: None,
            draft: None,
        }
    }
}

impl KnowledgeUiState {
    /// Builds one exact logical directory page from this state's service cursors.
    /// 从此状态服务游标构建一个精确逻辑目录页。
    /// Parameters: library_id is the physical owner. Returns the shared current request shape.
    /// 参数：library_id 为物理所有者。返回共享当前请求形状。
    pub fn directory_request(&self, library_id: &str) -> KnowledgeLibraryDocumentListRequest {
        KnowledgeLibraryDocumentListRequest {
            library_id: library_id.into(),
            path: self.folder.clone(),
            depth: 1,
            cursor: self.cursors.last().expect("initial cursor").clone(),
            limit: KNOWLEDGE_PAGE_SIZE,
        }
    }

    /// Builds the current or exact historical reader without probing candidate fields.
    /// 构建当前或精确历史阅读器，不探测候选字段。
    /// Parameters: library_id owns the selected document. Returns no request until a document is explicitly selected.
    /// 参数：library_id 拥有所选文档。在明确选择文档前不返回请求。
    pub fn document_call(&self, library_id: &str) -> Option<ServiceCall> {
        // Pagination always follows an actual service continuation.
        // 分页始终遵循真实服务续读值。
        let start_line = Some(*self.starts.last().expect("initial body start"));
        Some(match self.document.as_ref()? {
            KnowledgeDocumentAddress::Current(id) => {
                knowledge_base::read_managed_knowledge_document(KnowledgeDocumentReadRequest {
                    library_id: library_id.into(),
                    document_id: Some(id.clone()),
                    path: None,
                    start_line,
                    end_line: None,
                })
            }
            KnowledgeDocumentAddress::Historical {
                seq,
                document_id,
                side,
            } => {
                knowledge_base::read_knowledge_change_document(KnowledgeChangeDocumentReadRequest {
                    library_id: library_id.into(),
                    seq: *seq,
                    document_id: document_id.clone(),
                    side: *side,
                    start_line,
                    end_line: None,
                })
            }
        })
    }
}

/// Projects the exact declared binding of one official candidate into the human request route.
/// 将一个官方候选精确声明绑定投影为人工请求路由。
/// Parameters: candidate is a current public inference candidate. Returns None if its declared binding cannot be decoded.
/// 参数：candidate 为当前公开推理候选。声明绑定无法解码时返回空值。
pub fn candidate_route(candidate: &Value) -> Option<KnowledgeRouteReferenceRequest> {
    // The route has the exact snake-case public-AI wire format.
    // 路由具有精确蛇形公开 AI 线协议格式。
    let route = candidate.get("route")?;
    // Binding kind alone selects its documented protocol or action identifier.
    // 仅绑定种类选择其文档定义协议或动作身份。
    let binding = route.get("binding")?;
    // Both branches are explicitly declared by the shared public-AI union.
    // 两个分支由共享公开 AI 联合明确声明。
    let (kind, protocol) = match binding.get("kind")?.as_str()? {
        "protocol" => ("protocol", binding.get("protocol_profile_id")?.as_str()?),
        "action" => ("action", binding.get("action_binding_id")?.as_str()?),
        _ => return None,
    };
    serde_json::from_value(json!({"providerId":route.get("provider_id")?,"serviceEntryId":route.get("service_entry_id")?,"connectionId":route.get("connection_id")?,"modelId":route.get("model_id")?,"bindingKind":kind,"protocol":protocol})).ok()
}

/// Produces a non-secret exact route identity without delimiter collisions.
/// 产生不含秘密的精确路由身份，避免分隔符碰撞。
/// Parameters: candidate is a current catalog member. Returns its serialized declared route or None for invalid structure.
/// 参数：candidate 为当前目录成员。返回其序列化声明路由，结构无效时返回空值。
pub fn candidate_key(candidate: &Value) -> Option<String> {
    candidate_route(candidate)
        .map(|route| serde_json::to_string(&route).expect("serializable route"))
}

/// Checks the same dense/float/query/document/metric admission as the WebUI.
/// 检查与 WebUI 相同的稠密、浮点、查询、文档及度量准入。
/// Parameters: candidate owns one current official capability. Returns whether it is an eligible enabled background route.
/// 参数：candidate 拥有一个当前官方能力。返回其是否为符合要求且已启用的后台路由。
pub fn embedding_candidate(candidate: &Value) -> bool {
    // Capability absence is defined by the public model contract, not a fallback location.
    // 能力缺失由公开模型契约定义，绝非兜底位置。
    let Some(capability) = candidate
        .pointer("/capabilities/embedding")
        .filter(|value| value.is_object())
    else {
        return false;
    };
    // Each declared capability list is required for admission.
    // 每个声明能力列表均为准入所需。
    let includes = |key: &str, value: &str| {
        capability
            .get(key)
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().any(|entry| entry.as_str() == Some(value)))
    };
    candidate.get("enabled").and_then(Value::as_bool) == Some(true)
        && candidate
            .pointer("/route/operation")
            .and_then(Value::as_str)
            == Some("embedding_create")
        && candidate
            .pointer("/route/usage_scope")
            .and_then(Value::as_str)
            == Some("internal_background")
        && includes("output_kinds", "dense")
        && includes("encodings", "float")
        // Like the WebUI, any declared metric is admitted; the service treats `unknown` as cosine.
        // 与 WebUI 一致，接受任意声明的度量；服务将 `unknown` 按余弦处理。
        && (includes("tasks", "query") || includes("tasks", "provider_default"))
        && (includes("tasks", "document") || includes("tasks", "provider_default"))
        && capability.get("maximum_inputs").and_then(Value::as_u64) != Some(0)
        && candidate_route(candidate).is_some()
}

impl KnowledgePolicyDraft {
    /// Validates the complete explicit human draft against its exact current model, never choosing a replacement.
    /// 对照精确当前模型验证完整明确人工草稿，绝不选择替代项。
    /// Parameters: ai is the authoritative settings snapshot. Returns shared settings only for a fully admitted draft.
    /// 参数：ai 为权威设置快照。仅对完整准入草稿返回共享设置。
    pub fn settings(&self, ai: &Value) -> Option<KnowledgeLibrarySettingsRequest> {
        // The human must select a mode even when no inference is needed.
        // 即使无需推理，人工也必须选择模式。
        let index_mode = self.mode?;
        if matches!(
            index_mode,
            KnowledgeIndexModeRequest::Browse | KnowledgeIndexModeRequest::Bm25
        ) {
            return Some(KnowledgeLibrarySettingsRequest {
                index_mode,
                embedding: None,
            });
        }
        // Only the chosen official candidate may supply capabilities and route identity.
        // 仅选定官方候选可提供能力及路由身份。
        let candidate = ai
            .get("internalInferenceCandidates")?
            .as_array()?
            .iter()
            .find(|candidate| {
                candidate_key(candidate).as_deref() == Some(self.route_key.as_str())
                    && embedding_candidate(candidate)
            })?;
        // Model dimensions must satisfy its explicit list and optional declared range.
        // 模型维度必须满足其明确列表及可选声明范围。
        let dimensions = self
            .dimensions
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|value| *value > 0)?;
        // The capability location was proven during candidate admission.
        // 能力位置已在候选准入时证明。
        let capability = candidate.pointer("/capabilities/embedding")?;
        // An empty allowed list defers to the declared model range, matching the WebUI.
        // 空允许列表遵循声明模型范围，与 WebUI 相同。
        let allowed = capability.get("dimensions")?.as_array()?;
        if (!allowed.is_empty()
            && !allowed
                .iter()
                .any(|value| value.as_u64() == Some(u64::from(dimensions))))
            || capability
                .get("minimum_dimensions")
                .and_then(Value::as_u64)
                .is_some_and(|minimum| u64::from(dimensions) < minimum)
            || capability
                .get("maximum_dimensions")
                .and_then(Value::as_u64)
                .is_some_and(|maximum| u64::from(dimensions) > maximum)
        {
            return None;
        }
        // Blank batching requests the server default; explicit input is never silently replaced.
        // 空批量请求服务默认；明确输入绝不被静默替换。
        let batch_size = if self.batch.trim().is_empty() {
            None
        } else {
            Some(
                self.batch
                    .trim()
                    .parse::<u32>()
                    .ok()
                    .filter(|value| *value > 0)?,
            )
        };
        if batch_size.is_some_and(|batch| {
            capability
                .get("maximum_inputs")
                .and_then(Value::as_u64)
                .is_some_and(|maximum| u64::from(batch) > maximum)
        }) {
            return None;
        }
        Some(KnowledgeLibrarySettingsRequest {
            index_mode,
            embedding: Some(KnowledgeEmbeddingSettingsRequest {
                route: candidate_route(candidate)?,
                dimensions,
                batch_size,
            }),
        })
    }
}

/// Localizes declared modes with shared WebUI keys.
/// 使用共享 WebUI 键本地化声明模式。
/// Parameters: mode is the exact service or draft code. Returns its label or a visible unavailable state.
/// 参数：mode 为精确服务或草稿代码。返回其标签或可见不可用状态。
pub fn mode_label(mode: &str) -> String {
    match mode {
        "browse" => tr!("knowledge.mode.browse"),
        "bm25" => tr!("knowledge.mode.bm25"),
        "vector" => tr!("knowledge.mode.vector"),
        "hybrid" => tr!("knowledge.mode.hybrid"),
        _ => tr!("knowledge.unavailable"),
    }
}

/// Explains the declared search cost and capability using the shared catalog.
/// 使用共享目录解释声明检索成本及能力。
/// Parameters: mode is the exact current library mode. Returns its localized explanation.
/// 参数：mode 为精确当前知识库模式。返回其本地化说明。
pub fn mode_description(mode: &str) -> String {
    match mode {
        "browse" => tr!("knowledge.mode.browse.description"),
        "bm25" => tr!("knowledge.mode.bm25.description"),
        "vector" => tr!("knowledge.mode.vector.description"),
        "hybrid" => tr!("knowledge.mode.hybrid.description"),
        _ => tr!("knowledge.unavailable"),
    }
}

/// Localizes the service's full-text state without exposing internal codes as UI labels.
/// 本地化服务全文状态，不将内部代码作为界面标签。
/// Parameters: state is the exact status code. Returns an explicit localized state.
/// 参数：state 为精确状态代码。返回明确本地化状态。
pub fn fts_label(state: &str) -> String {
    match state {
        "ready" => tr!("knowledge.indexStatus.ready"),
        "disabled" => tr!("knowledge.indexStatus.disabled"),
        "building" => tr!("knowledge.indexStatus.building"),
        "stale" => tr!("knowledge.index.needsRepair"),
        "error" => tr!("knowledge.indexStatus.error"),
        _ => tr!("knowledge.unavailable"),
    }
}
