# 协议角色转换与来源恢复规则

日期：2026-08-30

## 一、决策与适用范围

会话以 V1 原事件及供应商无关消息为权威；Responses 是公共请求转换基线，不是所有供应商必须
接受的线路格式。目标协议不支持的角色或工具形式必须显式映射，不能直接转发。

采用**宿主持有的原始角色与来源元数据**实现可恢复性，不把 `<developer ...>` 等内部标签写入
模型正文，也不依赖供应商回传自定义字段。供应商可能删除、改写或复制正文标签，用户文本也可能
包含相同标签；因此解析模型正文不能作为恢复角色、提升权限或配对工具的依据。

本轮复用已经持久化的原事件和投影证明，不新增一份平行的角色镜像、会话格式或每秒写入任务。
本规则覆盖当前角色、指令文本块、工具调用/结果和历史重放，不等同于所有供应商参数、媒体能力
或网关行为均已通过真实 API 验收。

## 二、原始角色与语义来源

公共消息 `MessageRole` 是封闭五态：`System`、`Developer`、`User`、`Assistant`、`Tool`。
以下业务语义不是额外 HTTP 角色：

- `SystemRoleHint::HiddenUserShell`：明确投影为用户层，原事件仍保持系统上下文身份。
- `ProjectedItem::SubagentReturnText`：作为父模型输入投影，原事件仍是子代理回流。
- 目标、计划、续跑提示：由原事件的 `context_key`、角色提示及作用域识别，不从消息正文识别。
- 推理、拒绝、结构化 JSON、资源：属于消息内容类型，不能伪造为新角色。

恢复依据已有的以下事实：

| 事实 | 所有者与用途 |
| --- | --- |
| `seq`、事件种类、`context_key`、`provider_role_hint` | V1 原事件；恢复业务来源与原始指令层 |
| `SourcedProjectedItem.source_from_seq/source_to_seq` | 投影来源；标识一项内容来自哪个事件区间 |
| `ProviderProjectionProof` 的 generation、revision、seq 范围及 fingerprint | 一次物理 API 的不可变规范请求证明 |
| 本地 `tool_call_id` 与 `provider_tool_call_id` | 工具调用及结果配对，不依赖角色名称或正文标签 |
| `ProviderReplayEnvelope` 的供应商、连接、路由、协议、模型 | 限制供应商专属签名和原始输出只能在精确原路由重放 |

这些事实不是逐个 Wire JSON 节点的反查表；当前恢复方法是读取原始事件并重新投影，而不是从
已经合并的供应商正文拆回原消息。诊断追踪可能轮换或写入失败，不能成为会话恢复的唯一依据。

## 三、出站映射

| 协议族 | System / Developer | User / Assistant | 工具调用 / 结果 |
| --- | --- | --- | --- |
| Responses、Codex Responses | 公共运行链将 System 放入 instructions；Developer 保持 developer | user / assistant | function_call / function_call_output |
| 兼容 Chat Completions | 两者映射为 system，并按来源顺序合并指令文本 | user / assistant | assistant.tool_calls / role=tool，以 call ID 配对 |
| Anthropic Messages、视频变体、Claude Code | 两者进入顶层 system | user / assistant | assistant.tool_use / user.tool_result，以 tool_use_id 配对 |
| Gemini GenerateContent、Code Assist | 两者进入 systemInstruction；Code Assist 增加 request 外层 | user / model | model.functionCall / user.functionResponse，保留 ID 与名称 |
| Gemini Interactions | 两者进入 system_instruction | user_input / model_output | function_call / function_result |
| Cohere v2 | 独立编码器存在，但公共 Responses 转换链尚无转换器 | 公共链明确拒绝 | 不能假定独立编解码器注册即表示整链可用 |

约束：

1. 只修改请求副本，不覆盖 V1 原事件、原角色或业务来源。
2. 同一目标协议不能因不同供应商名称而临时采用不同角色映射。
3. Chat 指令内容支持字符串和有序文本块数组。合并不能丢掉数组文本，也不能把图像等非文本
   内容静默删除；不支持的指令内容在发送前明确失败。
4. 原生托管工具、媒体和不透明签名不能因为另一个协议可表达普通函数就被视为可转换。
5. 没有注册公共转换器的协议必须在发送前失败，报告 not_dispatched、不可重试；不能清掉头后
   原样发送 Responses 正文，也不能探测或自动切换第二协议。
6. 新协议接入必须同时验证注册、公共转换入口、请求/响应 Profile、流式解码和完整工具回合。

## 四、返回与反向恢复

### 4.1 切回原协议或重启恢复

读取 V1 原事件 → 按原始角色和来源重新生成规范消息 → 按当前冻结协议重新编码。

例如，发送到 Chat 的 developer 会变成 system；切回 Responses 后仍从原始 Developer 消息生成
developer。不得从合并后的 system 字符串猜测分界，也不得把 Chat 请求正文写回为下一轮规范历史。
合并本身是多对一转换，脱离原始来源就不具备无损逆变换；缺失来源时必须明确报告无法精确恢复。

### 4.2 新的供应商响应

新的响应不等于上一个请求的回显。协议解码器将正文、推理、工具调用、资源等归一化为公共事件；
宿主按 API 身份及事件类型持久化。不能因为请求包含 developer，就把新的助手回复反标成 developer。
实际工具结果由宿主执行器产出并按调用 ID 配对，不从模型正文中的标签推断。

### 4.3 同路由重放与切换

供应商原始上下文和不透明签名必须通过 `ProviderReplayEnvelope.matches_route` 的精确身份检查。
切换供应商、连接、路由、协议或模型后，不借用旧原始项或续传游标；从规范事件重新构建请求。
本轮 Apply Patch 的旧 custom 记录适配遵循同一原则：只转换请求副本，新工具使用标准 JSON 函数；
已有 Codex 游标持有原 custom 调用时，仅待提交结果保留原配对类型。

## 五、验证与当前边界

- 直接 Chat 编码与 Responses 基线转换均覆盖 developer → system。
- 测试覆盖五类规范角色、指令文本块数组、字面角色文本、完整工具配对和切回 Responses。
- 公共生产编码入口覆盖 9 个协议配置；Cohere 和未知协议明确拒绝的回归测试单独覆盖。
- 规范请求在所有编码前后保持不变；已有 V1 请求重建测试覆盖动态指令和来源证明。
- 目前没有新增“逐 Wire 节点映射表”或 XML 反解析器；现有恢复契约不需要它们。
- Cohere 的完整公共转换尚未实现；不得把“阻止错误发送”宣传成“Cohere 已支持”。
- 实际供应商接受的参数与协议能力仍需独立 API 验收；本地转换测试不能代替该验收。
