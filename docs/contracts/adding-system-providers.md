# 新增系统供应商流程

本文记录 Vulcan Code 新增、迁移和维护系统供应商时必须遵守的完整流程，以及 OpenCode Go、
APIKEY.FAN 多分组中转站接入过程中已经实际遇到的错误。本文面向内置系统供应商，不适用于
`custom:` 自定义供应商。

完整字段定义仍以以下文件为准：

- `docs/contracts/provider-manifest-authoring.md`；
- `crates/vulcan-runtime-assets/resources/provider-manifests/schema/provider-manifest-v2.schema.json`；
- `crates/vulcan-provider-adapters/src/provider_manifest/` 下的编译器和验证器。

本文重点回答“新增一个系统供应商时实际应该按什么顺序做，以及哪些地方最容易漏掉”。

## 一、不可违背的接入原则

模型目录的来源事实、设置页管理目录与可执行目录必须分层。远端发现到模型，或 Manifest 中存在模型规则，都不等于账号已启用该模型。全局模型列表只显示至少一个账号已启用的模型；账号管理目录仍保留未启用模型以便手动开启。文本会话模型或输入输出类型尚未明确的模型，其最终上下文或输出限制为零时，保留来源记录和设置页警告，将该模型从可执行目录隔离；其余模型及公共 AI 代际继续发布。专用多模态模型不套用通用会话令牌窗口：在未声明会话操作的前提下，发现证据明确不支持文本输入或文本输出，或仅声明语音转写操作时，跳过该窗口的检查与输出上限收敛。明确只生成图片、音频、视频等资源的模型同样豁免。语音转写的文本结果不能被误当作文本会话能力；已声明会话操作的混合模型仍须校验。专用操作自身的参数和资源限制继续生效。未勾选的模型也不能因为这种逐模型能力异常阻止启动。界面须阻止重新启用受阻模型，并提供手动修正能力配置的入口；不得自动回写用户配置或把零值猜成正数。连接、Manifest 完整性及目录资源边界仍按各自的配置契约校验。

1. **先取证，后配置。** 模型 ID、协议、入口、鉴权、上下文、最大输出、输入输出模态、思考档位、
   工具和媒体参数都必须有源码、官方资料或真实请求证据；不允许从相邻模型推测。
2. **模型级协议是唯一执行权威。** 一个模型可以声明多个 `supported_protocols`，但必须得到唯一
   `selected_protocol`。请求失败后直接报错，不探测第二协议、不降级、不静默切换。
3. **系统供应商只由已编译 Manifest 持有。** 不在 Rust 代码中重新维护一份模型表、协议表或
   Provider Tool 参数表。用户新增的会话模型保存于独立的用户配置层，只复用已验证的入口、鉴权和
   协议路由，不修改受签名 Manifest。
4. **模型发现不等于模型授权。** `/models` 只证明当前凭据可见的模型；本地 Manifest 规则只证明
   宿主知道如何安全执行该模型。最终可用集合必须同时满足两者。
5. **供应商连接与模型用途分离。** 供应商设置只管理入口和凭据；模型是否用于会话、知识库或
   VMM，由模型列表中的模型级状态管理。
6. **生成与理解能力分离。** 图片输入不等于图片生成，视频输入不等于视频生成；图片生成与带
   Mask 的图片编辑也是两个独立能力。
   用户新增会话模型仅允许文本输出；视频输入还必须由该供应商所选协议路由明确声明视频传入约束。
7. **源码资源是唯一编辑入口。** 禁止直接修改 `output/resources/`；最终通过 `make.ps1 sync` 同步。
8. **任何 Manifest 字节变化都必须重新固定 Index。** 只改 JSON 而不更新字节数和 SHA-256 会让
   整个随包信任基线失败关闭。
9. **密钥不能进入仓库、fixture、日志或文档。** 真实调用记录必须脱敏，只保留请求形状和必要证据。
10. **面向 AI 的工具名称、description 和 Schema 使用英文；面向用户的设置文案可以使用中文。**
11. **内部多模态不等于上游多模态。** 公共入口使用完整的强类型内部资源语义；每个目标协议仍须
    独立映射并通过最终媒体校验。接入验收必须调用实际 `ProviderExecutionClient::encode_request`
    链路，不能只证明某个独立编码器支持媒体，也不能用内部媒体字段替代供应商的真实字段。

## 二、权威文件与运行链路

### 2.1 必须修改的源码资源

```text
crates/vulcan-runtime-assets/resources/provider-manifests/
├── manifests/<provider-id>.json
├── bundled-index.json
├── schema/provider-manifest-v2.schema.json
├── schema/provider-index-v1.schema.json
└── fixtures/
```

各文件职责如下：

| 文件 | 作用 |
| --- | --- |
| `manifests/<provider-id>.json` | 供应商、入口、凭据、模型、协议、Profile、Operation 和 Tool Target 的权威定义 |
| `bundled-index.json` | 固定每份随包 Manifest 的身份、revision、字节数、SHA-256 和最低引擎契约 |
| `schema/provider-manifest-v2.schema.json` | 可发布字段形状与封闭枚举 |
| `fixtures/` | 真实协议形状的可执行证据，不是只验证 JSON 能解析的样板 |

`output/resources/provider-manifests/` 是同步产物，不是长期来源。源资源验证完成后执行：

```powershell
rtk proxy powershell -NoProfile -File .\make.ps1 sync
```

### 2.2 关键运行时代码

| 位置 | 责任 |
| --- | --- |
| `crates/vulcan-provider-adapters/src/registry.rs` | 只从当前代际编译后的系统 Manifest 生成 Provider Registry |
| `crates/vulcan-provider-adapters/src/connection_graph.rs` | 校验连接、凭据档与系统 Manifest 路由 |
| `crates/vulcan-provider-adapters/src/client.rs` | 模型发现，以及发现结果到模型协议变体的投影 |
| `crates/vulcan-provider-adapters/src/runtime.rs` | 模型选择、冻结绑定、执行投影和运行时代际 |
| `crates/vulcan-service-kernel/src/provider_manifest_sync.rs` | Bundled/Active/Previous 状态、同步、诊断和 WebUI 状态投影 |
| `crates/vulcan-agent-runtime/src/provider_tool_bridge.rs` | 直连工具与稳定模型扩展工具外壳 |

新增普通供应商或模型时应优先只修改 Manifest、Index、fixtures 和测试。只有真实协议需要新的认证、
Wire 编解码、上传、异步生命周期或安全边界时，才允许扩展宿主原语。上传响应 ID 的 JSON 标量类型
也属于 Wire 契约：默认字符串；真实接口返回非负整数时显式声明 `response_id_encoding: unsigned_integer`，
不得在适配器中按供应商名称做隐式强制转换。

## 三、先建立证据表，再写 Manifest

### 3.1 每个模型必须收集的事实

开始编码前，至少建立以下字段的证据表：

| 维度 | 必须确认的内容 |
| --- | --- |
| 身份 | 精确模型 ID、显示名、是否别名、是否带日期版本 |
| 目录 | 静态目录还是 `GET /models`；是否随账号、Key 分组、地区或套餐变化 |
| 协议 | Chat、Anthropic Messages、Responses、Gemini GenerateContent 或其他已注册协议 |
| 路由 | 完整基地址、endpoint path、是否流式、是否异步轮询 |
| 鉴权 | Bearer、`x-api-key`、固定版本 Header、区域或产品入口差异 |
| 会话归属 | 是否配置 `session_header`；若配置，确认服务端要求的会话请求头名称；若与已有或宿主请求头冲突，运行时忽略且不覆盖 |
| 容量 | 上下文窗口、最大输出，以及网关是否需要显式设置输出上限 |
| 模态 | 文本、图片、音频、视频、PDF/文件的输入与输出分别记录 |
| 思考 | 是否可关闭、支持哪些档位、默认档位、具体请求字段和映射 |
| 工具 | 原生搜索、函数调用、抓取、图片/视频/音频理解是否由当前网关真实转发 |
| 媒体扩展 | 生成、编辑、延长、转写等操作的参数、尺寸、格式、限制、同步/异步响应 |
| 证据 | 官方文档、官方/客户端源码、脱敏真实请求与响应，各自证明哪一层事实 |

证据层必须分开：模型厂商官方资料可以证明模型本身的上下文、输出、模态和原生能力；中转站真实
请求才能证明中转站是否接受该协议、字段和工具。两者不能互相代替。

### 3.2 中转站和 Key 分组的特殊规则

APIKEY.FAN 证明了一个常见陷阱：同一个供应商的每个 API Key 可能只属于一个模型分组，切换分组后
`GET /models` 返回完全不同的集合。此类供应商的最终模型目录必须按下式计算：

```text
当前凭据 GET /models 返回的模型 ID
∩
本地 Manifest 中有精确规则、已验证参数和唯一协议绑定的模型 ID
```

处理规则：

- `/models` 返回但本地没有规则的模型直接忽略，不显示、不执行、不猜参数；
- 本地有规则但当前 Key 未返回的模型保持不可用；
- 分组名称不能代替模型目录；
- 每次切换分组都要保存一份脱敏原始 `/models` 响应作为证据；
- 隐式媒体能力可通过 `availability_model_ids_any` 绑定到已出现的前置模型，但不能因此伪造一个
  `/models` 中不存在的普通会话模型。

### 3.3 套餐与额度驱动的目录过滤

供应商客户端按套餐和额度过滤模型时，使用凭据档的 `account_model_access` 与模型规则的
`account_access_groups` 声明，不得增加供应商专属 Rust 分支：

- `plan_identity_field` 显式选择 `plan_id` 或 `plan_name`；
- `plan_rules` 声明每个精确套餐允许的模型分组与 `blocked_model_ids`；
- `unrestricted_when_quota_remaining` 声明哪些额度窗口在剩余值大于零时解锁全部已声明模型；
- 在线用量刷新触发对应凭据目录重建，暂时失败保留最后确认的访问证据；
- 没有声明该策略的供应商不会进入套餐过滤。APIKEY.FAN 仍仅采用 Key 级 `/models` 与本地精确规则
  的交集，不能因为其他供应商使用套餐策略而改变。

## 四、编写供应商与服务入口

### 4.1 根结构

新系统供应商使用：

```json
{
  "schema_version": "vulcan.provider-manifest.v2",
  "provider_id": "example",
  "revision": 1,
  "published_at": "<明确时间>",
  "engine_contract": {},
  "display": {},
  "services": [],
  "model_defaults": {},
  "model_rules": [],
  "usage_profiles": [],
  "request_profiles": [],
  "response_profiles": [],
  "error_profiles": [],
  "upload_channels": [],
  "operations": [],
  "tool_targets": [],
  "provenance": {}
}
```

`provider_id`、文件名和 Index 条目身份必须完全一致。任何协议、执行、上传和转换原语都必须同时
列入 `engine_contract.required_*`，不能只在下方引用。

### 4.2 Base URL 与 endpoint path

基地址和路由必须按真实入口拆分，避免重复 `/v1`：

```text
base_url:     https://opencode.ai/inference/go
Chat:        /openai/v1/chat/completions
Anthropic:   /anthropic/v1/messages
Responses:   /openai/v1/responses
```

常见错误是把 `base_url` 写成已含 `/v1`，同时把 `endpoint_path` 再写成 `/v1/responses`，最终形成
`/v1/v1/responses`。每条 route 都必须通过 `inspect` 或请求投影测试核对最终 URL。

不同地区、套餐、外接/常规入口如果具有不同模型、权限或工具能力，应建立不同 `service_entry_id`，
不得用运行时猜测同一入口的含义。

### 4.3 凭据档与用途默认值

每个入口显式声明凭据档、鉴权和 `default_usage_scopes`。这里最容易误解：

- `default_usage_scopes` 只是模型第一次出现时的初始默认值，不是供应商能力上限；
- 用户最终在模型列表分别管理“会话使用”和“内部使用”；
- 知识库与 VMM 共用模型级内部使用授权，但运行时仍以不同用途进行调度和审计；
- 一个模型可以仅用于会话、仅用于内部任务、两者都用或两者都不用；
- 关闭会话使用不能让模型从模型管理目录消失；
- 不应在供应商页面再维护一份模型启用开关。

这能避免“供应商已启用模型，但知识库不可见”一类由用途上限误建模造成的问题。

### 4.4 登录授权必须声明在凭据档

`credential_mode=auth` 必须同时声明 `authorization`，`credential_mode=token` 则禁止声明。当前有两类
封闭授权入口：

- `registered_oauth`：选择一个宿主已审核的 OAuth 或 Device Flow 原语。OpenAI、Claude、Google、
  Kimi、MiniMax、OpenDesign 使用这一类；Manifest 统一声明入口，但网络交换、刷新、设备身份和区域信任边界仍由
  各自已注册原语负责。
- `browser_callback`：系统 Manifest 声明 HTTPS 授权页、有界回环端口、精确路径、超时、正文上限、
  JSON Pointer 与秘密投影。适用于 CommandCode 这种浏览器直接向本地回调交付凭据的流程。

通用化只覆盖“凭据档声明、事务绑定、前端启动/轮询/提交和安全存储”这些共同生命周期，不能把不同
供应商的 Device Flow 强行改写成字段配置。回调事务必须绑定精确的 provider/service/profile 身份，
提交时重新计算并核对；State 不匹配失败关闭。自定义供应商不得声明宿主管理授权，避免远程或用户
配置自行扩大本地监听与凭据接收权限。

注册 OAuth 的执行边界同样是宿主统一能力，不得在 Kimi、MiniMax、OpenAI、Claude 或 Google 的
Manifest 中增加供应商专用重试字段：LLM 运行时冻结账号引用、模型、协议和路由，但每次真实 LLM
Provider 请求前重读同一秘密引用的最新修订版。另一个请求刷新 Token 后，长任务的下一次请求立即采用新值；可安全重放
的首次 401 允许同账号强制刷新并重放一次。静态 Key、第二次 401、已经产生语义输出或副作用的请求
保持终态，禁止改选账号、模型、入口或协议。

`browser_callback` 授权页若通过跨域 JSON `POST` 向回环地址交付凭据，浏览器会先发送 `OPTIONS`
预检。宿主必须以授权 URL 的精确 Origin 返回 `204`、`Access-Control-Allow-Methods: POST, OPTIONS`
和 `Access-Control-Allow-Headers: Content-Type`，随后继续监听同一事务；预检本身不能被当成凭据请求，
也不能让监听器退出。真正的凭据仍只接受精确路径上的有界 `application/json` `POST`，并继续执行
State、字段映射和秘密前缀校验。

OpenDesign 使用原生设备授权原语，成对保存模型调用的 `runtimeKey` 与钱包查询的 `controlKey`；
没有已验证的刷新令牌协议，失效后重新授权。协议证据见
[OpenDesign 接入记录](providers/open-design-verified-facts.md)。

新增授权能力时先判断现有两类是否精确覆盖。若需要新的网络交换、签名、刷新或浏览器信任边界，先
实现并审核新的宿主原语，再允许 Manifest 引用；禁止为单个供应商增加散落的前端登录分支。

## 五、模型目录、规则与协议绑定

### 5.1 选择正确的目录策略

| 情形 | 推荐配置 |
| --- | --- |
| 模型固定，完全以本地已验证清单为准 | `manifest_only + manifest_static` |
| 模型按 Key、套餐或账号动态变化 | `discovery_only`，并由精确本地规则过滤 |
| 既有固定模型又要求远端证据 | `manifest_and_discovery` |

`manifest_static` 必须同时维护 `model_catalog.models` 和覆盖每个模型的 Exact rule。只加模型身份、
不加模型规则会失败关闭。

当一个服务入口复用相同网络地址但声明身份只属于部分凭据档时，目录项使用
`credential_profile_ids` 绑定精确凭据档；空数组仍表示该入口全部凭据档。此字段只表达已经取得证据的
目录归属，不能代替 Exact rule、协议绑定或能力证据。

### 5.2 每个模型必须得到唯一协议

模型规则应明确声明：

```json
{
  "matcher": {"kind": "exact", "value": "model-id"},
  "supported_protocols": ["openai-responses.v1", "anthropic-messages.v1"],
  "selected_protocol": "openai-responses.v1"
}
```

关键约束：

- `supported_protocols` 表示已经确认可用的协议集合；
- `selected_protocol` 表示实际唯一执行协议；
- “支持多个协议”不表示失败后可以换协议；
- 系统模型绑定不能读取旧连接的 `selected_protocol` 决定当前请求；
- retry 必须保留选择时冻结的供应商、入口、凭据档、模型、协议和 Manifest revision；
- 未命中精确规则的模型不得从供应商默认值猜测执行方式。

### 5.3 工具 Schema 契约

`tool_schema_projection` 是模型规则可选字段，枚举为 `lightweight`、`passthrough`、`strict`。

- 系统供应商不声明时就是 `lightweight`，不得为了“写完整”而给普通模型重复声明；
- `lightweight` 递归去除 `anyOf` / `oneOf` / `allOf` / `$ref` 等组合与引用语法，仅投影基础类型、属性、必填项、数组元素、枚举、描述和布尔形 `additionalProperties`；
- 纯对象备选分支在轻量投影中合并属性，`anyOf` / `oneOf` 只保留所有分支共同的必填项，`allOf` 保留必填项并集；宿主仍使用未投影的规范 Schema 校验实际调用；
- 当前只有 Gemini 模型规则声明 `strict`，其他系统模型全部省略；
- `passthrough` 只在真实请求证明供应商必须接收宿主原始 Schema 时使用；
- 档位冻结进执行路由并参与路由修订，不能根据模型名、供应商名或外层协议临时猜测；
- 自定义模型可在高级设置中显式选择三档，默认仍是 `lightweight`。

所有模型都以直接函数工具调用宿主工具，清单不存在工具模式字段。Codex 订阅模型的上游目录虽然标注
`code_mode_only`，但以直接工具调用同样可用（OpenCode 在同一订阅上长期如此），因此 Vulcan 不提供
JavaScript 代码模式（2026-10-23 移除）。

OpenCode Go 的经验是：同一供应商可以同时有 Chat、Anthropic 和 Responses 三个真实入口，但每个模型
支持的协议不同，必须逐模型声明。不能因为供应商提供三个入口就把三个协议批量赋给全部模型。

### 5.3 思考、多模态和输出限制

思考档位必须逐模型精确配置：

- 不支持关闭时不要提供 `none`；
- 供应商只支持自动思考时，使用 `provider_adaptive` 并只声明 `adaptive`；省略线路字段与显式发送
  `adaptive` 语义相同，不得用 `off` 或标准 OpenAI 档位填充界面；
- `none` 请求返回 200 不代表思考真的关闭，要检查响应是否仍有 reasoning 内容；
- 官方思考档位与中转站实际接受字段分开记录；
- 相邻版本新增 `xhigh` 时只给已验证版本增加，不能批量复制；
- 数组中不要放“为了 UI 完整”而未经验证的 `minimal/medium/xhigh/max`。

系统供应商的上述事实由 Manifest 独占所有权。后台必须拒绝或清理对输入资源、结构化输出、思考支持、
思考控制、思考档位和请求投影的人工覆盖，设置页只显示实际声明；上下文上限、输出上限和上下文窗口
策略仍可由用户调整。自定义供应商不受此限制。

多模态也逐输入输出方向声明。Responses 能承载富内容块不代表模型真实支持图片、音频或视频；模型
支持图片理解也不代表它能生成图片。

结构化输出按精确模型路由声明 `structured_output`：

- `json_object`：可稳定返回 JSON 对象，是未明确声明时的系统基线；
- `json_schema`：可接收并遵循精确 JSON Schema；
- `unsupported`：明确不支持稳定结构化输出。

远端目录没有该字段时不得推断为 `unsupported`。普通会话模型只要同时具备
`conversation_respond`、对应 `input_resources`、有效账号路由，并由用户打开唯一的“多模态理解候选”
开关，即可进入图片、视频或音频理解的模型代替列表。原生结构化输出不是模型代替的准入条件：精确
路线无论是否支持 JSON Object 或 JSON Schema，模型替代理解都只请求纯文本回答，再由宿主包装规范结果。

如果网关在未设置输出上限时使用很小默认值，例如 Gemini 网关默认 8192，应通过 Manifest
Request Profile 写入已确认的 `maxOutputTokens`，不要在供应商命名的 Rust 分支中硬编码。

### 5.4 Request/Response/Usage Profile

供应商字段差异通过封闭 Profile 表达：

- 思考字段映射；
- 最大输出默认值；
- 流式 Usage 开关；
- 标准字段删除、重命名或枚举映射；
- 响应与 Usage 的精确 JSON Pointer。

特定供应商只有在真实响应证明输入分项与输出互不重叠时，才可在 Usage Profile 的 `derivations`
中显式启用 `total_from_input_components_and_output`。它按“非缓存输入 + 缓存读取 + 缓存写入 +
完整输出”计算总额，省略的缓存分项按零处理；未配置时必须保持总额 Unknown，禁止全局自动相加。

不要编写候选字段轮询、`a || b || c` 式兼容或供应商命名 projector。Profile 引用、
`engine_contract.required_transformers` 和 fixture 必须形成闭包。

## 六、第一版 Manifest 唯一权威

项目尚未正式发布，因此系统供应商从第一版开始就只有一套协议事实源，不设计迁移桥或旧协议兼容层。

### 6.1 系统供应商完整条件

每个系统供应商必须同时满足：

1. Manifest 包含真实服务入口、凭据档、模型规则和协议路由；
2. `bundled-index.json` 固定 Manifest 的 revision、字节数、摘要与最低引擎契约；
3. Registry 只从当前已编译 Manifest 注册系统供应商；
4. 模型发现、连接校验、模型选择和执行读取同一不可变 Manifest 代际；
5. 缺失、损坏或不兼容 Manifest 时失败关闭，不回退到代码 descriptor、连接协议或可变旧目录；
6. 每个可执行模型都持有非空模型级协议绑定；
7. `CredentialConnection` 只保存凭据、服务入口和凭据档身份，不保存 selected/verified protocol。

### 6.2 在线更新与客户端版本适配

正式发布后的在线清单服务必须按客户端支持范围投放：

- 服务端只向客户端提供其 Manifest schema、engine contract 与原语集合能够执行的版本；
- 客户端每次请求在线 Index 和供应商清单时都声明客户端发行版本、唯一支持的 Manifest schema 与最高 engine contract，服务端据此选择兼容代际；
- 客户端仍必须独立校验 `schema_version`、`minimum_manifest_engine` 和所有 required primitives；
- 收到更高 schema、engine 或未知原语时立即拒绝激活，并明确提示升级客户端；
- 不允许通过旧协议、代码 descriptor、字段猜测、协议探测或跨协议重试实现“临时适配”；
- 同一受支持 schema 内可以在线增删模型和更新声明；新增宿主原语必须先发布新客户端，再向该版本投放清单。

这使在线更新承担“同一客户端能力范围内的数据更新”，而客户端升级承担“引擎能力升级”，两者不能混用。

## 七、Provider Tool 与多模态扩展

### 7.1 五项直连能力

以下能力变化小、调用目标明确，保持单选直连：

```text
web_search
web_extract
image_understand
video_understand
audio_understand
```

搜索和抓取由用户直接选择一个目标。三项媒体理解仍然是单开关、单后端，但后端是封闭二选一：

1. 供应商专用模式：选择一个当前可用的对应 Provider Tool Target；
2. 模型代替模式：选择一个已准入且支持对应输入模态的普通会话模型，并选择该模型真实支持的一个
   思考级别。

两种模式均不可多选，不进入生成型模型扩展查询流程。系统设置只保存新会话使用的默认开关与默认
路由；会话能力设置始终从当前可执行的供应商专用 Target 和支持对应输入模态的普通会话模型中独立
列出全部候选，不受系统默认开关、默认模式或默认路由是否已保存的限制。会话保存精确的模式、模型或
Target、账号及可选思考档位，首次物化后即成为该会话自己的路由权威，并覆盖系统默认值；后续系统
设置变化不得改写已初始化会话。供应商专用模式不得携带思考档位；会话保存的精确路由不再可执行，
或会话档位已不被精确模型路由支持时，必须显式报错并要求重新选择。
会话能力入口必须由当前可执行 Target 与模型的真实 `input_resources` 共同生成，不能只显示已保存过
系统默认后端的媒体类型。模型代替模式仅在精确路线声明 `toggle` 或 `effort_selection` 且存在可选
档位时显示会话思考设置；`fixed_off`、`fixed_on`、`provider_adaptive` 与未知控制模式不得渲染覆写
控件或虚构可选级别。
系统设置中的候选可用性不等于默认后端已经保存；缺少活动选择时必须明确显示“未配置”，不得把首个
候选模型或账号渲染成已持久化默认值。会话页面可把已保存的系统默认路由作为首次选择偏好，但必须
允许选择其余全部当前可执行路由，且不得把展示回退误认为已保存会话配置。

模型列表中的共享“多模态理解候选”开关只控制系统设置页可保存的新会话默认候选，不是会话能力的
准入开关。会话能力设置始终从当前可执行目录按真实输入模态列出模型，即使该模型未被勾选为系统
默认候选也必须可选。系统默认候选开关开启前必须存在至少一条可执行媒体会话路由。结构化输出能力由供应商层或模型层手动维护为
`json_object`、`json_schema`、`unsupported` 三态；未声明时默认 `json_object`，精确系统模型路线
明确不支持稳定 JSON 时必须设为 `unsupported`。该状态不阻止模型作为理解候选，也不参与媒体理解
模型替代请求的编码。

三项媒体理解工具对 AI 的 Schema 固定为“实际媒体路径或 URL + `prompt`”。供应商 Target 必须通过
`media_input_projection` 把固定外壳映射到内部 `input_schema`：声明 `path_field`、`prompt_field`、
可选单元素数组包装和最多 16 个 `fixed_arguments`。缺失投影、投影字段不在内部 Schema、固定参数
冲突或能力不属于三项理解时，Manifest 必须激活失败。切换模式、目标、账号、模型或思考级别不得
改变工具 ID 与 Schema；搜索、抓取仍保留供应商目标相关 Schema 例外。

供应商专用模式可返回纯文本，由宿主包装为固定的 `vulcan.media-understanding.v1` 结果。模型替代
模式同样固定请求纯文本，不发送 JSON Object、JSON Schema 或其他结构化输出控制；宿主将回答包装为
`answer`，并生成空 `segments` 与 `warnings`。即使回答文本外观类似 JSON，也按普通理解文本保留，
不得把会话模型当作宿主结果 Schema 的实现者。
上传通道需要用户可见进度时声明 `progress_supported=true`；底层可中断时再声明
`cancellation_supported=true`，所有成功、失败和运行取消都必须产生终态事件。
模型代替内部请求只有在显式档位实际启用推理时才能设置推理输出控制；未选择档位或显式选择
`none` 时必须使用协议默认输出策略，不能生成“输出控制已启用但推理档位缺失”的非法请求。

### 7.2 九项模型扩展能力

以下能力使用系统原生模型，每项能力分为单模型模式（默认）与多模型模式：

```text
image_generate
image_edit
video_generate
video_edit
video_extend
speech_synthesize
speech_transcribe
music_generate
music_cover
```

模式保存在 `provider_extension_modes`（缺省条目即“单模型、未选择”）；系统设置保存新会话的默认值，
会话可在快速选择中覆盖。两种模式的选择分开保存，切换模式不会丢失另一模式的选择。

**单模型模式。** 每项能力只固定一个模型（`single_target`），不可多选；未选择即不注册该能力的工具。
宿主直接以该模型的 `args_schema` 注册执行工具，并省略取值固定且等于默认值的可选字段；AI 不传
`model`，也不经过参数查询。

**多模型模式。** AI 可见工具 Schema 必须稳定，不能因为用户刷新模型或改变多选集合而增删一批模型专属
工具；多模型模式下没有已选模型的能力不注册。当前固定流程是：

1. AI 调用 `get_model_extension_parameters`，传入精确 `capability`；
2. 宿主返回当前已选择、可用的系统原生模型、说明、限制和该模型精确 `args_schema`；
3. AI 选择返回的公开模型 ID；
4. AI 调用对应稳定执行工具，只传：

```json
{
  "model": "apikeyfan/gpt-image-2",
  "args": {
    "prompt": "..."
  }
}
```

执行工具只接受 `model` 和 `args`，不接受模糊别名和备用字段。自定义供应商、Custom Manifest、过期
选择或未被用户选中的目标必须在查询与执行两层排除。

纯生成模型若未声明 `conversation_respond`，不得投影为会话模型路由、会话使用开关或内部 LLM 候选；
它只出现在对应的模型扩展设置中。`availability_model_ids_any` 校验的是当前凭据经原始模型目录与账号
权限共同过滤后的有效目录实际拥有该上游模型，不得复用“会话使用”状态作为 Provider Tool 门禁。
生成扩展是否允许 AI 调用，只由该能力当前模式的选择决定：
单模型模式看 `provider_extension_modes` 的固定模型，多模型模式看 `provider_extension_target_sets`。这样同一个账号关闭会话模型发布时，
不会误伤图片、视频、语音或音乐扩展；反过来，勾选扩展目标也不会让纯生成模型进入会话选择器。

会话发出首个请求后再修改工具选择会打破已缓存的上下文：界面先请求确认（建议先压缩上下文），保存后
宿主在下一次请求以 `dynamic.provider_tools` 注入一条英文说明，完整列出当前生效的配置。

### 7.3 每个扩展目标必须有完整说明

系统 Manifest 的每个生成型 `tool_target` 必须有 `extension_profile`，至少说明：

- 模型特点与适用场景；
- 输入模式；
- 支持的尺寸、分辨率、画幅和时长；
- 输入输出格式；
- 自由尺寸还是固定枚举；
- 数量、文件、MIME、大小和异步生命周期约束；
- 已知限制和证据来源。

缺少已审核说明或精确 Schema 的模型扩展不应允许选择。

### 7.4 图片生成与图片编辑不能自动互相路由

APIKEY.FAN 的实测纠正了一个重要误区：

- 无参考图的文生图属于 `image_generate`；
- 有参考图、无 Mask 的参考条件生成仍属于 `image_generate`；
- 只有调用方明确选择图片编辑并提供原图和 Mask 时，才属于 `image_edit`；
- 不能因为生成请求存在参考图就自动改走 `/images/edits`；
- 列表中出现一个“edit”模型但真实调用 404 时，必须忽略该模型，不能只凭模型名启用。

### 7.5 媒体接口必须逐模型验证

不同媒体模型即使共用 `/images/generations`，参数也可能完全不同。例如 GPT 图片使用 `size`、
`quality`、SSE 与 Base64 PNG；Grok 图片使用 `aspect_ratio`、固定 `1k` 和同步 JPEG。不能把某个模型的
Schema 注册给同一 endpoint 下的所有模型。

异步视频还必须验证：

- 提交路径和任务 ID 字段；
- 状态查询路径、成功/失败/取消状态集合；
- 轮询间隔、最大退避和总超时；
- 内容下载路径、鉴权和 MIME；
- 文生视频、单图生视频、多参考模式的互斥字段；
- 分辨率、画幅和时长真实边界。

### 7.6 会话附件准入不等于模型原生输入能力

系统供应商的 `input_resources`、协议编码器和 `input_resource_constraints` 只决定一次冻结请求如何
消费附件，不得作为用户能否把普通文件挂入草稿的上限。当前统一附件兼容层按以下顺序处理：

1. 所有普通文件先进入宿主管理目录，生成稳定资产 ID、SHA-256、字节数和项目相对
   `resource_path`；未知格式按普通 `File` 保存，不尝试活动内容预览。
2. 当前精确模型、协议、服务入口与输入约束共同生成 `InlineOriginal`、`UploadReference`、
   或 `MetadataOnly` 请求局部计划；新计划不选择旧 `InlineDerivedPreview`，不能用预览绕过限制。
3. 主会话四种媒体各自配置有上传／无上传的原始内联阈值。无上传默认512 KiB；有上传偏好默认零，
   即默认真实上传。后台显式允许的小文件可内联，但必须先符合选中接口硬大小及实际编码能力。
4. 内联资源共享默认2 MiB、后台可配置的编码后预算，并继续服从 Manifest 的 MIME、单文件大小、
   数量和引用策略。模型数量按单条消息限制，上传接口数量按单次上传限制；二者不能混为一个总数。
   任一适用限制不满足时，只把该请求副本降级为确定性的附件清单。
5. 模型不支持、协议不支持、上传失败、资源读取失败或预算不足都不能删除规范历史附件，也不能
   导致整个文本请求被拒绝；模型只能看到附件 ID、项目相对路径、文件名、MIME、大小和不可见原因。
6. 清单按附件类型区分。`project-resource://`／`attachment://` 是逻辑定位符，普通文件工具、命令和
   脚本都不能直接打开，因此文档类（`File`、`Pdf`）清单另外给出已保存副本的绝对路径 `path`，模型用
   普通工具直接读取，不复制、不额外调用工具。项目资源的路径由项目资源目录与 SHA-256 推出
   （`project_resource_entity_path`），不访问文件系统，回放文本保持一致；副本可能没有扩展名，清单
   说明要求模型按 `file_name`／`mime_type` 选择解析方式、需要扩展名时自行复制，且不得修改副本。
   读取工作区外路径由文件权限决定，将来的权限功能在读取前提示授权。图片、音频与视频在本回合
   存在工具路线时不给路径，仍走理解工具与 `read_media`（后者负责压缩与上下文控制）；工具路线指
   本请求可见对应理解工具（`image_understand`／`video_understand`／`audio_understand`），或可见
   `read_media` 且所选模型原生接受该资源类别。没有任何工具路线时，媒体清单同样给出 `path`，
   避免模型只知其存在而无从读取。判断是否给路径只在
   `provider_input_normalizer::metadata_only_discloses_stored_path` 定义，V1 物化与旧标准化链路
   都以本请求实际暴露的工具名调用它。

`file_id`、供应商上传 URL 和上传任务 ID 只存在于最终 Provider 物化边界，禁止进入规范历史、AI
附件清单、工具参数、工具结果和模型可见错误。媒体理解及声明了资源绑定的扩展工具接受安全
`project-resource://<id>`、当前会话持有的`attachment://<asset-id>`或显式本地文件路径。显式路径
可由用户提供给AI并进入本次工具参数，但宿主必须在分派前将其物化为上传、内联或供应商引用，不能把
宿主路径原样发送给供应商。相对路径以活动工作区为基准；当前尚无文件权限功能，因此绝对路径及项目外
路径均可读取，未来权限功能应在读取前提示授权而非静态禁用。执行前按来源类型校验定位符所有权或路径
可读性，并统一校验普通文件、MIME、大小和SHA-256。宿主不得因为附件或路径存在而自动调用理解工具；
会话AI根据用户真实意图主动调用已注册工具。

新增供应商时，只有 `InputResourceConstraint + upload_channel_refs + reference_policy` 与当前协议
消费字段形成可验证闭环，才声明可引用上传。系统兼容层不会根据供应商名、相邻模型或较大的官方
Base64 上限猜测上传能力，也不会把某一家供应商的限制写成全局特例。

独立 AI 媒体扩展已经使用单独冻结的投递设置，不继承主会话阈值。图片、音频、视频和文件
各自具有有上传／无上传两项阈值；无上传默认 10 MiB，有上传内联偏好默认零，累计编码预算默认
16 MiB。必须声明实际可执行上传和返回引用的消费形态；内联原语不算真实上传。接口硬限制优先，
超限不得以 Base64 绕过，扩展必须明确报告无法理解或处理文件。细节见作者手册“独立媒体扩展的
投递策略”；主会话与历史链的迁移状态以统一多模态方案及审核记录为准。

V1原投递身份现在通过用户提示词参数表达，不挂在API起始事件或全局文件的“最后模型”上。提示词及其
资源出现位置拥有各自来源，按既有事件序号处理时序；同文件在后续提示词重新附加不会覆盖较早来源。
首次原身份冻结与物理API起始共用可靠提交边界，但原参数仍仅保存在原用户事件。上传回执只记录
不透明身份，不能把私有引用URL或媒体正文混进提示词。项目资源绑定保留原上传回执及原生命周期，
只能追加，不能以之后的上传替换早先回执。首次请求路线和每个资源出现的原路线分开保存，支持
同一提示词包含不同原模型的附件。当前已接入V1逐提示词物化、精确原回执恢复、草稿逐出现来源移交，
以及固定理解和动态模型扩展中的项目资源地址。纯项目草稿按原输入动作及原资源位置保存来源，
不借用最新模型或全局文件缓存。动态扩展只解析冻结Operation明确声明的媒体绑定；项目元数据保留
原MIME、文件名、大小和摘要，全部输入形态先检后解析，不以已知MIME绕过绑定格式限制。旧
`attachment://ID`也已按明确调用会话及托管索引解析，先做元数据准入、后在物化时验证正文，不能
借用其他会话或未提升资产。普通预构建已改为纯安全元数据，正式媒体由V1原提示词按时序准备；
WebUI远程压缩接入相同冻结来源的只读投影，不产生新上传或改写历史。精确捕获使用宿主来源端口，
恢复原媒体或预览新内联；需要新上传、缺少含附件来源证明时明确返回不可精确捕获，不能把临时
占位文本当成最终线上内容。任务压缩、非会话入口、上传生命周期及完整浏览器流程仍按统一方案
继续验收，不代表整体完成。

V1新上传现已复用运行时进度桥：先策略及精确缓存预检，再开始真实传输；缓存和历史恢复不发出假
进度。选中接口总超时覆盖凭据刷新、物化及返回校验，Future丢弃取消局部后台工作，超时不冒充父
请求取消。终态封闭、锁外有序观察器防止迟到回调重开进度。会话上传失败提示附件保留及本轮未发送
内容。实际HTTP响应状态及签名/传输阶段已经强类型传递至V1附件说明与进度，经过真实HTTP/HTTPS
回环验证；错误响应不先读正文，不暴露供应商载荷。文件创建上传改为48KiB原始分块、精确总长度的
单个JSON请求，本地文件不再整文件分配及编码；字段、接口硬限额和原始字节进度保持准确。此处
Base64是上传接口封装，不是上下文内联，不能套用会话512KiB限制。来源长度/摘要不符、取消或正文
未完成时不得接纳原生引用，即使服务器提前200也不能绕过校验。前置准备有界性、整条取消/重准备
与浏览器验收仍未完成，不能据局部测试宣称整体完成。

准备取消令牌现从元数据规划排队前开始生效；异步来源操作等待中响应取消，同步内联读取按64KiB
块协作停止。下一次可靠V1 API许可前，较新输入/配置触发的自动重启最多3次，跨路由、策略及压缩
重启保留计数；成功许可后清零，不限制跨成功API的正常模型切换。耗尽按原流所有权报告准备不
稳定，不改写prompt原身份。共同期限及完整流/浏览器验收仍需继续。

完整预检后，最终预算只由实际生成的原生内容占用；失败项释放占用，后项按原提示词/附件顺序
重新判断，仍使用相同冻结接口与原来源规则。捕获按最终逐项决定拒绝新上传，不再因暂定上传而
拒绝后来实际可内联的内容；不产生上传副作用或原身份写入。首次V1上传的已确认成功结果现按
精确prompt出现保存，前沿失效仍拒绝旧请求；不使用文件最近回执代替原身份。

参数层已新增逐出现准备状态，保留原选择、已完成原投递与尚未决定空位，且不改写初始准入证明或提前
冻结首次请求路线。完成结果幂等且不可替换，最终冻结必须保留它们；读取端、实际存储滚动以及
A上传→B地址→A原引用、历史内联切换的定向测试已覆盖该状态。真实上传生产者现已接通完成交接，
父Future取消不会撤销已知成功结果的写入工作线程；同一准备门保留到原证明及私有回执结算，Stop
及失败终态等待该门。自身修订由正式可靠快照重捕获承接，外部修订仍拒绝旧请求。真实HTTPS成功
后切模型的回归已通过。首次V1上传现也在凭据及网络操作之前持久声明精确原上传选择；无回执时
保持地址，不对B重新上传，也不借用同文件其他回执。真实HTTPS发完正文、收到回执前取消的验证
及输入区规划检查已通过。草稿转prompt保留相同未完成选择及原意图ID；提交注册按真实项目所有者
登记晚到回执目标，不在草稿规划时猜定默认项目。晚到完成只发布到活动原意图或已登记交接，不覆盖
移除重加后的新草稿，也不改写已冻结prompt或新增消息。回A匹配精确意图及有效原私有回执，B仍为地址；
未关联旧选择仍不查最新缓存。真实HTTPS跨B首次API冻结的回归已通过，不能据此宣称全部RPC取消时点、
存储故障、代际替换、共同期限及完整浏览器矩阵已经验收。

主会话V1和草稿的当前选中上传从来源及缓存准备开始使用同一原通道绝对期限，随后凭据、签名、发送
及响应不得重置时钟。超时保持失败且不取消后续文件；实际取消由父令牌证明，不解析错误文本。
已确认成功的回执发布保持在期限等待之外，允许原子结算完成，不把后续发布耗时改报成未知上传失败。
同步操作系统读取只能在其实际返回时结算；异步调用方可到期退出，不能声称强制终止已进入内核的调用。
取消登记由唯一守卫跨准备及已知结果发布持有；请求Future丢弃不能留下幽灵登记，旧守卫不能删除
同ID的新操作。取消RPC现在等待真实结算确认后再成功返回，不能把“已请求取消”当成“可以释放文件”。
真实RPC分发等待边界已有定向回归。前端显式移除也已保留失败取消及原标识，等待确认后再释放租约和
移除原对象；新增隔离React Hook覆盖，不能当作实际浏览器整链。租约返回布尔值表示是否变更，`false`
仍是成功的幂等协调，不能当作未确认。原在途上传现在由单次许可持有，模型视图只观察/投影；陈旧视图
仅丢弃计划，不能隐式取消原上传，无任务/原回执时不得自动重传。实际来源寿命、待发送提示词整组编辑
替换及全部实际准备RPC仍须继续验证。这些单文件上传期限不等于新增全局会话时限，完整压缩及浏览器
流程仍须单独验收。
实体上传不能只持有取消登记或假设界面草稿仍存在。开始物理准备前须在资产索引事务中校验活动原意图
并取得进程内来源持有，持续到网络与已知结果发布结束；它不写草稿、回执、历史或缓存。草稿释放与清理
须跳过活动读取者，最后读取者退出后再按原持久引用回收。已存在Windows资产根的索引锁以规范物理路径
为身份，不能由同一路径的文本别名建立第二个所有者。

## 八、Bundled Index、Golden 与资源同步

### 8.1 为什么会出现 `bundled_baseline_invalid`

Manifest 内容修改后，只要以下任意一项没同步，整个 Bundled 基线就会失败：

- provider revision；
- Index 条目 revision；
- Manifest 实际字节数；
- Manifest SHA-256；
- Manifest 文件集合；
- 最低引擎契约。

APIKEY.FAN 曾因为 Manifest 已修改而 Index 仍保存旧 SHA-256 和旧字节数，状态页显示：

```text
内置回退不可用：bundled_baseline_invalid
```

不要只手改某一个摘要来让校验“通过”。应使用 CLI 重新生成完整 Index：

```powershell
rtk cargo run -p vulcan-provider-manifest-cli -- build-index crates/vulcan-runtime-assets/resources/provider-manifests/manifests --revision <index-revision> --key-id bundled-package-integrity-v1 --issued-ms <unix-ms> --expires-ms 253402300799999 --relative-prefix manifests --out crates/vulcan-runtime-assets/resources/provider-manifests/bundled-index.json
```

### 8.2 同步到运行资源

所有源码资源和测试完成后执行：

```powershell
rtk proxy powershell -NoProfile -File .\make.ps1 sync
```

随后检查源文件与 `output/resources/provider-manifests/` 对应文件的字节数和 SHA-256 完全一致。修改
Rust 路由、校验或状态逻辑后，还必须重新编译并彻底重启当前应用；仅点击同步不能替换正在运行的旧
二进制代码。

## 九、推荐验证顺序

按以下顺序执行可以最快定位问题所属层：

### 9.1 Manifest 与 Index

```powershell
rtk cargo run -p vulcan-provider-manifest-cli -- validate-all
rtk cargo run -p vulcan-provider-manifest-cli -- validate crates/vulcan-runtime-assets/resources/provider-manifests/manifests/<provider-id>.json
rtk cargo run -p vulcan-provider-manifest-cli -- compile crates/vulcan-runtime-assets/resources/provider-manifests/manifests/<provider-id>.json
rtk cargo run -p vulcan-provider-manifest-cli -- inspect crates/vulcan-runtime-assets/resources/provider-manifests/manifests/<provider-id>.json
rtk cargo run -p vulcan-provider-manifest-cli -- fixture-check
```

### 9.2 适配器与运行时

```powershell
rtk cargo test -p vulcan-provider-adapters
rtk cargo test -p vulcan-provider-adapters <provider-id>
rtk cargo test -p vulcan-agent-runtime provider_tool_bridge
```

涉及 Service Kernel 的 Bundled 资源测试若出现并行争用，应单线程复验：

```powershell
rtk cargo test -p vulcan-service-kernel provider_manifest_status -- --test-threads=1
```

### 9.3 发布门禁与工程检查

```powershell
rtk proxy powershell -NoProfile -File .\scripts\check_provider_manifest_release.ps1
rtk cargo fmt --all -- --check
rtk git diff --check
```

发布门禁可能同时检查全部供应商。如果失败来自未修改的另一供应商或并行会话，必须记录精确 fixture、
目标签名和所有者边界，不要为了让当前任务变绿而修改无关供应商。

### 9.4 运行时验收

1. 执行 `make.ps1 sync`；
2. 重新编译并彻底重启应用；
3. 状态页确认 Bundled/Active revision、SHA-256 和 layer 正确；
4. 确认没有 `bundled_baseline_invalid`、不支持的 schema 或 engine contract；
5. 添加或复用已有凭据，执行模型同步；
6. 确认模型目录等于远端发现与本地规则交集；
7. 在模型列表分别测试会话使用和内部使用；
8. 知识库和 VMM 验证内部使用模型可见；
9. 会话验证模型最终协议、思考档位和多模态输入；
10. 对每项模型扩展先查询参数，再按返回 Schema 调用；
11. 重启后复验模型用途、连接和模型扩展选择都能持久恢复。

## 十、常见错误与定位表

| 现象 | 首查 | 正确处理 |
| --- | --- | --- |
| `bundled_baseline_invalid` | Manifest revision、bytes、SHA-256、Index 文件集合 | 重新生成 Index，再跑 `validate-all` |
| 资源 JSON 有模型但 UI 没有 | source policy、`/models` 交集、Exact rule、模型级用途状态、runtime generation | 沿目录到 Workbench 投影逐层定位，不在 UI 加 fallback |
| 提示 Manifest schema/engine 不支持 | 在线服务投放了超出当前客户端能力的清单 | 拒绝激活并升级客户端；不要增加运行时兼容分支 |
| 模型可见但无法执行 | 唯一 selected protocol、verified route、凭据档作用域 | 修正精确规则，不试发其他协议 |
| 思考选项异常多 | 复制了全局枚举或相邻模型档位 | 回到该模型官方资料与中转实测，只保留精确档位 |
| 模型声明图片但不能生图 | 把图片理解输入误当输出能力 | 分离 input resources 与独立 `image_generate` 扩展 |
| 有参考图时错误进入编辑 | 用“是否有图片”决定接口 | 生成与 Mask 编辑保持两个独立工具 |
| 修改扩展多选后会话上下文断开 | 为每个模型动态注册不同工具 Schema | 保持稳定查询工具和稳定 `model + args` 执行外壳 |
| 媒体返回成功但尺寸不符 | 把请求 size 当作输出保证 | 解码实际文件并记录供应商归一化限制 |
| 请求路径 404 | base URL 与 endpoint path 重复 `/v1`，或模型只在目录存在、后台未声明 | 检查最终 URL并真实调用；目录存在不能代替执行证据 |
| 浏览器授权页提示缺少必填参数 | Windows 使用 `cmd /C start` 打开包含 `&` 的授权 URL，查询串被 `cmd.exe` 当作命令分隔符截断 | 外部 URL 必须作为单个进程参数直接交给系统 URL Handler；用包含 `callback` 与 `state` 的回归测试核对完整参数 |
| 授权状态提示 `Browser callback requires POST` | 浏览器发送跨域 JSON `POST` 前的 `OPTIONS` 预检被当成终态错误，监听器在真实回调到达前关闭 | 对精确回调路径返回有界 CORS `204` 并继续等待；不得把预检当凭据，也不得改成接受 `GET` 交付秘密 |
| `output` 仍是旧配置 | 只修改源码资源未同步，或运行进程仍加载旧二进制 | 执行 `make.ps1 sync`，重新编译并重启 |
| `fixture-check` 报其他供应商签名不一致 | 并行任务修改了其他生产 target 或 fixture | 报告边界，不越权修复无关模块 |

## 十一、OpenCode Go 与 APIKEY.FAN 的关键经验

### 11.1 OpenCode Go

- Console API Key / OAuth 与旧 Go Key 使用独立服务入口；设备授权、工作区绑定及三类额度窗口见
  [已核对协议](providers/opencode-go-verified-facts.md)，不得将 OAuth 令牌直接发送到旧 Key 网关。

- 入口必须分别验证 Chat、Anthropic Messages 和 Responses；
- 模型参数优先取 OpenCode 客户端源码和 models.dev，再用真实请求确认网关行为；
- 同一 DeepSeek 模型在不同协议上的请求投影可以不同，应使用 Request Profile 表达；
- 一个模型不支持 Responses 时，只从该模型规则移除 Responses，不能删除供应商整个 Responses route；
- 新模型先进入静态目录和 Exact rule，再配置模态、上下文、输出、思考和搜索声明。

### 11.2 APIKEY.FAN

- 它是按 API Key 分组的中转站，不是一个 Key 自动拥有全部模型；
- 实际启用模型必须取 `/models` 与本地预设规则的交集；
- 官方模型能力与中转转发能力分开验证；
- `gpt-image-2` 图片生成和 Mask 图片编辑是两个独立扩展；
- Grok 图片、GPT 图片和 Grok 视频虽然共享供应商，参数与生命周期完全不同；
- 列表返回但真实接口 404 的媒体模型必须忽略；
- 视频异步任务必须验证提交、轮询、文件信息和实际下载四段协议；Provider 已返回成功后，任一结果
  获取、解码或资源安全错误必须立即终态化，不能继续按运行中任务轮询；
- Gemini 必须显式考虑 `maxOutputTokens`，避免网关默认 8192；
- GLM 即使接受 `reasoning.effort: none` 也可能继续输出 reasoning，不能据此声明可关闭；
- Kimi 的本地默认输出预算与官方硬上限分开记录；
- DeepSeek 上下文、最大输出、模态和思考档位按官方配置，中转站只决定入口和实际转发是否成立；
- Web Search 只有在当前中转路由真实验证后才能声明为供应商原生工具，宿主搜索工具不能伪装成模型原生搜索。

### 11.3 CommandCode

- CommandCode 的 Provider API 不能把“兼容两种协议”解释为“每个模型可任选入口”；入口由模型系列严格决定。
- CommandCode 1.26.0 的网页授权通过跨域 JSON `POST` 回调本地端口，浏览器会先发送 `OPTIONS`；宿主必须返回 CORS `204` 后继续等待真实 `POST`。
- CommandCode 1.26.0 不声明固定 API Key 前缀，而是将非空密钥提交账号接口验证；不得根据旧报告为手动 Key 或网页登录凭据配置 `key_prefix`。
- `claude-*` 模型只声明 `anthropic-messages.v1`，最终路径为 `/provider/v1/messages`。
- 其他已验证的 OpenAI 与 OSS 模型只声明 `openai-chat-completions.v1`，最终路径为 `/provider/v1/chat/completions`。
- Manifest 可以同时声明两条 Provider route，但每条 Exact model rule 必须只有一个 `selected_protocol` 和一个 `supported_protocols` 成员；禁止跨系列探测、重试或降级。
- `/models` 只负责发现当前账户可见身份；本地 Exact rule、套餐规则、余额解锁规则与发现结果共同决定有效目录，不能让未声明的新模型自动进入。
- 用量窗口的 `cap` 与 `resetAt` 保持 `u64`；真实 `used` 可能是精确小数，必须无损保存并用十进制规则计算剩余额度，不能先转浮点或强制解析为整数。
- GOAT Chat 路由的思考档位不包含 `none` 或 `minimal`。除报告明确列出更窄档位的模型外，有独立思考通道的模型统一声明 `low/medium/high/xhigh/max`；`moonshotai/Kimi-K2.5` 与 `nvidia/nemotron-3-ultra-550b-a55b` 仅在正文内推理，不声明可选思考模式。
- `reasoning.summary` 与其他 Chat 供应商一致，作为仅展示思考进入思考弹窗、思考记录与转录，但绝不进入重放；`commandcode.chat.reasoning-summary.v1` 只删除顶层 `reasoning` 镜像，防止镜像变成可重放思考。`reasoning.encrypted` 和只有签名的思考详情属于不透明协议数据，只能驱动通用“正在思考（可选 Token）”临时状态，既不能展示载荷，也不能持久化。展示优先级为“非空完整思考 > 可读 summary > 加密进度”：完整思考出现后丢弃当前请求之后的摘要，可读 summary 取代尚无语义内容的加密进度；不得在运行时按 CommandCode 或模型身份硬编码；空详情不得创建空思考块。
- CommandCode 用量窗口必须按稳定 ID 映射本地化标签，并复用 Kimi 的“套餐/余额 + 额度窗口”结构；美元额度统一按货币精度展示，不能把上游高精度小数直接暴露给用户。
- 会话账号选择器必须把 `command_code_usage` 识别为额度窗口能力，使已刷新账号在会话区域显示剩余值；设置页和会话区必须消费同一份规范化用量快照，不能分别实现供应商专属结构。

### 11.4 OpenAI Codex 订阅

- 用量原语 `open_ai_codex_usage` 以 Codex 登录凭据（Bearer + `chatgpt-account-id`）请求固定入口 `https://chatgpt.com/backend-api/wham/usage`；响应形状以上游 `codex-rs/codex-backend-openapi-models` 的 `RateLimitStatusPayload` 为准。
- 上游只报告整数 `used_percent`，不公开额度本身；窗口只填 `remaining_percent`、`duration_seconds`（`limit_window_seconds`）与 `resets_at`（`reset_at`，Unix 秒），不得臆造 `used`/`total`。主限额桶窗口 ID 为 `codex:primary`/`codex:secondary`，`additional_rate_limits` 为 `<metered_feature>:primary|secondary` 并以 `limit_name` 作为 `bucket_name`；界面按时长派生周期标签。
- 模型发现声明的 `client_version` 由 `OPENAI_CODEX_DISCOVERY_CLIENT_VERSION` 唯一定义；账号目录会隐藏 `minimal_client_version` 更高的模型，服务端实际最低版本可能高于上游 `models.json` 的标注（GPT-6.1 Sol 标注 0.153.0，2026-10-23 实测 0.155.0 仍隐藏、0.160.0 起返回），因此该常量跟随本机已发布官方客户端（`~/.codex/models_cache.json` 的 `client_version`）。
- 速度档位（Fast/Ultrafast）由账号目录逐模型的 `service_tiers` 决定：发现阶段把除 `default` 外列出的档位 ID（如 `priority`、`ultrafast`）注册为模型参数 `codex.service_tier`（`CODEX_SERVICE_TIER_PARAMETER_ID` 唯一定义，默认交给供应商），套餐不提供时不注册、界面不显示。用户在模型设置中选择后，Codex 路由的请求 Profile `openai-codex.service-tier.v1` 把值写入请求体 `service_tier`；未选择时省略该字段，等同官方客户端的标准档位。档位名称只走语言目录，不使用上游展示文本。

## 十二、五轮连续审核要求

系统供应商、协议路由或模型扩展发生较大变更时，完成实现后执行连续五轮聚焦审核。任一轮发现问题，
修复后从第一轮重新计数：

1. **R1：Manifest 与 Index 完整性。** Schema、revision、bytes、SHA-256、engine contract、文件集合。
2. **R2：目录与协议。** 模型身份、发现交集、Exact rule、唯一协议、上下文、输出、模态和思考。
3. **R3：运行时权威。** 凭据无协议字段、选择冻结、Manifest 版本门、retry、重启持久化和失败关闭。
4. **R4：Provider Tool 与媒体。** 直连/扩展分类、系统原生限制、稳定 Schema、上传、异步和资源输出。
5. **R5：状态与发布。** 状态徽标、Golden、Service 投影、`output` 同步、格式化、目标 crate 测试和发布门禁。

只有连续五轮没有发现本供应商范围内的问题，才可以宣布接入完成。

## 十三、完成定义

- [ ] 供应商和模型事实都有可追溯证据；
- [ ] Manifest、文件名、provider ID 和 Index 身份一致；
- [ ] 每个模型有精确目录身份、规则和唯一执行协议；
- [ ] 上下文、最大输出、输入输出模态和思考档位没有猜测；
- [ ] 中转供应商的目录使用当前凭据发现结果与本地规则交集；
- [ ] 凭据连接不保存协议，执行只接受非空模型级 Manifest 绑定；
- [ ] Registry、状态投影和运行时不存在系统供应商旧协议或双轨回退；
- [ ] 在线清单按客户端能力投放，不兼容版本要求升级客户端；
- [ ] 会话使用与内部使用由模型列表独立管理；
- [ ] 直连能力与九项模型扩展分类正确；
- [ ] 每个模型扩展有英文 AI 描述、精确 Schema、完整限制和证据；
- [ ] 图片生成、参考图生成和 Mask 图片编辑边界正确；
- [ ] 媒体模型的请求、响应、资源解码与异步生命周期有 fixture；
- [ ] `validate-all`、fixture、适配器、Agent Runtime、Service 和 Golden 测试通过；
- [ ] Bundled Index 已重新生成并验证；
- [ ] `make.ps1 sync` 后源码与 `output` 字节一致；
- [ ] 重编译、重启和真实 UI 验收通过；
- [ ] 连续五轮审核没有发现本供应商范围内的问题；
- [ ] 未修改并行任务或其他供应商拥有的无关代码。
