# 子代理模型白名单

本文是子代理可用模型范围的契约。实现以下列源码为准，修改其中任一处时同步本文与对应测试：

- 授权条目、边界与规范化：`crates/vulcan-provider-config/src/config/subagent_models.rs`
- 派生校验：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/arguments.rs`
  （`validate_spawn_model_is_granted`）与 `runtime_execution.rs`
- 模型列表投影：`task_runtime/invocation_methods.rs`（`agent_orchestration_info_models`）与
  `tool_surface/result_markdown.rs`
- 会话白名单文件与服务入口：`crates/vulcan-service-kernel/src/subagent_models.rs`、
  `crates/vulcan-workspace-store/src/workspace_store/session_sidecars.rs`
- 界面：`apps/vulcan-code-web/src/components/features/subagent-models/` 与
  `components/composer/composer-subagent-models.tsx`

## 生效规则

1. **调用模型可选包含。** 系统与会话编辑器提供“包含主代理模型”复选框，默认勾选。
   当前请求的会话路由仅在勾选时额外获准；取消勾选后，只允许显式清单中的模型。
   子代理自身再派生时，调用模型为该子代理的模型。
2. **策略只有两级。** 系统默认保存显式模型清单与包含开关；会话自有策略完整替换两项。
   没有项目级。会话未保存自有策略时跟随系统；恢复系统默认会删除会话策略文件。
3. **空的有效集合关闭子代理。** 先将策略与当前已启用账号上的可用会话模型求交。
   交集为空时，请求构建不注册 `spawn_agent`、`agent_orchestration_info`、`send_message`、
   `interrupt_agent`、`close_agent`、`list_agents`。仅列入不可用模型也视为没有可用模型。
   `ask` 用户提问及独立会话协作工具保持各自准入；共享 `wait_agents_result` 仍服务于跨会话问题与通知。
4. **所有新派生统一校验。** 显式选择、继承调用模型、代理定义的固定模型均按最终解析模型校验。
   省略 `model` 不会绕过未勾选的调用模型限制。超出范围直接拒绝，指引从获准模型中选择或请用户调整设置，
   不自动挑选清单中的其他模型。已建立的子代理继续运行及回流，不重新选择模型；恢复、续跑不视为新派生。
5. **AI 模型选择信息只展示生效范围。** 模型列表只包含获准且当前可用的模型及用户说明。
   取消包含后，调用模型也不能从独立的 session model 区块重新出现。
6. **恢复路径跟随根会话策略。** 子代理问题恢复从根会话读取完整策略，不混合系统开关与会话清单。
   下一个回合读取已保存策略；当前回合已冻结的请求不变，也不改写已创建子代理的模型。
7. **新派生独立选择思考档位。** `spawn_agent.reasoning_effort` 可指定目标模型支持的精确档位；省略时选取
   目标模型支持的最高档，而不是继承父级档位或强行发送字面量 `max`。不支持的显式档位直接拒绝，绝不规范化。
   `agent_orchestration_info` 的模型行公开 `reasoning_efforts` 与 `default_reasoning_effort`，使用宿主枚举拼写
   （如 `x-high`）；`off` 表示保留供应商默认行为。选择复用既有 `ProviderModelSnapshot.reasoning_effort`，
   最终子请求的档位捕获到既有子执行配置中；后续消息、答疑及业务恢复不重新应用新派生默认值，不改变持久格式。

## 存储与边界

- 系统清单：全局 Provider 配置的 `subagentModels` 数组，空数组不写出。
  `subagentExcludeSessionModel` 缺省为 false（包含模型），只在用户取消勾选后写入 true。
- 会话策略：会话目录下的 `subagent-models.json`，版本唯一来源为 `SESSION_SUBAGENT_MODELS_VERSION`。
  当前形状为 `{"version":2,"includeSessionModel":true,"models":[...]}`；开关必填，未知字段拒绝。
  文件缺失表示跟随系统。版本 1 的既有行为是始终包含调用模型，首次读取自动升级为 true：
  使用稳定文件锁串行读取、保存及删除，先保留原字节 `subagent-models.v1.backup.json`，
  再原子发布已验证的版本 2。相同备份可复用，冲突备份停止升级；未来版本、损坏文档与无效条目直接报错。
  该旁置文件独立于会话元数据，不改变 `METADATA_SCHEMA_VERSION`。
- 条目：`providerId`（不含 `/`）、`modelId`（可含 `/`）、`note`（去除首尾空白后保存）。
- 边界唯一来源：`MAX_SUBAGENT_MODEL_NOTE_CHARS`（说明字符上限，按 Unicode 标量值计）与
  `MAX_SUBAGENT_MODEL_GRANTS`（条目上限）；服务快照把二者下发给界面，界面不得另写字面量。
- 重复条目、空标识与超限在保存时拒绝。

## 服务入口

| 方法 | 参数 | 结果 |
| --- | --- | --- |
| `service_subagent_models` | 无 | 系统清单与边界 |
| `service_save_subagent_models` | `request.models`、`request.includeSessionModel` | 替换完整系统策略 |
| `service_session_subagent_models` | `sessionId` | 会话清单与 `includeSessionModel`（同时 `null` 表示跟随系统）、系统清单、`systemIncludeSessionModel` 与边界 |
| `service_save_session_subagent_models` | `request.sessionId`、`request.models`、`request.includeSessionModel`（后两项同时 `null` 恢复系统默认） | 同上 |

会话入口只接受活动普通会话，并与归档、删除串行。保存后下一回合生效。

## 界面

- 系统设置 → 模型与供应商 → 子代理模型：编辑系统清单。
- 会话设置面板 → 子代理模型：在“跟随系统 / 本会话”之间切换、打开对话框编辑本会话清单、一键恢复系统默认。
  草稿会话在创建后才能单独设置；专职会话不显示该项。
- TUI：设置区“子代理模型”页同时编辑系统清单与所选已保存会话的清单（切换“本会话”），四个服务入口一对一镜像。
