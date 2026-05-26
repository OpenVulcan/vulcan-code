/// Cache invalidation reason.
/// 缓存失效原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptInvalidationReason {
    /// Model changed.
    /// 模型变化。
    ModelChanged,
    /// Provider family changed.
    /// Provider 家族变化。
    ProviderFamilyChanged,
    /// Personality selection changed.
    /// 人格选择变化。
    PersonalityChanged,
    /// Visible tool set changed.
    /// 可见工具集变化。
    ToolVisibilityChanged,
    /// Tool schema changed.
    /// 工具 schema 变化。
    ToolSchemaChanged,
    /// LuaSkill catalog changed.
    /// LuaSkill 目录变化。
    LuaSkillCatalogChanged,
    /// Project rule file changed.
    /// 项目规则文件变化。
    ProjectRuleFileChanged,
    /// Agent profile changed.
    /// Agent 配置变化。
    AgentProfileChanged,
    /// Subagent mode changed.
    /// 子代理模式变化。
    SubagentModeChanged,
    /// Session compaction completed.
    /// 会话压缩已完成。
    SessionCompacted,
    /// Session baseline rebuilt.
    /// 会话基线已重建。
    SessionBaselineRebuilt,
}

/// External invalidation event delivered into the composer.
/// 传递给组合器的外部失效事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptInvalidationEvent {
    /// Invalidation reason.
    /// 失效原因。
    pub reason: PromptInvalidationReason,
    /// Optional cache-key prefixes to drop.
    /// 需要丢弃的可选缓存键前缀。
    pub cache_key_prefixes: Vec<String>,
}
