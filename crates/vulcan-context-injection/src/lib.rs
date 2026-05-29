//! Shared context-injection primitives for hook-driven provider context overlays.
//! 面向 hook 驱动 Provider 上下文覆盖层的共享上下文注入原语。

use serde::{Deserialize, Serialize};
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
        for insert in inserts {
            self.apply_insert(insert, now);
        }
        self.remove_empty_groups();
    }

    /// Applies one hook-collected insertion operation to the state.
    /// 将一条 hook 收集到的插入操作应用到当前状态。
    pub fn apply_insert(&mut self, insert: InjectionInsert, now: &str) {
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
            let content_hash = stable_content_hash(&normalized_content);
            let ttl_turns = match insert.kind {
                InjectionKind::Persistent => None,
                InjectionKind::Transient => {
                    item.ttl_turns.or(insert.ttl_turns).filter(|ttl| *ttl > 0)
                }
            };
            if insert.kind == InjectionKind::Transient && ttl_turns.is_none() {
                continue;
            }
            let compaction_policy = item.compaction_policy.unwrap_or(insert.compaction_policy);
            let source_display = item
                .source_display
                .or_else(|| insert.source_display.clone());
            group
                .items
                .entry(content_hash.clone())
                .and_modify(|existing| {
                    existing.content = normalized_content.clone();
                    existing.ttl_turns = ttl_turns;
                    existing.remaining_turns = ttl_turns;
                    existing.compaction_policy = compaction_policy;
                    existing.source_display = source_display.clone();
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

    /// Builds the provider-facing resolved view without mutating persisted state.
    /// 构建面向 Provider 的解析视图，同时不修改持久化状态。
    pub fn resolve_view(&self) -> ResolvedInjectionView {
        let mut persistent_blocks = Vec::new();
        let mut transient_blocks = Vec::new();
        for group in self.sorted_groups() {
            let Some(block) = render_group_block(group) else {
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
        self.sorted_groups()
            .into_iter()
            .filter_map(|group| {
                let items = group
                    .items
                    .values()
                    .map(|item| InjectionPanelItem {
                        content: item.content.clone(),
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
    /// Full provider-visible item content.
    /// 完整的 Provider 可见内容项。
    pub content: String,
    /// Human-readable TTL label shown in muted text.
    /// 以灰色文字展示的人类可读 TTL 标签。
    pub ttl_label: String,
    /// Optional source label shown in diagnostics and UI only.
    /// 仅在诊断和界面中展示的可选来源标签。
    pub source_display: Option<String>,
}

/// Renders one group into provider-facing tag text.
/// 将一个分组渲染为面向 Provider 的标签文本。
fn render_group_block(group: &InjectionStateGroup) -> Option<String> {
    if group.items.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    lines.push(format!("<{}>", group.tag));
    let description = group.description.trim();
    if !description.is_empty() {
        lines.push(description.to_string());
    }
    for item in group.items.values() {
        lines.push(item.content.clone());
    }
    lines.push(format!("</{}>", group.tag));
    Some(lines.join("\n"))
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
            "<vmm_inject>\n这是vmm记忆的说明，以下内容仅供参考，不作为事实标准\nVMM_ID:137 这个是某个记忆\n</vmm_inject>"
        );
        assert!(!view.transient_prefix.contains("TTL"));
        assert!(!view.transient_prefix.contains("source"));
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
}
