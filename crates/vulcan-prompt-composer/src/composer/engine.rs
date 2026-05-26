use crate::cache::keys::node_cache_key;
use crate::cache::memory::MemoryPromptCache;
use crate::composer::request::PromptAssemblyInput;
use crate::composer::resources::PromptComposerResourceConfig;
use crate::composer::response::PromptAssemblyOutput;
use crate::error::PromptComposerError;
use crate::node::render_node;
use crate::personality::catalog::PersonalityCatalog;
use crate::source::hashing::stable_text_hash;
use crate::strategy::compaction::ContextCompactionStrategy;
use crate::strategy::primary::PrimaryAgentStrategy;
use crate::strategy::subagent_clone::SubagentCloneStrategy;
use crate::strategy::subagent_functional::SubagentFunctionalStrategy;
use crate::strategy::traits::PromptAssemblyStrategy;
use crate::tool_help::bundle::build_tool_help_bundle;
use crate::tool_help::registry::ToolHelpRegistry;
use crate::types::{PromptAssemblyMode, PromptSegment, PromptTarget};

/// Prompt composer engine.
/// 提示词组合引擎。
#[derive(Debug)]
pub struct PromptComposerEngine {
    /// Shared node and bundle cache.
    /// 共享节点与组合缓存。
    cache: MemoryPromptCache,
    /// Personality catalog.
    /// 人格目录。
    personality_catalog: PersonalityCatalog,
    /// Tool-help registry.
    /// 工具帮助注册表。
    tool_help_registry: ToolHelpRegistry,
}

impl PromptComposerEngine {
    /// Build one prompt composer engine.
    /// 构建一个提示词组合引擎。
    pub fn new(
        cache: MemoryPromptCache,
        personality_catalog: PersonalityCatalog,
        tool_help_registry: ToolHelpRegistry,
    ) -> Self {
        Self {
            cache,
            personality_catalog,
            tool_help_registry,
        }
    }

    /// Build one engine with built-in defaults.
    /// 使用内置默认值构建一个引擎。
    pub fn with_builtins() -> Self {
        Self::new(
            MemoryPromptCache::new(),
            PersonalityCatalog::built_in(),
            ToolHelpRegistry::built_in(),
        )
    }

    /// Build one engine from optional resource directories.
    /// 基于可选资源目录构建一个引擎。
    pub fn with_resources(
        resources: &PromptComposerResourceConfig,
    ) -> Result<Self, PromptComposerError> {
        Self::with_resources_and_cache(resources, MemoryPromptCache::new())
    }

    /// Build one engine from optional resource directories and a reusable cache.
    /// 基于可选资源目录与可复用缓存构建一个引擎。
    pub fn with_resources_and_cache(
        resources: &PromptComposerResourceConfig,
        cache: MemoryPromptCache,
    ) -> Result<Self, PromptComposerError> {
        let personality_catalog = match resources.personality_directory.as_deref() {
            Some(directory) => PersonalityCatalog::load_with_directory(directory)?,
            None => PersonalityCatalog::built_in(),
        };
        let tool_help_registry = match resources.tool_help_directory.as_deref() {
            Some(directory) => ToolHelpRegistry::load_with_directory(directory)?,
            None => ToolHelpRegistry::built_in(),
        };
        Ok(Self::new(cache, personality_catalog, tool_help_registry))
    }

    /// Assemble one structured prompt output.
    /// 装配一个结构化提示词输出。
    pub fn compose(
        &self,
        input: &PromptAssemblyInput,
    ) -> Result<PromptAssemblyOutput, PromptComposerError> {
        let strategy = self.select_strategy(input.mode);
        let personality = self.personality_catalog.resolve(&input.personality)?;

        let mut output = PromptAssemblyOutput::default();
        output.diagnostics.strategy_id = strategy.id().to_string();
        output.diagnostics.personality_id = personality.as_ref().map(|value| value.id.clone());

        let plan = strategy.build_plan(input);
        for planned_node in plan.nodes {
            if matches!(planned_node, crate::strategy::PlannedNode::ToolHelpBundle) {
                for segment in build_tool_help_bundle(
                    &input.tool_catalog,
                    &self.tool_help_registry,
                    &self.cache,
                    &mut output.diagnostics,
                ) {
                    push_segment(&mut output, segment);
                }
                continue;
            }

            let maybe_segment = render_node(
                planned_node,
                input,
                personality.as_ref(),
                &self.cache,
                &mut output.diagnostics,
            );
            if let Some(segment) = maybe_segment {
                push_segment(&mut output, segment);
            }
        }

        for (index, custom_segment) in input.custom_developer_segments.iter().enumerate() {
            if custom_segment.trim().is_empty() {
                continue;
            }
            let segment = build_cached_custom_segment(
                &self.cache,
                &mut output.diagnostics,
                "custom_developer",
                index,
                crate::types::PromptNodeKind::Developer,
                PromptTarget::Developer,
                format!("Custom Developer Segment {index}"),
                custom_segment,
                format!("Runtime custom developer segment {index}"),
            );
            push_segment(&mut output, segment);
        }
        for (index, custom_segment) in input.custom_user_segments.iter().enumerate() {
            if custom_segment.trim().is_empty() {
                continue;
            }
            let segment = build_cached_custom_segment(
                &self.cache,
                &mut output.diagnostics,
                "custom_user",
                index,
                crate::types::PromptNodeKind::User,
                PromptTarget::User,
                format!("Custom User Segment {index}"),
                custom_segment,
                format!("Runtime custom user segment {index}"),
            );
            push_segment(&mut output, segment);
        }

        Ok(output)
    }

    /// Expose the shared cache reference.
    /// 暴露共享缓存引用。
    pub fn cache(&self) -> &MemoryPromptCache {
        &self.cache
    }

    /// Select the concrete strategy for one assembly mode.
    /// 为一种装配模式选择具体策略。
    fn select_strategy(&self, mode: PromptAssemblyMode) -> Box<dyn PromptAssemblyStrategy> {
        match mode {
            PromptAssemblyMode::Primary => Box::new(PrimaryAgentStrategy),
            PromptAssemblyMode::ContextCompaction => Box::new(ContextCompactionStrategy),
            PromptAssemblyMode::SubagentClone => Box::new(SubagentCloneStrategy),
            PromptAssemblyMode::SubagentFunctional => Box::new(SubagentFunctionalStrategy),
        }
    }
}

/// Push one segment into the matching output bucket.
/// 将一个片段推入匹配的输出桶。
fn push_segment(output: &mut PromptAssemblyOutput, segment: PromptSegment) {
    match segment.target {
        PromptTarget::System => output.system_segments.push(segment),
        PromptTarget::Developer => output.developer_segments.push(segment),
        PromptTarget::User => output.user_segments.push(segment),
        PromptTarget::ToolMetadata => output.tool_metadata_segments.push(segment),
        PromptTarget::DiagnosticsOnly => {}
    }
}

/// Build one cached runtime-provided custom segment.
/// 构建一个带缓存的运行时自定义片段。
fn build_cached_custom_segment(
    cache: &MemoryPromptCache,
    diagnostics: &mut crate::composer::diagnostics::PromptComposerDiagnostics,
    namespace: &str,
    index: usize,
    kind: crate::types::PromptNodeKind,
    target: PromptTarget,
    title: impl Into<String>,
    body: &str,
    source_label: impl Into<String>,
) -> PromptSegment {
    let fingerprint = stable_text_hash(body);
    let cache_key = node_cache_key(namespace, fingerprint);
    let source_label = source_label.into();
    if let Some(segment) = cache.get_node(&cache_key) {
        diagnostics.record_node_cache(&cache_key, true);
        return segment.with_source_label(source_label);
    }
    diagnostics.record_node_cache(&cache_key, false);
    let segment = PromptSegment::new(
        format!("{namespace}_{index}"),
        cache_key,
        kind,
        target,
        title,
        body.to_string(),
    )
    .with_source_label(source_label);
    cache.put_node(segment.clone());
    segment
}
