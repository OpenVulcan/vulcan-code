# Vulcan Code Agent Notes

本文件只保存任何会话进入仓库后都必须知道的长期约定。专项实现细节放在按需文档中；任务命中对应
条件时才读取，不能把某次会话纪要、版本快照或调试步骤继续堆回本文件。

## 一、所有会话必须遵守

1. **事实先于实现。** 代码路径、类型、字段、协议、配置来源和调用链必须以当前源码、Schema、测试
   和真实运行证据为准；历史计划、完成记录和旧会话只能作为线索。
2. **保护并行工作与脏工作树。** 只修改本任务拥有的文件和逻辑。发现错误属于其他并行会话或未修改
   模块时，记录边界并停止越界修复。
3. **后端文本一律英文，用户文案走目录。** 面向 AI 的 tool name、description、Schema、参数说明、
   工具结果与错误使用英文；后端（Rust）产生的服务错误、操作回馈与显示标签同样写英文。界面上的标题、
   标签与说明走目录（前端按稳定标识或元数据渲染，或由后端以 `WebuiLocalizedMessage` 目录键返回）；
   错误消息与 JSON 诊断只保留英文，不进入目录。
   `make check` 中的 `scripts/backend-cjk-literals.mjs` 按文件棘轮限制 Rust 生产代码中的 CJK 字面量，
   只减不增。界面文案本身只存在于 `crates/vulcan-i18n/catalogs/`。
4. **工具能力必须走完整暴露链。** “已实现”不等于“AI 可见”；新增或修改工具时必须核对注册、请求
   构建、模式可见性、策略过滤、执行和结果投影。
5. **不得静默改写用户配置。** 系统默认优先使用运行时内存默认值；不能在启动或刷新时偷偷回写用户
   持久化策略、模型用途或工具选择。
6. **声明后直接执行。** 模型级协议、工具目标和运行时代际一旦冻结就按声明使用；错误直接暴露，
   不探测备用协议、不静默降级、不用旧连接字段覆盖新权威。
7. **历史状态与实时状态分层。** 一次性结果可以依赖历史记录；长生命周期任务、命令和异步 Provider
   工作必须保留按稳定 ID 查询实时状态的路径。
8. **源码资源与运行产物分离。** 只修改 `crates/vulcan-runtime-assets/resources/` 下的权威资源，不直接
   编辑 `output/`。资源、配置、技能或运行时依赖变化后显式执行 `make.ps1 sync`；`build`、`br` 和
   `release` 不负责同步。
9. **版本以实时依赖为准。** 任何库、协议或运行时版本先读取当前 `Cargo.toml`、`Cargo.lock`、Schema
   和生成配置，不从本文档标题、日期或历史记录推断。
10. **按需文档是条件性强制要求。** 任务命中下列领域时，必须先阅读对应链接，再开始修改。
11. **同一边界值只允许一处定义。** 同一语义的上限、超时、容量等边界值必须有唯一来源，其余位置
    一律派生或引用；发现同一概念存在多个独立字面量时按缺陷处理，无论当前取值是否巧合一致。
12. **正式发布前直接同步开发数据库。** 正式发布版本从 V0.2 起算；当前版本以根 `Cargo.toml`
    `[workspace.package].version` 为准。V0.1.x 期间应用 SQLite 数据库保持 v1，结构以当前源码为准。
    结构有调整时，必须通过独立、显式的维护入口将本地已存在的 Debug 数据库同步到最新结构，
    先备份，事务执行，验证保留数据与完整性；不得删除数据库重建，不得猜测字段映射或补默认值。
    应用启动流程不承担历史结构兼容、迁移或降级，维护工具不进入 Release 启动及发布流程。
    其他持久格式仍按各自契约递增版本并明确拒绝旧格式。V0.2 及以后，持久格式变化需要逐版本迁移
    与原字节备份。

## 二、按需文档入口

下表每一行的“红线”是本会话在不打开文档的情况下也必须遵守的约束；右侧链接只提供细节。

| 任务条件 | 红线（不可协商） | 细节 |
| --- | --- | --- |
| 修改宿主消息固定指引或锁定基线校验 | 指引版本目录只能追加；已锁定正文不得重写，新版说明通过持久动态上下文追加 | [`host-message-guidance-versions.md`](docs/contracts/host-message-guidance-versions.md) |
| 修改会话元数据持久字段、模式版本或读取入口 | 递增 `METADATA_SCHEMA_VERSION`；V0.1.x 不新增迁移，旧版本明确拒绝（见第一节第 12 条），V0.2 起才提供逐版本迁移并在升级前保留原字节备份；未知版本不得降级，用户观察权限不得猜测 | [`session-metadata-migrations.md`](docs/contracts/session-metadata-migrations.md) |
| 修改预发布应用 SQLite 数据库结构 | 应用数据库保持 v1；通过独立工具显式同步本地既有 Debug 库，先备份并验证数据，应用启动不自动迁移；未知字段映射必须停止 | [`debug-database-maintenance.md`](docs/contracts/debug-database-maintenance.md) |
| 修改 `shell_command`、`terminal_session` 或命令会话状态 | `timeout_ms` 省略表示无限等待；`shell_command` 合并输出超过 8 MiB 硬上限时终止进程树并丢弃全部输出，不返回给模型；内联输出超过 `200000` 字节转到 `.vulcan/temp/` 分页文件；`terminal_session.read` 是轮询不是事件推送；`cwd` 只在 `start` 生效 | [`tooling-and-terminal-runtime.md`](docs/contracts/tooling-and-terminal-runtime.md) |
| 修改权限模式、工具权限闸门、判定模型、复核 LLM、用户审批或其设置界面 | 管线固定为模式闸门 → 确定性规则 → 判定模型 → 复核 LLM → 用户，任何错误都不放行，放行只对本次调用有效；判定与复核只用于自动审批模式，无人值守只属于自动审批且切换模式即关闭；无人值守会话永不询问用户，复核未放行即拒绝；高风险或判定拦截项不能由单层放行；模型与后端是系统级，项目配置只保存类别档位；只有显式选择写入判定设置 | [`permission-modes-and-judge.md`](docs/contracts/permission-modes-and-judge.md) |
| 修改 `ask`、提问状态机、Ask WebUI 或 Markdown 预览/渲染 | 模型可见提问工具只有 `ask`，不保留 `question` 别名；必须复用共享 `QuestionRuntime`，刷新或流式启动不得丢弃等待中的问题；Ask 是会话内非模态面板，不得劫持其他窗口键盘；会话 Markdown 复用当前 `conversation-markdown` 链路，不建立第二套解析器 | [`ask-and-markdown-interaction.md`](docs/contracts/ask-and-markdown-interaction.md) |
| 编写或修改插件界面挂载、设置版面或插件 UI 模块 | 设置页只用 `kind: settings` 声明式版面，不在模块中手写设置表单；模块页面由 `sdk.components` 页面套件组成，不注入样式、不操作 DOM、不写颜色字面量，`className` 只用 `plugin-host-components.css` 词汇 | [`plugin-ui-authoring.md`](docs/contracts/plugin-ui-authoring.md) |
| 修改回答模式、`ui-*` 回答组件语法或其 WebUI/TUI 渲染 | 组件语法唯一权威是 `agent-prompts/answer-mode-interactive.md`；WebUI 与 TUI 渲染器必须同步且只渲染已闭合围栏；回答模式是新会话物化时复制的快照，修改全局默认不影响已有会话 | [`answer-mode-and-components.md`](docs/contracts/answer-mode-and-components.md) |
| 修改专职会话（专职助手）的元数据、记忆锁定、请求组成、冻结设置或其界面 | 只能在新建时选择，与普通会话互不转换；基础提示词创建后永不修改；锁定期间已结束的回合不进入 Provider 请求但照常显示；锁定/解锁只在回合之间；冻结设置在首个 `ApiStarted` 之后一律拒绝修改；不使用子代理、上下文压缩与 VMM 记忆 | [`dedicated-sessions.md`](docs/contracts/dedicated-sessions.md) |
| 修改知识库内容、历史、索引、套件、四工具、授权或其界面 | 人工只管理生命周期，AI 工具管理文档及目录内容；写入为 `Ungoverned` 且计划模式禁用；历史永久保留，还原只由人工确认并追加新变更；模式归库，付费查询不随刷新自动重放；旧库、套件及旧选择字段明确拒绝 | [`knowledge-base.md`](docs/contracts/knowledge-base.md) |
| 修改 Markdown 记忆模式、记忆模式切换、三工具、注入快照或其界面 | 记忆只存用户目录，绝不写项目目录；读取绝不改写文件，修改已有节点必须带 `expectedRevision` 且整批原子写入；快照走锁定基线，只在新会话与压缩后刷新；非 `vmm` 模式只在内存中视 VMM 为关闭，不改写 VMM 配置或任一后端数据；`memory_write` 在子代理与计划模式不可用；查询不依赖 codekit | [`markdown-memory.md`](docs/contracts/markdown-memory.md) |
| 修改代码导航、代码检索或其工具暴露链 | 代码导航使用 CodeKit 与实时工作树；不得新增主机预建整库代码索引或内置代码向量检索；语义代码检索通过外部 MCP 接入，概念说明由知识库笔记承担 | [`code-navigation.md`](docs/contracts/code-navigation.md) |
| 修改子代理模型白名单、派生模型校验或 `agent_orchestration_info` 模型列表 | 会话模型始终允许；只有系统与会话两级，会话清单缺失即跟随系统；只校验新派生的显式 `model`，已建立子代理不受后续变化影响；超出范围直接拒绝并指引用户加入白名单，不静默替换；AI 只看到生效范围及用户说明；会话清单损坏或版本不符时报错，绝不放宽 | [`subagent-model-allowlist.md`](docs/contracts/subagent-model-allowlist.md) |
| 修改对外 MCP 服务（`/mcp` 端点、客户端令牌与授权、MCP 工具、媒体任务或其设置界面） | 只接受回环且不带 `Origin` 的请求；令牌只存摘要、只在签发时显示一次，TUI 不签发；写权限依赖读取，范围外会话按不存在处理，工具按授权暴露；权限审批永远不能经 MCP 放行；MCP 提交为后台激活，不改变 WebUI 选中与输入区控件；媒体只开放授权清单内的图片与视频目标，多账号歧义直接拒绝 | [`mcp-service.md`](docs/contracts/mcp-service.md) |
| 修改环境变量的存储、`env` 工具、注入（命令/终端/MCP）、取值请求或其界面 | 密钥值永不返回给 AI 或界面，工具结果按登记值脱敏；用户输入的值只经 `service_submit_environment_values` 存储，绝不经过提问回复；会话变量只写会话侧车 `environment.json`，全局与项目变量只在应用数据库；存储只做简单遮蔽而非加密；保留名禁止存储；MCP 只见全局层；终端在 `start` 时固定环境；不得修改系统环境 | [`environment-variables.md`](docs/contracts/environment-variables.md) |
| 修改会话临时 MCP（`mcp_connect`、`mcp_tools`、`mcp_call`、会话连接注册表）或其权限分类 | 只有三个固定工具，被连接服务器的工具不进入工具清单；连接属于根会话，子代理可用不可开关；绝不写配置、绝不自动重连，Core 重启后明确报错；归档即关闭；stdio 连接按执行命令审批，http/sse 为 Opaque，调用按工具名只读启发式；密钥只经环境变量名取值，不接受字面令牌或请求头；调用前按输入 Schema 校验；启动边界复用已配置服务器的常量 | [`session-mcp.md`](docs/contracts/session-mcp.md) |
| 修改界面文案、语言清单或语言匹配规则 | 唯一权威是 `crates/vulcan-i18n/catalogs/`；任何界面不得另建语言列表、匹配规则或文案表副本；默认语言补全一切，非默认语言不得有额外键，占位符必须一致 | [`shared-language-catalogs.md`](docs/contracts/shared-language-catalogs.md) |
| 修改终端客户端 TUI（`apps/vulcan-code-tui`） | 先读该目录的 `AGENTS.md`；服务调用一对一镜像 WebUI 函数、方法与参数；服务发现令牌不得出现在日志、错误、`Debug` 或命令输出中；验证只用模拟后端或本地合成服务，不对真实服务执行有副作用或付费的操作 | [`tui-design.md`](docs/contracts/tui-design.md)、[`tui-service-adaptation.md`](docs/contracts/tui-service-adaptation.md) |
| 修改 LuaSkills 版本、技能配置、Schema、宿主注入或管理链路 | 版本以根 `Cargo.toml` 与 `Cargo.lock` 为准；实现以 `luaskills_runtime/` 与 `tooling/bridge/` 当前源码为准 | [`luaskills-skill-config-guide.md`](docs/contracts/luaskills-skill-config-guide.md) |
| 新增、迁移或维护系统供应商、模型协议、Provider Tool、媒体扩展 | **字段形状的权威是 Schema 与 Rust 编译器的共同校验结果，不是文档**；供应商事实只通过受签名 JSON 发布 | 参考手册（非契约）：[`adding-system-providers.md`](docs/contracts/adding-system-providers.md)、[`provider-manifest-authoring.md`](docs/contracts/provider-manifest-authoring.md) |
| 维护 APIKEY.FAN（`apikeyfan`）的图片、视频声明、媒体入口门控或证据引用 | 图片/视频模型**只列入媒体入口**的定死目录，不混入对话入口；该目录成员即媒体可用性的权威依据，媒体目标门控指向目录内模型自身 ID，启用由用户在该列表中按模型选择。「按 Key 有效模型集合（如含 `gpt-5.6`）推断生图能力」的旧规则已废弃，不得恢复。带参考图的生成与带 `mask` 的编辑均未验证，禁止据接口形态反推 | [`apikeyfan-verified-media-facts.md`](docs/contracts/providers/apikeyfan-verified-media-facts.md) |

## 三、文档维护规则

- 专项契约变化时，同步更新对应按需文档和源码测试；不要把完整实现细节复制回 `AGENTS.md`。
- 链接目标已经删除、移动或与当前源码不一致时，必须在同一任务中修正或移除链接。
- 已完成计划可以保存决策证据，但不能作为当前运行时契约的唯一来源。
- 复述上游契约的章节必须标注契约归属与同步触发点；上游版本升级时必须回读源码同步该章节，不得让
  转录内容成为无法在仓库内验证的第二权威。

## 四、测试与构建资源约束

1. **默认并发上限。** 仓库 `.cargo/config.toml` 将 Cargo 编译并发限制为 `build.jobs = 16`（同时是 rustc 并行代码生成线程上限），常规 Rust 测试 harness 限制为 `RUST_TEST_THREADS = 4`；不得在验证命令中无故移除或绕过这两个默认值。
2. **nextest 约束。** `.config/nextest.toml` 将默认 `test-threads` 限制为 4；使用 nextest 时保留该配置，若临时提高并发必须明确说明原因与资源风险。
3. **共享资源测试必须串行。** 涉及共享 Provider Manifest、服务内核注册表、端口或其他进程级资源的测试，使用 `--test-threads=1`；nextest 使用 `cargo nextest run --test-threads=1`，避免并行测试互相争用导致假失败。
4. **优先小范围验证。** 修改后先执行目标 crate 的 `cargo check` 或定向测试，再决定是否运行工作区级测试；工作区全量测试可能同时启动大量测试进程，不得默认无约束执行。`cargo check -p <crate>` 不检查测试目标，涉及结构体字段、公开签名或测试代码变更时必须加 `--all-targets` 或直接运行定向测试，否则测试目标可能长期无法编译而不被发现。
5. **doctest 单独执行。** `cargo nextest` 不覆盖 doctest；需要验证文档测试时必须额外执行 `cargo test --doc`。
6. **新环境入口。** 新开发环境优先执行 `make init-system`，不要要求会话手工拼接宿主、Lua、VMM 和 Rust 测试工具安装命令；初始化入口的变更必须同步更新 `docs/contracts/development-environment.md`。
7. **测试断言不得绑定会演进的结构位置。** 断言不得硬编码会随数据增删而移动的下标、序号或行号；需要定位时按特征查找，失败信息必须点明期望值可能已过期，避免把数据演进误判为资源缺失。
8. **测试只约束行为与格式，不约束内容。** 仅为开发期调试而写的测试完成后删除；保留的测试必须约束行为或格式（如 JSON 是否符合 Schema），不得锁定内容快照（如目录数据黄金文件、摘要值）、`include_str!` 源码文本守卫或 AI/用户文案的精确措辞。约束格式契约的线协议、持久化与指纹黄金值可以保留；代码结构规则使用真实机制表达（如各 crate 的 `clippy.toml` `disallowed-methods`、模块可见性）。遗留的中文文案断言由 `make check` 中的 `scripts/wording-assertions.mjs` 按文件棘轮限制，只减不增；新断言改为断言错误码、目录键或测试标识。
