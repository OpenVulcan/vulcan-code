# 子代理模型白名单

本文是子代理可用模型范围的契约。实现以下列源码为准，修改其中任一处时同步本文与对应测试：

- 授权条目、边界与规范化：`crates/vulcan-provider-config/src/config/subagent_models.rs`
- 派生校验：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/arguments.rs`
  （`validate_spawn_model_is_granted`）与 `runtime_execution.rs`
- 模型列表投影：`task_runtime/invocation_methods.rs`（`agent_orchestration_info_models`）与
  `tool_surface/result_markdown.rs`
- 会话白名单文件与服务入口：`crates/vulcan-service-kernel/src/subagent_models.rs`、
  `crates/vulcan-workspace-store/src/workspace_store/session_subagent_models.rs`
- 界面：`apps/vulcan-code-web/src/components/features/subagent-models/` 与
  `components/composer/composer-subagent-models.tsx`

## 生效规则

1. **会话模型始终允许。** 当前请求的会话路由（`provider_id_override` / `model_id_override`，缺省时为
   全局激活模型）不需要也不能出现在白名单中才可用；它永远额外获准。子代理自身再派生时，其会话模型即为
   该子代理的模型。
2. **白名单只有两级。** 系统默认清单保存在全局配置 `subagentModels` 字段；会话可以保存自有清单替换系统
   清单。没有项目级。会话没有自有清单时跟随系统清单；“恢复系统默认”即删除会话自有清单。
3. **只校验新派生。** 校验只在 `spawn_agent` 建立新子代理时执行；已存在的子代理继续运行、恢复或回答问题时
   不重新校验。会话更换模型、白名单被修改后，已建立的子代理保留原模型。
4. **超出范围直接拒绝。** 无论模型由 AI 还是用户指定，不在“会话模型 + 生效清单”内即在子代理创建前拒绝，
   错误结果用英文告诉 AI：不要换用该模型重试，应请用户先把它加入白名单，或省略 `model` 使用会话模型。
   不静默替换为其他模型。校验对象是派生参数中显式给出的 `model`；子代理定义自身声明的固定模型属于定义
   作者的选择，不受白名单约束（列表中以 agent fixed model 标识）。
5. **AI 只看到生效范围。** `agent_orchestration_info` 的模型列表只包含会话模型（标记为 session model）和
   生效清单中当前可派生的模型，按用户顺序排列，并逐条附带用户说明。清单中当前不可用的模型不列出。
6. **恢复的子代理按根会话清单派生。** 子代理问题恢复路径从根会话读取自有清单，不会因恢复而放宽范围。

## 存储与边界

- 系统清单：全局 Provider 配置的 `subagentModels` 数组，空数组不写出。
- 会话清单：会话目录下的 `subagent-models.json`，形如 `{"version":1,"models":[...]}`，字段拒绝未知项。
  文件缺失表示跟随系统；文件损坏或版本不是 1 时返回错误，绝不回退成更宽的系统清单。
  该文件独立于会话元数据，不改变 `METADATA_SCHEMA_VERSION`。
- 条目：`providerId`（不含 `/`）、`modelId`（可含 `/`）、`note`（去除首尾空白后保存）。
- 边界唯一来源：`MAX_SUBAGENT_MODEL_NOTE_CHARS`（说明字符上限，按 Unicode 标量值计）与
  `MAX_SUBAGENT_MODEL_GRANTS`（条目上限）；服务快照把二者下发给界面，界面不得另写字面量。
- 重复条目、空标识与超限在保存时拒绝。

## 服务入口

| 方法 | 参数 | 结果 |
| --- | --- | --- |
| `service_subagent_models` | 无 | 系统清单与边界 |
| `service_save_subagent_models` | `request.models` | 替换系统清单 |
| `service_session_subagent_models` | `sessionId` | 会话自有清单（`null` 表示跟随系统）、系统清单与边界 |
| `service_save_session_subagent_models` | `request.sessionId`、`request.models`（`null` 恢复系统默认） | 同上 |

会话入口只接受活动普通会话，并与归档、删除串行。保存后下一回合生效。

## 界面

- 系统设置 → 模型与供应商 → 子代理模型：编辑系统清单。
- 会话设置面板 → 子代理模型：在“跟随系统 / 本会话”之间切换、打开对话框编辑本会话清单、一键恢复系统默认。
  草稿会话在创建后才能单独设置；专职会话不显示该项。
- TUI：设置区“子代理模型”页同时编辑系统清单与所选已保存会话的清单（切换“本会话”），四个服务入口一对一镜像。
