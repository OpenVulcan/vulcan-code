//! Precise turn and Provider event-window readers for V1 sessions.
//! V1 会话的精确回合与 Provider 事件窗口读取器。
//!
//! Every read is resolved only through `metadata.split_list`. The readers never
//! scan a directory, never repair data, and never cut a logical turn or the
//! requested Provider seq range.
//! 每次读取都只通过 `metadata.split_list` 定位。读取器绝不扫描目录、绝不修复
//! 数据，也绝不截断逻辑回合或请求的 Provider seq 范围。

use vulcan_session_core::v1::config::SessionConfig;
use vulcan_session_core::v1::event::{ConversationEvent, EventBody};
use vulcan_session_core::v1::ids::{Seq, TurnId};

use super::errors::{ErrorCode, SessionV1Error, SessionV1ErrorDetails};
use super::layout::SessionV1Layout;
use super::loader::{load_segment_entry, load_validated_metadata};

/// Default number of complete logical turns returned by initial hydration.
/// 首次 hydration 返回的完整逻辑回合默认数量。
/// Metrics of one precise event-window read.
/// 一次精确事件窗口读取的指标。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventWindowReadReport {
    /// Segment files actually opened.
    /// 实际打开的分片文件数。
    pub opened_segment_files: u64,
    /// Events deserialized before exact range selection.
    /// 精确范围筛选前反序列化的事件数。
    pub parsed_event_count: u64,
    /// Events returned to the caller.
    /// 返回给调用方的事件数。
    pub returned_event_count: u64,
}

/// Persisted ownership frontier captured from the same validated metadata used
/// by one physical window read.
/// 从一次物理窗口读取使用的同一份已校验 metadata 捕获的持久化归属前沿。
#[derive(Debug, Clone, PartialEq)]
pub struct EventWindowState {
    /// Current persisted segment generation.
    /// 当前持久化分片代次。
    pub segment_generation: u64,
    /// Exact document revision durably published with this metadata view.
    /// 与该 metadata 视图一同可靠发布的精确文档修订号。
    pub document_revision: u64,
    /// Next seq that would be allocated by the persisted stream.
    /// 持久化流将分配的下一 seq。
    pub next_seq: u64,
    /// Exact start seq of the persisted open Turn, when any.
    /// 持久化开放 Turn 的精确起始 seq（如有）。
    pub open_turn_start_seq: Option<Seq>,
    /// Session request and compaction configuration captured from the same metadata.
    /// 从同一份 metadata 捕获的会话请求与压缩配置。
    pub config: SessionConfig,
}

/// One complete, seq-contiguous turn window.
/// 一个完整且 seq 连续的回合窗口。
#[derive(Debug, Clone, PartialEq)]
pub struct TurnEventWindow {
    /// Ownership frontier read atomically with this window's metadata view.
    /// 与该窗口 metadata 视图一同读取的归属前沿。
    pub state: EventWindowState,
    /// Events in exact ascending seq order.
    /// 按精确 seq 升序排列的事件。
    pub events: Vec<ConversationEvent>,
    /// Exclusive cursor for loading the immediately older window.
    /// 加载紧邻更早窗口的排他游标。
    pub older_before_seq: Option<Seq>,
    /// Read metrics for performance and no-full-scan assertions.
    /// 用于性能与禁止全扫描断言的读取指标。
    pub report: EventWindowReadReport,
}

/// One exact Provider projection source range.
/// 一个精确 Provider 投影来源范围。
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderEventRange {
    /// Ownership frontier read atomically with this range's metadata view.
    /// 与该范围 metadata 视图一同读取的归属前沿。
    pub state: EventWindowState,
    /// Events from `from_seq` through `through_seq`, both inclusive.
    /// 从 `from_seq` 到 `through_seq`（两端均包含）的事件。
    pub events: Vec<ConversationEvent>,
    /// Read metrics for performance and no-full-scan assertions.
    /// 用于性能与禁止全扫描断言的读取指标。
    pub report: EventWindowReadReport,
}

/// Loads the latest complete turn window, including an entire open tail turn.
/// 加载最新完整回合窗口，并包含整个开放尾部回合。
pub fn load_recent_turn_window(
    layout: &SessionV1Layout,
    turn_limit: usize,
) -> Result<TurnEventWindow, SessionV1Error> {
    load_turn_window(layout, None, turn_limit)
}

/// Loads complete turns strictly before one cursor returned by a newer window.
/// 加载由较新窗口返回的游标之前的完整回合。
pub fn load_turn_window_before(
    layout: &SessionV1Layout,
    before_seq: Seq,
    turn_limit: usize,
) -> Result<TurnEventWindow, SessionV1Error> {
    load_turn_window(layout, Some(before_seq), turn_limit)
}

/// Loads one exact inclusive seq range for Provider projection.
/// 为 Provider 投影加载一个精确的闭区间 seq 范围。
pub fn load_provider_event_range(
    layout: &SessionV1Layout,
    from_seq: Seq,
    through_seq: Seq,
) -> Result<ProviderEventRange, SessionV1Error> {
    let metadata = load_validated_metadata(layout)?;
    let from = from_seq.value();
    let through = through_seq.value();
    let next_seq = metadata.id_frontier.next_seq;
    if from > through || through >= next_seq {
        return Err(request_range_error(
            layout,
            "provider event range",
            format!("1 <= from <= through < {next_seq}"),
            format!("from={from}, through={through}"),
        ));
    }
    let mut events = Vec::new();
    let mut opened_segment_files = 0u64;
    let mut parsed_event_count = 0u64;
    for entry in metadata.split_list.iter().filter(|entry| {
        let entry_end = entry.end_seq.map_or(next_seq.saturating_sub(1), Seq::value);
        entry.start_seq.value() <= through && entry_end >= from
    }) {
        let (_, parsed) = load_segment_entry(layout, &metadata, entry, entry.start_seq.value())?;
        opened_segment_files += 1;
        parsed_event_count += parsed.len() as u64;
        events.extend(
            parsed
                .into_iter()
                .filter(|event| (from..=through).contains(&event.seq().value())),
        );
    }
    validate_exact_range(layout, &events, from, through)?;
    Ok(ProviderEventRange {
        state: event_window_state(&metadata),
        report: EventWindowReadReport {
            opened_segment_files,
            parsed_event_count,
            returned_event_count: events.len() as u64,
        },
        events,
    })
}

/// Loads one latest or historical turn window through the shared cursor rules.
/// 通过统一游标规则加载一个最新或历史回合窗口。
fn load_turn_window(
    layout: &SessionV1Layout,
    before_seq: Option<Seq>,
    turn_limit: usize,
) -> Result<TurnEventWindow, SessionV1Error> {
    if turn_limit == 0 {
        return Err(request_range_error(
            layout,
            "turn window limit",
            "turn_limit >= 1",
            "turn_limit=0",
        ));
    }
    let metadata = load_validated_metadata(layout)?;
    let state = event_window_state(&metadata);
    let next_seq = metadata.id_frontier.next_seq;
    let upper_exclusive = before_seq.map_or(next_seq, Seq::value);
    if upper_exclusive == 0 || upper_exclusive > next_seq {
        return Err(request_range_error(
            layout,
            "turn window cursor",
            format!("1 <= before_seq <= {next_seq}"),
            format!("before_seq={upper_exclusive}"),
        ));
    }
    if upper_exclusive == 1 {
        return Ok(TurnEventWindow {
            state,
            events: Vec::new(),
            older_before_seq: None,
            report: EventWindowReadReport {
                opened_segment_files: 0,
                parsed_event_count: 0,
                returned_event_count: 0,
            },
        });
    }
    let target_seq = upper_exclusive - 1;
    let mut entry_index = metadata
        .split_list
        .iter()
        .position(|entry| {
            let end = entry.end_seq.map_or(next_seq.saturating_sub(1), Seq::value);
            entry.start_seq.value() <= target_seq && target_seq <= end
        })
        .ok_or_else(|| {
            request_range_error(
                layout,
                "turn window cursor",
                "cursor covered by split_list",
                format!("before_seq={upper_exclusive}"),
            )
        })?;
    let mut accumulated = Vec::new();
    let mut opened_segment_files = 0u64;
    let mut parsed_event_count = 0u64;
    loop {
        let entry = &metadata.split_list[entry_index];
        let (_, parsed) = load_segment_entry(layout, &metadata, entry, entry.start_seq.value())?;
        opened_segment_files += 1;
        parsed_event_count += parsed.len() as u64;
        let mut prefix: Vec<ConversationEvent> = parsed
            .into_iter()
            .filter(|event| event.seq().value() < upper_exclusive)
            .collect();
        prefix.append(&mut accumulated);
        accumulated = prefix;
        let at_stream_start = entry_index == 0;
        if let Some(start_index) =
            selected_turn_window_start(&accumulated, turn_limit, at_stream_start)
        {
            if before_seq.is_some() {
                validate_historical_cursor(layout, &accumulated, upper_exclusive)?;
            }
            let events = accumulated.split_off(start_index);
            validate_contiguous_events(layout, &events)?;
            let older_before_seq = events
                .first()
                .filter(|event| event.seq().value() > 1)
                .map(ConversationEvent::seq);
            return Ok(TurnEventWindow {
                state,
                report: EventWindowReadReport {
                    opened_segment_files,
                    parsed_event_count,
                    returned_event_count: events.len() as u64,
                },
                events,
                older_before_seq,
            });
        }
        if at_stream_start {
            return Err(request_range_error(
                layout,
                "turn window",
                "a complete turn boundary",
                "no complete boundary found",
            ));
        }
        entry_index -= 1;
    }
}

/// Projects one already validated metadata document into its service-visible
/// event-window frontier.
/// 把一份已经校验的 metadata 文档投影为服务可见事件窗口前沿。
fn event_window_state(
    metadata: &vulcan_session_core::v1::config::MetadataDocument,
) -> EventWindowState {
    EventWindowState {
        segment_generation: metadata.segment_generation,
        document_revision: metadata.document_revision,
        next_seq: metadata.id_frontier.next_seq,
        open_turn_start_seq: metadata.open_turn_start_seq,
        config: metadata.config.clone(),
    }
}

/// Selects the first event of the requested complete-turn window.
/// 选择所请求完整回合窗口的首个事件。
fn selected_turn_window_start(
    events: &[ConversationEvent],
    turn_limit: usize,
    at_stream_start: bool,
) -> Option<usize> {
    let mut turn_order: Vec<TurnId> = Vec::new();
    for turn_id in events.iter().filter_map(|event| event.turn_id) {
        if turn_order.last().copied() != Some(turn_id) && !turn_order.contains(&turn_id) {
            turn_order.push(turn_id);
        }
    }
    if turn_order.is_empty() {
        return at_stream_start.then_some(0);
    }
    let latest_turn_is_open = turn_order.last().is_some_and(|latest_turn| {
        !events.iter().any(|event| {
            event.turn_id == Some(*latest_turn) && matches!(&event.event, EventBody::TurnStats(_))
        })
    });
    let selected_limit = turn_limit.saturating_add(usize::from(latest_turn_is_open));
    if turn_order.len() < selected_limit && !at_stream_start {
        return None;
    }
    let selected_count = turn_order.len().min(selected_limit);
    let earliest_turn = turn_order[turn_order.len() - selected_count];
    let first_turn_index = events
        .iter()
        .position(|event| event.turn_id == Some(earliest_turn))?;
    if first_turn_index == 0 {
        return at_stream_start.then_some(0);
    }
    if let Some(previous_stats_index) = events[..first_turn_index]
        .iter()
        .rposition(|event| matches!(&event.event, EventBody::TurnStats(_)))
    {
        return Some(previous_stats_index + 1);
    }
    at_stream_start.then_some(0)
}

/// Validates that a historical cursor falls exactly after terminal turn stats.
/// 校验历史游标恰好位于终态回合统计之后。
fn validate_historical_cursor(
    layout: &SessionV1Layout,
    events: &[ConversationEvent],
    upper_exclusive: u64,
) -> Result<(), SessionV1Error> {
    let previous = events
        .iter()
        .find(|event| event.seq().value() + 1 == upper_exclusive);
    if previous.is_some_and(|event| matches!(&event.event, EventBody::TurnStats(_))) {
        return Ok(());
    }
    Err(request_range_error(
        layout,
        "turn window cursor",
        "cursor immediately after turn_stats",
        format!("before_seq={upper_exclusive}"),
    ))
}

/// Validates one exact inclusive seq range.
/// 校验一个精确闭区间 seq 范围。
fn validate_exact_range(
    layout: &SessionV1Layout,
    events: &[ConversationEvent],
    from: u64,
    through: u64,
) -> Result<(), SessionV1Error> {
    let expected_count = through - from + 1;
    if events.len() as u64 != expected_count {
        return Err(SessionV1Error::discontinuity(
            layout.session_dir(),
            "provider event count",
            expected_count,
            events.len(),
        )
        .with_context(&layout.session_id, layout.session_dir()));
    }
    validate_contiguous_events(layout, events)?;
    if events.first().map(ConversationEvent::seq) != Seq::try_new(from).ok()
        || events.last().map(ConversationEvent::seq) != Seq::try_new(through).ok()
    {
        return Err(SessionV1Error::discontinuity(
            layout.session_dir(),
            "provider event endpoints",
            format!("{from}..={through}"),
            format!(
                "{:?}..={:?}",
                events.first().map(|event| event.seq().value()),
                events.last().map(|event| event.seq().value())
            ),
        )
        .with_context(&layout.session_id, layout.session_dir()));
    }
    Ok(())
}

/// Validates ascending, gap-free seq order in one returned window.
/// 校验一个返回窗口中的 seq 升序且无空洞。
fn validate_contiguous_events(
    layout: &SessionV1Layout,
    events: &[ConversationEvent],
) -> Result<(), SessionV1Error> {
    for pair in events.windows(2) {
        let expected = pair[0].seq().value() + 1;
        let actual = pair[1].seq().value();
        if actual != expected {
            return Err(SessionV1Error::discontinuity(
                layout.session_dir(),
                "event window",
                expected,
                actual,
            )
            .with_context(&layout.session_id, layout.session_dir()));
        }
    }
    Ok(())
}

/// Builds one structured invalid-window request error.
/// 构建一条结构化无效窗口请求错误。
fn request_range_error(
    layout: &SessionV1Layout,
    context: &str,
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> SessionV1Error {
    SessionV1Error::from_details(
        ErrorCode::ReferenceViolation,
        SessionV1ErrorDetails {
            session_id: Some(layout.session_id.clone()),
            path: Some(layout.metadata_path()),
            context: Some(context.to_string()),
            line: None,
            byte_offset: None,
            expected: Some(expected.into()),
            actual: Some(actual.into()),
            detail: "event-window request does not match the published split map".to_string(),
        },
    )
}
