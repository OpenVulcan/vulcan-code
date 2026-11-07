# OpenCode Go 账号授权与额度查询

核对日期：2026-10-17。本文记录上游契约及本仓映射；字段形状仍以实际 Schema、Rust 类型及生产客户端为准。
上游 Console 更新设备授权、推理入口、工作区规则或 Go 计量结构时，必须重新核对本文与对应合成协议测试。

## 证据来源

- OpenCode `v2` 固定提交 `4617210822bdbb31e5749751afbfb4584ccedac8`，
  `packages/core/src/plugin/provider/opencode.ts`：设备授权、组织作用域、刷新、用户及组织读取。
- 同一提交的 `packages/core/src/models-dev/snapshot.txt`、`models-dev.ts`、`plugin/models-dev.ts`、
  `aisdk-native.ts` 与 `variant.ts`：Go 模型目录、弃用过滤、SDK 协议、上下文、输出、模态及思考变体。
  `packages/ai/src/protocols/shared.ts` 与 `anthropic-messages.ts`：显式思考预算与输出上限联动。
- [官方推理协议](https://opencode.ai/v2/docs/console/api/inference/)、
  [Console 鉴权与服务账号](https://opencode.ai/v2/docs/console/api/)、
  [逐模型端点](https://opencode.ai/v2/docs/console/models/)、
  [Go 套餐](https://opencode.ai/v2/docs/console/go/)：新版推理域名、模型唯一协议与套餐成员。
- 生产 Console 发布的 `assets/meters-C_9Kv9WT.js`：`GET /api/go/status`。
  其导入的 `assets/index-eUJxkx3p.js` 定义订阅、窗口、金额、时间与工作区请求头结构，
  并声明 `GET /api/billing/status`。这些是核对当日发布产物，不能将资源文件名当作永久 API。
- 未鉴权读取生产 `/console/api/go/status` 返回 `401` 与结构化未授权错误。
- 使用自有 `client_id=vulcan-code`、真实 `User-Agent: vulcan-code/0.1` 发起设备授权，
  生产服务返回 `200` 与完整设备挑战；有效期 600 秒，轮询间隔 5 秒。
  仅核对公开挑战，不记录设备证明、用户代码或令牌。
- 使用用户明确提供于环境变量的 Console 服务账号 Key，只携带 Bearer 与自有 User-Agent，
  不携带 OAuth、Cookie 或组织请求头：`/api/go/status` 返回 `200` 和全部三个有效窗口，
  `/api/v2/config` 返回 `200`，`/api/billing/status` 返回 `403 Forbidden`。
  不带凭据的 Go 状态对照请求返回 `401`。这里只记录结果，不保存密钥、账号标识或原始响应。
- 同一已鉴权配置明确区分 `opencode` 的 `/inference/openai/v1` 与
  `opencode-go` 的 `/inference/go/openai/v1`；Go 凭据档必须使用含 `/go` 的套餐入口。

## 认证与工作区

授权端固定为 `https://opencode.ai/console`。Vulcan Code 使用自有客户端标识 `vulcan-code`，
不冒用上游的 `opencode-cli`；所有新请求复用推理模块持有的 Vulcan Code User-Agent。

API 模式使用标准 `ApiKey` 材料，密钥作为不透明 Bearer 发送，不按 `sk-` 或 `oc_sk_` 前缀区分网关。
已验证的服务账号 Key 已绑定工作区，
不生成 OAuth 组织身份，也不发送 `x-org-id`。官方服务账号权限与实测访问结果需分别记录，
不能将这把 Key 的余额 `403` 扩大解释为所有 API Key 都不能访问余额。

1. `POST /auth/device/code` 发送 `client_id` 与 `supports_org_scope: true`。
2. 浏览器访问同一 Console 来源的 `verification_uri_complete`，用户确认工作区。
3. `POST /auth/device/token` 按设备授权类型轮询，处理等待、减速、拒绝与到期。
4. 成功响应包含 `access_token`、`refresh_token`、`expires_in`，组织作用域 `org_id` 可选。
5. 使用同一访问令牌读取 `/api/user` 与 `/api/orgs`，核对组织成员关系。
   服务端省略组织时，只允许唯一组织；多个组织必须重新授权并明确选择，不按名称排序猜测默认值。
6. 刷新仍使用 `/auth/device/token`，类型为 `refresh_token`；若明确返回另一个组织，要求重新授权，
   不静默改换计费工作区。新令牌复用已有修订号比较替换机制与同账号 401 恢复边界。

重新授权可能在相同连接 ID 下更换用户或计费工作区，因此验证新目录前先清除该连接的旧用量及访问证据；
其他账号缓存保持不变。验证、保存或发布失败时，使用原有完整前镜像和凭据修订补偿恢复原状态。
普通连接编辑中，替换 Key 或改变入口、凭据档、凭据模式等身份同样先使自身旧用量失效；
因此 OAuth 切为 Key 后不继续显示旧工作区余额。仅修改显示名称不触发该失效。

用户主体保存在既有 `account_subject` 列，计费工作区保存在既有供应商账号 `account_id` 列；
材料类型为 `opencode_console_oauth`。执行投影单独持有组织字段，不投影到 Codex 账号请求头。
数据库不新增列、不改版本，只增加准确材料类型及必要工作区约束；本机既有 Debug 库必须通过
独立维护入口备份并同步约束，应用启动不迁移。

## 推理与既有连接

只保留 `go-api` 一个入口，认证选择为 `OpenCode Go Account`（`account-oauth`）与
`OpenCode Go API`（`api-key`）。单入口供应商的添加账号对话框不再显示服务入口选择。
原 API 连接保留入口、凭据档、连接 ID 及秘密引用；不保留旧网关运行模式或自动探测逻辑。
两类凭据均使用 `https://opencode.ai/inference/go`；OAuth 发送 Bearer 令牌、
`x-org-id` 与当前会话自己的 `x-opencode-session`。不经重定向转发凭据，不将新令牌发往旧 Go 网关。
普通开放模型走 `/openai/v1/chat/completions`，GPT、Grok、Muse Spark 走 `/openai/v1/responses`，
Claude、MiniMax、Qwen 按源码 SDK 绑定走 `/anthropic/v1/messages`。鉴权配置还明确公布了
MiniMax 与 Qwen 的 Go Anthropic 基地址；不能将通用公开模型表中的 Chat 路由覆盖到这个套餐配置。
具有视频输入声明的 Messages 模型使用宿主既有 `anthropic-messages-video.v1` 编码扩展，端点不变。

### Chat 路由推理回放

Chat 路由声明 `reasoning_replay_contract = deepseek_chat_reasoning_content_v1`，供应商
`engine_contract.required_execution_primitives` 列出同名原语：历史助手回合的推理原样写回
`reasoning_content`，与同一回合的正文及工具调用放在同一条消息。该声明作用于整条路由，
因此路由上每个产出推理的模型都按同一契约回放。

2026-10-21 以 API Key 在同一会话中实测带工具调用的回合：

- 声明前，DeepSeek V4 Flash 在工具调用后的下一次请求失败；声明后连续 6 次工具调用正常完成。
- 声明后 GLM-5.3-Flash、MiMo-V2.6-Flash、Hy3、Kimi K3 均完成工具循环。
- LongCat 2.5 Preview Free 会产出纯空白推理（两个换行）。宿主按原文回放，不把模型真实产出的
  空白推理当作丢失；Responses 与 Anthropic 契约仍要求非空白文本。按原文回放后，同一历史上的
  后续请求正常完成。
- Step 5 Preview Free 返回 `Model is unavailable`；Space Bunny 返回已弃用并指向
  `minimax-m3.1-flash-free`。两者都在端点层面拒绝，未进入推理回放。固定源码目录暂不随之调整，
  理由见上文目录说明。

未验证：路由上其余模型的工具循环；Responses 与 Messages 路由不在本声明范围。

Account 与 API 共用固定源码目录：上游 `opencode-go` 的 31 个未弃用文本输出模型。
上下文、最大输出、输入模态及明确支持温度的模型逐项从同一快照映射；PDF 对应本仓的 `file` 资源。
思考档位先按 SDK 协议执行 `Variant.resolve` 的规则，再映射到本仓控制字段；不把只声明在快照中、
但该 SDK 没有生成的开关当作可选参数。Qwen3.7 Plus 的预算变体还按上游规则随实际输出上限缩减，
输出不足以容纳最低预算时显式拒绝，绝不偷偷增大用户输出设置。

静态源码目录不等同于账号实时可用性。核对日的鉴权配置包含 29 个 Go 模型，
相对于该固定源码快照新增 Beam Free、Gemini 4 Argon Contributor、Step 5 Preview，
未包含快照中的五个旧型号；上游会将远端配置叠加在源码目录上。
本次按用户要求对齐固定 v2 源码目录，后续更新须同时核对快照、协议实现与生产配置，不能混用其他日期参数。

删除旧入口不会在应用启动时迁移或回写配置。已保存的 `go-oauth-api/account-oauth` 连接需要通过
显式维护将入口改为 `go-api`，秘密及其他账号字段保持原样；操作前保留原字节备份。

## 账户额度

账号查询按确切凭据材料选择请求，不进行权限探测或失败后的端点切换：

- API Key 模式只读取 `/api/go/status`，省略组织请求头，绝不请求余额接口。
  余额字段保持缺省，不能用零填充未查询余额。
- OAuth 模式使用同一 Bearer 令牌与同一 `x-org-id`，读取 `/api/go/status` 与
  `/api/billing/status`；任一请求失败均显式报告，不把余额权限失败静默降为部分快照。
- Go 状态返回可空订阅；产品为 `go` 或 `go-plus`，可空 `access` 持有 `meters`。
- 计费状态返回独立的 `balanceMicroCents`；余额不是订阅剩余额度。

`access.meters` 的成员固定为 `fiveHour`、`week`、`month`，金额字段固定为
`limitMicroCents`、`usedMicroCents`，均为整数的 JSON 字符串。每美元等于 100000000 微美分，
必须使用整数运算，不能经浮点数丢失精度。
五小时窗口的 `startsAt`、`resetsAt` 必填但可为 null；周窗口二者必填且非 null；
月窗口只有必填 `resetsAt`。时间按来源字符串解析，不猜测其他字段或单位。
缺失订阅或失效付费周期不生成全额窗口；任一实际请求失败时显式报告查询失败，不伪装成完整成功。
设置页与会话选择器复用相同快照，窗口按稳定身份读取共享语言目录，并显示来源重置时间；
无有效周期时显示无有效 Go 额度，不能误报为供应商不支持用量查询。

## 验收范围

合成服务覆盖授权请求、等待轮询、取消、单次消费、多组织歧义、刷新组织一致性、
凭据关系化存储、最终推理请求头、重定向拒绝、金额精度及三类真实窗口结构。
API Key 合成测试另外核对唯一 Go 状态请求、省略组织请求头、余额缺省、无效订阅与传输失败；
OAuth 测试保留双端点读取及余额失败时整体失败的行为。
生产端已完成自有客户端设备挑战、未授权对照与真实 API Key 的 Go 用量读取。
修改后的 Rust `HttpProviderMetadataClient` 已通过真实 Key 验证清单凭据档分派、三个额度窗口及余额缺省；
在线诊断测试运行后删除，永久测试只保留不依赖真实服务的协议与行为覆盖。
真实 OAuth 账号批准、令牌刷新、余额读取与付费模型调用仍须在用户完成授权后进行运行时验收，
不将 API Key 实测或模拟通过写成 OAuth 线上验收通过。

### 2026-10-18 提交前检查

- Go 定向测试 16 项、思考预算边界测试 2 项、凭据替换及重授权回滚测试 2 项通过。
- 22 个供应商声明与索引有效，107 个生产目标具备覆盖，45 个执行 fixture 通过。
- 前端类型检查、账号设置文件的代码规范检查、相关 Rust 文件格式检查与暂存差异检查通过。
- 清单按仓库要求保存为 LF，索引按实际暂存字节校验；本提交仅更新 Go 的供应商固定值。
  资源同步后，72 个相关资源文件与发行目录逐字节一致。

这些检查不增加真实 OAuth 授权或付费模型调用的线上验收范围。

## 2026-10-31 目录同步、结构化输出与会话请求头

证据：使用 API Key 读取 `GET https://opencode.ai/inference/go/v1/models`（`/openai/v1/models`
返回 404，不是目录入口），并对 Chat 与 Responses 路由发送 strict `json_schema` 真实请求。

- 目录按实时 Go 目录对齐，清单 revision 10，目录与规则各 25 个模型。按用户要求取消 free 系列。
  删除：`deepseek-v4-flash`、`deepseek-v4-flash-vision-exp`、`grok-4.6`、`longcat-2.5-preview-free`、
  `mimo-v2.5`、`mimo-v2.5-pro`、`minimax-m2.7`、`space-bunny`、`step-5-preview-free`。
  新增（Chat，证据修订 `opencode-go.live-catalog.2026-10-31`）：`kimi-k3-1-pro`、`minimax-m3-1-flash`、
  `step-5-preview`，思考档位取自 Go 目录本身。Gemini 4 Argon 需要 Gemini 协议路由，本次未接入。
- 结构化输出：以下模型 strict `json_schema` 真实返回正确结构，并能回填 Prompt 未提及的枚举探针字段，
  因此按所选协议声明 `json_schema`：Chat 上的 `glm-5.2`、`glm-5.3`、`glm-5.3-flash`、`kimi-k2.7-code`、
  `kimi-k3`、`kimi-k3.1-pro`、`step-5-preview`、`deepseek-v4.1-flash`、`deepseek-v4-pro`；Responses 上的
  `grok-4.7`、`muse-spark-1.2-contributor`、`muse-spark-1.3-contributor`、`gpt-5.6-luna`、`gpt-6-luna`。
  muse 与 gpt 在 `required` 只列部分属性时返回 400；宿主 strict 投影会补全为全部属性，不受影响。
  其余模型未实测，保持原声明。
- 会话请求头：API Key 请求缺少 `x-opencode-session` 时返回 400 `MissingSessionID`，因此该请求头对两类
  凭据都是必需的。会话请求的值是会话 ID；VMM 后台请求使用产生该 VMM 命令的会话 ID
  （`session_source_id`），不属于任何会话的命令（如画像指令、清洗、维护）不发送该请求头。

## 2026-11-07 目录同步与 Gemini 路由

证据：使用 API Key 读取 `GET https://opencode.ai/inference/go/v1/models`（30 个模型），对照
[Go 套餐说明](https://opencode.ai/docs/console/go/#how-it-works) 的模型清单与逐模型端点表，以及 OpenCode
官方仓库提交 `c72ce6cf82` 的 `packages/core/src/models-dev/snapshot.txt` 和
`packages/core/src/plugin/provider/opencode.ts`。清单 revision 12，证据修订
`opencode-go.live-catalog.2026-11-07`，目录与规则各 27 个模型。

- 新增 `gemini-4-argon`：走 `google.gemini.generate_content`，路由级 `base_url_override` 为
  `https://opencode.ai/inference/go/google/v1beta`，与说明页端点 `.../google/v1beta/models/gemini-4-argon`
  一致。输入为图片、音频、视频、文件，思考常开，档位 low/medium/high，上下文 1048576、输出 65536。
  鉴权与其他 Go 路由相同，使用 `Authorization: Bearer`：OpenCode 的 `credentialHeaders` 对 Console 凭据只
  发 Bearer 与可选 `x-org-id`。实测 Bearer 流式请求和带 `thinkingLevel` 的请求均返回 200。
- 新增 `hy4`：Chat，图片输入，档位 none/high，上下文 1048576、输出 65536。
- `glm-5.3-flash`：实时目录不再列出文件输入，输入改为图片和视频。OpenCode 快照仍写有 `pdf`，这里以实时目录为准。
- 本次同步时未登记 `beam-free`、`crouton-alpha-free`、`iceman-contributor-free`（当时不收录免费模型，随后按用户决定加入，见下节）。
  `mimo-v2.5` 与 `mimo-v2.5-pro` 不登记：说明页列了它们，但实时目录不返回（2026-10-31 已因此移除）。
- 说明页的协议表与清单所选协议逐一核对一致：Grok、GPT、Muse 走 Responses，Claude Haiku、MiniMax M3、Qwen
  走 Messages，Gemini 走 generateContent，其余走 Chat。

## 2026-11-07 结构化输出补测

证据：对所有尚无结构化输出证据的 13 个 Go 模型，按宿主编码器的实际请求形状发送真实请求，每种模式 2 次。
`json_schema` 使用 strict Schema，并带一个 Prompt 未提及的枚举探针字段，回填正确才算通过。Messages 编码器不支持
无 Schema 的 JSON，只测 `json_schema`。清单 revision 13，证据修订 `opencode-go.structured-output-probe.2026-11-07`。

| 模型 | 协议 | `json_object` | `json_schema` | 声明 |
| --- | --- | --- | --- | --- |
| Claude Haiku 5.5 | Messages | 不适用 | 2/2 | `json_schema` |
| Qwen3.7 Plus、Qwen3.8 Flash、Qwen3.8 Max | Messages | 不适用 | 各 2/2 | `json_schema` |
| Gemini 4 Argon | generateContent | 2/2 | 2/2 | `json_schema` |
| Hy3 | Chat | 400（上游错误） | 2/2 | `json_schema` |
| Hy4 Preview、LongCat-2.0、MiMo-V2.6 Flash、MiMo-V2.6 Pro | Chat | 各 2/2 | 各 2/2 | `json_schema` |
| Hy4 | Chat | 2/2 | 3/5，2 次 JSON 被截断 | `json_object` |
| MiniMax M3.1 Flash | Chat | 2/2 | 忽略 Schema，只回答 `42` | `json_object` |
| MiniMax M3 | Messages | 不适用 | 忽略 Schema，只回答 `42` | `unsupported` |

- 宿主只在 Chat、Responses 与 Messages 路由上把调用方 Schema 发给供应商；Chat 与 Responses 仅限声明了
  `json_schema` 的模型，否则发 `json_object`。因此 Hy3 的 `json_object` 返回 400 不影响使用。Gemini 路由
  无论声明什么都只发 `responseMimeType`（JSON 对象），由宿主校验 Schema，Argon 在该模式下通过。
- Qwen3.8 Max 的输出带多余换行与空白，但都是合法 JSON，结束原因为 `end_turn`。
- MiniMax M3 在 Messages 上没有可用的结构化模式，声明为 `unsupported`。

## 2026-11-07 加入免费模型（用户决定）

用户决定取消“不收录免费模型”的规则，清单 revision 14，目录与规则各 30 个模型，证据修订
`opencode-go.free-models.2026-11-07`。协议、模态、上下文、输出与档位取自实时 Go 目录，并用 API Key 实测。

| 模型 | 协议 | 输入 | 思考 | 上下文 / 输出 | 结构化输出 |
| --- | --- | --- | --- | --- | --- |
| Beam Free | Chat | 文本 | low/medium/high/xhigh/max，均返回 `reasoning_content` | 1048576 / 131072 | `json_schema`（两种模式各 2/2） |
| Iceman Contributor Free | Chat | 文本 | 常开，目录不公布档位；只返回思考 token 数，不返回内容 | 262144 / 131072 | `unsupported`：任何 `response_format` 都返回 400 “Structured `response_format` is not enabled” |
| Crouton Alpha Free | Responses | 图片、视频、文件 | minimal/low/medium/high/xhigh/max，均被接受 | 1048576 / 270000 | `json_schema`（两种模式各 2/2） |

- 三个模型都是限时免费。按 Go 说明页，Beam 与 Crouton 有地区限制（含中国大陆、港澳），本次从当前网络调用均成功；
  Iceman 会用请求与回复训练模型，不属于零数据保留。
- Crouton 在简单题上各档位都不报告思考 token，档位按目录公布声明，强度差异未验证。
