//! Shared context-injection primitives for hook-driven provider context overlays.
//! 面向 hook 驱动 Provider 上下文覆盖层的共享上下文注入原语。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Describes how one injection should be placed into the provider-facing request.
/// 描述一条注入应如何放置到面向 Provider 的请求中。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectionKind {
    /// Context that should be present as hidden user content until removed by configuration or state.
    /// 应作为隐藏用户内容持续存在、直到被配置或状态移除的上下文。
    Persistent,
    /// Context that should be prefixed to user turns and expire by turn-based TTL.
    /// 应前置到用户回合并按回合 TTL 失效的上下文。
    Transient,
}

impl Default for InjectionKind {
    /// Returns the default injection kind used by helper shorthand calls.
    /// 返回 helper 简写调用使用的默认注入类型。
    fn default() -> Self {
        Self::Transient
    }
}

/// Describes how one injection should react when conversation compaction happens.
/// 描述会话上下文压缩发生时一条注入应如何响应。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectionCompactionPolicy {
    /// Keep the injection after compaction because the source still considers it valid.
    /// 压缩后保留该注入，因为来源仍认为它有效。
    Keep,
    /// Remove the injection after compaction because it only applied to the pre-compaction window.
    /// 压缩后删除该注入，因为它只适用于压缩前窗口。
    Clear,
}

impl Default for InjectionCompactionPolicy {
    /// Returns the conservative default that avoids treating compaction as memory loss.
    /// 返回保守默认值，避免把压缩误判为失忆。
    fn default() -> Self {
        Self::Keep
    }
}

/// Describes the scope that owns one injection record.
/// 描述一条注入记录所属的作用域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectionScope {
    /// Global context shared by every session that opts into the source.
    /// 被所有选择该来源的会话共享的全局上下文。
    Global,
    /// Project context shared by sessions under one project.
    /// 由同一项目下会话共享的项目上下文。
    Project,
    /// Session context owned by one conversation session.
    /// 由单个会话持有的会话上下文。
    Session,
    /// Turn context bound to one logical user turn.
    /// 绑定到单个逻辑用户回合的回合上下文。
    Turn,
}

impl Default for InjectionScope {
    /// Returns the default helper scope for runtime injections.
    /// 返回运行时注入的默认 helper 作用域。
    fn default() -> Self {
        Self::Session
    }
}

/// One item submitted by a hook for insertion under a tag.
/// 由 hook 提交并插入到某个标签下的一条内容项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InjectionInsertItem {
    /// Provider-visible content rendered directly inside the tag.
    /// 直接渲染在标签内部的 Provider 可见内容。
    pub content: String,
    /// Optional TTL override for this single item.
    /// 该单条内容的可选 TTL 覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_turns: Option<u32>,
    /// Optional compaction policy override for this single item.
    /// 该单条内容的可选压缩策略覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction_policy: Option<InjectionCompactionPolicy>,
    /// Optional source label shown in diagnostics and UI only.
    /// 仅在诊断和界面中展示的可选来源标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_display: Option<String>,
    /// Optional renderer id used to turn structured metadata into model and UI text.
    /// 用于将结构化元数据转换为模型与界面文本的可选渲染器标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renderer_id: Option<String>,
    /// Optional structured metadata retained for plugin-specific rendering.
    /// 为插件专属渲染保留的可选结构化元数据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    /// Optional stable key used for deduplication before rendered text is built.
    /// 在生成渲染文本前用于去重的可选稳定键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedupe_key: Option<String>,
}

impl InjectionInsertItem {
    /// Builds one insertion item from provider-visible content.
    /// 根据 Provider 可见内容构建一条插入项。
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            ttl_turns: None,
            compaction_policy: None,
            source_display: None,
            renderer_id: None,
            metadata: None,
            dedupe_key: None,
        }
    }
}

/// One hook-submitted insertion operation collected by an injection helper.
/// 由注入 helper 收集的一条 hook 提交插入操作。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InjectionInsert {
    /// Injection placement kind applied to every submitted item unless overridden later.
    /// 应用于每条提交内容的注入放置类型。
    pub kind: InjectionKind,
    /// Outer XML-like tag name used to group rendered content.
    /// 用于分组渲染内容的外层类 XML 标签名。
    pub tag: String,
    /// Human-readable tag description rendered once before item content.
    /// 在内容项之前渲染一次的人类可读标签说明。
    #[serde(default)]
    pub description: String,
    /// Items submitted under the tag.
    /// 提交到该标签下的内容项集合。
    #[serde(default)]
    pub items: Vec<InjectionInsertItem>,
    /// Default TTL applied to transient items that do not carry their own TTL.
    /// 应用于未携带独立 TTL 的临时内容项的默认 TTL。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_turns: Option<u32>,
    /// Default compaction policy inherited by items without an override.
    /// 由未覆盖条目继承的默认压缩策略。
    #[serde(default)]
    pub compaction_policy: InjectionCompactionPolicy,
    /// Scope that owns the submitted content.
    /// 持有已提交内容的作用域。
    #[serde(default)]
    pub scope: InjectionScope,
    /// Stable ordering priority, where lower values render earlier.
    /// 稳定排序优先级，数值越小越靠前渲染。
    #[serde(default)]
    pub priority: i32,
    /// Optional source label shown in diagnostics and UI only.
    /// 仅在诊断和界面中展示的可选来源标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_display: Option<String>,
}

impl InjectionInsert {
    /// Builds one insertion operation from a single content value.
    /// 根据单条内容值构建一条插入操作。
    pub fn single(
        kind: InjectionKind,
        tag: impl Into<String>,
        description: impl Into<String>,
        content: impl Into<String>,
        ttl_turns: Option<u32>,
    ) -> Self {
        Self {
            kind,
            tag: tag.into(),
            description: description.into(),
            items: vec![InjectionInsertItem::new(content)],
            ttl_turns,
            compaction_policy: InjectionCompactionPolicy::default(),
            scope: InjectionScope::default(),
            priority: 0,
            source_display: None,
        }
    }
}

/// Helper handed to hooks so they can submit context injections without mutating runtime state directly.
/// 传递给 hook 的 helper，使其能提交上下文注入而不直接修改运行时状态。
#[derive(Debug, Clone, Default)]
pub struct InjectionHelper {
    /// Operations collected during the current hook phase.
    /// 当前 hook 阶段收集到的操作。
    ops: Vec<InjectionInsert>,
}

impl InjectionHelper {
    /// Builds an empty helper for one hook phase.
    /// 为单个 hook 阶段构建一个空 helper。
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one shorthand injection operation and returns the helper for chaining.
    /// 添加一条简写注入操作，并返回 helper 以便链式调用。
    pub fn inject(
        &mut self,
        kind: InjectionKind,
        tag: impl Into<String>,
        description: impl Into<String>,
        content: impl Into<String>,
        ttl_turns: Option<u32>,
    ) -> &mut Self {
        self.insert(InjectionInsert::single(
            kind,
            tag,
            description,
            content,
            ttl_turns,
        ))
    }

    /// Adds one fully structured insertion operation and returns the helper for chaining.
    /// 添加一条完整结构化插入操作，并返回 helper 以便链式调用。
    pub fn insert(&mut self, insert: InjectionInsert) -> &mut Self {
        self.ops.push(insert);
        self
    }

    /// Consumes the helper and returns the collected operations.
    /// 消费 helper 并返回已收集的操作集合。
    pub fn finish(self) -> Vec<InjectionInsert> {
        self.ops
    }
}

/// One persisted injection item under a tag group.
/// 某个标签分组下的一条持久化注入内容项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InjectionStateItem {
    /// Provider-visible content rendered directly inside the tag.
    /// 直接渲染在标签内部的 Provider 可见内容。
    pub content: String,
    /// Stable hash built from normalized provider-visible content.
    /// 根据规范化 Provider 可见内容构建的稳定哈希。
    pub content_hash: String,
    /// Initial TTL attached to transient items.
    /// 附着到临时内容项的初始 TTL。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_turns: Option<u32>,
    /// Remaining turn count before the item is deleted.
    /// 内容项被删除前剩余的回合数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_turns: Option<u32>,
    /// Last logical turn that consumed this item.
    /// 最近一次消耗该内容项的逻辑回合。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_consumed_turn_id: Option<String>,
    /// Compaction policy applied to this item.
    /// 应用于该内容项的压缩策略。
    #[serde(default)]
    pub compaction_policy: InjectionCompactionPolicy,
    /// Source label shown in diagnostics and UI only.
    /// 仅在诊断和界面中展示的来源标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_display: Option<String>,
    /// Optional renderer id used to turn structured metadata into model and UI text.
    /// 用于将结构化元数据转换为模型与界面文本的可选渲染器标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renderer_id: Option<String>,
    /// Optional structured metadata retained for plugin-specific rendering.
    /// 为插件专属渲染保留的可选结构化元数据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    /// Optional stable key used for deduplication before rendered text is built.
    /// 在生成渲染文本前用于去重的可选稳定键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedupe_key: Option<String>,
    /// Logical turn marker that created or most recently refreshed this item.
    /// 创建或最近刷新该内容项的逻辑回合标记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_turn_marker: Option<String>,
    /// Unix-millis text captured when the item was first created.
    /// 内容项首次创建时捕获的 Unix 毫秒文本。
    #[serde(default)]
    pub created_at: String,
    /// Unix-millis text captured when the item was last refreshed.
    /// 内容项最近一次刷新时捕获的 Unix 毫秒文本。
    #[serde(default)]
    pub updated_at: String,
}

/// One persisted tag group containing provider-visible injection items.
/// 一个包含 Provider 可见注入内容项的持久化标签分组。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InjectionStateGroup {
    /// Outer XML-like tag name used to group rendered content.
    /// 用于分组渲染内容的外层类 XML 标签名。
    pub tag: String,
    /// Human-readable tag description rendered once before item content.
    /// 在内容项之前渲染一次的人类可读标签说明。
    #[serde(default)]
    pub description: String,
    /// Injection placement kind applied to this group.
    /// 应用于该分组的注入放置类型。
    #[serde(default)]
    pub kind: InjectionKind,
    /// Scope that owns this group.
    /// 持有该分组的作用域。
    #[serde(default)]
    pub scope: InjectionScope,
    /// Stable ordering priority, where lower values render earlier.
    /// 稳定排序优先级，数值越小越靠前渲染。
    #[serde(default)]
    pub priority: i32,
    /// Default compaction policy inherited by new items.
    /// 新内容项继承的默认压缩策略。
    #[serde(default)]
    pub compaction_policy: InjectionCompactionPolicy,
    /// Persisted items keyed by normalized content hash.
    /// 按规范化内容哈希索引的持久化内容项。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub items: BTreeMap<String, InjectionStateItem>,
}

/// Session or scope level context-injection state.
/// 会话或作用域级上下文注入状态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextInjectionState {
    /// Active groups keyed by rendered tag name.
    /// 按渲染标签名称索引的活动分组。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub groups: BTreeMap<String, InjectionStateGroup>,
}

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

/// Provider-facing resolved injection view.
/// 面向 Provider 的解析后注入视图。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedInjectionView {
    /// Persistent hidden user blocks rendered as clean tag text.
    /// 渲染为干净标签文本的永久隐藏用户块。
    pub persistent_blocks: Vec<String>,
    /// Transient blocks that should prefix the target user prompt.
    /// 应前置到目标用户提示词的临时块。
    pub transient_prefix: String,
    /// Whether transient content is present in the current view.
    /// 当前视图是否存在临时内容。
    pub has_transient: bool,
    /// Stable fingerprint for diagnostics and continuation safety checks.
    /// 用于诊断与 continuation 安全检查的稳定指纹。
    pub fingerprint: String,
}

/// One simplified side-panel card for a rendered tag.
/// 一个渲染标签对应的简洁侧面板卡片。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectionPanelCard {
    /// Tag name shown as the card title.
    /// 作为卡片标题展示的标签名。
    pub tag: String,
    /// Tag description shown below the title.
    /// 展示在标题下方的标签说明。
    pub description: String,
    /// Effective items shown inside the card.
    /// 展示在卡片内部的有效内容项。
    pub items: Vec<InjectionPanelItem>,
}

/// One simplified side-panel item for active injection content.
/// 一个活动注入内容对应的简洁侧面板条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectionPanelItem {
    /// Compact item content shown directly inside the panel.
    /// 直接显示在面板中的紧凑内容项。
    pub content: String,
    /// Detailed hover text assembled by the renderer.
    /// 由渲染器组装的悬停详情文本。
    pub detail: String,
    /// Optional turn-origin label shown in the panel.
    /// 面板中展示的可选回合来源标签。
    pub turn_label: Option<String>,
    /// Human-readable TTL label shown in muted text.
    /// 以灰色文字展示的人类可读 TTL 标签。
    pub ttl_label: String,
    /// Optional source label shown in diagnostics and UI only.
    /// 仅在诊断和界面中展示的可选来源标签。
    pub source_display: Option<String>,
}

/// Options used when rendering provider-visible injection blocks.
/// 渲染 Provider 可见注入块时使用的选项。
pub struct InjectionRenderOptions<'a> {
    /// Renderer registry used for structured plugin metadata.
    /// 用于结构化插件元数据的渲染器注册表。
    pub registry: &'a InjectionRendererRegistry,
    /// Current logical turn marker used to split transient items.
    /// 用于拆分临时内容项的当前逻辑回合标记。
    pub current_turn_marker: Option<&'a str>,
}

impl Default for InjectionRenderOptions<'_> {
    /// Builds default render options with no custom renderer and no current turn marker.
    /// 构建无自定义渲染器且无当前回合标记的默认渲染选项。
    fn default() -> Self {
        Self {
            registry: &DEFAULT_RENDERER_REGISTRY,
            current_turn_marker: None,
        }
    }
}

/// Empty renderer registry used by default render options.
/// 默认渲染选项使用的空渲染器注册表。
static DEFAULT_RENDERER_REGISTRY: InjectionRendererRegistry = InjectionRendererRegistry {
    renderers: BTreeMap::new(),
};

/// Renderer callback set for one structured injection item type.
/// 单类结构化注入内容项的渲染回调集合。
#[derive(Clone, Copy)]
pub struct InjectionItemRenderer {
    /// Renders one item for provider-visible model context.
    /// 为模型可见上下文渲染单条内容项。
    pub render_model: fn(&InjectionStateItem) -> String,
    /// Renders one compact panel row.
    /// 渲染一条紧凑面板行。
    pub render_panel: fn(&InjectionStateItem) -> String,
    /// Renders one detailed panel tooltip body.
    /// 渲染一段详细面板悬停正文。
    pub render_detail: fn(&InjectionStateItem) -> String,
}

/// Registry that maps structured injection renderer ids to renderers.
/// 将结构化注入渲染器标识映射到渲染器的注册表。
#[derive(Default)]
pub struct InjectionRendererRegistry {
    /// Registered renderers keyed by renderer id.
    /// 按渲染器标识索引的已注册渲染器。
    renderers: BTreeMap<String, InjectionItemRenderer>,
}

impl InjectionRendererRegistry {
    /// Registers one renderer id and returns the registry for chaining.
    /// 注册一个渲染器标识，并返回注册表以便链式调用。
    pub fn with_renderer(mut self, id: impl Into<String>, renderer: InjectionItemRenderer) -> Self {
        self.register(id, renderer);
        self
    }

    /// Registers or replaces one renderer id.
    /// 注册或替换一个渲染器标识。
    pub fn register(&mut self, id: impl Into<String>, renderer: InjectionItemRenderer) {
        let id = normalize_renderer_id(&id.into());
        if !id.is_empty() {
            self.renderers.insert(id, renderer);
        }
    }

    /// Renders one item for provider-visible model context.
    /// 为模型可见上下文渲染一条内容项。
    pub fn model_content(&self, item: &InjectionStateItem) -> String {
        self.renderer_for(item)
            .map(|renderer| (renderer.render_model)(item))
            .filter(|content| !content.trim().is_empty())
            .unwrap_or_else(|| item.content.clone())
    }

    /// Renders one compact panel row.
    /// 渲染一条紧凑面板行。
    pub fn panel_content(&self, item: &InjectionStateItem) -> String {
        self.renderer_for(item)
            .map(|renderer| (renderer.render_panel)(item))
            .filter(|content| !content.trim().is_empty())
            .unwrap_or_else(|| item.content.clone())
    }

    /// Renders one detailed panel tooltip body.
    /// 渲染一段详细面板悬停正文。
    pub fn panel_detail(&self, item: &InjectionStateItem) -> String {
        self.renderer_for(item)
            .map(|renderer| (renderer.render_detail)(item))
            .filter(|content| !content.trim().is_empty())
            .unwrap_or_else(|| item.content.clone())
    }

    /// Returns the renderer registered for one item.
    /// 返回一条内容项注册的渲染器。
    fn renderer_for(&self, item: &InjectionStateItem) -> Option<&InjectionItemRenderer> {
        item.renderer_id
            .as_deref()
            .and_then(|id| self.renderers.get(id))
    }
}

/// Renders one group into provider-facing tag text.
/// 将一个分组渲染为面向 Provider 的标签文本。
fn render_group_block(
    group: &InjectionStateGroup,
    options: &InjectionRenderOptions<'_>,
) -> Option<String> {
    if group.items.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    lines.push(format!("<{}>", group.tag));
    let description = group.description.trim();
    if !description.is_empty() {
        lines.push(description.to_string());
    }
    match group.kind {
        InjectionKind::Persistent => {
            for item in group.items.values() {
                let content = options.registry.model_content(item);
                if !content.trim().is_empty() {
                    lines.push(content);
                }
            }
        }
        InjectionKind::Transient => {
            append_transient_section(
                &mut lines,
                "CURRENT_TURN",
                group.items.values().filter(|item| {
                    item_turn_origin(item, options.current_turn_marker)
                        == InjectionTurnOrigin::Current
                }),
                options.registry,
            );
            append_transient_section(
                &mut lines,
                "RETAINED_FROM_EARLIER_TURNS",
                group.items.values().filter(|item| {
                    item_turn_origin(item, options.current_turn_marker)
                        == InjectionTurnOrigin::Retained
                }),
                options.registry,
            );
        }
    }
    lines.push(format!("</{}>", group.tag));
    Some(lines.join("\n"))
}

/// Appends one transient turn-origin section when it contains renderable items.
/// 当临时回合来源分段包含可渲染内容项时追加该分段。
fn append_transient_section<'a>(
    lines: &mut Vec<String>,
    section_tag: &str,
    items: impl Iterator<Item = &'a InjectionStateItem>,
    registry: &InjectionRendererRegistry,
) {
    let contents = items
        .map(|item| registry.model_content(item))
        .filter(|content| !content.trim().is_empty())
        .collect::<Vec<_>>();
    if contents.is_empty() {
        return;
    }
    lines.push(format!("<{section_tag}>"));
    lines.extend(contents);
    lines.push(format!("</{section_tag}>"));
}

/// Normalizes one tag name to the stable limited form accepted by the renderer.
/// 将标签名规范化为渲染器接受的稳定受限形式。
fn normalize_tag(tag: &str) -> String {
    tag.trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .collect()
}

/// Normalizes one provider-visible item content value.
/// 规范化一条 Provider 可见内容值。
fn normalize_content(content: &str) -> String {
    content.trim().replace("\r\n", "\n").replace('\r', "\n")
}

/// Normalizes one renderer id to a stable lookup key.
/// 将渲染器标识规范化为稳定查询键。
fn normalize_renderer_id(id: &str) -> String {
    id.trim().to_ascii_lowercase()
}

/// Provider-visible transient turn-origin category.
/// Provider 可见临时注入的回合来源分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InjectionTurnOrigin {
    /// Item belongs to the current user turn.
    /// 内容项属于当前用户回合。
    Current,
    /// Item was retained from an earlier user turn.
    /// 内容项从更早用户回合保留下来。
    Retained,
}

/// Classifies one item by comparing its marker to the current turn marker.
/// 通过比较内容项标记与当前回合标记对其分类。
fn item_turn_origin(
    item: &InjectionStateItem,
    current_turn_marker: Option<&str>,
) -> InjectionTurnOrigin {
    let Some(current_turn_marker) = current_turn_marker
        .map(str::trim)
        .filter(|marker| !marker.is_empty())
    else {
        return if item.last_consumed_turn_id.is_some() {
            InjectionTurnOrigin::Retained
        } else {
            InjectionTurnOrigin::Current
        };
    };
    if item.created_turn_marker.as_deref() == Some(current_turn_marker) {
        InjectionTurnOrigin::Current
    } else {
        InjectionTurnOrigin::Retained
    }
}

/// Builds a short human label for one item's turn origin.
/// 为内容项回合来源构建简短人类标签。
fn item_turn_label(item: &InjectionStateItem, current_turn_marker: Option<&str>) -> Option<String> {
    if item.created_turn_marker.is_none() {
        return None;
    }
    Some(match item_turn_origin(item, current_turn_marker) {
        InjectionTurnOrigin::Current => String::from("当前轮次"),
        InjectionTurnOrigin::Retained => String::from("历史保留"),
    })
}

/// Builds a stable short content hash without adding an extra dependency.
/// 在不增加额外依赖的情况下构建稳定短内容哈希。
fn stable_content_hash(content: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in content.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
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
    }

    #[test]
    fn panel_label_treats_consumed_transient_items_as_retained_without_current_marker() {
        let mut state = ContextInjectionState::default();
        state.apply_insert_for_turn(transient_insert("desc", "memory", 5), "1", Some("turn-a"));
        state.consume_turn("turn-a", "2");

        let cards = state.panel_cards();

        assert_eq!(cards[0].items[0].turn_label.as_deref(), Some("历史保留"));
    }
}
