//! host runtime adapters for shared context-injection state and hook helpers.
//! 共享上下文注入状态与 hook helper 的宿主运行时适配器。

use sha2::{Digest, Sha256};
use vulcan_context_injection::{
    ContextInjectionState, InjectionExpiryEvent, InjectionHelper, InjectionInsert, InjectionKind,
    InjectionPanelCard, InjectionRenderOptions, InjectionRendererRegistry,
};
use vulcan_core::types::{
    ProviderContextAnchor as CoreProviderContextAnchor,
    ProviderContextExpiredEvent as CoreProviderContextExpiredEvent,
    ProviderContextHostBlock as CoreProviderContextHostBlock,
    ProviderContextInjection as CoreProviderContextInjection,
    ProviderContextMutationKind as CoreProviderContextMutationKind,
};
use vulcan_vmm::{VmmRuntimeFeaturePolicy, VmmSessionMemoryState};

/// Marker prefix used by the built-in manual test hook inside user prompts.
/// 用户提示词中内置手动测试 hook 使用的标记前缀。
const MANUAL_TEST_INJECTION_PREFIX: &str = "@inject ";
/// Converts shared context-injection state into the core overlay for one logical turn.
/// 将共享上下文注入状态转换为某个逻辑回合的 core 覆盖层。
pub fn core_provider_context_injection_from_state_for_turn(
    state: &ContextInjectionState,
    vmm_state: &VmmSessionMemoryState,
    current_turn_marker: Option<&str>,
    vmm_runtime_ready: bool,
) -> Option<CoreProviderContextInjection> {
    core_provider_context_injection_from_state_for_turn_with_pending_expiry_events(
        state,
        vmm_state,
        current_turn_marker,
        vmm_runtime_ready,
        &[],
    )
}

/// Drains typed TTL expiration markers and projects them into one non-V1 runtime overlay.
/// 取出强类型 TTL 失效标记，并将其投影到一次非 V1 运行时覆盖层。
pub fn core_provider_context_injection_from_state_for_turn_draining_expiry_events(
    state: &mut ContextInjectionState,
    vmm_state: &VmmSessionMemoryState,
    current_turn_marker: Option<&str>,
    vmm_runtime_ready: bool,
) -> Option<CoreProviderContextInjection> {
    let expired_events = state.take_pending_expiry_events();
    core_provider_context_injection_from_state_for_turn_with_pending_expiry_events(
        state,
        vmm_state,
        current_turn_marker,
        vmm_runtime_ready,
        &expired_events,
    )
}

/// Projects shared state with a caller-owned set of append-only expiration markers.
/// 使用调用方持有的追加式失效标记集合投影共享状态。
fn core_provider_context_injection_from_state_for_turn_with_pending_expiry_events(
    state: &ContextInjectionState,
    vmm_state: &VmmSessionMemoryState,
    current_turn_marker: Option<&str>,
    vmm_runtime_ready: bool,
    expired_events: &[InjectionExpiryEvent],
) -> Option<CoreProviderContextInjection> {
    let registry = context_injection_renderer_registry();
    let options = InjectionRenderOptions {
        registry: &registry,
        current_turn_marker,
    };
    let mut view = state.resolve_view_with_options(&options);
    // feature_policy independently gates state consumption and provider anchor rendering.
    // feature_policy 分别控制状态消费与 Provider 锚点渲染。
    let feature_policy = VmmRuntimeFeaturePolicy::from_env();
    let vmm_domain_active = feature_policy.session_state_enabled && vmm_runtime_ready;
    let anchor_rendering_active = vmm_domain_active
        && feature_policy.stable_anchors_enabled
        && vmm_state.session_memory_status.allows_runtime_memory()
        && vmm_state.stable_anchor_rendering_enabled;
    // After compaction, a request-local hidden block carries live memory until the next real user can own an anchor.
    // 压缩后，由请求局部隐藏块承载活动记忆，直到下一个真实用户消息可以拥有锚点。
    let pending_baseline = if anchor_rendering_active {
        vmm_state.pending_active_baseline_prefix()
    } else {
        None
    };
    // The fingerprint includes exact baseline bytes without exposing those bytes in diagnostic identities.
    // 指纹覆盖准确基线字节，但不在诊断身份中暴露正文。
    if let Some(prefix) = &pending_baseline {
        // Pending baselines can shrink within one epoch as leases expire or are revoked.
        // 待绑定基线会因租约到期或撤销而在同一纪元内缩减。
        // Version exact rendered bytes; unrelated state revisions must not duplicate the same visible event.
        // 对准确渲染字节进行版本化；无关状态修订不能重复生成相同可见事件。
        let baseline_hash = hex::encode(Sha256::digest(prefix.as_bytes()));
        view.fingerprint = format!(
            "{}|vmm-pending:{}:{}",
            view.fingerprint, vmm_state.cache_epoch.epoch_id, baseline_hash
        );
        view.persistent_blocks.push(prefix.clone());
        view.host_blocks
            .push(vulcan_context_injection::ResolvedInjectionHostBlock {
                source_id: format!("vmm:pending-baseline:{}", vmm_state.cache_epoch.epoch_id),
                occurrence_id: format!(
                    "vmm:pending-baseline:{}:{baseline_hash}",
                    vmm_state.cache_epoch.epoch_id
                ),
                source: String::from("vmm"),
                tag: String::from("vmm_pending_baseline"),
                content_hash: baseline_hash,
                turn_id: None,
                transient: false,
                scope: String::from("session"),
                body: prefix.clone(),
            });
    }
    let anchored_prefixes = if anchor_rendering_active {
        vmm_state
            .cache_epoch
            .anchors
            .iter()
            .map(|anchor| CoreProviderContextAnchor {
                anchor_id: anchor.anchor_id.clone(),
                target_message_id: anchor.target_message_id.clone(),
                prefix: anchor.prefix.clone(),
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut host_blocks = view
        .host_blocks
        .iter()
        .map(|block| CoreProviderContextHostBlock {
            source_id: block.source_id.clone(),
            occurrence_id: block.occurrence_id.clone(),
            source: block.source.clone(),
            tag: Some(block.tag.clone()),
            content_hash: Some(block.content_hash.clone()),
            turn_id: block.turn_id.clone(),
            transient: block.transient,
            target: String::from("model"),
            task_id: None,
            scope: block.scope.clone(),
            body: block.body.clone(),
        })
        .collect::<Vec<_>>();
    host_blocks.extend(
        anchored_prefixes
            .iter()
            .map(|anchor| CoreProviderContextHostBlock {
                source_id: format!("vmm:anchor:{}", anchor.anchor_id),
                occurrence_id: format!("vmm:anchor:{}", anchor.anchor_id),
                source: String::from("vmm"),
                tag: None,
                content_hash: Some(hex::encode(Sha256::digest(anchor.prefix.as_bytes()))),
                turn_id: None,
                transient: false,
                target: anchor.target_message_id.clone(),
                task_id: None,
                scope: String::from("message"),
                body: anchor.prefix.clone(),
            }),
    );
    let mutation_kind = if pending_baseline.is_some() || !view.transient_prefix.trim().is_empty() {
        CoreProviderContextMutationKind::HistoricalRewrite
    } else if anchored_prefixes.is_empty() {
        CoreProviderContextMutationKind::None
    } else {
        CoreProviderContextMutationKind::AppendOnly
    };
    let fingerprint = provider_context_injection_fingerprint(
        &view.fingerprint,
        if vmm_domain_active {
            vmm_state.cache_epoch.epoch_id.as_str()
        } else {
            ""
        },
        &anchored_prefixes,
    );
    let expired_events = expired_events
        .iter()
        .map(|event| CoreProviderContextExpiredEvent {
            event_id: event.event_id.clone(),
            source_id: event.source_id.clone(),
            occurrence_id: event.occurrence_id.clone(),
            tag: event.tag.clone(),
            content_hash: event.content_hash.clone(),
            source: String::from("context_injection"),
            target: String::from("model"),
            scope: String::from("turn"),
            turn_id: event.turn_id.clone(),
            body: event.body.clone(),
        })
        .collect::<Vec<_>>();
    let fingerprint = if expired_events.is_empty() {
        fingerprint
    } else {
        let event_ids = expired_events
            .iter()
            .map(|event| event.event_id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        format!("{fingerprint}|expired:{event_ids}")
    };
    // Only persistent generic layers need fingerprint comparison across provider cursors.
    // 只有持久化通用层需要跨 Provider 游标比较改写指纹。
    let rewrite_fingerprint =
        (!view.persistent_blocks.is_empty()).then(|| view.fingerprint.clone());
    // cache_pressure contains body-free aggregate capacity diagnostics for the active VMM epoch.
    // cache_pressure 包含当前 VMM 纪元不含正文的聚合容量诊断。
    let cache_pressure =
        if vmm_domain_active && vmm_state.session_memory_status.allows_runtime_memory() {
            vmm_state.cache_pressure()
        } else {
            vulcan_vmm::VmmCachePressure::default()
        };
    let injection = CoreProviderContextInjection {
        persistent_blocks: view.persistent_blocks,
        transient_prefix: view.transient_prefix,
        anchored_prefixes,
        mutation_kind,
        cache_epoch_id: (vmm_domain_active
            && (!vmm_state.cache_epoch.anchors.is_empty() || pending_baseline.is_some()))
        .then(|| vmm_state.cache_epoch.epoch_id.clone()),
        anchor_revision: (vmm_domain_active
            && (!vmm_state.cache_epoch.anchors.is_empty() || pending_baseline.is_some()))
        .then_some(vmm_state.cache_epoch.anchor_revision),
        rewrite_fingerprint,
        active_memory_body_bytes: cache_pressure.active_body_bytes,
        active_memory_estimated_tokens: cache_pressure.active_body_estimated_tokens,
        reclaimable_anchor_bytes: cache_pressure.reclaimable_anchor_bytes,
        reclaimable_anchor_estimated_tokens: cache_pressure.reclaimable_anchor_estimated_tokens,
        fingerprint,
        expired_events,
        host_blocks,
    };
    (!injection.is_empty()).then_some(injection)
}

/// Builds a stable diagnostic fingerprint from generic state and immutable VMM anchor identities.
/// 根据通用状态与不可变 VMM 锚点身份构建稳定诊断指纹。
///
/// # Parameters
/// 参数
/// `generic_fingerprint` is the shared context-injection view fingerprint.
/// `generic_fingerprint` 是共享上下文注入视图指纹。
/// `cache_epoch_id` is the active VMM cache epoch identity.
/// `cache_epoch_id` 是当前 VMM 缓存纪元身份。
/// `anchors` are the exact provider anchors included in this overlay.
/// `anchors` 是该覆盖层包含的精确 Provider 锚点。
///
/// # Returns
/// 返回值
/// A stable text fingerprint that does not change on score-only lease refreshes.
/// 返回不会因仅评分变化的租约刷新而改变的稳定文本指纹。
fn provider_context_injection_fingerprint(
    generic_fingerprint: &str,
    cache_epoch_id: &str,
    anchors: &[CoreProviderContextAnchor],
) -> String {
    if anchors.is_empty() {
        return generic_fingerprint.to_string();
    }
    let anchor_ids = anchors
        .iter()
        .map(|anchor| anchor.anchor_id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    if generic_fingerprint.trim().is_empty() {
        format!("vmm:{cache_epoch_id}:{anchor_ids}")
    } else {
        format!("{generic_fingerprint}|vmm:{cache_epoch_id}:{anchor_ids}")
    }
}

/// Returns simplified side-panel cards filtered by injection kind when requested.
/// 按需根据注入类型过滤并返回简洁侧面板卡片。
pub fn context_injection_panel_cards_for_kind(
    state: &ContextInjectionState,
    kind: Option<InjectionKind>,
) -> Vec<InjectionPanelCard> {
    let registry = context_injection_renderer_registry();
    let mut state = state.clone();
    if let Some(kind) = kind {
        state.groups.retain(|_, group| group.kind == kind);
    }
    state.panel_cards_with_registry(&registry)
}

/// Builds the renderer registry used by host runtime context-injection views.
/// 构建宿主运行时上下文注入视图使用的渲染器注册表。
fn context_injection_renderer_registry() -> InjectionRendererRegistry {
    InjectionRendererRegistry::default().with_renderer(
        vulcan_vmm::adapter::VMM_PRECHECK_MEMORY_RENDERER_ID,
        vulcan_vmm::adapter::vmm_precheck_memory_renderer(),
    )
}

/// Runs the built-in manual test hook and returns collected injections plus the cleaned user prompt.
/// 运行内置手动测试 hook，并返回已收集注入以及清理后的用户提示词。
pub fn collect_builtin_manual_injection_ops_and_sanitized_prompt(
    prompt: &str,
) -> (Vec<InjectionInsert>, String) {
    let mut helper = InjectionHelper::new();
    let mut sanitized_lines = Vec::new();
    for line in prompt.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix(MANUAL_TEST_INJECTION_PREFIX) else {
            sanitized_lines.push(line);
            continue;
        };
        let Some((ttl_text, content)) = rest.split_once(' ') else {
            sanitized_lines.push(line);
            continue;
        };
        let Ok(ttl_turns) = ttl_text.trim().parse::<u32>() else {
            sanitized_lines.push(line);
            continue;
        };
        let content = content.trim();
        if content.is_empty() || ttl_turns == 0 {
            sanitized_lines.push(line);
            continue;
        }
        helper.inject(
            InjectionKind::Transient,
            "manual_inject",
            "手动测试注入内容，仅用于验证当前会话的上下文注入链路。",
            content,
            Some(ttl_turns),
        );
    }
    (
        helper.finish(),
        sanitized_lines.join("\n").trim().to_string(),
    )
}

#[cfg(test)]
mod tests {
    //! Tests for host context-injection runtime adapters.
    //! 宿主上下文注入运行时适配器测试。

    use super::*;
    use vulcan_vmm::{ContextItem, VmmRecallBatch};

    /// Builds one persistent injection operation for adapter tests.
    /// 为适配器测试构建一条持久注入操作。
    fn persistent_insert(tag: &str, content: &str) -> InjectionInsert {
        InjectionInsert::single(InjectionKind::Persistent, tag, "test", content, None)
    }

    /// Verifies panel cards retain their explicit persistent and transient kind filtering.
    /// 验证面板卡片保留明确的持久与临时类型过滤。
    #[test]
    fn panel_cards_filter_by_injection_kind() {
        let mut state = ContextInjectionState::default();
        state.apply_inserts(
            vec![
                persistent_insert("persistent_memory", "stable memory"),
                InjectionInsert::single(
                    InjectionKind::Transient,
                    "vmm_inject",
                    "memory",
                    "ttl memory",
                    Some(2),
                ),
            ],
            "1",
        );

        let dynamic =
            context_injection_panel_cards_for_kind(&state, Some(InjectionKind::Transient));
        let static_cards =
            context_injection_panel_cards_for_kind(&state, Some(InjectionKind::Persistent));

        assert_eq!(dynamic.len(), 1);
        assert_eq!(dynamic[0].tag, "vmm_inject");
        assert_eq!(static_cards.len(), 1);
        assert_eq!(static_cards[0].tag, "persistent_memory");
    }

    /// Verifies cancelled injection groups no longer render into provider overlays.
    /// 验证已取消的注入分组不会继续渲染进 Provider 覆盖层。
    #[test]
    fn provider_overlay_excludes_cancelled_injection_groups() {
        let mut state = ContextInjectionState::default();
        state.apply_inserts(
            vec![
                persistent_insert("persistent_memory", "stable memory"),
                InjectionInsert::single(
                    InjectionKind::Transient,
                    "turn_memory",
                    "memory",
                    "dynamic memory",
                    Some(2),
                ),
            ],
            "1",
        );

        assert!(state.remove_group(Some(InjectionKind::Persistent), "persistent_memory"));
        assert!(state.remove_group(Some(InjectionKind::Transient), "turn_memory"));

        let overlay = core_provider_context_injection_from_state_for_turn(
            &state,
            &VmmSessionMemoryState::default(),
            Some("turn-a"),
            false,
        );

        assert!(overlay.is_none());
    }

    /// Verifies a non-V1 projection drains one typed TTL marker into the Core overlay.
    /// 验证非 V1 投影会将一条强类型 TTL 标记取出并放入 Core 覆盖层。
    #[test]
    fn provider_overlay_drains_ttl_expiry_event() {
        let mut state = ContextInjectionState::default();
        state.apply_insert(
            InjectionInsert::single(
                InjectionKind::Transient,
                "turn_memory",
                "memory",
                "dynamic memory",
                Some(1),
            ),
            "1",
        );
        state.consume_turn("turn-1", "2");

        let overlay = core_provider_context_injection_from_state_for_turn_draining_expiry_events(
            &mut state,
            &VmmSessionMemoryState::default(),
            Some("turn-2"),
            false,
        )
        .expect("expiry marker should keep the overlay present");

        assert_eq!(overlay.expired_events.len(), 1);
        assert_eq!(overlay.expired_events[0].turn_id, "turn-1");
        assert!(state.pending_expiry_events.is_empty());
    }

    /// Verifies a disabled stable-anchor epoch never leaks retained VMM bodies into provider overlays.
    /// 验证禁用稳定锚点的纪元绝不会把保留的 VMM 正文泄漏进 Provider 覆盖层。
    #[test]
    fn provider_overlay_omits_vmm_anchors_when_epoch_rendering_is_disabled() {
        let mut vmm_state = VmmSessionMemoryState::default();
        vmm_state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &VmmRecallBatch {
                    ttl_turns: 3,
                    injection_tag: String::from("VMM_MEMORY"),
                    injection_description: String::from("Relevant memories."),
                    items: vec![ContextItem {
                        text: String::from("retained-but-not-rendered"),
                        memory_id: 7,
                        ..ContextItem::default()
                    }],
                },
                "1",
            )
            .expect("initial recall should succeed");
        vmm_state
            .synchronize_stable_anchor_rendering(false, "2")
            .expect("anchor rendering should disable through an epoch reset");

        let overlay = core_provider_context_injection_from_state_for_turn(
            &ContextInjectionState::default(),
            &vmm_state,
            Some("turn-a"),
            true,
        );

        assert!(overlay.is_none());
        assert!(vmm_state.active_leases.contains_key("memory:7"));
    }

    /// Verifies TTL expiry changes diagnostics but never changes provider anchors or the cache fingerprint.
    /// 验证 TTL 到期只改变诊断指标，绝不会改变 Provider 锚点或缓存指纹。
    #[test]
    fn provider_overlay_remains_cache_stable_when_vmm_lease_expires() {
        let mut vmm_state = VmmSessionMemoryState::default();
        vmm_state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &VmmRecallBatch {
                    ttl_turns: 1,
                    injection_tag: String::from("VMM_MEMORY"),
                    injection_description: String::from("Relevant memories."),
                    items: vec![ContextItem {
                        text: String::from("cache-stable-memory"),
                        memory_id: 7,
                        ..ContextItem::default()
                    }],
                },
                "1",
            )
            .expect("initial recall should succeed");
        let overlay_before_expiry = core_provider_context_injection_from_state_for_turn(
            &ContextInjectionState::default(),
            &vmm_state,
            Some("turn-1"),
            true,
        )
        .expect("active VMM anchor should render");
        vmm_state
            .register_active_turn("turn-1", "1")
            .expect("logical turn should register");
        vmm_state
            .consume_turn("turn-1", "2")
            .expect("terminal turn should retain the expired resident");

        let overlay_after_expiry = core_provider_context_injection_from_state_for_turn(
            &ContextInjectionState::default(),
            &vmm_state,
            Some("turn-2"),
            true,
        )
        .expect("expired resident anchor should remain in the cache epoch");

        assert_eq!(
            overlay_after_expiry.anchored_prefixes,
            overlay_before_expiry.anchored_prefixes
        );
        assert_eq!(
            overlay_after_expiry.fingerprint,
            overlay_before_expiry.fingerprint
        );
        assert_eq!(
            overlay_after_expiry.cache_epoch_id,
            overlay_before_expiry.cache_epoch_id
        );
        assert_eq!(
            overlay_after_expiry.anchor_revision,
            overlay_before_expiry.anchor_revision
        );
        assert_eq!(
            overlay_after_expiry.mutation_kind,
            overlay_before_expiry.mutation_kind
        );
        assert!(overlay_before_expiry.active_memory_body_bytes > 0);
        assert_eq!(overlay_after_expiry.active_memory_body_bytes, 0);
        assert!(overlay_after_expiry.reclaimable_anchor_bytes > 0);
    }

    /// Keeps memory visible after compaction without inventing a user, then binds it once to the next real user.
    /// 压缩后不虚构用户仍保持记忆可见，随后仅向下一真实用户绑定一次。
    /// Uses a real lease transition and verifies both request-only rendering and stop gating.
    /// 使用真实租约转换，验证请求局部渲染及停用门禁。
    #[test]
    fn compaction_pending_baseline_survives_without_canonical_user() {
        // Seed one active lease and an old anchor under the actual state transition API.
        // 通过实际状态转换 API 建立一个活动租约和旧锚点。
        let mut memory = VmmSessionMemoryState::default();
        memory
            .synchronize_stable_anchor_rendering(true, "1")
            .expect("anchors enabled");
        memory
            .apply_recall_batch(
                "session-a",
                "old-user",
                &VmmRecallBatch {
                    ttl_turns: 3,
                    injection_tag: String::from("memory"),
                    injection_description: String::from("memory"),
                    items: vec![ContextItem {
                        memory_id: 7,
                        text: String::from("live-compacted-memory"),
                        ..Default::default()
                    }],
                },
                "1",
            )
            .expect("recall");
        memory
            .begin_cache_epoch("9", None, "2")
            .expect("exact compaction epoch");
        assert!(memory.cache_epoch.anchors.is_empty());
        assert_eq!(memory.active_leases.len(), 1);
        // The canonical state remains unchanged when a provider-only baseline is rendered.
        // 渲染 Provider 专属基线时，规范状态保持不变。
        let before = memory.clone();
        let overlay = core_provider_context_injection_from_state_for_turn(
            &ContextInjectionState::default(),
            &memory,
            None,
            true,
        )
        .expect("pending memory remains visible");
        assert!(overlay.anchored_prefixes.is_empty());
        assert_eq!(overlay.cache_epoch_id.as_deref(), Some("9"));
        assert_eq!(overlay.persistent_blocks.len(), 1);
        assert!(overlay.persistent_blocks[0].contains("live-compacted-memory"));
        assert!(overlay.rewrite_fingerprint.is_some());
        assert_eq!(memory, before);
        assert!(
            core_provider_context_injection_from_state_for_turn(
                &ContextInjectionState::default(),
                &memory,
                None,
                false,
            )
            .is_none()
        );
        memory
            .bind_pending_state_to_user("actual-next-user", "3")
            .expect("real user arrives");
        // Rendering must switch from the hidden baseline to one exact anchor, never duplicate both.
        // 渲染必须从隐藏基线切换为一个准确锚点，不能同时重复两者。
        let anchored = core_provider_context_injection_from_state_for_turn(
            &ContextInjectionState::default(),
            &memory,
            Some("next-turn"),
            true,
        )
        .expect("new anchor");
        assert!(anchored.persistent_blocks.is_empty());
        assert_eq!(anchored.anchored_prefixes.len(), 1);
        assert_eq!(
            anchored.anchored_prefixes[0].target_message_id,
            "actual-next-user"
        );
        assert!(
            anchored.anchored_prefixes[0]
                .prefix
                .contains("live-compacted-memory")
        );
        assert!(memory.pending_active_baseline_prefix().is_none());
    }

    /// Verifies pending baseline revisions survive lease expiry or revocation through real host projection.
    /// 验证待绑定基线经真实宿主投影后，能正确承载租约到期或撤销产生的新版本。
    #[test]
    fn pending_baseline_lease_changes_append_without_rewriting_history() {
        use crate::agent_runtime::execution_models::ExecutionPromptPurpose;
        use crate::agent_runtime::provider_request_bridge_conversation::core_snapshot_to_session_conversation_state;
        use crate::agent_runtime::provider_runtime_context::{
            runtime_context_host_event_candidates, synchronize_runtime_host_request_context,
        };

        // Exercise both producer mutations without constructing artificial host event identities.
        // 同时覆盖两种生产者变更，不手工伪造宿主事件身份。
        for revoke in [false, true] {
            // The two leases have distinct lifetimes inside the same pending compaction epoch.
            // 两条租约在同一个待绑定压缩纪元内具有不同存活期限。
            let mut memory = VmmSessionMemoryState::default();
            memory
                .synchronize_stable_anchor_rendering(true, "1")
                .unwrap();
            for (memory_id, text, ttl_turns) in
                [(7, "departing-memory", 1), (8, "retained-memory", 4)]
            {
                memory
                    .apply_recall_batch(
                        "session-a",
                        "old-user",
                        &VmmRecallBatch {
                            ttl_turns,
                            injection_tag: String::from("memory"),
                            injection_description: String::from("memory"),
                            items: vec![ContextItem {
                                memory_id,
                                text: text.to_owned(),
                                ..Default::default()
                            }],
                        },
                        "1",
                    )
                    .unwrap();
            }
            memory.begin_cache_epoch("9", None, "2").unwrap();
            // One helper always runs the production overlay builder, including its feature policy.
            // 此辅助闭包始终运行生产覆盖层构建器，包含真实功能策略。
            let overlay_for = |state: &VmmSessionMemoryState| {
                core_provider_context_injection_from_state_for_turn(
                    &ContextInjectionState::default(),
                    state,
                    None,
                    true,
                )
                .expect("pending baseline")
            };
            let first = overlay_for(&memory);
            let mut snapshot = vulcan_core::types::ConversationSnapshot::default();
            synchronize_runtime_host_request_context(
                &mut snapshot,
                &runtime_context_host_event_candidates(&first),
                None,
                ExecutionPromptPurpose::UserTurn,
            )
            .unwrap();
            let original_prefix = serde_json::to_value(&snapshot.messages).unwrap();
            memory.register_active_turn("turn-a", "3").unwrap();
            // Unrelated durable revision changes do not create another visible baseline occurrence.
            // 无关的持久化修订变化不会生成另一次可见基线发生点。
            let unchanged = overlay_for(&memory);
            assert_eq!(
                first.host_blocks[0].occurrence_id,
                unchanged.host_blocks[0].occurrence_id
            );
            synchronize_runtime_host_request_context(
                &mut snapshot,
                &runtime_context_host_event_candidates(&unchanged),
                None,
                ExecutionPromptPurpose::UserTurn,
            )
            .unwrap();
            assert_eq!(snapshot.messages.len(), 1);
            if revoke {
                let key = memory
                    .active_leases
                    .iter()
                    .find(|(_, lease)| lease.item.memory_id == 7)
                    .map(|(key, _)| key.clone())
                    .unwrap();
                memory.revoke_lease(&key, "4").unwrap();
            } else {
                memory.consume_turn("turn-a", "4").unwrap();
            }
            memory.validate("session-a").unwrap();
            let changed = overlay_for(&memory);
            assert_eq!(
                first.host_blocks[0].source_id,
                changed.host_blocks[0].source_id
            );
            assert_ne!(first.host_blocks[0].body, changed.host_blocks[0].body);
            synchronize_runtime_host_request_context(
                &mut snapshot,
                &runtime_context_host_event_candidates(&changed),
                None,
                ExecutionPromptPurpose::UserTurn,
            )
            .expect("changed live baseline must be a new event");
            assert_eq!(snapshot.messages.len(), 2);
            assert_eq!(
                serde_json::to_value(&snapshot.messages[..1]).unwrap(),
                original_prefix
            );
            assert_ne!(
                first.host_blocks[0].occurrence_id,
                changed.host_blocks[0].occurrence_id
            );
            synchronize_runtime_host_request_context(
                &mut snapshot,
                &runtime_context_host_event_candidates(&changed),
                None,
                ExecutionPromptPurpose::UserTurn,
            )
            .unwrap();
            assert_eq!(snapshot.messages.len(), 2);
            // A new compaction consumes only the replacement baseline; the original prefix stayed immutable.
            // 新压缩只消费替代后的基线；此前的原始前缀始终保持不变。
            let prepared = vulcan_context::runtime_host::prepare_runtime_host_compaction(
                &core_snapshot_to_session_conversation_state(&snapshot).unwrap(),
            )
            .unwrap();
            let summary_input = serde_json::to_string(&prepared.messages).unwrap();
            assert!(summary_input.contains("retained-memory"));
            assert!(!summary_input.contains("departing-memory"));
        }
    }
}
