//! State mutation and projection methods for context injections.
//! 上下文注入状态变更与投影方法。

use crate::render::{
    InjectionPanelCard, InjectionPanelItem, InjectionRenderOptions, InjectionRendererRegistry,
    ResolvedInjectionHostBlock, ResolvedInjectionView, item_turn_label, render_group_block,
    render_host_item_block, vmm_metadata_score_value,
};
use crate::types::{
    ContextInjectionState, InjectionCompactionPolicy, InjectionExpiryEvent, InjectionInsert,
    InjectionKind, InjectionPruneReport, InjectionScope, InjectionStateGroup, InjectionStateItem,
};
use crate::util::{normalize_content, normalize_renderer_id, normalize_tag, stable_content_hash};
use serde_json::Value;

/// Provider-visible scope of one turn-lifetime transient injection block.
/// 单条回合生命周期临时注入块的供应商可见作用域。
///
/// A transient item is consumed once per logical turn and invalidated with the same turn identity
/// that its expiry event carries, so its host block must declare the turn scope even when the
/// producer inserted it into a session-scoped group.
/// 临时项每个逻辑回合消费一次，并与其实效事件携带同一回合身份失效，因此即使生产者把它插入
/// 会话作用域分组，其宿主块也必须声明回合作用域。
pub const TRANSIENT_TURN_SCOPE: &str = "turn";

/// Provider-visible scope of one session-lifetime persistent injection block.
/// 单条会话生命周期持久注入块的供应商可见作用域。
pub const PERSISTENT_SESSION_SCOPE: &str = "session";

/// Builds one deterministic append-only marker for an expired injection item.
/// 为已失效注入内容构建一个确定性的追加式标记。
fn expiry_event_for_item(
    tag: &str,
    item: &InjectionStateItem,
    turn_id: &str,
) -> InjectionExpiryEvent {
    let event_id = format!("context-expired:{}:{}", item.occurrence_id, turn_id);
    let body = format!(
        "Context injection expired after TTL: tag={tag}; content_hash={}; turn_id={turn_id}",
        item.content_hash
    );
    InjectionExpiryEvent {
        event_id,
        source_id: item.source_id.clone(),
        occurrence_id: item.occurrence_id.clone(),
        tag: tag.to_string(),
        content_hash: item.content_hash.clone(),
        turn_id: turn_id.to_string(),
        body,
    }
}

impl ContextInjectionState {
    /// Allocates the next stable occurrence identity for one producer source.
    /// 为一个生产者来源分配下一个稳定发生点身份。
    fn allocate_occurrence_id(&mut self, source_id: &str) -> String {
        let sequence = self
            .source_sequences
            .entry(source_id.to_string())
            .and_modify(|value| *value = value.saturating_add(1))
            .or_insert(1);
        format!("{source_id}:occurrence:{sequence}")
    }

    /// Returns whether the state contains no active injection items.
    /// 返回该状态是否不包含任何活动注入内容项。
    pub fn is_empty(&self) -> bool {
        self.groups.values().all(|group| group.items.is_empty())
            && self.pending_expiry_events.is_empty()
    }

    /// Applies hook-collected insertion operations to the state.
    /// 将 hook 收集到的插入操作应用到当前状态。
    pub fn apply_inserts(&mut self, inserts: Vec<InjectionInsert>, now: &str) {
        self.apply_inserts_for_turn(inserts, now, None);
    }

    /// Applies hook-collected insertion operations and records their creating turn marker.
    /// 应用 hook 收集到的插入操作，并记录其创建回合标记。
    pub fn apply_inserts_for_turn(
        &mut self,
        inserts: Vec<InjectionInsert>,
        now: &str,
        turn_marker: Option<&str>,
    ) {
        for insert in inserts {
            self.apply_insert_for_turn(insert, now, turn_marker);
        }
        self.remove_empty_groups();
    }

    /// Applies one hook-collected insertion operation to the state.
    /// 将一条 hook 收集到的插入操作应用到当前状态。
    pub fn apply_insert(&mut self, insert: InjectionInsert, now: &str) {
        self.apply_insert_for_turn(insert, now, None);
    }

    /// Applies one insertion operation and records the creating turn marker on transient items.
    /// 应用一条插入操作，并在临时内容项上记录创建回合标记。
    pub fn apply_insert_for_turn(
        &mut self,
        insert: InjectionInsert,
        now: &str,
        turn_marker: Option<&str>,
    ) {
        let tag = normalize_tag(&insert.tag);
        if tag.is_empty() {
            return;
        }
        for item in insert.items {
            let normalized_content = normalize_content(&item.content);
            if normalized_content.is_empty() {
                continue;
            }
            let ttl_turns = match insert.kind {
                InjectionKind::Persistent => None,
                InjectionKind::Transient => {
                    item.ttl_turns.or(insert.ttl_turns).filter(|ttl| *ttl > 0)
                }
            };
            if insert.kind == InjectionKind::Transient && ttl_turns.is_none() {
                continue;
            }
            let renderer_id = item.renderer_id.and_then(|value| {
                let normalized = normalize_renderer_id(&value);
                (!normalized.is_empty()).then_some(normalized)
            });
            let metadata = item.metadata;
            let dedupe_key = item.dedupe_key.and_then(|value| {
                let normalized = normalize_content(&value);
                (!normalized.is_empty()).then_some(normalized)
            });
            let dedupe_hash =
                stable_content_hash(dedupe_key.as_deref().unwrap_or(normalized_content.as_str()));
            let content_hash = stable_content_hash(normalized_content.as_str());
            let source_id = format!("context_injection:{tag}:{dedupe_hash}");
            let source_display = item
                .source_display
                .or_else(|| insert.source_display.clone());
            // A group route change also changes sibling envelopes, even when this write updates only one item.
            // 即使本次只更新一个条目，组路由变化也会改变其兄弟条目的信封。
            let (route_changed, sibling_sources) = match self.groups.get(&tag) {
                Some(group) if group.kind != insert.kind || group.scope != insert.scope => (
                    true,
                    group
                        .items
                        .iter()
                        .filter(|(key, _)| *key != &dedupe_hash)
                        .map(|(key, sibling)| (key.clone(), sibling.source_id.clone()))
                        .collect::<Vec<_>>(),
                ),
                _ => (false, Vec::new()),
            };
            for (key, sibling_source) in sibling_sources {
                let sibling_occurrence = self.allocate_occurrence_id(&sibling_source);
                // Allocation only mutates source counters, so every captured sibling still exists under this exclusive borrow.
                // 分配只改变来源计数器，因此在此独占借用中所有已捕获的兄弟条目仍然存在。
                self.groups
                    .get_mut(&tag)
                    .expect("captured injection group")
                    .items
                    .get_mut(&key)
                    .expect("captured injection sibling")
                    .occurrence_id = sibling_occurrence;
            }
            let existing = self
                .groups
                .get(&tag)
                .and_then(|group| group.items.get(&dedupe_hash));
            let occurrence_id = match existing {
                Some(existing)
                    if !route_changed
                        && existing.content_hash == content_hash
                        && existing.renderer_id == renderer_id
                        && existing.metadata == metadata
                        && existing.source_display == source_display =>
                {
                    existing.occurrence_id.clone()
                }
                _ => self.allocate_occurrence_id(&source_id),
            };
            let group = self
                .groups
                .entry(tag.clone())
                .or_insert_with(|| InjectionStateGroup {
                    tag: tag.clone(),
                    kind: insert.kind,
                    scope: insert.scope,
                    priority: insert.priority,
                    compaction_policy: insert.compaction_policy,
                    ..InjectionStateGroup::default()
                });
            group.description = insert.description.clone();
            group.kind = insert.kind;
            group.scope = insert.scope;
            group.priority = insert.priority;
            group.compaction_policy = insert.compaction_policy;
            let compaction_policy = item.compaction_policy.unwrap_or(insert.compaction_policy);
            let created_turn_marker = if insert.kind == InjectionKind::Transient {
                turn_marker
                    .map(str::trim)
                    .filter(|marker| !marker.is_empty())
                    .map(str::to_string)
            } else {
                None
            };
            group
                .items
                .entry(dedupe_hash)
                .and_modify(|existing| {
                    existing.content = normalized_content.clone();
                    // Publish the newly allocated occurrence together with its exact body version.
                    // 将新分配的发生点与其精确正文版本一并保存。
                    existing.content_hash = content_hash.clone();
                    existing.occurrence_id = occurrence_id.clone();
                    existing.ttl_turns = ttl_turns;
                    existing.remaining_turns = ttl_turns;
                    existing.compaction_policy = compaction_policy;
                    existing.source_display = source_display.clone();
                    existing.renderer_id = renderer_id.clone();
                    existing.metadata = metadata.clone();
                    existing.dedupe_key = dedupe_key.clone();
                    existing.created_turn_marker = created_turn_marker.clone();
                    existing.updated_at = now.to_string();
                })
                .or_insert_with(|| InjectionStateItem {
                    content: normalized_content,
                    content_hash,
                    source_id,
                    occurrence_id,
                    ttl_turns,
                    remaining_turns: ttl_turns,
                    last_consumed_turn_id: None,
                    compaction_policy,
                    source_display,
                    renderer_id,
                    metadata,
                    dedupe_key,
                    created_turn_marker,
                    created_at: now.to_string(),
                    updated_at: now.to_string(),
                });
        }
    }

    /// Consumes transient TTL for a logical turn once and deletes expired items.
    /// 为一个逻辑回合消耗一次临时 TTL，并删除已经失效的内容项。
    pub fn consume_turn(&mut self, turn_id: &str, now: &str) {
        let _ = self.consume_turn_with_expiry_events(turn_id, now);
    }

    /// Consumes one logical turn and returns the expiration markers created by that turn.
    /// 消耗一个逻辑回合，并返回该回合产生的失效标记。
    pub fn consume_turn_with_expiry_events(
        &mut self,
        turn_id: &str,
        now: &str,
    ) -> Vec<InjectionExpiryEvent> {
        let mut expired_events = Vec::new();
        for (tag, group) in self.groups.iter_mut() {
            if group.kind != InjectionKind::Transient {
                continue;
            }
            for item in group.items.values_mut() {
                if item.last_consumed_turn_id.as_deref() == Some(turn_id) {
                    continue;
                }
                item.last_consumed_turn_id = Some(turn_id.to_string());
                item.remaining_turns = item.remaining_turns.map(|turns| turns.saturating_sub(1));
                item.updated_at = now.to_string();
                if item.remaining_turns == Some(0) {
                    expired_events.push(expiry_event_for_item(tag, item, turn_id));
                }
            }
            group
                .items
                .retain(|_, item| item.remaining_turns.is_none_or(|turns| turns > 0));
        }
        self.remove_empty_groups();
        self.enqueue_expiry_events(expired_events.clone());
        expired_events
    }

    /// Takes pending expiration markers exactly once for provider request projection.
    /// 为 Provider 请求投影一次性取出待发送的失效标记。
    pub fn take_pending_expiry_events(&mut self) -> Vec<InjectionExpiryEvent> {
        std::mem::take(&mut self.pending_expiry_events)
    }

    /// Removes transient items whose remaining TTL is already exhausted and reports the pruning counters.
    /// 移除剩余 TTL 已经耗尽的临时注入内容项，并返回裁剪计数。
    ///
    /// # Returns
    /// # 返回值
    /// An `InjectionPruneReport` describing removed item and group counts.
    /// 返回描述已移除内容项与分组数量的 `InjectionPruneReport`。
    pub fn prune_expired_transient_items(&mut self) -> InjectionPruneReport {
        let before_group_count = self.groups.len();
        let mut removed_item_count = 0usize;
        let mut expired_events = Vec::new();
        for (tag, group) in self.groups.iter_mut() {
            if group.kind != InjectionKind::Transient {
                continue;
            }
            let before_item_count = group.items.len();
            for item in group.items.values() {
                if item.remaining_turns == Some(0) {
                    if let Some(turn_id) = item.last_consumed_turn_id.as_deref() {
                        expired_events.push(expiry_event_for_item(tag, item, turn_id));
                    }
                }
            }
            group
                .items
                .retain(|_, item| item.remaining_turns.is_none_or(|turns| turns > 0));
            removed_item_count += before_item_count.saturating_sub(group.items.len());
        }
        self.remove_empty_groups();
        self.enqueue_expiry_events(expired_events);
        InjectionPruneReport {
            removed_item_count,
            removed_group_count: before_group_count.saturating_sub(self.groups.len()),
        }
    }

    /// Appends expiration markers without replaying an existing event identity.
    /// 追加失效标记并按稳定事件身份阻止已有事件重放。
    fn enqueue_expiry_events(&mut self, events: Vec<InjectionExpiryEvent>) {
        for event in events {
            if self
                .pending_expiry_events
                .iter()
                .any(|pending| pending.event_id == event.event_id)
            {
                continue;
            }
            self.pending_expiry_events.push(event);
        }
    }

    /// Returns a cloned injection state after removing expired transient items from the clone only.
    /// 返回只在克隆对象中移除已失效临时注入内容项后的注入状态。
    ///
    /// # Returns
    /// # 返回值
    /// A pruned cloned state plus the pruning report, leaving the original state unchanged.
    /// 返回已裁剪的克隆状态和裁剪报告，同时保持原始状态不变。
    pub fn clone_pruned_for_compaction(&self) -> (Self, InjectionPruneReport) {
        let mut cloned = self.clone();
        let report = cloned.prune_expired_transient_items();
        (cloned, report)
    }

    /// Applies one conversation-compaction event according to each item's effective policy.
    /// 按每条内容项的最终策略应用一次会话上下文压缩事件。
    pub fn apply_compaction(&mut self) {
        for group in self.groups.values_mut() {
            group
                .items
                .retain(|_, item| item.compaction_policy != InjectionCompactionPolicy::Clear);
        }
        self.remove_empty_groups();
    }

    /// Removes one whole injection group when its kind and tag match the requested scope.
    /// 在类型与标签匹配请求范围时移除整个注入分组。
    pub fn remove_group(&mut self, kind: Option<InjectionKind>, tag: &str) -> bool {
        let tag = normalize_tag(tag);
        if tag.is_empty() {
            return false;
        }
        let should_remove = self
            .groups
            .get(&tag)
            .map(|group| kind.is_none_or(|kind| group.kind == kind))
            .unwrap_or(false);
        if should_remove {
            self.groups.remove(&tag);
        }
        should_remove
    }

    /// Removes active injection items by stable dedupe key within the requested kind.
    /// 在请求类型范围内按稳定去重键移除活动注入内容项。
    pub fn remove_items_by_dedupe_key(
        &mut self,
        kind: Option<InjectionKind>,
        dedupe_key: &str,
    ) -> bool {
        let dedupe_key = normalize_content(dedupe_key);
        if dedupe_key.is_empty() {
            return false;
        }
        let changed = self.remove_items_matching(kind, |item| {
            item.dedupe_key.as_deref() == Some(dedupe_key.as_str())
        });
        if changed {
            self.remove_empty_groups();
        }
        changed
    }

    /// Removes active injection items whose structured metadata contains one exact string field.
    /// 移除结构化元数据包含某个精确字符串字段的活动注入内容项。
    pub fn remove_items_by_metadata_string(
        &mut self,
        kind: Option<InjectionKind>,
        field: &str,
        expected: &str,
    ) -> bool {
        let field = field.trim();
        if field.is_empty() {
            return false;
        }
        let changed = self.remove_items_matching(kind, |item| {
            item.metadata
                .as_ref()
                .and_then(|metadata| metadata.get(field))
                .and_then(Value::as_str)
                == Some(expected)
        });
        if changed {
            self.remove_empty_groups();
        }
        changed
    }

    /// Removes active injection items whose structured metadata contains one exact u64 field.
    /// 移除结构化元数据包含某个精确 u64 字段的活动注入内容项。
    pub fn remove_items_by_metadata_u64(
        &mut self,
        kind: Option<InjectionKind>,
        field: &str,
        expected: u64,
    ) -> bool {
        let field = field.trim();
        if field.is_empty() || expected == 0 {
            return false;
        }
        let changed = self.remove_items_matching(kind, |item| {
            item.metadata
                .as_ref()
                .and_then(|metadata| metadata.get(field))
                .and_then(Value::as_u64)
                == Some(expected)
        });
        if changed {
            self.remove_empty_groups();
        }
        changed
    }

    /// Builds the provider-facing resolved view without mutating persisted state.
    /// 构建面向 Provider 的解析视图，同时不修改持久化状态。
    pub fn resolve_view(&self) -> ResolvedInjectionView {
        self.resolve_view_with_options(&InjectionRenderOptions::default())
    }

    /// Builds the provider-facing resolved view with custom renderers and turn grouping.
    /// 使用自定义渲染器与回合分组构建面向 Provider 的解析视图。
    pub fn resolve_view_with_options(
        &self,
        options: &InjectionRenderOptions<'_>,
    ) -> ResolvedInjectionView {
        let mut persistent_blocks = Vec::new();
        let mut transient_blocks = Vec::new();
        let mut host_blocks = Vec::new();
        for group in self.sorted_groups() {
            let Some(block) = render_group_block(group, options) else {
                continue;
            };
            for item in group.items.values() {
                let Some(item_body) = render_host_item_block(item, options) else {
                    continue;
                };
                host_blocks.push(ResolvedInjectionHostBlock {
                    source_id: item.source_id.clone(),
                    occurrence_id: item.occurrence_id.clone(),
                    source: item
                        .source_display
                        .clone()
                        .unwrap_or_else(|| format!("context-injection:{}", group.tag)),
                    tag: group.tag.clone(),
                    content_hash: item.content_hash.clone(),
                    turn_id: item
                        .created_turn_marker
                        .clone()
                        .or_else(|| item.last_consumed_turn_id.clone()),
                    transient: group.kind == InjectionKind::Transient,
                    // A transient block is turn-lifetime by construction: its expiry event is keyed by
                    // the same logical turn and consumed once per turn boundary, so it must declare the
                    // turn scope even when the producer inserted it into a session-scoped group.
                    // Persistent blocks keep the declared group lifetime unchanged.
                    // 临时块按构造即为回合生命周期：其实效事件以同一逻辑回合为键，且每回合边界消费
                    // 一次，因此即使生产者把它插入会话作用域分组也必须声明回合作用域。
                    // 持久块保留声明的分组生命周期不变。
                    scope: match group.kind {
                        InjectionKind::Transient => TRANSIENT_TURN_SCOPE.to_string(),
                        InjectionKind::Persistent => match group.scope {
                            InjectionScope::Global => "global",
                            InjectionScope::Project => "project",
                            InjectionScope::Session => PERSISTENT_SESSION_SCOPE,
                            InjectionScope::Turn => TRANSIENT_TURN_SCOPE,
                        }
                        .to_string(),
                    },
                    body: item_body,
                });
            }
            match group.kind {
                InjectionKind::Persistent => persistent_blocks.push(block),
                InjectionKind::Transient => transient_blocks.push(block),
            }
        }
        let transient_prefix = transient_blocks.join("\n\n");
        let fingerprint = stable_content_hash(
            [persistent_blocks.join("\n\n"), transient_prefix.clone()]
                .join("\n---\n")
                .as_str(),
        );
        ResolvedInjectionView {
            persistent_blocks,
            transient_prefix,
            host_blocks,
            has_transient: !transient_blocks.is_empty(),
            fingerprint,
        }
    }

    /// Builds the simple side-panel cards from currently effective items.
    /// 根据当前有效内容项构建简洁侧面板卡片。
    pub fn panel_cards(&self) -> Vec<InjectionPanelCard> {
        self.panel_cards_with_registry(&InjectionRendererRegistry::default())
    }

    /// Builds side-panel cards with renderer-specific summaries and details.
    /// 使用渲染器专属摘要与详情构建侧面板卡片。
    pub fn panel_cards_with_registry(
        &self,
        registry: &InjectionRendererRegistry,
    ) -> Vec<InjectionPanelCard> {
        self.sorted_groups()
            .into_iter()
            .filter_map(|group| {
                let items = group
                    .items
                    .values()
                    .map(|item| InjectionPanelItem {
                        content: registry.panel_content(item),
                        detail: registry.panel_detail(item),
                        dedupe_key: item.dedupe_key.clone(),
                        memory_id: item
                            .metadata
                            .as_ref()
                            .and_then(|metadata| metadata.get("memory_id"))
                            .and_then(Value::as_u64),
                        turn_id: item
                            .metadata
                            .as_ref()
                            .and_then(|metadata| metadata.get("turn_id"))
                            .and_then(Value::as_u64),
                        score_label: item
                            .metadata
                            .as_ref()
                            .and_then(|metadata| metadata.get("score"))
                            .and_then(vmm_metadata_score_value)
                            .filter(|score| score.is_finite())
                            .map(|score| format!("{score:.3}")),
                        has_dialogue: item
                            .metadata
                            .as_ref()
                            .and_then(|metadata| metadata.get("has_dialogue"))
                            .and_then(Value::as_bool)
                            .unwrap_or_default(),
                        created_datetime: item
                            .metadata
                            .as_ref()
                            .and_then(|metadata| metadata.get("created_datetime"))
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        updated_at: (!item.updated_at.trim().is_empty())
                            .then(|| item.updated_at.clone()),
                        turn_label: item_turn_label(item, None),
                        ttl_label: item
                            .remaining_turns
                            .map(|ttl| format!("TTL: {ttl}"))
                            .unwrap_or_else(|| String::from("TTL: ∞")),
                        source_display: item.source_display.clone(),
                    })
                    .collect::<Vec<_>>();
                (!items.is_empty()).then(|| InjectionPanelCard {
                    tag: group.tag.clone(),
                    description: group.description.clone(),
                    items,
                })
            })
            .collect()
    }

    /// Returns sorted group references in stable render order.
    /// 按稳定渲染顺序返回分组引用集合。
    fn sorted_groups(&self) -> Vec<&InjectionStateGroup> {
        let mut groups = self.groups.values().collect::<Vec<_>>();
        groups.sort_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then_with(|| left.kind.cmp(&right.kind))
                .then_with(|| left.tag.cmp(&right.tag))
        });
        groups
    }

    /// Removes active items selected by one predicate while respecting the requested kind boundary.
    /// 按一个谓词移除活动内容项，同时遵守请求的注入类型边界。
    fn remove_items_matching(
        &mut self,
        kind: Option<InjectionKind>,
        mut predicate: impl FnMut(&InjectionStateItem) -> bool,
    ) -> bool {
        let mut changed = false;
        for group in self.groups.values_mut() {
            if kind.is_some_and(|kind| group.kind != kind) {
                continue;
            }
            let before = group.items.len();
            group.items.retain(|_, item| !predicate(item));
            changed |= group.items.len() != before;
        }
        changed
    }

    /// Removes tag groups that no longer contain active items.
    /// 删除不再包含活动内容项的标签分组。
    fn remove_empty_groups(&mut self) {
        self.groups.retain(|_, group| !group.items.is_empty());
    }
}
