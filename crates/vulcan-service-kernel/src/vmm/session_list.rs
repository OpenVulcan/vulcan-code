//! Lightweight native session projection and bounded server-side filtering.
//! 轻量原生会话投影及有界服务端筛选。
use super::*;
use vulcan_service_api::{VmmNativeSessionExtractionFilter, VmmNativeSessionListSort};
use vulcan_service_api::{
    VmmNativeSessionListPage, VmmNativeSessionListQuery, VmmNativeSessionListStatus,
    VmmNativeSessionStatus,
};

/// Maps one exact internal summary to the existing public DTO, preserving nullable lifecycle.
/// 将一个准确内部摘要映射为既有公共 DTO，保留可空生命周期。
/// Takes authoritative records and live host titles; returns no turn or memory bodies.
/// 接收权威记录及实时宿主标题；返回值不含回合或记忆正文。
pub(super) fn project_session_summary(
    session: crate::native_vmm::NativeVmmMemorySessionSummary,
    titles: &BTreeMap<String, String>,
) -> VmmNativeMemorySessionSummary {
    VmmNativeMemorySessionSummary {
        status: session.status.map(|status| match status {
            vulcan_memorymesh_core::StorageSessionStatus::Active => VmmNativeSessionStatus::Active,
            vulcan_memorymesh_core::StorageSessionStatus::Archived => {
                VmmNativeSessionStatus::Archived
            }
            vulcan_memorymesh_core::StorageSessionStatus::Recycled => {
                VmmNativeSessionStatus::Recycled
            }
            vulcan_memorymesh_core::StorageSessionStatus::Forgotten => {
                VmmNativeSessionStatus::Forgotten
            }
        }),
        display_title: titles.get(&session.session_source_id).cloned(),
        session_source_id: session.session_source_id,
        turn_count: session.turn_count,
        turn_statuses: native_turn_status_counts(session.turn_statuses),
        memory_count: session.memory_count,
        profile_node_count: session.profile_node_count,
        direct_memory_count: session.direct_memory_count,
        memory_levels: native_memory_level_counts(session.memory_levels),
        first_activity_at_unix_ms: session.first_activity_at_unix_ms,
        last_activity_at_unix_ms: session.last_activity_at_unix_ms,
    }
}

/// Validates explicit query bounds before any potentially expensive storage read.
/// 在任何潜在昂贵的存储读取前验证明确查询边界。
/// Takes the exact client query and returns an actionable error for unsupported bounds.
/// 接收准确客户端查询；不支持的边界返回可操作错误。
pub(super) fn validate(query: &VmmNativeSessionListQuery) -> Result<(), String> {
    if query.vmm_project_id == 0
        || !(1..=100).contains(&query.limit)
        || query.search.chars().take(513).count() > 512
    {
        return Err(
            "会话列表参数无效：请选择项目，分页数量为 1 至 100，搜索最多 512 个字符。".to_owned(),
        );
    }
    if query
        .last_activity_from_unix_ms
        .is_some_and(|value| value < 0)
        || query
            .last_activity_before_unix_ms
            .is_some_and(|value| value < 0)
        || matches!((query.last_activity_from_unix_ms, query.last_activity_before_unix_ms), (Some(from), Some(before)) if from >= before)
    {
        return Err("最近活动时间范围无效：结束时间必须晚于开始时间。".to_owned());
    }
    Ok(())
}

/// Filters the complete owner-scoped summary set before returning one deterministic bounded page.
/// 对完整归属范围内摘要集合先筛选，再返回确定性的有界分页。
/// Takes the validated query, coherent Core version and title-enriched summaries; stale versions fail explicitly.
/// 接收已校验查询、一致 Core 版本及含标题摘要；过期版本明确失败。
pub(super) fn paginate(
    query: VmmNativeSessionListQuery,
    storage_version: u64,
    mut sessions: Vec<VmmNativeMemorySessionSummary>,
) -> Result<VmmNativeSessionListPage, String> {
    validate(&query)?;
    if query
        .expected_storage_version
        .is_some_and(|expected| expected != storage_version)
    {
        return Err("记忆内容已变更，请刷新会话列表后重新翻页。".to_owned());
    }
    // Search is display metadata only; identity binding always uses the separate source ID.
    // 搜索仅用于显示元数据；身份绑定始终使用独立来源 ID。
    let search = query.search.trim().to_lowercase();
    sessions.retain(|session| {
        let state_matches = match query.status {
            VmmNativeSessionListStatus::All => true,
            VmmNativeSessionListStatus::Active => {
                session.status == Some(VmmNativeSessionStatus::Active)
            }
            VmmNativeSessionListStatus::Archived => {
                session.status == Some(VmmNativeSessionStatus::Archived)
            }
            VmmNativeSessionListStatus::Recycled => {
                session.status == Some(VmmNativeSessionStatus::Recycled)
            }
            VmmNativeSessionListStatus::Forgotten => {
                session.status == Some(VmmNativeSessionStatus::Forgotten)
            }
            VmmNativeSessionListStatus::Unregistered => session.status.is_none(),
        };
        let extraction_matches = match query.extraction_status {
            VmmNativeSessionExtractionFilter::All => true,
            VmmNativeSessionExtractionFilter::Pending => session.turn_statuses.pending > 0,
            VmmNativeSessionExtractionFilter::Completed => session.turn_statuses.completed > 0,
            VmmNativeSessionExtractionFilter::ProcessedUnknown => {
                session.turn_statuses.processed_unknown > 0
            }
            VmmNativeSessionExtractionFilter::AnalysisBypassed => {
                session.turn_statuses.analysis_bypassed > 0
            }
            VmmNativeSessionExtractionFilter::Corrupted => session.turn_statuses.corrupted > 0,
        };
        state_matches
            && extraction_matches
            && (search.is_empty()
                || session.session_source_id.to_lowercase().contains(&search)
                || session
                    .display_title
                    .as_ref()
                    .is_some_and(|title| title.to_lowercase().contains(&search)))
            && query.last_activity_from_unix_ms.is_none_or(|from| {
                session
                    .last_activity_at_unix_ms
                    .is_some_and(|time| time >= from)
            })
            && query.last_activity_before_unix_ms.is_none_or(|before| {
                session
                    .last_activity_at_unix_ms
                    .is_some_and(|time| time < before)
            })
    });
    sessions.sort_by(|left, right| {
        let order = match query.sort {
            VmmNativeSessionListSort::LastActivityDesc => right
                .last_activity_at_unix_ms
                .cmp(&left.last_activity_at_unix_ms),
            VmmNativeSessionListSort::LastActivityAsc => match (
                left.last_activity_at_unix_ms,
                right.last_activity_at_unix_ms,
            ) {
                (Some(left), Some(right)) => left.cmp(&right),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            },
            VmmNativeSessionListSort::TitleAsc => left
                .display_title
                .as_deref()
                .unwrap_or(&left.session_source_id)
                .to_lowercase()
                .cmp(
                    &right
                        .display_title
                        .as_deref()
                        .unwrap_or(&right.session_source_id)
                        .to_lowercase(),
                ),
        };
        order.then_with(|| left.session_source_id.cmp(&right.session_source_id))
    });
    let total = sessions.len() as u64;
    Ok(VmmNativeSessionListPage {
        storage_version,
        project_id: query.vmm_project_id,
        total,
        offset: query.offset,
        limit: query.limit,
        sessions: sessions
            .into_iter()
            .skip(query.offset as usize)
            .take(usize::from(query.limit))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an explicit unfiltered bounded query for pure projection tests.
    /// 构造明确未筛选的有界查询，用于纯投影测试。
    fn query() -> VmmNativeSessionListQuery {
        VmmNativeSessionListQuery {
            vmm_project_id: 2,
            status: VmmNativeSessionListStatus::All,
            extraction_status: VmmNativeSessionExtractionFilter::All,
            sort: VmmNativeSessionListSort::LastActivityDesc,
            search: String::new(),
            last_activity_from_unix_ms: None,
            last_activity_before_unix_ms: None,
            offset: 0,
            limit: 2,
            expected_storage_version: None,
        }
    }

    /// Creates deliberately interleaved states, equal timestamps and one unregistered row.
    /// 创建刻意交错的状态、相同时间戳及一个未登记记录。
    fn sessions() -> Vec<VmmNativeMemorySessionSummary> {
        [
            ("z", Some(VmmNativeSessionStatus::Archived), Some(20)),
            ("a", Some(VmmNativeSessionStatus::Active), Some(10)),
            ("b", Some(VmmNativeSessionStatus::Archived), Some(20)),
            ("u", None, None),
        ]
        .into_iter()
        .map(|(id, status, time)| VmmNativeMemorySessionSummary {
            session_source_id: id.to_owned(),
            display_title: Some(format!("Title-{id}")),
            status,
            last_activity_at_unix_ms: time,
            ..Default::default()
        })
        .collect()
    }

    /// Checks filtering before pagination, deterministic ties, explicit null-state selection and date bounds.
    /// 检查筛选先于分页、确定性同分顺序、明确空状态选择及日期边界。
    #[test]
    fn native_session_list_filters_before_paging() {
        let page = paginate(
            VmmNativeSessionListQuery {
                status: VmmNativeSessionListStatus::Archived,
                offset: 1,
                limit: 1,
                ..query()
            },
            7,
            sessions(),
        )
        .expect("page");
        assert_eq!(page.total, 2);
        assert_eq!(page.sessions[0].session_source_id, "z");
        let dates = paginate(
            VmmNativeSessionListQuery {
                last_activity_from_unix_ms: Some(10),
                last_activity_before_unix_ms: Some(20),
                ..query()
            },
            7,
            sessions(),
        )
        .expect("dates");
        assert_eq!(dates.total, 1);
        assert_eq!(dates.sessions[0].session_source_id, "a");
        let null = paginate(
            VmmNativeSessionListQuery {
                status: VmmNativeSessionListStatus::Unregistered,
                ..query()
            },
            7,
            sessions(),
        )
        .expect("unregistered");
        assert_eq!(null.total, 1);
        assert_eq!(null.sessions[0].session_source_id, "u");
        let title = paginate(
            VmmNativeSessionListQuery {
                search: "TITLE-B".to_owned(),
                ..query()
            },
            7,
            sessions(),
        )
        .expect("title");
        assert_eq!(title.total, 1);
        assert_eq!(title.sessions[0].session_source_id, "b");
        assert!(
            paginate(
                VmmNativeSessionListQuery {
                    offset: 99,
                    ..query()
                },
                7,
                sessions()
            )
            .expect("empty page")
            .sessions
            .is_empty()
        );
    }

    /// Rejects stale pages and invalid bounds instead of silently changing the query.
    /// 拒绝过期分页及非法边界，不静默改写查询。
    #[test]
    fn native_session_list_rejects_invalid_or_stale_queries() {
        for bad in [
            VmmNativeSessionListQuery {
                limit: 0,
                ..query()
            },
            VmmNativeSessionListQuery {
                limit: 101,
                ..query()
            },
            VmmNativeSessionListQuery {
                search: "字".repeat(513),
                ..query()
            },
            VmmNativeSessionListQuery {
                last_activity_from_unix_ms: Some(20),
                last_activity_before_unix_ms: Some(20),
                ..query()
            },
            VmmNativeSessionListQuery {
                expected_storage_version: Some(6),
                ..query()
            },
        ] {
            assert!(paginate(bad, 7, sessions()).is_err());
        }
    }

    /// Enforces closed JSON fields and every lifecycle choice independently from content counts.
    /// 强制封闭 JSON 字段，并独立于内容数量验证每一种生命周期选择。
    #[test]
    fn native_session_list_preserves_closed_filters_and_lifecycle_states() {
        for (filter, status) in [
            (
                VmmNativeSessionListStatus::Active,
                Some(VmmNativeSessionStatus::Active),
            ),
            (
                VmmNativeSessionListStatus::Archived,
                Some(VmmNativeSessionStatus::Archived),
            ),
            (
                VmmNativeSessionListStatus::Recycled,
                Some(VmmNativeSessionStatus::Recycled),
            ),
            (
                VmmNativeSessionListStatus::Forgotten,
                Some(VmmNativeSessionStatus::Forgotten),
            ),
            (VmmNativeSessionListStatus::Unregistered, None),
        ] {
            let page = paginate(
                VmmNativeSessionListQuery {
                    status: filter,
                    ..query()
                },
                7,
                vec![VmmNativeMemorySessionSummary {
                    session_source_id: "zero-content".to_owned(),
                    status,
                    ..Default::default()
                }],
            )
            .expect("state page");
            assert_eq!(page.total, 1);
            assert_eq!(page.sessions[0].status, status);
        }
        let mut encoded = serde_json::to_value(query()).expect("query JSON");
        encoded["unknownOption"] = serde_json::json!(true);
        assert!(serde_json::from_value::<VmmNativeSessionListQuery>(encoded).is_err());
        let mut encoded = serde_json::to_value(query()).expect("query JSON");
        encoded["status"] = serde_json::json!("invented_state");
        assert!(serde_json::from_value::<VmmNativeSessionListQuery>(encoded).is_err());
    }

    /// Checks extraction filters against turn counters, not generated-memory counts, and verifies both alternative sorts.
    /// 根据回合计数而非生成记忆数量检查提取筛选，并验证另两种排序。
    #[test]
    fn native_session_list_filters_extraction_and_sorts_missing_activity_last() {
        for (filter, counts) in [
            (
                VmmNativeSessionExtractionFilter::Pending,
                VmmNativeTurnStatusCounts {
                    pending: 1,
                    ..Default::default()
                },
            ),
            (
                VmmNativeSessionExtractionFilter::Completed,
                VmmNativeTurnStatusCounts {
                    completed: 1,
                    ..Default::default()
                },
            ),
            (
                VmmNativeSessionExtractionFilter::ProcessedUnknown,
                VmmNativeTurnStatusCounts {
                    processed_unknown: 1,
                    ..Default::default()
                },
            ),
            (
                VmmNativeSessionExtractionFilter::AnalysisBypassed,
                VmmNativeTurnStatusCounts {
                    analysis_bypassed: 1,
                    ..Default::default()
                },
            ),
            (
                VmmNativeSessionExtractionFilter::Corrupted,
                VmmNativeTurnStatusCounts {
                    corrupted: 1,
                    ..Default::default()
                },
            ),
        ] {
            let rows = vec![
                VmmNativeMemorySessionSummary {
                    session_source_id: "match".to_owned(),
                    turn_count: 1,
                    turn_statuses: counts,
                    ..Default::default()
                },
                VmmNativeMemorySessionSummary {
                    session_source_id: "no-turn".to_owned(),
                    memory_count: 10,
                    ..Default::default()
                },
            ];
            let page = paginate(
                VmmNativeSessionListQuery {
                    extraction_status: filter,
                    ..query()
                },
                7,
                rows,
            )
            .expect("extraction");
            assert_eq!(page.total, 1);
            assert_eq!(page.sessions[0].session_source_id, "match");
            assert_eq!(page.sessions[0].memory_count, 0);
        }
        let oldest = paginate(
            VmmNativeSessionListQuery {
                sort: VmmNativeSessionListSort::LastActivityAsc,
                limit: 100,
                ..query()
            },
            7,
            sessions(),
        )
        .expect("oldest");
        assert_eq!(
            oldest
                .sessions
                .iter()
                .map(|session| session.session_source_id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "z", "u"]
        );
        let title = paginate(
            VmmNativeSessionListQuery {
                sort: VmmNativeSessionListSort::TitleAsc,
                limit: 100,
                ..query()
            },
            7,
            sessions(),
        )
        .expect("title");
        assert_eq!(
            title
                .sessions
                .iter()
                .map(|session| session.session_source_id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "u", "z"]
        );
    }
}
