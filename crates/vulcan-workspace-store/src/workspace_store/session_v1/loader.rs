//! Strict V1 session loader with precise segment access.
//! 带精确分片访问的严格 V1 会话加载器。
//!
//! Loading touches only metadata, runtime, compactions, and the exact segment
//! files named by the split map. It never scans directories, never searches
//! for candidates and never rewrites files. The explicit runtime entry may stage a narrowly validated recovery.
//! `load_session` retains the full stream; `load_session_tail_only` retains
//! the active tail plus the exact open-turn/retained-goal interval and reports its metrics: sealed segments
//! stay logically addressable through the catalog and are validated on their
//! first precise load (plan 6.1.4/6.1.8).
//! 加载只接触 metadata、runtime、compactions 与分片映射精确命名的分片文件。
//! 它绝不扫描目录、绝不寻找候选、绝不重写文件。显式运行时入口可暂存严格限定的恢复候选。
//! `load_session` 保留完整流；`load_session_tail_only` 保留活动尾段与精确的开放回合、保留目标区间并报告
//! 其指标(审计 P0#2)：封口分片通过 catalog 保持逻辑可寻址，并在首次精确
//! 加载时校验(计划 6.1.4/6.1.8)。

use vulcan_session_core::v1::compaction_frontier::assert_node_order;
use vulcan_session_core::v1::config::{MetadataDocument, SegmentInfo};
use vulcan_session_core::v1::context::{CompactionsDocument, RuntimeDocument, RuntimeLock};
use vulcan_session_core::v1::document::SessionDocument;
use vulcan_session_core::v1::event::{ConversationEvent, EventBody, UserInputSource};
use vulcan_session_core::v1::ids::TurnId;

use super::documents::{map_domain_error, read_compactions, read_metadata, read_runtime};
use super::errors::{ErrorCode, SessionV1Error};
use super::layout::SessionV1Layout;
use super::segment::{parse_segment, segment_byte_len};

/// One fully loaded V1 session.
/// 一个完整加载的 V1 会话。
pub struct LoadedV1Session {
    /// The authoritative in-memory document with rebuilt derived state.
    /// 已重建派生状态的权威内存文档。
    pub document: SessionDocument,
    /// The metadata document as loaded from disk.
    /// 从磁盘加载的 metadata 文档。
    pub metadata: MetadataDocument,
}

/// Metrics of one load, satisfying the audit P0#2 acceptance: resident memory,
/// deserialized event count, and opened segment files are recorded.
/// 一次加载的指标，满足审计 P0#2 验收：记录常驻内存、反序列化事件数与打开
/// 分片数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadReport {
    /// Segment files actually opened and parsed.
    /// 实际打开并解析的分片文件数。
    pub opened_segment_files: u64,
    /// Events deserialized during the load.
    /// 加载期间反序列化的事件数。
    pub parsed_event_count: u64,
    /// Events retained resident in the returned document.
    /// 返回文档中常驻保留的事件数。
    pub resident_event_count: u64,
    /// Total split-map entries (sealed + active tail).
    /// 分片映射总条目数(封口 + 活动尾段)。
    pub catalog_entries: u64,
}

/// The three small JSON documents of one session.
/// 一个会话的三份小型 JSON 文档。
struct SmallDocuments {
    /// metadata document.
    /// metadata 文档。
    metadata: MetadataDocument,
    /// runtime document.
    /// runtime 文档。
    runtime_document: RuntimeDocument,
    /// compactions document.
    /// compactions 文档。
    compactions_document: CompactionsDocument,
}

/// Strictly loads one V1 session from its exact locator, retaining every event.
/// 从精确定位器严格加载一个 V1 会话并保留全部事件。
pub fn load_session(layout: &SessionV1Layout) -> Result<LoadedV1Session, SessionV1Error> {
    let SmallDocuments {
        metadata,
        runtime_document,
        compactions_document,
    } = validate_small_documents(layout)?;
    validate_split_chain(&metadata, layout)?;
    let mut events = Vec::new();
    let mut expected_start = 1u64;
    let mut expected_next_event_local_id = 1u64;
    for entry in &metadata.split_list {
        let (start, file_events) = load_segment_entry(layout, &metadata, entry, expected_start)?;
        let actual_next_event_local_id = expected_next_event_local_id.max(
            vulcan_session_core::v1::event::next_event_local_id(&file_events),
        );
        if entry.end_seq.is_some() && entry.next_event_local_id != actual_next_event_local_id {
            return Err(SessionV1Error::corrupt(
                ErrorCode::MetadataCorrupt,
                layout.metadata_path(),
                format!(
                    "sealed segment {} next_event_local_id {} does not match event facts {}",
                    entry.segment_number, entry.next_event_local_id, actual_next_event_local_id
                ),
            )
            .with_context(&layout.session_id, layout.metadata_path()));
        }
        if entry.end_seq.is_none() && entry.next_event_local_id > actual_next_event_local_id {
            return Err(SessionV1Error::corrupt(
                ErrorCode::MetadataCorrupt,
                layout.metadata_path(),
                format!(
                    "active segment {} next_event_local_id {} is ahead of event facts {}",
                    entry.segment_number, entry.next_event_local_id, actual_next_event_local_id
                ),
            )
            .with_context(&layout.session_id, layout.metadata_path()));
        }
        expected_next_event_local_id = actual_next_event_local_id;
        expected_start = start;
        events.extend(file_events);
    }
    let runtime_lock = RuntimeLock {
        initialization_state: runtime_document.initialization_state,
        current_compaction_id: runtime_document.current_compaction_id,
        locked_sources: runtime_document.locked_sources,
    };
    let compactions = compactions_document.nodes;
    // The allocator frontier is a cache: facts win when it lags behind, but a
    // frontier AHEAD of the facts signals lost or inconsistent state and must
    // be surfaced instead of silently rewound (plan 5.2.16).
    // 分配器 frontier 是缓存：滞后于事实时以事实为准；但 frontier 超前于事实
    // 说明状态缺失或不一致，必须显式暴露而不是静默回卷(计划 5.2.16)。
    let verified_document = SessionDocument::from_parts(
        metadata.clone(),
        runtime_lock,
        compactions,
        metadata.split_list.clone(),
        events,
    )
    .map_err(|error| map_domain_error(error, &layout.session_id, layout.metadata_path()))?;
    verify_frontier_not_ahead(&verified_document, &metadata, layout)?;
    Ok(LoadedV1Session {
        document: verified_document,
        metadata,
    })
}

/// Loads one session retaining only the active tail, and reports the load
/// metrics (audit P0#2). Sealed segments are validated structurally from the
/// split map only; their content is validated on first precise load (plan
/// 6.1.8), so a 100k-event session opens without materializing sealed history.
/// 只保留活动尾段加载一个会话，并报告加载指标(审计 P0#2)。封口分片仅按分片
/// 映射做结构性校验；内容在首次精确加载时校验(计划 6.1.8)，因此 10 万事件
/// 会话打开时不物化封口历史。
pub fn load_session_tail_only(
    layout: &SessionV1Layout,
) -> Result<(LoadedV1Session, LoadReport), SessionV1Error> {
    load_session_tail_with_recovery(layout, false)
}

/// Loads `layout` for a runtime owner, staging proven tail-only Goal corrections without writing files.
/// 为运行时 owner 加载 `layout`，暂存已证实的仅尾段目标纠正，不写文件。
/// Returns the validated candidate and metrics; the owner must persist dirty recovery before publication.
/// 返回已校验候选与指标；owner 必须在发布前持久化待写恢复内容。
pub(crate) fn load_session_tail_for_runtime(
    layout: &SessionV1Layout,
) -> Result<(LoadedV1Session, LoadReport), SessionV1Error> {
    load_session_tail_with_recovery(layout, true)
}

/// Reads exact session parts; `recover_goal_owners` selects strict or narrowly recoverable construction.
/// 读取精确会话部件；`recover_goal_owners` 选择严格构建或限定可恢复构建。
/// Returns no partial document on validation failure and performs no persistence.
/// 校验失败不返回部分文档，也不执行持久化。
fn load_session_tail_with_recovery(
    layout: &SessionV1Layout,
    recover_goal_owners: bool,
) -> Result<(LoadedV1Session, LoadReport), SessionV1Error> {
    let SmallDocuments {
        metadata,
        runtime_document,
        compactions_document,
    } = validate_small_documents(layout)?;
    validate_split_chain(&metadata, layout)?;
    let tail_entry = metadata.split_list.last().expect("validated: non-empty");
    let tail_path = layout.segment_entry_path(metadata.segment_generation, &tail_entry.file_name);
    if !tail_path.exists() {
        return Err(
            SessionV1Error::segment_missing(&tail_path, tail_entry.segment_number)
                .with_context(&inner_session_id(layout), &tail_path),
        );
    }
    let tail_events = parse_segment(
        &tail_path,
        tail_entry.segment_number,
        Some(tail_entry.start_seq),
    )
    .map_err(|error| error.with_context(&inner_session_id(layout), &tail_path))?;
    let prefix_next_event_local_id = metadata
        .split_list
        .iter()
        .rev()
        .nth(1)
        .map(|entry| entry.next_event_local_id)
        .unwrap_or(1);
    let actual_next_event_local_id = prefix_next_event_local_id.max(
        vulcan_session_core::v1::event::next_event_local_id(&tail_events),
    );
    if tail_entry.next_event_local_id > actual_next_event_local_id {
        return Err(SessionV1Error::corrupt(
            ErrorCode::MetadataCorrupt,
            layout.metadata_path(),
            format!(
                "active segment {} next_event_local_id {} is ahead of event facts {}",
                tail_entry.segment_number,
                tail_entry.next_event_local_id,
                actual_next_event_local_id
            ),
        )
        .with_context(&layout.session_id, layout.metadata_path()));
    }
    let (resident_events, opened_segment_files, parsed_event_count) =
        load_open_turn_resident_events(layout, &metadata, tail_events)?;
    // A retained goal owns cumulative facts across turns; its origin cannot be dropped at a segment rollover.
    // 保留的目标拥有跨回合累计事实，分片滚动时不得丢失其起点。
    let (resident_events, goal_opened_files, goal_parsed_events) =
        load_goal_resident_prefix(layout, &metadata, resident_events)?;
    let report = LoadReport {
        opened_segment_files: opened_segment_files + goal_opened_files,
        parsed_event_count: parsed_event_count + goal_parsed_events,
        resident_event_count: resident_events.len() as u64,
        catalog_entries: metadata.split_list.len() as u64,
    };
    let runtime_lock = RuntimeLock {
        initialization_state: runtime_document.initialization_state,
        current_compaction_id: runtime_document.current_compaction_id,
        locked_sources: runtime_document.locked_sources,
    };
    let compactions = compactions_document.nodes;
    // Both constructors run the same strict validators; runtime recovery corrects only its proven pair.
    // 两种构建器都执行相同严格校验；运行时恢复仅纠正已证实的事件对。
    let restore = if recover_goal_owners {
        SessionDocument::from_parts_with_goal_owner_recovery
    } else {
        SessionDocument::from_parts
    };
    let document = restore(
        metadata.clone(),
        runtime_lock,
        compactions,
        metadata.split_list.clone(),
        resident_events,
    )
    .map_err(|error| map_domain_error(error, &layout.session_id, layout.metadata_path()))?;
    verify_frontier_not_ahead(&document, &metadata, layout)?;
    Ok((LoadedV1Session { document, metadata }, report))
}

/// Loads only the retained goal's missing prefix; sessions without goals keep the ordinary tail-only path.
/// 只加载保留目标缺失的前缀；无目标会话仍沿用普通仅尾段路径。
/// `resident` already covers the open turn; returns the required contiguous stream and incremental I/O counts.
/// `resident` 已覆盖开放回合；返回必需的连续流及新增 I/O 计数。
fn load_goal_resident_prefix(
    layout: &SessionV1Layout,
    metadata: &MetadataDocument,
    resident: Vec<ConversationEvent>,
) -> Result<(Vec<ConversationEvent>, u64, u64), SessionV1Error> {
    let goal = &metadata.mode.goal;
    let Some(origin) = goal.origin_seq.filter(|_| goal.enabled) else {
        return Ok((resident, 0, 0));
    };
    if resident.iter().any(|event| event.seq == origin) {
        return Ok((resident, 0, 0));
    }
    // The open-turn loader always starts at a segment boundary, so no segment is parsed twice.
    // 开放回合加载器始终从分片边界开始，因此不会重复解析分片。
    let resident_start = resident
        .first()
        .map(|event| event.seq.value())
        .unwrap_or_else(|| {
            metadata
                .split_list
                .last()
                .expect("validated split map")
                .start_seq
                .value()
        });
    let mut prefix = Vec::new();
    let mut opened_files = 0;
    for entry in &metadata.split_list {
        if entry.start_seq.value() >= resident_start {
            break;
        }
        if entry.end_seq.is_some_and(|end| end < origin) {
            continue;
        }
        let (_, mut events) = load_segment_entry(layout, metadata, entry, entry.start_seq.value())?;
        opened_files += 1;
        prefix.append(&mut events);
    }
    if !prefix.iter().any(|event| event.seq == origin) {
        return Err(SessionV1Error::corrupt(
            ErrorCode::ReferenceViolation,
            layout.metadata_path(),
            format!("goal origin_seq {} has no persisted event", origin.value()),
        )
        .with_context(&layout.session_id, layout.metadata_path()));
    }
    let parsed_events = prefix.len() as u64;
    prefix.extend(resident);
    Ok((prefix, opened_files, parsed_events))
}

/// Extends the complete active tail backwards only to the cached open-Turn
/// start. Tail facts override a stale cursor when the Turn started or closed
/// in a tail write that reached disk before metadata.
/// 仅把完整活动尾段向前扩展到缓存的开放回合起点；若回合开始或结束的尾段写入
/// 先于 metadata 落盘，则尾段事实优先于滞后的游标。
fn load_open_turn_resident_events(
    layout: &SessionV1Layout,
    metadata: &MetadataDocument,
    tail_events: Vec<ConversationEvent>,
) -> Result<(Vec<ConversationEvent>, u64, u64), SessionV1Error> {
    let tail_event_count = tail_events.len() as u64;
    if let Some((turn_id, closed)) = latest_turn_state(&tail_events) {
        if closed
            || explicit_turn_start_index(&tail_events, turn_id, metadata.open_turn_start_seq)
                .is_some()
        {
            return Ok((tail_events, 1, tail_event_count));
        }
    } else if metadata.open_turn_start_seq.is_none() {
        return Ok((tail_events, 1, tail_event_count));
    }

    let open_turn_start = metadata.open_turn_start_seq.ok_or_else(|| {
        SessionV1Error::corrupt(
            ErrorCode::ReferenceViolation,
            layout.metadata_path(),
            "active tail contains an open Turn continuation but metadata has no open_turn_start_seq",
        )
        .with_context(&layout.session_id, layout.metadata_path())
    })?;
    let first_entry_index = metadata
        .split_list
        .iter()
        .position(|entry| {
            entry.start_seq.value() <= open_turn_start.value()
                && entry
                    .end_seq
                    .map(|end| open_turn_start.value() <= end.value())
                    .unwrap_or(true)
        })
        .ok_or_else(|| {
            SessionV1Error::corrupt(
                ErrorCode::ReferenceViolation,
                layout.metadata_path(),
                format!(
                    "open_turn_start_seq {} is outside the split map",
                    open_turn_start.value()
                ),
            )
            .with_context(&layout.session_id, layout.metadata_path())
        })?;
    let mut resident = Vec::new();
    let mut opened_segment_files = 0u64;
    let mut parsed_event_count = 0u64;
    for entry in &metadata.split_list[first_entry_index..metadata.split_list.len() - 1] {
        let (_, mut events) = load_segment_entry(layout, metadata, entry, entry.start_seq.value())?;
        opened_segment_files += 1;
        parsed_event_count += events.len() as u64;
        resident.append(&mut events);
    }
    resident.extend(tail_events);
    opened_segment_files += 1;
    parsed_event_count += tail_event_count;
    let (turn_id, closed) = latest_turn_state(&resident).ok_or_else(|| {
        SessionV1Error::corrupt(
            ErrorCode::ReferenceViolation,
            layout.metadata_path(),
            format!(
                "open_turn_start_seq {} resolves to no Turn events",
                open_turn_start.value()
            ),
        )
        .with_context(&layout.session_id, layout.metadata_path())
    })?;
    let actual_start = explicit_turn_start_index(&resident, turn_id, metadata.open_turn_start_seq)
        .map(|index| resident[index].seq)
        .ok_or_else(|| {
            SessionV1Error::corrupt(
                ErrorCode::ReferenceViolation,
                layout.metadata_path(),
                format!("open turn {} has no explicit start event", turn_id.value()),
            )
            .with_context(&layout.session_id, layout.metadata_path())
        })?;
    if closed || actual_start != open_turn_start {
        return Err(SessionV1Error::corrupt(
            ErrorCode::ReferenceViolation,
            layout.metadata_path(),
            format!(
                "open Turn cursor {} does not match resident facts: start {}, closed {closed}",
                open_turn_start.value(),
                actual_start.value()
            ),
        )
        .with_context(&layout.session_id, layout.metadata_path()));
    }
    Ok((resident, opened_segment_files, parsed_event_count))
}

/// Returns the latest turn id and whether its terminal statistics are present.
/// 返回最新回合 id 以及其终态统计是否存在。
fn latest_turn_state(events: &[ConversationEvent]) -> Option<(TurnId, bool)> {
    let turn_id = events.iter().rev().find_map(|event| event.turn_id)?;
    let closed = events.iter().any(|event| {
        event.turn_id == Some(turn_id) && matches!(&event.event, EventBody::TurnStats(_))
    });
    Some((turn_id, closed))
}

/// Returns the resident index of one turn's explicit root semantic.
/// 返回一个回合显式根语义在常驻流中的索引。
fn explicit_turn_start_index(
    events: &[ConversationEvent],
    turn_id: TurnId,
    open_start: Option<vulcan_session_core::v1::ids::Seq>,
) -> Option<usize> {
    events.iter().position(|event| {
        if event.turn_id != Some(turn_id) {
            return false;
        }
        match &event.event {
            EventBody::UserInput(input) => matches!(
                input.input_source,
                UserInputSource::Root | UserInputSource::QueuedScheduled
            ),
            // Match the same Goal start authority as SessionDocument::rebuild_derived_state.
            // 与 SessionDocument::rebuild_derived_state 使用同一 Goal 起点权威。
            EventBody::SystemContext(context) => {
                context.context_key == "goal_continuation"
                    || (context.context_key == "goal_state"
                        && context.operation
                            == vulcan_session_core::v1::event::SystemContextOperation::Upsert
                        && open_start == Some(event.seq))
            }
            _ => false,
        }
    })
}

/// Validates the small documents and the split-map shape shared by both load
/// paths: identity, pointer integrity, node order, split shape, canonical
/// file names.
/// 校验两个加载路径共享的小型文档与分片映射形状：身份、指针完整性、节点
/// 顺序、分片形状、规范文件名。
fn validate_small_documents(layout: &SessionV1Layout) -> Result<SmallDocuments, SessionV1Error> {
    if !layout.session_dir().exists() {
        return Err(SessionV1Error::session_missing(
            layout.session_id.clone(),
            layout.session_dir(),
        ));
    }
    if !layout.metadata_path().exists() {
        return Err(SessionV1Error::session_missing(
            layout.session_id.clone(),
            layout.metadata_path(),
        ));
    }
    let metadata = read_metadata(&layout.metadata_path())?;
    if metadata.session_id != layout.session_id {
        return Err(SessionV1Error::unsupported(
            layout.metadata_path(),
            format!(
                "metadata session_id {} does not match locator {}",
                metadata.session_id, layout.session_id
            ),
        ));
    }
    if metadata.segment_generation == 0 || metadata.active_context_generation == 0 {
        return Err(SessionV1Error::corrupt(
            ErrorCode::MetadataCorrupt,
            layout.metadata_path(),
            "segment_generation and active_context_generation must both be at least 1",
        )
        .with_context(&metadata.session_id, layout.metadata_path()));
    }
    let context_generation = metadata.active_context_generation;
    let runtime_path = layout.runtime_path(context_generation);
    let compactions_path = layout.compactions_path(context_generation);
    let runtime_document = read_runtime(&runtime_path)?;
    let compactions_document = read_compactions(&compactions_path)?;

    // Small-document identity: runtime and compactions must belong to the same
    // session as metadata (audit P0#6).
    // 小型文档身份：runtime 与 compactions 必须与 metadata 属于同一会话
    // (审计 P0#6)。
    ensure_same_session(
        &metadata.session_id,
        &runtime_document.session_id,
        layout,
        "runtime",
    )?;
    ensure_same_session(
        &metadata.session_id,
        &compactions_document.session_id,
        layout,
        "compactions",
    )?;

    // The runtime pointer must reference a real compaction node; a dangling
    // pointer is corruption, never silently trimmed.
    // runtime 指针必须指向真实压缩节点；悬空指针是损坏，绝不静默裁剪。
    if let Some(pointer) = runtime_document.current_compaction_id {
        let referenced = compactions_document
            .nodes
            .iter()
            .any(|node| node.compaction_id == pointer);
        if !referenced {
            return Err(SessionV1Error::corrupt(
                ErrorCode::RuntimeCorrupt,
                runtime_path.clone(),
                format!(
                    "current_compaction_id {} references no compaction node",
                    pointer.value()
                ),
            )
            .with_context(&layout.session_id, runtime_path.clone()));
        }
    }
    let terminal_compaction_id = compactions_document
        .nodes
        .last()
        .map(|node| node.compaction_id);
    if runtime_document.current_compaction_id != terminal_compaction_id {
        return Err(SessionV1Error::corrupt(
            ErrorCode::RuntimeCorrupt,
            runtime_path.clone(),
            format!(
                "current_compaction_id {:?} does not equal terminal node {:?}",
                runtime_document.current_compaction_id.map(|id| id.value()),
                terminal_compaction_id.map(|id| id.value())
            ),
        )
        .with_context(&layout.session_id, runtime_path.clone()));
    }

    // The persisted node list must stay strictly ordered by occurrence, ids,
    // and the retained frontier (including unpublished candidates), and a
    // retention boundary can never lie after its own occurrence.
    // 持久化节点列表必须严格按发生顺序、id 与保留前沿有序(含未发布候选)，
    // 且保留边界绝不能晚于其自身发生点。
    if !assert_node_order(&compactions_document.nodes) {
        return Err(SessionV1Error::corrupt(
            ErrorCode::CompactionCorrupt,
            compactions_path.clone(),
            "compaction nodes are out of order (occurrence or id)",
        )
        .with_context(&layout.session_id, compactions_path.clone()));
    }
    let mut previous_retained = 0u64;
    for node in &compactions_document.nodes {
        let retained = node.retained_from_seq.value();
        if retained < previous_retained || retained > node.occurred_at_seq.value() {
            return Err(SessionV1Error::corrupt(
                ErrorCode::CompactionCorrupt,
                compactions_path.clone(),
                format!(
                    "compaction node {} retained_from_seq {retained} violates the frontier (previous {previous_retained}, occurred {})",
                    node.compaction_id.value(),
                    node.occurred_at_seq.value()
                ),
            )
            .with_context(&layout.session_id, compactions_path.clone()));
        }
        previous_retained = retained;
    }

    // Split map shape: exactly one active tail, and it must be the last entry;
    // every file name must be the canonical name of its segment number.
    // 分片映射形状：恰好一个活动尾段且必须是最后一项；每个文件名必须是其
    // 分片序号的规范名称。
    let active_tail_count = metadata
        .split_list
        .iter()
        .filter(|entry| entry.end_seq.is_none())
        .count();
    if metadata.split_list.is_empty()
        || active_tail_count != 1
        || metadata
            .split_list
            .last()
            .map(|entry| entry.end_seq.is_some())
            .unwrap_or(false)
    {
        return Err(SessionV1Error::corrupt(
            ErrorCode::MetadataCorrupt,
            layout.metadata_path(),
            format!(
                "split_list must end with exactly one active tail ({} entries, {} active)",
                metadata.split_list.len(),
                active_tail_count
            ),
        )
        .with_context(&metadata.session_id, layout.metadata_path()));
    }
    for entry in &metadata.split_list {
        if !SessionV1Layout::is_segment_file_name(entry.segment_number, &entry.file_name) {
            return Err(SessionV1Error::corrupt(
                ErrorCode::MetadataCorrupt,
                layout.metadata_path(),
                format!(
                    "split_list file_name {} is not an exact banked name of segment {}",
                    entry.file_name, entry.segment_number
                ),
            )
            .with_context(&metadata.session_id, layout.metadata_path()));
        }
    }
    Ok(SmallDocuments {
        metadata,
        runtime_document,
        compactions_document,
    })
}

/// Loads and validates the small documents and split map, returning the
/// authoritative metadata used by precise window readers.
/// 加载并校验小型文档与分片映射，返回精确窗口读取器使用的权威 metadata。
pub(crate) fn load_validated_metadata(
    layout: &SessionV1Layout,
) -> Result<MetadataDocument, SessionV1Error> {
    let documents = validate_small_documents(layout)?;
    validate_split_chain(&documents.metadata, layout)?;
    Ok(documents.metadata)
}

/// Validates the split map's sealed chain from metadata alone (without parsing
/// segment content): each entry starts exactly one seq after the previous
/// sealed end, and segment numbers ascend by one.
/// 仅凭 metadata 校验分片映射的封口链(不解析分片内容)：每个条目恰好从上一
/// 封口分片结束 seq 的下一 seq 开始，且分片序号逐 1 递增。
pub(super) fn validate_split_chain(
    metadata: &MetadataDocument,
    layout: &SessionV1Layout,
) -> Result<(), SessionV1Error> {
    let first = metadata.split_list.first().expect("validated: non-empty");
    if first.next_event_local_id == 0 {
        return Err(SessionV1Error::corrupt(
            ErrorCode::MetadataCorrupt,
            layout.metadata_path(),
            "first split entry next_event_local_id must be at least 1",
        )
        .with_context(&metadata.session_id, layout.metadata_path()));
    }
    for pair in metadata.split_list.windows(2) {
        let previous = &pair[0];
        let next = &pair[1];
        let previous_end = previous.end_seq.ok_or_else(|| {
            SessionV1Error::corrupt(
                ErrorCode::MetadataCorrupt,
                layout.metadata_path(),
                format!(
                    "split entry {} before the last is not sealed",
                    previous.segment_number
                ),
            )
            .with_context(&metadata.session_id, layout.metadata_path())
        })?;
        if next.start_seq.value() != previous_end.value() + 1 {
            return Err(SessionV1Error::discontinuity(
                layout.session_dir(),
                "sealed segment chain",
                previous_end.value() + 1,
                next.start_seq.value(),
            ));
        }
        if next.segment_number != previous.segment_number + 1 {
            return Err(SessionV1Error::corrupt(
                ErrorCode::MetadataCorrupt,
                layout.metadata_path(),
                format!(
                    "segment numbers do not ascend by one: {} then {}",
                    previous.segment_number, next.segment_number
                ),
            )
            .with_context(&metadata.session_id, layout.metadata_path()));
        }
        if next.next_event_local_id < previous.next_event_local_id {
            return Err(SessionV1Error::corrupt(
                ErrorCode::MetadataCorrupt,
                layout.metadata_path(),
                format!(
                    "next_event_local_id regresses from {} to {} between segments {} and {}",
                    previous.next_event_local_id,
                    next.next_event_local_id,
                    previous.segment_number,
                    next.segment_number
                ),
            )
            .with_context(&metadata.session_id, layout.metadata_path()));
        }
    }
    Ok(())
}

/// Rejects a metadata frontier that lies ahead of the actual stream facts.
/// 拒绝超前于实际流事实的 metadata frontier。
fn verify_frontier_not_ahead(
    document: &SessionDocument,
    metadata: &MetadataDocument,
    layout: &SessionV1Layout,
) -> Result<(), SessionV1Error> {
    let (derived_local, derived_seq) = {
        let frontier = document.id_frontier();
        (frontier.next_local_id, frontier.next_seq)
    };
    if metadata.id_frontier.next_seq > derived_seq
        || metadata.id_frontier.next_local_id > derived_local
    {
        return Err(SessionV1Error::corrupt(
            ErrorCode::MetadataCorrupt,
            layout.metadata_path(),
            format!(
                "metadata.id_frontier ({}, {}) is ahead of stream facts ({derived_local}, {derived_seq})",
                metadata.id_frontier.next_local_id, metadata.id_frontier.next_seq
            ),
        ));
    }
    Ok(())
}

/// Returns the session id of one layout.
/// 返回一个布局的会话 id。
fn inner_session_id(layout: &SessionV1Layout) -> String {
    layout.session_id.clone()
}

/// Loads one split-map entry precisely and validates its boundary.
/// 精确加载一个分片映射条目并校验其边界。
pub(super) fn load_segment_entry(
    layout: &SessionV1Layout,
    metadata: &MetadataDocument,
    entry: &SegmentInfo,
    expected_start: u64,
) -> Result<(u64, Vec<ConversationEvent>), SessionV1Error> {
    if entry.start_seq.value() != expected_start {
        return Err(SessionV1Error::discontinuity(
            layout.session_dir(),
            format!("segment s{:07}", entry.segment_number),
            expected_start,
            entry.start_seq.value(),
        ));
    }
    let path = layout.segment_entry_path(metadata.segment_generation, &entry.file_name);
    if !path.exists() {
        return Err(SessionV1Error::segment_missing(&path, entry.segment_number)
            .with_context(&inner_session_id(layout), &path));
    }
    let parsed = parse_segment(&path, entry.segment_number, Some(entry.start_seq))
        .map_err(|error| error.with_context(&inner_session_id(layout), &path))?;
    let parsed_end = parsed
        .last()
        .map(|event| event.seq().value())
        .unwrap_or(entry.start_seq.value() - 1);
    match entry.end_seq {
        Some(sealed_end) => {
            if parsed_end != sealed_end.value() {
                return Err(SessionV1Error::discontinuity(
                    path,
                    format!("sealed segment s{:07}", entry.segment_number),
                    sealed_end.value(),
                    parsed_end,
                ));
            }
        }
        None => {
            // The active tail's file content is the authority for its terminal
            // position; metadata end_seq may legitimately lag behind but must
            // never point beyond the file.
            // 活动尾段的文件内容是终止位置的权威；metadata 的 end_seq 可以
            // 合法滞后，但绝不能指向文件之外。
            if parsed_end + 1 < entry.start_seq.value() {
                return Err(SessionV1Error::discontinuity(
                    path,
                    format!("active tail s{:07}", entry.segment_number),
                    entry.start_seq.value(),
                    parsed_end + 1,
                ));
            }
        }
    }
    if entry.end_seq.is_some() {
        // Sealed segments are immutable: byte_len and event_count must match
        // exactly. The active tail's split fields are a rebuildable cache; the
        // file content itself stays authoritative (plan 5.2.15).
        // 封口分片不可变：byte_len 与 event_count 必须精确匹配。活动尾段的
        // 分片字段是可重建缓存；文件内容本身保持权威(计划 5.2.15)。
        let actual_byte_len = segment_byte_len(&path, entry.segment_number)?;
        if actual_byte_len != entry.byte_len {
            return Err(SessionV1Error::corrupt(
                ErrorCode::SegmentCorrupt,
                path.clone(),
                format!(
                    "split_list byte_len {} does not match sealed file size {}",
                    entry.byte_len, actual_byte_len
                ),
            )
            .with_context(&inner_session_id(layout), &path));
        }
        let actual_event_count = parsed.len() as u64;
        if actual_event_count != entry.event_count {
            return Err(SessionV1Error::corrupt(
                ErrorCode::SegmentCorrupt,
                path.clone(),
                format!(
                    "split_list event_count {} does not match sealed file events {}",
                    entry.event_count, actual_event_count
                ),
            )
            .with_context(&inner_session_id(layout), &path));
        }
    }
    let next_start = parsed_end + 1;
    Ok((next_start, parsed))
}

/// Returns an error when one small document's session id diverges from the
/// metadata session id.
/// 当某个小型文档的会话 id 与 metadata 会话 id 不一致时返回错误。
fn ensure_same_session(
    metadata_session: &str,
    other_session: &str,
    layout: &SessionV1Layout,
    kind: &str,
) -> Result<(), SessionV1Error> {
    if metadata_session != other_session {
        return Err(SessionV1Error::corrupt(
            ErrorCode::MetadataCorrupt,
            layout.metadata_path(),
            format!(
                "{kind}.session_id {other_session} does not match metadata.session_id {metadata_session}"
            ),
        )
        .with_context(metadata_session, layout.metadata_path()));
    }
    Ok(())
}
