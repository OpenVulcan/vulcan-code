# WebUI 服务适配说明

本文记录 TUI（`apps/vulcan-code-tui`）与核心服务的对接矩阵。协议权威来源为 WebUI 的
`apps/vulcan-code-web/src/service/*-service.ts`、对应 `types/*.ts` 及服务实现；TUI 一对一镜像这些
函数、方法与参数，不另造同功能、不同名的接口。
同步触发点：WebUI 服务函数、RPC 方法或参数形状变化时，回读上述源码并同步本文与 `service_api`。

## 调用与状态边界

`app/pages/*` 将权威 JSON 槽位转换成声明式行，经
`UiAction → action::perform → Command::Call(ServiceCall)` 调用后端。
后端通过 `BackendEvent::Slot`、`SlotFailed` 或 `Preview` 回传。真实服务与模拟服务共用这一边界。

| 调用种类 | 行为 |
| --- | --- |
| `Load(Slot)` | 保存该槽位的最近加载器；返回值替换槽位 |
| `Mutate(&[Slot])` | 成功后按最近加载器重读已登记的受影响槽位；吸收工作台快照 |
| `Then(Box<ServiceCall>)` | 对应 WebUI 中顺序等待两个请求的函数：首个写入成功后发送后续写入；后续写入失败时仍重读其槽位 |
| `Preview(Preview)` | 只读预检，展示完整影响、目标及决定，再构建真正的确认请求 |

设置编辑使用 `Setter::Dto` 保留完整 DTO 的同级字段；协议定义的部分更新使用专用构建器。
小数必须有限且在协议范围内；字符串列表只修改已确认的数组指针。
不存在独立的系统快照、虚构系统命令或系统设置草稿。

通用读取按连接代际及槽位序号过滤。应用层另按知识库、VCE 项目、VMM 项目、画像目标、搜索词、
文档目录与偏移、文档身份及语料检查归属。失败同样检查归属，并持续显示脱敏原因直到匹配重试或结果。
变更后的重读失败会清空旧值并保留加载器，页面重新加载可重试。
工作台快照读取失败或格式错误时同样清空可编辑槽位，并禁止其他局部增量重新发布旧设置；只有完整权威快照恢复后才重新发布。
完整文档写入串行等待权威刷新；重连会重送当前页面加载器。

侧栏会话命令（重命名、归档等）是类型化 `Command`，由工作线程直接调用对应 RPC。归档返回存储错误码
`unsupported_development_format`（会话以旧格式保存，无法加载或归档）时，工作线程发出
`BackendEvent::UnsupportedConversation`，应用层弹出共享键 `workbenchConversation.unsupportedDelete.*` 的危险确认框；
确认后发送 `Command::DeleteUnsupportedConversation`，对应 WebUI `deleteUnsupportedConversation` 与
`service_delete_unsupported_conversation`（见[会话元数据版本](session-metadata-migrations.md)）。

追踪设置继续使用 `service_save_provider_request_trace_settings`：
`enabled` 控制完整记录，可选 `errorsEnabled` 控制会话错误独立记录，省略或空值保留原设置。
工作台对应字段是 `providerRequestTraceEnabled` 与 `providerErrorTraceEnabled`。
错误开关从下一次会话运行生效，覆盖普通会话请求与远端上下文压缩；成功请求不保存正文，
失败请求和已收到的原始响应进入会话 `temp/provider-api-trace/errors/` 独立轮转。
HTTP 错误、网络及协议解析失败、工具参数解析失败和宿主响应投影失败均属于错误；
没有其他失败证据的手动停止不进入错误目录。打开和清理动作仍作用于整个追踪根目录。
记忆与索引后台推理继续沿用各自现有诊断策略。

## 页面范围

会话回答模式通过 `service_api/answer_mode.rs` 镜像 WebUI 的 `service_set_session_answer_mode`；
权威状态从会话 hydration 与 durable 增量取得，草稿会话的选择随首次提交的 `sessionSettings.answerMode`
一并发送。完整语义见 [`answer-mode-and-components.md`](answer-mode-and-components.md)。
全局默认通过 `settings_actions::save_prompt_answer_mode` 在设置页保存，设置页从工作台槽位的
`defaultAnswerMode` 读取当前值；新草稿一次性复制该默认。

会话权限模式通过 `service_api/permission_mode.rs` 镜像 WebUI 的 `service_set_session_permission_mode`；
权威状态从会话 hydration 的 `session_config.permission_mode` 取得，只在服务回执确认后改变显示。草稿会话的
显式选择随首次提交的 `sessionSettings.permissionMode` 一并发送，未选择时不发送值、保留会话默认。
选择器中的无人值守入口镜像 WebUI 的 `setSessionUnattended` 与 `loadSessionUnattendedDefault`：权威状态取自
hydration 的 `session_config.unattended`，草稿不可用；关闭时先读取分层预填值再打开文本输入框，确认后才发送，
开启时直接编辑当前策略或关闭，显示只在服务回执确认后改变。
选择器中的权限判定入口读取工作台的 `permissionJudgeState`：未设定时提示需要设定，启用时显示状态，
关闭时隐藏；入口只跳转到权限判定页面，不改写判定设置。

| 页面 | 已适配交互 | 状态来源 |
| --- | --- | --- |
| 外观 | TUI 主题、侧栏、思考浮层；语言同步到服务 | 本地状态、Workbench |
| 上下文 | 五档压缩窗口、新会话默认档位、用量刷新、输出风格、思考展示、工具折叠；基础系统提示词按用户或项目作用域编辑与恢复（作用域取自已加载快照的 `projectId`，未修改的正文只重读不写入） | PublicAiSettings、OutputStyleCatalog、SystemPrompt、Workbench |
| Git 提交设置 | 分支前缀、补充提示、摘要模型，完整配置保存 | Workbench、PublicAiSettings |
| 权限判定 | 开关、后端、设备与放行开关以完整保存请求立即保存；云端判定路由（计费测试先确认）；LLM 复核路由与思考档位（换模型时仅保留新模型声明的档位）；本机模型状态按状态中服务发布的 `pollIntervalMs` 重读；1 条与 10 条测速（云端先确认，结果只留本页）；模型登记、切换、回退、移除；CUDA 文件夹锁定、一键安装与官方链接，安装结束后重读设置；判定记录开关随完整保存请求保存，记录按判定结果、工作区与只看已标记筛选，翻页以记录 ID 为游标并可回到最新，有判定轨迹的记录可标记或取消标记，清除先确认。样本导出（`exportPermissionJudgeRecords`）结果是浏览器下载文件，只在 WebUI 提供 | PermissionJudgeSettings、PermissionJudgeStatus、PermissionJudgeBenchmark、PermissionJudgeRecords、PublicAiSettings |
| 初始化引导 | 第一步权限判定：本机（设备、模型就地切换与登记，需使用中模型才能确认）、云端（复用云端判定路由区块与提供方入口，需已保存路由才能确认）、暂不启用；只有确认按钮写入设置，离开页面不写入；登记后不自动切换，由用户在列表中切换 | PermissionJudgeSettings、PermissionJudgeStatus、PublicAiSettings |
| 权限策略 | 仅在判定启用时列出；按项目选择显示使用中判定模型的类别，每个类别可选默认或模型声明的档位，修改立即保存完整选择映射；保留的旧类别选择只注明不编辑 | ProjectPermissionJudgePolicy |
| 无人值守默认值 | 系统默认拒绝策略编辑；按项目选择（与权限策略页共用项目选择）切换“使用项目策略”，开启时以系统文本为起点并可编辑，关闭时显示所跟随的系统文本 | UnattendedSystemDefault、ProjectUnattendedDefault |
| 提供方 | 服务条目及凭据配置查看 | PublicAiSettings |
| 凭据 | 添加、启用、同步、设默认、移除；默认移除时原子选择同提供方替代连接 | PublicAiSettings |
| 模型 | 镜像 WebUI 模型清单：供应商分组与优先级、仅列有可用账号的模型、搜索及用途/输出筛选；每个模型的对话使用、多模态理解、内部使用与签名扩展开关；接入路由与账号详情；失效准入关闭及受阻路由诊断 | PublicAiSettings |
| AI 能力 | 选择服务公开的能力目标、启用能力；供应商异步任务列表与取消，仍有未终结任务时每 5 秒重读 | PublicAiSettings、ProviderTasks |
| MCP | stdio/http/sse 配置表单（含 OAuth 开关）、移除、运行态与能力检查；启用 OAuth 或等待授权的服务器可登录、打开授权页、手动完成、取消与退出，流程进行中每 2 秒重读 | Workbench.mcpServers、McpRuntime、McpCapabilities、McpOAuth |
| 已归档会话 | 恢复与删除 | Workbench.archivedConversations |
| 诊断 | 重试配置、完整请求追踪和会话错误独立记录开关、打开及清理追踪目录、模块与日志 | Workbench |
| 关于 | 应用版本、许可证目录与按哈希读取正文 | Workbench、Licenses、LicenseBody |
| 代理技能 | 真实分类导航与生效数量、状态/来源/覆盖/跨语言搜索筛选、查看正文、启用、刷新、用户目录、删除用户覆盖与还原默认 | Workbench.agentSkills.categories/records/errors |
| 子代理定义 | 真实分类导航与数量、状态及跨语言搜索筛选、查看定义、启用、刷新、用户目录、删除用户覆盖与还原默认 | Workbench.subagentCatalog.categories/definitions |
| LuaSkills | GitHub 安装、更新、卸载、启用、显式/隐式/禁用工具 | Workbench.luaSkills |
| LuaSkills 设置 | 完整运行时配置、私有地址允许列表、运行时重试 | Workbench.luaSkills.runtimeSettings |
| Vulcan 插件 | 检查包、审查权限及指纹、输入安装密钥、安装/更新、启用、卸载 | Workbench.plugins、插件预检 |
| 插件设置页 | 设置版面标签、字段整份配置写入、凭据状态/设置/删除、操作、数据区块参数与刷新、包内文档阅读 | PluginUi、PluginConfig、PluginCredential(n)、PluginData(n)、PluginAsset |
| VCE 项目索引 | 项目选择、概览、十一项设置、忽略模式、初始化/同步、任务读取/取消、维护计划及执行 | VceStatus、VceOverview、VceJob |
| VCE 引擎设置 | 背景推理路由、模型能力过滤、明确向量维度、路由预检及逐项目兼容/重建 | PublicAiSettings、VceStatus |
| 知识库列表 | 创建、重命名、描述修改、删除 | KnowledgeLibraries |
| 单库概览 | 按库名导航、文档/切片数量、检索策略、可读完整性及任务进度、快捷操作 | KnowledgeLibraries、KnowledgeIntegrity、KnowledgeJob |
| 文档列表 | 目录筛选、根目录与全部目录区分、每页 20 条及哨兵分页、打开准确文档 | KnowledgeFolders、KnowledgeDocuments |
| 文档阅读 | 原始/展示语料读取、保留缩进与空行、滚动、返回列表、持久错误及重试 | KnowledgeDocument |
| 知识检索 | 保留及编辑查询、立即清空旧命中、受管理知识库检索、引用路径/行号/正文、打开命中文档、失败与警告 | KnowledgeSearch |
| 单库设置 | 名称/描述、检索模式、回退策略、整理/向量路由及维度/批量 | KnowledgeLibraries、PublicAiSettings |
| 单库来源 | 导入目录、重新扫描、更换目录且保留来源身份、任务进度 | KnowledgeSources、KnowledgeJob |
| 项目授权 | 授权存在状态、已授权知识库默认使用状态 | KnowledgeBindings |
| 索引维护 | 可读完整性、修复/重建、展示预检与确认构建、向量构建、活动任务取消及自动进度读取 | KnowledgeIntegrity、KnowledgeJob、KnowledgeLibraries |
| VMM 概览 | 原生项目目录与宿主绑定 | VmmProjects |
| VMM 内容 | 项目选择、会话/轮次/记忆全文、按会话或轮次预检回收 | VmmProjects、VmmRecords |
| VMM 画像 | 用户/项目目标选择、全部有效状态及来源查看 | VmmProjects、VmmProfile |
| VMM 记忆项目 | 创建绑定、重命名、清空、受保护删除、迁移及碰撞改名确认 | VmmProjects、VMM 项目预检 |
| VMM 内部维护 | 向量预检重建、词法重建、保留预检执行、回收恢复/清除 | VmmVector、VmmLexical、VmmRetention、VmmRecycleBin |
| VMM 规则 | 全局召回/提炼配置组 | VmmRuntimePolicy.recall |
| VMM 内部推理 | 按能力筛选的背景 LLM、向量、重排路由及明确确认的测试入口 | PublicAiSettings |
| VMM 原生资源 | 组合设置、目录、正文读取、用途与修改两次确认、正文覆盖及重置 | VmmResources、VmmResourceFile |
| VMM 高级设置 | 排序、保留、文本清洗、用户/项目画像有效期，保留配置同级字段 | VmmRuntimePolicy |

插件详情挂载中 `kind: settings` 版面由 TUI 渲染：`load_plugin_ui_module` 到达后才按版面内容发出配置、凭据
状态与当前标签的数据读取，其余标签的数据在切换时按需读取；凭据与数据区块按文档顺序占用位置槽位。每次
`invoke_plugin_action` 携带进程级 `clientId` 与一次性 `invocationId`，重新加载或刷新时以 `ServiceCall::renewed`
更新调用标识；Action 结果只在插件与运行时代际（`runtimeIdentity`）都匹配当前挂载时采用。`kind: module` 的
ESM 模块只作元数据展示，TUI 不执行。
凭据页可以导入服务声明的令牌或已取得的授权凭据，尚未提供浏览器 OAuth 授权流程。
系统原生记忆按会话/轮次回收，不增加逐条原生记忆删除入口。
浏览器本地通知设置、会话置顶、手动标记未读、子代理消息、MCP 重启及 VMM 服务启停没有 TUI 虚构入口。
工作台的项目置顶及服务返回的会话未读状态仍按真实协议处理。

## 请求构建器与 RPC

下面的 WebUI 函数名从只读上游服务源文件匹配实际方法取得。
Rust 名称列同时标明内部确认构建器，避免将预检确认辅助函数当成新增服务方法。

| TUI 构建器 | WebUI 函数 | RPC 方法或 VMM 动作 |
| --- | --- | --- |
| `knowledge_base::list_knowledge_libraries` | listKnowledgeLibraries | `service_list_knowledge_libraries` |
| `knowledge_base::create_knowledge_library` | createKnowledgeLibrary | `service_create_knowledge_library` |
| `knowledge_base::update_knowledge_library_settings` | updateKnowledgeLibrarySettings | `service_set_knowledge_library_settings` |
| `knowledge_base::rename_knowledge_library` | renameKnowledgeLibrary | `service_rename_knowledge_library` |
| `knowledge_base::delete_knowledge_library` | deleteKnowledgeLibrary | `service_delete_knowledge_library` |
| `knowledge_base::list_knowledge_sources` | listKnowledgeSources | `service_list_knowledge_sources` |
| `knowledge_base::list_knowledge_project_bindings` | listKnowledgeProjectBindings | `service_list_knowledge_project_bindings` |
| `knowledge_base::get_knowledge_import_job` | getKnowledgeImportJob | `service_get_knowledge_import_job` |
| `knowledge_base::set_knowledge_project_binding` | setKnowledgeProjectBinding | `service_set_knowledge_project_binding` |
| `knowledge_base::preview_knowledge_import` | previewKnowledgeImport | `service_preview_knowledge_import` |
| `knowledge_base::import_knowledge_directory` | importKnowledgeDirectory | `service_import_knowledge_directory` |
| `knowledge_base::cancel_knowledge_import` | cancelKnowledgeImport | `service_cancel_knowledge_import` |
| `knowledge_base::rebuild_knowledge_index` | rebuildKnowledgeIndex | `service_rebuild_knowledge_index` |
| `knowledge_base::repair_knowledge_index` | repairKnowledgeIndex | `service_repair_knowledge_index` |
| `knowledge_base::get_knowledge_index_integrity` | getKnowledgeIndexIntegrity | `service_get_knowledge_index_integrity` |
| `knowledge_base::search_managed_knowledge_library` | searchManagedKnowledgeLibrary | `service_search_knowledge_library` |
| `knowledge_base::list_managed_knowledge_documents` | listManagedKnowledgeDocuments | `service_list_knowledge_library_documents` |
| `knowledge_base::list_managed_knowledge_document_folders` | listManagedKnowledgeDocumentFolders | `service_list_knowledge_library_folders` |
| `knowledge_base::read_managed_knowledge_document` | readManagedKnowledgeDocument | `service_read_knowledge_library_document` |
| `knowledge_base::preview_knowledge_display` | previewKnowledgeDisplay | `service_preview_knowledge_display` |
| `knowledge_base::build_knowledge_display_index` | buildKnowledgeDisplayIndex | `service_build_knowledge_display_index` |
| `knowledge_base::build_knowledge_vector_index` | buildKnowledgeVectorIndex | `service_build_knowledge_vector_index` |
| `licenses::load_license_catalog` | loadLicenseCatalog | `service_license_catalog` |
| `licenses::load_license_body` | loadLicenseBody | `service_license_body` |
| `mcp_settings::load_mcp_runtime_snapshot` | loadMcpRuntimeSnapshot | `service_mcp_runtime_snapshot` |
| `mcp_settings::load_mcp_server_capabilities` | loadMcpServerCapabilities | `service_mcp_server_capabilities` |
| `mcp_settings::load_mcp_oauth_status` | loadMcpOAuthStatus | `service_mcp_oauth_status` |
| `mcp_settings::start_mcp_oauth` | startMcpOAuth | `service_start_mcp_oauth` |
| `mcp_settings::finish_mcp_oauth` | finishMcpOAuth | `service_finish_mcp_oauth` |
| `mcp_settings::cancel_mcp_oauth` | cancelMcpOAuth | `service_cancel_mcp_oauth` |
| `mcp_settings::remove_mcp_oauth` | removeMcpOAuth | `service_remove_mcp_oauth` |
| `memory_actions::native_project_catalog` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_project_catalog` |
| `memory_actions::native_memory_records` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_memory_records` |
| `memory_actions::native_profile_records` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_profile_records` |
| `memory_actions::native_resource_catalog` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_resource_catalog` |
| `memory_actions::native_runtime_policy` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_runtime_policy` |
| `memory_actions::native_vector_status` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_vector_status` |
| `memory_actions::native_lexical_status` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_lexical_status` |
| `memory_actions::native_retention_status` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_retention_status` |
| `memory_actions::native_recycle_bin` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_recycle_bin` |
| `memory_actions::native_project_create_and_bind` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_project_create_and_bind` |
| `memory_actions::native_project_rename` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_project_rename` |
| `memory_actions::native_project_clear_preview` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_project_clear_preview` |
| `memory_actions::native_project_delete_preview` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_project_delete_preview` |
| `memory_actions::native_project_migration_preview` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_project_migration_preview` |
| `memory_actions::native_project_confirmation` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_project_migration_preview`, `native_project_migrate`, `native_project_clear_preview`, `native_project_clear`, `native_project_delete_preview`, `native_project_delete` |
| `memory_actions::native_removal_preview` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_removal_preview` |
| `memory_actions::native_removal_recycle` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_removal_recycle` |
| `memory_actions::native_resource_reset` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_resource_reset` |
| `memory_actions::native_resource_read` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_resource_read` |
| `memory_actions::native_resource_settings_update` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_resource_settings_update` |
| `memory_actions::native_resource_write` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_resource_write` |
| `memory_actions::native_runtime_policy_update` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_runtime_policy_update` |
| `memory_actions::native_retrieval_policy_update` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_retrieval_policy_update` |
| `memory_actions::native_retention_settings_update` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_retention_settings_update` |
| `memory_actions::native_text_policy_update` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_text_policy_update` |
| `memory_actions::native_profile_policy_update` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_profile_policy_update` |
| `memory_actions::native_vector_rebuild_preview` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_vector_rebuild_preview` |
| `memory_actions::native_vector_rebuild` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_vector_rebuild` |
| `memory_actions::native_lexical_rebuild` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_lexical_rebuild` |
| `memory_actions::native_retention_preview` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_retention_preview` |
| `memory_actions::native_retention_run` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_retention_run` |
| `memory_actions::native_recycle_restore` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_recycle_restore` |
| `memory_actions::native_recycle_purge` | runVmmMemoryAction | `service_run_vmm_memory_action` · `native_recycle_purge` |
| `project_actions::restore_archived_conversations` | restoreArchivedConversations | `service_restore_archived_conversations` |
| `project_actions::delete_archived_conversations` | deleteArchivedConversations | `service_delete_archived_conversations` |
| `project_actions::open_external_url` | openExternalUrl | `service_open_external_url` |
| `settings_actions::load_output_style_catalog` | loadOutputStyleCatalog | `service_output_style_catalog` |
| `settings_actions::save_prompt_output_style` | savePromptOutputStyle | `service_save_prompt_output_style` |
| `settings_actions::save_prompt_answer_mode` | savePromptAnswerMode | `service_save_prompt_answer_mode` |
| `permission_mode::set_session_permission_mode` | setSessionPermissionMode | `service_set_session_permission_mode` |
| `permission_mode::set_session_unattended` | setSessionUnattended | `service_set_session_unattended` |
| `permission_mode::load_session_unattended_default` | loadSessionUnattendedDefault | `service_session_unattended_default` |
| `settings_actions::load_system_prompt_settings` | loadSystemPromptSettings | `service_system_prompt_settings` |
| `settings_actions::save_system_prompt` | saveSystemPrompt | `service_save_system_prompt` |
| `settings_actions::reset_system_prompt` | resetSystemPrompt | `service_reset_system_prompt` |
| `settings_actions::load_public_ai_settings` | loadPublicAiSettings | `service_public_ai_settings_snapshot` |
| `settings_actions::add_public_ai_connection` | addPublicAiConnection | `service_add_public_ai_connection` |
| `settings_actions::save_public_ai_connection_enabled` | savePublicAiConnectionEnabled | `service_save_public_ai_connection_enabled` |
| `settings_actions::remove_public_ai_connection` | removePublicAiConnection | `service_remove_public_ai_connection` |
| `settings_actions::sync_public_ai_connection` | syncPublicAiConnection | `service_sync_public_ai_connection` |
| `settings_actions::save_public_ai_default_connection` | savePublicAiDefaultConnection | `service_save_public_ai_default_connection` |
| `settings_actions::save_public_ai_usage_refresh_settings` | savePublicAiUsageRefreshSettings | `service_save_public_ai_usage_refresh_settings` |
| `settings_actions::save_public_ai_context_compaction_window_nodes` | savePublicAiContextCompactionWindowNodes | `service_save_public_ai_context_compaction_window_nodes` |
| `settings_actions::save_public_ai_new_session_context_window_default` | savePublicAiNewSessionContextWindowDefault | `service_save_public_ai_new_session_context_window_default` |
| `settings_actions::save_public_ai_model_enabled` | savePublicAiModelEnabled | `service_save_public_ai_model_enabled` |
| `settings_actions::save_public_ai_internal_model_enabled` | savePublicAiInternalModelEnabled | `service_save_public_ai_internal_model_enabled` |
| `settings_actions::save_public_ai_multimodal_understanding_candidate` | savePublicAiMultimodalUnderstandingCandidate | `service_save_public_ai_multimodal_understanding_candidate` |
| `settings_actions::save_public_ai_provider_extension_target_set` | savePublicAiProviderExtensionTargetSet | `service_save_public_ai_provider_extension_target_set` |
| `settings_actions::save_public_ai_inference_route` | savePublicAiInferenceRoute | `service_save_public_ai_inference_route` |
| `settings_actions::test_public_ai_inference_route` | testPublicAiInferenceRoute | `service_test_public_ai_inference_route` |
| `permission_judge::load_permission_judge_settings` | loadPermissionJudgeSettings | `service_permission_judge_settings` |
| `permission_judge::load_permission_judge_status` | loadPermissionJudgeStatus | `service_permission_judge_status` |
| `permission_judge::save_permission_judge_settings` | savePermissionJudgeSettings | `service_save_permission_judge_settings` |
| `permission_judge::inspect_permission_judge_package` | inspectPermissionJudgePackage | `service_inspect_permission_judge_package` |
| `permission_judge::register_permission_judge_model` | registerPermissionJudgeModel | `service_register_permission_judge_model` |
| `permission_judge::activate_permission_judge_model` | activatePermissionJudgeModel | `service_activate_permission_judge_model` |
| `permission_judge::rollback_permission_judge_model` | rollbackPermissionJudgeModel | `service_rollback_permission_judge_model` |
| `permission_judge::remove_permission_judge_model` | removePermissionJudgeModel | `service_remove_permission_judge_model` |
| `permission_judge::reload_permission_judge` | reloadPermissionJudge | `service_reload_permission_judge` |
| `permission_judge::benchmark_permission_judge` | benchmarkPermissionJudge | `service_benchmark_permission_judge` |
| `permission_judge::install_permission_judge_cuda` | installPermissionJudgeCuda | `service_install_permission_judge_cuda` |
| `permission_judge::load_permission_judge_records` | loadPermissionJudgeRecords | `service_permission_judge_records` |
| `permission_judge::mark_permission_judge_record` | markPermissionJudgeRecord | `service_mark_permission_judge_record` |
| `permission_judge::clear_permission_judge_records` | clearPermissionJudgeRecords | `service_clear_permission_judge_records` |
| `permission_judge::load_project_permission_judge_policy` | loadProjectPermissionJudgePolicy | `service_project_permission_judge_policy` |
| `permission_judge::save_project_permission_judge_policy` | saveProjectPermissionJudgePolicy | `service_save_project_permission_judge_policy` |
| `permission_judge::load_unattended_system_default` | loadUnattendedSystemDefault | `service_unattended_system_default` |
| `permission_judge::save_unattended_system_default` | saveUnattendedSystemDefault | `service_save_unattended_system_default` |
| `permission_judge::load_project_unattended_default` | loadProjectUnattendedDefault | `service_project_unattended_default` |
| `permission_judge::save_project_unattended_default` | saveProjectUnattendedDefault | `service_save_project_unattended_default` |
| `settings_actions::save_public_ai_provider_tool_selection` | savePublicAiProviderToolSelection | `service_save_public_ai_provider_tool_selection` |
| `settings_actions::save_public_ai_provider_tool_capability_enabled` | savePublicAiProviderToolCapabilityEnabled | `service_save_public_ai_provider_tool_capability_enabled` |
| `settings_actions::list_provider_tasks` | listProviderTasks | `service_list_provider_tasks` |
| `settings_actions::cancel_provider_task` | cancelProviderTask | `service_cancel_provider_task` |
| `settings_actions::select_workbench_thinking_display_mode` | selectWorkbenchThinkingDisplayMode | `service_select_workbench_thinking_display_mode` |
| `settings_actions::save_tool_collapse_default` | saveToolCollapseDefault | `service_save_tool_collapse_default` |
| `settings_actions::select_webui_locale` | selectWebuiLocale | `service_select_webui_locale` |
| `settings_actions::save_git_commit_settings` | saveGitCommitSettings | `service_save_git_commit_settings` |
| `settings_actions::save_provider_retry_settings` | saveProviderRetrySettings | `service_save_provider_retry_settings` |
| `settings_actions::save_provider_request_trace_settings` | saveProviderRequestTraceSettings | `service_save_provider_request_trace_settings` |
| `settings_actions::open_selected_provider_api_trace_directory` | openSelectedProviderApiTraceDirectory | `service_open_selected_provider_api_trace_directory` |
| `settings_actions::clear_selected_provider_api_trace_directory` | clearSelectedProviderApiTraceDirectory | `service_clear_selected_provider_api_trace_directory` |
| `settings_actions::save_mcp_server_configuration` | saveMcpServerConfiguration | `service_save_mcp_server_configuration` |
| `settings_actions::remove_mcp_server_configuration` | removeMcpServerConfiguration | `service_remove_mcp_server_configuration` |
| `skills_actions::refresh_luaskills_catalog` | refreshLuaSkillsCatalog | `service_refresh_luaskills_catalog` |
| `skills_actions::retry_luaskills_managed_runtimes` | retryLuaSkillsManagedRuntimes | `service_retry_luaskills_managed_runtimes` |
| `skills_actions::save_luaskills_runtime_settings` | saveLuaSkillsRuntimeSettings | `service_save_luaskills_runtime_settings` |
| `skills_actions::install_luaskill` | installLuaSkill | `service_install_luaskill` |
| `skills_actions::update_luaskill` | updateLuaSkill | `service_update_luaskill` |
| `skills_actions::set_luaskill_enabled` | setLuaSkillEnabled | `service_set_luaskill_enabled` |
| `skills_actions::uninstall_luaskill` | uninstallLuaSkill | `service_uninstall_luaskill` |
| `skills_actions::set_luaskill_tool_registration_state` | setLuaSkillToolRegistrationState | `service_set_luaskill_tool_registration_state` |
| `skills_actions::set_vulcan_plugin_enabled` | setVulcanPluginEnabled | `service_set_vulcan_plugin_enabled` |
| `skills_actions::inspect_local_plugin` | inspectLocalPlugin | `service_inspect_local_plugin` |
| `skills_actions::install_local_plugin` | installLocalPlugin、updateLocalPlugin | `service_install_local_plugin`、`service_update_local_plugin` |
| `skills_actions::uninstall_user_plugin` | uninstallUserPlugin | `service_uninstall_user_plugin` |
| `skills_actions::load_plugin_ui_module` | loadPluginUiModule | `service_load_plugin_ui_module` |
| `skills_actions::read_plugin_text_asset` | readPluginTextAsset | `service_read_plugin_text_asset` |
| `skills_actions::invoke_plugin_action` | invokePluginAction | `service_invoke_plugin_action` |
| `skills_actions::refresh_agent_skills_catalog` | refreshAgentSkillsCatalog | `service_refresh_agent_skills_catalog` |
| `skills_actions::open_agent_skills_user_directory` | openAgentSkillsUserDirectory | `service_open_agent_skills_user_directory` |
| `skills_actions::set_agent_skill_enabled` | setAgentSkillEnabled | `service_set_agent_skill_enabled` |
| `skills_actions::delete_agent_skill` | deleteAgentSkill | `service_delete_agent_skill` |
| `skills_actions::restore_agent_skill_default` | restoreAgentSkillDefault | `service_restore_agent_skill_default` |
| `skills_actions::refresh_subagent_catalog` | refreshSubagentCatalog | `service_refresh_subagent_catalog` |
| `skills_actions::open_subagent_user_directory` | openSubagentUserDirectory | `service_open_subagent_user_directory` |
| `skills_actions::set_subagent_definition_enabled` | setSubagentDefinitionEnabled | `service_set_subagent_definition_enabled` |
| `skills_actions::delete_subagent_definition` | deleteSubagentDefinition | `service_delete_subagent_definition` |
| `skills_actions::restore_subagent_definition_default` | restoreSubagentDefinitionDefault | `service_restore_subagent_definition_default` |
| `vce::preflight_vce_embedding_route` | preflightVceEmbeddingRoute | `service_preflight_vce_embedding_route` |
| `vce::save_reviewed_embedding` | savePublicAiInferenceRoute | `service_save_public_ai_inference_route` |
| `vce::load_vce_job` | loadVceJob | `service_get_vce_job` |
| `vce::load_vce_configuration_status` | loadVceConfigurationStatus | `service_get_vce_configuration_status` |
| `vce::load_vce_project_overview` | loadVceProjectOverview | `service_get_vce_project_overview` |
| `vce::update_vce_project_settings` | updateVceProjectSettings | `service_update_vce_project_settings` |
| `vce::submit_vce_job` | submitVceJob | `service_submit_vce_job` |
| `vce::cancel_vce_job` | cancelVceJob | `service_cancel_vce_job` |
| `vce::plan_vce_maintenance` | planVceMaintenance | `service_plan_vce_maintenance` |
| `vce::execute_vce_maintenance` | executeVceMaintenance | `service_execute_vce_maintenance` |
| `workbench_snapshot::load_workbench_snapshot` | loadWorkbenchSnapshot | `workbench.snapshot` |


工作台聊天、项目和 Git 命令保留类型化变体，其 RPC 函数同样来自 WebUI：
选择/读取会话、提交/停止轮次、回复/拒绝提问、模型/推理强度选择、项目目录浏览及管理、
Git 目录/仓库监听（`workspace.git.setMonitoring`）/差异/分支/提交、目标控制和子代理停止/关闭。
这些操作需要后端管理本地新会话草稿、选中会话、事件窗口或 Git 文件标识，因此不改成独立的系统设置 DTO。
本地附件选择只读本机文件，随后使用 `service_stage_composer_attachment_bytes` 暂存真实字节。

## 预检保护

| 操作 | 确认请求冻结的依据 |
| --- | --- |
| 知识目录导入 | libraryId/sourceRoot/sourceId、previewFingerprint；selectedFileKeys 只能为本次 ready 集合的不重复子集 |
| 插件安装/更新 | sourcePath、contentFingerprint、全部批准权限、注册表依赖同意；初始密钥按 serverId/secretKey 逐项遮蔽输入 |
| VCE 维护 | settingsRevision、checkpointVersion、embeddingFingerprint、完整 actionIds、mode、幂等键；无检查点仅允许 rebuild |
| VCE 路由 | preflightRevision、准确候选路由与正维度、每个独立项目的 compatible/rebuild 决定；hardRebuildReason 强制 rebuild |
| VMM 清空/删除/迁移 | 复用预检 commandId、storageVersion、准确项目身份；受保护项目不可删，绑定解除及迁移碰撞改名显式确认，迁移还保留 previewFingerprint |
| VMM 会话/轮次回收及保留执行 | 服务签发的单次 previewId、confirmed；不重新发送可替换的目标清单 |
| VMM 向量重建 | expectedStorageVersion、expectedPreviewFingerprint、confirmed |
| VMM 词法重建/回收操作 | 当前快照 storageVersion 与准确批次身份；派生词法操作不冒充源内容版本变更 |
| VMM 资源正文 | 准确目录 path、完整原文、confirmedPurpose 与 confirmedModification；取消时不发送 |

后端始终保留最终准入权。TUI 不在预检过期时换用当前版本，用户需要重新预检。

## 机密与模拟服务

发现文件只由程序读取，传输令牌仅用于请求头及套接字认证。
递归清理明确的密钥/令牌/密码字段和所有 MCP headers/env、initialSecrets 的值。
服务以 `********` 占位符投影的已存储机密原样保留；表单中机密字段留空即回传占位符，由服务还原原值。
文件 key、repositoryKey、安装 secretKey 与 headersFromEnv 的变量名称引用保留。
ServiceCall、Setter、Form、输入框及发现端点的 Debug 输出省略机密；
错误文本移除已知输入密钥、JSON 转义形式及服务令牌。

模拟服务只加载仓库内合成夹具，按相同方法分发。已建模写入修改权威 JSON：
连接仅保留公开元数据，知识库创建不依赖已有条目，VCE 按项目保留修订和幂等任务，
VMM 按项目独立保存记录，预检核对版本与目标，回收恢复只合并已移动记录并检查身份冲突。
向量、词法及保留计数随活动记录与回收批次刷新；演示夹具的向量保持当前代次，计数不代表索引完整性。
未知演示变更按交接约定回成功；这类回执不作为真实功能或端到端验收证据。

## 验证范围

验证命令与范围见 [TUI 结构说明](tui-design.md#验证)。
测试覆盖同级字段保护、数值边界、预检指纹/版本/目标、密钥回显清理与单次发送、
HTTP 认证信封、WebSocket 帧、乱序读取、重连代际、工作器关闭、页面归属过滤、
模拟状态修改及回收恢复、分类与生效版本筛选、目录分页、正文与语料读取、持久错误重试及无重叠任务轮询。
无头渲染在 160×45 和 64×20 下覆盖系统页面、工作台与预检浮层，并驱动分类和知识库的点击、按键及滚轮；
阅读页检查实际终端单元格中的正文缩进及读取失败消息。
服务表单、资源正文及数组设置保留多行粘贴；Alt/Shift+Enter 换行，普通 Enter 仍确认。
Ctrl+U 清空后可粘贴替换全文或文件键列表，输入框显示末尾六行；凭据粘贴保留原始字节并始终遮蔽。
