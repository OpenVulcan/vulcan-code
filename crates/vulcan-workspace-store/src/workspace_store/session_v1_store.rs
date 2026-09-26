//! Narrow V1 session-storage facade for service-layer ownership.
//! 面向服务层所有权的 V1 会话存储窄门面。
//!
//! The facade is the only public path from an exact indexed or parent-derived
//! locator to physical V1 storage. Callers retain an opaque locator and can
//! create, tail-load, or flush one session without constructing
//! `SessionV1Layout`, assembling a `FlushRequest`, scanning directories, or
//! understanding file names.
//! 本门面是从精确索引定位器或父级派生定位器进入 V1 物理存储的唯一公共路径。调用方只保留
//! 不透明定位器，即可创建、尾段加载或刷新会话，无需构造 `SessionV1Layout`、拼装
//! `FlushRequest`、扫描目录或理解文件名规则。

use std::fmt;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};

use vulcan_session_core::v1::SessionDocument;
use vulcan_session_core::v1::config::MetadataDocument;
use vulcan_session_core::v1::config::SessionConfig;
use vulcan_session_core::v1::context::{CompactionNode, RuntimeLock};
use vulcan_session_core::v1::document::{FlushCandidate, GenerationBuildOutput};
use vulcan_session_core::v1::event::ConversationEvent;
use vulcan_session_core::v1::ids::Seq;

use crate::data_plane::HostRuntimeHandle;

pub use super::session_index_v1::{SessionIndexV1ConflictField, SessionIndexV1Error};
pub use super::session_v1::errors::{
    ErrorCode as SessionV1StorageErrorCode, SessionV1Error as SessionV1StorageError,
};

use super::session_index_v1::{
    SessionIndexV1Row, compose_relative_path, delete_session_index_v1,
    ensure_session_index_v1_schema, insert_session_index_v1, list_active_session_index_v1,
    list_archived_session_index_v1, locate_session_index_v1, project_session_lifecycle_index_v1,
    set_session_index_unread_v1, validate_new_session_index_v1, validate_project_partition,
    validate_relative_path,
};
use super::session_v1::event_window::{
    EventWindowReadReport, load_provider_event_range, load_recent_turn_window,
    load_turn_window_before,
};
use super::session_v1::generation::{PublishEditGenerationRequest, publish_edit_generation};
use super::session_v1::layout::SessionV1Layout;
use super::session_v1::loader::{load_session, load_session_tail_only, load_validated_metadata};
use super::session_v1::writer::{
    create_session, flush_candidate, verify_existing_session_creation,
};

mod index_probe;
mod lifecycle_tree;
pub(super) use index_probe::inspect as inspect_workspace_session_rows;

/// Fixed parent-owned directory containing every embedded task-session V1 tree.
/// 保存全部内嵌任务会话 V1 目录树的固定父级目录。
const EMBEDDED_TASK_PARTITION: &str = "TASK";

/// Fixed deterministic bucket used to derive `TASK/task-{session_id}` without scanning.
/// 用于无扫描派生 `TASK/task-{session_id}` 的固定确定性目录桶。
const EMBEDDED_TASK_BUCKET: &str = "task";

/// Metadata-only recovery selection from one atomic metadata publication.
/// 从一次原子元数据发布取得的恢复筛选快照。
/// This snapshot locates work; callers must validate the canonical document before mutation.
/// 此快照用于定位待办；修改前调用方必须校验规范文档。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionV1CollaborationRecoverySnapshot {
    /// Physical generation read together with the collaboration records.
    /// 与协作记录一起读取的物理代次。
    pub segment_generation: u64,
    /// Typed collaboration records belonging to that same metadata publication.
    /// 属于同一次元数据发布的强类型协作记录。
    pub collaboration: vulcan_session_core::v1::collaboration::CollaborationState,
}

/// Fixed V1 session partition below the application data root.
/// 应用数据根目录下固定的 V1 会话分区。
pub const SESSION_V1_STORAGE_PARTITION: &str = "sessions";

/// One failure crossing the V1 index and physical-storage boundary.
/// 一条跨越 V1 索引与物理存储边界的失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionV1StoreError {
    /// SQLite locator failure.
    /// SQLite 定位索引失败。
    Index(SessionIndexV1Error),
    /// Strict physical-storage failure.
    /// 严格物理存储失败。
    Storage(SessionV1StorageError),
}

impl fmt::Display for SessionV1StoreError {
    /// Formats one V1 facade failure without discarding its source category.
    /// 格式化一条 V1 门面失败且不丢弃其来源类别。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Index(error) => write!(formatter, "{error}"),
            Self::Storage(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for SessionV1StoreError {
    /// Returns the exact lower-layer error.
    /// 返回精确的下层错误。
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Index(error) => Some(error),
            Self::Storage(error) => Some(error),
        }
    }
}

impl From<SessionIndexV1Error> for SessionV1StoreError {
    /// Preserves one index failure at the facade boundary.
    /// 在门面边界保留一条索引失败。
    fn from(error: SessionIndexV1Error) -> Self {
        Self::Index(error)
    }
}

impl From<SessionV1StorageError> for SessionV1StoreError {
    /// Preserves one physical-storage failure at the facade boundary.
    /// 在门面边界保留一条物理存储失败。
    fn from(error: SessionV1StorageError) -> Self {
        Self::Storage(error)
    }
}

/// Opaque exact locator retained by one service-owned session handle.
/// 由服务层会话句柄保留的不透明精确定位器。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionV1StorageLocator {
    /// Cross-system session identity validated by the locator owner.
    /// 由定位器 owner 校验的跨系统会话身份。
    session_id: String,
    /// Owning project identity shared by indexed roots and embedded task sessions.
    /// 索引根会话与内嵌任务会话共享的所属项目身份。
    project_id: String,
    /// Exact relative path retained only for diagnostics and parent-child derivation.
    /// 仅用于诊断与父子派生的精确相对路径。
    relative_path: String,
    /// Exact project resource directory inherited by every embedded child session.
    /// 每个内嵌子会话继承的精确项目资源目录。
    project_directory: PathBuf,
    /// Exact physical layout derived and validated inside this crate.
    /// 在本 crate 内派生并校验的精确物理布局。
    layout: SessionV1Layout,
}

impl SessionV1StorageLocator {
    /// Checks the exact metadata file without parsing it or changing storage.
    /// 检查精确元数据文件，不解析或修改存储。
    /// Returns false only for a missing path; malformed file types and I/O failures remain errors.
    /// 仅路径不存在时返回假；异常文件类型及读写失败仍返回错误。
    pub fn metadata_exists(&self) -> Result<bool, SessionV1StoreError> {
        exact_metadata_file_exists(self)
    }

    /// Reads only the exact metadata file, validating its contract and locator identity.
    /// 仅读取精确 metadata 文件，校验契约及定位器身份。
    /// Returns metadata without reading runtime, compactions, or interaction segments.
    /// 返回元数据，不读取运行时、压缩文档或交互分片。
    fn read_indexed_metadata(&self) -> Result<MetadataDocument, SessionV1StoreError> {
        // Metadata is the sole configuration authority; event replay is deliberately excluded.
        // metadata 是唯一配置权威；此处刻意排除事件重放。
        let metadata = super::session_v1::documents::read_metadata(&self.layout.metadata_path())?;
        if metadata.session_id != self.session_id {
            return Err(SessionIndexV1Error::InvalidLocator {
                session_id: Some(self.session_id.clone()),
                detail: String::from("V1 configuration metadata owner does not match its locator"),
            }
            .into());
        }
        Ok(metadata)
    }

    /// Reads validated collaboration state for recovery selection without loading interaction history.
    /// 读取已校验协作状态用于恢复筛选，不加载交互历史。
    /// The exact locator owns the file; callers must serialize this read with lifecycle replacement.
    /// 精确定位器拥有该文件；调用方必须将此读取与生命周期替换串行化。
    /// Returns only typed metadata; actual repair must load and revalidate the canonical document.
    /// 仅返回强类型元数据；实际修复必须加载并重新校验规范文档。
    pub fn read_collaboration_recovery_state(
        &self,
    ) -> Result<vulcan_session_core::v1::collaboration::CollaborationState, SessionV1StoreError>
    {
        Ok(self.read_collaboration_recovery_snapshot()?.collaboration)
    }

    /// Reads generation and collaboration together without opening interaction or runtime files.
    /// 一起读取代次与协作记录，不打开交互或运行时文件。
    /// The locator supplies ownership; returns a selection hint or an explicit metadata error.
    /// 定位器提供归属；返回筛选线索或明确元数据错误。
    pub fn read_collaboration_recovery_snapshot(
        &self,
    ) -> Result<SessionV1CollaborationRecoverySnapshot, SessionV1StoreError> {
        // An unpublished directory is missing; an existing directory with absent metadata is corrupt.
        // 尚未发布的目录属于缺失；已有目录缺少元数据则属于损坏。
        // Preserve IO failures so permission errors cannot authorize a recovery publication.
        // 保留 IO 失败，避免权限错误授权恢复发布。
        let directory = self.session_directory();
        if !directory
            .try_exists()
            .map_err(|error| SessionV1StorageError::io(&directory, error.to_string()))?
        {
            return Err(SessionV1StorageError::session_missing(&self.session_id, directory).into());
        }
        // Metadata is a work-selection hint, never authorization to execute or settle a message.
        // 元数据是待办筛选线索，绝不是执行或结算消息的授权。
        let metadata = self.read_indexed_metadata()?;
        if !(1..=vulcan_session_core::v1::ids::JS_SAFE_INTEGER_MAX)
            .contains(&metadata.segment_generation)
        {
            return Err(SessionV1StorageError::corrupt(
                SessionV1StorageErrorCode::MetadataCorrupt,
                self.layout.metadata_path(),
                "collaboration recovery generation is invalid",
            )
            .into());
        }
        metadata.collaboration.validate().map_err(|error| {
            SessionV1StorageError::corrupt(
                SessionV1StorageErrorCode::MetadataCorrupt,
                self.layout.metadata_path(),
                format!("collaboration recovery metadata: {error}"),
            )
        })?;
        Ok(SessionV1CollaborationRecoverySnapshot {
            segment_generation: metadata.segment_generation,
            collaboration: metadata.collaboration.state,
        })
    }

    /// Reads the capability configuration's metadata and validates its own selection subtree.
    /// 读取能力配置元数据，并校验其自有选择子树。
    /// Returns validated metadata without loading any interaction data.
    /// 返回已校验元数据，不加载任何交互数据。
    fn read_configuration_metadata(&self) -> Result<MetadataDocument, SessionV1StoreError> {
        // Capability validation stays scoped to capability access, not model restoration.
        // 能力校验限定在能力访问中，不与模型恢复耦合。
        let metadata = self.read_indexed_metadata()?;
        metadata
            .config
            .selection
            .provider_tools
            .validate()
            .map_err(|error| {
                SessionV1StorageError::corrupt(
                    SessionV1StorageErrorCode::MetadataCorrupt,
                    self.layout.metadata_path(),
                    format!("provider tool configuration: {error}"),
                )
            })?;
        Ok(metadata)
    }

    /// Reads the exact active and pending request configuration from metadata only.
    /// 仅从 metadata 读取精确的活动及待生效请求配置。
    /// Returns both request states without replaying Turns, repairing history, or writing files.
    /// 返回两种请求状态，不重放回合、不修复历史，也不写入文件。
    pub fn read_request_configuration(
        &self,
    ) -> Result<
        (
            vulcan_session_core::v1::config::ActiveRequestConfig,
            Option<vulcan_session_core::v1::config::PendingRequestConfig>,
        ),
        SessionV1StoreError,
    > {
        // The metadata owner is validated before exposing its session-local route.
        // 暴露会话自有路由前先校验 metadata 所有者。
        let metadata = self.read_indexed_metadata()?;
        Ok((
            metadata.config.active_request,
            metadata.config.pending_request,
        ))
    }

    /// Reads session-owned capability settings without constructing a runtime session.
    /// 读取会话自有能力设置，不构建运行时会话。
    /// Returns the persisted selection; inherited defaults are resolved by the caller in memory.
    /// 返回持久化选择；继承默认值由调用方在内存中解析。
    pub fn read_provider_tool_selection(
        &self,
    ) -> Result<vulcan_session_core::v1::config::ProviderToolSelectionConfig, SessionV1StoreError>
    {
        Ok(self
            .read_configuration_metadata()?
            .config
            .selection
            .provider_tools)
    }

    /// Atomically changes capability settings for an unloaded session under its registry gate.
    /// 在注册表门控内原子修改尚未加载会话的能力设置。
    /// `selection` is canonical; `now_ms` is the nonnegative mutation timestamp.
    /// `selection` 为规范选择；`now_ms` 为非负修改时间戳。
    /// Returns whether metadata changed, preserving all unrelated fields and interaction files.
    /// 返回元数据是否变化，保留全部无关字段和交互文件。
    /// The caller must exclude physical loading and all other writes for this session.
    /// 调用方必须排除该会话的物理加载及其他写入。
    pub fn save_unloaded_provider_tool_selection(
        &self,
        selection: vulcan_session_core::v1::config::ProviderToolSelectionConfig,
        now_ms: i64,
    ) -> Result<bool, SessionV1StoreError> {
        // Validate even a no-op call; idempotence must not bypass the timestamp contract.
        // 即便无操作调用也必须校验；幂等不能绕过时间戳契约。
        if now_ms < 0 {
            return Err(SessionV1StorageError::corrupt(
                SessionV1StorageErrorCode::MetadataCorrupt,
                self.layout.metadata_path(),
                "configuration timestamp must be nonnegative",
            )
            .into());
        }
        selection.validate().map_err(|error| {
            SessionV1StorageError::corrupt(
                SessionV1StorageErrorCode::MetadataCorrupt,
                self.layout.metadata_path(),
                format!("provider tool configuration: {error}"),
            )
        })?;
        // Read within the exclusive gate, never overwrite a metadata snapshot captured earlier.
        // 在独占门控内读取，绝不覆盖此前捕获的旧元数据快照。
        let mut metadata = self.read_configuration_metadata()?;
        if metadata.config.selection.provider_tools == selection {
            return Ok(false);
        }
        metadata.config.selection.provider_tools = selection;
        self.commit_provider_tool_metadata(metadata, now_ms)
    }

    /// Rebuilds only capability settings with `selection` at `now_ms` under exclusive admission.
    /// 在独占准入内于 `now_ms` 使用 `selection` 仅重建能力设置。
    /// Returns a durable metadata change without reading interaction files or accepting other corruption.
    /// 返回可靠元数据变更，不读取交互文件，也不放行其他损坏。
    pub fn rebuild_unloaded_provider_tool_selection(
        &self,
        selection: vulcan_session_core::v1::config::ProviderToolSelectionConfig,
        now_ms: i64,
    ) -> Result<bool, SessionV1StoreError> {
        let metadata = super::session_v1::documents::read_metadata_for_provider_tool_rebuild(
            &self.layout.metadata_path(),
            &selection,
        )?;
        self.commit_provider_tool_metadata(metadata, now_ms)
    }

    /// Commits a capability-only candidate; `metadata` must come from this locator's guarded read.
    /// 提交仅能力变更候选；`metadata` 必须来自本定位器的受保护读取。
    /// Validates ownership, selection and `now_ms`; returns true after atomic publication.
    /// 校验归属、选择及 `now_ms`；原子发布后返回 true。
    fn commit_provider_tool_metadata(
        &self,
        mut metadata: MetadataDocument,
        now_ms: i64,
    ) -> Result<bool, SessionV1StoreError> {
        if metadata.session_id != self.session_id {
            return Err(SessionIndexV1Error::InvalidLocator {
                session_id: Some(self.session_id.clone()),
                detail: String::from("V1 configuration metadata owner does not match its locator"),
            }
            .into());
        }
        metadata
            .config
            .selection
            .provider_tools
            .validate()
            .map_err(|error| {
                SessionV1StorageError::corrupt(
                    SessionV1StorageErrorCode::MetadataCorrupt,
                    self.layout.metadata_path(),
                    format!("provider tool configuration: {error}"),
                )
            })?;
        if now_ms < 0 {
            return Err(SessionV1StorageError::corrupt(
                SessionV1StorageErrorCode::MetadataCorrupt,
                self.layout.metadata_path(),
                "configuration timestamp must be nonnegative",
            )
            .into());
        }
        metadata.document_revision =
            metadata.document_revision.checked_add(1).ok_or_else(|| {
                SessionV1StorageError::corrupt(
                    SessionV1StorageErrorCode::MetadataCorrupt,
                    self.layout.metadata_path(),
                    "configuration document revision overflow",
                )
            })?;
        metadata.updated_at_ms = metadata.updated_at_ms.max(now_ms);
        super::session_v1::documents::write_json_document(
            &self.layout.metadata_path(),
            &metadata,
            "metadata",
        )?;
        Ok(true)
    }

    /// Returns the cross-system session id.
    /// 返回跨系统会话 ID。
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Returns the owning project id used by the lightweight list index.
    /// 返回轻量列表索引使用的所属项目 ID。
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// Returns the exact relative path for diagnostics only.
    /// 返回仅用于诊断的精确相对路径。
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    /// Returns the exact project directory that owns this session's resources.
    /// 返回持有该会话资源的精确项目目录。
    ///
    /// Indexed roots derive this path from SQLite while embedded task sessions inherit it from
    /// their exact parent locator; neither path scans storage.
    /// 索引根会话从 SQLite 派生该路径，内嵌任务会话从精确父定位器继承；两者都不扫描存储目录。
    pub fn project_directory(&self) -> PathBuf {
        self.project_directory.clone()
    }

    /// Returns the exact published V1 session directory.
    /// 返回精确的已发布 V1 会话目录。
    ///
    /// The directory is derived from the validated SQLite locator for indexed roots or from the
    /// validated parent locator for embedded child sessions. Callers must not append legacy
    /// session metadata, state documents, or transcript segments beneath this path.
    /// 该目录由已校验的 SQLite 根会话定位器或已校验的父定位器派生。调用方不得在此路径下
    /// 追加旧会话 metadata、state 文档或转录分段。
    pub fn session_directory(&self) -> PathBuf {
        self.layout.session_dir()
    }

    /// Returns the exact session-scoped Provider API trace directory.
    /// 返回精确的会话级 Provider API 追踪目录。
    ///
    /// The path is derived only from this validated locator and never discovered by scanning.
    /// 该路径仅由当前已校验定位器派生，绝不通过目录扫描发现。
    pub fn provider_api_trace_directory(&self) -> PathBuf {
        self.layout.trace_dir()
    }

    /// Resolves the exact V1-owned VMM document after validating the session metadata.
    /// 在校验会话 metadata 后解析由 V1 持有的精确 VMM 文档。
    ///
    /// # Returns
    /// 返回值
    /// The exact `vmm.json` path only when this locator still addresses a valid V1 session.
    /// 仅当该定位器仍指向有效 V1 会话时返回精确的 `vmm.json` 路径。
    pub(super) fn validated_vmm_document_path(&self) -> Result<PathBuf, SessionV1StoreError> {
        let metadata = load_validated_metadata(&self.layout)?;
        if metadata.session_id != self.session_id {
            return Err(SessionIndexV1Error::InvalidLocator {
                session_id: Some(self.session_id.clone()),
                detail: format!(
                    "V1 metadata owner mismatch: locator {}, metadata {}",
                    self.session_id, metadata.session_id
                ),
            }
            .into());
        }
        Ok(self.layout.session_dir().join("vmm.json"))
    }

    /// Reads the exact persisted event-window frontier without scanning a
    /// session directory.
    /// 在不扫描会话目录的情况下读取精确持久化事件窗口前沿。
    ///
    /// # Returns
    /// 返回值
    /// Returns the current segment generation, next seq, and open-Turn cursor
    /// validated from the metadata addressed by this SQLite locator.
    /// 返回从该 SQLite 定位器指向的 metadata 校验出的当前分片代次、下一 seq 与
    /// 开放 Turn 游标。
    pub fn event_window_state(&self) -> Result<SessionV1EventWindowState, SessionV1StoreError> {
        let metadata = load_validated_metadata(&self.layout)?;
        Ok(SessionV1EventWindowState {
            segment_generation: metadata.segment_generation,
            document_revision: metadata.document_revision,
            next_seq: metadata.id_frontier.next_seq,
            open_turn_start_seq: metadata.open_turn_start_seq,
            config: metadata.config,
        })
    }

    /// Loads the latest complete-turn event window through this exact SQLite
    /// locator.
    /// 通过该精确 SQLite 定位器加载最近的完整回合事件窗口。
    ///
    /// # Parameters
    /// 参数
    /// `turn_limit` is the maximum number of complete logical turns selected by
    /// the storage reader while preserving an entire open Turn.
    /// `turn_limit` 是存储读取器选择的最大完整逻辑回合数，同时保留整个开放 Turn。
    ///
    /// # Returns
    /// 返回值
    /// Returns one contiguous window plus its generation and pagination state.
    /// 返回一个连续窗口及其代次与分页状态。
    pub fn load_recent_event_window(
        &self,
        turn_limit: usize,
    ) -> Result<SessionV1TurnEventWindow, SessionV1StoreError> {
        let window = load_recent_turn_window(&self.layout, turn_limit)?;
        Ok(SessionV1TurnEventWindow {
            state: SessionV1EventWindowState::from(window.state),
            events: window.events,
            older_before_seq: window.older_before_seq,
            report: window.report,
        })
    }

    /// Loads one immediately adjacent historical complete-turn page.
    /// 加载一页紧邻的更早完整回合历史。
    ///
    /// # Parameters
    /// 参数
    /// `before_seq` is the exclusive cursor returned by the newer window and
    /// `turn_limit` is the maximum complete-Turn count.
    /// `before_seq` 是较新窗口返回的排他游标，`turn_limit` 是最大完整 Turn 数量。
    ///
    /// # Returns
    /// 返回值
    /// Returns one contiguous historical page resolved only through
    /// `metadata.split_list`.
    /// 返回一页仅通过 `metadata.split_list` 解析的连续历史记录。
    pub fn load_event_window_before(
        &self,
        before_seq: Seq,
        turn_limit: usize,
    ) -> Result<SessionV1TurnEventWindow, SessionV1StoreError> {
        let window = load_turn_window_before(&self.layout, before_seq, turn_limit)?;
        Ok(SessionV1TurnEventWindow {
            state: SessionV1EventWindowState::from(window.state),
            events: window.events,
            older_before_seq: window.older_before_seq,
            report: window.report,
        })
    }

    /// Loads one exact inclusive event seq range without applying Provider or
    /// display visibility filtering.
    /// 加载一个不应用 Provider 或显示可见性过滤的精确闭区间事件 seq 范围。
    ///
    /// # Parameters
    /// 参数
    /// `from_seq` and `through_seq` are inclusive persisted stream boundaries.
    /// `from_seq` 与 `through_seq` 是两端均包含的持久化流边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns every event in exact seq order from only intersecting segments.
    /// 返回仅从相交分片读取、按精确 seq 排列的全部事件。
    pub fn load_event_range(
        &self,
        from_seq: Seq,
        through_seq: Seq,
    ) -> Result<SessionV1EventRange, SessionV1StoreError> {
        let range = load_provider_event_range(&self.layout, from_seq, through_seq)?;
        Ok(SessionV1EventRange {
            state: SessionV1EventWindowState::from(range.state),
            events: range.events,
            report: range.report,
        })
    }
}

/// Persisted generation and stream frontier of one exactly located V1 session.
/// 一个精确定位 V1 会话的持久化代次与流前沿。
#[derive(Debug, Clone, PartialEq)]
pub struct SessionV1EventWindowState {
    /// Current persisted event-tree generation.
    /// 当前持久化事件树代次。
    pub segment_generation: u64,
    /// Exact document revision durably published with this metadata snapshot.
    /// 与该 metadata 快照一同可靠发布的精确文档修订号。
    pub document_revision: u64,
    /// Next seq that would be allocated by the persisted stream.
    /// 持久化流将分配的下一 seq。
    pub next_seq: u64,
    /// Exact start seq of the persisted open Turn, when any.
    /// 持久化开放 Turn 的精确起始 seq（如有）。
    pub open_turn_start_seq: Option<Seq>,
    /// Session request and compaction configuration from the same validated metadata.
    /// 来自同一份已校验 metadata 的会话请求与压缩配置。
    pub config: SessionConfig,
}

impl From<super::session_v1::event_window::EventWindowState> for SessionV1EventWindowState {
    /// Converts one storage-internal frontier captured by a physical read into
    /// the opaque-locator facade contract.
    /// 把一次物理读取捕获的存储内部前沿转换为不透明定位器门面契约。
    fn from(state: super::session_v1::event_window::EventWindowState) -> Self {
        Self {
            segment_generation: state.segment_generation,
            document_revision: state.document_revision,
            next_seq: state.next_seq,
            open_turn_start_seq: state.open_turn_start_seq,
            config: state.config,
        }
    }
}

/// One exact complete-turn read plus its persisted ownership boundary.
/// 一次精确完整回合读取及其持久化归属边界。
#[derive(Debug, Clone, PartialEq)]
pub struct SessionV1TurnEventWindow {
    /// Persisted generation and stream frontier read from metadata.
    /// 从 metadata 读取的持久化代次与流前沿。
    pub state: SessionV1EventWindowState,
    /// Events in exact ascending seq order.
    /// 按精确 seq 升序排列的事件。
    pub events: Vec<ConversationEvent>,
    /// Exclusive cursor for the immediately older complete-turn page.
    /// 紧邻更早完整回合分页的排他游标。
    pub older_before_seq: Option<Seq>,
    /// Precise segment-read metrics.
    /// 精确分片读取指标。
    pub report: EventWindowReadReport,
}

/// One exact event range plus its persisted ownership boundary.
/// 一个精确事件范围及其持久化归属边界。
#[derive(Debug, Clone, PartialEq)]
pub struct SessionV1EventRange {
    /// Persisted generation and stream frontier read from metadata.
    /// 从 metadata 读取的持久化代次与流前沿。
    pub state: SessionV1EventWindowState,
    /// Events in exact ascending seq order.
    /// 按精确 seq 升序排列的事件。
    pub events: Vec<ConversationEvent>,
    /// Precise segment-read metrics.
    /// 精确分片读取指标。
    pub report: EventWindowReadReport,
}

/// Borrowed inputs required to create one indexed V1 session.
/// 创建一个带索引的 V1 会话所需的借用输入。
#[derive(Debug)]
pub struct CreateSessionV1Request<'a> {
    /// Owning project id stored only in the lightweight index.
    /// 仅存于轻量索引中的所属项目 ID。
    pub project_id: &'a str,
    /// Relative storage partition below the host data root.
    /// 宿主数据根目录下的相对存储分区。
    pub storage_partition: &'a str,
    /// Human-readable date bucket used by the immutable directory name.
    /// 不可变目录名使用的人类可读日期桶。
    pub date_bucket: &'a str,
    /// Authoritative metadata candidate.
    /// 权威 metadata 候选。
    pub metadata: &'a MetadataDocument,
    /// Current runtime lock document.
    /// 当前运行时锁定文档。
    pub runtime_lock: &'a RuntimeLock,
    /// Current compaction nodes.
    /// 当前压缩节点。
    pub compactions: &'a [CompactionNode],
    /// Initial ordered event stream.
    /// 初始有序事件流。
    pub events: &'a [ConversationEvent],
}

/// Borrowed inputs required to create one non-indexed child TaskSession V1 tree.
/// 创建一个不进入索引的子 TaskSession V1 目录树所需的借用输入。
#[derive(Debug)]
pub struct CreateEmbeddedTaskSessionV1Request<'a> {
    /// Authoritative child metadata candidate.
    /// 权威子会话 metadata 候选。
    pub metadata: &'a MetadataDocument,
    /// Current child runtime lock document.
    /// 当前子会话运行时锁定文档。
    pub runtime_lock: &'a RuntimeLock,
    /// Current child compaction nodes.
    /// 当前子会话压缩节点。
    pub compactions: &'a [CompactionNode],
    /// Initial ordered child event stream.
    /// 初始有序子会话事件流。
    pub events: &'a [ConversationEvent],
}

/// Result returned only after files and the SQLite locator are both published.
/// 仅在文件与 SQLite 定位记录均发布后返回的结果。
#[derive(Debug)]
pub struct CreatedSessionV1 {
    /// Opaque locator for later load and flush operations.
    /// 供后续加载与刷新操作使用的不透明定位器。
    pub locator: SessionV1StorageLocator,
    /// Metadata exactly published by the physical writer.
    /// 物理写入器精确发布的 metadata。
    pub metadata: MetadataDocument,
}

/// Metrics reported by one tail-only session load.
/// 一次仅尾段会话加载报告的指标。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionV1LoadReport {
    /// Segment files actually opened and parsed.
    /// 实际打开并解析的分片文件数。
    pub opened_segment_files: u64,
    /// Events deserialized during the load.
    /// 加载期间反序列化的事件数。
    pub parsed_event_count: u64,
    /// Events retained in the returned document.
    /// 返回文档中常驻保留的事件数。
    pub resident_event_count: u64,
    /// Total sealed and active split-map entries.
    /// 已封口与活动分片映射条目总数。
    pub catalog_entries: u64,
}

/// One row-only V1 session list entry returned without opening a session directory.
/// 一条不打开会话目录即可返回的纯 V1 会话列表记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionV1ListEntry {
    /// Cross-system session id.
    /// 跨系统会话 ID。
    pub session_id: String,
    /// Owning project id.
    /// 所属项目 ID。
    pub project_id: String,
    /// Human-readable project partition.
    /// 人类可读项目分区。
    pub project_tag: String,
    /// Session display title.
    /// 会话显示标题。
    pub title: String,
    /// Creation time in Unix milliseconds.
    /// 创建时间（Unix 毫秒）。
    pub created_at_ms: i64,
    /// Last update time in Unix milliseconds.
    /// 最后更新时间（Unix 毫秒）。
    pub updated_at_ms: i64,
    /// Archive time when the row is archived.
    /// 记录归档时的归档时间。
    pub archived_at_ms: Option<i64>,
    /// Lightweight unread marker.
    /// 轻量未读标记。
    pub unread: i64,
}

impl From<SessionIndexV1Row> for SessionV1ListEntry {
    /// Projects one locator row into the list-only public contract.
    /// 把一条定位记录投影为仅列表使用的公开契约。
    fn from(row: SessionIndexV1Row) -> Self {
        Self {
            session_id: row.session_id,
            project_id: row.project_id,
            project_tag: row.project_tag,
            title: row.title,
            created_at_ms: row.created_at_ms,
            updated_at_ms: row.updated_at_ms,
            archived_at_ms: row.archived_at_ms,
            unread: row.unread,
        }
    }
}

/// Outcome of validating one archived V1 session before metadata-first restore.
/// 在 metadata 优先恢复前校验一个已归档 V1 会话的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionV1StoragePreflightOutcome {
    /// The exact metadata file exists and the authoritative restore may proceed.
    /// 精确 metadata 文件存在，权威恢复可以继续。
    Ready {
        /// Exact reusable storage locator.
        /// 可复用的精确存储定位器。
        locator: Box<SessionV1StorageLocator>,
    },
    /// The exact metadata file is missing and the stale SQLite row was removed.
    /// 精确 metadata 文件缺失且陈旧 SQLite 记录已删除。
    MissingStorageIndexRemoved {
        /// Session id whose stale row was removed.
        /// 陈旧记录已删除的会话 ID。
        session_id: String,
    },
}

/// Outcome of deleting one V1 session by its exact SQLite locator.
/// 按精确 SQLite 定位器删除一个 V1 会话的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteSessionV1Outcome {
    /// Whether an exact indexed row existed before deletion.
    /// 删除前是否存在精确索引记录。
    pub index_existed: bool,
    /// Whether the exact session directory existed and was removed.
    /// 精确会话目录是否存在并已删除。
    pub directory_removed: bool,
}

/// One exact tail-only load result.
/// 一次精确的仅尾段加载结果。
pub struct LoadedSessionV1 {
    /// Opaque locator reused by later flushes.
    /// 后续刷新复用的不透明定位器。
    pub locator: SessionV1StorageLocator,
    /// Authoritative in-memory session document.
    /// 权威内存会话文档。
    pub document: SessionDocument,
    /// Metadata loaded from the exact indexed directory.
    /// 从精确索引目录加载的 metadata。
    pub metadata: MetadataDocument,
    /// Tail-only load metrics.
    /// 仅尾段加载指标。
    pub report: SessionV1LoadReport,
}

/// Result of persisting one complete document-owned flush candidate.
/// 持久化一份由文档完整拥有的刷新候选的结果。
#[derive(Debug, Clone)]
pub struct SessionV1FlushResult {
    /// Number of files actually written in dependency order.
    /// 按依赖顺序实际写入的文件数。
    pub written_file_count: usize,
    /// Metadata exactly published by this flush, when changed.
    /// 本次刷新发生变化时精确发布的 metadata。
    pub published_metadata: Option<MetadataDocument>,
}

/// Resolves the exact project-level resource directory for one V1 partition.
/// 解析一个 V1 分区的精确项目级资源目录。
///
/// The function validates both relative components and never creates, scans,
/// or probes candidate directories. The resource catalog owner performs the
/// later exact-directory creation under its project mutex.
/// 该函数校验两个相对组件，绝不创建、扫描或探测候选目录。资源目录 owner 随后会在项目互斥锁下
/// 创建这个精确目录。
///
/// # Parameters / 参数
///
/// - `data_root`: Absolute application data root retained by the runtime handle.
///   `data_root`：宿主运行时保留的应用数据绝对根目录。
/// - `storage_partition`: Exact V1 storage partition `sessions` below `data_root`.
///   `storage_partition`：位于 `data_root` 下的精确 V1 存储分区 `sessions`。
/// - `project_partition`: Exact project partition stored in V1 metadata/index.
///   `project_partition`：V1 metadata/index 中保存的精确项目分区。
///
/// # Returns / 返回值
///
/// The absolute `data_root/storage_partition/project_partition` path.
/// 绝对的 `data_root/storage_partition/project_partition` 路径。
pub fn project_resource_directory_v1(
    data_root: &Path,
    storage_partition: &str,
    project_partition: &str,
) -> Result<PathBuf, SessionV1StoreError> {
    if !data_root.is_absolute() {
        return Err(SessionIndexV1Error::InvalidLocator {
            session_id: None,
            detail: format!(
                "configured V1 data root must be absolute: {}",
                data_root.display()
            ),
        }
        .into());
    }
    validate_production_storage_partition(storage_partition, None)?;
    validate_project_partition(project_partition).map_err(|detail| {
        SessionIndexV1Error::InvalidLocator {
            session_id: None,
            detail,
        }
    })?;
    Ok(data_root.join(storage_partition).join(project_partition))
}

/// Prepares the V1 lightweight locator table.
/// 准备 V1 轻量定位索引表。
pub async fn ensure_session_v1_storage_schema(
    host_runtime: &HostRuntimeHandle,
) -> Result<(), SessionV1StoreError> {
    ensure_session_index_v1_schema(host_runtime)
        .await
        .map_err(SessionV1StoreError::from)
}

/// Creates all V1 files first and publishes the strict SQLite row last.
/// 先创建全部 V1 文件，最后发布严格 SQLite 记录。
///
/// A failed index insertion intentionally leaves an unindexed directory. The
/// facade neither scans for it nor deletes, repairs, or imports it.
/// 索引插入失败时会有意保留未索引目录；本门面既不扫描它，也不删除、修复或导入它。
pub async fn create_indexed_session_v1(
    host_runtime: &HostRuntimeHandle,
    request: &CreateSessionV1Request<'_>,
) -> Result<CreatedSessionV1, SessionV1StoreError> {
    ensure_session_index_v1_schema(host_runtime).await?;
    let row = create_index_row(request)?;
    validate_new_session_index_v1(host_runtime, &row).await?;
    create_indexed_session_v1_with_insert(&host_runtime.data_root, request, row, |row| async move {
        insert_session_index_v1(host_runtime, &row).await
    })
    .await
}

/// Creates one exact non-indexed TaskSession below its parent V1 session directory.
/// 在父 V1 会话目录下创建一个精确且不进入索引的 TaskSession。
///
/// The child path is deterministically derived as `TASK/task-{child_session_id}` from the
/// already validated parent locator. The child inherits the parent's project identity and
/// project resource directory, never inserts a SQLite row, and never scans sibling directories.
/// 子路径由已校验父定位器确定性派生为 `TASK/task-{child_session_id}`。子会话继承父级项目
/// 身份与项目资源目录，绝不插入 SQLite 记录，也绝不扫描同级目录。
///
/// # Parameters / 参数
///
/// - `parent`: Exact published parent-session locator.
///   `parent`：已经发布的精确父会话定位器。
/// - `request`: Complete child-session creation candidate.
///   `request`：完整的子会话创建候选。
///
/// # Returns / 返回值
///
/// The exact child locator and metadata after the complete directory tree is published.
/// 完整目录树发布后的精确子定位器与 metadata。
pub fn create_embedded_task_session_v1(
    parent: &SessionV1StorageLocator,
    request: &CreateEmbeddedTaskSessionV1Request<'_>,
) -> Result<CreatedSessionV1, SessionV1StoreError> {
    let parent_metadata = load_validated_metadata(&parent.layout)?;
    if request.metadata.project_tag != parent_metadata.project_tag {
        return Err(SessionIndexV1Error::InvalidLocator {
            session_id: Some(request.metadata.session_id.clone()),
            detail: format!(
                "embedded task project_tag mismatch: expected {}, actual {}",
                parent_metadata.project_tag, request.metadata.project_tag
            ),
        }
        .into());
    }
    if request.metadata.session_id == parent.session_id {
        return Err(SessionIndexV1Error::InvalidLocator {
            session_id: Some(request.metadata.session_id.clone()),
            detail: "embedded task session_id must differ from its parent session_id".to_string(),
        }
        .into());
    }
    let locator = embedded_task_locator(parent, request.metadata.session_id.as_str())?;
    let verify_existing = || {
        verify_existing_session_creation(
            &locator.layout,
            request.metadata,
            request.runtime_lock,
            request.metadata.updated_at_ms,
            request.compactions,
            request.events,
        )
    };
    let metadata = if locator.layout.session_dir().exists() {
        verify_existing()?
    } else {
        match create_session(
            &locator.layout,
            request.metadata,
            request.runtime_lock,
            request.metadata.updated_at_ms,
            request.compactions,
            request.events,
        ) {
            Ok(metadata) => metadata,
            Err(_error) if locator.layout.session_dir().exists() => verify_existing()?,
            Err(error) => return Err(error.into()),
        }
    };
    Ok(CreatedSessionV1 { locator, metadata })
}

/// Derives one exact non-indexed TaskSession locator from its parent and stable child id.
/// 从父定位器与稳定子会话 ID 派生一个精确且不进入索引的 TaskSession 定位器。
///
/// # Parameters / 参数
///
/// - `parent`: Exact parent-session locator.
///   `parent`：精确父会话定位器。
/// - `child_session_id`: Stable child TaskSession identity.
///   `child_session_id`：稳定子 TaskSession 身份。
///
/// # Returns / 返回值
///
/// A deterministic locator that does not touch SQLite or inspect the filesystem.
/// 一个不接触 SQLite、不检查文件系统的确定性定位器。
pub fn locate_embedded_task_session_v1(
    parent: &SessionV1StorageLocator,
    child_session_id: &str,
) -> Result<SessionV1StorageLocator, SessionV1StoreError> {
    embedded_task_locator(parent, child_session_id).map_err(SessionV1StoreError::from)
}

/// Resolves one session id through SQLite without touching session directories.
/// 仅通过 SQLite 解析一个会话 ID，不接触会话目录。
pub async fn locate_session_v1_storage(
    host_runtime: &HostRuntimeHandle,
    session_id: &str,
) -> Result<SessionV1StorageLocator, SessionV1StoreError> {
    let row = locate_session_index_v1(host_runtime, session_id)
        .await?
        .ok_or_else(|| SessionIndexV1Error::Missing {
            session_id: session_id.to_string(),
        })?;
    storage_locator_from_row(&host_runtime.data_root, row).map_err(SessionV1StoreError::from)
}

/// Resolves one optional session id through SQLite without touching session directories.
/// 仅通过 SQLite 解析一个可选会话 ID，不接触会话目录。
///
/// # Parameters / 参数
/// `host_runtime` owns the exact list index and `session_id` is the stable cross-system identity.
/// `host_runtime` 持有精确列表索引，`session_id` 是稳定的跨系统身份。
///
/// # Returns / 返回值
/// Returns the validated locator when the row exists, `None` when it does not, or a structured
/// index failure. No directory scan, repair, or fallback participates.
/// 索引行存在时返回已校验定位器，不存在时返回 `None`，否则返回结构化索引失败；不执行目录扫描、修复或回退。
pub async fn locate_optional_session_v1_storage(
    host_runtime: &HostRuntimeHandle,
    session_id: &str,
) -> Result<Option<SessionV1StorageLocator>, SessionV1StoreError> {
    let Some(row) = locate_session_index_v1(host_runtime, session_id).await? else {
        return Ok(None);
    };
    storage_locator_from_row(&host_runtime.data_root, row)
        .map(Some)
        .map_err(SessionV1StoreError::from)
}

/// Lists active V1 sessions from SQLite rows only.
/// 只从 SQLite 记录列出活动 V1 会话。
pub async fn list_active_sessions_v1(
    host_runtime: &HostRuntimeHandle,
    project_id: &str,
) -> Result<Vec<SessionV1ListEntry>, SessionV1StoreError> {
    let rows = list_active_session_index_v1(host_runtime, project_id).await?;
    current_partition_list_entries(&host_runtime.data_root, rows)
}

/// Lists archived V1 sessions from SQLite rows only.
/// 只从 SQLite 记录列出归档 V1 会话。
pub async fn list_archived_sessions_v1(
    host_runtime: &HostRuntimeHandle,
    project_id: &str,
) -> Result<Vec<SessionV1ListEntry>, SessionV1StoreError> {
    let rows = list_archived_session_index_v1(host_runtime, project_id).await?;
    current_partition_list_entries(&host_runtime.data_root, rows)
}

/// Projects supported locators without silently hiding corrupt current-partition records.
/// 投影受支持定位器，不再静默隐藏当前分区的损坏记录。
/// Only the explicitly retired duplicate-root partition remains omitted, as documented.
/// 仅按文档约定继续排除明确退役的重复根分区。
fn current_partition_list_entries(
    data_root: &Path,
    rows: Vec<SessionIndexV1Row>,
) -> Result<Vec<SessionV1ListEntry>, SessionV1StoreError> {
    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        if row.storage_partition == "code/sessions" {
            continue;
        }
        storage_locator_from_row(data_root, row.clone())?;
        entries.push(SessionV1ListEntry::from(row));
    }
    Ok(entries)
}

/// Validates one V1 restore target without changing an existing SQLite lifecycle row.
/// 校验一个 V1 恢复目标，但不修改现有 SQLite 生命周期记录。
///
/// A missing file removes the stale row and returns a reportable outcome. No
/// directory scan, compatibility load, or automatic file repair is attempted.
/// 文件缺失时删除陈旧记录并返回可报告结果；不执行目录扫描、兼容加载或自动文件修复。
pub async fn preflight_session_v1_storage(
    host_runtime: &HostRuntimeHandle,
    session_id: &str,
) -> Result<SessionV1StoragePreflightOutcome, SessionV1StoreError> {
    let row = locate_session_index_v1(host_runtime, session_id)
        .await?
        .ok_or_else(|| SessionIndexV1Error::Missing {
            session_id: session_id.to_string(),
        })?;
    let locator = storage_locator_from_row(&host_runtime.data_root, row)?;
    if !exact_metadata_file_exists(&locator)? {
        delete_session_index_v1(host_runtime, session_id).await?;
        return Ok(
            SessionV1StoragePreflightOutcome::MissingStorageIndexRemoved {
                session_id: session_id.to_string(),
            },
        );
    }
    Ok(SessionV1StoragePreflightOutcome::Ready {
        locator: Box::new(locator),
    })
}

/// Deletes one V1 session directory first and then removes its SQLite row.
/// 先删除一个 V1 会话目录，再删除其 SQLite 记录。
///
/// A missing row or directory is already deleted. The function never searches
/// for an alternative directory and never touches project resources.
/// 缺失记录或目录视为已经删除；本函数绝不搜索替代目录，也绝不触碰项目资源。
pub async fn delete_session_v1(
    host_runtime: &HostRuntimeHandle,
    session_id: &str,
) -> Result<DeleteSessionV1Outcome, SessionV1StoreError> {
    let Some(row) = locate_session_index_v1(host_runtime, session_id).await? else {
        return Ok(DeleteSessionV1Outcome {
            index_existed: false,
            directory_removed: false,
        });
    };
    let locator = storage_locator_from_row(&host_runtime.data_root, row)?;
    let directory_removed = remove_exact_session_directory(&locator)?;
    delete_session_index_v1(host_runtime, session_id).await?;
    Ok(DeleteSessionV1Outcome {
        index_existed: true,
        directory_removed,
    })
}

/// Updates the lightweight unread projection for one exact indexed V1 session.
/// 更新一个精确索引 V1 会话的轻量未读投影。
///
/// # Parameters / 参数
///
/// - `host_runtime`: Host runtime runtime owning the application SQLite binding.
///   `host_runtime`：持有应用 SQLite 绑定的 Host runtime 运行时。
/// - `session_id`: Stable session identity whose list projection is updated.
///   `session_id`：其列表投影需要更新的稳定会话身份。
/// - `unread`: Exact unread state to persist.
///   `unread`：需要持久化的精确未读状态。
///
/// # Returns / 返回值
///
/// Success after the exact SQLite row accepts the projection.
/// 在精确 SQLite 记录接受投影后返回成功。
pub async fn set_session_unread_v1(
    host_runtime: &HostRuntimeHandle,
    session_id: &str,
    unread: bool,
) -> Result<(), SessionV1StoreError> {
    set_session_index_unread_v1(host_runtime, session_id, unread)
        .await
        .map_err(SessionV1StoreError::from)
}

/// Projects one already-durable metadata revision into the exact SQLite list row.
/// 把一份已经可靠落盘的 metadata 修订投影到精确 SQLite 列表记录。
///
/// # Parameters / 参数
///
/// - `host_runtime`: Host runtime runtime owning the application SQLite binding.
///   `host_runtime`：持有应用 SQLite 绑定的 Host runtime 运行时。
/// - `locator`: Immutable exact locator owned by the session handle.
///   `locator`：由会话句柄持有的不可变精确定位器。
/// - `metadata`: Authoritative metadata read after the V1 flush completed.
///   `metadata`：V1 flush 完成后读取的权威 metadata。
///
/// # Returns / 返回值
///
/// Success only after the exact row mirrors the durable title, update time, and archive state.
/// 仅在精确记录镜像可靠标题、更新时间与归档状态后返回成功。
pub async fn project_session_lifecycle_v1(
    host_runtime: &HostRuntimeHandle,
    locator: &SessionV1StorageLocator,
    metadata: &MetadataDocument,
) -> Result<(), SessionV1StoreError> {
    if metadata.session_id != locator.session_id {
        return Err(SessionIndexV1Error::InvalidLocator {
            session_id: Some(metadata.session_id.clone()),
            detail: format!(
                "metadata session_id {} does not match locator session_id {}",
                metadata.session_id, locator.session_id
            ),
        }
        .into());
    }
    project_session_lifecycle_index_v1(
        host_runtime,
        locator.session_id(),
        locator.relative_path(),
        &metadata.title,
        metadata.updated_at_ms,
        metadata.archived_at_ms,
    )
    .await
    .map_err(SessionV1StoreError::from)
}

/// Tail-loads one session from its already resolved exact locator.
/// 从已解析的精确定位器仅加载一个会话的尾段。
pub fn load_session_v1_tail(
    locator: &SessionV1StorageLocator,
) -> Result<LoadedSessionV1, SessionV1StoreError> {
    let (loaded, report) = load_session_tail_only(&locator.layout)?;
    Ok(LoadedSessionV1 {
        locator: locator.clone(),
        document: loaded.document,
        metadata: loaded.metadata,
        report: SessionV1LoadReport {
            opened_segment_files: report.opened_segment_files,
            parsed_event_count: report.parsed_event_count,
            resident_event_count: report.resident_event_count,
            catalog_entries: report.catalog_entries,
        },
    })
}

/// Reads a runtime candidate from `locator`, allowing only proven active-tail Goal ownership recovery.
/// 从 `locator` 读取运行时候选，仅允许已证实的活动尾段目标归属恢复。
/// Returns an unpublished document; the unique owner must flush dirty recovery before registration.
/// 返回尚未发布的文档；唯一 owner 必须在注册前 flush 待写恢复。
pub fn load_session_v1_for_runtime(
    locator: &SessionV1StorageLocator,
) -> Result<LoadedSessionV1, SessionV1StoreError> {
    let (loaded, report) =
        super::session_v1::loader::load_session_tail_for_runtime(&locator.layout)?;
    Ok(LoadedSessionV1 {
        locator: locator.clone(),
        metadata: loaded.metadata,
        document: loaded.document,
        report: SessionV1LoadReport {
            opened_segment_files: report.opened_segment_files,
            parsed_event_count: report.parsed_event_count,
            resident_event_count: report.resident_event_count,
            catalog_entries: report.catalog_entries,
        },
    })
}

/// Fully loads one session from its already resolved exact locator.
/// 从已解析的精确定位器完整加载一个会话。
///
/// # Parameters / 参数
/// `locator` is the opaque SQLite-validated physical owner.
/// `locator` 是经过 SQLite 校验的不透明物理 owner。
///
/// # Returns / 返回值
/// Returns every event plus exact load metrics; no directory scan or repair participates.
/// 返回全部事件与精确加载指标；不参与目录扫描或修复。
pub fn load_session_v1_full(
    locator: &SessionV1StorageLocator,
) -> Result<LoadedSessionV1, SessionV1StoreError> {
    let loaded = load_session(&locator.layout)?;
    let event_count = u64::try_from(loaded.document.events().len()).map_err(|_| {
        SessionV1StorageError::corrupt(
            SessionV1StorageErrorCode::MetadataCorrupt,
            locator.layout.metadata_path(),
            "full session event count overflows u64",
        )
    })?;
    let segment_count = u64::try_from(loaded.metadata.split_list.len()).map_err(|_| {
        SessionV1StorageError::corrupt(
            SessionV1StorageErrorCode::MetadataCorrupt,
            locator.layout.metadata_path(),
            "full session segment count overflows u64",
        )
    })?;
    Ok(LoadedSessionV1 {
        locator: locator.clone(),
        document: loaded.document,
        metadata: loaded.metadata,
        report: SessionV1LoadReport {
            opened_segment_files: segment_count,
            parsed_event_count: event_count,
            resident_event_count: event_count,
            catalog_entries: segment_count,
        },
    })
}

/// Publishes one Core-built in-place edit through the exact locator.
/// 通过精确定位器发布一份由 Core 构建的原位编辑。
///
/// # Parameters / 参数
/// `locator` owns the physical session, `current_metadata` is the exact loaded generation, and
/// `output` is the complete validated Core candidate.
/// `locator` 持有物理会话，`current_metadata` 是精确加载的 generation，`output` 是完整且已校验的
/// Core 候选。
///
/// # Returns / 返回值
/// Returns only after metadata atomically publishes both the event and context generation.
/// 仅在 metadata 原子发布事件与 context generation 后返回。
pub fn publish_edit_session_v1(
    locator: &SessionV1StorageLocator,
    current_metadata: &MetadataDocument,
    output: &GenerationBuildOutput,
) -> Result<MetadataDocument, SessionV1StoreError> {
    publish_edit_generation(
        &locator.layout,
        PublishEditGenerationRequest {
            current_metadata,
            output,
        },
    )
    .map_err(SessionV1StoreError::from)
}

/// Persists one complete `SessionDocument` flush candidate through the opaque locator.
/// 通过不透明定位器持久化一份完整的 `SessionDocument` 刷新候选。
pub fn flush_session_v1_candidate(
    locator: &SessionV1StorageLocator,
    candidate: &FlushCandidate,
) -> Result<SessionV1FlushResult, SessionV1StoreError> {
    let outcome = flush_candidate(&locator.layout, candidate)?;
    Ok(SessionV1FlushResult {
        written_file_count: outcome.written_files.len(),
        published_metadata: outcome.published_metadata,
    })
}

/// Creates one session with an injected final index publisher.
/// 使用注入的最终索引发布器创建一个会话。
///
/// The injection point exists only to prove publication order and failure
/// semantics without touching a real database in unit tests.
/// 该注入点仅用于在单元测试中无需触及真实数据库即可证明发布顺序与失败语义。
async fn create_indexed_session_v1_with_insert<F, Fut>(
    data_root: &Path,
    request: &CreateSessionV1Request<'_>,
    row: SessionIndexV1Row,
    insert: F,
) -> Result<CreatedSessionV1, SessionV1StoreError>
where
    F: FnOnce(SessionIndexV1Row) -> Fut,
    Fut: Future<Output = Result<(), SessionIndexV1Error>>,
{
    let locator = storage_locator_from_row(data_root, row.clone())?;
    let verify_existing = || {
        verify_existing_session_creation(
            &locator.layout,
            request.metadata,
            request.runtime_lock,
            request.metadata.updated_at_ms,
            request.compactions,
            request.events,
        )
    };
    let metadata = if locator.layout.session_dir().exists() {
        verify_existing()?
    } else {
        match create_session(
            &locator.layout,
            request.metadata,
            request.runtime_lock,
            request.metadata.updated_at_ms,
            request.compactions,
            request.events,
        ) {
            Ok(metadata) => metadata,
            Err(_error) if locator.layout.session_dir().exists() => verify_existing()?,
            Err(error) => return Err(error.into()),
        }
    };
    insert(row).await?;
    Ok(CreatedSessionV1 { locator, metadata })
}

/// Builds the exact lightweight row for one new session.
/// 为一个新会话构建精确的轻量索引记录。
fn create_index_row(
    request: &CreateSessionV1Request<'_>,
) -> Result<SessionIndexV1Row, SessionIndexV1Error> {
    if request.project_id.is_empty() {
        return Err(SessionIndexV1Error::InvalidLocator {
            session_id: Some(request.metadata.session_id.clone()),
            detail: "project_id must not be empty".to_string(),
        });
    }
    validate_production_storage_partition(
        request.storage_partition,
        Some(request.metadata.session_id.as_str()),
    )?;
    let relative_path = compose_relative_path(
        &request.metadata.project_tag,
        request.date_bucket,
        &request.metadata.session_id,
    )
    .map_err(|detail| SessionIndexV1Error::InvalidLocator {
        session_id: Some(request.metadata.session_id.clone()),
        detail,
    })?;
    Ok(SessionIndexV1Row {
        session_id: request.metadata.session_id.clone(),
        project_id: request.project_id.to_string(),
        project_tag: request.metadata.project_tag.clone(),
        storage_partition: request.storage_partition.to_string(),
        relative_path,
        title: request.metadata.title.clone(),
        created_at_ms: request.metadata.created_at_ms,
        updated_at_ms: request.metadata.updated_at_ms,
        archived_at_ms: request.metadata.archived_at_ms,
        unread: 0,
    })
}

/// Converts one authoritative SQLite row into an exact physical layout.
/// 将一条权威 SQLite 记录转换为精确物理布局。
fn storage_locator_from_row(
    data_root: &Path,
    row: SessionIndexV1Row,
) -> Result<SessionV1StorageLocator, SessionIndexV1Error> {
    validate_production_storage_partition(&row.storage_partition, Some(row.session_id.as_str()))?;
    validate_relative_path(&row.relative_path).map_err(|detail| {
        SessionIndexV1Error::InvalidLocator {
            session_id: Some(row.session_id.clone()),
            detail: format!("invalid relative_path: {detail}"),
        }
    })?;
    let project_prefix = format!("{}/", row.project_tag);
    let relative_leaf = row
        .relative_path
        .strip_prefix(&project_prefix)
        .ok_or_else(|| SessionIndexV1Error::InvalidLocator {
            session_id: Some(row.session_id.clone()),
            detail: "relative_path project partition does not match project_tag".to_string(),
        })?;
    let session_suffix = format!("-{}", row.session_id);
    let date_bucket = relative_leaf.strip_suffix(&session_suffix).ok_or_else(|| {
        SessionIndexV1Error::InvalidLocator {
            session_id: Some(row.session_id.clone()),
            detail: "relative_path directory does not end with the exact session_id".to_string(),
        }
    })?;
    if date_bucket.is_empty() {
        return Err(SessionIndexV1Error::InvalidLocator {
            session_id: Some(row.session_id.clone()),
            detail: "relative_path date bucket must not be empty".to_string(),
        });
    }
    let expected_relative_path =
        compose_relative_path(&row.project_tag, date_bucket, &row.session_id).map_err(
            |detail| SessionIndexV1Error::InvalidLocator {
                session_id: Some(row.session_id.clone()),
                detail,
            },
        )?;
    if expected_relative_path != row.relative_path {
        return Err(SessionIndexV1Error::InvalidLocator {
            session_id: Some(row.session_id.clone()),
            detail: format!(
                "relative_path mismatch: expected {expected_relative_path}, actual {}",
                row.relative_path
            ),
        });
    }
    let layout = SessionV1Layout::new(
        data_root.join(&row.storage_partition),
        row.project_tag.clone(),
        date_bucket,
        row.session_id.clone(),
    );
    let project_directory = layout.root.join(&layout.project_partition);
    Ok(SessionV1StorageLocator {
        session_id: row.session_id,
        project_id: row.project_id,
        relative_path: row.relative_path,
        project_directory,
        layout,
    })
}

/// Validates the single production partition and rejects obsolete duplicate-root locators.
/// 校验唯一生产分区，并拒绝会重复拼接数据根的过时定位信息。
fn validate_production_storage_partition(
    storage_partition: &str,
    session_id: Option<&str>,
) -> Result<(), SessionIndexV1Error> {
    validate_relative_path(storage_partition).map_err(|detail| {
        SessionIndexV1Error::InvalidLocator {
            session_id: session_id.map(str::to_string),
            detail: format!("invalid storage_partition: {detail}"),
        }
    })?;
    if storage_partition != SESSION_V1_STORAGE_PARTITION {
        return Err(SessionIndexV1Error::InvalidLocator {
            session_id: session_id.map(str::to_string),
            detail: format!(
                "unsupported storage_partition {storage_partition:?}; expected {SESSION_V1_STORAGE_PARTITION:?} below the application data root"
            ),
        });
    }
    Ok(())
}

/// Creates one exact physical V1 session tree for downstream persistence tests.
/// 为下游持久化测试创建一棵精确的 V1 会话物理目录树。
///
/// This fixture bypasses only the relational index. It still uses the production
/// locator validation and complete V1 file publisher, and is unavailable in production builds.
/// 该夹具仅绕过关系索引；仍使用生产定位校验与完整 V1 文件发布器，并且在
/// 生产构建中不可用。
///
/// # Parameters
/// 参数
/// `data_root` is the isolated test-owned application data root.
/// `data_root` 是测试独占的隔离应用数据根目录。
/// `session_id` is the exact V1 session identity.
/// `session_id` 是精确的 V1 会话身份。
/// `updated_at_ms` is the initial metadata timestamp.
/// `updated_at_ms` 是初始 metadata 时间戳。
///
/// # Returns
/// 返回值
/// A validated locator whose complete V1 root has already been published.
/// 返回一份完整 V1 根目录已经发布的已校验定位器。
#[cfg(any(test, feature = "test-support"))]
pub fn create_session_v1_storage_fixture(
    data_root: &Path,
    session_id: &str,
    updated_at_ms: i64,
) -> Result<SessionV1StorageLocator, String> {
    let row = SessionIndexV1Row {
        session_id: session_id.to_string(),
        project_id: "v1-test-project".to_string(),
        project_tag: "v1-test-project".to_string(),
        storage_partition: SESSION_V1_STORAGE_PARTITION.to_string(),
        relative_path: compose_relative_path("v1-test-project", "1970-01-01", session_id)
            .map_err(|error| format!("Failed to compose V1 test locator: {error}"))?,
        title: "V1 persistence test session".to_string(),
        created_at_ms: updated_at_ms,
        updated_at_ms,
        archived_at_ms: None,
        unread: 0,
    };
    let locator = storage_locator_from_row(data_root, row)
        .map_err(|error| format!("Failed to construct V1 test locator: {error}"))?;
    let metadata = MetadataDocument::new(
        session_id,
        "v1-test-project",
        "V1 persistence test session",
        updated_at_ms,
    );
    let runtime_lock = RuntimeLock {
        initialization_state:
            vulcan_session_core::v1::context::RuntimeInitializationState::FreshUninitialized,
        current_compaction_id: None,
        locked_sources: Vec::new(),
    };
    create_session(
        &locator.layout,
        &metadata,
        &runtime_lock,
        updated_at_ms,
        &[],
        &[],
    )
    .map_err(|error| format!("Failed to publish V1 test session: {error}"))?;
    Ok(locator)
}

/// Builds one deterministic embedded TaskSession locator from an exact parent locator.
/// 从精确父定位器构建一个确定性的内嵌 TaskSession 定位器。
fn embedded_task_locator(
    parent: &SessionV1StorageLocator,
    child_session_id: &str,
) -> Result<SessionV1StorageLocator, SessionIndexV1Error> {
    if child_session_id.is_empty() || child_session_id.trim() != child_session_id {
        return Err(SessionIndexV1Error::InvalidLocator {
            session_id: (!child_session_id.is_empty()).then(|| child_session_id.to_string()),
            detail:
                "embedded task session_id must be non-empty and contain no surrounding whitespace"
                    .to_string(),
        });
    }
    validate_relative_path(EMBEDDED_TASK_PARTITION).map_err(|detail| {
        SessionIndexV1Error::InvalidLocator {
            session_id: Some(child_session_id.to_string()),
            detail: format!("invalid embedded task partition: {detail}"),
        }
    })?;
    let layout = SessionV1Layout::new(
        parent.layout.session_dir(),
        EMBEDDED_TASK_PARTITION,
        EMBEDDED_TASK_BUCKET,
        child_session_id,
    );
    let child_relative_path = layout.relative_path();
    validate_relative_path(&child_relative_path).map_err(|detail| {
        SessionIndexV1Error::InvalidLocator {
            session_id: Some(child_session_id.to_string()),
            detail: format!("invalid embedded task relative path: {detail}"),
        }
    })?;
    Ok(SessionV1StorageLocator {
        session_id: child_session_id.to_string(),
        project_id: parent.project_id.clone(),
        relative_path: format!("{}/{}", parent.relative_path, child_relative_path),
        project_directory: parent.project_directory.clone(),
        layout,
    })
}

/// Checks one exact metadata path without opening or scanning sibling directories.
/// 检查一个精确 metadata 路径，不打开或扫描同级目录。
fn exact_metadata_file_exists(
    locator: &SessionV1StorageLocator,
) -> Result<bool, SessionV1StoreError> {
    let path = locator.layout.metadata_path();
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() && !metadata.file_type().is_symlink() => {
            Ok(true)
        }
        Ok(metadata) => Err(SessionV1StorageError::io(
            &path,
            format!(
                "expected regular metadata file, actual file_type={:?}",
                metadata.file_type()
            ),
        )
        .with_context(locator.session_id(), &path)
        .into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(SessionV1StorageError::io(&path, error.to_string())
            .with_context(locator.session_id(), &path)
            .into()),
    }
}

/// Removes only the exact indexed session directory and accepts absence.
/// 只删除精确索引的会话目录，并接受目录缺失。
fn remove_exact_session_directory(
    locator: &SessionV1StorageLocator,
) -> Result<bool, SessionV1StoreError> {
    let path = locator.layout.session_dir();
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(&path).map_err(|error| {
                SessionV1StorageError::io(&path, error.to_string())
                    .with_context(locator.session_id(), &path)
            })?;
            Ok(true)
        }
        Ok(metadata) => Err(SessionV1StorageError::io(
            &path,
            format!(
                "expected regular session directory, actual file_type={:?}",
                metadata.file_type()
            ),
        )
        .with_context(locator.session_id(), &path)
        .into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(SessionV1StorageError::io(&path, error.to_string())
            .with_context(locator.session_id(), &path)
            .into()),
    }
}

#[cfg(test)]
mod tests {
    /// Recovery distinguishes unpublished storage from an incomplete or corrupt existing directory.
    /// 恢复区分尚未发布的存储与不完整或损坏的已有目录。
    /// Returns nothing; all paths belong to one temporary fixture and no existing data is removed.
    /// 不返回值；全部路径属于一个临时夹具，不移除任何已有数据。
    #[test]
    fn collaboration_recovery_metadata_distinguishes_missing_publication() {
        let temporary = tempfile::tempdir().unwrap();
        let root = super::create_session_v1_storage_fixture(
            temporary.path(),
            "missing-publication-root",
            1,
        )
        .unwrap();
        let child =
            super::locate_embedded_task_session_v1(&root, "missing-publication-child").unwrap();
        assert!(matches!(child.read_collaboration_recovery_snapshot(),
            Err(super::SessionV1StoreError::Storage(error)) if error.code == super::SessionV1StorageErrorCode::SessionMissing));
        std::fs::create_dir_all(child.session_directory()).unwrap();
        assert!(matches!(child.read_collaboration_recovery_snapshot(),
            Err(super::SessionV1StoreError::Storage(error)) if error.code == super::SessionV1StorageErrorCode::MetadataCorrupt));
        std::fs::write(child.layout.metadata_path(), b"invalid metadata").unwrap();
        assert!(matches!(child.read_collaboration_recovery_snapshot(),
            Err(super::SessionV1StoreError::Storage(error)) if error.code == super::SessionV1StorageErrorCode::MetadataCorrupt));
        assert_eq!(
            std::fs::read(child.layout.metadata_path()).unwrap(),
            b"invalid metadata"
        );
    }

    /// Recovery selection validates metadata without opening or repairing a broken interaction tail.
    /// 恢复筛选校验元数据，不打开或修复损坏的交互尾部。
    #[test]
    fn collaboration_recovery_metadata_does_not_load_interactions() {
        // The isolated fixture contains the real metadata contract and locator ownership.
        // 隔离夹具包含真实元数据契约及定位器所有权。
        let temporary = tempfile::tempdir().unwrap();
        let locator =
            super::create_session_v1_storage_fixture(temporary.path(), "collaboration-metadata", 1)
                .unwrap();
        let mut metadata = locator.read_indexed_metadata().unwrap();
        let tail = locator.layout.segment_entry_path(
            metadata.segment_generation,
            &metadata.split_list[0].file_name,
        );
        std::fs::write(&tail, b"invalid interaction JSON\n").unwrap();
        let original = std::fs::read(locator.layout.metadata_path()).unwrap();
        assert!(super::load_session_v1_tail(&locator).is_err());
        assert_eq!(
            locator.read_collaboration_recovery_state().unwrap(),
            metadata.collaboration.state
        );
        assert_eq!(
            locator.read_collaboration_recovery_snapshot().unwrap(),
            super::SessionV1CollaborationRecoverySnapshot {
                segment_generation: metadata.segment_generation,
                collaboration: metadata.collaboration.state.clone(),
            }
        );
        assert_eq!(
            std::fs::read(locator.layout.metadata_path()).unwrap(),
            original
        );
        assert_eq!(std::fs::read(&tail).unwrap(), b"invalid interaction JSON\n");
        // Typed collaboration corruption cannot be mistaken for an empty outbox.
        // 强类型协作损坏不能被误判为空出站箱。
        metadata.collaboration.version = 99;
        super::super::session_v1::documents::write_json_document(
            &locator.layout.metadata_path(),
            &metadata,
            "metadata",
        )
        .unwrap();
        assert!(locator.read_collaboration_recovery_state().is_err());
        // A valid collaboration container cannot hide an unusable physical generation.
        // 合法协作容器不能掩盖不可用物理代次。
        metadata.collaboration.version = 1;
        metadata.segment_generation = 0;
        super::super::session_v1::documents::write_json_document(
            &locator.layout.metadata_path(),
            &metadata,
            "metadata",
        )
        .unwrap();
        assert!(locator.read_collaboration_recovery_snapshot().is_err());
    }

    /// Model restoration reads metadata even when interactions are corrupt, without changing either file.
    /// 即使交互内容损坏，模型恢复仍仅读取 metadata，且不修改任何文件。
    #[test]
    fn request_metadata_restores_active_and_pending_without_interaction_loading() {
        use vulcan_session_core::v1::config::{ActiveRequestConfig, PendingRequestConfig};
        // An isolated real directory exercises the production metadata reader without user data.
        // 隔离的真实目录用于验证生产 metadata 读取器，不使用用户数据。
        let temporary = tempfile::tempdir().unwrap();
        // The fixture owns the authoritative locator and its exact identity.
        // 夹具持有权威定位器及其精确身份。
        let locator =
            super::create_session_v1_storage_fixture(temporary.path(), "model-metadata", 1)
                .unwrap();
        // Save both an executed route and a newer unexecuted user choice.
        // 保存一条已执行路线和一条更新但尚未执行的用户选择。
        let mut metadata = locator.read_indexed_metadata().unwrap();
        metadata.config.active_request = ActiveRequestConfig {
            provider: Some("kimi".into()),
            connection_id: Some("kimi-account".into()),
            model: Some("k3-256k".into()),
            thinking_level: Some("max".into()),
            ..Default::default()
        };
        metadata.config.pending_request = Some(PendingRequestConfig {
            provider: Some("other-provider".into()),
            connection_id: Some("other-account".into()),
            model: Some("other-model".into()),
            set_at_ms: 2,
            thinking_level: None,
            temperature: None,
        });
        super::super::session_v1::documents::write_json_document(
            &locator.layout.metadata_path(),
            &metadata,
            "metadata",
        )
        .unwrap();
        // A broken interaction tail proves no Turn replay is needed to restore a model.
        // 损坏的交互尾部证明恢复模型不需要重放回合。
        let tail = locator.layout.segment_entry_path(
            metadata.segment_generation,
            &metadata.split_list[0].file_name,
        );
        std::fs::write(&tail, b"invalid interaction JSON\n").unwrap();
        // Preserve the exact source bytes to detect unintended repair or persistence.
        // 保留精确源字节，以检测非预期修复或持久化。
        let before = std::fs::read(locator.layout.metadata_path()).unwrap();
        assert!(super::load_session_v1_tail(&locator).is_err());
        assert_eq!(
            locator.read_request_configuration().unwrap(),
            (
                metadata.config.active_request.clone(),
                metadata.config.pending_request.clone()
            )
        );
        assert_eq!(
            std::fs::read(locator.layout.metadata_path()).unwrap(),
            before
        );
        assert_eq!(std::fs::read(&tail).unwrap(), b"invalid interaction JSON\n");
        // Metadata belonging to another owner must never become this session's model.
        // 其他 owner 的 metadata 绝不能成为当前会话的模型。
        metadata.session_id = "wrong-owner".into();
        super::super::session_v1::documents::write_json_document(
            &locator.layout.metadata_path(),
            &metadata,
            "metadata",
        )
        .unwrap();
        assert!(locator.read_request_configuration().is_err());
    }

    /// Verifies metadata reads and writes remain independent of corrupt or absent interactions.
    /// 验证元数据读写独立于损坏或缺失的交互内容。
    #[test]
    fn capability_metadata_ignores_interaction_storage() {
        use vulcan_session_core::v1::config::ProviderToolSelectionConfig;
        // Each case owns an isolated real V1 directory, never user session data.
        // 每个用例拥有独立真实 V1 目录，绝不使用用户会话数据。
        for remove_context in [false, true] {
            let temporary = tempfile::tempdir().unwrap();
            let locator = super::create_session_v1_storage_fixture(
                temporary.path(),
                "capability-metadata",
                1,
            )
            .unwrap();
            let before = locator.read_configuration_metadata().unwrap();
            let original_bytes = std::fs::read(locator.layout.metadata_path()).unwrap();
            let tail = locator
                .layout
                .segment_entry_path(before.segment_generation, &before.split_list[0].file_name);
            if remove_context {
                std::fs::remove_file(&tail).unwrap();
                std::fs::remove_dir_all(locator.layout.session_dir().join("context")).unwrap();
            } else {
                std::fs::write(&tail, b"invalid interaction JSON\n").unwrap();
            }
            assert!(super::load_session_v1_tail(&locator).is_err());
            assert_eq!(
                locator.read_provider_tool_selection().unwrap(),
                ProviderToolSelectionConfig::default()
            );
            assert_eq!(
                std::fs::read(locator.layout.metadata_path()).unwrap(),
                original_bytes
            );
            // Only the explicit selection and its metadata revision/time may change.
            // 仅显式选择及其元数据修订/时间允许变化。
            let selection = ProviderToolSelectionConfig {
                initialized: true,
                overridden: true,
                enabled_capabilities: vec![String::from("search")],
                ..Default::default()
            };
            assert!(
                locator
                    .save_unloaded_provider_tool_selection(selection.clone(), 3)
                    .unwrap()
            );
            let mut expected = before;
            expected.config.selection.provider_tools = selection.clone();
            expected.updated_at_ms = 3;
            expected.document_revision += 1;
            assert_eq!(locator.read_configuration_metadata().unwrap(), expected);
            assert_eq!(locator.read_provider_tool_selection().unwrap(), selection);
            let saved_bytes = std::fs::read(locator.layout.metadata_path()).unwrap();
            assert!(
                !locator
                    .save_unloaded_provider_tool_selection(selection, 5)
                    .unwrap()
            );
            assert_eq!(
                std::fs::read(locator.layout.metadata_path()).unwrap(),
                saved_bytes
            );
            assert!(super::load_session_v1_tail(&locator).is_err());
            if !remove_context {
                assert_eq!(std::fs::read(&tail).unwrap(), b"invalid interaction JSON\n");
            }
        }
    }

    /// Invalid selections fail before publication and are classified as configuration errors.
    /// 非法选择在发布前失败，并归类为配置错误。
    #[test]
    fn capability_metadata_rejects_invalid_write_without_changing_bytes() {
        // The baseline includes all unrelated metadata fields preserved by the narrow writer.
        // 基线包含窄写入器必须保留的全部无关元数据字段。
        let temporary = tempfile::tempdir().unwrap();
        let locator =
            super::create_session_v1_storage_fixture(temporary.path(), "invalid-capability", 1)
                .unwrap();
        let before = std::fs::read(locator.layout.metadata_path()).unwrap();
        assert!(
            locator
                .save_unloaded_provider_tool_selection(Default::default(), -1)
                .is_err()
        );
        let invalid = vulcan_session_core::v1::config::ProviderToolSelectionConfig {
            initialized: true,
            enabled_capabilities: vec![String::from("search"), String::from("search")],
            ..Default::default()
        };
        let error = locator
            .save_unloaded_provider_tool_selection(invalid, 2)
            .unwrap_err()
            .to_string();
        assert!(error.contains("metadata_corrupt"));
        assert!(!error.contains("segment_corrupt"));
        assert_eq!(
            std::fs::read(locator.layout.metadata_path()).unwrap(),
            before
        );
    }

    use super::*;

    /// Rebuild accepts damage confined to the capability subtree and preserves every other fact.
    /// 重建接受仅限能力子树的损坏，并保留全部其他事实。
    #[test]
    fn capability_metadata_rebuild_replaces_only_the_addon() {
        use vulcan_session_core::v1::config::ProviderToolSelectionConfig;
        // Cover valid overrides, invalid semantics, malformed shape and the absent optional addon.
        // 覆盖合法覆盖、语义错误、结构损坏和可选附加项缺失。
        for damaged in [
            Some(
                serde_json::json!({"initialized": true, "overridden": true, "enabled_capabilities": ["search"]}),
            ),
            Some(
                serde_json::json!({"initialized": true, "enabled_capabilities": ["search", "search"]}),
            ),
            Some(serde_json::json!("broken capability configuration")),
            None,
        ] {
            let temporary = tempfile::tempdir().unwrap();
            let locator =
                create_session_v1_storage_fixture(temporary.path(), "rebuild-addon", 1).unwrap();
            let path = locator.layout.metadata_path();
            let mut expected = locator.read_configuration_metadata().unwrap();
            let tail = locator.layout.segment_entry_path(
                expected.segment_generation,
                &expected.split_list[0].file_name,
            );
            let tail_bytes = std::fs::read(&tail).unwrap();
            let mut raw = serde_json::to_value(&expected).unwrap();
            let selections = raw
                .pointer_mut("/config/selection")
                .unwrap()
                .as_object_mut()
                .unwrap();
            match damaged {
                Some(value) => {
                    selections.insert("provider_tools".into(), value);
                }
                None => {
                    selections.remove("provider_tools");
                }
            }
            std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
            let defaults = ProviderToolSelectionConfig {
                initialized: true,
                ..Default::default()
            };
            locator
                .rebuild_unloaded_provider_tool_selection(defaults.clone(), 7)
                .unwrap();
            expected.config.selection.provider_tools = defaults;
            expected.document_revision += 1;
            expected.updated_at_ms = 7;
            assert_eq!(locator.read_configuration_metadata().unwrap(), expected);
            assert_eq!(std::fs::read(&tail).unwrap(), tail_bytes);
            assert!(load_session_v1_tail(&locator).is_ok());
        }
    }

    /// Recovery refuses unrelated damage and invalid timestamps without changing any bytes.
    /// 恢复拒绝无关损坏及非法时间戳，不改变任何字节。
    #[test]
    fn capability_metadata_rebuild_refuses_unrelated_corruption() {
        // Each exact JSON pointer addresses an existing frozen metadata field outside the addon.
        // 每个精确 JSON 指针都指向附加项之外已有的冻结元数据字段。
        for (pointer, value) in [
            ("/session_id", serde_json::json!("another-session")),
            ("/schema_version", serde_json::json!(99)),
            ("/mode/goal", serde_json::json!("broken goal")),
            ("/config/selection", serde_json::Value::Null),
        ] {
            let temporary = tempfile::tempdir().unwrap();
            let locator =
                create_session_v1_storage_fixture(temporary.path(), "reject-rebuild", 1).unwrap();
            let path = locator.layout.metadata_path();
            let mut raw: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            *raw.pointer_mut(pointer).unwrap() = value;
            let before = serde_json::to_vec(&raw).unwrap();
            std::fs::write(&path, &before).unwrap();
            assert!(
                locator
                    .rebuild_unloaded_provider_tool_selection(Default::default(), 7)
                    .is_err()
            );
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
        let temporary = tempfile::tempdir().unwrap();
        let locator =
            create_session_v1_storage_fixture(temporary.path(), "reject-timestamp", 1).unwrap();
        let before = std::fs::read(locator.layout.metadata_path()).unwrap();
        assert!(
            locator
                .rebuild_unloaded_provider_tool_selection(Default::default(), -1)
                .is_err()
        );
        assert_eq!(
            std::fs::read(locator.layout.metadata_path()).unwrap(),
            before
        );
    }
    use crate::data_plane::HostRuntimeMetadata;
    use crate::local_database_runtime::LogicalDatabase;
    use crate::local_sql;
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Child, Command, Stdio};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use vulcan_sqlite_engine::sql_exec::SqliteValue;

    /// Owns one external SQLite transaction used to prove real lock conflicts.
    /// 持有一个用于证明真实锁冲突的外部 SQLite 事务。
    struct SqliteLockHolder {
        /// Managed Python process whose SQLite connection owns the exclusive transaction.
        /// SQLite 连接持有独占事务的受管 Python 进程。
        child: Option<Child>,
    }

    impl SqliteLockHolder {
        /// Releases the exclusive transaction and requires the lock holder to exit successfully.
        /// 释放独占事务，并要求锁持有进程成功退出。
        ///
        /// # Returns
        /// 返回值
        /// This method returns only after the child has rolled back and exited successfully.
        /// 仅在子进程回滚并成功退出后返回。
        fn release(mut self) {
            let mut child = self.child.take().expect("SQLite lock holder must exist");
            child
                .stdin
                .as_mut()
                .expect("SQLite lock holder stdin must exist")
                .write_all(b"x")
                .expect("release SQLite lock holder");
            let status = child.wait().expect("wait for SQLite lock holder");
            assert!(status.success(), "SQLite lock holder must exit cleanly");
        }
    }

    impl Drop for SqliteLockHolder {
        /// Terminates an unreleased lock holder so a failed assertion cannot leave a child process behind.
        /// 终止尚未释放的锁持有进程，避免断言失败后遗留子进程。
        fn drop(&mut self) {
            if let Some(mut child) = self.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    /// Resolves the exact managed Python executable from its packaged runtime manifest.
    /// 从打包运行时清单中解析精确的受管 Python 可执行文件。
    ///
    /// # Returns
    /// 返回值
    /// The single executable declared by the packaged Python runtime manifest.
    /// 打包 Python 运行时清单声明的唯一可执行文件。
    fn managed_python_executable() -> PathBuf {
        let python_root = vulcan_runtime_assets::resolve_managed_runtime_distribution_root()
            .expect("managed runtime root must exist")
            .join("python");
        let mut executables = fs::read_dir(&python_root)
            .expect("read managed Python runtime root")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_dir()))
            .filter_map(|entry| {
                let manifest_path = entry.path().join("runtime-manifest.json");
                let bytes = fs::read(&manifest_path).ok()?;
                let document = serde_json::from_slice::<serde_json::Value>(&bytes).ok()?;
                (document.get("runtime").and_then(serde_json::Value::as_str) == Some("python"))
                    .then(|| {
                        let executable = document
                            .get("executable")
                            .and_then(serde_json::Value::as_str)
                            .expect("Python runtime manifest must declare executable");
                        entry.path().join(executable)
                    })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            executables.len(),
            1,
            "exactly one managed Python runtime must be packaged"
        );
        let executable = executables.pop().expect("managed Python executable");
        assert!(executable.is_file(), "managed Python executable must exist");
        executable
    }

    /// Starts an external SQLite connection and waits until its exclusive transaction is active.
    /// 启动外部 SQLite 连接，并等待其独占事务生效。
    ///
    /// # Parameters
    /// 参数
    /// `sqlite_path` is the exact database used by the in-process session index.
    /// `sqlite_path` 是宿主运行时会话索引使用的精确数据库。
    ///
    /// # Returns
    /// 返回值
    /// A process guard that keeps the transaction open until explicitly released or dropped.
    /// 返回一个进程守卫，在显式释放或析构前持续持有事务。
    fn start_sqlite_lock_holder(sqlite_path: &Path) -> SqliteLockHolder {
        const LOCK_SCRIPT: &str = r#"
import sqlite3
import sys

connection = sqlite3.connect(sys.argv[1], timeout=1.0, isolation_level=None)
connection.execute("BEGIN EXCLUSIVE")
connection.execute(
    "UPDATE session_index_v1 SET unread = unread WHERE session_id = ?",
    ("session-e2e",),
)
print("READY", flush=True)
sys.stdin.buffer.read(1)
connection.rollback()
connection.close()
"#;
        let mut child = Command::new(managed_python_executable())
            .arg("-c")
            .arg(LOCK_SCRIPT)
            .arg(sqlite_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn managed SQLite lock holder");
        let mut ready = String::new();
        BufReader::new(
            child
                .stdout
                .take()
                .expect("SQLite lock holder stdout must exist"),
        )
        .read_line(&mut ready)
        .expect("read SQLite lock holder readiness");
        assert_eq!(ready.trim(), "READY", "SQLite lock holder must be ready");
        SqliteLockHolder { child: Some(child) }
    }

    /// Builds one empty V1 creation fixture.
    /// 构建一个空 V1 创建夹具。
    fn creation_fixture<'a>(
        metadata: &'a MetadataDocument,
        runtime_lock: &'a RuntimeLock,
    ) -> CreateSessionV1Request<'a> {
        CreateSessionV1Request {
            project_id: "project-id",
            storage_partition: SESSION_V1_STORAGE_PARTITION,
            date_bucket: "2026-08-27",
            metadata,
            runtime_lock,
            compactions: &[],
            events: &[],
        }
    }

    /// File publication finishes before the injected SQLite publisher runs.
    /// 文件发布会在注入的 SQLite 发布器运行前完成。
    #[tokio::test]
    async fn creation_publishes_complete_files_before_index() {
        let data_root = tempfile::tempdir().expect("temporary data root");
        let metadata = MetadataDocument::new("session-1", "project-tag", "title", 1);
        let runtime_lock = RuntimeLock {
            initialization_state:
                vulcan_session_core::v1::context::RuntimeInitializationState::Initialized,
            current_compaction_id: None,
            locked_sources: Vec::new(),
        };
        let request = creation_fixture(&metadata, &runtime_lock);
        let row = create_index_row(&request).expect("valid index row");
        let index_called = Arc::new(AtomicBool::new(false));
        let observed_index_called = Arc::clone(&index_called);
        let expected_metadata_path = data_root
            .path()
            .join("sessions/project-tag/2026-08-27-session-1/metadata.json");

        let created = create_indexed_session_v1_with_insert(
            data_root.path(),
            &request,
            row,
            move |_row| async move {
                assert!(expected_metadata_path.is_file());
                observed_index_called.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
        .await
        .expect("indexed V1 creation");

        assert!(index_called.load(Ordering::SeqCst));
        assert_eq!(created.locator.session_id(), "session-1");
        assert_eq!(created.metadata.session_id, "session-1");
    }

    /// An index failure leaves the complete unindexed directory untouched.
    /// 索引失败会原样保留完整的未索引目录。
    #[tokio::test]
    async fn index_failure_keeps_published_files_without_repair() {
        let data_root = tempfile::tempdir().expect("temporary data root");
        let metadata = MetadataDocument::new("session-2", "project-tag", "title", 2);
        let runtime_lock = RuntimeLock {
            initialization_state:
                vulcan_session_core::v1::context::RuntimeInitializationState::Initialized,
            current_compaction_id: None,
            locked_sources: Vec::new(),
        };
        let request = creation_fixture(&metadata, &runtime_lock);
        let row = create_index_row(&request).expect("valid index row");
        let expected_session_dir = data_root
            .path()
            .join("sessions/project-tag/2026-08-27-session-2");

        let error =
            create_indexed_session_v1_with_insert(data_root.path(), &request, row, |_row| async {
                Err(SessionIndexV1Error::Write {
                    session_id: "session-2".to_string(),
                    detail: "injected index failure".to_string(),
                })
            })
            .await
            .expect_err("index failure must be returned");

        assert!(matches!(
            error,
            SessionV1StoreError::Index(SessionIndexV1Error::Write { .. })
        ));
        assert!(expected_session_dir.join("metadata.json").is_file());
        assert!(
            expected_session_dir
                .join("context/c0000001/runtime.json")
                .is_file()
        );
        assert!(
            expected_session_dir
                .join(format!(
                    "segments/g0000001/{}",
                    SessionV1Layout::segment_file_name(1)
                ))
                .is_file()
        );
    }

    /// An identical retry reuses the exact complete directory and only retries the index insert.
    /// 完全相同的重试复用精确完整目录，并且只重试索引插入。
    #[tokio::test]
    async fn index_failure_retry_reuses_exact_published_directory() {
        let data_root = tempfile::tempdir().expect("temporary data root");
        let metadata = MetadataDocument::new("session-retry", "project-tag", "title", 2);
        let runtime_lock = RuntimeLock {
            initialization_state:
                vulcan_session_core::v1::context::RuntimeInitializationState::FreshUninitialized,
            current_compaction_id: None,
            locked_sources: Vec::new(),
        };
        let request = creation_fixture(&metadata, &runtime_lock);
        let row = create_index_row(&request).expect("valid index row");
        let metadata_path = data_root
            .path()
            .join("sessions/project-tag/2026-08-27-session-retry/metadata.json");

        create_indexed_session_v1_with_insert(
            data_root.path(),
            &request,
            row.clone(),
            |_row| async {
                Err(SessionIndexV1Error::Write {
                    session_id: "session-retry".to_string(),
                    detail: "injected first index failure".to_string(),
                })
            },
        )
        .await
        .expect_err("first index insert must fail");
        let before_retry = fs::read(&metadata_path).expect("published metadata before retry");

        let created =
            create_indexed_session_v1_with_insert(data_root.path(), &request, row, |_row| async {
                Ok(())
            })
            .await
            .expect("identical retry must only complete the missing index publication");

        assert_eq!(
            fs::read(&metadata_path).expect("published metadata after retry"),
            before_retry
        );
        assert_eq!(created.metadata.session_id, metadata.session_id);
        assert_eq!(created.metadata.title, metadata.title);
        assert_eq!(created.metadata.split_list.len(), 1);
    }

    /// A conflicting retry cannot overwrite a complete unindexed directory from another request.
    /// 冲突重试不能覆盖另一条请求留下的完整未索引目录。
    #[tokio::test]
    async fn conflicting_retry_cannot_overwrite_unindexed_directory() {
        let data_root = tempfile::tempdir().expect("temporary data root");
        let original = MetadataDocument::new("session-conflict", "project-tag", "winner", 3);
        let runtime_lock = RuntimeLock {
            initialization_state:
                vulcan_session_core::v1::context::RuntimeInitializationState::FreshUninitialized,
            current_compaction_id: None,
            locked_sources: Vec::new(),
        };
        let original_request = creation_fixture(&original, &runtime_lock);
        let original_row = create_index_row(&original_request).expect("valid original row");
        create_indexed_session_v1_with_insert(
            data_root.path(),
            &original_request,
            original_row,
            |_row| async {
                Err(SessionIndexV1Error::Write {
                    session_id: "session-conflict".to_string(),
                    detail: "injected index failure".to_string(),
                })
            },
        )
        .await
        .expect_err("leave one complete unindexed directory");

        let conflicting = MetadataDocument::new("session-conflict", "project-tag", "loser", 3);
        let conflicting_request = creation_fixture(&conflicting, &runtime_lock);
        let conflicting_row =
            create_index_row(&conflicting_request).expect("valid conflicting row");
        let error = create_indexed_session_v1_with_insert(
            data_root.path(),
            &conflicting_request,
            conflicting_row,
            |_row| async { Ok(()) },
        )
        .await
        .expect_err("conflicting retry must not overwrite the winner");

        assert!(matches!(error, SessionV1StoreError::Storage(_)));
        let locator = storage_locator_from_row(
            data_root.path(),
            create_index_row(&original_request).expect("original locator row"),
        )
        .expect("original locator");
        let loaded = load_session_v1_tail(&locator).expect("winner remains loadable");
        assert_eq!(loaded.metadata.title, "winner");
    }

    /// The exact row resolves to one tail-only load without directory search.
    /// 精确索引记录无需目录搜索即可解析为一次仅尾段加载。
    #[tokio::test]
    async fn exact_locator_tail_load_round_trip() {
        let data_root = tempfile::tempdir().expect("temporary data root");
        let metadata = MetadataDocument::new("session-3", "project-tag", "title", 3);
        let runtime_lock = RuntimeLock {
            initialization_state:
                vulcan_session_core::v1::context::RuntimeInitializationState::Initialized,
            current_compaction_id: None,
            locked_sources: Vec::new(),
        };
        let request = creation_fixture(&metadata, &runtime_lock);
        let row = create_index_row(&request).expect("valid index row");
        let created =
            create_indexed_session_v1_with_insert(data_root.path(), &request, row, |_row| async {
                Ok(())
            })
            .await
            .expect("indexed V1 creation");

        let loaded = load_session_v1_tail(&created.locator).expect("tail-only load");

        assert_eq!(loaded.locator.session_id(), "session-3");
        assert_eq!(loaded.metadata.session_id, "session-3");
        assert_eq!(loaded.report.opened_segment_files, 1);
        assert_eq!(loaded.report.resident_event_count, 0);
    }

    /// An embedded TaskSession publishes below the exact parent without a SQLite row or scan.
    /// 内嵌 TaskSession 在精确父目录下发布，不写 SQLite 记录也不执行扫描。
    #[tokio::test]
    async fn embedded_task_session_round_trips_through_parent_derived_locator() {
        let data_root = tempfile::tempdir().expect("temporary data root");
        let parent_metadata = MetadataDocument::new("parent-session", "project-tag", "parent", 3);
        let runtime_lock = RuntimeLock {
            initialization_state:
                vulcan_session_core::v1::context::RuntimeInitializationState::Initialized,
            current_compaction_id: None,
            locked_sources: Vec::new(),
        };
        let parent_request = creation_fixture(&parent_metadata, &runtime_lock);
        let parent_row = create_index_row(&parent_request).expect("valid parent row");
        let parent = create_indexed_session_v1_with_insert(
            data_root.path(),
            &parent_request,
            parent_row,
            |_row| async { Ok(()) },
        )
        .await
        .expect("published parent");
        let child_metadata = MetadataDocument::new(
            "child-session",
            "project-tag",
            "child",
            parent_metadata.updated_at_ms + 1,
        );
        let child_request = CreateEmbeddedTaskSessionV1Request {
            metadata: &child_metadata,
            runtime_lock: &runtime_lock,
            compactions: &[],
            events: &[],
        };

        let child = create_embedded_task_session_v1(&parent.locator, &child_request)
            .expect("published embedded child");
        let derived = locate_embedded_task_session_v1(&parent.locator, "child-session")
            .expect("deterministic child locator");
        let loaded = load_session_v1_tail(&derived).expect("embedded child tail load");

        assert_eq!(child.locator, derived);
        assert_eq!(child.locator.project_id(), parent.locator.project_id());
        assert_eq!(
            child.locator.project_directory(),
            parent.locator.project_directory()
        );
        assert_eq!(
            child.locator.relative_path(),
            "project-tag/2026-08-27-parent-session/TASK/task-child-session"
        );
        assert_eq!(loaded.metadata.session_id, "child-session");
        assert_eq!(loaded.metadata.project_tag, "project-tag");
        assert!(
            data_root
                .path()
                .join(
                    "sessions/project-tag/2026-08-27-parent-session/TASK/task-child-session/metadata.json"
                )
                .is_file()
        );
    }

    /// An embedded child cannot cross the project boundary fixed by its parent metadata.
    /// 内嵌子会话不能跨越父 metadata 固定的项目边界。
    #[tokio::test]
    async fn embedded_task_session_rejects_project_tag_drift_before_publication() {
        let data_root = tempfile::tempdir().expect("temporary data root");
        let parent_metadata = MetadataDocument::new("parent-session", "project-tag", "parent", 3);
        let runtime_lock = RuntimeLock {
            initialization_state:
                vulcan_session_core::v1::context::RuntimeInitializationState::Initialized,
            current_compaction_id: None,
            locked_sources: Vec::new(),
        };
        let parent_request = creation_fixture(&parent_metadata, &runtime_lock);
        let parent_row = create_index_row(&parent_request).expect("valid parent row");
        let parent = create_indexed_session_v1_with_insert(
            data_root.path(),
            &parent_request,
            parent_row,
            |_row| async { Ok(()) },
        )
        .await
        .expect("published parent");
        let child_metadata = MetadataDocument::new("child-session", "other-project", "child", 4);
        let child_request = CreateEmbeddedTaskSessionV1Request {
            metadata: &child_metadata,
            runtime_lock: &runtime_lock,
            compactions: &[],
            events: &[],
        };

        let error = create_embedded_task_session_v1(&parent.locator, &child_request)
            .expect_err("project drift must fail");

        assert!(matches!(
            error,
            SessionV1StoreError::Index(SessionIndexV1Error::InvalidLocator { .. })
        ));
        assert!(
            !data_root
                .path()
                .join("sessions/project-tag/2026-08-27-parent-session/TASK/task-child-session")
                .exists()
        );
    }

    /// Locator decoding rejects redundant fields that disagree with the exact path.
    /// 定位解析会拒绝与精确路径不一致的冗余字段。
    #[test]
    fn locator_rejects_project_partition_mismatch() {
        let data_root = tempfile::tempdir().expect("temporary data root");
        let row = SessionIndexV1Row {
            session_id: "session-4".to_string(),
            project_id: "project-id".to_string(),
            project_tag: "project-tag".to_string(),
            storage_partition: SESSION_V1_STORAGE_PARTITION.to_string(),
            relative_path: "other-tag/2026-08-27-session-4".to_string(),
            title: "title".to_string(),
            created_at_ms: 4,
            updated_at_ms: 4,
            archived_at_ms: None,
            unread: 0,
        };

        let error = storage_locator_from_row(data_root.path(), row)
            .expect_err("mismatched project partition must fail");

        assert!(matches!(error, SessionIndexV1Error::InvalidLocator { .. }));
    }

    /// The production session partition is joined exactly once below the application data root.
    /// 生产会话分区只在应用数据根目录下拼接一次。
    #[test]
    fn production_partition_is_directly_below_application_data_root() {
        let profile_root = tempfile::tempdir().expect("temporary profile root");
        let application_data_root = profile_root.path().join("code");
        let row = SessionIndexV1Row {
            session_id: "session-direct-root".to_string(),
            project_id: "project-id".to_string(),
            project_tag: "project-tag".to_string(),
            storage_partition: SESSION_V1_STORAGE_PARTITION.to_string(),
            relative_path: "project-tag/2026-08-29-session-direct-root".to_string(),
            title: "title".to_string(),
            created_at_ms: 1,
            updated_at_ms: 1,
            archived_at_ms: None,
            unread: 0,
        };

        let locator = storage_locator_from_row(&application_data_root, row).expect("exact locator");

        assert_eq!(
            locator.session_directory(),
            application_data_root.join("sessions/project-tag/2026-08-29-session-direct-root")
        );
        assert!(
            !locator
                .session_directory()
                .starts_with(application_data_root.join("code"))
        );
    }

    /// Obsolete locators that prepend `code` a second time are unsupported and omitted from lists.
    /// 会再次前置 `code` 的过时定位信息不受支持，并从列表中忽略。
    #[test]
    fn legacy_duplicate_code_partition_is_rejected_and_omitted_from_lists() {
        let data_root = tempfile::tempdir().expect("temporary application data root");
        let row = SessionIndexV1Row {
            session_id: "session-legacy-root".to_string(),
            project_id: "project-id".to_string(),
            project_tag: "project-tag".to_string(),
            storage_partition: "code/sessions".to_string(),
            relative_path: "project-tag/2026-08-29-session-legacy-root".to_string(),
            title: "legacy".to_string(),
            created_at_ms: 1,
            updated_at_ms: 1,
            archived_at_ms: None,
            unread: 0,
        };

        let error = storage_locator_from_row(data_root.path(), row.clone())
            .expect_err("duplicate code partition must be unsupported");

        assert!(matches!(error, SessionIndexV1Error::InvalidLocator { .. }));
        assert!(
            current_partition_list_entries(data_root.path(), vec![row])
                .expect("retired partition is deliberately omitted")
                .is_empty()
        );
    }

    /// Missing metadata is reported as absence and never triggers a sibling scan.
    /// metadata 缺失会报告为不存在，绝不触发同级目录扫描。
    #[test]
    fn restore_precheck_uses_only_the_exact_metadata_path() {
        let data_root = tempfile::tempdir().expect("temporary data root");
        let row = SessionIndexV1Row {
            session_id: "session-5".to_string(),
            project_id: "project-id".to_string(),
            project_tag: "project-tag".to_string(),
            storage_partition: SESSION_V1_STORAGE_PARTITION.to_string(),
            relative_path: "project-tag/2026-08-27-session-5".to_string(),
            title: "title".to_string(),
            created_at_ms: 5,
            updated_at_ms: 5,
            archived_at_ms: Some(5),
            unread: 0,
        };
        let locator = storage_locator_from_row(data_root.path(), row).expect("exact locator");
        fs::create_dir_all(
            data_root
                .path()
                .join("sessions/project-tag/another-session"),
        )
        .expect("create unrelated sibling");
        assert!(!exact_metadata_file_exists(&locator).expect("missing metadata outcome"));
    }

    /// Exact directory deletion is idempotent and does not touch project-level resources.
    /// 精确目录删除具有幂等性，且不触碰项目级资源。
    #[test]
    fn exact_delete_is_idempotent_and_preserves_project_resources() {
        let data_root = tempfile::tempdir().expect("temporary data root");
        let row = SessionIndexV1Row {
            session_id: "session-6".to_string(),
            project_id: "project-id".to_string(),
            project_tag: "project-tag".to_string(),
            storage_partition: SESSION_V1_STORAGE_PARTITION.to_string(),
            relative_path: "project-tag/2026-08-27-session-6".to_string(),
            title: "title".to_string(),
            created_at_ms: 6,
            updated_at_ms: 6,
            archived_at_ms: None,
            unread: 0,
        };
        let locator = storage_locator_from_row(data_root.path(), row).expect("exact locator");
        fs::create_dir_all(locator.layout.session_dir()).expect("create session directory");
        let project_resource = data_root.path().join("project-resources/keep.bin");
        fs::create_dir_all(project_resource.parent().expect("resource parent"))
            .expect("create resource parent");
        fs::write(&project_resource, b"keep").expect("write resource fixture");
        assert!(remove_exact_session_directory(&locator).expect("first delete"));
        assert!(!remove_exact_session_directory(&locator).expect("idempotent delete"));
        assert_eq!(
            fs::read(project_resource).expect("resource preserved"),
            b"keep"
        );
    }

    /// An unusable database is surfaced as an explicit index schema failure.
    /// 不可用的数据库会显式返回索引结构失败。
    ///
    /// This replaces an earlier assertion that a *disconnected host_runtime* produced the failure. That
    /// premise no longer exists: the session index now executes against an already-open in-process
    /// database, so the transport cannot be the reason. What survives, and is what this checks, is
    /// that a database this layer cannot use is reported explicitly instead of being treated as a
    /// successfully prepared schema.
    ///
    /// 本测试取代了先前"*未连接宿主运行时*导致失败"的断言。该前提已不存在：会话索引现在针对一个已打开的
    /// 进程内数据库执行，因此传输层不可能是原因。保留下来的、也是本测试所检查的，是本层无法使用的数据库
    /// 会被显式报告，而不是被当作已成功准备好的结构。
    #[tokio::test]
    async fn unusable_database_is_an_explicit_index_schema_failure() {
        let temporary = tempfile::tempdir().expect("temporary data root");

        // A regular file where the database's parent directory belongs makes the open fail
        // deterministically, and the assertion below pins that the reported reason names that path —
        // which is only possible if the failure came from the local engine.
        // 在本应是数据库父目录的位置放一个普通文件会使打开确定性地失败，下方断言固定报告的原因会指明该
        // 路径——只有失败来自本地引擎时才可能如此。
        let blocker = temporary.path().join("blocked-parent");
        fs::write(&blocker, b"not a directory").expect("write blocking file");

        let host_runtime = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
            data_root: temporary.path().to_path_buf(),
            sqlite_path: blocker.join("app.sqlite3"),
            sqlite_binding_id: "unusable".to_string(),
            system_statistics_sqlite_path: temporary.path().join("statistics.sqlite3"),
            system_statistics_sqlite_binding_id: "unusable-statistics".to_string(),
        });

        match ensure_session_v1_storage_schema(&host_runtime).await {
            Err(SessionV1StoreError::Index(SessionIndexV1Error::Schema { detail })) => {
                assert!(
                    detail.contains("blocked-parent"),
                    "the failure must name the database path rather than the transport: {detail}"
                );
            }
            other => panic!("expected an explicit index schema failure, got {other:?}"),
        }

        // Nothing may have been created next to the blocker: a refused open is not a partial setup.
        // 阻塞物旁不得有任何东西被创建：被拒绝的打开不是部分完成的初始化。
        assert_eq!(
            fs::read_dir(temporary.path())
                .expect("list temporary root")
                .count(),
            1,
            "the refused database must not have created any file"
        );
    }

    /// Exercises the V1 index lifecycle, row failure, transaction failure, and lock conflict.
    /// 验证 V1 索引生命周期、记录失败、事务失败与锁冲突。
    ///
    /// The relational work needs no host_runtime: the database is opened directly through the same
    /// in-process helpers production code uses, and the handle's client is never connected, so any
    /// code that reached the transport would fail rather than silently succeed. The external SQLite
    /// lock holder still needs the managed Python runtime, which is what the ignore reason names.
    ///
    /// 关系侧工作不需要宿主运行时：数据库通过生产代码所用的同一批进程内辅助函数直接打开，且句柄的客户端从未
    /// 连接，因此任何触达传输层的代码都会失败而不是静默成功。外部 SQLite 锁持有者仍需托管 Python 运行时，
    /// 这正是忽略原因所指。
    #[tokio::test(flavor = "current_thread")]
    #[ignore = "requires the managed Python runtime for the external SQLite lock holder"]
    async fn in_process_session_v1_lifecycle_and_failures() {
        let temporary = tempfile::tempdir().expect("temporary V1 root");
        let data_root = temporary.path().join("data");
        let sqlite_path = data_root.join("sqlite/app-workspace.sqlite3");
        fs::create_dir_all(sqlite_path.parent().expect("SQLite parent"))
            .expect("create SQLite parent");

        let host_runtime = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
            data_root: data_root.clone(),
            sqlite_path: sqlite_path.clone(),
            sqlite_binding_id: "session-v1-lifecycle".to_string(),
            system_statistics_sqlite_path: sqlite_path.clone(),
            system_statistics_sqlite_binding_id: "session-v1-lifecycle-statistics".to_string(),
        });
        let app = host_runtime.local.as_ref();
        // Seed the production v1 schema, then drop the V1 session index. That makes this fixture an
        // old profile in the exact sense the loading path must tolerate: every other table is
        // current and only the index it must survive without is absent.
        //
        // 播种生产 v1 结构，然后删除 V1 会话索引。这使该夹具成为加载路径必须容忍的确切意义上的旧配置：
        // 其他表都是当前的，只有它必须在其缺失下存活的索引不存在。
        //
        // A hand-written minimal schema was used before, and it had already rotted: it lacked
        // `project_vce_settings`, which the project query joins, so this fixture could not have
        // loaded even before the migration. Seeding the real schema is what keeps it from rotting
        // again, because a new table arrives here automatically.
        //
        // 此前使用的是手写极简结构，而它已经腐坏：它缺少项目查询要连接的 `project_vce_settings`，因此
        // 该夹具在迁移之前就已无法加载。播种真实结构正是防止它再次腐坏的办法，因为新增的表会自动出现在这里。
        local_sql::execute_script(
            app,
            LogicalDatabase::AppState,
            &crate::data_plane::build_application_schema_script(),
            &[],
        )
        .expect("create the production application schema");
        local_sql::execute_script(
            app,
            LogicalDatabase::AppState,
            "DROP TABLE IF EXISTS session_index_v1;",
            &[],
        )
        .expect("drop the V1 session index to model an old profile");
        local_sql::execute_batch(
            app,
            LogicalDatabase::AppState,
            r#"
INSERT INTO projects (
    id, name, root_path, pinned, sort_order, icon_pack, icon_key,
    codebase_memory_enabled, archived, created_at, updated_at
) VALUES (?1, ?2, ?3, 0, 0, NULL, NULL, 0, 0, '1', '1');
"#,
            &[vec![
                SqliteValue::Text("project-id".to_string()),
                SqliteValue::Text("Project".to_string()),
                SqliteValue::Text(crate::runtime_assets::native_host_path_text_from_path(
                    &data_root,
                )),
            ]],
        )
        .expect("insert legacy workspace project")
        .success
        .then_some(())
        .expect("legacy workspace project insert must succeed");
        // The project query inner-joins these settings, so the row is part of a loadable profile.
        // 项目查询内连接这些设置，因此该行是可加载配置的一部分。
        local_sql::execute_batch(
            app,
            LogicalDatabase::AppState,
            "INSERT INTO project_vce_settings (project_id, updated_at) VALUES (?1, '1');",
            &[vec![SqliteValue::Text("project-id".to_string())]],
        )
        .expect("insert legacy project VCE settings")
        .success
        .then_some(())
        .expect("legacy project VCE settings insert must succeed");

        let startup_snapshot =
            super::super::loading::load_workspace_index_snapshot(host_runtime.clone())
                .await
                .expect("old profile without V1 session index must still load");
        assert!(startup_snapshot.sessions.is_empty());
        assert!(startup_snapshot.archived_sessions.is_empty());
        local_sql::execute_batch(
            app,
            LogicalDatabase::AppState,
            r#"
INSERT INTO session_index_v1 (
    session_id, project_id, project_tag, storage_partition, relative_path,
    title, created_at_ms, updated_at_ms, archived_at_ms, unread
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, 1, NULL, 0);
"#,
            &[vec![
                SqliteValue::Text("not-a-session-uuid".to_string()),
                SqliteValue::Text("project-id".to_string()),
                SqliteValue::Text("project-tag".to_string()),
                SqliteValue::Text(SESSION_V1_STORAGE_PARTITION.to_string()),
                SqliteValue::Text("project-tag/2026-08-28-not-a-session-uuid".to_string()),
                SqliteValue::Text("Unsupported session".to_string()),
            ]],
        )
        .expect("insert unsupported session row")
        .success
        .then_some(())
        .expect("unsupported session row insert must succeed");
        let unsupported_report =
            super::super::loading::load_workspace_index_report(host_runtime.clone())
                .await
                .expect("unsupported session row must not block tolerant workspace startup");
        assert!(unsupported_report.snapshot.sessions.is_empty());
        assert!(
            !unsupported_report.is_complete(),
            "an unsupported current row must be reported"
        );
        assert!(
            super::super::loading::load_workspace_index_snapshot(host_runtime.clone())
                .await
                .is_err(),
            "a refresh must not publish the partial list as an authoritative replacement"
        );
        assert!(
            locate_session_index_v1(&host_runtime, "not-a-session-uuid")
                .await
                .expect("index remains readable")
                .is_some(),
            "reporting invalid data must not delete it"
        );
        local_sql::execute_batch(
            app,
            LogicalDatabase::AppState,
            "DELETE FROM session_index_v1 WHERE session_id = ?1;",
            &[vec![SqliteValue::Text("not-a-session-uuid".to_string())]],
        )
        .expect("delete unsupported session row")
        .success
        .then_some(())
        .expect("unsupported session row deletion must succeed");

        let metadata = MetadataDocument::new("session-e2e", "project-tag", "title", 1);
        let runtime_lock = RuntimeLock {
            initialization_state:
                vulcan_session_core::v1::context::RuntimeInitializationState::Initialized,
            current_compaction_id: None,
            locked_sources: Vec::new(),
        };
        let request = creation_fixture(&metadata, &runtime_lock);
        let created = create_indexed_session_v1(&host_runtime, &request)
            .await
            .expect("create indexed V1 session");
        assert!(created.locator.layout.metadata_path().is_file());
        assert_eq!(
            list_active_sessions_v1(&host_runtime, "project-id")
                .await
                .expect("list active sessions")
                .len(),
            1
        );
        assert!(matches!(
            create_indexed_session_v1(&host_runtime, &request).await,
            Err(SessionV1StoreError::Index(
                SessionIndexV1Error::Conflict { .. }
            ))
        ));

        // archived_metadata is the durable file authority projected into the row-only index.
        // archived_metadata 是投影到纯行索引中的可靠文件权威。
        let mut archived_metadata = metadata.clone();
        archived_metadata.archived_at_ms = Some(2);
        archived_metadata.updated_at_ms = 2;
        project_session_lifecycle_v1(&host_runtime, &created.locator, &archived_metadata)
            .await
            .expect("archive V1 session");
        assert_eq!(
            list_archived_sessions_v1(&host_runtime, "project-id")
                .await
                .expect("list archived sessions")
                .len(),
            1
        );
        assert!(matches!(
            preflight_session_v1_storage(&host_runtime, "session-e2e")
                .await
                .expect("preflight V1 session restore"),
            SessionV1StoragePreflightOutcome::Ready { .. }
        ));
        project_session_lifecycle_v1(&host_runtime, &created.locator, &metadata)
            .await
            .expect("project authoritative unarchived metadata");
        set_session_unread_v1(&host_runtime, "session-e2e", true)
            .await
            .expect("mark indexed V1 session unread");
        assert_eq!(
            list_active_sessions_v1(&host_runtime, "project-id")
                .await
                .expect("list unread active session")[0]
                .unread,
            1
        );
        set_session_unread_v1(&host_runtime, "session-e2e", false)
            .await
            .expect("mark indexed V1 session read");
        assert_eq!(
            list_active_sessions_v1(&host_runtime, "project-id")
                .await
                .expect("list read active session")[0]
                .unread,
            0
        );
        assert!(matches!(
            set_session_unread_v1(&host_runtime, "missing-session", false).await,
            Err(SessionV1StoreError::Index(
                SessionIndexV1Error::Missing { .. }
            ))
        ));
        // renamed_metadata proves list projection consumes the complete authoritative metadata.
        // renamed_metadata 证明列表投影消费完整的权威 metadata。
        let mut renamed_metadata = metadata.clone();
        renamed_metadata.title = "renamed".to_string();
        renamed_metadata.updated_at_ms = 4;
        project_session_lifecycle_v1(&host_runtime, &created.locator, &renamed_metadata)
            .await
            .expect("rename V1 session");

        local_sql::execute_script(
            app,
            LogicalDatabase::AppState,
            r#"
CREATE TRIGGER fail_v1_archive
BEFORE UPDATE OF archived_at_ms ON session_index_v1
BEGIN
    SELECT RAISE(ABORT, 'injected V1 transaction failure');
END;
"#,
            &[],
        )
        .expect("install transaction failure trigger");
        assert!(matches!(
            {
                // failed_archive_metadata carries the exact projection rejected by the trigger.
                // failed_archive_metadata 携带被触发器拒绝的精确投影。
                let mut failed_archive_metadata = renamed_metadata.clone();
                failed_archive_metadata.archived_at_ms = Some(5);
                failed_archive_metadata.updated_at_ms = 5;
                project_session_lifecycle_v1(
                    &host_runtime,
                    &created.locator,
                    &failed_archive_metadata,
                )
                .await
            },
            Err(SessionV1StoreError::Index(
                SessionIndexV1Error::Write { .. }
            ))
        ));
        local_sql::execute_script(
            app,
            LogicalDatabase::AppState,
            "DROP TRIGGER fail_v1_archive;",
            &[],
        )
        .expect("drop transaction failure trigger");

        let lock_holder = start_sqlite_lock_holder(&sqlite_path);
        assert!(matches!(
            {
                // locked_archive_metadata carries the exact projection rejected by the file lock.
                // locked_archive_metadata 携带被文件锁拒绝的精确投影。
                let mut locked_archive_metadata = renamed_metadata.clone();
                locked_archive_metadata.archived_at_ms = Some(6);
                locked_archive_metadata.updated_at_ms = 6;
                project_session_lifecycle_v1(
                    &host_runtime,
                    &created.locator,
                    &locked_archive_metadata,
                )
                .await
            },
            Err(SessionV1StoreError::Index(
                SessionIndexV1Error::Write { .. }
            ))
        ));
        lock_holder.release();

        // final_archive_metadata is the committed projection used by missing-file preflight.
        // final_archive_metadata 是缺失文件预检使用的已提交投影。
        let mut final_archive_metadata = renamed_metadata;
        final_archive_metadata.archived_at_ms = Some(7);
        final_archive_metadata.updated_at_ms = 7;
        project_session_lifecycle_v1(&host_runtime, &created.locator, &final_archive_metadata)
            .await
            .expect("archive before missing-file restore");
        fs::remove_file(created.locator.layout.metadata_path())
            .expect("remove exact metadata fixture");
        assert_eq!(
            preflight_session_v1_storage(&host_runtime, "session-e2e")
                .await
                .expect("clean missing restore row"),
            SessionV1StoragePreflightOutcome::MissingStorageIndexRemoved {
                session_id: "session-e2e".to_string(),
            }
        );
        assert!(
            list_archived_sessions_v1(&host_runtime, "project-id")
                .await
                .expect("list after stale-row cleanup")
                .is_empty()
        );

        let delete_metadata = MetadataDocument::new("session-delete", "project-tag", "title", 9);
        let delete_request = creation_fixture(&delete_metadata, &runtime_lock);
        create_indexed_session_v1(&host_runtime, &delete_request)
            .await
            .expect("create deletion fixture");
        assert_eq!(
            delete_session_v1(&host_runtime, "session-delete")
                .await
                .expect("delete V1 session"),
            DeleteSessionV1Outcome {
                index_existed: true,
                directory_removed: true,
            }
        );
        assert_eq!(
            delete_session_v1(&host_runtime, "session-delete")
                .await
                .expect("repeat V1 deletion"),
            DeleteSessionV1Outcome {
                index_existed: false,
                directory_removed: false,
            }
        );
    }
}
