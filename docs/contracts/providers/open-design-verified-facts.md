# OpenDesign 套餐与供应商接入证据

本记录对应 2026-10-14 的模型实测与 2026-10-15 的授权、余额协议验证。清单字段以
`provider-manifest-v2.schema.json` 与 Rust 编译器为准；模型身份与执行能力由
`open-design.json` 持有。本文件记录证据及限制，不另建运行时目录。

## 证据来源与同步条件

- 官方套餐：[价格方案](https://open-design.ai/zh/pricing/)、
  [模型额度与使用说明](https://open-design.ai/zh/model-credits/)。价格、权益、模型成员及窗口变更时重新核验。
- 指定源码：`D:\thirdgithub\open-design`，提交
  `7da9a8aaa2d406bf3aa0a1cb9d4e8e1cd24c9095`。上游升级时重新读取下列路径。
- `apps/daemon/src/runtimes/defs/amr.ts`：AMR 通过 Vela 查询远端模型目录；没有静态可用模型回退。
  `vela model list --all --format json` 提供目录，`vela billing summary --format json` 提供登录账号账单。
- `apps/web/src/runtime/amr-unlimited-models.ts`：套餐名只用于旧标签；实际资格由服务端预检确定。
- `apps/web/src/runtime/amr-low-balance-plan.ts`：付费套餐名本身不能证明额度足够。
- `GET https://amr-link.open-design.ai/v1/models`：Bearer 测试凭据返回 200；无凭据返回 401。
  返回的 `architecture` 与 `metadata` 是本次模型取证来源。
- `POST /v1/chat/completions`：完成文本、流式、函数调用、工具结果续接、思考及媒体探测。
  测试仅使用公开合成内容，不保存密钥、账号身份、上游响应头或加密思考载荷。
- [脱敏验证记录](open-design-20261014-evidence.json) 保留目录、状态、能力验证结果与请求形状摘要。

## 套餐方式

常规月付价格与当日中文额度说明如下；首月折扣、年付折扣和临时赠送可能变化。

| 套餐 | 月费 | 展示的每月最高等值额度 | 套餐模型额度 | Design Plan 7 天窗口 | 5 小时窗口 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Go | $10 | $120 | $0 | $30 | $10 |
| Plus | $20 | $180 | $20 | $40 | 无 |
| Pro | $100 | $480 | $120 | $90 | 无 |
| Max | $200 | $900 | $300 | $150 | 无 |

Design Plan 模型共享额度，先扣 Design Plan，再扣剩余套餐模型额度；Go 没有后一部分。
Go 达到任一窗口上限需要等待重置。7 天窗口从订阅开始计时。套餐模型额度按计费周期发放，
不受这两个窗口限制。API 与客户端共享额度及窗口，不额外发放一份额度。

“每月最高”是指定模型的用量等值，不是现金余额，也不能乘以模型数量。
当日说明中特别指出 DeepSeek V4.1 Flash 的等值可用量翻倍，**共享池本身没有增加**。
价格页内部分逐模型表格和其他语言页面仍显示不同额度；实际以账号账单为准，客户端不硬编码金额或套餐授权。
Go 不含图片生成权益；本接入只提供文本输出的会话模型，图片理解与图片生成分别处理。

## 目录与路由

- 供应商 ID：`open-design`；服务入口：`design-plan`；凭据档：`api-key` 与 `account-auth`。
- 基地址：`https://amr-link.open-design.ai/v1`；唯一已选协议：OpenAI Chat Completions。
- 鉴权：`Authorization: Bearer <credential>`；API Key 前缀为 `sk-amr-`，账号授权使用返回的 `runtimeKey`。
- 目录采用 `discovery_only` 与精确规则交集。没有规则的远端模型不会被猜测启用；
  Key 不再返回的模型不会由本地清单补回。
- 官方营销目录列出 12 个模型，测试 Key 实际只返回 11 个。
  **Hunyuan H4 Preview 未返回，未取得精确 ID、容量及可执行协议证据，因此不创建可用规则。**
- API Key 凭据档的 `account_usage_query` 为 `unsupported`；账号授权凭据档使用
  `open_design_wallet`，以独立 `controlKey` 查询 Design Plan 套餐窗口和美元钱包余额。
  两类额度独立显示，不把钱包金额当作常规模型的套餐剩余额度。
- `metadata.context_limit` 是本次声明的容量来源；`context_budget` 是上游客户端预算，
  不将它误当模型硬上限。输出使用该网关明确发布的 `output_budget = 32000`。

## 逐模型声明

所有模型均支持文本输入及输出。下表只列额外输入。能力来源同时包括目录声明与真实调用；
图片实测为 PNG，文件实测为 PDF，音频实测为 WAV。

| 模型 ID | 额外输入 | 上下文上限 | 输出上限 | 思考档位 |
| --- | --- | ---: | ---: | --- |
| `claude-haiku-5.5` | 图片、PDF | 1000000 | 32000 | none / minimal / low / medium / high / xhigh / max |
| `gpt-6-luna` | 图片、PDF | 1050000 | 32000 | 供应商管理 |
| `deepseek-v4.1-flash` | 图片 | 1048000 | 32000 | none / minimal / low / medium / high / xhigh / ultra / max |
| `glm-5.3-flashx` | 图片、PDF | 1000000 | 32000 | low / high / max |
| `mimo-v2.6-flash` | 图片、音频 | 1048000 | 32000 | none / minimal / low / medium / high / xhigh / max |
| `mimo-v2.6-pro` | 图片、音频 | 1048000 | 32000 | none / minimal / low / medium / high / xhigh / max |
| `deepseek-v4-flash` | 无 | 1048000 | 32000 | none / minimal / low / medium / high / xhigh / ultra / max |
| `deepseek-v4-flash-vision-exp` | 图片 | 1048000 | 32000 | none / minimal / low / medium / high / xhigh / ultra / max |
| `deepseek-v4-pro` | 无 | 1048000 | 32000 | none / minimal / low / medium / high / xhigh / ultra / max |
| `glm-5.3-flash` | 图片 | 1048000 | 32000 | minimal / low / medium / high / xhigh / max |
| `kimi-k2.7-code` | 图片 | 262000 | 32000 | minimal / low / medium / high / xhigh |

### 思考取证边界

- Claude、DeepSeek、GLM 和 Kimi 的无效档位请求返回 400，并明确列出接受值；
  这是网关线路的档位证据，不等同于原厂全部档位语义。
- MiMo 两个模型分别实测上述七档返回成功，无效值被拒绝；`none` 无思考，其他档位返回思考。
- 可控模型通过请求 Profile 从强类型请求复制 `reasoning_effort`，确保内部协议转换后仍显式发送
  `none`，不会把用户选择的关闭思考变为供应商默认。GPT 模型规则替换该 Profile，删除此字段。
- Kimi 在 `none` 下仍返回 `reasoning.text` 与非零思考 Token，因此从可选档位中排除 `none`。
- GLM-5.3 Flash 的图片请求在 `none` 下仍返回 `reasoning.summary` 与非零思考 Token，
  同样排除 `none`，避免承诺无法跨输入类型保证的关闭行为。
- GPT-6 Luna 接受无效档位；Chat 的 `none` 仍可能返回加密思考，Responses 对不同输入回报的
  `reasoning.effort` 均为 medium。没有证据证明固定档位可控，因此声明 `provider_adaptive`，
  仅开放 adaptive，并通过请求 Profile 删除线路中的 `reasoning_effort`。
- DeepSeek 和 Kimi 开启思考时拒绝强制 `tool_choice: required`；默认工具选择可成功调用及续接。
  不通过删除用户要求的强制工具语义掩盖上游错误。

### 视频声明与实际线路不一致

目录为 `glm-5.3-flash`、`glm-5.3-flashx`、两个 MiMo 声明了视频输入。
使用宿主当前 Chat 编码形状 `video_url.url = data:video/mp4;base64,...`，提交两秒合成红色视频后：

- GLM-5.3 Flash 返回 200，但明确表示未收到视频；
- GLM-5.3 Flash-X 返回 `video_url format error`；
- 两个 MiMo 返回上游 400。

因此本次可执行能力不包含视频。远程视频 URL、文件上传或其他协议能否消费视频保持未验证，
不自动切换协议、不拆帧冒充原生视频支持，也不据目录形状开放不可用线路。
恢复视频能力前必须补齐精确载荷、生产 `encode_request` 和真实网关验收。

## 设备授权与余额（2026-10-15）

协议归属是 OpenDesign 的 Vela 设备授权服务；上游 CLI 或上述 daemon 集成升级时重新核验。
本接入复用宿主的已注册授权事务、设备码界面、取消、重新授权与安全凭据存储，
不依赖用户安装 Vela，也不读取或改写 `~/.amr/config.json`。

- 官方包 `@powerformer/vela-cli-win32-x64@0.1.4` 的 tarball 已核对 npm 发布的 SHA-512。
  以隔离临时配置和本地模拟服务运行该二进制，捕获其请求并验证获批响应被接受和保存。
- 创建：`POST https://amr-api.open-design.ai/api/v1/cli/device-authorizations`，
  JSON 为 `cliVersion`、`host`、`profile`；宿主如实使用自身版本、`Vulcan Code` 和 `prod`。
- 创建返回 `deviceId`、`deviceSecret`、`userCode`、`activationUrl`、`expiresAt`、
  `pollIntervalSeconds`、`status`。激活页面固定为 `https://open-design.ai/cloud/cli/activate`，
  URL 内的设备身份与确认码必须和响应一致。设备秘密只留在内存任务中。
- 轮询：`POST /api/v1/cli/device-authorizations/{deviceId}/token`，JSON 为 `deviceSecret`。
  `pending` 按服务端间隔继续；`expired` 终止；`approved` 必须包含 `controlKey`、`runtimeKey`
  和 `user.id`、`user.email`。2026-10-15 真实批准响应没有 `apiUrl`、`linkUrl`；这两个字段曾由
  CLI 模拟夹具提供，不能据此认定其为服务端必填字段。控制请求始终使用宿主注册端点，模型入口由
  Manifest 持有，不从批准响应推断目标。未知状态、错误响应和重定向都直接失败。
- 官方 CLI 的本地捕获显示模型目录请求为 `GET /v1/models?all=true`，使用 `Bearer runtimeKey`。
  `runtimeKey` 只通过推理值访问器投影到模型发现和推理；钱包专用访问器选择 `controlKey`。
  两者没有已验证的刷新协议，不制造 Refresh Token 或过期时间。失效后重新授权。
- 钱包：`GET https://amr-api.open-design.ai/api/v1/wallet/balance`，
  `Authorization: Bearer <controlKey>`。此请求提供当前已鉴权的 `workspaceId` 和
  `workspaceMemberId`，作为后续套餐查询的唯一作用域来源。
- 套餐预检：`GET /api/v1/workspaces/{workspaceId}/billing/preflight`，继续使用 `controlKey`，
  不发送模型参数。返回结构版本必须为 1，工作区、成员及 `codingPlan.workspaceId` 必须与钱包身份
  一致。`codingPlan.tier` 提供实时套餐身份；每个窗口独立保留 `policyId`、`durationSeconds`、
  `usedCredits`、`remainingCredits`、`limitCredits`、`windowStart` 和 `resetsAt`。
  2026-10-15 的真实 Go 账户返回 18000 秒、604800 秒两个窗口。重置时间可空时保持空，不自行推算。
- 预检中的 `balanceUsd` 独立投影到美元钱包余额；该接口明确允许带负号的十进制字符串。
  保留所有十进制位，拒绝 JSON 数字、指数及非法小数。套餐积分保留任意大小的整数字符串，
  界面先用 BigInt 计算比例再转为有界百分比。钱包余额不写入套餐的 `remaining_amount`，
  也不与两个并行窗口求和；钱包余额为零不构成套餐耗尽证据。最终请求计费与准入仍由上游网关决定。
- 套餐读取失败使整次观测暂不可用，不退化为“仅余额查询成功”；`observed_at` 使用实际查询时间。
  [套餐协议脱敏证据](open-design-plan-20261015-evidence.json) 包含官方 CLI 的隔离请求捕获与真实接口字段。
- 已提供的测试 API Key 访问正式钱包接口返回 401，因此保留 API Key 档的余额不支持声明。
  Auth 账户查询的 401/403 明确要求重新登录；其他错误保留为暂时不可用，不回显可能包含凭据的上游正文。
- 成对凭据使用 `open_design_auth` 类型并由既有事务原子保存；运行密钥、控制密钥、
  用户 ID、邮箱分别写入已有类型化列。SQLite 新增该类型的完整性约束，应用结构版本保持 v1。
  预发布开发数据库通过[独立维护入口](../debug-database-maintenance.md)同步最新结构，
  先备份并验证保留值；应用初始化不自动迁移结构，也不重标不受支持的版本。

[脱敏授权证据](open-design-auth-20261015-evidence.json) 区分正式服务响应、官方 CLI 的模拟
协议验证和本地回归。首次挑战验证了创建 201、等待轮询 200 与到期状态 `expired`；后续诊断授权由
用户确认，取得真实 `approved` 响应 200，随后使用返回的控制密钥访问钱包得到 200，金额符合字符串
十进制契约。这证明上游批准和钱包协议可用；独立诊断不等同于桌面端凭据已保存。
随后使用修复后 Debug Core 的正常设置 RPC 完成另一轮用户确认：授权状态为 `succeeded`，账户提交
成功，模型目录状态为 `ready`（本次返回 11 个模型），余额查询为 `supported`、币种为 `USD`，
再次读取设置快照中的余额与查询结果一致。本地数据库仍为 v1，已确认 `open_design_auth` 双密钥
记录落库，`quick_check` 为 `ok`。此验收覆盖应用服务和持久化链路，未执行付费推理或界面视觉验收。
仓库只保存字段名、类型和结果，不保存设备秘密、实际密钥、账户身份或实际余额。

## Logo

Logo 原样取自指定仓库 `apps/web/public/logo.svg`，复制为前端
`assets/provider-icons/provider-open-design.svg`。保留原路径与颜色，复用现有单色图标的暗色主题反转。
没有生成替代图案、外链图片或新增图标渲染分支。

## 验证与限制

真实验证包括 11 个模型的文本、流式和函数调用，9 个模型的图片识别，3 个模型的 PDF 标记读取，
2 个 MiMo 的语音数字识别；DeepSeek、Kimi、MiMo 覆盖工具结果回传续接。
本地回归使用生产 Manifest 编译器及 `ProviderExecutionClient::encode_request`，
从发现目录激活、模型级线路选择到编码，核对思考投影、媒体线路形状及私有路径不泄漏。
生产编码经过内部 Responses 到 Chat 的转换，输出上限映射为 `max_tokens`；
直接协议编码器的 `max_completion_tokens` 不作为本线路生产载荷的推断依据。
两种字段均有真实请求记录；目录内容增减不通过精确快照锁死。

本次本地验收通过：

- OpenDesign 生产选择链与请求编码回归：2 项；
- `manifest_conformance` 与 `bundled_manifests`：23 项；
- 发布门禁：22 份清单、42 个可执行夹具，以及临时签名生成、验签和摘要核对；
- 前端既有供应商图标测试：7 项，变更文件 ESLint 与前端 TypeScript 检查通过；
- `rustfmt --check`、`git diff --check` 通过；Logo 与上游原文件字节一致，
  同步后的 Manifest 与索引和权威资源字节一致。

这不等同于 1M 上下文压力测试、真实额度耗尽测试、其他套餐账号验收或桌面应用重启后的界面验收。
资源通过 `make.ps1 sync` 同步；在线发布的签名与远端分发仍属于独立发布流程。


### 2026-10-15 授权与余额接入验证

- 用缺少 `apiUrl`、`linkUrl` 的真实响应结构先复现授权失败，再修正批准响应类型。
  完整设备事务回归覆盖批准结果读取、运行与控制凭据分离、单次消费；缺失密钥或账户身份继续失败。
- OpenDesign 定向测试通过，覆盖生产模型投影、设备创建和轮询、取消后迟到响应、到期、
  不可信目标、秘密不回显、控制凭据选择及十进制金额格式。
- 实际 SQLite 凭据测试通过，涵盖 OpenDesign 双密钥往返读写；数据库结构测试覆盖
  拒绝未知版本时保留既有版本号和记录。应用数据库版本保持 v1。
- `vulcan-provider-adapters`、`vulcan-service-kernel`、`vulcan-workspace-store` 的
  `cargo check --all-targets` 通过；清单符合性与随包加载 23 项测试通过。
- 前端 TypeScript、变更文件 ESLint、余额展示 12 项既有测试通过；
  后端中文字符串及文案断言检查通过。
- 源清单、索引及 Schema 已通过 `make.ps1 sync` 同步，产物与权威资源字节一致。
- 发布门禁通过：22 份供应商清单、42 个执行夹具，以及临时签名、验签和摘要校验。

### 2026-10-15 套餐用量与钱包分离验证

- 官方客户端的隔离请求捕获与真实账户接口确认了套餐预检路径、控制凭据角色及窗口字段；
  脱敏字段证据见 [open-design-plan-20261015-evidence.json](open-design-plan-20261015-evidence.json)。
- OpenDesign 后端定向测试 11 项、前端计算与卡片渲染测试 16 项、语言目录测试 5 项通过；
  前端类型检查、定向 ESLint、服务内核所有目标编译检查及两项文案门禁通过。
- 已执行资源同步并更新本机 Debug Core。通过正常应用接口查询既有 Auth 账户，确认实时套餐为 Go，
  18000 秒与 604800 秒窗口独立保留精确积分和重置时刻；钱包单独显示，应用用量缓存与查询结果一致。
- 浏览器实测显示 Go、5 小时与 7 天额度、独立进度条、重置倒计时及“高级模型余额”；
  点击刷新后重新读取真实额度，短周期跨过重置边界后显示新的剩余额度和重置时间。
- 本次没有数据库结构变更；本地数据库版本仍为 v1，完整性检查通过。未执行收费推理、额度耗尽测试
  或其他套餐账户验收。

### 2026-10-31 Chat 结构化输出

对 `deepseek-v4.1-flash`、`deepseek-v4-flash`、`deepseek-v4-flash-vision-exp`、`deepseek-v4-pro`
发送 Chat `response_format.type = json_schema` 且 `strict = true` 的真实请求，均返回符合 Schema 的结构，
并正确回填 Prompt 未提及的枚举探针字段。清单 revision 4 为这 4 条规则在
`openai-chat-completions.v1` 上声明 `structured_output = json_schema`（证据
`open-design.chat-json-schema.2026-10-31`）。宿主 strict 投影会把每个对象的 `required` 补全为全部属性。
其余模型未实测，仍为 JSON Object。
