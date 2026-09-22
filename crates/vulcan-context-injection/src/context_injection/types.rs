//! Serializable context-injection data types.
//! 可序列化的上下文注入数据类型。

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
    /// Stable producer source identity independent from the rendered body.
    /// 与渲染正文无关的稳定生产者来源身份。
    pub source_id: String,
    /// Stable occurrence identity allocated when this instruction is first created.
    /// 在该指令首次创建时分配的稳定发生点身份。
    pub occurrence_id: String,
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

/// Describes one transient injection that expired at a concrete logical turn.
/// 描述一条在明确逻辑回合失效的临时注入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InjectionExpiryEvent {
    /// Stable identity for the append-only expiration marker.
    /// 用于追加式失效标记的稳定身份。
    pub event_id: String,
    /// Stable producer source identity of the expired instruction.
    /// 已失效指令的稳定生产者来源身份。
    pub source_id: String,
    /// Stable occurrence identity allocated when the instruction was created.
    /// 指令创建时分配的稳定发生点身份。
    pub occurrence_id: String,
    /// Normalized injection tag that owned the expired item.
    /// 拥有已失效内容项的规范化注入标签。
    pub tag: String,
    /// Stable content hash retained for correlation with the earlier body.
    /// 保留稳定内容哈希，用于关联此前已经发送的正文。
    pub content_hash: String,
    /// Logical turn that consumed the final TTL unit.
    /// 消耗最后一个 TTL 单位的逻辑回合。
    pub turn_id: String,
    /// Provider-visible marker body appended after the retained earlier body.
    /// 追加在此前保留正文之后、供 Provider 看到的失效标记正文。
    pub body: String,
}

/// Session or scope level context-injection state.
/// 会话或作用域级上下文注入状态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextInjectionState {
    /// Active groups keyed by rendered tag name.
    /// 按渲染标签名称索引的活动分组。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub groups: BTreeMap<String, InjectionStateGroup>,
    /// Monotonic occurrence counters keyed by producer source identity.
    /// 按生产者来源身份保存的单调发生点计数器。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_sequences: BTreeMap<String, u64>,
    /// Append-only expiration markers waiting for the next provider request.
    /// 等待下一次 Provider 请求追加发送的失效标记队列。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_expiry_events: Vec<InjectionExpiryEvent>,
}

/// Result counters produced when expired transient injections are removed.
/// 移除已失效临时注入时产出的结果计数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InjectionPruneReport {
    /// Number of transient injection items removed because their TTL has expired.
    /// 因 TTL 已失效而被移除的临时注入内容项数量。
    pub removed_item_count: usize,
    /// Number of groups removed after their last item was pruned.
    /// 最后一条内容项被裁剪后随之移除的分组数量。
    pub removed_group_count: usize,
}
