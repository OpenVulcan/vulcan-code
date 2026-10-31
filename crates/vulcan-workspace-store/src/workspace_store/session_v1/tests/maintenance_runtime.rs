//! Production tail-loader regressions for persisted manual compaction and restart recovery.
//! 已持久化手动压缩与重启恢复的生产尾段加载器回归。

use super::*;
use vulcan_session_core::v1::{collaboration::CollaborationExecutionId, document::TurnOrigin};

/// Rejects malformed maintenance start anchors instead of accepting any host-only event as a turn root.
/// 拒绝畸形维护起点锚点，不将任意仅宿主事件接受为回合根。
/// Parameters: none. Returns: none; verifies terminal, inner-owner and sequence corruption remain explicit failures.
/// 参数：无。返回值：无；验证终态、内部归属与序号损坏仍为显式失败。
#[test]
fn manual_maintenance_tail_reload_rejects_invalid_start_anchors() {
    for corruption in ["terminal", "owner", "sequence"] {
        // Corruption stays confined to disposable storage created with the current format.
        // 损坏仅限于使用当前格式创建的可丢弃存储。
        let (_root, layout) = fresh_layout(&format!("maintenance-invalid-{corruption}"));
        // Obtain the start through the real domain transition before modifying its persisted evidence.
        // 修改已持久化证据前，通过真实领域转换取得起点。
        let mut document = SessionDocument::new(&layout.session_id, "project-1", "Maintenance", 1);
        document.metadata = publish_initial(&layout, &document);
        submit_test_root(&mut document, "original-input", "Task", 2);
        document
            .fail_execution_boundary_and_close_turn("fixture_failure", None, 3)
            .unwrap();
        flush_candidate(&mut document, &layout, 3);
        // Admission captures the exact durable business source.
        // 准入捕获精确可靠业务来源。
        let source = document
            .capture_manual_context_maintenance_source()
            .unwrap();
        document
            .begin_manual_context_maintenance(
                &source,
                CollaborationExecutionId::try_new("manual-maintenance").unwrap(),
                4,
            )
            .unwrap();
        flush_candidate(&mut document, &layout, 4);
        // Alter only the typed start evidence, keeping the actual metadata owner unchanged.
        // 仅改变强类型起点证据，保持实际元数据所有者不变。
        let mut events = document.events().to_vec();
        // Find by event kind, never by a position that changes as history evolves.
        // 按事件类型定位，绝不按会随历史演进改变的位置定位。
        let start = events
            .iter_mut()
            .find_map(|event| match &mut event.event {
                EventBody::ContextMaintenance(record) => Some(record),
                _ => None,
            })
            .expect("fixture must contain the maintenance start");
        match corruption {
            "terminal" => {
                start.terminal = Some(
                    vulcan_session_core::v1::context_maintenance::ContextMaintenanceTerminal {
                        reason: TurnClosureReason::Failed,
                        compaction_id: None,
                        end_seq: Seq::try_new(start.started_seq.value() + 1).unwrap(),
                        completed_at_ms: 5,
                    },
                )
            }
            "owner" => start.turn_id = TurnId::try_new(start.turn_id.value() + 1).unwrap(),
            "sequence" => start.started_seq = Seq::try_new(start.started_seq.value() + 1).unwrap(),
            _ => unreachable!(),
        }
        // Write the corrupt fixture to the metadata-selected tail without touching another bank.
        // 将损坏夹具写入元数据选中的尾段，不触碰其他 bank。
        let tail = document.metadata.split_list.last().unwrap();
        write_file_atomic(
            &layout.segment_entry_path(document.metadata.segment_generation, &tail.file_name),
            &serialize_segment(&events).unwrap(),
            "invalid maintenance start fixture",
        )
        .unwrap();
        // Recognition cannot bypass the explicit reference checks for any malformed start.
        // 识别逻辑不能绕过任意畸形起点的显式引用检查。
        let error = load_session_tail_only(&layout)
            .err()
            .expect("invalid maintenance start must fail");
        assert_eq!(
            error.code,
            ErrorCode::ReferenceViolation,
            "corruption: {corruption}"
        );
    }
}

/// Persists a real manual maintenance owner, then reloads and recovers it through the production tail loader.
/// 持久化真实手动维护所有者，再经生产尾段加载器重载并恢复。
/// Parameters: none. Returns: none; checks first-use, rollover, zero-output API recovery, draft preservation and new input.
/// 参数：无。返回值：无；检查首次使用、分片滚动、零输出 API 恢复、草稿保留与新输入。
#[test]
fn manual_maintenance_tail_reload_recovers_and_accepts_new_input() {
    for rollover in [false, true] {
        for with_api in [false, true] {
            // Every case owns isolated storage with the current metadata schema from its first creation.
            // 每种情况都拥有隔离存储，从首次创建起即使用当前元数据模式。
            let (_root, layout) = fresh_layout(&format!("maintenance-{rollover}-{with_api}"));
            // Create actual completed business history rather than synthesizing a maintenance envelope.
            // 创建真实已结束业务历史，不合成维护信封。
            let mut document =
                SessionDocument::new(&layout.session_id, "project-1", "Maintenance", 1);
            document.metadata = publish_initial(&layout, &document);
            submit_test_root(&mut document, "original-input", "Keep the original task", 2);
            document
                .fail_execution_boundary_and_close_turn("fixture_failure", None, 3)
                .unwrap();
            document.set_draft("Unsent draft", Vec::new(), 4).unwrap();
            flush_candidate(&mut document, &layout, 4);
            // The initial history and unsent draft must survive recovery byte-for-byte as structured facts.
            // 初始历史与未发送草稿必须作为结构化事实原样保留。
            let original_events = document.events().to_vec();
            // Capture a durable source exactly as the service does before admission.
            // 与服务准入前一样捕获可靠来源。
            let source = document
                .capture_manual_context_maintenance_source()
                .unwrap();
            // The receipt supplies every ownership anchor checked after cold loading.
            // 回执提供冷加载后检查的每个归属锚点。
            let maintenance = document
                .begin_manual_context_maintenance(
                    &source,
                    CollaborationExecutionId::try_new("manual-maintenance").unwrap(),
                    5,
                )
                .unwrap()
                .into_value();
            if rollover {
                flush_with_limits(
                    &layout,
                    &mut document,
                    &FlushLimits {
                        max_lines: 1,
                        ..FlushLimits::default()
                    },
                    5,
                );
            } else {
                flush_candidate(&mut document, &layout, 5);
            }
            if with_api {
                prepare_api(
                    &mut document,
                    ApiPurposeKind::Compaction,
                    ApiRequestSnapshot::new("provider", "model"),
                    6,
                );
                flush_candidate(&mut document, &layout, 6);
            }
            // This is the tail-only path used by draft hydration and normal runtime admission.
            // 这是草稿加载与正常运行时准入使用的仅尾段路径。
            let (mut loaded, _) = load_session_tail_only(&layout).unwrap();
            assert_eq!(
                loaded.document.active_turn().unwrap().origin,
                TurnOrigin::ContextMaintenance
            );
            assert_eq!(
                loaded.document.active_turn().unwrap().turn_id,
                maintenance.turn_id
            );
            assert_eq!(
                loaded.document.active_turn().unwrap().start_seq,
                maintenance.started_seq
            );
            assert!(loaded.document.project_persisted_interruptions().unwrap() > 0);
            assert_eq!(
                loaded.document.project_persisted_interruptions().unwrap(),
                0
            );
            flush_candidate(&mut loaded.document, &layout, 10);
            // Reopening the repaired document must stay idle and preserve its actual draft.
            // 重新打开恢复后的文档必须保持空闲并保留真实草稿。
            let (mut reopened, _) = load_session_tail_only(&layout).unwrap();
            assert!(reopened.document.active_turn().is_none());
            assert!(reopened.document.active_api().is_none());
            assert_eq!(
                reopened.document.metadata.input_state.draft,
                document.metadata.input_state.draft
            );
            assert!(
                reopened
                    .document
                    .metadata
                    .context_maintenance
                    .as_ref()
                    .unwrap()
                    .terminal
                    .is_some()
            );
            // Full-history loading independently verifies that recovery never rewrites prior business facts.
            // 全历史加载独立核验恢复绝不重写先前业务事实。
            let full = load_session(&layout).unwrap();
            for original in &original_events {
                assert_eq!(
                    full.document
                        .events()
                        .iter()
                        .find(|event| event.seq == original.seq),
                    Some(original)
                );
            }
            submit_test_root(
                &mut reopened.document,
                "after-restart",
                "Resume the task",
                11,
            );
            flush_candidate(&mut reopened.document, &layout, 11);
            assert_eq!(
                load_session_tail_only(&layout)
                    .unwrap()
                    .0
                    .document
                    .active_turn()
                    .unwrap()
                    .origin,
                TurnOrigin::UserRoot
            );
        }
    }
}
