# 工具链（`tool_chain`）

本文是 `tool_chain` 模型工具的运行时契约。并行工具调用解决“多个互不依赖的调用一次发出”；`tool_chain`
解决“后一步依赖前一步的成败或输出”：模型一次提交有序步骤，宿主按顺序执行，后续步骤用条件和表达式引用前面
步骤的结果。

实现位置：

- 计划解析、表达式、执行顺序与报告：`crates/vulcan-tool-chain/`（与宿主无关，可独立测试）。
- 工具目录与执行接入：`crates/vulcan-agent-runtime/src/tooling/native/tool_chain.rs`、
  `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge/tool_chain.rs`。
- 系统与会话设置：`crates/vulcan-service-kernel/src/tool_chain_settings.rs`、`workbench_model_routes.rs`。
- 界面：WebUI `conversation/tools/cards/tool-chain-card.tsx`、`components/composer/composer-tool-chain.tsx`、
  `components/features/settings/tool-chain-settings.tsx`；TUI 设置页 `app/pages/settings.rs`。

## 一、语法

1. 采用 GitHub Actions 步骤写法：参数根对象只有 `steps`；每步只允许 `id`、`tool`、`if`、
   `continue_on_error`、`arguments`，`tool` 与 `arguments`（对象）必填。`id` 匹配
   `^[A-Za-z_][A-Za-z0-9_-]*$` 且唯一。
2. `tool` 只能是可链接清单中的原生工具（见第三节第 7 条）；其他工具名在计划校验时即被拒绝，错误中列出
   完整清单。
3. 表达式写在 `${{ }}` 中，只能引用更早的步骤：`steps.<id>.outcome|output|exit_code|stdout|stderr`；
   函数 `success()`、`failure()`、`always()`、`contains`、`startsWith`、`test`（正则是否匹配，返回布尔）、
   `match`（返回第 1 个捕获组或整段匹配，不匹配即错误）、`fromJSON`；运算符 `!`、`&&`、`||`、`==`、`!=`。
   字面正则在解析时校验。所有 `steps.<id>` 引用在执行前静态校验，包括短路分支中不会求值的部分。
4. 整个字符串只有一个表达式时保留其 JSON 类型；与文本混排时转为文本。
5. 计划无效时整个链不执行，一次返回全部错误。用法说明只写在工具描述里（英文），不动态注入。

## 二、引用取值

1. `output` 是直接调用时模型看到的文本（去首尾空白），命令结果含头部信息，超限时为分页文件提示。
2. `stdout`、`stderr` 是命令步骤解码后的原始流（去尾部空白），即使可见输出已转为分页文件也完整可用；
   非命令步骤或未执行命令的步骤引用即为表达式错误。原始流由 `shell_command` 经
   `VulcanToolInvocationResult.raw_streams` 交给桥，只存在于链条执行期间的内存中，不进入报告与宿主结果。
3. 不提供结构化结果引用：可链接工具都返回文本，结构化数据通过 `fromJSON(steps.<id>.output)` 或
   `fromJSON(steps.<id>.stdout)` 读取；内部宿主载荷从不暴露给表达式。
4. 脱敏边界与直接调用一致：每步的规范结果与原始流在进入表达式之前即按已登记的敏感值遮蔽，
   链条最终报告再整体遮蔽一次；过滤器不可用时该步失败且不传出任何内容。
5. 原始取值本身不记录；一旦被插入后续步骤的参数，这些参数随该步骤记录并显示。整条链保留的原始流
   合计不超过 `MAX_KEPT_VALUE_BYTES`，超出后的步骤不再保留，引用即错误；含表达式的步骤解析后
   参数不超过 `MAX_STEP_ARGUMENT_BYTES`，超出则该步失败。单条命令仍受 `shell_command` 自身的硬上限约束，
   超限的命令没有输出且步骤失败。

## 三、执行

1. 步骤缺省条件等同 `success()`：前面有计入失败的步骤即跳过。表达式出错时该步失败且不调用工具。
2. `continue_on_error: true` 的步骤失败时仍记为 `failure`（`steps.<id>.outcome` 也是 `failure`），但不计入
   `failure()`/`success()`、缺省跳过与链条状态。
3. 链本身不经过权限判定；每一步按普通工具调用单独经过计划模式闸门与权限管线。任一步被拒绝即停止，
   剩余步骤记为未运行，`if` 与 `continue_on_error` 都不能让后续步骤继续；报告要求模型改为直接调用该步。
   用户停止会中止整个运行并丢弃链条的执行，之后的步骤同样不会开始。
4. 失败判定：工具结果 `is_error`，或命令结果退出码非 0 / 状态不是 `completed`。
5. `shell_command` 的 `command` 参数中，表达式只能得到数字或布尔值；任何字符串、null、数组或对象都使该步
   失败而不调用工具。文本必须通过 `stdin`（或文件）传给命令。不做转义，也不用字符白名单。工具的这类参数由
   桥通过 `StepToolPolicy.command_arguments` 声明。
6. 不回滚：已执行步骤的效果保留。描述要求模型不要把长串写入、删除或发布操作放进一条链。
7. 可链接范围只有原生工具清单 `CHAINABLE_TOOL_IDS`（`tooling/native/tool_chain.rs`，唯一定义）：
   `shell_command`、`file`、`file_read`、`file_write`、`file_edit`、`apply_patch`、`knowledge_search`、
   `knowledge_list`、`knowledge_read`、`memory_search`、`memory_outline`。工具描述由同一清单逐个列出这些
   名称，避免模型猜测；清单是静态的，不随会话可用工具变化，以保持描述稳定，清单内但本会话不可用的工具在
   校验时拒绝。MCP 工具、LuaSkills 工具、会话临时 MCP（`mcp_connect`、`mcp_tools`、`mcp_call`）及其他
   原生工具（`tool_chain` 自身、提问、意图声明、子代理、`terminal_session`、配置与环境变量工具等）永远
   不可链接。新增可链接工具必须返回文本且不需要用户交互，只改这一份清单。
8. 返回给模型的是按步骤排列的文本报告，首行写明 `tool_chain SUCCEEDED`、`FAILED`（列出计入失败的步骤）或
   `STOPPED`（列出被拒绝的步骤）。之后的恢复步骤成功不会撤销失败。工具调用本身始终不是错误（`is_error` 为假）；
   宿主结果 `kind: tool_chain` 携带 `status`（`succeeded`/`failed`/`stopped`）和每步的结果、参数、
   `continue_on_error` 及该步自身的宿主结果，供界面把每一步渲染成直接调用时的卡片，并让卡片头在 `failed`
   或 `stopped` 时显示失败状态。图片输出替换为文字说明。

## 四、设置

1. 系统设置在 `vulcan-code.json`：`toolChainEnabled`（缺省开启）与 `toolChainMaxSteps`（缺省
   `DEFAULT_TOOL_CHAIN_MAX_STEPS`，范围 `MIN_TOOL_CHAIN_MAX_STEPS`..=`MAX_TOOL_CHAIN_MAX_STEPS`）。缺省只在内存中
   补齐，不回写。范围常量只在 `vulcan-tool-chain` 定义，服务与 TUI 引用，WebUI 由服务校验。
2. 会话选择保存在根会话元数据 `config.selection.provider_tools.tool_chain_enabled`，缺省跟随系统；子代理
   跟随根会话。修改系统开关会影响未自行选择的会话，从其下一次请求起生效，不另行通知。
3. 工具清单在每次请求时按有效开关过滤；回合进行中被关闭时工具仍在桥中，调用返回“已禁用”失败。
4. 会话首次请求后切换会改变工具清单并打破缓存上下文：界面先提示并建议先压缩，用户确认后才保存；保存同时
   置位 `tool_chain_changed_mid_session`，此后请求追加动态上下文 `dynamic.tool_chain`（只说明当前开或关）。
   能力面板等其他工具选择保存保留该会话的工具链选择。
