# Vulcan 插件首发契约与原生 MCP 组件实施计划

## 计划状态

- 当前状态：已完成
- 制定日期：2026-07-16
- 当日序号：01
- 工作阶段：插件体系首发底层定型与原生 MCP 组件实现
- 计划文件：`docs/plan/20260716-01-VULCAN_PLUGIN_MCP_INITIAL_RELEASE.md`
- 完成归档目标：`docs/completed/20260716/01-VULCAN_PLUGIN_MCP_INITIAL_RELEASE.md`
- 退出条件：全部实现与验证完成，并连续 5 轮完整代码审核未发现任何问题

## 一、任务背景

Vulcan 插件体系已经完成第一阶段通用内容组件建设，当前可以贡献 Lua 工具、Hook、Action、动态 WebUI、配置、Skill 与 Agent，也允许完全不包含 Lua Runtime 的内容型插件。现有实现仍保留了早期以 Lua 执行为中心形成的底层结构：插件公开类型大量使用 `SystemPlugin*` 命名，插件包在清单中自报 `kind`，依赖清单归属于 Lua `runtime`，Node、Python、Native 资源与 Lua 生命周期存在结构性耦合。

当前插件体系尚未作为公开稳定协议发布。仓库内只有 4 个版本为 `0.1.0` 的内建插件及相关测试夹具，因此本任务不建立临时格式兼容层，不增加 schema v2，不迁移未发布测试产物，而是直接将最终设计定为首个公开的 `schema_version: 1`。

Vulcan 已经具备完整宿主级 MCP 客户端能力，包括 stdio、Streamable HTTP、legacy SSE、OAuth、初始化、工具发现与调用、Prompt、Resource、列表变化通知、断线事件、工具策略和 RuntimeFoundation 投影。本任务不重写 MCP 协议，而是让插件目录成为现有 MCP Runtime 的受控配置来源，并把 MCP Server 纳入插件安装、更新、启停、卸载、generation、诊断和原子发布生命周期。

## 二、任务目标

1. 将当前未发布插件格式直接重构为最终首发 `schema_version: 1`，不保留旧结构解析、字段别名、迁移器或 deprecated 分支。
2. 从清单移除由插件自报的 `kind`，由宿主根据受信根、安装事务和目录来源确定插件 Origin。
3. 将公开领域类型、RPC、Service、WebUI 和诊断中的 `SystemPlugin` 语义统一重构为 `VulcanPlugin` 或用户可见“插件”。
4. 将依赖、Native、Node、Python 资源从 Lua Runtime 中解耦为通用执行资源。
5. 将 Lua bundle、Lua 生命周期、Lua Tool、Lua Hook、Lua Action 和 Lua Session 明确归入可选 `lua_runtime` 执行协议。
6. 在插件首发清单中增加独立 `mcp.servers` 组件，支持本地 stdio、Streamable HTTP 和 legacy SSE。
7. 本地 stdio MCP 只允许引用清单内受管 Node Entry、私有 npm package bin 或清单内 Native Binary，禁止任意命令、PATH 搜索和系统全局 Node/npm/npx。
8. 为 npm MCP 建立按插件 ID、版本、内容指纹隔离的私有依赖环境，并复用 Vulcan/LuaSkills 已有受管 Node 与 pnpm 发行版。
9. 安装、更新、启用前真实完成 MCP 初始化、工具发现和工具 Schema 校验，失败时不得发布半可用插件 generation。
10. 将用户 MCP 与每个插件 generation 拆分为独立 Runtime 分区，插件启停不得重启无关用户 MCP 或其他插件 MCP。
11. 建立插件目录快照、MCP Lease、Plugin UI、Skill、Agent、Hook、Lua Tool 与 RuntimeFoundation 的统一 Extension Generation。
12. 新执行原子使用新 generation，已开始执行继续使用其冻结的旧 generation，旧 MCP 进程在排空后关闭。
13. 插件停用、卸载后，新执行的 MCP 工具立即消失；旧执行只能在受控排空期限内使用旧 Lease。
14. 建立 MCP Server 状态、工具数量、Prompt/Resource 数量、重试状态、最近错误和 generation 的 Service/WebUI 诊断。
15. 补齐本地、远程、安全、失败回滚、更新接管、断线重连、列表变化、策略过滤和回归测试。
16. 实现完成后持续开展循环代码审核，对发现问题进行根因级长期修复；任何一轮发现问题都将连续零问题计数重置为 0，直到连续 5 轮完整审核零问题后才能结束任务。

## 三、非目标

1. 不支持 Claude、Codex、VS Code 或其他产品的插件清单格式兼容。
2. 不增加通用直接 Node 插件桥或直接 Python 插件桥。
3. 不实现 LSP、Theme、Monitor、Output Style 或独立 Commands 领域。
4. 不允许插件声明任意本地命令、Shell 字符串、系统 PATH 程序或系统全局包管理器。
5. 不为未发布的旧清单结构提供读取兼容、自动迁移或静默重写。
6. 不在插件清单中保存 bearer token、OAuth client secret、私有 Header 或其他敏感值。
7. 不把 Provider 特有工具 Schema 兼容逻辑下沉到插件清单或 MCP 原始 Schema。
8. 不在启动时静默改写用户持久化工具策略或清理开发期旧插件数据。

## 四、首发清单核心设计

### 4.1 首发版本规则

1. `SYSTEM_PLUGIN_MANIFEST_SCHEMA_VERSION` 替换为语义准确的 Vulcan 插件首发版本常量，值保持 `1`。
2. 解析器只接受最终首发结构。
3. 仓库内 4 个内建插件、测试夹具、示例和文档全部直接更新。
4. 开发环境残留旧插件出现格式错误时返回明确重新安装诊断，不进行候选字段兼容。

### 4.2 插件来源由宿主持有

1. 删除清单 `kind` 字段。
2. 插件来源由加载根确定：内建根为 `Bundled`，用户安装根为 `UserInstalled`。
3. 安装授权记录保存宿主判定的来源，不读取插件自报信任等级。
4. 用户插件不能覆盖同 ID 内建插件。
5. 用户 MCP Server 不能占用保留的插件 Server 命名空间。

### 4.3 最终清单领域分层

首发清单按以下领域组织：

1. 基础身份：`schema_version`、`id`、`version`、`display`、`default_enabled`、`platforms`。
2. 权限请求：`permissions`。
3. 包完整性：`required_files`。
4. 通用执行资源：`execution`。
5. 可选 Lua 协议：`lua_runtime`。
6. 可选 MCP 协议：`mcp`。
7. 内容与界面：`ui`、`config`、`prompt_contributions`、`locales`。
8. 约定目录组件：`skills/`、`agents/`。

### 4.4 通用执行资源

`execution` 统一持有：

1. `dependencies.manifest`：插件包内依赖描述文件。
2. `dependencies.install_mode`：`bundled` 或 `registry`。
3. `native_binaries`：按目标平台映射的受管 Native Binary。
4. `node_entries`：包内 Node 模块入口与工作目录。
5. `python_entries`：包内 Python 脚本入口与工作目录。

Node、Python、Native 入口本身不是独立插件运行协议，只能被 Lua Runtime 或 MCP stdio launcher 引用。

### 4.5 Lua Runtime 归位

`lua_runtime` 统一持有：

1. Lua bundle。
2. invoke、shutdown、hook shutdown 函数。
3. Session 并发上限与空闲超时。
4. Lua module allowlist。
5. Lua lifecycle callbacks。
6. Lua Tool、Hook 与 Action 声明。

没有 `lua_runtime` 时，清单不得声明任何 Lua 执行组件。Skill、Agent、MCP、配置和静态内容不依赖 Lua Runtime。

### 4.6 MCP Server 声明

`mcp.servers` 中每个 Server 必须包含稳定插件内 ID、用户可见名称和一个严格区分的 transport 定义。

支持的 transport：

1. `stdio`：本地子进程协议。
2. `http`：Streamable HTTP。
3. `sse`：仅用于兼容旧服务器的 legacy SSE。

stdio launcher 支持：

1. `node_entry`：引用 `execution.node_entries`。
2. `node_package_bin`：引用私有依赖环境中某个 npm package 的明确 bin 名。
3. `native_binary`：引用 `execution.native_binaries`。

不提供 `command` 字段，不允许 Shell，不允许 PATH 候选搜索。

### 4.7 配置值来源

远程 URL、Header、stdio 环境变量等动态值采用带类型的唯一来源：

1. 非敏感 literal。
2. 插件配置键引用。
3. 宿主 Secret Store 引用。
4. 明确环境变量引用。

每个值只能声明一种来源。禁止 `literal || config || env` 一类多路 fallback。

## 五、领域类型与命名重构

### 5.1 Rust 领域类型

公开类型统一使用 `VulcanPlugin*`：

1. Manifest、Origin、Availability、RuntimeState。
2. Descriptor、CatalogSnapshot、ToolDescriptor、HookDescriptor。
3. RuntimeManager、RuntimeError、LifecycleEvent。
4. PermissionPreview、InstallationResult、DrainReport。

内部过于通用且不会跨 crate 暴露的类型可以使用精确领域名，不保留 `SystemPlugin*` type alias。

### 5.2 Service 与 RPC

1. RPC method 从 `system_plugin` 语义迁移到 `vulcan_plugin`。
2. Service 函数、事件、状态字段和测试名称同步迁移。
3. 不保留旧 RPC 方法别名。
4. WebUI 调用方与桌面壳同步更新。

### 5.3 用户可见文案

1. 普通插件统一显示“插件”。
2. 来源单独显示“内建”或“用户安装”。
3. 不再使用“系统插件”泛指全部插件。
4. 模型可见 Tool description、schema 和参数说明继续使用英文。

## 六、MCP Runtime 分区架构

### 6.1 Source-neutral Server 定义

将 MCP Runtime 从只接受 `ProviderConfigurationFile` 重构为接受经过校验的 `McpServerConfig` 集合，并为每个 Server 保存来源：

1. `UserConfigured`。
2. `Plugin { plugin_id, generation }`。

Provider 配置映射与插件清单映射分别负责生成同一中立配置，不允许 MCP Runtime 反向依赖插件清单。

### 6.2 Runtime 分区

1. 用户 MCP 使用稳定用户分区。
2. 每个已启用插件 generation 使用独立插件分区。
3. 修改用户 MCP 只替换用户分区。
4. 启停插件只替换目标插件分区。
5. 聚合层统一生成诊断、工具视图、冲突检查和调用绑定。

### 6.3 Extension Generation

新增原子扩展代次，至少包含：

1. 不可变插件目录快照。
2. 插件 MCP 分区 Lease 映射。
3. Plugin Tool、Hook、Prompt、Skill、Agent 和 UI 投影。
4. 单调 generation/revision。

RuntimeFoundation 在一次准备中捕获同一个 `Arc<ExtensionGeneration>`，不能分别读取插件目录与 MCP 当前状态。

### 6.4 旧代次排空

1. 原子切换后，新执行只使用新 generation。
2. 已开始执行继续持有旧 generation。
3. 旧 MCP Lease 在引用归零后优雅关闭。
4. 设置最大排空期限，超过期限后取消旧调用并强制关闭。
5. 插件停用后，新执行不能再发现目标 MCP 工具。

## 七、本地 npm MCP 私有环境

### 7.1 运行时来源

1. Node 与 pnpm 只能通过现有受管运行时安装清单解析。
2. 禁止使用系统 Node、npm、npx 或 pnpm。
3. 运行时版本和 package manager 版本来自插件依赖描述文件。

### 7.2 环境隔离

目录按插件 ID、版本、内容指纹隔离，并保留不可变环境标记。共享缓存只允许保存下载内容，插件实际 `node_modules` 必须私有。

### 7.3 lockfile 与 registry

1. 强制 package.json 与 pnpm lockfile 位于同一私有包目录。
2. 强制 `pnpm install --frozen-lockfile`。
3. 直接依赖不得使用浮动版本。
4. Registry 由宿主批准的 registry alias 解析，插件不能提交任意 registry URL。
5. Registry 网络安装必须经过现有安装授权流程。
6. 默认禁止依赖 install lifecycle scripts；需要构建的产物必须随包交付。

### 7.4 Node package bin 解析

1. 从安装后 package.json 的 `bin` 字段解析明确 bin 名。
2. 规范化并验证实际 JavaScript 入口仍位于插件私有环境。
3. 使用受管 Node 直接执行实际入口。
4. 不执行 `.cmd`、Shell shim 或 PATH 命中结果。

## 八、MCP 验证与发布事务

### 8.1 安装与更新准备

1. 解析清单并完成静态验证。
2. 将包复制到 staging。
3. 重新计算内容指纹，关闭扫描到复制之间的篡改窗口。
4. 在 staging 对应私有环境中安装依赖。
5. 解析全部本地 MCP 启动入口。
6. 对本地 MCP 真实启动并完成初始化和工具发现。
7. 验证全部工具 Schema。
8. 关闭 staging 验证进程。
9. 验证全部成功后才发布包、环境和安装记录。

### 8.2 远程 MCP

1. 安装时完成 URL、transport、Header 来源、OAuth 与 Secret 引用的静态验证。
2. 安装允许网络暂时不可达，因为用户插件安装后保持禁用。
3. 启用时必须真实连接并完成初始化和工具发现。
4. 远程鉴权失败视为启用事务失败。

### 8.3 启用事务

1. 构建目标插件目录但不向新执行发布。
2. 准备依赖环境和 Lua 生命周期。
3. 启动候选插件 MCP 分区并等待所有 Server Ready。
4. 完成 MCP Schema 校验和 Hook 校验。
5. 持久化目标启用策略。
6. 原子发布 Extension Generation。
7. 候选 MCP 分区提升为正式 Lease。
8. 失败时关闭候选 Lease、补偿 Lua 生命周期并恢复旧投影。

### 8.4 停用与卸载事务

1. 先进入 Draining，拒绝新调用。
2. 构建不含目标插件 MCP 分区的新 generation。
3. 原子发布新 generation。
4. 排空旧 Lua Session 与旧 MCP Lease。
5. 执行停用或卸载回调。
6. 仅在完全排空后删除用户插件包、私有环境和非敏感状态。
7. Secret 清理由宿主 Secret Store 独立执行并记录失败诊断。

## 九、MCP 工具与 Schema 规则

1. Server ID 固定为 `<plugin-id>:<server-id>`。
2. 原始工具 ID 固定为 `<plugin-id>:<server-id>/<tool-name>`。
3. 公开工具名继续通过宿主统一正规化器生成。
4. 工具策略与 Agent 白名单使用稳定原始 ID，不依赖可能发生冲突消解的公开名称。
5. 同一 Server 内工具名不得重复。
6. `inputSchema` 必须是对象形 JSON Schema。
7. 使用 JSON Schema 2020-12 编译校验。
8. 禁止无法受控解析的外部 `$ref`。
9. `tools/list` 可以合法返回空列表。
10. `tools/list_changed` 必须先完整验证新列表，再原子替换当前 Server 工具集合。
11. 任一新工具非法时保留上一份完整成功快照并记录错误，不发布部分列表。
12. Provider 特有 Schema 正规化继续只在 Provider 兼容层执行。

## 十、敏感信息与权限模型

1. 插件清单和普通插件配置不得保存敏感值。
2. HTTP bearer token、OAuth client secret 和敏感 Header 使用宿主 Secret Store。
3. stdio 凭据在进程启动时从 Secret Store 注入环境变量。
4. 所有诊断、进程输出、Service payload 和 WebUI 状态必须脱敏。
5. Secret Key 使用插件 ID 与 Server ID 命名空间，插件之间不能互读。
6. 权限预览必须展示远程网络、本地进程、Secret 使用和 registry 安装请求。
7. 插件更新导致权限集合变化时必须重新授权。

## 十一、进程生命周期与诊断

### 11.1 状态机

MCP Server 状态至少包含：Disabled、Preparing、Connecting、Ready、Degraded、Failed、Draining、Stopped。

### 11.2 超时

首发采用宿主固定上限：

1. 启动与 initialize：15 秒。
2. `tools/list`：10 秒。
3. 远程连接：15 秒。
4. 优雅关闭：5 秒。
5. 强制终止等待：2 秒。
6. generation 排空：30 秒。

插件不得自行放大超时上限。

### 11.3 重启策略

1. 使用 1、2、4、8、16、30 秒上限的指数退避并加入抖动。
2. 连续 5 次启动或连接失败后进入 Failed。
3. 稳定运行 60 秒后清零连续失败计数。
4. Failed 状态需要用户重试、重新启用或插件 generation 更新后恢复。
5. 运行期单 Server 崩溃只隔离该 Server，不自动停用整个插件。

### 11.4 日志与详情

1. 保留最近 200 行或 64 KiB 脱敏日志。
2. 插件详情显示 Server ID、来源、transport、endpoint 摘要、状态、generation、重试次数、工具/Prompt/Resource 数量和最近错误。
3. 不显示 token、secret、完整敏感 Header 或敏感环境变量值。

## 十二、Service 与 WebUI 实施

1. 扩展插件目录描述符与 API DTO，增加 MCP Server 摘要。
2. Service Kernel 负责候选 generation 的准备、持久化、提交和补偿。
3. Workbench 增量事件必须携带一致 revision。
4. WebUI 插件详情增加 MCP Server 区域。
5. 启用、停用、安装、更新期间显示明确阶段和错误。
6. 插件 MCP 不出现在用户可编辑 MCP 设置列表中，避免绕过插件生命周期单独修改。
7. 插件 MCP 只允许在所属插件详情查看状态、重试和授权入口。
8. 用户 MCP 配置和插件 MCP 来源在诊断中明确区分。

## 十三、详细实施步骤

### 步骤一：建立首发 schema v1 最终模型

1. 重构 manifest Rust 类型和 serde 结构。
2. 删除 `kind`、旧 `runtime.dependencies_file` 和旧 `dependency_policy` 布局。
3. 增加 `execution`、`lua_runtime`、`mcp`。
4. 建立严格字段、ID、引用关系和权限校验。
5. 更新 4 个内建插件及清单单元测试。

### 步骤二：清理插件领域命名

1. 重命名 `SystemPlugin*` Rust 类型。
2. 重命名 Service、RPC、事件与测试。
3. 更新 WebUI API 调用和用户文案。
4. 删除所有旧 type alias 和 RPC alias。

### 步骤三：重构通用执行环境

1. 让依赖环境不再要求 Lua Runtime。
2. 让 Node、Python、Native 资源由通用 execution 描述符持有。
3. 保持内容型插件无需执行环境。
4. 回归现有 Lua 插件依赖安装、Session 和 Host Bridge。

### 步骤四：建立插件 MCP 描述符

1. 将 manifest MCP 声明解析为不可变描述符。
2. 完成 Server ID 命名空间化。
3. 验证 stdio launcher 引用和远程配置来源。
4. 将 MCP Server 纳入插件指纹、权限预览和详情诊断。

### 步骤五：重构 MCP Runtime 输入与分区

1. 将 Runtime 输入改为 source-neutral Server 集合。
2. 保留用户 Provider 配置映射适配器。
3. 增加插件 MCP 映射适配器。
4. 增加用户分区、插件分区和聚合访问面。
5. 保持现有调用、OAuth、Prompt、Resource 和策略能力。

### 步骤六：实现本地 stdio MCP 环境

1. 复用受管 Node/pnpm。
2. 实现 node_entry 启动解析。
3. 实现 node_package_bin 安全解析。
4. 实现 native_binary 启动解析。
5. 建立 staging 验证与进程关闭。

### 步骤七：实现远程 MCP 与 Secret 引用

1. 映射 Streamable HTTP 与 legacy SSE。
2. 接入配置、环境变量和 Secret Store 唯一来源。
3. 复用并补强现有 OAuth 会话。
4. 完成脱敏诊断。

### 步骤八：实现 Extension Generation

1. 建立包含插件目录和插件 MCP Lease 的不可变 generation。
2. RuntimeFoundation 改为一次捕获完整 generation。
3. 实现候选准备、原子发布、旧代次排空和超时回收。
4. 消除插件目录与 MCP Runtime 分别读取造成的混代窗口。

### 步骤九：接入插件生命周期事务

1. 安装与更新增加 staging MCP 验证。
2. 启用增加候选 Lease 准备和 Ready 门槛。
3. 停用和卸载增加分区撤销与排空。
4. 完成所有失败补偿和状态恢复。

### 步骤十：实现动态工具刷新与 Schema 校验

1. 建立共享 MCP 工具 Schema 验证器。
2. 增加完整列表原子替换。
3. 失败时保留上一份完整成功快照。
4. 验证 Provider 兼容层仍是唯一 Schema 正规化位置。

### 步骤十一：Service API 与 WebUI

1. 增加 MCP Server DTO 和插件详情投影。
2. 增加状态、日志、重试与授权入口。
3. 增加安装、更新、启停阶段进度。
4. 完成只读与来源隔离。

### 步骤十二：测试与全量回归

1. 完成单元、集成和端到端测试。
2. 运行 Rust fmt、check、测试和严格 Clippy。
3. 运行 Web lint、类型检查、测试和生产构建。
4. 对比 2026-07-15 基线并记录结果。

### 步骤十三：循环代码审核与长期修复

1. 每轮从完整 Git diff、架构不变量、错误处理、安全、并发、生命周期、测试和 UI 一致性重新审核。
2. 发现任一问题后立即把连续零问题轮数重置为 0。
3. 修复必须消除根因，同时补充能阻止回归的测试或结构守卫。
4. 修复后重新执行受影响验证，再开始下一轮独立审核。
5. 只有连续 5 轮完整审核均为零问题时才能通过审核门槛。

### 步骤十四：计划闭环与归档

1. 对照本计划逐项验证。
2. 在计划末尾追加完整执行变更总结。
3. 记录新增、修改、删除文件和关键代码调整。
4. 记录遗留问题；若无则明确写“无”。
5. 将计划迁移到 `docs/completed/20260716/01-VULCAN_PLUGIN_MCP_INITIAL_RELEASE.md`。

## 十四、验证矩阵

### 14.1 首发清单与领域模型

- 最终 schema v1 正常解析。
- 旧临时结构被明确拒绝。
- 清单不能声明来源 kind。
- 内容型插件无需 execution、Lua 或 MCP。
- Lua 插件必须满足全部 Lua 引用约束。
- MCP-only 插件无需 Lua Runtime。
- 内建与用户来源由宿主唯一确定。

### 14.2 本地 npm MCP

- 私有 npm MCP 成功安装、初始化和发现工具。
- npm 包不存在。
- package bin 不存在或指向包外。
- package.json 与 lockfile 不一致。
- lockfile 缺失或 frozen install 失败。
- Node/pnpm 受管版本缺失或身份不匹配。
- 依赖绝不进入系统全局环境。
- install scripts 不会被执行。
- staging 验证完成后进程被关闭。

### 14.3 本地进程失败

- 入口不存在。
- 进程启动后立即退出。
- stdout 输出非 MCP 数据。
- initialize 超时或返回错误。
- initialized/能力协商失败。
- tools/list 超时或失败。
- 工具名重复。
- 工具 Schema 非法。
- 优雅关闭失败后强制终止。

### 14.4 远程 MCP

- Streamable HTTP 成功连接。
- legacy SSE 兼容连接。
- URL 非法。
- 网络不可达时安装仍可保持禁用完成。
- 启用时网络不可达导致事务回滚。
- Header、bearer 和 OAuth 鉴权失败。
- Secret 引用不存在。
- 敏感值不进入日志和 DTO。

### 14.5 生命周期与原子性

- 插件停用后新执行立即看不到其 MCP 工具。
- 已开始执行保持旧 generation。
- 插件更新后新进程接管，旧进程排空关闭。
- 候选 MCP 失败不会发布新插件 generation。
- 一个插件不能覆盖其他插件或用户 MCP Server。
- 插件启停不会重启无关 MCP 分区。
- Lua Tool、Hook、Skill、Agent、UI 与 MCP 使用同一 revision。
- 卸载在全部 Lease 排空前被拒绝。

### 14.6 动态工具与重连

- tools/list_changed 成功原子替换完整列表。
- 新列表部分非法时不发布任何部分更新。
- 断线后按退避重连。
- 连续失败 5 次进入 Failed。
- 稳定运行后失败计数清零。
- 单 Server 崩溃不自动停用整个插件。

### 14.7 策略与 AI 可见性

- 插件 MCP 工具使用稳定来源 ID。
- Agent Server 白名单和 Tool 白名单正常过滤。
- 工具公开名冲突确定性消解。
- Provider Schema 正规化保持在 Provider 边界。
- RuntimeFoundation 不混用插件目录和 MCP generation。

### 14.8 回归

- 4 个内建 Lua 插件正常加载与执行。
- 内容型 Skill/Agent 插件正常加载。
- 现有用户 MCP 配置正常工作。
- LuaSkills 工具表即时刷新行为不回退。
- 插件安装、更新、启停、卸载和权限确认正常。
- WebUI 插件、Skill、Agent、MCP 设置和工作台展示正常。

## 十五、验收标准

1. 所有任务目标均有实现和自动化验证证据。
2. 最终公开清单仅有一个干净的 schema v1 契约。
3. 仓库不存在为旧临时插件格式保留的兼容代码。
4. 插件来源、权限、依赖、Lua 与 MCP 领域边界清晰且由类型表达。
5. MCP 安装和启用失败不能产生半发布状态。
6. 新旧执行 generation 隔离符合设计。
7. Secret 不进入清单、普通配置、日志或 UI payload。
8. Rust 与 Web 全量验证通过。
9. 计划逐项复核完成并写入执行变更总结。
10. 连续 5 轮完整代码审核零问题；任何问题都会重置计数并触发长期修复。

## 十六、循环审核协议

每轮审核必须覆盖以下方面，不能只重复运行测试：

1. 清单结构、领域命名和未发布兼容代码残留。
2. 文件路径、资源归属和唯一事实来源。
3. 安装、更新、启停、卸载事务与失败补偿。
4. generation、锁、Arc 生命周期、进程排空和并发竞态。
5. MCP 初始化、工具发现、动态列表和重连状态机。
6. npm、registry、lockfile、PATH、进程和 Secret 安全边界。
7. Provider 工具命名、策略过滤和 Schema 正规化边界。
8. Service DTO、事件 revision 和 WebUI 状态一致性。
9. 测试覆盖、错误分支和回归守卫。
10. 注释、错误文案、死代码、重复抽象和维护成本。

审核记录规则：

1. 每轮记录轮次、审查范围、发现项和验证结果。
2. “零问题”表示没有发现正确性、安全性、并发、架构、可维护性、测试或文档问题。
3. 如果发现问题，即使当轮已修复，该轮也不能计入零问题连续轮数。
4. 修复后必须从下一轮重新开始计数。
5. 连续零问题轮次达到 5 前，目标保持 active，计划状态保持执行中。

## 十七、主要风险与控制措施

### 17.1 全链路命名重构遗漏

通过全仓库符号检索、RPC 列表测试、Web 类型检查和“旧命名零残留”结构守卫控制。

### 17.2 插件目录与 MCP Runtime 混代

通过单一 Extension Generation、一次性 RuntimeFoundation 捕获和并发测试控制。

### 17.3 Windows 进程占用阻塞原子发布

staging 验证进程必须关闭并确认退出后才重命名发布目录。

### 17.4 npm 安装执行恶意脚本

默认禁止 lifecycle scripts，registry 由宿主批准，依赖版本与完整性由 frozen lockfile 固定。

### 17.5 插件变更重启无关 MCP

采用来源分区 Runtime，禁止通过重建全局 MCP Runtime 实现插件启停。

### 17.6 断线时工具集合部分更新

Server 工具列表先完整验证，再一次写入快照；失败保留上一份完整成功数据。

### 17.7 审核只修表面问题

每个问题必须说明根因、调整不变量并补充回归验证；禁止通过 suppress、宽泛 fallback 或吞错绕过。

## 十八、计划文件变更边界

预计涉及但不限于：

1. `crates/vulcan-plugin-runtime/`：清单、目录、执行环境、生命周期、描述符和测试。
2. `crates/vulcan-mcp-runtime/`：传输启动、验证、关闭和事件能力。
3. `crates/vulcan-agent-runtime/src/mcp/`：source-neutral 配置、分区 Runtime、聚合访问和工具 Schema。
4. `crates/vulcan-agent-runtime/src/agent_runtime/`：Extension Generation 与 RuntimeFoundation 捕获。
5. `crates/vulcan-service-runtime/`：受管运行时和插件/MCP 适配。
6. `crates/vulcan-service-kernel/`：安装、更新、启停、卸载、事务和投影。
7. `crates/vulcan-service-api/`：DTO、事件和 RPC。
8. `apps/vulcan-code-web/`：插件 MCP 状态、诊断、重试和授权 UI。
9. `plugins/`：4 个内建插件最终清单。
10. `docs/`：首发插件格式与完成记录。

具体文件必须在实施中通过源码和调用链确认后再修改，不以本清单替代事实检索。

## 十九、执行变更总结

### 19.1 核心修复与调整概述

1. 将未发布的插件试验结构直接重构为唯一首发 `schema_version: 1`，公开领域统一为 Vulcan Plugin；删除清单来源 `kind`、旧迁移器、旧 RPC/DTO/WebUI 命名及临时兼容分支。
2. 建立通用 `execution`、可选 `lua_runtime` 与独立 `mcp.servers` 分层；内容型插件、Lua 插件与 MCP-only 插件均由同一严格清单契约表达。
3. 建立本地 stdio、Streamable HTTP 与 legacy SSE 插件 MCP；本地启动只接受受管 Node Entry、私有 package bin 或清单 Native Binary，禁止 Shell、系统 `PATH` 与全局 npm 环境。
4. 使用按插件 ID、版本和内容指纹隔离的依赖环境；安装与更新在私有 staging 中完成依赖、MCP 初始化、工具发现、Schema 与生命周期验证，再原子发布。
5. 新增 `ExtensionGeneration`，把插件目录、Lua/Hook/Skill/Agent/UI 投影与 MCP 分区绑定到同一单调 revision；新执行捕获新代次，旧执行持有旧 Lease 直至排空并确认传输进入 `Stopped`。
6. 建立操作系统凭据库 Secret Store、精确权限预览、内容指纹授权绑定、敏感 Header 强制 Secret 来源、跨读取 stderr 脱敏和有界日志；Secret 值不进入清单、普通配置、诊断 DTO 或 UI 回显。
7. MCP 工具输入 Schema 固定执行 Draft 2020-12 元 Schema、编译器与引用语义三重校验；插件配置 Schema 同样禁止 `$ref`、`$dynamicRef`、`$recursiveRef` 外部 URI，错误不回显凭据化地址。
8. 完成插件安装、更新、启用、停用、重试、Secret 管理、卸载及状态诊断的 Service/RPC/WebUI 闭环，并直接升级 4 个内建插件清单与构建/测试脚本。

### 19.2 📂文件变更清单

#### 新增

- `apps/vulcan-code-web/src/components/features/skills/vulcan-plugins-panel.tsx`
- `crates/vulcan-agent-runtime/src/agent_runtime/extension_generation.rs`
- `crates/vulcan-agent-runtime/src/agent_runtime/vulcan_plugin_hooks.rs`
- `crates/vulcan-agent-runtime/src/mcp/plugin.rs`
- `crates/vulcan-service-kernel/src/plugin_mcp_secret_store.rs`
- `crates/vulcan-service-kernel/src/skills_services/vulcan_plugin_lifecycle.rs`
- `docs/vulcan-plugin-runtime-architecture.md`
- `scripts/test_vulcan_plugin_lua_modules.lua`

#### 修改

- 根依赖：`Cargo.toml`、`Cargo.lock`。
- 插件运行时：`crates/vulcan-plugin-runtime/Cargo.toml`、`build.rs` 及 `src/` 下清单、目录、宿主服务、运行时和导出文件。
- MCP 运行时：`crates/vulcan-mcp-runtime/src/` 下 HTTP、SSE、stdio、launch 与 transport；`crates/vulcan-agent-runtime/src/mcp/` 下配置、注册表、聚合访问、诊断、工具与运行时实现。
- Agent 运行时：`crates/vulcan-agent-runtime/Cargo.toml`、Agent 配置/执行准备/RuntimeFoundation/流启动/边界守卫/工具桥相关文件。
- Provider 与持久化：`crates/vulcan-provider-config/src/config.rs`、`config/runtime_persistence.rs` 与配置往返测试。
- Service/API：`crates/vulcan-service-api/src/settings.rs`、`skills_catalog.rs`；`crates/vulcan-service-kernel/src/` 下启动、状态、插件、技能、MCP、RPC、投影、工作台及测试支持文件；`crates/vulcan-service-runtime/src/mcp_bridge.rs`。
- 工具管理：`crates/vulcan-tools-manager/src/public_names.rs`、`types.rs`。
- WebUI：Skills/Agent/Sidebar/Workspace 组件、Service Actions、类型、语言包、样式和浏览器回退数据。
- 内建插件：`plugins/context7/`、`plugins/vulcan-debug-runtime/plugin.yaml`、`plugins/vulcan-rtk/plugin.yaml`、`plugins/vulcan-usage-statistics/plugin.yaml`。
- 构建与测试：`scripts/build_tauri.ps1`、`scripts/build_tauri.sh`、`scripts/test_context7_plugin.lua`。
- 文档：`docs/luaskills-host-tooling-protocol-20260509.md`。

#### 删除

- `apps/vulcan-code-web/src/components/features/skills/system-plugins-panel.tsx`
- `crates/vulcan-agent-runtime/src/agent_runtime/system_plugin_hooks.rs`
- `crates/vulcan-provider-config/src/config/rtk_plugin_migration.rs`
- `crates/vulcan-service-kernel/src/skills_services/system_plugin_lifecycle.rs`
- `docs/system-plugin-runtime-architecture-20260712.md`
- `scripts/test_system_plugin_lua_modules.lua`

### 19.3 💻关键代码调整详情

1. `VulcanPluginManifest` 及其子结构全部启用未知字段拒绝；`load_vulcan_plugin_catalog` 只按受信内建根和用户根确定 `Bundled`/`UserInstalled`，同 ID 内建包确定性遮蔽用户包。
2. `begin_local_user_plugin_install` 与更新事务要求调用方提交权限预览的 `content_fingerprint`，并在复制、生命周期验证及发布前重新计算候选指纹，消除授权与发布之间的 TOCTOU。
3. MCP 配置使用来源中立类型；插件适配器只把清单内已验证的 Launcher、参数、环境、URL、Header 与 OAuth 来源转换为运行时配置，不复用用户配置的环境继承策略。
4. MCP 分区采用插件 ID 与 generation 稳定命名空间；候选验证分区从不发布，替换后旧分区保留到全部 Server 确认 `Stopped`，单 Server 失败不会重启或污染其他分区。
5. `tools/list_changed` 先完整发现并验证新工具列表，再一次性替换成功快照；非法工具、重复名、外部 Schema 引用或编译失败均保留上一份完整列表。
6. stdio 传输显式清空插件子进程环境；stderr 按完整行组装后脱敏，跨 pipe read 的 Secret 不会绕过匹配，超长未终止行整行丢弃，并同时受 200 行与 64 KiB 上限约束。
7. Service 生命周期通过私有事务目录、Secret 回滚记录、MCP 候选验证、Lua 回调补偿与 ExtensionGeneration 发布形成闭环；发布前失败恢复旧包与状态，发布后清理失败作为独立诊断报告。
8. WebUI 使用 `contentFingerprint` 绑定安装/更新确认，展示 MCP transport、状态、工具/Prompt/Resource 数量、最近脱敏日志、重试与 Secret Key 名称，凭据输入提交后不从服务端回填。

### 19.4 循环审核与验证记录

#### 发现问题并长期修复的审核阶段

1. 修复授权预览与实际安装/更新包之间的内容替换竞态，加入源包与 staging 双重指纹验证及安装/更新回归测试。
2. 修复敏感 Header 可由字面量或普通配置提供的问题，强制 Authorization、Cookie、API Key 与 Token 类 Header 使用 Secret 来源。
3. 修复初始化或发现失败后传输未必关闭的问题，统一失败移除与显式 shutdown，保留关闭失败的组合诊断并增加关闭次数测试。
4. 修复可选 Prompt/Resource 发现可能无限等待、插件 stdio 继承宿主全环境、权限预览遗漏 MCP Server 三项边界。
5. 修复退休分区只收到 shutdown 请求却尚未确认 `Stopped` 时可能提前释放的问题，新增旧 Lease 直至终止停止仍可用的回归测试。
6. 修复 MCP 工具 Schema 只做结构检查、允许外部引用的问题，改为 Draft 2020-12 元校验、语义引用遍历和真实编译；同一安全边界同步用于插件配置 Schema。
7. 终审发现架构文档未记录 Schema 外部引用限制，补齐配置 Schema 与 MCP 工具 Schema 的正式安全契约，并把连续计数清零重审。
8. 终审发现跨 crate 测试仍残留 `bundled_system_packages`、`system_root` 等旧插件领域措辞，统一为 `bundled` 语义并通过全仓新增差异守卫后再次清零重审。

#### 最终连续零问题审核

1. 第 1 轮：清单结构、首发命名、来源唯一性、路径边界、配置 Schema 与包完整性；零问题，插件运行时 58 项测试通过。
2. 第 2 轮：安装/更新事务、失败补偿、内容指纹、Generation 原子发布、锁、Arc 与退休 Lease；零问题，Agent 运行时 691 项测试通过。
3. 第 3 轮：MCP 初始化、工具 Schema、动态列表、关闭、超时、重连、环境隔离、Header 与日志脱敏；零问题，MCP 传输运行时 18 项测试通过。
4. 第 4 轮：Service/RPC/DTO、事件 revision、权限确认、Secret 写入边界与 WebUI 状态一致性；零问题，Service Kernel 481 项测试及 Web 类型检查通过。
5. 第 5 轮：完整 Git diff、维护性、双语注释、旧命名/兼容/临时代码残留、架构文档与最终全量门禁；零问题。
6. 连续零问题审核计数达到 `5/5`，满足退出条件。

#### 最终验证结果

1. `cargo fmt --all -- --check` 通过。
2. `cargo check --workspace --all-targets --all-features` 通过。
3. `cargo clippy --workspace --all-targets --all-features -- -D warnings` 零告警通过。
4. `cargo test --workspace --all-features --no-fail-fast` 通过：67 个套件，2019 项通过、3 项忽略、0 失败。
5. Web `pnpm lint` 通过；项目未定义独立 Web 测试脚本。
6. Web `pnpm build` 通过，4459 个模块完成生产构建。
7. Context7、Vulcan RTK 与 Usage Statistics 三个插件 UI 的 `node --check` 全部通过；4 个内建插件清单由插件运行时生产 Catalog 测试覆盖。
8. 全仓新增差异不存在 `SystemPlugin`、旧插件 `system_root`、schema v2、TODO/FIXME/HACK、Clippy 豁免或未实现占位残留。
9. `git diff --check` 通过。

### 19.5 ⚠️遗留问题与注意事项

1. 遗留问题：无。
2. Web 生产构建仍报告既有的大 Chunk 体积提示，但构建成功；该提示不由本次插件改造引入，也不影响本任务验收。
