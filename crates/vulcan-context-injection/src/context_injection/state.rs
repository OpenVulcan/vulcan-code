//! State mutation and projection methods for context injections.
//! 上下文注入状态变更与投影方法。

use crate::render::{
    InjectionPanelCard, InjectionPanelItem, InjectionRenderOptions, InjectionRendererRegistry,
    ResolvedInjectionView, item_turn_label, render_group_block, vmm_metadata_score_value,
};
use crate::types::{
    ContextInjectionState, InjectionCompactionPolicy, InjectionInsert, InjectionKind,
    InjectionPruneReport, InjectionStateGroup, InjectionStateItem,
};
use crate::util::{normalize_content, normalize_renderer_id, normalize_tag, stable_content_hash};
use serde_json::Value;

impl ContextInjectionState {
    /// Returns whether the state contains no active injection items.
    /// 返回该状态是否不包含任何活动注入内容项。
    pub fn is_empty(&self) -> bool {
        self.groups.values().all(|group| group.items.is_empty())
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
        group.description = insert.description;
        group.kind = insert.kind;
        group.scope = insert.scope;
        group.priority = insert.priority;
        group.compaction_policy = insert.compaction_policy;

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
            let content_hash =
                stable_content_hash(dedupe_key.as_deref().unwrap_or(normalized_content.as_str()));
            let compaction_policy = item.compaction_policy.unwrap_or(insert.compaction_policy);
            let source_display = item
                .source_display
                .or_else(|| insert.source_display.clone());
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
                .entry(content_hash.clone())
                .and_modify(|existing| {
                    existing.content = normalized_content.clone();
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
        for group in self.groups.values_mut() {
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
            }
            group
                .items
                .retain(|_, item| item.remaining_turns.is_none_or(|turns| turns > 0));
        }
        self.remove_empty_groups();
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
        for group in self.groups.values_mut() {
            if group.kind != InjectionKind::Transient {
                continue;
            }
            let before_item_count = group.items.len();
            group
                .items
                .retain(|_, item| item.remaining_turns.is_none_or(|turns| turns > 0));
            removed_item_count += before_item_count.saturating_sub(group.items.len());
        }
        self.remove_empty_groups();
        InjectionPruneReport {
            removed_item_count,
            removed_group_count: before_group_count.saturating_sub(self.groups.len()),
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
        for group in self.sorted_groups() {
            let Some(block) = render_group_block(group, options) else {
                continue;
            };
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
