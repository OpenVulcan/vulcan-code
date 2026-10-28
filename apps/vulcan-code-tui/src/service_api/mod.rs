// WebUI service functions, mirrored one to one.
// 一对一镜像的 WebUI 服务函数。
//
// Every submodule mirrors one file of `apps/vulcan-code-web/src/service/` and every function keeps
// the WebUI function's name (in snake case), its RPC method and its parameter shape. The TUI does
// not define a second vocabulary for the same service: pages call these functions, the service
// backend sends the resulting `ServiceCall` as is, and the mock backend answers the same method
// names. When the WebUI adds or changes a function, the change belongs here first.
// 每个子模块镜像 `apps/vulcan-code-web/src/service/` 中的一个文件，每个函数保留 WebUI 函数名
// （蛇形命名）、RPC 方法与参数形状。TUI 不为同一服务定义第二套词汇：页面调用这些函数，服务后端
// 原样发送得到的 `ServiceCall`，模拟后端响应同样的方法名。WebUI 新增或修改函数时，应先在此处同步。

pub mod answer_mode;
#[cfg(test)]
mod contracts_tests;

pub mod knowledge_base;
#[cfg(test)]
mod knowledge_contract_tests;
pub mod licenses;
pub mod markdown_memory;
pub mod mcp_service;
pub mod mcp_settings;
pub mod memory_actions;
pub mod permission_judge;
pub mod permission_mode;
pub mod project_actions;
pub mod review_prompt;
pub mod setter;
pub mod settings_actions;
pub mod skills_actions;
pub mod subagent_models;
pub mod workbench_snapshot;

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde_json::Value;

/// A fresh id in UUID v4 layout for `commandId`, submission ids and idempotency keys, where the
/// WebUI uses `crypto.randomUUID()`.
/// UUID v4 格式的新 ID，用于 `commandId`、提交 ID 与幂等键，对应 WebUI 的 `crypto.randomUUID()`。
/// Returns `String`.
/// 返回 `String`。
pub fn fresh_command_id() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    // Word.
    // 随机散列字生成器。
    let word = || {
        // Hasher.
        // 独立随机种子散列器。
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        hasher.finish()
    };
    // First random hash word; Second random hash word.
    // 第一个随机散列字；第二个随机散列字。
    let (a, b) = (word(), word());
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        a >> 32,
        (a >> 16) & 0xffff,
        a & 0xfff,
        0x8000 | (b >> 48) & 0x3fff,
        b & 0xffff_ffff_ffff
    )
}

/// Data the system areas keep per loader; a mutation names the slots it invalidates.
/// 系统区域按加载器保存的数据；变更调用会列出其失效的槽位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Slot {
    /// `loadWorkbenchSnapshot`; mutations returning a workbench snapshot refresh it directly.
    /// 返回工作台快照的变更会直接刷新它。
    Workbench,
    /// `searchLuaSkillsHub`
    /// 当前一页 LuaSkills Hub 搜索结果。
    LuaSkillsHub,
    /// `getLuaSkillsHubSkill`
    /// 当前打开的 LuaSkills Hub 技能详情。
    LuaSkillsHubSkill,
    /// `loadPublicAiSettings`
    /// 公开 AI 设置快照。
    PublicAiSettings,
    /// `loadOutputStyleCatalog`
    /// 输出风格目录。
    OutputStyleCatalog,
    /// `loadSystemPromptSettings` for the user layer and the optional selected project.
    /// 用户层级与可选所选项目的基础系统提示词设置。
    SystemPrompt,
    /// `loadMcpRuntimeSnapshot`
    /// MCP 运行时快照。
    McpRuntime,
    /// `loadMcpServerCapabilities` of the open server.
    /// 当前打开服务器的能力。
    McpCapabilities,
    /// `loadMcpOAuthStatus` of the open server.
    /// 当前打开服务器的 OAuth 登录状态。
    McpOAuth,
    /// `listProviderTasks`
    /// 保留的供应商异步任务。
    ProviderTasks,
    /// `listKnowledgeLibraries`
    /// 知识库目录。
    KnowledgeLibraries,
    /// Current custom dictionary of the exact selected library.
    /// 精确所选知识库的当前自定义词典。
    KnowledgeWords,
    /// `listKnowledgeProjectBindings` of the open library.
    /// 当前打开知识库的项目绑定。
    KnowledgeBindings,
    /// `getKnowledgeOperation` of the open library.
    /// 当前打开知识库的通用操作。
    KnowledgeJob,
    /// `getKnowledgeIndexIntegrity` of the open library.
    /// 当前打开知识库的索引完整性。
    KnowledgeIntegrity,
    /// `searchManagedKnowledgeLibrary`
    /// 所选知识库的托管搜索结果。
    KnowledgeSearch,
    /// Folder identities in the selected managed library.
    /// 所选托管知识库中的目录身份。
    KnowledgeFolders,
    /// One bounded page of managed document summaries.
    /// 托管文档摘要的一页有界结果。
    KnowledgeDocuments,
    /// Exact current or immutable historical body window selected for reading.
    /// 为阅读选择的精确当前或不可变历史正文窗口。
    KnowledgeDocument,
    /// Current desired/active vector spaces and bounded failure details.
    /// 当前期望及活动向量空间和有界失败详情。
    KnowledgeEmbeddings,
    /// Immutable bounded history summaries.
    /// 不可变有界历史摘要。
    KnowledgeChanges,
    /// Exact immutable history change and bounded pre/post images.
    /// 精确不可变历史变更及有界前后镜像。
    KnowledgeChange,
    /// Native package directory and exact archive file entries.
    /// 原生套件目录及精确归档文件条目。
    KnowledgePackages,
    /// Currently authorized session library selection.
    /// 当前授权会话知识库选择。
    KnowledgeSelection,
    /// Exact current project's authorized libraries for session choice; separate from the global registry.
    /// 当前精确项目用于会话选择的授权知识库；与全局注册表分离。
    KnowledgeAuthorizedLibraries,
    /// Exact project library grants used by session defaults.
    /// 会话默认使用的精确项目知识库授权。
    ProjectKnowledgeBindings,
    /// `runVmmMemoryAction(native_project_catalog)`
    /// 原生记忆项目及绑定目录。
    VmmProjects,
    /// `runVmmMemoryAction(native_memory_records)`
    /// 所选原生记忆项目的记录。
    VmmRecords,
    /// `runVmmMemoryAction(native_profile_records)`
    /// 所选原生项目或用户的画像记录。
    VmmProfile,
    /// `runVmmMemoryAction(native_resource_catalog)`
    /// 原生记忆资源目录。
    VmmResources,
    /// The selected physical resource's effective text.
    /// 所选物理资源的生效文本。
    VmmResourceFile,
    /// `runVmmMemoryAction(native_runtime_policy)`
    /// 原生记忆运行时策略。
    VmmRuntimePolicy,
    /// `runVmmMemoryAction(native_vector_status)`
    /// 原生记忆向量代次状态。
    VmmVector,
    /// `runVmmMemoryAction(native_lexical_status)`
    /// 原生记忆词法索引状态。
    VmmLexical,
    /// `runVmmMemoryAction(native_retention_status)`
    /// 原生记忆保留策略状态。
    VmmRetention,
    /// `runVmmMemoryAction(native_recycle_bin)`
    /// 原生记忆回收批次目录。
    VmmRecycleBin,
    /// `loadMemorySettings`: the memory mode and the Markdown injection budget.
    /// 记忆模式与 Markdown 注入预算。
    MemorySettings,
    /// `loadMarkdownMemory` of the open Markdown memory page; its loader carries the project.
    /// 当前 Markdown 记忆页的 `loadMarkdownMemory`；其加载器携带项目。
    MarkdownMemory,
    /// `readMarkdownMemoryFile` opened for whole-file editing; its loader carries the scope.
    /// 为整文件编辑打开的 `readMarkdownMemoryFile`；其加载器携带层。
    MarkdownMemoryFile,
    /// `previewMarkdownMemoryImport` of the package under review; its loader carries the
    /// package path and the chosen project targets.
    /// 正在审阅的导出包的 `previewMarkdownMemoryImport`；其加载器携带导出包路径与所选项目去向。
    MarkdownMemoryImport,
    /// `loadLicenseCatalog`
    /// 许可证目录。
    Licenses,
    /// `loadLicenseBody` of the open section.
    /// 当前打开章节的许可证正文。
    LicenseBody,
    /// `loadPluginUiModule` of the open plugin mount: its layout, schema and messages.
    /// 当前打开插件挂载的 `loadPluginUiModule`：其版面、Schema 与文案。
    PluginUi,
    /// The open settings layout's configuration read action.
    /// 当前设置版面的配置读取 Action。
    PluginConfig,
    /// Status read of the layout's n-th credential item, in document order.
    /// 版面中第 n 个凭据条目（按文档顺序）的状态读取。
    PluginCredential(u8),
    /// Read action of the layout's n-th data section, in document order.
    /// 版面中第 n 个数据区块（按文档顺序）的读取 Action。
    PluginData(u8),
    /// `readPluginTextAsset` of the open header document.
    /// 当前打开的标题区文档的 `readPluginTextAsset`。
    PluginAsset,
    /// `loadPermissionJudgeSettings`
    /// 权限判定设置页快照。
    PermissionJudgeSettings,
    /// `loadPermissionJudgeStatus`, polled while the permission judge page is open.
    /// 权限判定页打开期间轮询的实时状态。
    PermissionJudgeStatus,
    /// `loadPermissionJudgeEdgeModels`: the official direct models the signed-in account's
    /// deployment currently serves.
    /// 已登录账户所属部署当前提供的官方直连模型。
    PermissionJudgeEdgeModels,
    /// `loadPermissionJudgeEdgeEndpoints`: the endpoint table of the chosen official direct model,
    /// polled while the permission judge page is open.
    /// 所选官方直连模型的端点表，权限判定页打开期间轮询。
    PermissionJudgeEdgeEndpoints,
    /// The latest `benchmarkPermissionJudge` result of this page; never saved.
    /// 本页最近一次 `benchmarkPermissionJudge` 结果；不会持久化。
    PermissionJudgeBenchmark,
    /// The latest `testPermissionJudgeEdgeRequest` result of this page; never saved.
    /// 本页最近一次 `testPermissionJudgeEdgeRequest` 结果；不会持久化。
    PermissionJudgeEdgeRequestTest,
    /// The latest `inspectPermissionJudgePackage` result of the registration form; never saved.
    /// 登记表单最近一次 `inspectPermissionJudgePackage` 结果；不会持久化。
    PermissionJudgePackageInspection,
    /// `loadProjectPermissionJudgePolicy` of the project chosen on the permission strategy page.
    /// 权限策略页所选项目的 `loadProjectPermissionJudgePolicy`。
    ProjectPermissionJudgePolicy,
    /// The shown page of `loadPermissionJudgeRecords`; its loader carries the filters.
    /// `loadPermissionJudgeRecords` 当前显示的一页；其加载器携带筛选条件。
    PermissionJudgeRecords,
    /// `loadUnattendedSystemDefault` of the unattended defaults page.
    /// 无人值守默认值页的 `loadUnattendedSystemDefault`。
    UnattendedSystemDefault,
    /// `loadProjectUnattendedDefault` of the project chosen on the unattended defaults page.
    /// 无人值守默认值页所选项目的 `loadProjectUnattendedDefault`。
    ProjectUnattendedDefault,
    /// `loadReviewRulesSystemDefault` of the review rules page.
    /// 复核规则页的 `loadReviewRulesSystemDefault`。
    ReviewRulesSystemDefault,
    /// `loadProjectReviewRules` of the project chosen on the review rules page.
    /// 复核规则页所选项目的 `loadProjectReviewRules`。
    ProjectReviewRules,
    /// `loadReviewPromptSystemDefault` of the review rules page.
    /// 复核规则页的 `loadReviewPromptSystemDefault`。
    ReviewPromptSystemDefault,
    /// `loadSessionReviewPrompt` of the selected saved session.
    /// 所选已保存会话的 `loadSessionReviewPrompt`。
    SessionReviewPrompt,
    /// `loadSubagentModels` of the subagent models page.
    /// 子代理模型页的 `loadSubagentModels`。
    SubagentModels,
    /// `loadSessionSubagentModels` of the selected saved session.
    /// 所选已保存会话的 `loadSessionSubagentModels`。
    SessionSubagentModels,
    /// `loadMcpService`: the endpoint switch, its clients and the grantable media models.
    /// 端点开关、其客户端与可授权媒体模型的 `loadMcpService`。
    McpService,
    /// `loadWorkspaceFileDirectory` of the folder the composer's `@` path mention lists.
    /// 输入框 `@` 路径提及所列文件夹的 `loadWorkspaceFileDirectory`。
    PathMentionDirectory,
}

impl Slot {
    /// Slots owned by the open plugin mount; they are dropped together when the mount changes.
    /// 当前插件挂载持有的槽位；挂载改变时一并丢弃。
    pub fn is_plugin_mount(self) -> bool {
        matches!(
            self,
            Slot::PluginUi
                | Slot::PluginConfig
                | Slot::PluginCredential(_)
                | Slot::PluginData(_)
                | Slot::PluginAsset
        )
    }
}

/// What a call does with its result.
/// 调用如何处理其结果。
#[derive(Debug, Clone, PartialEq)]
pub enum CallKind {
    /// A loader: the result replaces the slot.
    /// 加载器：结果替换该槽位。
    Load(Slot),
    /// A mutation: on success the listed slots reload with their last loader. Most lists are
    /// fixed; plugin settings name the positional slots of the open layout at run time.
    /// 变更：成功后所列槽位用其最近一次的加载器重新加载。多数列表固定；插件设置在运行时列出当前
    /// 版面的按位置槽位。
    Mutate(Cow<'static, [Slot]>),
    /// A write whose success sends one follow-up write, as WebUI functions that await two
    /// requests in order do. The first write has already taken effect when the follow-up runs, so
    /// the follow-up's reload set refreshes whether or not it succeeds.
    /// 成功后再发送一个后续写入的写入，对应 WebUI 中按顺序等待两个请求的函数。后续写入执行时第一个
    /// 写入已经生效，因此无论后续写入是否成功，都会刷新其重读集合。
    Then(Box<ServiceCall>),
    /// The read-only first step of a two-step action; its result opens a confirmation whose
    /// accept sends the second step, built from this call and its result.
    /// 两步操作中只读的第一步；其结果打开确认框，接受后发送由本调用及其结果构建的第二步。
    Preview(Preview),
}

/// Which confirmation a preview result opens.
/// 预检结果打开的确认类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preview {
    /// Current package inspection freezes the full archive fingerprint before independent import.
    /// 当前套件检查在独立导入前冻结完整归档指纹。
    KnowledgePackage,
    /// `inspectLocalPlugin` → `installLocalPlugin` / `updateLocalPlugin`
    /// 本地插件检查转为安装或更新。
    PluginInstall,

    /// `native_project_{clear,delete,migration}_preview` → `nativeProjectConfirmationRequest`
    /// 原生项目清空、删除或迁移预检转为确认执行。
    VmmProject,
    /// `native_removal_preview` → `native_removal_recycle`
    /// 原生来源移除预检转为回收执行。
    VmmRemoval,
    /// `native_retention_preview` → `native_retention_run`
    /// 原生保留策略预检转为确认执行。
    VmmRetention,
    /// `native_vector_rebuild_preview` → `native_vector_rebuild`.
    /// 向量重建预检转为确认执行。
    VmmVector,
    /// `native_vector_rebuild_preview` → `native_vector_adoption`.
    /// 向量重建预检转为确认沿用现有向量。
    VmmVectorAdoption,
}

/// One request to the Vulcan Code service, exactly as the WebUI sends it.
/// 发往 Vulcan Code 服务的一次请求，与 WebUI 发送的完全一致。
#[derive(Clone, PartialEq)]
pub struct ServiceCall {
    /// The WebUI function this call mirrors, in snake case, for notices and diagnostics.
    /// 本调用镜像的 WebUI 函数名（蛇形），用于提示与诊断。
    pub function: &'static str,
    /// Exact service RPC method; pages never invent alternate command names.
    /// 准确的服务 RPC 方法；页面不另行杜撰命令名称。
    pub method: &'static str,
    /// Exact request document, including transient secrets omitted from Debug output.
    /// 准确请求文档，包含在 Debug 输出中省略的临时机密。
    pub params: Value,
    /// Result routing and authoritative reload or preview behavior.
    /// 结果路由及权威重读或预检行为。
    pub kind: CallKind,
}

impl std::fmt::Debug for ServiceCall {
    /// Omits all request parameters because credential forms contain one-shot secrets.
    /// 省略全部请求参数，因为凭据表单包含仅单次发送的机密。
    /// `formatter` is the destination formatter.
    /// `formatter` 为目标格式化器。
    /// Returns `std::fmt::Result`.
    /// 返回 `std::fmt::Result`。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServiceCall")
            .field("function", &self.function)
            .field("method", &self.method)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl ServiceCall {
    /// Builds a read-only request whose result replaces exactly one declared slot.
    /// 构建只读请求，其结果仅替换一个声明的槽位。
    /// `function` is the matching WebUI function name used for notices.
    /// `function` 为用于提示的对应 WebUI 函数名。
    /// `method` is the static WebUI RPC method name.
    /// `method` 为静态 WebUI RPC 方法名。
    /// `params` is the exact RPC parameter document.
    /// `params` 为准确 RPC 参数文档。
    /// `slot` is the declared result slot.
    /// `slot` 为已声明的结果槽位。
    /// Returns `Self`.
    /// 返回 `Self`。
    pub(crate) fn load(
        function: &'static str,
        method: &'static str,
        params: Value,
        slot: Slot,
    ) -> Self {
        ServiceCall {
            function,
            method,
            params,
            kind: CallKind::Load(slot),
        }
    }

    /// Builds a write request with its explicit authoritative reload set.
    /// 构建写入请求及其明确的权威重读集合。
    /// `function` is the matching WebUI function name used for notices.
    /// `function` 为用于提示的对应 WebUI 函数名。
    /// `method` is the static WebUI RPC method name.
    /// `method` 为静态 WebUI RPC 方法名。
    /// `params` is the exact RPC parameter document.
    /// `params` 为准确 RPC 参数文档。
    /// `reload` is the authoritative slots invalidated by success.
    /// `reload` 为成功后失效的权威槽位。
    /// Returns `Self`.
    /// 返回 `Self`。
    pub(crate) fn mutate(
        function: &'static str,
        method: &'static str,
        params: Value,
        reload: &'static [Slot],
    ) -> Self {
        ServiceCall {
            function,
            method,
            params,
            kind: CallKind::Mutate(Cow::Borrowed(reload)),
        }
    }

    /// This write followed, once it succeeds, by `next`; see `CallKind::Then`.
    /// 本写入成功后再发送 `next`；参见 `CallKind::Then`。
    pub fn then(mut self, next: ServiceCall) -> ServiceCall {
        self.kind = CallKind::Then(Box::new(next));
        self
    }

    /// The same request ready to be sent again: a one-shot invocation identity is replaced,
    /// since the service never accepts one twice; every other call is returned unchanged.
    /// 可再次发送的同一请求：一次性调用身份会被替换，因为服务绝不接受重复身份；其他调用原样返回。
    /// Returns the call to send for a reload or a refresh.
    /// 返回重新加载或刷新时要发送的调用。
    pub fn renewed(&self) -> ServiceCall {
        let mut call = self.clone();
        if let Some(invocation) = call.params.pointer_mut("/identity/invocationId") {
            *invocation = Value::String(fresh_command_id());
        }
        call
    }

    /// Reads one parameter by JSON pointer, e.g. `/request/enabled`.
    /// 按 JSON 指针读取一个参数，例如 `/request/enabled`。
    /// Returns `&Value`.
    /// 返回 `&Value`。
    pub fn param(&self, pointer: &str) -> &Value {
        self.params.pointer(pointer).unwrap_or(&Value::Null)
    }
}

/// The latest result of one slot and the loader that produced it.
/// 单个槽位的最新结果及产生它的加载器。
#[derive(Debug, Clone)]
pub struct SlotData {
    /// Exact loader whose result is currently displayed.
    /// 当前显示结果对应的准确加载器。
    pub call: ServiceCall,
    /// Sanitized authoritative result, retaining the original WebUI DTO shape.
    /// 脱敏后的权威结果，保留原始 WebUI DTO 形状。
    pub value: Value,
}

/// Raw WebUI DTOs of the system areas, keyed by slot. Pages read them directly, so the TUI keeps
/// no second model of the same data.
/// 系统区域的原始 WebUI DTO，以槽位为键。页面直接读取它们，因此 TUI 不为同一数据维护第二套模型。
#[derive(Debug, Clone, Default)]
pub struct Slots(BTreeMap<Slot, SlotData>);

impl Slots {
    /// Replaces the exact slot value and records the loader that produced it.
    /// 替换准确槽位值并记录产生它的加载器。
    /// `slot` is the declared result slot.
    /// `slot` 为已声明的结果槽位。
    /// `call` is the exact request metadata for this result.
    /// `call` 为本结果的准确请求元数据。
    /// `value` is the protocol value being read or projected.
    /// `value` 为正在读取或投影的协议值。
    pub fn set(&mut self, slot: Slot, call: ServiceCall, value: Value) {
        self.0.insert(slot, SlotData { call, value });
    }

    /// The slot value, or `Null` while it has not loaded.
    /// 槽位的值；尚未加载时为 `Null`。
    /// `slot` is the declared result slot.
    /// `slot` 为已声明的结果槽位。
    /// Returns `&Value`.
    /// 返回 `&Value`。
    pub fn value(&self, slot: Slot) -> &Value {
        self.0.get(&slot).map_or(&Value::Null, |data| &data.value)
    }

    /// Reads inside a slot by JSON pointer, e.g. `/gitCommit/enabled`.
    /// 按 JSON 指针读取槽位内部，例如 `/gitCommit/enabled`。
    /// `slot` is the declared result slot.
    /// `slot` 为已声明的结果槽位。
    /// Returns `&Value`.
    /// 返回 `&Value`。
    pub fn at(&self, slot: Slot, pointer: &str) -> &Value {
        self.value(slot).pointer(pointer).unwrap_or(&Value::Null)
    }

    /// The loader that filled the slot, for reloading it.
    /// 填充该槽位的加载器，用于重新加载。
    /// `slot` is the declared result slot.
    /// `slot` 为已声明的结果槽位。
    /// Returns `Option<&ServiceCall>`.
    /// 返回 `Option<&ServiceCall>`。
    pub fn loader(&self, slot: Slot) -> Option<&ServiceCall> {
        self.0.get(&slot).map(|data| &data.call)
    }

    /// Checks whether a slot has an observed result, including an explicit Null response.
    /// 检查槽位是否已有观察结果，包含明确的 Null 响应。
    /// `slot` is the declared result slot.
    /// `slot` 为已声明的结果槽位。
    /// Returns `bool`.
    /// 返回 `bool`。
    pub fn loaded(&self, slot: Slot) -> bool {
        self.0.contains_key(&slot)
    }

    /// Discards a slot and its loader when the selected authority changes.
    /// 所选权威数据改变时丢弃槽位及其加载器。
    /// `slot` is the declared result slot.
    /// `slot` 为已声明的结果槽位。
    pub fn clear(&mut self, slot: Slot) {
        self.0.remove(&slot);
    }

    /// Drops every slot `drop` selects, e.g. all slots of a plugin mount that closed.
    /// 丢弃 `drop` 选中的全部槽位，例如已关闭插件挂载的所有槽位。
    pub fn clear_where(&mut self, drop: impl Fn(Slot) -> bool) {
        self.0.retain(|slot, _| !drop(*slot));
    }
}

/// Whether a field name holds a credential. Matching is on the normalized name (lowercase, no
/// `_` or `-`), so `api_key`, `apiKey` and `grpcApiKey` all match while ids such as
/// `credential_id` or `repositoryKey` do not.
/// 字段名是否承载凭据。按规范化名称（小写、去掉 `_` 与 `-`）匹配，因此 `api_key`、`apiKey`、
/// `grpcApiKey` 均匹配，而 `credential_id`、`repositoryKey` 等标识不匹配。
/// `name` is the exact requested catalog or branch name.
/// `name` 为准确请求的目录或分支名称。
/// Returns `bool`.
/// 返回 `bool`。
fn secret_field(name: &str) -> bool {
    // Name.
    // 当前名称。
    let name: String = name
        .chars()
        .filter(|c| *c != '_' && *c != '-')
        .collect::<String>()
        .to_lowercase();
    const PARTS: [&str; 8] = [
        "secret",
        "password",
        "apikey",
        "accesskey",
        "privatekey",
        "managementkey",
        "authorization",
        "bearer",
    ];
    // Manifest secretKey identifies a credential-store entry; it never contains its value.
    // 清单 secretKey 标识凭据库条目，不承载其值。
    name != "secretkey" && (PARTS.iter().any(|part| name.contains(part)) || name.ends_with("token"))
}

/// Blanks every string stored under a credential field, recursively. The TUI never shows or
/// keeps secrets, so every service result passes through here before it reaches the UI.
/// 递归清空凭据字段下的全部字符串。TUI 从不显示或保存密钥，因此所有服务结果在到达界面前都要经过
/// 此处。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
pub fn scrub_secrets(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // Header/environment DTO rows use key/value pairs; only the value is credential data.
            // 请求头或环境 DTO 使用键值行；仅其值承载凭据。
            if map
                .get("key")
                .and_then(Value::as_str)
                .is_some_and(secret_field)
                && let Some(value) = map.get_mut("value")
                && value.is_string()
            {
                *value = Value::String(String::new());
            }
            for (key, item) in map.iter_mut() {
                // MCP headersFromEnv values are environment-variable names, not credential values.
                // MCP headersFromEnv 的值为环境变量名，不是凭据值。
                if key == "headersFromEnv" {
                    continue;
                }
                // Plugin message catalogs map message ids such as `apiKeyLabel` to display text.
                // 插件文案目录将 `apiKeyLabel` 等消息标识映射为显示文本。
                if key == "messagesByLocale" {
                    continue;
                }
                // Arbitrarily named headers, environment values and installation inputs may be secret.
                // 任意命名的请求头、环境值与安装输入都可能包含机密。
                if matches!(key.as_str(), "headers" | "env" | "initialSecrets")
                    && let Some(rows) = item.as_array_mut()
                {
                    for row in rows {
                        if let Some(value) = row.get_mut("value")
                            && value.is_string()
                        {
                            *value = Value::String(String::new());
                        }
                    }
                }
                // The service placeholder stands for a stored secret; echoing it back keeps that secret.
                // 服务占位符代表已存储的机密；原样回传即保留该机密。
                if secret_field(key)
                    && item.as_str().is_some_and(|value| {
                        value != vulcan_service_api::WEBUI_REDACTED_SECRET_PLACEHOLDER
                    })
                {
                    *item = Value::String(String::new());
                } else {
                    scrub_secrets(item);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(scrub_secrets),
        _ => {}
    }
}

/// The second step of an accepted preview, built from the preview call and its result.
/// 已接受预检的第二步，由预检调用及其结果构建。
/// `workbench` identifies install versus update; `None` rejects incomplete or non-executable previews.
/// `workbench` 区分安装与更新；`None` 拒绝信息缺失或无法执行的预检。
/// `call` is the exact request metadata for this result.
/// `call` 为本结果的准确请求元数据。
/// Returns `Option<ServiceCall>`.
/// 返回 `Option<ServiceCall>`。
pub fn confirm_preview(
    call: &ServiceCall,
    result: &Value,
    workbench: &Value,
) -> Option<ServiceCall> {
    let CallKind::Preview(preview) = call.kind else {
        return None;
    };
    match preview {
        Preview::KnowledgePackage => knowledge_base::confirmed_package_import(result),
        Preview::PluginInstall => {
            // Plugin id.
            // 准确插件身份。
            let plugin_id = result.get("pluginId").and_then(Value::as_str);
            // Update.
            // 是否替换用户插件。
            let update = workbench
                .get("plugins")
                .and_then(Value::as_array)
                .and_then(|plugins| {
                    plugins
                        .iter()
                        .find(|p| p.get("id").and_then(Value::as_str) == plugin_id)
                })
                .and_then(|p| p.pointer("/extension/origin"))
                .and_then(Value::as_str)
                == Some("userInstalled");
            skills_actions::install_local_plugin(call, result, update)
        }

        Preview::VmmProject => memory_actions::native_project_confirmation(call, result),
        Preview::VmmRemoval => memory_actions::native_removal_recycle(result),
        Preview::VmmRetention => memory_actions::native_retention_run(result),
        Preview::VmmVector => memory_actions::native_vector_rebuild(result),
        Preview::VmmVectorAdoption => memory_actions::native_vector_adoption(result),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Verifies fresh ids look like uuids with synthetic data and assertions.
    /// 使用合成数据及断言验证 fresh_ids_look_like_uuids。
    fn fresh_ids_look_like_uuids() {
        // Exact entity identity.
        // 准确实体身份。
        let id = fresh_command_id();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        assert_ne!(id, fresh_command_id());
    }

    #[test]
    /// Verifies scrubs credentials but keeps ids with synthetic data and assertions.
    /// 使用合成数据及断言验证 scrubs_credentials_but_keeps_ids。
    fn scrubs_credentials_but_keeps_ids() {
        // Value.
        // 当前协议值。
        let mut value = serde_json::json!({
            "grpcApiKey": "k",
            "connections": [{ "api_key": "k", "credential_id": "c1", "has_management_key": true }],
            "entries": [{ "key": "docs/a.md" }],
            "repositoryKey": "repo",
            "serviceToken": "t",
            "maxTokens": 8,
        });
        scrub_secrets(&mut value);
        assert_eq!(value["grpcApiKey"], "");
        assert_eq!(value["connections"][0]["api_key"], "");
        assert_eq!(value["connections"][0]["credential_id"], "c1");
        assert_eq!(value["connections"][0]["has_management_key"], true);
        assert_eq!(value["entries"][0]["key"], "docs/a.md");
        assert_eq!(value["repositoryKey"], "repo");
        assert_eq!(value["serviceToken"], "");
        assert_eq!(value["maxTokens"], 8);
    }

    /// The service placeholder for a stored secret survives scrubbing so an edit keeps that secret.
    /// 已存储机密的服务占位符在清理后保留，使编辑能保留该机密。
    #[test]
    fn scrubbing_keeps_the_stored_secret_placeholder() {
        let mut value = serde_json::json!({
            "bearerToken": vulcan_service_api::WEBUI_REDACTED_SECRET_PLACEHOLDER,
            "oauthClientSecret": "raw-secret",
        });
        scrub_secrets(&mut value);
        assert_eq!(
            value["bearerToken"],
            vulcan_service_api::WEBUI_REDACTED_SECRET_PLACEHOLDER
        );
        assert_eq!(value["oauthClientSecret"], "");
    }
}
