//! Strict V1 session creation, exact loading, and registry publication.
//! 严格的 V1 会话创建、精确加载与注册表发布。

use std::fmt;
use std::sync::Arc;

#[cfg(test)]
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, macros::format_description};
use vulcan_agent_runtime::agent_runtime::task_runtime::TaskV1ExecutionFacts;
use vulcan_core::types::ConversationAttachment;
use vulcan_session_core::v1::config::{ForkOrigin, MetadataDocument};
use vulcan_session_core::v1::document::{
    ClientInputSubmission, ForkGenerationInput, IdempotentMutation, SessionDocument,
};
use vulcan_session_core::v1::errors::DomainError;
use vulcan_session_core::v1::ids::{ClientActionId, ResourceId, Seq};
use vulcan_workbench_model::SessionId;
use vulcan_workspace_store::data_plane::HostRuntimeHandle;
use vulcan_workspace_store::workspace_store::{
    CreateEmbeddedTaskSessionV1Request, CreateSessionV1Request, ProjectResourceCatalogError,
    ProjectResourceCatalogRegistry, SESSION_V1_STORAGE_PARTITION, SessionV1ListEntry,
    SessionV1StoragePreflightOutcome, SessionV1StoreError, create_embedded_task_session_v1,
    create_indexed_session_v1, delete_session_v1, list_active_sessions_v1,
    list_archived_sessions_v1, load_session_v1_full, load_session_v1_tail,
    locate_embedded_task_session_v1, locate_optional_session_v1_storage, locate_session_v1_storage,
    preflight_session_v1_storage, project_resource_directory_v1, project_session_lifecycle_v1,
};

use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionHandle, SessionMutationAdmission, SessionMutationChange,
    SessionRuntimeError, SessionRuntimeRegistry,
};

mod deletion;
pub(crate) mod guards;

pub(crate) use guards::publish_guarded_edit_generation;

/// Returns an explicit lifecycle error for `root` without converting corruption into missing data.
/// 为 `root` 返回明确生命周期错误，不将损坏转换为数据缺失。
fn invalid(root: SessionId, detail: impl Into<String>) -> SessionV1LifecycleError {
    SessionV1LifecycleError::InvalidDeletionTree {
        session_id: root,
        detail: detail.into(),
    }
}

/// Resolves the declared physical tree of `root` into globally ordered immutable identities.
/// 将 `root` 声明的物理树解析为全局有序的不可变身份。
/// Returns an error for any malformed identity, duplicated owner or unreadable embedded metadata.
/// 任何畸形身份、重复所有者或不可读嵌入元数据均返回错误。
fn deletion_members(
    root: &vulcan_workspace_store::workspace_store::SessionV1StorageLocator,
    root_id: SessionId,
) -> Result<
    std::collections::BTreeMap<
        SessionId,
        vulcan_workspace_store::workspace_store::SessionV1StorageLocator,
    >,
    SessionV1LifecycleError,
> {
    // Keep the root in the same ordered lock set as its physically owned descendants.
    // 将根与其物理归属后代保留在同一有序锁集合中。
    let mut tree = std::collections::BTreeMap::from([(root_id, root.clone())]);
    for locator in root.embedded_task_tree_locators()? {
        // Service identities are UUIDs, while lower-level storage validates its own name contract.
        // 服务身份为 UUID，底层存储则校验自身名称契约。
        let id = locator
            .session_id()
            .parse::<SessionId>()
            .map_err(|error| invalid(root_id, error.to_string()))?;
        if tree.insert(id, locator).is_some() {
            return Err(invalid(
                root_id,
                "Physical tree repeats a session identity.",
            ));
        }
    }
    Ok(tree)
}

/// Complete caller-resolved facts required to create one root V1 session.
/// 创建一个根 V1 会话所需的完整调用方已解析事实。
pub(crate) struct CreateRootSessionV1Request {
    /// Stable application session identity.
    /// 稳定的应用会话身份。
    pub(crate) session_id: SessionId,
    /// Explicit owning project id, also used as the metadata project tag.
    /// 显式所属项目 ID，同时用作 metadata 项目标记。
    pub(crate) project_id: String,
    /// Initial display title.
    /// 初始显示标题。
    pub(crate) title: String,
    /// Complete root submission retaining client identity, text, and ordered resources.
    /// 保留客户端身份、正文与有序资源的完整根提交。
    pub(crate) root_submission: ClientInputSubmission,
    /// Authoritative creation and root-input Unix time in milliseconds.
    /// 权威创建与根输入 Unix 毫秒时间。
    pub(crate) now_ms: i64,
}

/// Complete immutable request for creating or replaying one forked V1 session.
/// 创建或重放一个分叉 V1 会话的完整不可变请求。
pub(crate) struct CreateForkSessionV1Request {
    /// Exact parent session identity.
    /// 精确父会话身份。
    pub(crate) source_session_id: SessionId,
    /// Parent event-tree generation observed by the client.
    /// 客户端观察到的父事件树 generation。
    pub(crate) expected_source_generation: u64,
    /// Stable child identity reused by exact retries.
    /// 由精确重试复用的稳定子会话身份。
    pub(crate) child_session_id: SessionId,
    /// Exact child display title.
    /// 精确子会话显示标题。
    pub(crate) title: String,
    /// Parent terminal TurnStats seq retained as the child completed-history boundary.
    /// 作为子会话完整历史边界保留的父级终态 TurnStats seq。
    pub(crate) source_turn_end_seq: Seq,
    /// Semantic creation time used only by the first successful publication.
    /// 仅由首次成功发布使用的语义创建时间。
    pub(crate) now_ms: i64,
}

/// Result of one exact fork lifecycle admission.
/// 一次精确分叉生命周期准入的结果。
pub(crate) struct CreatedForkSessionV1 {
    /// Unique process-local owner of the child session.
    /// 子会话的唯一进程内 owner。
    pub(crate) handle: Arc<SessionHandle>,
    /// Whether this invocation physically published the child.
    /// 本次调用是否物理发布了子会话。
    pub(crate) created: bool,
    /// Exact parent terminal TurnStats seq retained by the child.
    /// 子会话保留的精确父级终态 TurnStats seq。
    pub(crate) source_turn_end_seq: Seq,
}

/// Structured failures produced by the strict V1 lifecycle boundary.
/// 严格 V1 生命周期边界产生的结构化失败。
#[derive(Debug)]
pub(crate) enum SessionV1LifecycleError {
    /// The process-local owner or domain rejected the operation.
    /// 进程内所有者或领域层拒绝了该操作。
    Runtime(SessionRuntimeError),
    /// The exact V1 storage facade rejected the operation.
    /// 精确 V1 存储门面拒绝了该操作。
    Storage(SessionV1StoreError),
    /// The project-level resource owner rejected attachment registration.
    /// 项目级资源 owner 拒绝了附件注册。
    ResourceCatalog(ProjectResourceCatalogError),
    /// The supplied Unix millisecond timestamp cannot form a UTC date bucket.
    /// 给定 Unix 毫秒时间戳无法形成 UTC 日期桶。
    InvalidDateBucket {
        /// Rejected Unix timestamp in milliseconds.
        /// 被拒绝的 Unix 毫秒时间戳。
        now_ms: i64,
        /// Stable formatting or range error detail.
        /// 稳定的格式或范围错误详情。
        detail: String,
    },
    /// One TaskRuntime admission timestamp cannot fit the V1 signed millisecond contract.
    /// 一条 TaskRuntime 准入时间戳无法放入 V1 有符号毫秒契约。
    InvalidTaskAdmissionTimestamp {
        /// Rejected unsigned runtime timestamp.
        /// 被拒绝的无符号运行时时间戳。
        admitted_at_ms: u64,
    },
    /// Root input mutated memory without yielding one complete flush candidate.
    /// 根输入修改了内存，却没有产生一份完整 flush 候选。
    MissingInitialFlushCandidate(SessionId),
    /// Physical deletion is blocked while the canonical session has unsettled collaboration.
    /// 当规范会话仍有未结协作义务时，阻止物理删除。
    UnsettledCollaboration(SessionId),
    /// One claimed session handle is not the registered canonical owner of its indexed root.
    /// 一个声明的会话句柄不是其索引根的已注册规范所有者。
    CanonicalOwnerUnavailable {
        /// Exact session whose canonical owner could not be proven.
        /// 无法证明其规范所有者的精确会话。
        session_id: SessionId,
    },
    /// Physical membership or canonical ownership changed before a deletion could be sealed.
    /// 删除封存前，物理成员集合或规范归属发生变化。
    InvalidDeletionTree {
        /// Exact indexed root selected for deletion.
        /// 选中删除的精确索引根。
        session_id: SessionId,
        /// Explicit rejected identity, structure or cleanup failure.
        /// 明确被拒绝的身份、结构或清理失败。
        detail: String,
    },
    /// A fresh construction unexpectedly observed an already committed root action.
    /// 全新构建意外观察到已经提交的根动作。
    UnexpectedIdempotentRootAction {
        /// Session being constructed.
        /// 正在构建的会话。
        session_id: SessionId,
        /// External root action reported as already committed.
        /// 被报告为已经提交的外部根动作。
        client_action_id: ClientActionId,
    },
    /// Durable metadata committed but the SQLite list projection failed.
    /// 权威 metadata 已可靠提交，但 SQLite 列表投影失败。
    IndexProjectionFailed {
        /// Session whose durable metadata now leads the list projection.
        /// 可靠 metadata 当前领先列表投影的会话。
        session_id: SessionId,
        /// Stable lifecycle operation name.
        /// 稳定的生命周期操作名。
        operation: &'static str,
        /// Exact storage-layer projection failure.
        /// 精确的存储层投影失败。
        source: SessionV1StoreError,
    },
    /// Restore found no exact metadata and removed the stale SQLite locator.
    /// 恢复时未找到精确 metadata，并已删除陈旧 SQLite 定位记录。
    RestoreStorageMissing {
        /// Session whose stale locator was removed.
        /// 陈旧定位记录已删除的会话。
        session_id: SessionId,
    },
    /// One SQLite V1 list row contains a session identity that violates the application contract.
    /// 涓€鏉?SQLite V1 鍒楄〃璁板綍鍖呭惈杩濆弽搴旂敤濂戠害鐨勪細璇濊韩浠姐€?
    InvalidIndexedSessionId {
        /// Project whose exact row query returned the invalid identity.
        /// 绮剧‘琛屾煡璇㈣繑鍥炴棤鏁堣韩浠界殑椤圭洰銆?
        project_id: String,
        /// Rejected persisted identity text.
        /// 琚嫆缁濈殑鎸佷箙鍖栬韩浠芥枃鏈€?
        session_id: String,
        /// Exact UUID parsing failure.
        /// 绮剧‘鐨?UUID 瑙ｆ瀽澶辫触銆?
        detail: String,
    },
    /// A project-level lifecycle transition left rows in the source state.
    /// 椤圭洰绾х敓鍛藉懆鏈熻浆鎹㈠悗浠嶆湁璁板綍鐣欏湪婧愮姸鎬併€?
    ProjectLifecycleIncomplete {
        /// Exact project identity being transitioned.
        /// 姝ｅ湪杞崲鐨勭簿纭」鐩韩浠姐€?
        project_id: String,
        /// Stable operation label.
        /// 绋冲畾鐨勬搷浣滄爣璁般€?
        operation: &'static str,
        /// Persisted session identities still left in the source state.
        /// 浠嶇暀鍦ㄦ簮鐘舵€佺殑鎸佷箙鍖栦細璇濊韩浠姐€?
        remaining_session_ids: Vec<String>,
    },
    /// A stable child id already belongs to a different fork request.
    /// 一个稳定子会话 ID 已经属于另一条分叉请求。
    ForkRetryConflict {
        /// Conflicting child session identity.
        /// 发生冲突的子会话身份。
        child_session_id: SessionId,
        /// Exact immutable field that disagreed.
        /// 不一致的精确不可变字段。
        field: &'static str,
    },
}

impl fmt::Display for SessionV1LifecycleError {
    /// Formats one stable operator-facing lifecycle failure.
    /// 格式化一条稳定的面向操作方生命周期失败。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(error) => write!(formatter, "V1 session runtime failure: {error}"),
            Self::Storage(error) => write!(formatter, "V1 session storage failure: {error}"),
            Self::ResourceCatalog(error) => {
                write!(formatter, "V1 project resource failure: {error}")
            }
            Self::InvalidDateBucket { now_ms, detail } => write!(
                formatter,
                "V1 session timestamp {now_ms} cannot form a UTC date bucket: {detail}"
            ),
            Self::InvalidTaskAdmissionTimestamp { admitted_at_ms } => write!(
                formatter,
                "V1 task admission timestamp {admitted_at_ms} exceeds the signed millisecond range"
            ),
            Self::MissingInitialFlushCandidate(session_id) => write!(
                formatter,
                "V1 session {session_id} root input produced no initial flush candidate"
            ),
            Self::UnsettledCollaboration(session_id) => write!(
                formatter,
                "V1 session {session_id} cannot be physically deleted while collaboration is unsettled"
            ),
            Self::CanonicalOwnerUnavailable { session_id } => write!(
                formatter,
                "V1 session {session_id} history edit requires its registered canonical indexed root owner"
            ),
            Self::InvalidDeletionTree { session_id, detail } => write!(
                formatter,
                "V1 session {session_id} deletion tree rejected: {detail}"
            ),
            Self::UnexpectedIdempotentRootAction {
                session_id,
                client_action_id,
            } => write!(
                formatter,
                "fresh V1 session {session_id} unexpectedly reported root client action {} as idempotent",
                client_action_id.as_str()
            ),
            Self::IndexProjectionFailed {
                session_id,
                operation,
                source,
            } => write!(
                formatter,
                "V1 session {session_id} durable {operation} metadata is pending SQLite projection: {source}"
            ),
            Self::RestoreStorageMissing { session_id } => write!(
                formatter,
                "V1 session {session_id} restore failed because exact metadata is missing; the stale SQLite locator was removed"
            ),
            Self::InvalidIndexedSessionId {
                project_id,
                session_id,
                detail,
            } => write!(
                formatter,
                "V1 project {project_id} contains invalid indexed session id `{session_id}`: {detail}"
            ),
            Self::ProjectLifecycleIncomplete {
                project_id,
                operation,
                remaining_session_ids,
            } => write!(
                formatter,
                "V1 project {project_id} {operation} left source-state sessions: {}",
                remaining_session_ids.join(", ")
            ),
            Self::ForkRetryConflict {
                child_session_id,
                field,
            } => write!(
                formatter,
                "V1 fork retry for child {child_session_id} conflicts on immutable field {field}"
            ),
        }
    }
}

impl std::error::Error for SessionV1LifecycleError {
    /// Returns the wrapped runtime or storage cause when one exists.
    /// 在存在包装的运行时或存储原因时返回该原因。
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Runtime(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::ResourceCatalog(error) => Some(error),
            Self::IndexProjectionFailed { source, .. } => Some(source),
            Self::InvalidDateBucket { .. }
            | Self::InvalidTaskAdmissionTimestamp { .. }
            | Self::MissingInitialFlushCandidate(_)
            | Self::UnsettledCollaboration(_)
            | Self::CanonicalOwnerUnavailable { .. }
            | Self::InvalidDeletionTree { .. }
            | Self::RestoreStorageMissing { .. }
            | Self::InvalidIndexedSessionId { .. }
            | Self::ProjectLifecycleIncomplete { .. }
            | Self::ForkRetryConflict { .. }
            | Self::UnexpectedIdempotentRootAction { .. } => None,
        }
    }
}

impl From<SessionRuntimeError> for SessionV1LifecycleError {
    /// Preserves one structured runtime failure at the lifecycle boundary.
    /// 在生命周期边界保留一条结构化运行时失败。
    fn from(error: SessionRuntimeError) -> Self {
        Self::Runtime(error)
    }
}

impl From<SessionV1StoreError> for SessionV1LifecycleError {
    /// Preserves one structured storage failure at the lifecycle boundary.
    /// 在生命周期边界保留一条结构化存储失败。
    fn from(error: SessionV1StoreError) -> Self {
        Self::Storage(error)
    }
}

impl From<ProjectResourceCatalogError> for SessionV1LifecycleError {
    /// Preserves one project-resource failure at the lifecycle boundary.
    /// 在生命周期边界保留一条项目资源失败。
    fn from(error: ProjectResourceCatalogError) -> Self {
        Self::ResourceCatalog(error)
    }
}

impl From<DomainError> for SessionV1LifecycleError {
    /// Preserves one domain failure through the existing runtime error contract.
    /// 通过现有运行时错误契约保留一条领域失败。
    fn from(error: DomainError) -> Self {
        Self::Runtime(SessionRuntimeError::from(error))
    }
}

/// Registers authoritative attachments before any root session publication.
/// 在发布任何根会话前注册权威附件。
///
/// The caller must pass the canonical attachments already rebuilt from the
/// host asset index. This function reads only their exact managed paths,
/// verifies the retained byte length and SHA-256, preserves input order, and
/// delegates all logical-id allocation to the unique project owner.
/// 调用方必须传入已经由宿主资源索引重建的规范附件。本函数只读取其精确托管路径，校验保留的字节长度
/// 与 SHA-256，保持输入顺序，并把全部逻辑 ID 分配委托给唯一项目 owner。
///
/// # Parameters / 参数
///
/// - `controller`: Authoritative application data-root owner.
///   `controller`：权威应用数据根 owner。
/// - `registry`: Process-local project resource owner registry.
///   `registry`：进程内项目资源 owner 注册表。
/// - `project_id`: Exact V1 project partition id.
///   `project_id`：精确 V1 项目分区 ID。
/// - `attachments`: Canonical attachments in submitted order.
///   `attachments`：按提交顺序排列的规范附件。
///
/// # Returns / 返回值
///
/// Ordered project-scoped resource ids, or an explicit attachment/catalog
/// failure before root session creation is allowed to start.
/// 有序项目级资源 ID，或在允许开始根会话创建前返回显式附件/目录失败。
pub(crate) fn register_project_attachments_v1(
    controller: &HostRuntimeHandle,
    registry: &ProjectResourceCatalogRegistry,
    project_id: &str,
    attachments: &[ConversationAttachment],
) -> Result<Vec<ResourceId>, SessionV1LifecycleError> {
    // project_dir is derived from the same data root, storage partition, and
    // project partition used by root-session persistence.
    // project_dir 由根会话持久化使用的同一数据根、存储分区与项目分区派生。
    let project_dir = project_resource_directory_v1(
        &controller.data_root,
        SESSION_V1_STORAGE_PARTITION,
        project_id,
    )?;
    registry
        .resolve_or_register_managed_attachments(&project_dir, attachments)
        .map_err(SessionV1LifecycleError::from)
}

/// Explicit source owner for one ordered prompt attachment, distinguishing staged assets from existing project occurrences.
/// 一个有序提示词附件的显式来源所有者，区分暂存资产与既有项目出现。
#[derive(Debug, Clone)]
pub(crate) enum PromptAttachmentOrigin {
    /// A newly staged asset and its optional original composer preparation.
    /// 新暂存资产及其可选原输入区准备。
    Staged(Option<Box<vulcan_agent_runtime::attachment_asset_index::AttachmentDraftMediaOrigin>>),
    /// An existing project ResourceId and preparation resolved from its exact original input position.
    /// 既有项目ResourceId及从其精确原输入位置解析的准备。
    Project {
        /// Immutable existing logical resource, not a newly allocated staged copy.
        /// 不可变既有逻辑资源，而非新分配暂存副本。
        resource_id: ResourceId,
        /// Known, pending or explicitly unproven preparation owned by the source occurrence.
        /// 来源出现持有的已知、待准备或明确未经证明的准备。
        preparation: Box<vulcan_session_core::v1::UserPromptResourcePreparation>,
    },
}

/// Resolves ordered `attachments` using explicit optional project `existing_ids`; only staged occurrences read/register new bytes.
/// 使用显式可选项目`existing_ids`解析有序`attachments`；只有暂存出现读取或注册新字节。
/// The exact controller/project owner validates all existing catalog identities before any staged registration and returns IDs in input order.
/// 精确控制器及项目所有者在暂存注册之前校验全部既有目录身份，按输入顺序返回ID。
pub(crate) fn register_project_attachment_occurrences_v1(
    controller: &HostRuntimeHandle,
    registry: &ProjectResourceCatalogRegistry,
    project_id: &str,
    attachments: &[ConversationAttachment],
    existing_ids: &[Option<ResourceId>],
) -> Result<Vec<ResourceId>, String> {
    if attachments.len() != existing_ids.len() {
        return Err(String::from(
            "Attachment occurrence identity count is invalid.",
        ));
    }
    // Existing logical identities belong to the same partition used by session persistence.
    // 既有逻辑身份属于会话持久化使用的同一分区。
    let project = project_resource_directory_v1(
        &controller.data_root,
        SESSION_V1_STORAGE_PARTITION,
        project_id,
    )
    .map_err(|error| error.to_string())?;
    if existing_ids.iter().any(Option::is_some) {
        // Read catalog metadata once; source bytes are deliberately deferred to the final V1 media gate.
        // 只读取一次目录元数据；来源字节有意延迟至最终V1媒体门。
        let catalog =
            vulcan_workspace_store::workspace_store::load_project_resource_catalog(&project)
                .map_err(|error| error.to_string())?;
        for (attachment, resource_id) in attachments.iter().zip(existing_ids) {
            if let Some(resource_id) = resource_id {
                // Exact logical identity is mandatory; identical content in another row is not a substitute.
                // 必须具有精确逻辑身份；另一条目中的相同内容不能替代。
                let entry = catalog
                    .entries
                    .iter()
                    .find(|entry| entry.resource_id == *resource_id)
                    .ok_or_else(|| {
                        String::from(
                            "Existing attachment resource is absent from the project catalog.",
                        )
                    })?;
                // Metadata and private path must come from this catalog owner, not from client-supplied source hints.
                // 元数据及私有路径必须来自本目录所有者，而非客户端提供的来源提示。
                let path = project
                    .join(&entry.entity_relative_path)
                    .to_string_lossy()
                    .into_owned();
                if attachment.asset_id.is_some()
                    || attachment.source_path.is_some()
                    || attachment.resource_path.is_some()
                    || attachment.source_sha256.as_deref() != Some(entry.sha256.as_str())
                    || attachment.byte_len != Some(entry.size)
                    || attachment.mime_type
                        != entry.mime.as_deref().unwrap_or("application/octet-stream")
                    || attachment.file_name != entry.original_name
                    || vulcan_runtime_assets::native_host_path_text(&attachment.local_path)
                        != vulcan_runtime_assets::native_host_path_text(&path)
                {
                    return Err(String::from(
                        "Existing attachment metadata differs from its exact project catalog owner.",
                    ));
                }
            }
        }
    }
    // Only explicit staged occurrences acquire permanent resources; project references never re-upload or reread here.
    // 只有显式暂存出现取得永久资源；项目引用在此绝不重新上传或读取。
    let staged = attachments
        .iter()
        .zip(existing_ids)
        .filter_map(|(attachment, resource_id)| resource_id.is_none().then_some(attachment.clone()))
        .collect::<Vec<_>>();
    // Registration's existing atomic source checks remain authoritative for the staged subset.
    // 既有注册的原子来源检查继续对暂存子集具有权威性。
    let mut registered = if staged.is_empty() {
        Vec::new()
    } else {
        register_project_attachments_v1(controller, registry, project_id, &staged)
            .map_err(|error| error.to_string())?
    }
    .into_iter();
    existing_ids
        .iter()
        .map(|existing| match existing {
            Some(resource_id) => Ok(*resource_id),
            None => registered.next().ok_or_else(|| {
                String::from("Staged registration did not return every ordered resource.")
            }),
        })
        .collect()
}

/// Registers ordered `attachments` and transfers their exact optional draft `origins` into prompt-owned parameters.
/// 注册有序`attachments`，并将其精确可选草稿`origins`转入提示词所有参数。
/// Private receipts are appended before a new prompt is published; `accepted_input` instead restores an exact retry's original admission proof.
/// 新提示词发布前追加私有回执；`accepted_input`则恢复精确重试的原准入证明。
pub(crate) fn register_project_prompt_attachments_v1(
    controller: &HostRuntimeHandle,
    registry: &ProjectResourceCatalogRegistry,
    project_id: &str,
    attachments: &[ConversationAttachment],
    origins: &[PromptAttachmentOrigin],
    accepted_input: Option<&ClientInputSubmission>,
) -> Result<
    (
        Vec<ResourceId>,
        Option<vulcan_session_core::v1::UserPromptParameters>,
    ),
    String,
> {
    use vulcan_session_core::v1::{UserPromptParameters, UserPromptResourcePreparation};
    if origins.len() != attachments.len() {
        return Err(String::from(
            "Prompt media origin count differs from its exact attachment order.",
        ));
    }
    // An exact retry carries initial admission proof, never a new draft preparation.
    // 精确重试携带初始准入证明，绝不是新草稿准备。
    if let Some(accepted) = accepted_input {
        if origins
            .iter()
            .any(|origin| matches!(origin, PromptAttachmentOrigin::Staged(Some(_))))
        {
            return Err(String::from(
                "Accepted input retry must not replace its original proof with current draft preparation.",
            ));
        }
        if let Some(parameters) = &accepted.parameters {
            parameters
                .validate(&accepted.resource_ids)
                .map_err(|error| error.to_string())?;
        }
    }
    for (attachment, origin) in attachments.iter().zip(origins) {
        match origin {
            PromptAttachmentOrigin::Staged(Some(origin)) => {
                origin.validate_attachment(attachment)?
            }
            PromptAttachmentOrigin::Project {
                resource_id,
                preparation,
            } => {
                // Validate known source proof before any staged resource or receipt can be published.
                // 发布任何暂存资源或回执之前，校验已知来源证明。
                UserPromptParameters::for_resource_preparations(
                    &[*resource_id],
                    vec![preparation.as_ref().clone()],
                )
                .map_err(|error| error.to_string())?;
            }
            PromptAttachmentOrigin::Staged(None) => {}
        }
    }
    // Existing project occurrences preserve their exact IDs; only staged entries cross byte registration.
    // 既有项目出现保留精确ID；只有暂存条目跨越字节注册。
    let existing_ids = origins
        .iter()
        .map(|origin| match origin {
            PromptAttachmentOrigin::Project { resource_id, .. } => Some(*resource_id),
            PromptAttachmentOrigin::Staged(_) => None,
        })
        .collect::<Vec<_>>();
    // One registry owner validates the entire mixed source sequence before assembling prompt parameters.
    // 同一注册表所有者在组装提示词参数之前校验整个混合来源序列。
    let resource_ids = register_project_attachment_occurrences_v1(
        controller,
        registry,
        project_id,
        attachments,
        &existing_ids,
    )?;
    if let Some(accepted) = accepted_input {
        if resource_ids != accepted.resource_ids {
            return Err(String::from(
                "Client input retry resource order differs from its originally accepted input.",
            ));
        }
        // Reset only the returned retry envelope; the original frozen event remains immutable.
        // 只重置返回的重试封装；原冻结事件保持不可变。
        let parameters = accepted.parameters.clone().map(|mut parameters| {
            parameters.media_delivery = vulcan_session_core::v1::UserPromptMediaDelivery::Pending;
            parameters
        });
        return Ok((resource_ids, parameters));
    }
    // Private receipts stay in the existing exact project partition.
    // 私有回执留在既有精确项目分区。
    let project = project_resource_directory_v1(
        &controller.data_root,
        SESSION_V1_STORAGE_PARTITION,
        project_id,
    )
    .map_err(|error| error.to_string())?;
    // Each occurrence keeps its own source classification and chronological position.
    // 每次出现保留自己的来源分类及时间顺序位置。
    let mut preparations = Vec::with_capacity(resource_ids.len());
    for ((resource_id, origin), attachment) in resource_ids.iter().zip(origins).zip(attachments) {
        match origin {
            PromptAttachmentOrigin::Staged(Some(origin)) => {
                if let vulcan_agent_runtime::attachment_asset_index::AttachmentDraftMediaDelivery::Upload { receipt: Some(receipt), .. } = &origin.delivery {
                    registry.record_provider_upload_receipt(&project, *resource_id, &origin.source_sha256, origin.source_byte_len, receipt.clone()).map_err(|error| error.to_string())?;
                }
                if matches!(&origin.delivery, vulcan_agent_runtime::attachment_asset_index::AttachmentDraftMediaDelivery::Upload { receipt: None, .. }) {
                    // The actual prompt owner records the destination before lease promotion; late completion keeps this exact intent.
                    // 实际提示词所有者在租约提升前登记目标；晚到完成保留本精确意图。
                    vulcan_agent_runtime::attachment_asset_index::register_attachment_draft_upload_handoff(
                        attachment, origin, &project, *resource_id, registry,
                    )?;
                }
                preparations.push(UserPromptResourcePreparation::Prepared(
                    origin.prompt_delivery(*resource_id)?,
                ));
            }
            PromptAttachmentOrigin::Staged(None) => {
                preparations.push(UserPromptResourcePreparation::Pending)
            }
            PromptAttachmentOrigin::Project { preparation, .. } => {
                preparations.push(preparation.as_ref().clone())
            }
        }
    }
    Ok((
        resource_ids.clone(),
        UserPromptParameters::for_resource_preparations(&resource_ids, preparations)
            .map_err(|error| error.to_string())?,
    ))
}

/// Creates, fully publishes, and only then registers one root-input V1 session.
/// 创建并完整发布一个带根输入的 V1 会话，随后才把它注册到内存。
#[cfg(test)]
pub(crate) async fn create_and_register_root_session_v1(
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    request: CreateRootSessionV1Request,
) -> Result<Arc<SessionHandle>, SessionV1LifecycleError> {
    create_and_register_root_session_v1_with_context_window(controller, registry, request, None)
        .await
}

/// Creates one root session with its planning window in the initial durable metadata revision.
/// 在初始持久元数据修订中写入规划窗口并创建根会话。
///
/// `controller` owns storage, `registry` owns admission, `request` supplies the root input,
/// and `node_id` is the already validated default for this new conversation.
/// `controller` 持有存储，`registry` 持有准入，`request` 提供根输入，`node_id` 是已校验的新会话默认值。
pub(crate) async fn create_and_register_root_session_v1_with_context_window(
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    request: CreateRootSessionV1Request,
    node_id: Option<vulcan_context::ContextCompactionWindowNodeId>,
) -> Result<Arc<SessionHandle>, SessionV1LifecycleError> {
    let CreateRootSessionV1Request {
        session_id,
        project_id,
        title,
        root_submission,
        now_ms,
    } = request;
    // reservation closes the same-session overwrite window before any document
    // or file publication begins and is automatically released on every error.
    // reservation 在任何文档或文件发布开始前关闭同会话覆盖窗口，并在任意错误时自动释放。
    let reservation = registry.reserve_creation(session_id)?;

    // document is the sole construction-time authority; the root input is
    // never replayed through a handle after publication.
    // document 是构建期唯一权威；发布后绝不再通过句柄重放根输入。
    let mut document = SessionDocument::new(
        session_id.to_string(),
        project_id.as_str(),
        title.as_str(),
        now_ms,
    );
    document.set_context_compaction_window_node_id(node_id, now_ms);
    let client_action_id = root_submission.client_action_id.clone();
    match document.submit_root_input(root_submission, now_ms)? {
        IdempotentMutation::Committed(_) => {}
        IdempotentMutation::Idempotent(_) => {
            return Err(SessionV1LifecycleError::UnexpectedIdempotentRootAction {
                session_id,
                client_action_id,
            });
        }
    }
    // Creation bypasses SessionHandle::submit_root_input, so publish its Running transition with the first input.
    // 创建绕过 SessionHandle::submit_root_input，因此必须将运行状态与首次输入共同发布。
    document.set_collaboration_run_control(
        vulcan_session_core::v1::collaboration::SetRunControlInput {
            operation_id:
                vulcan_session_core::v1::collaboration::CollaborationOperationId::try_new(
                    format!("root-input-running:{session_id}:{}", client_action_id.as_str()),
                )?,
            state: vulcan_session_core::v1::collaboration::CollaborationRunControl::Running,
            changed_at_ms: now_ms,
        },
    )?;
    // candidate contains the exact metadata, runtime, compactions, and active
    // tail belonging to the same memory revision.
    // candidate 包含属于同一内存修订号的精确 metadata、runtime、压缩节点和活动尾段。
    let candidate = document.prepare_flush_candidates()?.ok_or(
        SessionV1LifecycleError::MissingInitialFlushCandidate(session_id),
    )?;
    // date_bucket is derived from the supplied timestamp through the existing
    // service-kernel time dependency instead of accepting caller path text.
    // date_bucket 通过 service-kernel 现有 time 依赖从给定时间戳派生，而不接受调用方路径文本。
    let date_bucket = session_v1_date_bucket(now_ms)?;
    let request = CreateSessionV1Request {
        project_id: &project_id,
        storage_partition: SESSION_V1_STORAGE_PARTITION,
        date_bucket: &date_bucket,
        metadata: &candidate.metadata,
        runtime_lock: &candidate.runtime_lock,
        compactions: &candidate.compactions,
        events: &candidate.tail_events,
    };
    // created is returned only after all files and the strict SQLite locator
    // have both been published by the storage facade.
    // created 只会在全部文件与严格 SQLite 定位记录均已由存储门面发布后返回。
    let created = create_indexed_session_v1(controller, &request).await?;
    document.apply_published_storage_metadata(&created.metadata)?;
    document.advance_durable(candidate.revision)?;

    // handle becomes observable only after its document is fully published and
    // its exact revision is marked durable.
    // handle 仅在文档完整发布且精确修订号标记为持久化后才变得可见。
    let handle = SessionHandle::new(session_id, created.locator, document);
    handle.initialize_root_usage(true, now_ms);
    reservation
        .publish(handle)
        .map_err(SessionV1LifecycleError::from)
}

/// Creates one forked V1 session or returns the exact child from an identical retry.
/// 创建一个分叉 V1 会话，或在精确重试时返回同一子会话。
///
/// # Parameters / 参数
/// `controller` owns the SQLite locator and physical data root, `registry` owns process-local
/// handles, and `request` carries the exact parent generation boundary and stable child identity.
/// `controller` 持有 SQLite 定位器与物理数据根，`registry` 持有进程内句柄，`request` 携带精确父
/// generation 边界与稳定子会话身份。
///
/// # Returns / 返回值
/// Returns only after the child files, SQLite row, strict reload, and registry publication agree.
/// Exact retries return the already-published child after immutable origin validation.
/// 仅在子文件、SQLite 行、严格重载与注册表发布一致后返回；精确重试在校验不可变来源后返回已发布子会话。
pub(crate) async fn create_or_get_forked_session_v1(
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    request: CreateForkSessionV1Request,
) -> Result<CreatedForkSessionV1, SessionV1LifecycleError> {
    if request.source_session_id == request.child_session_id {
        return Err(SessionV1LifecycleError::ForkRetryConflict {
            child_session_id: request.child_session_id,
            field: "source_session_id_matches_child",
        });
    }
    let _child_lifecycle = registry
        .acquire_lifecycle_transition(request.child_session_id)
        .await?;
    let source_handle =
        get_or_load_session_v1(controller, registry, request.source_session_id).await?;
    let source_locator = source_handle.storage_locator()?;
    let expected_origin = ForkOrigin {
        source_session_id: request.source_session_id.to_string(),
        source_segment_generation: request.expected_source_generation,
        source_target_seq: request.source_turn_end_seq,
    };

    if let Some(existing_locator) =
        locate_optional_session_v1_storage(controller, &request.child_session_id.to_string())
            .await?
    {
        let existing =
            get_or_load_session_v1(controller, registry, request.child_session_id).await?;
        validate_existing_fork_retry(
            &existing,
            &existing_locator,
            source_locator.project_id(),
            &request.title,
            &expected_origin,
        )?;
        return Ok(CreatedForkSessionV1 {
            handle: existing,
            created: false,
            source_turn_end_seq: request.source_turn_end_seq,
        });
    }

    let reservation = registry.reserve_creation(request.child_session_id)?;
    let output = source_handle.build_fork_generation(
        request.expected_source_generation,
        ForkGenerationInput {
            child_session_id: request.child_session_id.to_string(),
            title: request.title.clone(),
            source_turn_end_seq: request.source_turn_end_seq,
            now_ms: request.now_ms,
        },
    )?;
    if output.metadata.fork_origin.as_ref() != Some(&expected_origin) {
        return Err(SessionV1LifecycleError::ForkRetryConflict {
            child_session_id: request.child_session_id,
            field: "core_fork_origin",
        });
    }
    let date_bucket = session_v1_date_bucket(request.now_ms)?;
    let storage_request = CreateSessionV1Request {
        project_id: source_locator.project_id(),
        storage_partition: SESSION_V1_STORAGE_PARTITION,
        date_bucket: &date_bucket,
        metadata: &output.metadata,
        runtime_lock: &output.runtime_lock,
        compactions: &output.compactions,
        events: &output.events,
    };
    let created = create_indexed_session_v1(controller, &storage_request).await?;
    let loaded = load_session_v1_full(&created.locator)?;
    let handle = SessionHandle::new(request.child_session_id, loaded.locator, loaded.document);
    handle.initialize_root_usage(true, request.now_ms);
    validate_existing_fork_retry(
        &handle,
        &created.locator,
        source_locator.project_id(),
        &request.title,
        &expected_origin,
    )?;
    let handle = reservation.publish(handle)?;
    Ok(CreatedForkSessionV1 {
        handle,
        created: true,
        source_turn_end_seq: request.source_turn_end_seq,
    })
}

/// Validates that an indexed child is the exact immutable result of one fork request.
/// 校验一个已索引子会话正是某条分叉请求的精确不可变结果。
fn validate_existing_fork_retry(
    handle: &SessionHandle,
    locator: &vulcan_workspace_store::workspace_store::SessionV1StorageLocator,
    expected_project_id: &str,
    expected_title: &str,
    expected_origin: &ForkOrigin,
) -> Result<(), SessionV1LifecycleError> {
    let child_session_id = handle.session_id();
    if locator.project_id() != expected_project_id {
        return Err(SessionV1LifecycleError::ForkRetryConflict {
            child_session_id,
            field: "project_id",
        });
    }
    let (actual_title, actual_origin) = handle.read(|document| {
        (
            document.metadata.title.clone(),
            document.metadata.fork_origin.clone(),
        )
    })?;
    if actual_title != expected_title {
        return Err(SessionV1LifecycleError::ForkRetryConflict {
            child_session_id,
            field: "title",
        });
    }
    if actual_origin.as_ref() != Some(expected_origin) {
        return Err(SessionV1LifecycleError::ForkRetryConflict {
            child_session_id,
            field: "fork_origin",
        });
    }
    Ok(())
}

/// Closes a superseded open child Turn before one queued or interrupted execution starts a new root input.
/// 在排队或插队执行启动新的根输入前，关闭已被取代但仍开放的子会话回合。
///
/// # Parameters / 参数
///
/// - `handle`: Exact embedded child-session owner loaded from the root-derived `TASK` path.
///   `handle`：从根会话派生的 `TASK` 路径加载的精确内嵌子会话所有者。
/// - `submission`: New execution root input whose client action identity provides replay protection.
///   `submission`：以客户端动作身份提供重放保护的新执行根输入。
/// - `now_ms`: Authoritative admission time used for the stopped boundary and successor input.
///   `now_ms`：用于停止边界和后继输入的权威准入时间。
///
/// # Returns / 返回值
///
/// Returns success after an exact replay is confirmed or after the old Turn is stopped and the new
/// root input is durably admitted, including the interrupted physical API of the superseded execution.
/// 在确认精确重放，或被替代执行的物理 API 中断、旧回合停止且新根输入可靠准入后返回成功。
fn submit_resumed_embedded_task_root_input(
    handle: &SessionHandle,
    submission: ClientInputSubmission,
    now_ms: i64,
) -> Result<(), SessionV1LifecycleError> {
    handle.mutate_durable_if_changed(
        SessionMutationAdmission::RootTurn,
        SessionFlushReason::InputAccepted,
        move |document| match document.submit_resumed_child_root_input(submission, now_ms)? {
            IdempotentMutation::Committed(_) => Ok(SessionMutationChange::Changed(())),
            IdempotentMutation::Idempotent(_) => Ok(SessionMutationChange::Unchanged(())),
        },
    )?;
    Ok(())
}

/// Creates or exactly reloads one child TaskSession V1 owner below the root session `TASK` tree.
/// 在根会话 `TASK` 目录树下创建或精确重载一个子 TaskSession V1 owner。
///
/// The first execution publishes one complete non-indexed child directory before registry
/// visibility. A resumed execution loads the deterministic child locator, projects any persisted
/// interruption, and durably submits its exact execution prompt as a new root Turn. No SQLite row,
/// TaskRuntime snapshot, directory scan, or lifecycle payload parsing participates.
/// 首次执行会在注册表可见前完整发布一个不进入索引的子目录。续跑执行从确定性子定位器加载，
/// 投影任何已持久化中断，并把该执行的精确提示词作为新根 Turn 可靠提交。整个过程不使用
/// SQLite 记录、TaskRuntime 快照、目录扫描或生命周期载荷解析。
///
/// # Parameters / 参数
///
/// - `registry`: Sole process-local owner of root and child session handles.
///   `registry`：根会话与子会话句柄的唯一进程内 owner。
/// - `facts`: Immutable execution facts retained before `DelegateStarted`.
///   `facts`：在 `DelegateStarted` 前保留的不可变执行事实。
///
/// # Returns / 返回值
///
/// The unique published child handle after its execution input is durable.
/// 本次执行输入可靠落盘后的唯一已发布子句柄。
pub(crate) fn create_or_load_embedded_task_session_v1(
    registry: &SessionRuntimeRegistry,
    facts: TaskV1ExecutionFacts,
) -> Result<Arc<SessionHandle>, SessionV1LifecycleError> {
    let now_ms = i64::try_from(facts.admitted_at_ms).map_err(|_| {
        SessionV1LifecycleError::InvalidTaskAdmissionTimestamp {
            admitted_at_ms: facts.admitted_at_ms,
        }
    })?;
    let root_handle = registry.get(facts.root_session_id)?;
    let parent_locator = root_handle.storage_locator()?;
    let child_locator =
        locate_embedded_task_session_v1(&parent_locator, &facts.task_id.to_string())?;
    let project_id = parent_locator.project_id().to_string();
    let client_action_id = ClientActionId::try_new(facts.execution_id.clone())?;
    let child_session_id = facts.task_id;
    let child_title = facts.display_title.clone();
    let child_prompt = facts.prompt.clone();
    let is_resumed = facts.is_resumed;
    // Child usage shares the root file even when this child delegates further work.
    // 即使该子会话继续委派工作，其用量也共享根文件。
    let usage_parent_id = facts.parent_session_id;
    let initial_action_id = client_action_id.clone();
    let initial_prompt = child_prompt.clone();
    let handle = registry.get_or_load(child_session_id, move || {
        if is_resumed {
            let loaded = load_session_v1_tail(&child_locator)?;
            if loaded.metadata.session_id != child_session_id.to_string() {
                return Err(SessionRuntimeError::LoadedSessionDocumentIdentityMismatch {
                    expected: child_session_id,
                    actual: loaded.metadata.session_id,
                });
            }
            let mut document = loaded.document;
            let projected = document.project_persisted_interruptions()?;
            let handle = SessionHandle::new(child_session_id, loaded.locator, document);
            if projected > 0 {
                handle.flush_all()?;
            }
            return Ok(handle);
        }

        let mut document = SessionDocument::new(
            child_session_id.to_string(),
            project_id.as_str(),
            child_title.as_str(),
            now_ms,
        );
        match document.submit_root_input(
            ClientInputSubmission::new(initial_action_id.clone(), initial_prompt.clone(), vec![]),
            now_ms,
        )? {
            IdempotentMutation::Committed(_) => {}
            IdempotentMutation::Idempotent(_) => {
                return Err(SessionRuntimeError::Domain(Box::new(
                    DomainError::ImmutableFieldMutation {
                        entity: "embedded_task_session",
                        field: "unexpected_idempotent_initial_input",
                    },
                )));
            }
        }
        // Publish Running with the first root input because embedded creation bypasses handle submission.
        // 嵌入创建绕过句柄提交，因此必须将 Running 与首个根输入一起发布。
        document.set_collaboration_run_control(
            vulcan_session_core::v1::collaboration::SetRunControlInput {
                operation_id:
                    vulcan_session_core::v1::collaboration::CollaborationOperationId::try_new(
                        format!(
                            "root-input-running:{child_session_id}:{}",
                            initial_action_id.as_str(),
                        ),
                    )?,
                state: vulcan_session_core::v1::collaboration::CollaborationRunControl::Running,
                changed_at_ms: now_ms,
            },
        )?;
        let candidate = document
            .prepare_flush_candidates()?
            .ok_or(SessionRuntimeError::MutationProducedNoRevision)?;
        let created = create_embedded_task_session_v1(
            &parent_locator,
            &CreateEmbeddedTaskSessionV1Request {
                metadata: &candidate.metadata,
                runtime_lock: &candidate.runtime_lock,
                compactions: &candidate.compactions,
                events: &candidate.tail_events,
            },
        )?;
        document.apply_published_storage_metadata(&created.metadata)?;
        document.advance_durable(candidate.revision)?;
        Ok(SessionHandle::new(
            child_session_id,
            created.locator,
            document,
        ))
    })?;

    handle.bind_child_usage(root_handle.as_ref(), usage_parent_id, now_ms);
    if facts.is_resumed {
        submit_resumed_embedded_task_root_input(
            handle.as_ref(),
            ClientInputSubmission::new(client_action_id, child_prompt, vec![]),
            now_ms,
        )?;
    }
    Ok(handle)
}

/// Resolves SQLite first, then loads or reuses the unique process-local handle.
/// 先通过 SQLite 定位，再加载或复用进程内唯一句柄。
pub(crate) async fn get_or_load_session_v1(
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    session_id: SessionId,
) -> Result<Arc<SessionHandle>, SessionV1LifecycleError> {
    // locator is always resolved before consulting the registry, making SQLite
    // the only accepted directory authority even for an already-loaded id.
    // locator 始终先于注册表解析，使 SQLite 即使对已加载 ID 也是唯一目录权威。
    let locator = locate_session_v1_storage(controller, &session_id.to_string()).await?;
    registry
        .get_or_load(session_id, move || {
            let loaded =
                vulcan_workspace_store::workspace_store::load_session_v1_for_runtime(&locator)?;
            let actual_session_id = loaded.metadata.session_id.clone();
            if actual_session_id != session_id.to_string() {
                return Err(SessionRuntimeError::LoadedSessionDocumentIdentityMismatch {
                    expected: session_id,
                    actual: actual_session_id,
                });
            }

            // document may contain one persisted in-flight API after restart;
            // its interrupted projection must become durable before publication.
            // document 在重启后可能包含一个已持久化的在途 API；其中断投影必须在发布前落盘。
            let mut document = loaded.document;
            // A corrected legacy tail must be durable even when no running API needs interruption.
            // 即使没有运行中的 API 需要中断，纠正后的旧尾段也必须可靠落盘。
            let recovered_goal_owners = document.memory_revision() != document.durable_revision();
            let projected = document.project_persisted_interruptions()?;
            let handle = SessionHandle::new(session_id, loaded.locator, document);
            if projected > 0 || recovered_goal_owners {
                handle.flush_all()?;
            }
            if recovered_goal_owners {
                eprintln!(
                    "[webui-service] recovered persisted Goal event ownership: session={session_id}"
                );
            }
            handle.initialize_root_usage(false, 0);
            Ok(handle)
        })
        .map_err(SessionV1LifecycleError::from)
}

/// Renames one V1 session through durable metadata followed by exact SQLite projection.
/// 先可靠修改 metadata，再精确投影 SQLite，以此重命名一个 V1 会话。
///
/// # Parameters / 参数
///
/// - `controller`: Controller runtime owning the exact locator row.
///   `controller`：持有精确定位记录的 Controller 运行时。
/// - `registry`: Unique process-local owner and lifecycle serializer.
///   `registry`：唯一进程内 owner 与生命周期串行器。
/// - `session_id`, `title`, and `now_ms`: Exact domain input for the rename.
///   `session_id`、`title` 与 `now_ms`：重命名的精确领域输入。
///
/// # Returns / 返回值
///
/// Success only after both durable metadata and its exact list projection agree.
/// 仅当可靠 metadata 与其精确列表投影一致后返回成功。
pub(crate) async fn rename_session_lifecycle_v1(
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    session_id: SessionId,
    title: String,
    now_ms: i64,
) -> Result<(), SessionV1LifecycleError> {
    let _lifecycle = registry.acquire_lifecycle_transition(session_id).await?;
    let handle = get_or_load_session_v1(controller, registry, session_id).await?;
    let locator = handle.storage_locator()?;
    let receipt = handle.mutate_durable_if_changed(
        SessionMutationAdmission::MetadataOnly,
        SessionFlushReason::MetadataChanged,
        move |document| {
            let changed = document.rename_session(title, now_ms)?;
            let metadata = document.metadata.clone();
            Ok(if changed {
                SessionMutationChange::Changed(metadata)
            } else {
                SessionMutationChange::Unchanged(metadata)
            })
        },
    )?;
    project_lifecycle_metadata_or_report(controller, session_id, "rename", &locator, &receipt.value)
        .await
}

/// Archives one V1 session without allowing SQLite to lead the metadata authority.
/// 在不允许 SQLite 领先 metadata 权威的前提下归档一个 V1 会话。
///
/// # Parameters / 参数
///
/// - `controller`, `registry`, and `session_id`: Exact storage and ownership identity.
///   `controller`、`registry` 与 `session_id`：精确存储及所有权身份。
/// - `now_ms`: Archive timestamp used only when the session is not already archived.
///   `now_ms`：仅在会话尚未归档时使用的归档时间戳。
///
/// # Returns / 返回值
///
/// Success after metadata and SQLite agree; the active registry owner is then detached.
/// metadata 与 SQLite 一致后返回成功，随后移除活动注册表 owner。
pub(crate) async fn archive_session_lifecycle_v1(
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    session_id: SessionId,
    now_ms: i64,
) -> Result<(), SessionV1LifecycleError> {
    let _lifecycle = registry.acquire_lifecycle_transition(session_id).await?;
    let handle = get_or_load_session_v1(controller, registry, session_id).await?;
    let locator = handle.storage_locator()?;
    let receipt = handle.mutate_durable_if_changed(
        SessionMutationAdmission::MetadataOnly,
        SessionFlushReason::MetadataChanged,
        move |document| {
            let archived_at_ms = document.metadata.archived_at_ms.unwrap_or(now_ms);
            let changed = document.set_archived_at(Some(archived_at_ms), now_ms)?;
            let metadata = document.metadata.clone();
            Ok(if changed {
                SessionMutationChange::Changed(metadata)
            } else {
                SessionMutationChange::Unchanged(metadata)
            })
        },
    )?;
    project_lifecycle_metadata_or_report(
        controller,
        session_id,
        "archive",
        &locator,
        &receipt.value,
    )
    .await?;
    registry.unregister_lifecycle_session(session_id)?;
    Ok(())
}

/// Parses one project-scoped V1 row set into exact application session identities.
/// 灏嗕竴缁勯」鐩綔鐢ㄥ煙 V1 琛岃В鏋愪负绮剧‘鐨勫簲鐢ㄤ細璇濊韩浠姐€?
///
/// # Parameters / 鍙傛暟
///
/// - `project_id`: Exact project identity used by the SQLite query.
///   `project_id`锛?SQLite 鏌ヨ浣跨敤鐨勭簿纭」鐩韩浠姐€?
/// - `rows`: Row-only V1 session list returned by the authoritative index.
///   `rows`锛氭潈濞佺储寮曡繑鍥炵殑绾?V1 浼氳瘽鍒楄〃銆?
///
/// # Returns / 杩斿洖鍊?
///
/// Returns ordered UUID identities, or an explicit row-contract failure.
/// 杩斿洖鏈夊簭 UUID 韬唤锛屾垨鏄惧紡鐨勮濂戠害澶辫触銆?
fn parse_project_session_ids_v1(
    project_id: &str,
    rows: Vec<SessionV1ListEntry>,
) -> Result<Vec<SessionId>, SessionV1LifecycleError> {
    rows.into_iter()
        .map(|row| {
            if row.project_id != project_id {
                return Err(SessionV1LifecycleError::InvalidIndexedSessionId {
                    project_id: project_id.to_string(),
                    session_id: row.session_id,
                    detail: format!(
                        "row project id `{}` does not match the requested project",
                        row.project_id
                    ),
                });
            }
            row.session_id.parse::<SessionId>().map_err(|error| {
                SessionV1LifecycleError::InvalidIndexedSessionId {
                    project_id: project_id.to_string(),
                    session_id: row.session_id,
                    detail: error.to_string(),
                }
            })
        })
        .collect()
}

/// Archives every exact active V1 session row for one project through the metadata-first protocol.
/// 閫氳繃 metadata 浼樺厛鍗忚褰掓。涓€涓」鐩殑姣忔潯绮剧‘娲诲姩 V1 浼氳瘽璁板綍銆?
///
/// # Parameters / 鍙傛暟
///
/// - `controller`: Authoritative SQLite and storage runtime.
///   `controller`锛氭潈濞?SQLite 涓庡瓨鍌ㄨ繍琛屾椂銆?
/// - `registry`: Unique in-process mutable session owner registry.
///   `registry`锛氳繘绋嬪唴鍞竴鍙彉浼氳瘽 owner 娉ㄥ唽琛ㄣ€?
/// - `project_id`: Exact project whose active V1 rows must transition.
///   `project_id`锛氬繀椤昏浆鎹㈠叾娲诲姩 V1 琛岀殑绮剧‘椤圭洰銆?
/// - `now_ms`: One fixed timestamp shared by the complete batch.
///   `now_ms`锛氭暣涓壒娆″叡浜殑鍞竴鍥哄畾鏃堕棿鎴炽€?
///
/// # Returns / 杩斿洖鍊?
///
/// Returns the complete authoritative archived session set after the transition.
/// 杩斿洖杞崲鍚庡畬鏁寸殑鏉冨▉褰掓。浼氳瘽闆嗗悎銆?
pub(crate) async fn archive_project_sessions_lifecycle_v1(
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    project_id: &str,
    now_ms: i64,
) -> Result<Vec<SessionId>, SessionV1LifecycleError> {
    let active_session_ids = parse_project_session_ids_v1(
        project_id,
        list_active_sessions_v1(controller, project_id).await?,
    )?;
    for session_id in active_session_ids {
        archive_session_lifecycle_v1(controller, registry, session_id, now_ms).await?;
    }

    let remaining_rows = list_active_sessions_v1(controller, project_id).await?;
    if !remaining_rows.is_empty() {
        return Err(SessionV1LifecycleError::ProjectLifecycleIncomplete {
            project_id: project_id.to_string(),
            operation: "archive",
            remaining_session_ids: remaining_rows
                .into_iter()
                .map(|row| row.session_id)
                .collect(),
        });
    }
    parse_project_session_ids_v1(
        project_id,
        list_archived_sessions_v1(controller, project_id).await?,
    )
}

/// Restores every exact archived V1 session row for one project through the metadata-first protocol.
/// 閫氳繃 metadata 浼樺厛鍗忚鎭㈠涓€涓」鐩殑姣忔潯绮剧‘褰掓。 V1 浼氳瘽璁板綍銆?
///
/// # Parameters / 鍙傛暟
///
/// - `controller`: Authoritative SQLite and storage runtime.
///   `controller`锛氭潈濞?SQLite 涓庡瓨鍌ㄨ繍琛屾椂銆?
/// - `registry`: Unique in-process mutable session owner registry.
///   `registry`锛氳繘绋嬪唴鍞竴鍙彉浼氳瘽 owner 娉ㄥ唽琛ㄣ€?
/// - `project_id`: Exact project whose archived V1 rows must transition.
///   `project_id`锛氬繀椤昏浆鎹㈠叾褰掓。 V1 琛岀殑绮剧‘椤圭洰銆?
/// - `now_ms`: One fixed timestamp shared by the complete batch.
///   `now_ms`锛氭暣涓壒娆″叡浜殑鍞竴鍥哄畾鏃堕棿鎴炽€?
///
/// # Returns / 杩斿洖鍊?
///
/// Returns the complete authoritative active session set after the transition.
/// 杩斿洖杞崲鍚庡畬鏁寸殑鏉冨▉娲诲姩浼氳瘽闆嗗悎銆?
pub(crate) async fn restore_project_sessions_lifecycle_v1(
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    project_id: &str,
    now_ms: i64,
) -> Result<Vec<SessionId>, SessionV1LifecycleError> {
    let archived_session_ids = parse_project_session_ids_v1(
        project_id,
        list_archived_sessions_v1(controller, project_id).await?,
    )?;
    for session_id in archived_session_ids {
        restore_session_lifecycle_v1(controller, registry, session_id, now_ms).await?;
    }

    let remaining_rows = list_archived_sessions_v1(controller, project_id).await?;
    if !remaining_rows.is_empty() {
        return Err(SessionV1LifecycleError::ProjectLifecycleIncomplete {
            project_id: project_id.to_string(),
            operation: "restore",
            remaining_session_ids: remaining_rows
                .into_iter()
                .map(|row| row.session_id)
                .collect(),
        });
    }
    parse_project_session_ids_v1(
        project_id,
        list_active_sessions_v1(controller, project_id).await?,
    )
}

/// Restores one V1 session through the same metadata-first lifecycle protocol.
/// 通过同一 metadata 优先生命周期协议恢复一个 V1 会话。
///
/// # Parameters / 参数
///
/// - `controller`, `registry`, and `session_id`: Exact storage and ownership identity.
///   `controller`、`registry` 与 `session_id`：精确存储及所有权身份。
/// - `now_ms`: Restore mutation timestamp.
///   `now_ms`：恢复修改时间戳。
///
/// # Returns / 返回值
///
/// Success only after the durable metadata archive marker is clear and SQLite mirrors it.
/// 仅在可靠 metadata 的归档标记已清除且 SQLite 完成镜像后返回成功。
pub(crate) async fn restore_session_lifecycle_v1(
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    session_id: SessionId,
    now_ms: i64,
) -> Result<(), SessionV1LifecycleError> {
    let _lifecycle = registry.acquire_lifecycle_transition(session_id).await?;
    match preflight_session_v1_storage(controller, &session_id.to_string()).await? {
        SessionV1StoragePreflightOutcome::Ready { .. } => {}
        SessionV1StoragePreflightOutcome::MissingStorageIndexRemoved { .. } => {
            let _ = registry.unregister_lifecycle_session(session_id)?;
            return Err(SessionV1LifecycleError::RestoreStorageMissing { session_id });
        }
    }
    let handle = get_or_load_session_v1(controller, registry, session_id).await?;
    let locator = handle.storage_locator()?;
    let receipt = handle.mutate_durable_if_changed(
        SessionMutationAdmission::MetadataOnly,
        SessionFlushReason::MetadataChanged,
        move |document| {
            let changed = document.set_archived_at(None, now_ms)?;
            let metadata = document.metadata.clone();
            Ok(if changed {
                SessionMutationChange::Changed(metadata)
            } else {
                SessionMutationChange::Unchanged(metadata)
            })
        },
    )?;
    project_lifecycle_metadata_or_report(
        controller,
        session_id,
        "restore",
        &locator,
        &receipt.value,
    )
    .await
}

/// Checks and seals the full embedded tree before deleting the V1 root and its SQLite locator row.
/// 检查并封存完整嵌入树后，删除 V1 根及其 SQLite 定位记录。
///
/// # Parameters / 参数
///
/// - `controller`, `registry`, and `session_id`: Exact lifecycle ownership inputs.
///   `controller`、`registry` 与 `session_id`：精确生命周期所有权输入。
///
/// # Returns / 返回值
///
/// Success after the exact directory is absent and the locator row is deleted. Project resources
/// are never touched.
/// 精确目录不存在且定位记录删除后返回成功；项目资源绝不会被触碰。
pub(crate) async fn delete_session_lifecycle_v1(
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    session_id: SessionId,
) -> Result<(), SessionV1LifecycleError> {
    deletion::delete_tree(controller, registry, session_id).await
}

/// Projects one durable metadata snapshot and preserves projection lag as a structured error.
/// 投影一份可靠 metadata 快照，并把投影滞后保留为结构化错误。
async fn project_lifecycle_metadata_or_report(
    controller: &HostRuntimeHandle,
    session_id: SessionId,
    operation: &'static str,
    locator: &vulcan_workspace_store::workspace_store::SessionV1StorageLocator,
    metadata: &MetadataDocument,
) -> Result<(), SessionV1LifecycleError> {
    project_session_lifecycle_v1(controller, locator, metadata)
        .await
        .map_err(|source| SessionV1LifecycleError::IndexProjectionFailed {
            session_id,
            operation,
            source,
        })
}

/// Returns the already-registered V1 handle without any legacy fallback.
/// 返回已注册的 V1 句柄，不执行任何旧链路回退。
#[cfg(test)]
pub(crate) fn require_registered_session_v1(
    registry: &SessionRuntimeRegistry,
    session_id: SessionId,
) -> Result<Arc<SessionHandle>, SessionRuntimeError> {
    registry.get(session_id)
}

/// Converts one Unix millisecond timestamp into the immutable UTC date bucket.
/// 把一个 Unix 毫秒时间戳转换为不可变 UTC 日期桶。
fn session_v1_date_bucket(now_ms: i64) -> Result<String, SessionV1LifecycleError> {
    let timestamp = OffsetDateTime::from_unix_timestamp_nanos(i128::from(now_ms) * 1_000_000)
        .map_err(|error| SessionV1LifecycleError::InvalidDateBucket {
            now_ms,
            detail: error.to_string(),
        })?;
    timestamp
        .format(format_description!("[year]-[month]-[day]"))
        .map_err(|error| SessionV1LifecycleError::InvalidDateBucket {
            now_ms,
            detail: error.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use uuid::Uuid;
    use vulcan_core::types::ConversationAttachmentKind;
    use vulcan_session_core::v1::context::{LockedBaselineProjection, RuntimeInitializationState};
    use vulcan_session_core::v1::document::PrepareAndStartApiInput;
    use vulcan_session_core::v1::event::{
        ApiCallStatus, ApiPurposeKind, ApiRequestSnapshot, AssistantSemanticStatus, EventBody,
        ProviderProjectionProof, TurnOutcome,
    };
    use vulcan_session_core::v1::ids::Seq;
    use vulcan_session_core::v1::projection::provider_visible_fingerprint;
    use vulcan_workspace_store::data_plane::HostRuntimeMetadata;
    use vulcan_workspace_store::workspace_store::{
        ProjectResourceCatalogRegistry, list_active_sessions_v1, list_archived_sessions_v1,
        load_project_resource_catalog, project_resource_directory_v1,
    };

    use super::*;

    mod collaboration_deletion;
    mod draft_handoff;
    mod model_projection;
    mod root_turn_completion;

    /// Isolated storage fixture used by lifecycle integration tests.
    /// 生命周期集成测试使用的隔离存储夹具。
    struct LifecycleStorageFixture {
        /// Keeps the isolated data root alive for the entire test.
        /// 在整个测试期间保持隔离数据根存活。
        _temporary: tempfile::TempDir,
        /// Production-shaped controller runtime handle under test.
        /// 被测的生产形态控制器运行时句柄。
        controller: HostRuntimeHandle,
    }
    impl LifecycleStorageFixture {
        /// Creates one isolated data root with its APP SQLite binding.
        /// 创建一个隔离数据根及其 APP SQLite 绑定。
        async fn start() -> Self {
            // temporary owns every file written by this fixture.
            // temporary 拥有该夹具写入的全部文件。
            let temporary = tempfile::tempdir().expect("temporary lifecycle controller root");
            let data_root = temporary.path().join("data");
            let sqlite_path = data_root.join("sqlite/app-workspace.sqlite3");
            std::fs::create_dir_all(sqlite_path.parent().expect("SQLite parent"))
                .expect("create SQLite parent");
            let binding_id = format!("session-v1-lifecycle-{}", Uuid::new_v4());
            let controller = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
                data_root,
                sqlite_path: sqlite_path.clone(),
                sqlite_binding_id: binding_id.clone(),
                system_statistics_sqlite_path: sqlite_path,
                system_statistics_sqlite_binding_id: binding_id.clone(),
            });
            Self {
                _temporary: temporary,
                controller,
            }
        }

        /// Releases the exclusive database file lock after one test.
        /// 在一次测试后释放独占数据库文件锁。
        async fn shutdown(self) {
            self.controller.local.shutdown();
        }
    }

    /// Creates one fully published root session with deterministic facts.
    /// 使用确定性事实创建一个完整发布的根会话。
    async fn create_fixture_session(
        fixture: &LifecycleStorageFixture,
        registry: &SessionRuntimeRegistry,
        session_id: SessionId,
    ) -> Arc<SessionHandle> {
        create_and_register_root_session_v1(
            &fixture.controller,
            registry,
            CreateRootSessionV1Request {
                session_id,
                project_id: String::from("project-explicit"),
                title: String::from("Lifecycle title"),
                root_submission: ClientInputSubmission::new(
                    ClientActionId::try_new(format!("root-{session_id}"))
                        .expect("root client action id"),
                    "root input",
                    vec![ResourceId::try_new(1).expect("resource id")],
                ),
                now_ms: 1_787_788_800_000,
            },
        )
        .await
        .expect("create and register root V1 session")
    }

    /// Closes and durably flushes the current fixture Turn, returning its terminal TurnStats seq.
    /// 关闭并可靠刷新当前夹具回合，返回其终态 TurnStats seq。
    fn close_fixture_turn(handle: &SessionHandle, now_ms: i64) -> Seq {
        handle
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::TurnCompleted,
                |document| document.close_turn(TurnOutcome::Completed, 1, now_ms),
            )
            .expect("close and flush fixture Turn")
            .value
    }

    /// Fork creation publishes one exact child and makes response-loss retries idempotent.
    /// 分叉创建只发布一个精确子会话，并使响应丢失后的重试保持幂等。
    #[tokio::test]
    async fn fork_creation_and_exact_retry_preserve_one_immutable_origin() {
        let fixture = LifecycleStorageFixture::start().await;
        let registry = SessionRuntimeRegistry::default();
        let source_session_id = SessionId::from_u128(0x72_01);
        let child_session_id = SessionId::from_u128(0x72_02);
        let source = create_fixture_session(&fixture, &registry, source_session_id).await;
        let source_turn_end_seq = close_fixture_turn(&source, 1_787_788_800_100);
        let source_generation = source
            .read(SessionDocument::segment_generation)
            .expect("read source generation");

        let first = create_or_get_forked_session_v1(
            &fixture.controller,
            &registry,
            CreateForkSessionV1Request {
                source_session_id,
                expected_source_generation: source_generation,
                child_session_id,
                title: String::from("Forked title"),
                source_turn_end_seq,
                now_ms: 1_787_788_800_500,
            },
        )
        .await
        .expect("create exact fork");
        assert!(first.created);
        assert_eq!(first.source_turn_end_seq, source_turn_end_seq);
        let expected_origin = ForkOrigin {
            source_session_id: source_session_id.to_string(),
            source_segment_generation: source_generation,
            source_target_seq: source_turn_end_seq,
        };
        assert_eq!(
            first
                .handle
                .read(|document| document.metadata.fork_origin.clone())
                .expect("read child origin"),
            Some(expected_origin.clone())
        );
        assert!(
            first
                .handle
                .read(|document| {
                    document.active_turn().is_none()
                        && document.metadata.open_turn_start_seq.is_none()
                        && !document.events().iter().any(|event| {
                            event.seq.value() > source_turn_end_seq.value()
                                && matches!(&event.event, EventBody::ApiStarted(_))
                        })
                })
                .expect("read dormant child state"),
            "fork publication must not open or dispatch a successor Turn"
        );

        let retried = create_or_get_forked_session_v1(
            &fixture.controller,
            &registry,
            CreateForkSessionV1Request {
                source_session_id,
                expected_source_generation: source_generation,
                child_session_id,
                title: String::from("Forked title"),
                source_turn_end_seq,
                now_ms: 1_787_788_800_900,
            },
        )
        .await
        .expect("retry exact fork");
        assert!(!retried.created);
        assert!(Arc::ptr_eq(&first.handle, &retried.handle));
        assert!(matches!(
            create_or_get_forked_session_v1(
                &fixture.controller,
                &registry,
                CreateForkSessionV1Request {
                    source_session_id,
                    expected_source_generation: source_generation,
                    child_session_id,
                    title: String::from("Conflicting title"),
                    source_turn_end_seq,
                    now_ms: 1_787_788_801_000,
                },
            )
            .await,
            Err(SessionV1LifecycleError::ForkRetryConflict {
                child_session_id: actual,
                field: "title",
            }) if actual == child_session_id
        ));
        assert_eq!(
            list_active_sessions_v1(&fixture.controller, "project-explicit")
                .await
                .expect("list source and child")
                .len(),
            2
        );
        fixture.shutdown().await;
    }

    /// A stale parent generation cannot publish any child row or directory owner.
    /// 陈旧父 generation 不能发布任何子会话索引或目录 owner。
    #[tokio::test]
    async fn fork_creation_rejects_stale_source_generation_before_child_publication() {
        let fixture = LifecycleStorageFixture::start().await;
        let registry = SessionRuntimeRegistry::default();
        let source_session_id = SessionId::from_u128(0x72_11);
        let child_session_id = SessionId::from_u128(0x72_12);
        let source = create_fixture_session(&fixture, &registry, source_session_id).await;
        let source_turn_end_seq = close_fixture_turn(&source, 1_787_788_801_050);
        let source_generation = source
            .read(SessionDocument::segment_generation)
            .expect("read source generation");

        assert!(matches!(
            create_or_get_forked_session_v1(
                &fixture.controller,
                &registry,
                CreateForkSessionV1Request {
                    source_session_id,
                    expected_source_generation: source_generation + 1,
                    child_session_id,
                    title: String::from("Rejected fork"),
                    source_turn_end_seq,
                    now_ms: 1_787_788_801_100,
                },
            )
            .await,
            Err(SessionV1LifecycleError::Runtime(
                SessionRuntimeError::GenerationSourceChanged { .. }
            ))
        ));
        assert!(
            locate_optional_session_v1_storage(&fixture.controller, &child_session_id.to_string())
                .await
                .expect("query missing rejected child")
                .is_none()
        );
        fixture.shutdown().await;
    }

    /// Builds one exact Provider preparation from the authoritative test document.
    /// 从权威测试文档构建一份精确 Provider 准备输入。
    fn lifecycle_test_api_input(
        document: &SessionDocument,
        request: ApiRequestSnapshot,
        now_ms: i64,
    ) -> PrepareAndStartApiInput {
        let through_seq = document
            .events()
            .last()
            .map(|event| event.seq)
            .expect("lifecycle API preparation requires a root event");
        PrepareAndStartApiInput {
            purpose: ApiPurposeKind::MainGeneration,
            request,
            request_budget_evidence:
                crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
            projection: ProviderProjectionProof {
                segment_generation: document.segment_generation(),
                document_revision: document.memory_revision(),
                retained_from_seq: Seq::try_new(1).expect("valid retained seq"),
                through_seq,
                fingerprint_sha256: provider_visible_fingerprint(document.events(), through_seq)
                    .expect("valid provider-visible fingerprint"),
            },
            locked_baseline: LockedBaselineProjection::empty(),
            continuation_action_id: None,
            now_ms,
        }
    }

    /// Builds one authoritative attachment fixture backed by exact managed bytes.
    /// 构建一条由精确托管字节支撑的权威附件夹具。
    ///
    /// # Parameters / 参数
    ///
    /// - `path`: Exact managed file path.
    ///   `path`：精确托管文件路径。
    /// - `asset_id`: Stable host-managed asset id.
    ///   `asset_id`：稳定宿主管理资源 ID。
    /// - `file_name`: Logical display file name.
    ///   `file_name`：逻辑显示文件名。
    /// - `content`: Exact bytes already written at `path`.
    ///   `content`：已经写入 `path` 的精确字节。
    ///
    /// # Returns / 返回值
    ///
    /// One canonical attachment with frozen length and digest identity.
    /// 一条锁定长度与摘要身份的规范附件。
    fn attachment_fixture(
        path: &Path,
        asset_id: &str,
        file_name: &str,
        content: &[u8],
    ) -> ConversationAttachment {
        ConversationAttachment {
            kind: ConversationAttachmentKind::File,
            file_name: file_name.to_string(),
            local_path: path.to_string_lossy().into_owned(),
            resource_path: None,
            source_path: None,
            asset_id: Some(asset_id.to_string()),
            byte_len: Some(content.len() as u64),
            source_sha256: Some(hex::encode(Sha256::digest(content))),
            delivery_mode: None,
            preview_local_path: None,
            mime_type: "application/octet-stream".to_string(),
        }
    }

    /// Persists per-occurrence draft proof through real project registration and exact root-session reload.
    /// 通过真实项目注册及精确根会话重载，持久保存逐出现草稿证明。
    #[tokio::test]
    async fn project_prompt_handoff_preserves_original_order_receipts_and_durable_owner() {
        use vulcan_agent_runtime::attachment_asset_index::{
            AttachmentDraftMediaDelivery, AttachmentDraftMediaOrigin,
        };
        use vulcan_session_core::v1::{UserPromptMediaRoute, UserPromptResourceDeliveryKind};
        use vulcan_workspace_store::workspace_store::{
            ProjectResourceUploadReceipt, ProviderBinding,
        };

        // The isolated real controller owns resource files, session metadata and event segments.
        // 隔离真实控制器持有资源文件、会话元数据及事件分片。
        let fixture = LifecycleStorageFixture::start().await;
        // Two occurrences intentionally reuse identical source bytes but carry different original modes/models.
        // 两次出现故意复用相同源字节，但携带不同原方式及模型。
        let path = fixture.controller.data_root.join("prompt-original.bin");
        std::fs::write(&path, b"original").expect("write isolated source");
        // Source identity is captured before any project registration.
        // 来源身份在任何项目注册之前捕获。
        let attachment = attachment_fixture(&path, "asset-original", "original.bin", b"original");
        // The first chronological occurrence selected inline on A.
        // 第一次时序出现选择A内联。
        let original_a = AttachmentDraftMediaOrigin::new(
            &attachment,
            UserPromptMediaRoute {
                provider_id: "provider".into(),
                service_entry_id: "entry".into(),
                connection_id: "connection".into(),
                model_id: "model-A".into(),
                protocol_profile_id: "openai-responses.v1".into(),
            },
            AttachmentDraftMediaDelivery::Inline,
        )
        .expect("inline original");
        // The later occurrence owns one private, immutable upload receipt selected on B.
        // 后续出现持有在B选择的一份私有不可变上传回执。
        let binding = ProviderBinding {
            provider: "provider".into(),
            connection_id: Some("connection".into()),
            media_id: "file-original-private".into(),
            upload_receipt: Some(ProjectResourceUploadReceipt {
                draft_intent_id: None,
                version: 1,
                service_entry_id: "entry".into(),
                upload_channel_id: "files".into(),
                upload_contract_fingerprint: format!("sha256:{}", "a".repeat(64)),
                resource_kind: vulcan_ai_contract::ResourceKind::File,
                mime_type: attachment.mime_type.clone(),
                source_kind: vulcan_ai_contract::ResourceSourceKind::ProviderFileId,
                session_scope_id: None,
                created_at_unix_millis: 1,
                expires_at_unix_millis: u64::MAX,
            }),
        };
        // Only model identity differs; shared provider and connection must not merge the occurrences.
        // 只有模型身份不同；共享供应商及连接不能合并两次出现。
        let mut route_b = original_a.original_route.clone();
        route_b.model_id = "model-B".into();
        // The full B receipt is validated before its opaque identity can enter any prompt.
        // 完整B回执在其不透明身份可进入任何提示词前校验。
        let original_b = AttachmentDraftMediaOrigin::new(
            &attachment,
            route_b,
            AttachmentDraftMediaDelivery::Upload {
                upload_channel_id: "files".into(),
                upload_contract_fingerprint: format!("sha256:{}", "a".repeat(64)),
                receipt: Some(binding.clone()),
            },
        )
        .expect("uploaded original");
        // A malformed later item must not publish an earlier valid receipt or even create the resource partition.
        // 错误后项不能发布前项有效回执，甚至不能创建资源分区。
        let mut invalid = original_b.clone();
        invalid.original_route.connection_id = "different-connection".into();
        // One serialized catalog owner is shared across failed admission and subsequent valid registration.
        // 失败准入及后续有效注册共享一个串行目录所有者。
        let resource_registry = ProjectResourceCatalogRegistry::default();
        assert!(
            register_project_prompt_attachments_v1(
                &fixture.controller,
                &resource_registry,
                "prompt-rejected",
                &[attachment.clone(), attachment.clone()],
                &[
                    PromptAttachmentOrigin::Staged(Some(Box::new(original_b.clone()))),
                    PromptAttachmentOrigin::Staged(Some(Box::new(invalid)))
                ],
                None,
            )
            .is_err()
        );
        assert!(
            !project_resource_directory_v1(
                &fixture.controller.data_root,
                SESSION_V1_STORAGE_PARTITION,
                "prompt-rejected"
            )
            .expect("project path")
            .exists()
        );
        // Accepted registration preserves the original occurrence order even when ResourceId is deduplicated.
        // 即使ResourceId去重，已接纳注册也保留原出现顺序。
        let (resource_ids, parameters) = register_project_prompt_attachments_v1(
            &fixture.controller,
            &resource_registry,
            "prompt-provenance",
            &[attachment.clone(), attachment.clone()],
            &[
                PromptAttachmentOrigin::Staged(Some(Box::new(original_a))),
                PromptAttachmentOrigin::Staged(Some(Box::new(original_b))),
            ],
            None,
        )
        .expect("register prompt proofs");
        assert_eq!(resource_ids[0], resource_ids[1]);
        // The submission receives prompt-owned proof, not a mutable file-global latest-model value.
        // 提交接收提示词所有证明，而非可变文件全局最近模型值。
        let parameters = parameters.expect("media parameters");
        assert_eq!(
            parameters.prepared_resources[0]
                .as_ref()
                .expect("first proof")
                .original_route
                .model_id,
            "model-A"
        );
        assert_eq!(
            parameters.prepared_resources[1]
                .as_ref()
                .expect("second proof")
                .original_route
                .model_id,
            "model-B"
        );
        assert!(matches!(
            parameters.prepared_resources[1]
                .as_ref()
                .expect("second proof")
                .delivery,
            UserPromptResourceDeliveryKind::Uploaded { .. }
        ));
        assert!(
            !serde_json::to_string(&parameters)
                .expect("serialize public proof")
                .contains("file-original-private")
        );
        // The existing project catalog retains the exact private receipt once, independently of occurrence count.
        // 既有项目目录仅保存一次精确私有回执，独立于出现次数。
        let project_dir = project_resource_directory_v1(
            &fixture.controller.data_root,
            SESSION_V1_STORAGE_PARTITION,
            "prompt-provenance",
        )
        .expect("project path");
        assert_eq!(
            load_project_resource_catalog(&project_dir)
                .expect("catalog")
                .entries[0]
                .provider_bindings,
            vec![binding]
        );
        // Materialize a real V1 root using the same production input path as immediate submission.
        // 使用与即时提交相同的生产输入路径物化真实V1根会话。
        let session_id = SessionId::new_v4();
        // The new registry publishes the unique original session owner.
        // 新注册表发布唯一原会话所有者。
        let registry = SessionRuntimeRegistry::default();
        create_and_register_root_session_v1(
            &fixture.controller,
            &registry,
            CreateRootSessionV1Request {
                session_id,
                project_id: "prompt-provenance".into(),
                title: "Original proof".into(),
                root_submission: ClientInputSubmission::new(
                    ClientActionId::try_new("original-proof").expect("client id"),
                    "ordered prompt",
                    resource_ids.clone(),
                )
                .with_parameters(parameters.clone())
                .expect("input parameters"),
                now_ms: 1_787_788_800_000,
            },
        )
        .await
        .expect("durable root publication");
        // A separate registry forces a real storage reload rather than reusing the in-memory owner.
        // 独立注册表强制真实存储重载，而非复用内存所有者。
        let reload_registry = SessionRuntimeRegistry::default();
        // Inspect the reloaded user event; no API event is needed to own preparation facts.
        // 检查重载用户事件；准备事实不需要API事件作为所有者。
        let reloaded = get_or_load_session_v1(&fixture.controller, &reload_registry, session_id)
            .await
            .expect("reload prompt");
        assert!(
            matches!(reloaded.read(|document| document.events()[0].event.clone()).expect("read prompt"), EventBody::UserInput(input) if input.text == "ordered prompt" && input.resource_ids == resource_ids && input.parameters == Some(parameters))
        );
        // Freeze actual prompt parameters durably before simulating an acknowledgement-lost retry.
        // 模拟确认丢失重试之前，可靠冻结真实提示词参数。
        reloaded
            .mutate_durable(
                crate::session_runtime_registry_v1::SessionMutationAdmission::MetadataOnly,
                crate::session_runtime_registry_v1::SessionFlushReason::MetadataChanged,
                |document| {
                    // All initial occurrences were prepared; the first request may still use a different model.
                    // 所有初始出现均已准备；首次请求仍可使用另一个模型。
                    let resources = document
                        .client_input_for_action(&ClientActionId::try_new("original-proof")?)?
                        .expect("accepted source")
                        .parameters
                        .expect("parameters")
                        .prepared_resources
                        .into_iter()
                        .map(|resource| resource.expect("original occurrence"))
                        .collect::<Vec<_>>();
                    // This is the actual first projection route, not a replacement for either original attachment model.
                    // 这是真实首次投影路线，而非替换任一原附件模型。
                    let mut route = resources[0].original_route.clone();
                    route.model_id = "model-C".into();
                    document.freeze_user_prompt_media(
                        vec![vulcan_session_core::v1::UserPromptMediaBinding {
                            prompt_seq: document.events()[0].seq(),
                            origin: vulcan_session_core::v1::UserPromptMediaOrigin {
                                route,
                                resources,
                            },
                        }],
                        1_787_788_800_001,
                    )
                },
            )
            .expect("durably freeze prompt parameters");
        // An acknowledged input can be retried after the draft has gone; no active asset lease or new receipt is needed.
        // 已接纳输入可以在草稿消失后重试；不需要活动资产租约或新回执。
        let accepted = reloaded
            .read(|document| {
                document.client_input_for_action(
                    &ClientActionId::try_new("original-proof").expect("action id"),
                )
            })
            .expect("read accepted owner")
            .expect("valid source")
            .expect("accepted input");
        // Reuse the original source order and initial proof through the production registration boundary.
        // 通过生产注册边界复用原来源顺序和初始证明。
        let (retry_resources, retry_parameters) = register_project_prompt_attachments_v1(
            &fixture.controller,
            &resource_registry,
            "prompt-provenance",
            &[attachment.clone(), attachment.clone()],
            &[
                PromptAttachmentOrigin::Staged(None),
                PromptAttachmentOrigin::Staged(None),
            ],
            Some(&accepted),
        )
        .expect("exact retry without draft lease");
        assert_eq!(retry_resources, accepted.resource_ids);
        assert_eq!(
            retry_parameters
                .as_ref()
                .expect("retry proof")
                .prepared_resources,
            accepted
                .parameters
                .as_ref()
                .expect("original proof")
                .prepared_resources
        );
        assert!(matches!(
            retry_parameters
                .as_ref()
                .expect("retry proof")
                .media_delivery,
            vulcan_session_core::v1::UserPromptMediaDelivery::Pending
        ));
        assert!(matches!(
            accepted
                .parameters
                .as_ref()
                .expect("original proof")
                .media_delivery,
            vulcan_session_core::v1::UserPromptMediaDelivery::Frozen { .. }
        ));
        assert!(
            !reloaded
                .submit_root_input(
                    ClientInputSubmission {
                        client_action_id: accepted.client_action_id.clone(),
                        text: accepted.text.clone(),
                        resource_ids: retry_resources,
                        parameters: retry_parameters,
                        bound_request_config: None,
                    },
                    1_787_788_800_002
                )
                .expect("idempotent prompt retry")
                .changed
        );
        // Content deduplication cannot hide a changed occurrence count on a retry.
        // 内容去重不能掩盖重试时出现次数变化。
        assert!(
            register_project_prompt_attachments_v1(
                &fixture.controller,
                &resource_registry,
                "prompt-provenance",
                &[attachment],
                &[PromptAttachmentOrigin::Staged(None)],
                Some(&accepted)
            )
            .is_err()
        );
        // A project reference keeps its logical identity even when the entity is unavailable; admission must not reread it as a staged file.
        // 即使实体不可用，项目引用仍保留逻辑身份；准入不能把它当作暂存文件重新读取。
        let locator = reloaded.storage_locator().expect("exact locator");
        // Reconstruct canonical metadata without source-byte I/O.
        // 不执行来源字节I/O，重建规范元数据。
        let project_attachment = crate::attachments::composer_attachment_summary_to_core(
            crate::conversation_resources_v1::composer_attachment_from_project_resource_v1(
                &locator,
                resource_ids[0].value(),
            )
            .expect("project metadata"),
        )
        .expect("canonical metadata");
        // The fixture alone owns this physical path; its catalog row deliberately remains intact.
        // 只有本夹具持有此物理路径；其目录行有意保持完整。
        std::fs::remove_file(&project_attachment.local_path)
            .expect("remove isolated project entity");
        assert!(
            crate::conversation_resources_v1::composer_attachment_from_project_resource_v1(
                &locator,
                resource_ids[0].value()
            )
            .is_ok()
        );
        // Add one genuinely new source between the two original occurrences.
        // 在两次原出现之间添加一个真正新来源。
        let new_path = fixture.controller.data_root.join("new-mixed-source.bin");
        std::fs::write(&new_path, b"new source").expect("write isolated new source");
        // Only this new staged source may allocate a new ResourceId.
        // 只有本新暂存来源可分配新ResourceId。
        let new_attachment =
            attachment_fixture(&new_path, "asset-new-source", "new.bin", b"new source");
        // Resolve original source positions, not the new draft order or latest receipt.
        // 解析原来源位置，而非新草稿顺序或最新回执。
        let original_second = reloaded
            .read(|document| {
                document.user_prompt_resource_preparation(
                    &vulcan_session_core::v1::UserPromptResourceSource {
                        client_action_id: accepted.client_action_id.clone(),
                        resource_position: 1,
                    },
                    resource_ids[0],
                )
            })
            .expect("read source")
            .expect("source proof");
        // The other equal ResourceId still has its independent first-position model.
        // 另一个相等ResourceId仍具有独立首位置模型。
        let original_first = reloaded
            .read(|document| {
                document.user_prompt_resource_preparation(
                    &vulcan_session_core::v1::UserPromptResourceSource {
                        client_action_id: accepted.client_action_id.clone(),
                        resource_position: 0,
                    },
                    resource_ids[0],
                )
            })
            .expect("read source")
            .expect("source proof");
        // The production mixed registration preserves original IDs and only reads the new staged entity.
        // 生产混合注册保留原ID，只读取新暂存实体。
        let (mixed_ids, mixed_parameters) = register_project_prompt_attachments_v1(
            &fixture.controller,
            &resource_registry,
            "prompt-provenance",
            &[
                project_attachment.clone(),
                new_attachment,
                project_attachment,
            ],
            &[
                PromptAttachmentOrigin::Project {
                    resource_id: resource_ids[0],
                    preparation: Box::new(original_second),
                },
                PromptAttachmentOrigin::Staged(None),
                PromptAttachmentOrigin::Project {
                    resource_id: resource_ids[0],
                    preparation: Box::new(original_first),
                },
            ],
            None,
        )
        .expect("mixed project and staged registration");
        assert_eq!(mixed_ids[0], resource_ids[0]);
        assert_eq!(mixed_ids[2], resource_ids[0]);
        assert_ne!(mixed_ids[1], resource_ids[0]);
        // New occurrence order is B, unprepared, A even though first/last refer to the same resource bytes.
        // 即使首尾引用相同资源字节，新出现顺序仍为B、未准备、A。
        let mixed = mixed_parameters.expect("mixed parameters");
        assert_eq!(
            mixed.prepared_resources[0]
                .as_ref()
                .expect("second source")
                .original_route
                .model_id,
            "model-B"
        );
        assert!(mixed.prepared_resources[1].is_none());
        assert_eq!(
            mixed.prepared_resources[2]
                .as_ref()
                .expect("first source")
                .original_route
                .model_id,
            "model-A"
        );
        assert_eq!(
            load_project_resource_catalog(&project_dir)
                .expect("catalog")
                .entries
                .len(),
            2
        );
        fixture.shutdown().await;
    }

    /// Attachment registration preserves order and exact logical-id reuse.
    /// 附件注册保持顺序与精确逻辑 ID 复用。
    #[tokio::test]
    async fn project_attachment_bridge_preserves_order_and_reuse() {
        let fixture = LifecycleStorageFixture::start().await;
        let managed_dir = fixture.controller.data_root.join("managed-attachments");
        std::fs::create_dir_all(&managed_dir).expect("create managed attachment directory");
        let first_path = managed_dir.join("first.bin");
        let second_path = managed_dir.join("second.bin");
        std::fs::write(&first_path, b"first").expect("write first managed attachment");
        std::fs::write(&second_path, b"second").expect("write second managed attachment");
        let first = attachment_fixture(&first_path, "asset-first", "first.bin", b"first");
        let second = attachment_fixture(&second_path, "asset-second", "second.bin", b"second");
        let registry = ProjectResourceCatalogRegistry::default();
        let resource_ids = register_project_attachments_v1(
            &fixture.controller,
            &registry,
            "project-resource-bridge",
            &[first.clone(), second, first],
        )
        .expect("register ordered project resources");
        let project_dir = project_resource_directory_v1(
            &fixture.controller.data_root,
            SESSION_V1_STORAGE_PARTITION,
            "project-resource-bridge",
        )
        .expect("resolve project resource directory");
        let catalog = load_project_resource_catalog(&project_dir).expect("load project catalog");

        assert_eq!(resource_ids[0].value(), 1);
        assert_eq!(resource_ids[1].value(), 2);
        assert_eq!(resource_ids[2], resource_ids[0]);
        assert_eq!(catalog.entries.len(), 2);
        fixture.shutdown().await;
    }

    /// Changed managed bytes fail before any project resource row is published.
    /// 托管字节变化会在发布任何项目资源行前失败。
    #[tokio::test]
    async fn project_attachment_bridge_rejects_changed_managed_bytes() {
        let fixture = LifecycleStorageFixture::start().await;
        let managed_path = fixture.controller.data_root.join("managed-changed.bin");
        std::fs::write(&managed_path, b"before").expect("write original managed attachment");
        let attachment =
            attachment_fixture(&managed_path, "asset-changed", "changed.bin", b"before");
        std::fs::write(&managed_path, b"after!").expect("change managed attachment bytes");
        let registry = ProjectResourceCatalogRegistry::default();
        let result = register_project_attachments_v1(
            &fixture.controller,
            &registry,
            "project-resource-failure",
            &[attachment],
        );
        let project_dir = project_resource_directory_v1(
            &fixture.controller.data_root,
            SESSION_V1_STORAGE_PARTITION,
            "project-resource-failure",
        )
        .expect("resolve failed project resource directory");

        assert!(matches!(
            result,
            Err(SessionV1LifecycleError::ResourceCatalog(
                ProjectResourceCatalogError::ManagedAttachment { .. }
            ))
        ));
        assert!(!project_dir.exists());
        fixture.shutdown().await;
    }

    /// The selected planning node survives the first durable root publication and exact reload.
    /// 选定规划节点随首次根发布持久化，并在精确重载后保持一致。
    #[tokio::test]
    async fn root_creation_persists_new_session_window_in_initial_metadata() {
        let fixture = LifecycleStorageFixture::start().await;
        let session_id = SessionId::new_v4();
        let registry = SessionRuntimeRegistry::default();
        let node_id = vulcan_context::ContextCompactionWindowNodeId::CompressionWindow3;
        let handle = create_and_register_root_session_v1_with_context_window(
            &fixture.controller,
            &registry,
            CreateRootSessionV1Request {
                session_id,
                project_id: String::from("project-explicit"),
                title: String::from("Window default"),
                root_submission: ClientInputSubmission::new(
                    ClientActionId::try_new(format!("root-{session_id}"))
                        .expect("root client action id"),
                    "root input",
                    Vec::new(),
                ),
                now_ms: 1_787_788_800_000,
            },
            Some(node_id),
        )
        .await
        .expect("create root with planning node");
        assert_eq!(
            handle
                .read(|document| document.metadata.config.context_compaction_window_node_id)
                .expect("read initial metadata"),
            Some(node_id)
        );
        let reloaded = get_or_load_session_v1(
            &fixture.controller,
            &SessionRuntimeRegistry::default(),
            session_id,
        )
        .await
        .expect("reload root metadata");
        assert_eq!(
            reloaded
                .read(|document| document.metadata.config.context_compaction_window_node_id)
                .expect("read persisted metadata"),
            Some(node_id)
        );
        fixture.shutdown().await;
    }

    /// Initial publication contains the root input and survives exact reload.
    /// 初始发布包含根输入，并在精确重载后保持一致。
    #[tokio::test]
    async fn root_creation_is_durable_and_exact_reload_preserves_frontiers() {
        let fixture = LifecycleStorageFixture::start().await;
        let session_id = SessionId::new_v4();
        let registry = SessionRuntimeRegistry::default();
        let handle = create_fixture_session(&fixture, &registry, session_id).await;
        let (memory_revision, durable_revision, event_count, last_seq, frontier) = handle
            .read(|document| {
                (
                    document.memory_revision(),
                    document.durable_revision(),
                    document.events().len(),
                    document.events().last().map(|event| event.seq),
                    document.id_frontier(),
                )
            })
            .expect("read created document");
        assert_eq!(memory_revision, durable_revision);
        assert!(matches!(
            &handle
                .read(|document| {
                    document.events().iter().find_map(|event| match &event.event {
                        EventBody::UserInput(input) => Some(input.clone()),
                        _ => None,
                    })
                })
                .expect("read root event"),
            Some(input)
                if input.text == "root input" && input.resource_ids.len() == 1
        ));
        assert_eq!(
            handle
                .read(|document| document.collaboration_run_control())
                .expect("created run control"),
            vulcan_session_core::v1::collaboration::CollaborationRunControl::Running,
        );
        let created_runtime_state = handle
            .read(|document| document.runtime_lock.initialization_state)
            .expect("read created runtime initialization state");
        assert_eq!(
            created_runtime_state,
            RuntimeInitializationState::FreshUninitialized
        );
        assert!(Arc::ptr_eq(
            &handle,
            &require_registered_session_v1(&registry, session_id)
                .expect("require registered V1 session")
        ));

        let reload_registry = SessionRuntimeRegistry::default();
        let reloaded = get_or_load_session_v1(&fixture.controller, &reload_registry, session_id)
            .await
            .expect("reload exact V1 tail");
        let (reloaded_count, reloaded_last_seq, reloaded_frontier, reloaded_runtime_state) =
            reloaded
                .read(|document| {
                    (
                        document.events().len(),
                        document.events().last().map(|event| event.seq),
                        document.id_frontier(),
                        document.runtime_lock.initialization_state,
                    )
                })
                .expect("read reloaded document");
        assert_eq!(reloaded_count, event_count);
        assert_eq!(reloaded_last_seq, last_seq);
        assert_eq!(reloaded_frontier, frontier);
        assert_eq!(reloaded_runtime_state, created_runtime_state);
        assert_eq!(
            reloaded
                .read(|document| document.collaboration_run_control())
                .expect("reloaded run control"),
            vulcan_session_core::v1::collaboration::CollaborationRunControl::Running,
        );

        assert!(matches!(
            create_and_register_root_session_v1(
                &fixture.controller,
                &registry,
                CreateRootSessionV1Request {
                    session_id,
                    project_id: String::from("project-explicit"),
                    title: String::from("duplicate"),
                    root_submission: ClientInputSubmission::new(
                        ClientActionId::try_new(format!("duplicate-root-{session_id}"))
                            .expect("duplicate root client action id"),
                        "duplicate",
                        Vec::new(),
                    ),
                    now_ms: 1_787_788_800_001,
                },
            )
            .await,
            Err(SessionV1LifecycleError::Runtime(
                SessionRuntimeError::DuplicateSession(actual)
            )) if actual == session_id
        ));
        fixture.shutdown().await;
    }

    /// Embedded task publication remains parent-addressed and exact execution replay is idempotent.
    /// 内嵌任务发布始终由父定位器寻址，精确执行重放保持幂等。
    #[tokio::test]
    async fn embedded_task_creation_has_no_index_row_and_exact_replay_is_idempotent() {
        let fixture = LifecycleStorageFixture::start().await;
        let root_session_id = SessionId::new_v4();
        let child_session_id = SessionId::new_v4();
        let create_registry = SessionRuntimeRegistry::default();
        let root_handle = create_fixture_session(&fixture, &create_registry, root_session_id).await;
        let execution_id = String::from("embedded-task-execution-1");
        let first_facts = TaskV1ExecutionFacts {
            root_session_id,
            parent_session_id: root_session_id,
            task_id: child_session_id,
            execution_id: execution_id.clone(),
            prompt: String::from("inspect the exact child ledger"),
            display_title: String::from("Embedded task"),
            is_resumed: false,
            admitted_at_ms: 1_787_788_800_020,
        };
        let child_handle =
            create_or_load_embedded_task_session_v1(&create_registry, first_facts.clone())
                .expect("create embedded task session");
        let (created_memory, created_durable, created_events) = child_handle
            .read(|document| {
                (
                    document.memory_revision(),
                    document.durable_revision(),
                    document.events().to_vec(),
                )
            })
            .expect("read created embedded task session");
        assert_eq!(created_memory, created_durable);
        assert_eq!(created_events.len(), 2);
        assert!(matches!(
            created_events[1].event,
            EventBody::Collaboration(_)
        ));
        assert_eq!(
            child_handle
                .read(|document| document.collaboration_run_control())
                .unwrap(),
            vulcan_session_core::v1::collaboration::CollaborationRunControl::Running
        );
        assert!(matches!(
            &created_events[0].event,
            EventBody::UserInput(input)
                if input.text == first_facts.prompt
                    && input.client_action_id.as_str() == execution_id
                    && input.resource_ids.is_empty()
        ));
        let root_locator = root_handle.storage_locator().expect("root storage locator");
        let child_locator =
            locate_embedded_task_session_v1(&root_locator, &child_session_id.to_string())
                .expect("derive embedded task locator");
        assert!(
            child_locator
                .relative_path()
                .ends_with(&format!("TASK/task-{child_session_id}"))
        );
        assert!(load_session_v1_tail(&child_locator).is_ok());
        assert!(
            locate_session_v1_storage(&fixture.controller, &child_session_id.to_string())
                .await
                .is_err(),
            "embedded task sessions must never publish SQLite rows"
        );

        let display_registry = SessionRuntimeRegistry::default();
        let displayed = child_locator
            .load_recent_event_window(30)
            .expect("read embedded task through the exact root-derived locator");
        assert_eq!(displayed.events, created_events);
        assert!(matches!(
            display_registry.get(child_session_id),
            Err(SessionRuntimeError::SessionNotRegistered(actual)) if actual == child_session_id
        ));
        assert!(
            locate_session_v1_storage(&fixture.controller, &child_session_id.to_string())
                .await
                .is_err(),
            "snapshot reading must not create an embedded-task SQLite row"
        );

        let reload_registry = SessionRuntimeRegistry::default();
        get_or_load_session_v1(&fixture.controller, &reload_registry, root_session_id)
            .await
            .expect("reload root owner before child replay");
        let replayed = create_or_load_embedded_task_session_v1(
            &reload_registry,
            TaskV1ExecutionFacts {
                is_resumed: true,
                ..first_facts
            },
        )
        .expect("replay exact embedded task execution");
        let (replayed_memory, replayed_durable, replayed_events) = replayed
            .read(|document| {
                (
                    document.memory_revision(),
                    document.durable_revision(),
                    document.events().to_vec(),
                )
            })
            .expect("read replayed embedded task session");
        assert_eq!(replayed_memory, replayed_durable);
        assert_eq!(replayed_events, created_events);
        fixture.shutdown().await;
    }

    /// A resumed child execution stops the prior open Turn before admitting its distinct root input.
    /// 续跑子执行会先停止先前仍开放的回合，再准入其不同的根输入。
    #[tokio::test]
    async fn embedded_task_resume_replaces_open_turn_before_new_root_input() {
        let fixture = LifecycleStorageFixture::start().await;
        let root_session_id = SessionId::new_v4();
        let child_session_id = SessionId::new_v4();
        let registry = SessionRuntimeRegistry::default();
        create_fixture_session(&fixture, &registry, root_session_id).await;
        let first = create_or_load_embedded_task_session_v1(
            &registry,
            TaskV1ExecutionFacts {
                root_session_id,
                parent_session_id: root_session_id,
                task_id: child_session_id,
                execution_id: String::from("embedded-task-execution-open"),
                prompt: String::from("run the original child task"),
                display_title: String::from("Resumable embedded task"),
                is_resumed: false,
                admitted_at_ms: 1_787_788_800_020,
            },
        )
        .expect("create initial embedded task turn");
        let first_turn_id = first
            .read(|document| document.active_turn().expect("initial child turn").turn_id)
            .expect("read initial child turn");
        // firstApi models the superseded physical child execution that interruption has already aborted.
        // firstApi 模拟已经被中止操作终止的被替代子代理物理执行。
        let first_api_preparation = first
            .read(|document| {
                lifecycle_test_api_input(
                    document,
                    ApiRequestSnapshot::new("provider", "model"),
                    1_787_788_800_030,
                )
            })
            .expect("build initial child API preparation");
        let first_api_call_id = first
            .prepare_and_start_api_durable(first_api_preparation)
            .expect("start initial child API")
            .value
            .api_call_id;
        first
            .update_reasoning(
                first_api_call_id,
                "superseded child reasoning",
                AssistantSemanticStatus::Streaming,
                1_787_788_800_040,
            )
            .expect("persist initial child API activity");

        // Invalid successor input must roll back interruption, closure, revisions and durable history together.
        // 非法后继输入必须一并回滚中断、闭合、修订及可靠历史。
        let before = first
            .read_full_durable(|document| {
                (
                    document.durable_revision(),
                    document.events().to_vec(),
                    serde_json::to_value(&document.metadata).unwrap(),
                )
            })
            .unwrap();
        assert!(
            submit_resumed_embedded_task_root_input(
                &first,
                ClientInputSubmission::new(
                    ClientActionId::try_new("rejected-empty-successor").unwrap(),
                    "",
                    vec![],
                ),
                1_787_788_800_100,
            )
            .is_err()
        );
        first
            .read_full_durable(|document| {
                assert_eq!(document.durable_revision(), before.0);
                assert_eq!(document.events(), before.1.as_slice());
                assert_eq!(serde_json::to_value(&document.metadata).unwrap(), before.2);
                assert_eq!(
                    document.active_api().unwrap().api_call_id,
                    first_api_call_id
                );
                assert_eq!(document.active_turn().unwrap().turn_id, first_turn_id);
                assert!(!document.active_turn().unwrap().closed);
            })
            .unwrap();

        let resumed = create_or_load_embedded_task_session_v1(
            &registry,
            TaskV1ExecutionFacts {
                root_session_id,
                parent_session_id: root_session_id,
                task_id: child_session_id,
                execution_id: String::from("embedded-task-execution-resumed"),
                prompt: String::from("process the queued parent message"),
                display_title: String::from("Resumable embedded task"),
                is_resumed: true,
                admitted_at_ms: 1_787_788_800_120,
            },
        )
        .expect("resume embedded task after stopping the superseded turn");
        resumed
            .read(|document| {
                let active_turn = document.active_turn().expect("resumed child turn");
                assert!(!active_turn.closed);
                assert_ne!(active_turn.turn_id, first_turn_id);
                assert!(document.events().iter().any(|event| {
                    event.turn_id == Some(first_turn_id)
                        && matches!(
                            &event.event,
                            EventBody::TurnBoundary(boundary)
                                if boundary.outcome == TurnOutcome::Stopped
                        )
                }));
                assert!(document.events().iter().any(|event| {
                    matches!(
                        &event.event,
                        EventBody::ApiStats(stats)
                            if stats.api_call_id == first_api_call_id
                                && stats.status == ApiCallStatus::Interrupted
                    )
                }));
                assert!(document.events().iter().any(|event| {
                    event.turn_id == Some(active_turn.turn_id)
                        && matches!(
                            &event.event,
                            EventBody::UserInput(input)
                                if input.client_action_id.as_str()
                                    == "embedded-task-execution-resumed"
                                    && input.text == "process the queued parent message"
                        )
                }));
                assert_eq!(document.memory_revision(), document.durable_revision());
                assert_eq!(document.durable_revision(), before.0 + 1);
            })
            .expect("verify resumed embedded task lifecycle");
        // projection is captured from the durable owner used by physical request reconstruction, not the legacy controller.
        // projection 从物理请求重建使用的可靠所有者捕获，而不是从旧控制器捕获。
        let projection = resumed
            .capture_context_compaction_planning_snapshot()
            .expect("capture durable successor provider input");
        assert!(projection.items.iter().any(|item| matches!(
            item,
            vulcan_session_core::v1::projection::ProjectedItem::UserInput { text, .. }
                if text == "process the queued parent message"
        )));
        fixture.shutdown().await;
    }

    /// Restart projection interrupts active API facts and publishes them first.
    /// 重启投影会中断活动 API 事实并先完成发布。
    #[tokio::test]
    async fn restart_interruption_is_durable_before_handle_publication() {
        let fixture = LifecycleStorageFixture::start().await;
        let session_id = SessionId::new_v4();
        let create_registry = SessionRuntimeRegistry::default();
        let handle = create_fixture_session(&fixture, &create_registry, session_id).await;
        let preparation = handle
            .read(|document| {
                lifecycle_test_api_input(
                    document,
                    ApiRequestSnapshot::new("provider", "model"),
                    1_787_788_800_010,
                )
            })
            .expect("build lifecycle API preparation");
        let api_call_id = handle
            .prepare_and_start_api_durable(preparation)
            .expect("persist API start")
            .value
            .api_call_id;
        handle
            .update_reasoning(
                api_call_id,
                "streaming reasoning",
                AssistantSemanticStatus::Streaming,
                1_787_788_800_011,
            )
            .expect("update streaming reasoning");
        handle.flush_all().expect("persist in-flight API state");

        let reload_registry = SessionRuntimeRegistry::default();
        let reloaded = get_or_load_session_v1(&fixture.controller, &reload_registry, session_id)
            .await
            .expect("reload and project interruption");
        let (active_api, memory_revision, durable_revision, has_interrupted_reasoning, has_stats) =
            reloaded
                .read(|document| {
                    (
                        document.active_api().is_some(),
                        document.memory_revision(),
                        document.durable_revision(),
                        document.events().iter().any(|event| {
                            matches!(
                                &event.event,
                                EventBody::AssistantReasoning(reasoning)
                                    if reasoning.api_call_id == api_call_id
                                        && reasoning.status
                                            == AssistantSemanticStatus::Interrupted
                            )
                        }),
                        document.events().iter().any(|event| {
                            matches!(
                                &event.event,
                                EventBody::ApiStats(stats)
                                    if stats.api_call_id == api_call_id
                                        && stats.status == ApiCallStatus::Interrupted
                            )
                        }),
                    )
                })
                .expect("read interrupted projection");
        assert!(!active_api);
        assert_eq!(memory_revision, durable_revision);
        assert!(has_interrupted_reasoning);
        assert!(has_stats);

        let locator = locate_session_v1_storage(&fixture.controller, &session_id.to_string())
            .await
            .expect("locate projected V1 session");
        let persisted = load_session_v1_tail(&locator).expect("reload projected V1 tail from disk");
        assert!(persisted.document.events().iter().any(|event| {
            matches!(
                &event.event,
                EventBody::ApiStats(stats)
                    if stats.api_call_id == api_call_id
                        && stats.status == ApiCallStatus::Interrupted
            )
        }));
        fixture.shutdown().await;
    }

    /// Concurrent exact loads converge on one physical process-local handle.
    /// 并发精确加载会收敛到一个进程内物理句柄。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_exact_loads_publish_one_handle() {
        let fixture = LifecycleStorageFixture::start().await;
        let session_id = SessionId::new_v4();
        let create_registry = SessionRuntimeRegistry::default();
        create_fixture_session(&fixture, &create_registry, session_id).await;

        // registry is shared by every racing loader while each caller still
        // performs its own mandatory SQLite locate first.
        // registry 由全部竞态加载器共享，同时每个调用方仍先执行强制 SQLite 定位。
        let registry = Arc::new(SessionRuntimeRegistry::default());
        let barrier = Arc::new(tokio::sync::Barrier::new(8));
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let controller = fixture.controller.clone();
            let registry = Arc::clone(&registry);
            let barrier = Arc::clone(&barrier);
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                get_or_load_session_v1(&controller, &registry, session_id)
                    .await
                    .expect("concurrent exact load")
            }));
        }
        let mut handles = Vec::new();
        for task in tasks {
            handles.push(task.await.expect("join concurrent loader"));
        }
        for handle in handles.iter().skip(1) {
            assert!(Arc::ptr_eq(&handles[0], handle));
        }
        fixture.shutdown().await;
    }

    /// Strict index failure leaves the receiving registry completely empty.
    /// 严格索引失败会让接收注册表保持完全为空。
    #[tokio::test]
    async fn indexed_creation_failure_never_registers_a_handle() {
        let fixture = LifecycleStorageFixture::start().await;
        let session_id = SessionId::new_v4();
        let first_registry = SessionRuntimeRegistry::default();
        create_fixture_session(&fixture, &first_registry, session_id).await;

        let failing_registry = SessionRuntimeRegistry::default();
        let result = create_and_register_root_session_v1(
            &fixture.controller,
            &failing_registry,
            CreateRootSessionV1Request {
                session_id,
                project_id: String::from("project-explicit"),
                title: String::from("conflict"),
                root_submission: ClientInputSubmission::new(
                    ClientActionId::try_new(format!("conflict-root-{session_id}"))
                        .expect("conflict root client action id"),
                    "conflict",
                    Vec::new(),
                ),
                now_ms: 1_787_788_800_020,
            },
        )
        .await;
        assert!(matches!(result, Err(SessionV1LifecycleError::Storage(_))));
        assert!(matches!(
            require_registered_session_v1(&failing_registry, session_id),
            Err(SessionRuntimeError::SessionNotRegistered(actual)) if actual == session_id
        ));
        drop(
            failing_registry
                .reserve_creation(session_id)
                .expect("failed creation releases its reservation"),
        );
        fixture.shutdown().await;
    }

    /// Same-id creators reserve before I/O so the loser cannot overwrite files.
    /// 相同 ID 创建器会在 I/O 前预留，因此失败方无法覆盖文件。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_same_id_creation_publishes_only_the_reserved_winner() {
        let fixture = LifecycleStorageFixture::start().await;
        let session_id = SessionId::new_v4();
        let registry = Arc::new(SessionRuntimeRegistry::default());
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let mut tasks = Vec::new();
        for (title, root_text) in [
            ("Concurrent title A", "concurrent root A"),
            ("Concurrent title B", "concurrent root B"),
        ] {
            let controller = fixture.controller.clone();
            let registry = Arc::clone(&registry);
            let barrier = Arc::clone(&barrier);
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                let result = create_and_register_root_session_v1(
                    &controller,
                    &registry,
                    CreateRootSessionV1Request {
                        session_id,
                        project_id: String::from("project-explicit"),
                        title: title.to_string(),
                        root_submission: ClientInputSubmission::new(
                            ClientActionId::try_new(format!(
                                "concurrent-root-{session_id}-{title}"
                            ))
                            .expect("concurrent root client action id"),
                            root_text,
                            Vec::new(),
                        ),
                        now_ms: 1_787_788_800_030,
                    },
                )
                .await;
                (title, root_text, result)
            }));
        }

        // winner records the only payload allowed to reach disk and SQLite;
        // loser_payload is later used to prove no pre-index overwrite occurred.
        // winner 记录唯一允许到达磁盘和 SQLite 的载荷；loser_payload 随后用于证明索引前未发生覆盖。
        let mut winner = None;
        let mut loser_payload = None;
        for task in tasks {
            let (title, root_text, result) = task.await.expect("join concurrent creator");
            match result {
                Ok(handle) => {
                    assert!(winner.replace((title, root_text, handle)).is_none());
                }
                Err(SessionV1LifecycleError::Runtime(
                    SessionRuntimeError::SessionRegistrationInProgress(actual)
                    | SessionRuntimeError::DuplicateSession(actual),
                )) if actual == session_id => {
                    assert!(loser_payload.replace((title, root_text)).is_none());
                }
                Err(error) => panic!("unexpected concurrent creation error: {error}"),
            }
        }
        let (winner_title, winner_root, winner_handle) = winner.expect("one creation winner");
        let (loser_title, loser_root) = loser_payload.expect("one creation loser");
        assert!(Arc::ptr_eq(
            &winner_handle,
            &require_registered_session_v1(&registry, session_id)
                .expect("winner is the sole registered handle")
        ));

        let rows = vulcan_workspace_store::workspace_store::list_active_sessions_v1(
            &fixture.controller,
            "project-explicit",
        )
        .await
        .expect("list exact project sessions");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, winner_title);
        assert_ne!(rows[0].title, loser_title);
        let locator = locate_session_v1_storage(&fixture.controller, &session_id.to_string())
            .await
            .expect("locate concurrent creation winner");
        let loaded = load_session_v1_tail(&locator).expect("load concurrent creation winner");
        assert_eq!(loaded.metadata.title, winner_title);
        assert_ne!(loaded.metadata.title, loser_title);
        assert!(matches!(
            &loaded.document.events()[0].event,
            EventBody::UserInput(input) if input.text == winner_root
        ));
        assert!(!loaded.document.events().iter().any(|event| {
            matches!(&event.event, EventBody::UserInput(input) if input.text == loser_root)
        }));
        fixture.shutdown().await;
    }

    /// Verifies a failed SQLite projection never rolls back durable metadata and an exact retry does not create a second revision.
    /// 验证 SQLite 投影失败绝不会回滚可靠 metadata，且精确重试不会创建第二个修订。
    #[tokio::test]
    async fn lifecycle_projection_failure_retries_from_durable_metadata_without_revision_drift() {
        // fixture owns the real controller and fault-injection binding.
        // fixture 持有真实控制器与故障注入绑定。
        let fixture = LifecycleStorageFixture::start().await;
        // registry owns the one mutable in-process document.
        // registry 持有唯一可变进程内文档。
        let registry = SessionRuntimeRegistry::default();
        // session_id identifies the isolated lifecycle fixture.
        // session_id 标识隔离生命周期夹具。
        let session_id = SessionId::from_u128(0x71_01);
        // handle observes the authoritative revision across the failed projection.
        // handle 跨越失败投影观察权威修订。
        let handle = create_fixture_session(&fixture, &registry, session_id).await;
        vulcan_workspace_store::local_sql::execute_script_async(
            &fixture.controller.local,
            vulcan_workspace_store::local_database_runtime::LogicalDatabase::AppState,
            r#"
CREATE TRIGGER fail_session_v1_lifecycle_projection
BEFORE UPDATE OF title ON session_index_v1
BEGIN
    SELECT RAISE(ABORT, 'injected lifecycle projection failure');
END;
"#,
            Vec::new(),
        )
        .await
        .expect("install lifecycle projection failure trigger");

        // first_error proves the metadata flush completed before SQLite failed.
        // first_error 证明 metadata flush 在 SQLite 失败前已经完成。
        let first_error = rename_session_lifecycle_v1(
            &fixture.controller,
            &registry,
            session_id,
            String::from("Durable rename"),
            1_787_788_800_100,
        )
        .await
        .expect_err("SQLite projection should fail");
        assert!(matches!(
            first_error,
            SessionV1LifecycleError::IndexProjectionFailed {
                operation: "rename",
                ..
            }
        ));
        // failed_revision is the one durable domain revision that retry must preserve.
        // failed_revision 是重试必须保留的唯一可靠领域修订。
        let failed_revision = handle
            .read(|document| {
                assert_eq!(document.metadata.title, "Durable rename");
                document.metadata.document_revision
            })
            .expect("read durable metadata after projection failure");

        vulcan_workspace_store::local_sql::execute_script_async(
            &fixture.controller.local,
            vulcan_workspace_store::local_database_runtime::LogicalDatabase::AppState,
            "DROP TRIGGER fail_session_v1_lifecycle_projection;",
            Vec::new(),
        )
        .await
        .expect("remove lifecycle projection failure trigger");
        rename_session_lifecycle_v1(
            &fixture.controller,
            &registry,
            session_id,
            String::from("Durable rename"),
            1_787_788_800_100,
        )
        .await
        .expect("retry exact durable lifecycle projection");

        assert_eq!(
            handle
                .read(|document| document.metadata.document_revision)
                .expect("read revision after projection retry"),
            failed_revision
        );
        // rows proves the list index converged to the already-durable metadata.
        // rows 证明列表索引已收敛到已经可靠落盘的 metadata。
        let rows = list_active_sessions_v1(&fixture.controller, "project-explicit")
            .await
            .expect("list projected active session");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Durable rename");
        fixture.shutdown().await;
    }

    /// Verifies archive retry preserves the first durable timestamp and restore projects metadata back to the active list.
    /// 验证归档重试保留首次可靠时间戳，且恢复会把 metadata 重新投影到活动列表。
    #[tokio::test]
    async fn archive_retry_preserves_timestamp_and_restore_is_metadata_first() {
        // fixture isolates the authoritative files and SQLite projection.
        // fixture 隔离权威文件与 SQLite 投影。
        let fixture = LifecycleStorageFixture::start().await;
        // registry serializes both lifecycle operations for the session.
        // registry 为该会话串行化两个生命周期操作。
        let registry = SessionRuntimeRegistry::default();
        // session_id identifies the archived-and-restored fixture.
        // session_id 标识归档后恢复的夹具。
        let session_id = SessionId::from_u128(0x71_02);
        create_fixture_session(&fixture, &registry, session_id).await;
        // archived_at_ms is the immutable timestamp that exact retries must reuse.
        // archived_at_ms 是精确重试必须复用的不可变时间戳。
        let archived_at_ms = 1_787_788_800_200;

        archive_session_lifecycle_v1(&fixture.controller, &registry, session_id, archived_at_ms)
            .await
            .expect("archive V1 session");
        archive_session_lifecycle_v1(
            &fixture.controller,
            &registry,
            session_id,
            1_787_788_800_900,
        )
        .await
        .expect("retry exact archive");
        // archived_rows proves retry did not replace the first archive timestamp.
        // archived_rows 证明重试没有替换首次归档时间戳。
        let archived_rows = list_archived_sessions_v1(&fixture.controller, "project-explicit")
            .await
            .expect("list archived V1 session");
        assert_eq!(archived_rows.len(), 1);
        assert_eq!(archived_rows[0].archived_at_ms, Some(archived_at_ms));

        restore_session_lifecycle_v1(
            &fixture.controller,
            &registry,
            session_id,
            1_787_788_801_000,
        )
        .await
        .expect("restore V1 session");
        // active_rows proves SQLite follows the restored durable metadata.
        // active_rows 证明 SQLite 跟随恢复后的可靠 metadata。
        let active_rows = list_active_sessions_v1(&fixture.controller, "project-explicit")
            .await
            .expect("list restored active session");
        assert_eq!(active_rows.len(), 1);
        assert_eq!(active_rows[0].archived_at_ms, None);
        assert!(
            list_archived_sessions_v1(&fixture.controller, "project-explicit")
                .await
                .expect("list archived sessions after restore")
                .is_empty()
        );
        fixture.shutdown().await;
    }

    /// Verifies restore removes a stale SQLite row when the exact metadata file is missing and never scans for another copy.
    /// 验证精确 metadata 文件缺失时恢复会删除陈旧 SQLite 记录，并且绝不扫描其他副本。
    #[tokio::test]
    async fn restore_missing_metadata_removes_stale_index_and_reports_failure() {
        // fixture owns the exact indexed directory used by the missing-file scenario.
        // fixture 持有缺失文件场景使用的精确索引目录。
        let fixture = LifecycleStorageFixture::start().await;
        // registry owns the root handle until archive detaches it.
        // registry 持有根句柄直到归档将其移除。
        let registry = SessionRuntimeRegistry::default();
        // session_id identifies the corrupted archived fixture.
        // session_id 标识损坏的归档夹具。
        let session_id = SessionId::from_u128(0x71_03);
        // handle retains the immutable locator needed to remove only the exact metadata file.
        // handle 保留只删除精确 metadata 文件所需的不可变定位器。
        let handle = create_fixture_session(&fixture, &registry, session_id).await;
        let locator = handle.storage_locator().expect("root storage locator");
        archive_session_lifecycle_v1(
            &fixture.controller,
            &registry,
            session_id,
            1_787_788_800_300,
        )
        .await
        .expect("archive missing-metadata fixture");
        // metadata_path is composed from the indexed relative path without directory search.
        // metadata_path 由索引相对路径直接拼接，不执行目录搜索。
        let metadata_path = fixture
            .controller
            .data_root
            .join(SESSION_V1_STORAGE_PARTITION)
            .join(locator.relative_path())
            .join("metadata.json");
        std::fs::remove_file(&metadata_path).expect("remove exact metadata file");

        assert!(matches!(
            restore_session_lifecycle_v1(
                &fixture.controller,
                &registry,
                session_id,
                1_787_788_800_400,
            )
            .await,
            Err(SessionV1LifecycleError::RestoreStorageMissing {
                session_id: actual
            }) if actual == session_id
        ));
        assert!(
            list_archived_sessions_v1(&fixture.controller, "project-explicit")
                .await
                .expect("list after stale archived row cleanup")
                .is_empty()
        );
        assert!(
            list_active_sessions_v1(&fixture.controller, "project-explicit")
                .await
                .expect("list active after stale row cleanup")
                .is_empty()
        );
        fixture.shutdown().await;
    }
}
