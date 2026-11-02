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

1. 采用 GitHub Actions 步骤写法：参数根对象只有 `steps`；每步只允许 `id`、`tool`、`if`、`arguments`，
   `tool` 与 `arguments`（对象）必填。`id` 匹配 `^[A-Za-z_][A-Za-z0-9_-]*$` 且唯一。
2. 表达式写在 `${{ }}` 中，只能引用更早的步骤：`steps.<id>.outcome|output|exit_code`；函数
   `success()`、`failure()`、`always()`、`contains`、`startsWith`、`match`（返回第 1 个捕获组或整段匹配，
   不匹配即错误）、`fromJSON`；运算符 `!`、`&&`、`||`、`==`、`!=`。字面正则在解析时校验。
3. 整个字符串只有一个表达式时保留其 JSON 类型；与文本混排时转为文本。
4. 计划无效时整个链不执行，一次返回全部错误。用法说明只写在工具描述里（英文），不动态注入。

## 二、执行

1. 步骤缺省条件等同 `success()`：前面有步骤失败即跳过。表达式出错时该步失败且不调用工具。
2. 链本身不经过权限判定；每一步按普通工具调用单独经过计划模式闸门与权限管线。任一步被拒绝即停止，
   剩余步骤记为未运行，报告要求模型改为直接调用该步。
3. 失败判定：工具结果 `is_error`，或命令结果退出码非 0 / 状态不是 `completed`。
4. 可链接范围：MCP 与 LuaSkills 工具缺省可链接；原生工具只有白名单（`CHAINABLE_HOST_TOOL_IDS`）内的可以。
   `tool_chain` 自身、提问、意图声明、子代理、`terminal_session`、`mcp_connect`、`mcp_config`、
   `provider_config`、`env` 永远不可链接。
5. 返回给模型的是按步骤排列的文本报告；宿主结果 `kind: tool_chain` 携带每步的结果、参数及该步自身的
   宿主结果，供界面把每一步渲染成直接调用时的卡片。图片输出替换为文字说明。

## 三、设置

1. 系统设置在 `vulcan-code.json`：`toolChainEnabled`（缺省开启）与 `toolChainMaxSteps`（缺省
   `DEFAULT_TOOL_CHAIN_MAX_STEPS`，范围 `MIN_TOOL_CHAIN_MAX_STEPS`..=`MAX_TOOL_CHAIN_MAX_STEPS`）。缺省只在内存中
   补齐，不回写。范围常量只在 `vulcan-tool-chain` 定义，服务与 TUI 引用，WebUI 由服务校验。
2. 会话选择保存在根会话元数据 `config.selection.provider_tools.tool_chain_enabled`，缺省跟随系统；子代理
   跟随根会话。修改系统开关会影响未自行选择的会话，从其下一次请求起生效，不另行通知。
3. 工具清单在每次请求时按有效开关过滤；回合进行中被关闭时工具仍在桥中，调用返回“已禁用”失败。
4. 会话首次请求后切换会改变工具清单并打破缓存上下文：界面先提示并建议先压缩，用户确认后才保存；保存同时
   置位 `tool_chain_changed_mid_session`，此后请求追加动态上下文 `dynamic.tool_chain`（只说明当前开或关）。
   能力面板等其他工具选择保存保留该会话的工具链选择。
