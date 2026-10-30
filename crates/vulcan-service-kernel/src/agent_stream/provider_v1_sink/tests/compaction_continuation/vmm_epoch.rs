//! Old persisted positional anchors must be reconciled before using the new compaction projection.
//! 使用新的压缩投影前必须对账旧的持久化位置锚点。

use super::*;
use crate::vmm_session_controller::SessionVmmControllerRegistry;
use vulcan_agent_runtime::agent_runtime::context_injection_runtime::core_provider_context_injection_from_state_for_turn;
use vulcan_session_core::v1::projection::project_context_generation_items_with_sources;
use vulcan_vmm::{
    ContextItem, VmmContextEpoch, VmmMemoryReadSource, VmmMemorySource, VmmRecallBatch,
};

/// Restarts real compacted and uncompacted VMM sidecars across the projection revision.
/// 跨投影修订重启真实的已压缩及未压缩 VMM 侧车。
/// Takes no arguments; verifies failed-write rollback, exact rebinding or pending baseline, and real wire encoding.
/// 无参数，验证写入失败回滚、精确重绑或待绑定基线以及真实线路编码。
#[tokio::test]
async fn projection_upgrade_reconciles_persisted_vmm_anchors_before_dispatch() {
    for profile in [
        ANTHROPIC_MESSAGES_PROFILE_ID,
        ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
        OPENAI_CHAT_PROFILE_ID,
        OPENAI_RESPONSES_PROFILE_ID,
        OPENAI_CODEX_RESPONSES_PROFILE_ID,
        GEMINI_GENERATE_CONTENT_PROFILE_ID,
        GOOGLE_CODE_ASSIST_PROFILE_ID,
        GEMINI_INTERACTIONS_PROFILE_ID,
        COHERE_CHAT_PROFILE_ID,
    ] {
        for (compacted, bind_user) in [(false, false), (false, true), (true, false), (true, true)] {
            // Own actual session and VMM files without touching the user's running application.
            // 持有实际会话及 VMM 文件，不触碰用户正在运行的应用。
            let (_directory, handle) = published_stored_open_turn();
            let service = crate::tests::test_support::test_service_state_with_parent_session(
                handle.session_id(),
            );
            service
                .session_runtime_registry
                .register(handle.clone())
                .unwrap();
            let state = Arc::new(std::sync::Mutex::new(service));
            // Exercise both a changed local-summary layout and an unchanged ordinary conversation.
            // 同时覆盖布局变化的本地摘要及布局不变的普通会话。
            if compacted {
                let summary_facts = facts(profile, ApiPurposeKind::Compaction, 10);
                let PreparedPhysicalApi::SessionV1 { core_start, .. } = prepare(
                    &handle,
                    &state,
                    summary_facts.clone(),
                    candidate(&handle, profile, None),
                )
                .await
                else {
                    panic!("summary permit");
                };
                let summary_seq = finish(
                    &handle,
                    core_start.api_call_id,
                    summary_facts.request,
                    "epoch-summary-fixture",
                    11,
                );
                handle
                    .record_compaction(
                        CompactionCommitInput::new(
                            core_start.api_call_id,
                            summary_seq,
                            Seq::try_new(1).unwrap(),
                            "epoch-summary-fixture",
                            super::super::super::memory_acceptance::host_guided_locked_baseline(),
                        ),
                        13,
                    )
                    .unwrap();
            }
            handle
                .mutate_durable(
                    SessionMutationAdmission::ExistingTurn,
                    SessionFlushReason::InputAccepted,
                    |document| {
                        document.add_pending_in_turn_input(
                            ClientInputSubmission::new(
                                ClientActionId::try_new("epoch-next-user")?,
                                "epoch-user-fixture",
                                Vec::new(),
                            ),
                            14,
                        )
                    },
                )
                .unwrap();
            // Promotion returns the authoritative user source sequence, independent of projection positions.
            // 晋升返回权威用户来源序号，与投影位置无关。
            let promoted = handle
                .mutate_durable(
                    SessionMutationAdmission::ExistingTurn,
                    SessionFlushReason::InputAccepted,
                    |document| document.promote_pending_in_turn_inputs_after_api(15),
                )
                .unwrap()
                .value;
            let input_seq = *promoted.first().expect("promoted user source");
            let snapshot = handle.capture_controller_projection_snapshot().unwrap();
            let current_target =
                provider_projection_user_message_id_for_source(&snapshot, input_seq).unwrap();
            // The production core projector reconstructs the previous layout before the new host control existed.
            // 生产 Core 投影器重建新增宿主控制出现之前的旧布局。
            let old_items = handle
                .read_full_durable(|document| {
                    project_context_generation_items_with_sources(
                        document.events(),
                        snapshot.retained_from_seq,
                        snapshot.compaction_frontier_seq,
                    )
                })
                .unwrap()
                .unwrap();
            let old_position = old_items
                .iter()
                .position(|item| {
                    item.source_from_seq == input_seq
                        && item.source_to_seq == input_seq
                        && matches!(item.item, ProjectedItem::UserInput { .. })
                })
                .expect("old canonical user source");
            let old_target = provider_projection_message_id(old_position);
            assert_eq!(old_target != current_target, compacted);
            // The old persisted format froze lineage without accounting for projection layout changes.
            // 旧持久化格式冻结沿革，却没有区分投影布局变化。
            let compaction_id = snapshot
                .current_compaction_id
                .map(|id| id.value().to_string());
            let old_epoch = match &compaction_id {
                Some(id) => format!("v1:{}:compaction:{id}", snapshot.segment_generation),
                None => format!("v1:{}:uncompacted", snapshot.segment_generation),
            };
            let context =
                VmmContextEpoch::new(snapshot.segment_generation, compaction_id.clone()).unwrap();
            let source =
                VmmMemoryReadSource::new(VmmMemorySource::native(1).unwrap(), vec![1]).unwrap();
            let registry = SessionVmmControllerRegistry::default();
            let locator = handle.storage_locator().unwrap();
            let controller = registry
                .get_or_open(locator.clone(), handle.session_id(), "16")
                .unwrap();
            controller
                .execute_command("seed-pre-upgrade-anchors", "17", |memory| {
                    memory.select_read_source(
                        &handle.session_id().to_string(),
                        Some(&source),
                        "17",
                    )?;
                    memory.synchronize_stable_anchor_rendering(true, "17")?;
                    memory.cache_epoch.epoch_id = old_epoch.clone();
                    memory.cache_epoch.covered_compaction_id = compaction_id.clone();
                    for (memory_id, target, text) in [
                        (1, old_target.as_str(), "older-memory-fixture"),
                        (2, current_target.as_str(), "newer-memory-fixture"),
                    ] {
                        memory.apply_recall_batch(
                            &handle.session_id().to_string(),
                            target,
                            &VmmRecallBatch {
                                ttl_turns: 30,
                                injection_tag: "vmm".into(),
                                injection_description: "fixture".into(),
                                queries: Vec::new(),
                                items: vec![ContextItem {
                                    memory_id,
                                    text: text.into(),
                                    ..Default::default()
                                }],
                            },
                            "17",
                        )?;
                    }
                    Ok(())
                })
                .unwrap();
            // Closing and reopening forces the production loader to retain the mixed persisted anchors.
            // 关闭并重开迫使生产加载器保留混合的持久化锚点。
            registry
                .begin_lifecycle_transition([handle.session_id()])
                .unwrap()
                .release()
                .unwrap();
            let controller = registry
                .get_or_open(locator.clone(), handle.session_id(), "18")
                .unwrap();
            let before = controller.snapshot().unwrap();
            assert_eq!(before.cache_epoch.epoch_id, old_epoch);
            assert_eq!(before.cache_epoch.anchors.len(), 2);
            // Admission must still reject the stale overlay instead of choosing a nearby message.
            // 准入仍须拒绝过期覆盖层，不得选择附近消息。
            let mut stale_candidate = candidate(&handle, profile, None);
            stale_candidate.controller_config.context_injection = Some(
                core_provider_context_injection_from_state_for_turn(
                    &Default::default(),
                    &before,
                    None,
                    true,
                )
                .unwrap(),
            );
            let mut sink = ProviderV1StreamSink::new(
                handle.clone(),
                Arc::new(ProjectResourceCatalogRegistry::default()),
            );
            assert!(matches!(
                sink.prepare_request(
                    &state,
                    facts(profile, ApiPurposeKind::MainGeneration, 19),
                    stale_candidate
                )
                .await,
                Err(ProviderV1SinkError::Runtime(
                    SessionRuntimeError::ProviderProjectionInvariant(_)
                ))
            ));
            assert!(
                handle
                    .read(|document| document.active_api().is_none())
                    .unwrap()
            );
            // Withhold only this private fixture's metadata to make the real durable write fail.
            // 仅暂移当前私有夹具的元数据，使真实持久化写入失败。
            let metadata_path = locator.session_directory().join("metadata.json");
            let withheld_metadata = metadata_path.with_extension("epoch-test-withheld");
            // Preserve the original sidecar bytes as well as its full authoritative in-memory state.
            // 同时保留原侧车字节及其完整权威内存状态。
            let memory_path = locator.session_directory().join("vmm.json");
            let memory_before_failure = std::fs::read(&memory_path).unwrap();
            std::fs::rename(&metadata_path, &withheld_metadata).unwrap();
            let failed_reconciliation = controller.reconcile_context_epoch(&context, &source, "20");
            std::fs::rename(&withheld_metadata, &metadata_path).unwrap();
            assert!(failed_reconciliation.is_err());
            assert_eq!(controller.snapshot().unwrap(), before);
            assert_eq!(std::fs::read(&memory_path).unwrap(), memory_before_failure);
            // Retry through the same durable owner, preserving live memories and their remaining TTL.
            // 通过同一可靠所有者重试，保留活动记忆及剩余 TTL。
            let reconciled = controller
                .reconcile_context_epoch(&context, &source, "20")
                .unwrap();
            assert!(reconciled.cache_epoch.anchors.is_empty());
            assert!(reconciled.cache_epoch.needs_active_baseline);
            assert_ne!(reconciled.cache_epoch.epoch_id, old_epoch);
            assert_eq!(
                reconciled.completed_turn_sequence,
                before.completed_turn_sequence
            );
            let mut old_leases = before.active_leases.clone();
            let mut current_leases = reconciled.active_leases.clone();
            for lease in old_leases.values_mut().chain(current_leases.values_mut()) {
                lease.active_anchor_id = None;
            }
            assert_eq!(current_leases, old_leases);
            assert!(reconciled.has_committed_controller_command("seed-pre-upgrade-anchors"));
            if bind_user {
                controller
                    .execute_command("bind-current-user", "21", |memory| {
                        memory.bind_pending_state_to_user(&current_target, "21")?;
                        Ok(())
                    })
                    .unwrap();
            }
            // Reload again to prove reconciliation and optional rebinding are durable and idempotent.
            // 再次加载，证明对账及可选重绑已可靠保存且幂等。
            let ready = controller.snapshot().unwrap();
            registry
                .begin_lifecycle_transition([handle.session_id()])
                .unwrap()
                .release()
                .unwrap();
            let reopened = registry
                .get_or_open(locator, handle.session_id(), "22")
                .unwrap();
            assert_eq!(
                reopened
                    .reconcile_context_epoch(&context, &source, "23")
                    .unwrap(),
                ready
            );
            assert!(
                ready
                    .cache_epoch
                    .anchors
                    .iter()
                    .all(|anchor| anchor.target_message_id == current_target)
            );
            assert_eq!(ready.cache_epoch.anchors.len(), usize::from(bind_user));
            // Both a real-user binding and a continuation without one reach the actual preparation and encoder.
            // 真实用户绑定及没有新绑定的续接均进入实际准备链和编码器。
            let mut prepared_candidate = candidate(&handle, profile, None);
            prepared_candidate.controller_config.context_injection = Some(
                core_provider_context_injection_from_state_for_turn(
                    &Default::default(),
                    &ready,
                    None,
                    true,
                )
                .unwrap(),
            );
            let PreparedPhysicalApi::SessionV1 {
                exact_request,
                core_start: main_start,
                ..
            } = prepare(
                &handle,
                &state,
                facts(profile, ApiPurposeKind::MainGeneration, 24),
                prepared_candidate,
            )
            .await
            else {
                panic!("reconciled business permit");
            };
            assert_user_tail(profile, &exact_request);
            // Each retained memory appears once regardless of whether it uses an anchor or a pending baseline.
            // 无论使用锚点还是待绑定基线，每份保留记忆都只出现一次。
            let encoded = serde_json::to_string(&exact_request).unwrap();
            for text in ["older-memory-fixture", "newer-memory-fixture"] {
                assert_eq!(encoded.matches(text).count(), 1);
            }
            // A later protocol switch reuses this same reconciled cache and cannot invalidate existing targets.
            // 后续协议切换复用同一份已对账缓存，不得使既有目标失效。
            finish(
                &handle,
                main_start.api_call_id,
                facts(profile, ApiPurposeKind::MainGeneration, 24).request,
                "memory-business-fixture",
                25,
            );
            let next_profile = if profile == ANTHROPIC_MESSAGES_PROFILE_ID {
                OPENAI_CHAT_PROFILE_ID
            } else {
                ANTHROPIC_MESSAGES_PROFILE_ID
            };
            let mut switched_candidate = candidate(&handle, next_profile, None);
            switched_candidate.controller_config.context_injection = Some(
                core_provider_context_injection_from_state_for_turn(
                    &Default::default(),
                    &ready,
                    None,
                    true,
                )
                .unwrap(),
            );
            let PreparedPhysicalApi::SessionV1 {
                exact_request: switched_request,
                ..
            } = prepare(
                &handle,
                &state,
                facts(next_profile, ApiPurposeKind::MainGeneration, 30),
                switched_candidate,
            )
            .await
            else {
                panic!("switched reconciled business permit");
            };
            assert_user_tail(next_profile, &switched_request);
            assert!(switched_request.continuation.is_none());
            assert_eq!(reopened.snapshot().unwrap(), ready);
        }
    }
}
