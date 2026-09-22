//! Context-injection state regression tests.
//! 上下文注入状态回归测试。

use super::*;

/// Builds one transient insert for concise tests.
/// 为简洁测试构建一条临时插入操作。
fn transient_insert(description: &str, content: &str, ttl: u32) -> InjectionInsert {
    InjectionInsert::single(
        InjectionKind::Transient,
        "vmm_inject",
        description,
        content,
        Some(ttl),
    )
}

/// Builds one persistent insert for removal boundary tests.
/// 为移除边界测试构建一条长期插入操作。
fn persistent_insert(tag: &str, content: &str) -> InjectionInsert {
    InjectionInsert::single(InjectionKind::Persistent, tag, "persistent", content, None)
}

#[test]
fn same_tag_and_content_resets_ttl_without_duplicate_items() {
    let mut state = ContextInjectionState::default();
    state.apply_insert(transient_insert("old", "VMM_ID:1 memory", 2), "1");
    state.consume_turn("turn-1", "2");
    state.apply_insert(transient_insert("new", "VMM_ID:1 memory", 5), "3");

    let group = state.groups.get("vmm_inject").expect("group exists");
    assert_eq!(group.description, "new");
    assert_eq!(group.items.len(), 1);
    let item = group.items.values().next().expect("item exists");
    assert_eq!(item.remaining_turns, Some(5));
}

#[test]
fn render_keeps_content_clean_without_item_metadata() {
    let mut insert = transient_insert(
        "这是vmm记忆的说明，以下内容仅供参考，不作为事实标准",
        "VMM_ID:137 这个是某个记忆",
        3,
    );
    insert.source_display = Some(String::from("vulcan-memory-mesh"));
    let mut state = ContextInjectionState::default();
    state.apply_insert(insert, "1");

    let view = state.resolve_view();
    assert_eq!(
        view.transient_prefix,
        "<vmm_inject>\n这是vmm记忆的说明，以下内容仅供参考，不作为事实标准\n<CURRENT_TURN>\nVMM_ID:137 这个是某个记忆\n</CURRENT_TURN>\n</vmm_inject>"
    );
    assert!(!view.transient_prefix.contains("TTL"));
    assert!(!view.transient_prefix.contains("source"));
}

#[test]
fn remove_group_respects_injection_kind_boundary() {
    let mut state = ContextInjectionState::default();
    state.apply_insert(transient_insert("desc", "dynamic memory", 2), "1");
    state.apply_insert(persistent_insert("vmm_profile_delta", "profile"), "1");

    assert!(!state.remove_group(Some(InjectionKind::Persistent), "vmm_inject"));
    assert!(state.groups.contains_key("vmm_inject"));
    assert!(state.remove_group(Some(InjectionKind::Persistent), "vmm_profile_delta"));
    assert!(!state.groups.contains_key("vmm_profile_delta"));
}

#[test]
fn remove_items_by_dedupe_key_respects_injection_kind_boundary() {
    let mut transient = transient_insert("desc", "dynamic memory", 2);
    transient.items[0].dedupe_key = Some(String::from("memory:1"));
    let mut persistent = persistent_insert("profile_delta", "profile memory");
    persistent.items[0].dedupe_key = Some(String::from("memory:1"));
    let mut state = ContextInjectionState::default();
    state.apply_insert(transient, "1");
    state.apply_insert(persistent, "1");

    assert!(state.remove_items_by_dedupe_key(Some(InjectionKind::Transient), "memory:1"));

    assert!(!state.groups.contains_key("vmm_inject"));
    assert!(state.groups.contains_key("profile_delta"));
}

#[test]
fn remove_items_by_metadata_string_removes_matching_dynamic_memory() {
    let mut insert = transient_insert("desc", "dynamic memory", 2);
    insert.items[0].metadata = Some(serde_json::json!({ "memory_id": "m-1" }));
    let mut state = ContextInjectionState::default();
    state.apply_insert(insert, "1");

    assert!(state.remove_items_by_metadata_string(
        Some(InjectionKind::Transient),
        "memory_id",
        "m-1"
    ));

    assert!(state.groups.is_empty());
}

#[test]
fn consume_turn_only_decrements_once_per_turn() {
    let mut state = ContextInjectionState::default();
    state.apply_insert(transient_insert("desc", "memory", 2), "1");

    state.consume_turn("turn-1", "2");
    state.consume_turn("turn-1", "3");

    let item = state
        .groups
        .get("vmm_inject")
        .and_then(|group| group.items.values().next())
        .expect("item remains");
    assert_eq!(item.remaining_turns, Some(1));

    state.consume_turn("turn-2", "4");
    assert!(state.groups.is_empty());
}

/// Verifies TTL expiration is emitted once while the earlier body has already been retained.
/// 验证 TTL 失效只发出一次，同时此前正文仍由历史保留。
#[test]
fn ttl_expiry_emits_one_append_only_event() {
    let mut state = ContextInjectionState::default();
    state.apply_insert(transient_insert("desc", "memory", 1), "1");

    let expired = state.consume_turn_with_expiry_events("turn-1", "2");

    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].tag, "vmm_inject");
    assert_eq!(expired[0].turn_id, "turn-1");
    assert!(
        expired[0]
            .body
            .contains("Context injection expired after TTL")
    );
    assert!(state.groups.is_empty());
    assert_eq!(state.take_pending_expiry_events(), expired);
    assert!(state.take_pending_expiry_events().is_empty());
}

/// Verifies re-creating the same transient body after expiry receives a new occurrence identity.
/// 验证相同临时正文在失效后重新创建会获得新的发生点身份。
#[test]
fn transient_reappearance_allocates_a_new_occurrence_id() {
    let mut state = ContextInjectionState::default();
    state.apply_insert(transient_insert("desc", "memory", 1), "1");
    let first_occurrence = state
        .groups
        .get("vmm_inject")
        .and_then(|group| group.items.values().next())
        .map(|item| item.occurrence_id.clone())
        .expect("initial occurrence");

    let expired = state.consume_turn_with_expiry_events("turn-1", "2");
    assert_eq!(expired[0].occurrence_id, first_occurrence);
    state.apply_insert(transient_insert("desc", "memory", 1), "3");
    let second_occurrence = state
        .groups
        .get("vmm_inject")
        .and_then(|group| group.items.values().next())
        .map(|item| item.occurrence_id.clone())
        .expect("reappeared occurrence");

    assert_ne!(second_occurrence, first_occurrence);
    assert_eq!(
        expired[0].content_hash,
        state
            .groups
            .get("vmm_inject")
            .and_then(|group| group.items.values().next())
            .map(|item| item.content_hash.clone())
            .expect("reappeared body hash")
    );
}

/// Verifies a session-scoped transient insert still projects a turn-scoped host block.
/// 验证会话作用域的临时插入仍投影为回合作用域的宿主块。
///
/// V1 provider projection rejects any transient host block whose scope is not `turn`, and the expiry
/// event for the same item already declares `turn`; the projected insertion must agree with it.
/// V1 供应商投影拒绝作用域非 `turn` 的临时宿主块，而同一项的实效事件已经声明 `turn`；
/// 投影出的插入必须与其一致。
#[test]
fn session_scoped_transient_insert_projects_turn_scoped_host_block() {
    let mut state = ContextInjectionState::default();
    // The default insert scope is the session, which is how producers actually insert.
    // 默认插入作用域为会话，这是生产者的实际插入方式。
    let inserted = transient_insert("hooked", "hook-provided instruction", 1);
    assert_eq!(inserted.scope, InjectionScope::Session);
    state.apply_insert(inserted, "1");
    state.apply_insert(persistent_insert("profile_delta", "profile memory"), "1");
    let view = state.resolve_view();
    let transient_block = view
        .host_blocks
        .iter()
        .find(|block| block.transient)
        .expect("transient host block");
    assert_eq!(
        transient_block.scope,
        crate::TRANSIENT_TURN_SCOPE,
        "a session-scoped transient insert must project the turn scope"
    );
    // A persistent block keeps its declared group lifetime; only the transient block is normalized.
    // 持久块保留其声明的分组生命周期；只有临时块被归一化。
    let mut project_insert = persistent_insert("profile_project", "project body");
    project_insert.scope = InjectionScope::Project;
    state.apply_insert(project_insert, "2");
    let view = state.resolve_view();
    let project_block = view
        .host_blocks
        .iter()
        .find(|block| block.body == "project body")
        .expect("project-scoped persistent host block");
    assert_eq!(
        project_block.scope,
        String::from("project"),
        "a project-scoped persistent block must keep its declared group scope"
    );
    assert!(
        !project_block.transient,
        "project block must stay persistent"
    );
}

/// Verifies expired transient injections are removed before compaction drafts are rendered.
/// 验证已失效临时注入会在压缩候选状态渲染前被移除。
#[test]
fn prune_expired_transient_items_removes_only_expired_transient_items() {
    let mut state = ContextInjectionState::default();
    state.apply_insert(transient_insert("expired", "expired memory", 1), "1");
    state.apply_insert(transient_insert("active", "active memory", 2), "1");
    state.apply_insert(persistent_insert("profile_delta", "profile memory"), "1");
    let expired_item = state
        .groups
        .get_mut("vmm_inject")
        .and_then(|group| {
            group
                .items
                .values_mut()
                .find(|item| item.content == "expired memory")
        })
        .expect("expired item");
    expired_item.remaining_turns = Some(0);
    let report = state.prune_expired_transient_items();

    assert_eq!(report.removed_item_count, 1);
    assert_eq!(report.removed_group_count, 0);
    let transient_group = state.groups.get("vmm_inject").expect("transient group");
    assert_eq!(transient_group.items.len(), 1);
    assert!(state.groups.contains_key("profile_delta"));
}

/// Verifies compaction pruning can be evaluated without mutating the original state.
/// 验证压缩裁剪可以在不修改原始状态的情况下完成评估。
#[test]
fn clone_pruned_for_compaction_keeps_original_state_unchanged() {
    let mut state = ContextInjectionState::default();
    state.apply_insert(transient_insert("expired", "expired memory", 1), "1");
    let expired_item = state
        .groups
        .get_mut("vmm_inject")
        .and_then(|group| group.items.values_mut().next())
        .expect("expired item");
    expired_item.remaining_turns = Some(0);

    let (pruned, report) = state.clone_pruned_for_compaction();

    assert_eq!(report.removed_item_count, 1);
    assert!(state.groups.contains_key("vmm_inject"));
    assert!(pruned.groups.is_empty());
}

#[test]
fn compaction_policy_removes_only_clear_items() {
    let mut state = ContextInjectionState::default();
    let mut insert = transient_insert("desc", "keep memory", 5);
    insert.items.push(InjectionInsertItem {
        content: String::from("clear memory"),
        ttl_turns: Some(5),
        compaction_policy: Some(InjectionCompactionPolicy::Clear),
        source_display: None,
        renderer_id: None,
        metadata: None,
        dedupe_key: None,
    });
    state.apply_insert(insert, "1");

    state.apply_compaction();

    let group = state.groups.get("vmm_inject").expect("group remains");
    assert_eq!(group.items.len(), 1);
    assert_eq!(
        group.items.values().next().expect("item remains").content,
        "keep memory"
    );
}

#[test]
fn item_compaction_policy_can_override_group_clear_policy() {
    let mut state = ContextInjectionState::default();
    let mut insert = transient_insert("desc", "keep memory", 5);
    insert.compaction_policy = InjectionCompactionPolicy::Clear;
    insert.items[0].compaction_policy = Some(InjectionCompactionPolicy::Keep);
    insert.items.push(InjectionInsertItem {
        content: String::from("inherited clear memory"),
        ttl_turns: Some(5),
        compaction_policy: None,
        source_display: None,
        renderer_id: None,
        metadata: None,
        dedupe_key: None,
    });
    state.apply_insert(insert, "1");

    state.apply_compaction();

    let group = state.groups.get("vmm_inject").expect("group remains");
    assert_eq!(group.items.len(), 1);
    assert_eq!(
        group.items.values().next().expect("item remains").content,
        "keep memory"
    );
}

#[test]
fn panel_cards_show_only_effective_runtime_fields() {
    let mut insert = transient_insert("desc", "memory", 5);
    insert.source_display = Some(String::from("manual-test"));
    let mut state = ContextInjectionState::default();
    state.apply_insert(insert, "1");

    let cards = state.panel_cards();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].tag, "vmm_inject");
    assert_eq!(cards[0].description, "desc");
    assert_eq!(cards[0].items[0].content, "memory");
    assert_eq!(cards[0].items[0].ttl_label, "TTL: 5");
    assert_eq!(
        cards[0].items[0].source_display.as_deref(),
        Some("manual-test")
    );
}

/// Renders test structured items for model context.
/// 为测试结构化内容项渲染模型上下文。
fn render_test_metadata(item: &InjectionStateItem) -> String {
    serde_json::to_string(item.metadata.as_ref().expect("fixture metadata")).unwrap()
}

/// Verifies changed rendering or routing inputs allocate a new occurrence while exact refreshes keep it.
/// 验证渲染或路由输入变化分配新发生点，精确刷新则保留发生点。
#[test]
fn injection_projection_changes_allocate_new_occurrences() {
    let registry = test_registry().with_renderer(
        "metadata",
        InjectionItemRenderer {
            render_model: render_test_metadata,
            render_panel: test_render_panel,
            render_detail: test_render_detail,
        },
    );
    let options = InjectionRenderOptions {
        registry: &registry,
        current_turn_marker: None,
    };
    for case in 0..4 {
        let mut insert = persistent_insert("memory", "unchanged raw body");
        insert.items[0].dedupe_key = Some(String::from("slot"));
        insert.items[0].renderer_id = Some(String::from("metadata"));
        insert.items[0].metadata = Some(serde_json::json!({"version": 1}));
        insert.source_display = Some(String::from("source-a"));
        let mut state = ContextInjectionState::default();
        state.apply_insert(insert.clone(), "1");
        let original = state.resolve_view_with_options(&options).host_blocks[0].clone();
        match case {
            0 => insert.items[0].metadata = Some(serde_json::json!({"version": 2})),
            1 => insert.items[0].renderer_id = Some(String::from("test.memory")),
            2 => insert.source_display = Some(String::from("source-b")),
            3 => insert.scope = InjectionScope::Project,
            _ => unreachable!(),
        }
        state.apply_insert(insert.clone(), "2");
        let changed = state.resolve_view_with_options(&options).host_blocks[0].clone();
        assert_eq!(changed.source_id, original.source_id);
        assert_eq!(changed.content_hash, original.content_hash);
        assert_ne!(changed.occurrence_id, original.occurrence_id, "case {case}");
        if case < 2 {
            assert_ne!(changed.body, original.body);
        }
        state.apply_insert(insert, "3");
        let repeated = state.resolve_view_with_options(&options).host_blocks[0].clone();
        assert_eq!(repeated.occurrence_id, changed.occurrence_id);
    }
}

/// Verifies changing a group scope also versions untouched sibling items whose rendered route changes.
/// 验证改变组作用域也会更新未被直接改写但渲染路由已改变的兄弟条目版本。
#[test]
fn injection_group_scope_change_versions_every_sibling() {
    let mut insert = persistent_insert("memory", "first");
    insert.items.push(InjectionInsertItem::new("second"));
    let mut state = ContextInjectionState::default();
    state.apply_insert(insert.clone(), "1");
    let previous = state.resolve_view().host_blocks;
    insert.items.truncate(1);
    insert.scope = InjectionScope::Project;
    state.apply_insert(insert, "2");
    let next = state.resolve_view().host_blocks;
    assert_eq!(next.len(), 2);
    for old in previous {
        let current = next
            .iter()
            .find(|item| item.source_id == old.source_id)
            .unwrap();
        assert_eq!(current.scope, "project");
        assert_ne!(current.occurrence_id, old.occurrence_id);
    }
}

/// Renders test structured items for model context.
/// 为测试结构化内容项渲染模型上下文。
fn test_render_model(item: &InjectionStateItem) -> String {
    format!("model:{}", item.content)
}

/// Renders test structured items for the side panel.
/// 为测试结构化内容项渲染侧边栏内容。
fn test_render_panel(item: &InjectionStateItem) -> String {
    format!("panel:{}", item.content)
}

/// Renders test structured item details for the side panel.
/// 为测试结构化内容项渲染侧边栏详情。
fn test_render_detail(item: &InjectionStateItem) -> String {
    format!("detail:{}", item.content)
}

/// Builds the renderer registry used by structured item tests.
/// 构建结构化内容项测试使用的渲染器注册表。
fn test_registry() -> InjectionRendererRegistry {
    InjectionRendererRegistry::default().with_renderer(
        "test.memory",
        InjectionItemRenderer {
            render_model: test_render_model,
            render_panel: test_render_panel,
            render_detail: test_render_detail,
        },
    )
}

#[test]
fn structured_renderer_controls_model_and_panel_output() {
    let mut insert = transient_insert("desc", "memory", 5);
    insert.items[0].renderer_id = Some(String::from("test.memory"));
    insert.items[0].metadata = Some(serde_json::json!({ "id": 1 }));
    let mut state = ContextInjectionState::default();
    state.apply_insert_for_turn(insert, "1", Some("turn-a"));
    let registry = test_registry();
    let options = InjectionRenderOptions {
        registry: &registry,
        current_turn_marker: Some("turn-a"),
    };

    let view = state.resolve_view_with_options(&options);
    let cards = state.panel_cards_with_registry(&registry);

    assert!(view.transient_prefix.contains("<CURRENT_TURN>"));
    assert!(view.transient_prefix.contains("model:memory"));
    assert_eq!(cards[0].items[0].content, "panel:memory");
    assert_eq!(cards[0].items[0].detail, "detail:memory");
    assert_eq!(cards[0].items[0].turn_label.as_deref(), Some("当前轮次"));
}

#[test]
fn transient_render_splits_current_and_retained_turn_items() {
    let mut state = ContextInjectionState::default();
    state.apply_insert_for_turn(transient_insert("desc", "current", 5), "1", Some("turn-b"));
    state.apply_insert_for_turn(transient_insert("desc", "retained", 5), "2", Some("turn-a"));
    let options = InjectionRenderOptions {
        registry: &InjectionRendererRegistry::default(),
        current_turn_marker: Some("turn-b"),
    };

    let view = state.resolve_view_with_options(&options);

    assert!(view.transient_prefix.contains("<CURRENT_TURN>\ncurrent"));
    assert!(
        view.transient_prefix
            .contains("<RETAINED_FROM_EARLIER_TURNS>\nretained")
    );
}

#[test]
fn structured_dedupe_key_resets_ttl_without_duplicate_items() {
    let mut insert = transient_insert("old", "first text", 2);
    insert.items[0].dedupe_key = Some(String::from("memory:1"));
    let mut state = ContextInjectionState::default();
    state.apply_insert_for_turn(insert, "1", Some("turn-a"));
    // Capture the producer's original identity before changing the content of the same slot.
    // 在修改同一槽位正文前捕获生产者的原始身份。
    let original = state.resolve_view().host_blocks[0].clone();
    state.consume_turn("turn-a", "2");
    let mut refreshed = transient_insert("new", "updated text", 5);
    refreshed.items[0].dedupe_key = Some(String::from("memory:1"));

    state.apply_insert_for_turn(refreshed, "3", Some("turn-b"));

    let group = state.groups.get("vmm_inject").expect("group exists");
    assert_eq!(group.items.len(), 1);
    let item = group.items.values().next().expect("item exists");
    assert_eq!(item.content, "updated text");
    assert_eq!(item.remaining_turns, Some(5));
    assert_eq!(item.created_turn_marker.as_deref(), Some("turn-b"));
    assert_eq!(item.source_id, original.source_id);
    assert_ne!(item.occurrence_id, original.occurrence_id);
    assert_ne!(item.content_hash, original.content_hash);
    // Expiry must reference the replacement occurrence and its new body hash.
    // 失效必须引用替换后的发生点和新正文哈希。
    let updated = state.resolve_view().host_blocks[0].clone();
    for turn in 0..4 {
        assert!(
            state
                .consume_turn_with_expiry_events(&format!("next-{turn}"), "4")
                .is_empty()
        );
    }
    let expired = state.consume_turn_with_expiry_events("last", "5");
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].occurrence_id, updated.occurrence_id);
    assert_eq!(expired[0].content_hash, updated.content_hash);
}

#[test]
fn panel_label_treats_consumed_transient_items_as_retained_without_current_marker() {
    let mut state = ContextInjectionState::default();
    state.apply_insert_for_turn(transient_insert("desc", "memory", 5), "1", Some("turn-a"));
    state.consume_turn("turn-a", "2");

    let cards = state.panel_cards();

    assert_eq!(cards[0].items[0].turn_label.as_deref(), Some("历史保留"));
}
