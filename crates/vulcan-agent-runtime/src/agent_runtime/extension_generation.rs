//! Immutable plugin and MCP extension generation captured by one execution.
//! 单次执行捕获的不可变插件与 MCP 扩展代次。

use super::conversation_hooks::VulcanPluginConversationHooks;
use crate::mcp::McpRuntimeAccess;
use std::sync::Arc;
use vulcan_plugin_runtime::VulcanPluginCatalogSnapshot;
use vulcan_prompt_skills::PromptSkillCatalog;

/// Atomically published extension resources consumed by one runtime preparation.
/// 由单次运行时准备原子消费的已发布扩展资源。
#[derive(Debug, Clone)]
pub struct ExtensionGeneration {
    /// Monotonic service-owned generation identifier.
    /// 由服务持有的单调代次标识。
    revision: u64,
    /// Immutable plugin catalog projected with this generation.
    /// 随当前代次投影的不可变插件目录。
    plugin_catalog: Option<Arc<VulcanPluginCatalogSnapshot>>,
    /// MCP partitions and leases projected with the same generation.
    /// 与同一代次一起投影的 MCP 分区与租约。
    mcp_runtime: McpRuntimeAccess,
    /// Immutable system-and-user prompt-skill catalog loaded only at startup or explicit refresh.
    /// 仅在启动或显式刷新时加载的不可变系统与用户提示词技能目录。
    prompt_skill_catalog: Option<Arc<PromptSkillCatalog>>,
    /// Frozen conversation plugin handlers prepared from this exact plugin catalog.
    /// 从本精确插件目录准备的冻结会话插件处理器。
    plugin_conversation_hooks: Option<Arc<VulcanPluginConversationHooks>>,
}

impl ExtensionGeneration {
    /// Builds one immutable extension generation from an already prepared catalog and MCP access surface.
    /// 根据已经准备好的目录与 MCP 访问面构建一个不可变扩展代次。
    ///
    /// # Parameters
    /// 参数
    /// `revision` is service-monotonic; remaining values are the exact atomically published resources.
    /// `revision` 由服务保证单调；其余值是需要原子发布的精确资源。
    ///
    /// # Returns
    /// 返回值
    /// Returns a generation that can be shared by in-flight executions through `Arc`.
    /// 返回一份可由执行中任务通过 `Arc` 共享的代次。
    pub fn new(
        revision: u64,
        plugin_catalog: Option<Arc<VulcanPluginCatalogSnapshot>>,
        mcp_runtime: McpRuntimeAccess,
        prompt_skill_catalog: Option<Arc<PromptSkillCatalog>>,
    ) -> Self {
        Self {
            revision,
            plugin_catalog,
            mcp_runtime,
            prompt_skill_catalog,
            plugin_conversation_hooks: None,
        }
    }

    /// Builds a service generation with the exact catalog owned by `hooks`.
    /// 使用 `hooks` 持有的精确目录构建服务代次。
    /// `revision`, `mcp_runtime`, and `prompt_skill_catalog` are the prepared publication resources.
    /// `revision`、`mcp_runtime` 与 `prompt_skill_catalog` 是准备发布的资源。
    /// Returns an immutable generation; catalog and conversation handlers cannot be paired independently.
    /// 返回不可变代次；目录与会话处理器不能被独立配对。
    pub fn new_with_plugin_conversation_hooks(
        revision: u64,
        hooks: Arc<VulcanPluginConversationHooks>,
        mcp_runtime: McpRuntimeAccess,
        prompt_skill_catalog: Option<Arc<PromptSkillCatalog>>,
    ) -> Self {
        Self {
            revision,
            plugin_catalog: Some(hooks.catalog()),
            mcp_runtime,
            prompt_skill_catalog,
            plugin_conversation_hooks: Some(hooks),
        }
    }

    /// Prepares a successor with replacement MCP and prompt catalogs while retaining plugin routing.
    /// 准备替换 MCP 与提示词目录的后继代次，同时保留插件路由。
    /// Returns the unpublished successor, or an error if the service revision is exhausted.
    /// 返回尚未发布的后继代次；服务修订号耗尽则返回错误。
    pub fn republish(
        &self,
        mcp_runtime: McpRuntimeAccess,
        prompt_skill_catalog: Option<Arc<PromptSkillCatalog>>,
    ) -> Result<Self, String> {
        Ok(Self {
            revision: self
                .revision
                .checked_add(1)
                .ok_or_else(|| String::from("Extension generation revision exhausted."))?,
            plugin_catalog: self.plugin_catalog.clone(),
            mcp_runtime,
            prompt_skill_catalog,
            plugin_conversation_hooks: self.plugin_conversation_hooks.clone(),
        })
    }

    /// Returns captured plugin handlers, or `None` for explicitly standalone generations.
    /// 返回捕获的插件处理器；显式独立代次返回 `None`。
    pub fn plugin_conversation_hooks(&self) -> Option<&VulcanPluginConversationHooks> {
        self.plugin_conversation_hooks.as_deref()
    }

    /// Returns the monotonic extension revision.
    /// 返回单调扩展修订号。
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the immutable plugin catalog captured by this generation.
    /// 返回当前代次捕获的不可变插件目录。
    pub fn plugin_catalog(&self) -> Option<Arc<VulcanPluginCatalogSnapshot>> {
        self.plugin_catalog.clone()
    }

    /// Returns the MCP access surface captured by this generation.
    /// 返回当前代次捕获的 MCP 访问面。
    pub fn mcp_runtime(&self) -> McpRuntimeAccess {
        self.mcp_runtime.clone()
    }

    /// Returns the immutable system-and-user prompt-skill catalog captured by this generation.
    /// 返回当前代次捕获的不可变系统与用户提示词技能目录。
    ///
    /// # Returns
    /// 返回值
    /// The published catalog, or `None` for standalone callers that did not provide one.
    /// 返回已发布目录；未提供目录的独立调用方返回 `None`。
    pub fn prompt_skill_catalog(&self) -> Option<Arc<PromptSkillCatalog>> {
        self.prompt_skill_catalog.clone()
    }
}
