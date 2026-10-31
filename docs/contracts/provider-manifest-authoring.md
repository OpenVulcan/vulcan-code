# Provider Manifest 作者、测试与运维手册

关联文档：

- `docs/contracts/adding-system-providers.md`。

它说明如何只通过受签名 JSON 发布供应商与模型事实，以及何时必须修改宿主原语。本文不替代
`provider-manifests/schema/provider-manifest-v2.schema.json`；字段形状以 Schema 和 Rust 编译器的共同校验结果为准。

## 1. 权威来源与不可跨越的边界

系统供应商源清单位于：

```text
crates/vulcan-runtime-assets/resources/provider-manifests/manifests/<provider>.json
```

同一随包发布批次由
`crates/vulcan-runtime-assets/resources/provider-manifests/bundled-index.json` 固定。Index 必须与目录
文件集合完全一致，并逐份绑定字节数、SHA-256、供应商身份、Manifest 修订和最低引擎契约。运行时在
编译任何可发布目录前整体校验该 Index；缺文件、多文件或任一固定值漂移都会使随包信任基线关闭
失败，不会形成部分运行时代际。

配套资源位于：

```text
crates/vulcan-runtime-assets/resources/provider-manifests/schema/provider-manifest-v2.schema.json
crates/vulcan-runtime-assets/resources/provider-manifests/schema/provider-index-v1.schema.json
crates/vulcan-runtime-assets/resources/provider-manifests/fixtures/
```

运行时唯一权威是一次性编译完成的不可变 `ResolvedProviderCatalog`。清单可声明数据和引用已注册
原语，但不能下载代码、执行脚本、扩展宿主权限或猜测供应商协议。

职责边界如下：

| 内容 | 所有者 | 是否可仅更新 JSON |
| --- | --- | --- |
| 模型身份、显示名、默认启用、模型级协议 | Manifest | 是 |
| 请求/响应/错误/usage 字段映射 | Manifest 中的封闭 Profile | 是 |
| 已支持 Operation、Upload Channel 与异步生命周期的组合 | Manifest | 是 |
| HTTP、流式解码、凭据投影、文件读取、资源上限、签名算法 | 宿主已注册原语 | 否 |
| 新网络权限、新认证机制、新 Wire 编解码或新任意转换能力 | 宿主代码及安全审核 | 否 |
| APIKEY.FUN 模型到协议事实 | APIKEY.FUN Manifest 与已验证事实记录 | 新事实仍需先验证，禁止从分组或相邻模型猜测 |

自定义供应商以 `custom:` 命名空间和独立文件持久化。它可以有多个实例、多个启用协议、发现模型
和手动模型；实例之间的端点、凭据、模型和选择不得串用。

自定义供应商的模型目录只配置一次，与会话可用协议集合独立。设置请求用 `modelDiscoveryProtocol`
选择 `open_ai`、`anthropic` 或 `gemini`，默认值在界面明确显示为 OpenAI；持久化权威是入口
`model_catalog.discovery_kind` 与 `model_catalog.path`。目录使用入口共享地址、鉴权和非敏感请求头，
只执行所选发现链（有分页时继续同一链），再为已声明会话协议投影模型变体。Chat 与 Responses 共用
OpenAI 目录格式；仅启用 Anthropic 会话协议也可以使用 OpenAI 目录。会话路由的地址、鉴权和请求头
覆盖只用于模型调用，路由级目录字段不再决定自定义供应商的发现行为，编辑器不再提交这些字段。
HTTP、解析或分页失败直接保留错误，绝不改用另一个目录协议；手工模式不读取凭据也不访问远端。
标准目录路径追加到入口基础地址之后；Gemini 的版本前缀由基础地址提供，例如 `.../v1beta`
请求 `.../v1beta/models`，不重复追加版本。显式自定义目录路径同样保留基础地址已有前缀，分页只增加查询参数。
Anthropic 目录请求复用 LLM 核心的协议版本头声明，显式入口版本头优先，认证仍严格按用户选择发送。
实现与回归位于 `client/model_discovery`、`custom_provider`、`runtime/catalog_sources`、独立清单保存重载
测试和 WebUI `custom-provider-catalog-draft`；系统供应商仍按签名清单的原有目录声明执行。

### 1.1 自定义供应商文件位置与格式

宿主先解析应用数据根目录，再把每个自定义供应商保存到：

```text
<application-data-root>/provider-manifests/custom/<percent-encoded-provider-id>.json
```

文件名是 `custom:` 供应商 ID 的可逆百分号编码，不允许调用方直接拼接路径；完整 ID 最多 64 个
ASCII 字节，后缀只允许 `[A-Za-z0-9._-]`，单文件上限为 4 MiB。
当前唯一可写格式是完整版本化文档：

```json
{
  "schema_version": "vulcan.custom-provider-manifest.v1",
  "provider_manifest": {
    "schema_version": "vulcan.provider-manifest.v2",
    "provider_id": "custom:example"
  },
  "local_overrides": {}
}
```

`provider_manifest` 保存完整供应商规则、协议路由和目录声明；`local_overrides` 保存该供应商持有的
手工模型、逐模型协议偏好、启用差异及模型执行覆盖。连接、凭据、会话与推理配置档仍保留在中央
连接文件中；中央文件只持久化 `custom_provider_file_refs`，不会再次写入自定义供应商定义。

加载只接受当前 `vulcan.custom-provider-manifest.v1` 文档；缺少该 Schema 标记的任何形态（包括仅含
描述符的旧文档）都作为该供应商的读取失败隔离，不迁移、不改写。中央文件内嵌 `custom_providers` 定义时
整份中央文件被拒绝。加载过程绝不改写中央文件或独立文件。

中央配置与独立文件的每次保存通过 `provider-manifests/custom-provider-transaction.v1.json` 保存完整
before/target 镜像；进程中断后先重放同一 target 镜像，再执行补水。事务恢复失败会阻止激活，不会静默
回退到另一供应商或丢弃该供应商。

独立自定义 Manifest 的读取、格式、身份、Schema 或本地覆盖校验失败只隔离精确所属供应商；
其他已验证供应商仍可启动和执行，且不得回退使用同 ID 的过期中央内嵌描述。Service 快照最多暴露 256 条
隔离诊断，供应商展示身份上限 256 字符、单条修复消息上限 2048 字符，并单独报告真实总数与是否截断。
隔离存续期间，中央引用和原始独立文件保持不变；跨文件事务会把被隔离文件的原始字节原样带入 target 镜像，
因此其他健康供应商及公共 AI 设置仍可保存，且不会把失败源误判为已删除供应商。被隔离供应商不会参与目录、路由、
用量或工具投影；操作者修复或移除报告的精确源后，需重启 Service 重新加载该供应商。

## 2. 根结构与标识规则

每份系统清单使用 `schema_version: "vulcan.provider-manifest.v2"`，根字段包括：

| 字段 | 作用 | 关键约束 |
| --- | --- | --- |
| `provider_id` | 稳定供应商标识 | 必须等于文件身份；系统清单使用 1～64 个 `[A-Za-z0-9._-]` ASCII 字节且不得使用 `custom:` |
| `revision` | 供应商内单调修订 | 远端必须高于本地有效版本；同版本不同摘要拒绝 |
| `published_at` | 审计时间 | 不参与协议猜测或版本排序 |
| `engine_contract` | 最低引擎和所需原语 | 只能引用宿主已注册的协议、执行、上传、转换原语 |
| `display` | 名称、类别、目录展示和推广信息 | 不得借远端更新扩大信任权限 |
| `credential_registration` | 凭据录入宿主原语 | 默认 `direct`；系统清单只能选择已注册且随包授权的原语 |
| `services` | 服务入口、凭据档、目录和协议路由 | 标识在供应商内唯一；引用必须闭合 |
| `model_defaults` | 未覆盖模型的目录与绑定策略 | `unknown_model_policy` 决定未知模型能否绑定默认协议 |
| `model_rules` | Exact/Prefix 模型覆盖 | 规则、作用域和候选协议必须确定性解析 |
| 四类 Profile | request/response/error/usage 规范化 | 只能使用封闭步骤；所有引用必须存在 |
| `upload_channels` / `operations` / `tool_targets` | 多模态和 Provider Tool | 必须引用已注册原语、入口和凭据角色 |
| `provenance` | 来源、证据与验证状态 | 未验证事实必须如实标注，不能用文案代替 fixture |

所有数组和字符串均有数量/长度上限。未知字段由 `deny_unknown_fields` 和 JSON Schema 双重拒绝。
不要靠增加“兼容字段”绕过校验；先确认真实上游契约，再扩展闭合 Schema。

### 2.1 引擎契约必须覆盖实际依赖

`engine_contract` 不是展示信息，而是候选清单激活前的闭包声明。清单中实际使用的每一项能力都
必须同时出现在对应的 `required_*` 集合中；只要缺少一项，验证就会失败关闭：

1. 模型默认或规则引用的协议必须列入 `required_protocol_profiles`；
2. Operation 或 Tool Target 展开后使用的执行原语必须列入 `required_execution_primitives`；
3. Upload Channel 使用的上传原语必须列入 `required_upload_primitives`；
4. Request/Response Profile 内每个 `MappingStep` 必须列入 `required_transformers`；Error 与
   Usage Profile 使用各自的封闭结构校验，不伪装成 Mapping Transformer。

当前宿主已注册的映射转换原语如下，标识与 Profile 中映射步骤的序列化名称完全一致：

```text
set_literal
set_default_if_missing
copy_json_pointer
rename_json_pointer
delete_json_pointer
fit_thinking_budget
enum_map
wrap_singleton_array
unwrap_singleton_array
string_template
copy_request_field
conditional_set
delete_if_equals
delete_unless_equals
remove_if_empty
reject_if_present
reject_if_all_present
rebuild_json_template
```

`reject_if_all_present` 使用 2 到 8 条带类型来源条件；只有全部条件存在才失败关闭。来源必须是
绝对 JSON Pointer，或已注册的 `@request_field`。它用于把“两个规范字段不能同时出现”等交叉字段
约束留在供应商 Profile 中，不允许生产代码再按供应商 ID 硬编码冲突。

`fit_thinking_budget` 仅用于请求投影。`path` 指向可选的无符号思考预算，`limit_path` 指向同一
请求体中的可选输出上限，`minimum` 声明正数预算下限。预算存在时必须为不小于下限的整数；
输出上限存在时也必须是整数且严格大于预算下限，预算调整为原预算与 `max(minimum, floor(limit / 2))`
中的较小值。预算或输出上限缺失时不补值；格式错误或无法同时容纳最低思考预算及答案时显式失败。
此步骤用于表达 OpenCode v2 的预算与输出长度联动，不在宿主中硬编码供应商或模型身份。

新增映射步骤时，必须先实现并注册宿主原语，再扩展 Schema、验证器、作者手册和可执行夹具；不得
只在运行时代码中识别步骤而遗漏 `engine_contract`。夹具使用的临时执行、上传或转换原语也必须由
夹具声明并合并进被测清单的引擎契约，确保测试不会依赖隐式能力。

`credential_registration` 是显式宿主原语，不得根据 `provider_id`、请求 projector 或 Key 前缀
反推。目前封闭取值如下：

- `direct`：直接保存经过格式校验的凭据；所有自定义供应商固定使用此值；
- `mini_max_regional_api_key`：先调用已注册的 MiniMax 区域识别流程，再把精确结果写入连接；仅
  随包 MiniMax 基线可授权。

新增凭据录入方式意味着新增宿主能力与安全边界，必须升级软件并审核，不能只发布 JSON。远端
清单不能改变随包 `credential_registration`，也不能改变凭据档的 `account_usage_query` 原语。

### 2.2 用途声明是初始默认值，不是能力上限

Schema v2 将凭据档的用途字段固定为 `default_usage_scopes`。它只负责在用户尚未作出模型级选择时，
提供会话发布与非会话内部使用的初始默认值；它不是供应商、套餐或凭据对知识库、VMM 的授权上限。
用户在模型列表保存的显式状态始终优先于此默认值。

会话使用与内部使用是两个独立状态：关闭会话使用不会从模型管理目录或账号可达目录移除模型，
也不会关闭知识库与 VMM；关闭内部使用同样不会影响会话模型选择。知识库和 VMM 共用一个模型级
内部使用开关，但运行时仍用 `internal_background` 与 `external_service` 区分调度、鉴权和审计。
`interactive_agent` 不能作为内部推理路由用途。

清单热更新可以调整 `default_usage_scopes`，因为它属于数据默认值而非信任权限扩张；已经保存的用户
显式状态不会被热更新覆盖。Schema v1 的 `allowed_usage_scopes` 语义不兼容，加载器必须直接拒绝，
不得探测或降级解析。

凭证的运行时启用状态由中央连接配置持有，不属于 Manifest 默认值。停用连接只写入稀疏的
`disabled_connection_ids`，保留凭据秘密、标签、入口、凭据档、模型用途和自定义模型配置；路由选择、
模型目录、usage 刷新、Provider Tool、媒体候选及会话待生效选择都必须把该连接视为不存在。若它正被
全局选择使用，Service 在同一配置事务内选择仍可用的确定性替代项；已加载会话保留用户拥有的精确
历史路由事实，但撤销进程内租约并清除指向该连接的待生效选择。重新启用只恢复参与资格，不伪造先前
默认账号或自动改写用户用途。

## 3. 服务入口、目录来源与模型热新增

一个 `service_entry_id` 表示一个真实产品入口，例如公开 API、Coding Plan 或 Provider Tools。入口可拥有：

- 独立 `base_url`、固定非秘密请求头和请求 projector；
- 可选 `session_header`，其值为接收当前公共会话 ID 的请求头名称；缺省时不注入会话请求头；
- 一个或多个 `credential_profiles`；
- `model_catalog`；
- 一个或多个已验证 `protocol_routes`；
- 可选入口级模型默认覆盖。

`model_defaults_override` 可以分别覆盖 `catalog_membership_policy`、`unknown_model_policy`、
`supported_protocols` 与 `selected_protocol`。缺省字段继承供应商根默认值。编译器将生效策略冻结到
逐入口默认值中，目录成员投影、发现授权、协议绑定、默认能力和 Profile 继承必须读取同一入口的
冻结值；不能用供应商根策略覆盖入口声明。远端更新不能通过删除覆盖字段或修改根默认值，扩大
随包入口的生效成员或绑定权限。

`request_projector` 只允许供应商无关的三种封闭原语：

- `direct`：协议编码后不进入供应商兼容投影；自定义供应商固定使用此值；
- `compatible`：供应商差异完全由冻结的 Request Profile 表达；
- `routed_compatible`：在 `compatible` 基础上允许已注册且明确禁止 fallback 的二级路由。

不得新增以供应商命名的 projector。新增字段差异应使用已注册 Mapping Transformer；新增 wire 协议
或新的二级路由握手才属于需要升级宿主的引擎能力。

`session_header` 是服务入口级的动态请求头声明，不是固定请求头值，也不能通过 Request Profile 写入。
宿主在最终 HTTP 与 WebSocket 请求头投影阶段，将当前公共会话 ID 写入该名称；没有会话 ID 时不发送，
未声明该字段时保持原有行为。若该名称已经存在于请求头，或属于宿主持有的鉴权、传输、身份请求头，
运行时忽略 `session_header`，绝不覆盖既有值；其他请求头名称仍受宿主边界保护。

### 3.1 User-Agent 与应用归因头边界

最终推理请求的 `User-Agent` 由宿主按冻结的模型级协议统一投影，不能由供应商名称、请求正文或
可变 Manifest 猜测：

- 只有 `openai-codex-responses.v1` 使用已采纳 Codex 客户端的精确 `User-Agent`；
- 其他全部协议统一使用 VulcanCode 自有 `User-Agent`；
- Codex 规范请求只作为跨协议转换的内部正文基线时，必须丢弃 Codex `originator`、会话标识和
  `User-Agent`，只保留目标协议自身要求的请求头；
- HTTP 与 WebSocket 传输必须共用同一最终请求头投影，不能在某个传输分支重新覆盖协议身份。

OpenRouter 应用归因由其随包 Manifest 的固定非秘密请求头声明。当前闭合契约同时发送
`HTTP-Referer`、`X-OpenRouter-Title` 与 `X-OpenRouter-Categories`；分类只能使用官方识别值，单次
最多两个、使用逗号分隔。VulcanCode 当前按独立桌面编程应用登记为 `programming-app`。

`model_catalog.source_policy` 明确模型从哪里来：

- `manifest_only`：只使用清单身份；
- `discovery_only`：只使用远端发现证据；
- `manifest_and_discovery`：合并清单与发现；
- `manual_only`：只使用用户手动模型；
- `discovery_and_manual`：合并发现与手动模型；
- `manifest_discovery_and_manual`：合并三者。

内置 OpenRouter 的 `public-api` 使用 `manifest_and_discovery` 与 `open_router` 发现器保留远端模型事实，
但 `catalog_membership_policy: declared_only` 使账号列表只展示清单明确列出的文本会话模型。清单中的模型
规则决定初始勾选状态，账号开关可覆盖初始状态；远端发现结果和账号勾选均不能单独增加模型身份。用户需要在
供应商层手工添加其他文本会话模型；向量、重排及图片、视频、音频等专用输出模型不能作为普通对话模型加入。
新增系统推荐模型时，必须同时更新静态身份目录与精确规则。

账号未作出选择时，模型逐一继承当前签名清单的默认勾选状态；即使供应商总体默认开启，某个模型的
默认关闭仍然生效。单模型勾选只覆盖该模型 ID，清单升级时新增模型继续继承新清单默认值；账号明确
执行全部开启或全部关闭后，该账号的全局决定才覆盖未来模型。系统模型被移除时清理对应单模型决定；
同 ID 系统模型接管手工定义时保留原账号的有效勾选。

`model_catalog.discovery_kind: manifest_static` 表示不发远程目录请求，模型身份、顺序与基础能力完全
由当前已编译 Manifest 持有。它的封闭约束为：

- `model_catalog.models` 必须出现；非空数组是权威身份与顺序，空数组明确表示没有模型；
- 每个身份必须有覆盖精确服务入口与凭据档的模型规则，否则整份清单编译失败；
- `path`、路由级发现路径与发现基础 URL 均禁止，避免非网络目录伪装网络权限；
- 模型从空能力开始，只应用 Manifest 规则，不继承 Rust 静态表或公共模型目录；
- 规则的 `supported_protocols` 决定该模型实际产生哪些协议变体，未声明协议不会被试发或降级；
- 新模型必须同时增加身份和规则；只增加身份会封闭失败。

Service 将入口的目录来源聚合为 `static`、`dynamic`、`manual` 或 `unavailable`，并把同一强类型
结果投影给 WebUI。只有 `dynamic` 允许“同步模型”：静态、手工及不可用目录既不显示同步按钮，后端
RPC 也必须在凭据校验和网络请求之前拒绝，以防旧客户端或直接调用绕过界面。该门禁来自精确入口的
已编译 Manifest，不得由前端根据供应商名称或模型列表猜测。

其他远程/手工发现类别可以在没有 Manifest 身份清单时省略 `models`；一旦提供非空或空数组，
其 presence 语义仍分别表示“显式身份清单”或“显式没有 Manifest 身份”，不能靠宿主猜测补齐。

同一服务入口包含多个凭据档、但某个声明身份只属于其中一部分产品套餐时，在该
`model_catalog.models[]` 项上声明 `credential_profile_ids`。省略或传空数组表示该服务入口的全部
凭据档；非空数组中的每个 ID 必须由同一服务入口声明。运行时只把该身份合并进匹配凭据档的目录，
并仍要求模型规则覆盖相同的服务入口和凭据档。此作用域用于表达 MiniMax Code 一类由远端托管目录
下发、但标准 `/models` 暂未返回的套餐模型；不得据此把未验证模型扩展到其他凭据档。

新增普通模型且不升级软件的标准步骤：

1. 在目标入口的 `model_catalog.models` 添加精确 `model_id` 和 `display_name`；若身份仅属于部分
   凭据档，同时声明精确 `credential_profile_ids`；
2. 添加 Exact `model_rules`，声明可用协议和唯一 `selected_protocol`；
3. 只写已有证据支持的 capabilities/Profile/operation 引用；
4. 提升供应商 `revision` 和 `published_at`；
5. 运行 Schema、编译、fixture、semantic diff 和 dry-run；
6. 构建并签名 Index，发布清单后触发同步；
7. 验证新 generation 的 Workbench 出现模型，旧会话仍使用旧快照。

仅添加模型身份不足以证明协议。`declared_rule_required` 下，未命中规则的模型必须保持不可执行；
`default_binding_allowed` 也必须有显式默认协议，运行时不会试发请求探测。

### 3.2 账号套餐与额度驱动的模型访问

供应商客户端需要根据在线套餐和额度在本地过滤模型时，禁止在 Rust 中按供应商 ID、套餐名称或
模型 ID 编写条件分支。凭据配置档必须显式声明 `account_model_access`，模型规则按需声明
`account_access_groups`：

```json
{
  "account_usage_query": "provider_usage_primitive",
  "account_model_access": {
    "plan_identity_field": "plan_id",
    "plan_rules": {
      "provider-plan-id": {
        "allowed_model_groups": ["standard"],
        "blocked_model_ids": ["model-with-plan-exclusion"]
      }
    },
    "unrestricted_when_quota_remaining": ["purchased-credits"]
  }
}
```

- `plan_identity_field` 明确选择用量快照中的 `plan_id` 或 `plan_name`，不能在运行时猜测；
- `plan_rules` 的键是供应商确认的精确套餐身份，值声明允许分组和精确模型排除项；
- 模型没有 `account_access_groups` 时不臆造类别，保持供应商原生的未分类模型行为；
- 任一 `unrestricted_when_quota_remaining` 窗口剩余额度大于零时跳过套餐分组过滤；
- 套餐或额度刷新后重建对应凭据的生效目录；暂时查询失败保留最后一次确认的访问证据；
- 未声明 `account_model_access` 的供应商完全不进入该链。APIKEY.FUN 的 Key 分组继续只使用
  `/models ∩ 精确本地规则`，不得与套餐投影混用。

### 3.3 凭据授权声明

凭据获取能力由 `credential_profiles[].authorization` 声明，不再使用供应商专属的
`oauth_provider` 字段。模式与声明必须精确配对：

- `credential_mode: "token"`：不得包含 `authorization`；
- `credential_mode: "auth"`：必须包含一个 `authorization`；
- 自定义供应商不得声明任何宿主管理授权。

已审核的 OAuth 与 Device Flow 使用注册原语：

```json
{
  "credential_mode": "auth",
  "authorization": {
    "kind": "registered_oauth",
    "provider": "kimi_coding_plan"
  }
}
```

Kimi 与 MiniMax 虽然共享这一声明形状，但仍分别走宿主中已审核的 Device Code、PKCE、轮询、刷新、
设备身份和区域资源 URL 校验。作者不能把这些协议改写成未经验证的通用字段映射。

OpenCode Console 使用 `opencode_console` 注册设备授权，凭据固定工作区，并由 `opencode_go`
查询真实套餐窗口；OAuth 另外查询独立余额，Console API Key 只查套餐窗口。
请求使用 Vulcan Code 自有客户端身份；Console 凭据与旧 Go Key 路由分开，
具体协议和同步触发点见 [OpenCode Go 协议记录](providers/opencode-go-verified-facts.md)。

LLM 运行时只冻结连接、秘密引用、模型、协议和路由，不把可续期 Access Token 冻结到完整智能体执行结束。
每次真实 LLM Provider 请求前，宿主必须为同一个 `secret_reference` 重读最新修订版；其他请求完成 OAuth
刷新后，当前长任务的下一次请求必须立即使用新 Token。上游对可安全重放且尚未产生语义输出或副作用
的请求返回 401 时，宿主允许为同一账号强制刷新并立即重放一次；静态 API Key、浏览器回调静态秘密、
第二次 401、跨账号切换和协议降级均不进入该恢复链。

供应商网页把凭据直接 POST 到本地回调时，可使用声明式 `browser_callback`。该 Profile 必须同时固定：

- 无 query、fragment、用户信息和非默认端口的 HTTPS 授权 URL；
- callback 与 State 查询参数名；
- `localhost` 或 `127.0.0.1` 回环 URL、精确路径、最多 32 个候选端口；
- 30 至 1800 秒超时、最多 65536 字节 JSON 正文；
- 秘密、State、账号主体、用户名、凭据标签和可选拒绝字段的精确 RFC 6901 JSON Pointer；
- `api_key` 或 `bearer` 秘密投影。

回调凭据只接受精确路径上的 `POST application/json`、单一 `Content-Length`，拒绝
`Transfer-Encoding`；跨域授权页可以先对精确路径发送 `OPTIONS` 预检，宿主返回精确 Origin 的 CORS
`204` 后必须继续等待真正的 `POST`。State 不匹配返回 403，供应商主动拒绝可以被浏览器确认，但授权
事务必须失败。
宿主把事务绑定到 provider/service/profile 三元组及完整授权声明指纹，提交时根据当前 Manifest
重新计算并校验；任一身份、URL、监听约束或响应映射变化都会使旧事务失效，禁止跨凭据档或跨声明
复用成功事务。

## 4. 模型规则、作用域与 presence 继承

`model_rules` 支持 Exact 和 Prefix matcher。确定性优先级为：

1. Exact 高于 Prefix；
2. 更具体的入口/凭据作用域高于宽作用域；
3. Prefix 更长者优先；
4. 同等条件由稳定声明顺序和冲突验证保证唯一结果。

作用域可限定 `service_entry_ids` 与 `credential_profile_ids`。同一模型在不同产品、地区或凭据档
下协议不同，必须写不同作用域规则，不得在代码里读取供应商名称分支。

补丁字段严格区分三种状态：

| JSON 状态 | 语义 |
| --- | --- |
| 字段省略 | 继承上一层；provenance 保留上一来源 |
| 显式空数组/空对象 | 清空集合或对象；不是“未配置” |
| 允许可空字段显式 `null` | 清除可选值；不是继承 |

能力解析顺序是供应商默认、入口覆盖、模型规则、协议作用域覆盖。编译结果为每个字段保存来源，
生产消费者只依据 provenance 中确实出现的字段应用补丁，不能用 `Unknown`、空数组或 `None` 猜测
作者是否想覆盖。

`capability_patch.structured_output` 使用封闭三态：`json_object`、`json_schema`、`unsupported`。
省略继续遵循 presence 继承；全部层均未声明时，运行时基线为 `json_object`。只有官方或真实请求证据
明确证明精确模型路由不支持稳定 JSON 时才写 `unsupported`，不得把远端目录省略字段解释成不支持。
`json_schema` 表示请求可以携带精确 Schema，`json_object` 表示只能约束为 JSON 对象；两者都会在宿主
再次执行固定 Schema 校验。系统供应商的输入资源、结构化输出、思考支持、思考控制、思考档位与请求
投影都是 Manifest 冻结事实，设置页只能只读展示，不得由用户覆盖；只有自定义供应商允许声明这些能力。
系统模型的上下文与输出能力上限同样由 Manifest 冻结；用户不能在供应商级或模型级覆盖。
供应商级手工新增的会话模型以其手工定义为能力来源，可修改手工定义本身。
是否允许模型进入多模态理解候选列表仍由独立的用途开关决定。

模型规则中的 `usage_profile_refs`、`request_profile_refs`、`response_profile_refs` 与
`operation_refs` 同样遵循上述 presence 语义。显式空 `usage_profile_refs` 清除供应商专属统计特例
后仍恢复所选协议的标准 usage 基线；显式空 request/response/operation 引用则清为空集合。若只是要
继承路由或供应商默认，必须省略字段，不要写样板空数组。

## 5. 模型级唯一协议与自定义供应商规则

角色、指令合并与跨协议历史恢复必须遵守以下边界：原角色保存在规范事件及来源元数据中，
不得依赖模型正文标签恢复，也不得将未实现转换的协议直接发送为 Responses 正文。

系统供应商的协议只来自编译后的 Manifest：

- 命中规则时必须得到一个 `selected_protocol`；
- 未命中规则时，只有 `default_binding_allowed` 和显式默认协议才能绑定；
- 用户单独新增的文本输出会话模型是独立的用户授权来源：仅在精确系统服务入口、已验证协议路由
  和显式模型绑定一致时参与目录；不把用户声明改写成 Manifest 规则，命中签名规则时仍以签名协议
  为准；
- 旧连接上的 `selected_protocol` 不参与系统模型绑定；
- 一个服务可以同时声明多个协议 route，但当上游按模型系列分配入口时，每条 Exact 模型规则仍只能
  声明该系列真实支持的单一协议；服务级 route 集合不是模型级可任选协议集合；
- 请求开始后冻结 `provider_id + service_entry_id + credential_profile_id + model_id +
  protocol_profile_id + manifest_revision`；
- retry 只允许使用冻结身份，协议或清单修订变化直接报错；
- 失败后不测试第二协议、不降级、不静默切换。

用户新增会话模型须明确声明正数上下文与输出上限，输出上限不得超过上下文窗口。输入音频先由
所选协议的实际编码器门禁；Anthropic Messages 没有原生音频输入投影。视频输入除编码器能力外，
还须由所选供应商路由声明视频输入约束。自定义供应商目前不生成视频输入路由约束，因此其用户
模型及供应商默认值不能声明视频输入。当前 OpenAI Chat 与 Gemini 编码器允许音频输入，标准
OpenAI Responses 编码器尚未确认音频 Wire 形状，暂时失败关闭；不能把 Chat 的 `input_audio`
形状直接套到 Responses。用户新增模型不进入随包通用模型能力目录。
上述 OpenAI 输入形状以上游 [Responses 请求参考](https://developers.openai.com/api/reference/cli/resources/responses/methods/create)
和 [官方 Python SDK 的消息内容联合类型](https://github.com/openai/openai-python/blob/main/src/openai/types/responses/response_input_content_param.py)
为契约；升级 OpenAI API/SDK 或新增 Responses 音频实现时，须重新核对联合类型和实际请求编码。

自定义供应商遵循用户确认的规则：

1. 只启用一个协议时自动形成唯一绑定，不显示模型协议选择器；
2. 启用多个协议时才允许逐模型选择；
3. 没有显式选择时，固定优先级为：
   `openai-responses.v1`、`openai-codex-responses.v1`、`anthropic-messages.v1`、
   `openai-chat-completions.v1`、`google.gemini.generate_content`；
4. 因此只要启用 Responses 就优先自动选 Responses；没有 Responses 才按上述顺序选择；
5. 禁用已选择协议会要求重新解析/选择，绝不保留失效绑定；
6. 保存后即按声明执行，不存在“先用 Responses 测试再切换”的探测阶段。

## 6. Request、Response、Error 与 Usage Profile

Profile 是供应商通用规则与模型覆盖的主要承载方式：

- `request_profiles`：作用于协议编码后的 Wire JSON 请求；
- `response_profiles`：作用于解码后的响应或错误值；
- `usage_profiles`：从精确字段提取输入、输出、缓存、推理等统计；
- 规则或路由通过 `*_profile_refs` 引用，模型作用域规则可覆盖供应商默认引用。

公共会话先构建支持图片、音频、视频和文件的强类型内部语义封装，再按冻结的精确协议生成
Wire JSON；内部封装不是标准 Responses 请求。媒体载荷保存在请求局部资源表，内部索引和源
路径不得通过 Profile 暴露给上游。`request_profiles` 只能消费目标编码后的线路字段，不能使用
`vulcan_resource` 等内部媒体项。映射完成后仍执行目标媒体护栏；普通 Anthropic 不能借 Profile
获得视频变体能力，标准 Responses 也不能通过映射注入音视频内容。

请求/响应映射只允许已注册封闭步骤，例如：字面量写入、缺失默认、JSON Pointer 复制/重命名/
删除、显式枚举映射、单元素数组包裹/解包、受限模板、受限宿主字段复制、条件写入/删除、空容器
清理和显式拒绝。禁止 JavaScript、Lua、表达式求值、反射字段轮询和任意 deep merge。

`response_profiles[].payload.transient_reasoning_status` 用于声明非持久化思考状态，当前只接受：

- `summary`：协议或响应映射产出可读思考摘要；宿主按协议提供的稳定摘要节点身份在当前回合底部逐项原地更新；
- `encrypted`：上游只提供不透明思考块或思考 Token；宿主只显示通用“正在思考”状态。

这两类状态都不得进入 assistant reasoning、转录记录、Provider replay、重试快照或下一轮上下文，
并在成功、失败、取消或流结束时清除。若协议本身已有原生 summary 事件，解码器可以直接规范化；若
供应商使用 Chat 等不具备原生 summary 事件的协议，则必须通过 Response Profile 的精确字段映射和
模型级 `response_profile_refs` 声明，禁止在运行时按供应商 ID 或模型 ID 写判断。同一冻结请求链最多
只能解析出一种临时状态模式；多个 Profile 同时声明时必须失败关闭。运行时展示优先级固定为“非空
完整思考 > 可读 summary > 加密进度”：完整思考一旦出现，必须立即清除全部瞬态节点并屏蔽后续瞬态
更新；可读 summary 可以取代尚未产生语义内容的加密进度。不得使用 Markdown 标题、正文文本模式或
供应商/模型身份猜测 summary 节点边界，节点身份只能来自协议事件或精确响应映射。

请求 Profile 不得写宿主保护根：`model`、`messages`、`input`、`tools`、`stream`。如真实供应商
需要改变协议核心形状，应新增并审核协议/请求 projector，而不是用 Profile 越权重写。

处理特殊 usage 的流程：

1. 保存脱敏真实响应 fixture；
2. 在供应商默认或模型作用域定义 usage Profile；
3. 声明精确字段路径和单位，不写候选字段 fallback；
4. fixture 同时断言规范化统计与 provenance；
5. 若供应商不同模型返回不同字段，分别用 Exact 规则引用不同 Profile。

已取证的逐帧累计 Usage 必须声明为 `snapshot`，避免第一条统计提前冻结终态；不能把累计值声明为
`delta` 后逐帧相加。入口与模型作用域示例见 [Step Plan 用量证据](providers/stepfun-usage-evidence.md)。

用量提取与累计的实现权威是 `vulcan-llm-core/src/usage.rs`。修改用量字段、派生项或上游协议修订时，
必须同时复核这里的口径和回归测试。传输层先合并一次物理请求的原始分量，再按已声明的依赖顺序
归一化，并向下游发布累计快照；不同请求及协议路由不得共享累加器。

- 下游 `input_tokens` 是含缓存读取、缓存写入各一次的完整输入；`uncached_input_tokens` 是未由缓存
  读取提供的输入，因此包含缓存写入。已包含缓存的输入不能再次加缓存明细。
- 下游 `output_tokens` 是完整输出，`reasoning_tokens` 保留为其中的明细。只有 Profile 显式声明
  `output_from_disjoint_reasoning` 时，才把线路上的独立思考量加入输出；使用该派生的 Manifest 必须
  声明 `minimum_manifest_engine >= 3`。思考可选省略；原始输出缺失时完整输出仍保持未知。
- 字段缺失与明确报告零值不同。缺失输入不能由总量减输出、再减思考来猜测；上下文预算保留已有的
  字符估算及警告。缺失完整输出但已知思考时，回合内预算可将思考作为已知下界，而不伪造完整输出。
- 默认 Anthropic Profile 从互斥的普通输入、缓存读取和缓存写入构造完整输入；`message_delta` 的
  累计更新继续替换原始分量，直到 `message_stop` 冻结。聚合缓存写入存在时不再叠加其时长分项。
- 默认 Gemini 与 Interactions 的现代字段把思考作为独立输出分量；Code Assist 使用精确的
  `/response/usageMetadata` 封装路径。OpenAI 的缓存与思考明细已包含在相应总维度中，不再相加。
  已声明的供应商覆盖 Profile 仍是权威，不因协议基线增加字段而自动继承未经取证的字段。
- Cohere 的 `billed_units` 是计费指标，不能代替 `tokens` 上下文计数；实际计数缺失时保持未知。

上述上游字段说明归属于 [Anthropic 流式契约](https://platform.claude.com/docs/en/build-with-claude/streaming)、
[Gemini 用量契约](https://ai.google.dev/api/generate-content#UsageMetadata) 和
[Cohere Chat 契约](https://docs.cohere.com/v2/reference/chat)；上游字段或内置 Profile 修订时必须重新核对，
不能把本节转录文字作为独立的字段定义来源。

`usage_profiles[].payload.derivations` 可选声明
`total_from_input_components_and_output`。该派生只在供应商证据证明
线路上绑定的 `uncached_input_tokens`、`cache_read_tokens`、`cache_write_tokens` 与完整输出互不重叠时使用，
计算式为三个原始输入分量加归一化输出，不能再次添加归一化未命中输入中已经包含的缓存写入；其中
缓存读取和缓存写入字段缺失按零处理，普通输入或输出缺失、以及加法溢出
仍保持 Unknown。供应商直接报告的 `total_tokens` 始终优先，派生不能覆盖。未声明该项的路由不得
自动相加，避免把“字段缺失”误判成可推导总额。

Usage Profile 必须按真实承载面声明 `record_kind`：

- `json_body`：成功的非流式 JSON 响应；
- `sse_event`：SSE 流事件；
- `web_socket_frame`：WebSocket 帧；
- `error_json_body`：HTTP 或协议失败时保留下来的 JSON 错误正文。

错误正文中的 Usage 不等于请求成功。宿主会独立保存失败终态与真实 Usage；不得把失败响应改写成成功，
也不得因失败而丢弃已计费用量。新增 `error_json_body` Profile 时必须复制脱敏错误 fixture，并同时断言
`usage`、`source_paths`、`update_kind` 与错误状态。仓库内的
`fixtures/profile-error-usage-extraction.json` 是可执行参考。

若 Provider 只有在请求中显式要求时才返回 Usage，应通过当前协议路由引用的 `request_profile_refs`
写入精确字段，例如 OpenAI Chat 流式接口的 `/stream_options/include_usage=true`。请求开关与返回提取
必须各自有 fixture；禁止在宿主传输代码中为供应商或模型写硬编码字段。

自定义供应商的每个协议路由分别声明 `usage_profile_refs`、`request_profile_refs` 和
`response_profile_refs`。模型覆盖只能通过该模型命中的 Exact/Regex 规则替换引用；一个协议的 Usage
路径不得作为另一协议的候选 fallback。Profile 未命中或字段不存在时记录为 `missing`，由会话预算层
显示字符粗估警告，不允许改试其他协议，也不允许轮询猜测字段。

## 7. Operation、Provider Tool 与 Upload Channel

当前宿主执行原语是封闭集合：

```text
http_json_sync
http_json_async_poll
conversation_media_analyze
mcp_http_call
```

`http_binary_sync` 仅保留为未来 Schema 枚举，当前引擎没有注册；`signed_control_plane` 是原始设计中
预留的执行族，当前 Schema 与运行时均未开放。签名对象上传由下述 `signed_object_upload` 和其封闭
`signing_primitive` 承担，不能把上传握手冒充 Provider Tool 执行原语。以上能力若要开放，必须先随
软件实现完整请求、响应、界限和 fixture 契约，远端 JSON 不能自行启用。

当前上传原语是：

```text
inline_data_url
inline_base64
remote_url_passthrough
multipart_file
provider_file_create
signed_object_upload
```

`presigned_form_upload` 与 `managed_asset_reference` 仅保留为未来 Schema 枚举，当前引擎没有注册，
候选清单引用时会以 `unknown_upload_primitive` / `unsupported_engine_contract` 失败关闭。清单只能引用
集合中已安装且在 `engine_contract` 声明的原语。`legacy_registered_adapter` 不是当前系统清单允许的
生产原语。

一个 JSON-only 多模态接入应按以下闭包建模：

1. `upload_channels` 声明 MIME、字节数、数量、HTTPS 宿主/路径、凭据角色、网络总超时和响应引用编码；
2. `operations` 声明操作 ID、载体、参数 schema、执行原语、上传绑定、必填总执行 `timeout_ms` 和可选异步生命周期；
3. `tool_targets` 把 AI Tool 绑定到入口、凭据档和 Operation；
4. 模型规则通过 `operation_refs` 或 capabilities 声明模型支持的功能；
5. fixtures 验证参数映射、请求、轮询/取消和规范化输出。

资源解码器的 `source_path` 选择一个载体或有序数组。响应为对象数组时，可额外声明
`item_source_path`，在每条记录内用精确 JSON Pointer 选择载体；例如 `/data` 配合 `/b64_json`。
每一项都必须解码成功，资源数量和单项字节上限继续生效。不得只读取数组首项或猜测载体字段。
二进制正文不能声明记录项选择器。

公共模型能力资源 `common_model_capabilities.json` 的版本为 2。除上下文和输入输出模态外，记录还可
声明 `output_limit`、`reasoning`、`reasoning_control`、`reasoning_efforts`。这些字段只由精确来源声明
补齐未知能力，不能覆盖更具体的供应商事实。旧资源版本明确拒绝。生成脚本优先采用官方供应商
声明，再处理同名中转记录；供应商清单仍持有逐入口协议和 Provider Tool 参数，公共表不授予模型
目录成员权限。

图片、视频、音频理解使用额外的固定宿主外壳。`image_understand`、`video_understand`、
`audio_understand` 暴露给 AI 的参数永远分别只有一个媒体路径或 URL 和 `prompt`；Manifest 中的
`tool_targets[].input_schema` 只描述供应商内部规范参数。每个媒体理解目标必须声明
`media_input_projection`，显式给出：

- `path_field`：内部目标接收媒体路径或路径数组的字段；
- `prompt_field`：内部目标接收提示词的字段；
- `path_as_single_item_array`：内部字段是否要求单元素数组；
- `fixed_arguments`：最多 16 个宿主持有的固定参数，不能与路径或提示词字段冲突。

媒体投影只能出现在三项理解 Target 上，三项理解 Target 缺失投影会在 Manifest 激活时失败关闭。
模式、目标、账号、模型或思考级别切换只替换宿主执行闭包，不能改变工具 ID 或 AI 可见 Schema。
系统配置的模型、账号和开关只作为新会话默认值；会话按媒体理解能力独立保存精确的供应商 Target 或
替代模型、账号及可选思考档位，并在执行时直接使用该路由覆盖系统默认值。供应商专用模式禁止携带
思考档位。内部模型请求仅在显式档位实际启用推理时设置隐藏推理输出；缺省或 `none` 使用协议默认
输出策略。
会话能力入口由当前可执行供应商 Target 与普通会话模型的真实 `input_resources` 共同生成，不受系统
开关、默认模式、默认路由或候选准入标记限制，也不得要求某种媒体类型必须先保存系统默认后端才显示。
模型代替模式仅在精确路线声明 `toggle` 或 `effort_selection` 且存在可选档位时显示会话思考设置；
`fixed_off`、`fixed_on`、`provider_adaptive` 与未知控制模式不得渲染覆写控件或虚构档位。
候选模型可用性与已保存的系统默认后端是两种状态：没有活动后端选择时，系统设置必须显示未配置，
不得用第一个候选填充系统默认选择器。会话可将已保存的系统默认路由作为首次选择偏好，但仍须列出并
允许保存全部当前可执行的独立路由。
供应商返回纯文本时，宿主统一包装为 `vulcan.media-understanding.v1`。普通会话模型代替模式固定
请求纯文本回答，不发送 JSON Object、JSON Schema 或其他结构化输出控制，也不读取冻结路线的
`structured_output` 能力来改变理解请求。宿主负责把回答包装为规范结果；即使回答文本外观类似 JSON，
也仍按普通理解文本保留，不能把会话模型当作宿主结果 Schema 的实现者。

Upload 在任何网络副作用之前完成全部资源预检。网络通道必须使用允许的 HTTPS 宿主和绝对安全
路径；每个通道必须声明精确 MIME 允许列表及有界数量，本地字节通道还必须声明有界字节数，
网络通道必须声明 `timeout_ms`，其截止时间覆盖签名握手、文件传输与响应解码的完整生命周期。
`remote_url_passthrough` 固定只允许不含凭据和 fragment 的 HTTPS URL，不限制宿主或端口：供应商
既已配置，就不再要求清单另列宿主白名单。资源上限不能由远端更新放宽。签名上传
没有已注册签名原语时必须失败，不能猜签名算法。Multipart/File Create 的 method、file field、
响应 JSON Pointer、引用模板、固定字段冲突和缓存时长均在激活前闭合验证，不留到请求期猜测。
`response_id_pointer` 默认只接受非空 JSON 字符串；当上游真实返回非负整数 ID 时，通道必须显式声明
`response_id_encoding: unsigned_integer`，宿主才会将该整数规范化为十进制文本。负数、浮点数、布尔值、
对象及未声明的类型变化继续失败关闭；URL 指针始终只接受字符串，不能借 ID 编码绕过 URL 校验。
上传通道支持上传进度时必须声明 `progress_supported=true`；支持中断底层传输时同时声明
`cancellation_supported=true`。宿主以实际读取或发送字节发布 Preparing、Uploading、Completed、
Failed、Cancelled 终态；上传 Future 被取消或所属运行结束时也必须产生终态，不能留下永久活动的进度条。

`reference_encoding: provider_file_id` 的会话上传通道只能被“协议本身拥有标准文件 ID 内容块”的路线
引用，矩阵以 `vulcan_llm_core::protocol_profile_supports_media_source` 为唯一权威：

| 协议 | 文件 ID 内容块 | 路线额外要求 |
| --- | --- | --- |
| OpenAI Responses | 图片 `input_image.file_id`，文件 `input_file.file_id` | 无 |
| Anthropic Messages（含视频与 Claude Code 变体） | 仅图片：`image.source = {type: file, file_id}` | 供应商要求 Files beta 时，在路线 `request_headers` 声明精确值（如 `anthropic-beta: files-api-2025-04-14`）；宿主不会按内容自动补头 |
| OpenAI Chat | 图片部件只有 `image_url`，没有标准文件 ID 形态 | 不得为图片引用此类通道；否则运行时判定 `UploadContractInvalid` 并降级为附件清单 |

供应商私有的非标准内容块（例如 DeepSeek Chat 只接受扁平 `{type: file, file_id}`、拒绝 OpenAI 嵌套
`file.file_id`）在资源投影 Profile 落地之前不可声明，该路线保持内联。上传对象归属 API Key 时声明
`reference_scope: credential`，避免跨凭据复用。路线只需追加上传引用并沿用模型规则的大小、数量和 MIME
时，可声明仅含 `resource_kind` 与 `upload_channel_refs` 的路线约束：模型规则约束覆盖路线约束，并在
自身未声明上传引用时继承路线引用，边界值仍只在模型规则中定义一次。

### 7.1 会话附件兼容层如何消费 Manifest

Manifest 的媒体声明不控制用户能否挂载文件，只控制请求副本能否直接携带资源。宿主先保存规范附件
事实，再对当前冻结路线生成以下封闭计划：

```text
InlineOriginal
UploadReference
MetadataOnly
```

只有模型资源能力、协议编码能力、精确 `input_resource_constraints`、引用策略、上传通道和当前操作的
引用消费字段全部闭合时，宿主才使用 `UploadReference`。主会话草稿及显式提交使用冻结后台策略：
四种媒体的无上传原始内联阈值默认各512 KiB，有上传内联偏好默认各零，累计编码预算默认2 MiB。
存在真实上传时默认上传，显式设置的小文件内联偏好仅在协议支持且预算足够时生效；接口硬大小先于
软阈值检查。模型 `max_count` 限制单条消息，上传通道 `max_count` 限制单次上传，不能混淆二者。
新规划不再通过 `InlineDerivedPreview` 绕过限制，该旧枚举仅保留已有表示的兼容读取。

不满足当前路线的附件不会让整次会话请求失败，而是在请求局部投影中变为 `MetadataOnly`。投影必须
保留稳定附件 ID、项目相对 `resource_path`、文件名、MIME、字节数和不可见原因；不得包含宿主绝对
路径、`file_id`、上传 URL 或任务 ID。规范历史不保存上述传输计划，每次请求和模型切换均从规范附件
重新投影。

V1新增的原投递来源属于 `UserInputEvent.parameters`，与上述旧附件上的请求局部 `delivery_mode`
不同。它按用户提示词事件顺序和资源出现位置持有待准备／已冻结状态、精确原模型路线及不透明上传
回执身份；后来的提示词、模型切换或缓存更新不能覆盖早先原身份。参数不含供应商URL或Base64正文，
不进入上游协议字段。首次物理请求路线与每个资源出现的原投递路线分开保存，防止同一提示词中在
不同模型下准备的附件互相覆盖。草稿准入时已经准备的逐项事实通过`prepared_resources`交接，首次
物理请求冻结时不得替换，冻结后保留初始证明以校验相同客户端动作的幂等重试。排队、过程内补充及
排队恢复草稿都保留该参数；它不属于`ApiStarted`，也不属于文件全局最近上传缓存。

尚未发送的托管草稿以精确活动草稿租约保存首次意图，上传回执只能从缺失填入一次。持久草稿另按
资源出现位置保存暂存资产ID，恢复时使用显式ID，不按相同摘要或资源ID推断原附件意图。纯项目资源
保留明确空值，不虚构旧暂存来源。即时/排队发送不再提前走旧预上传入口，最终投递由V1统一准备。
纯项目草稿另外保存“原客户端动作＋原资源位置”来源引用，重排、删除及插入新暂存附件按显式出现
组装参数；不将当前显示位置当成原来源。仅注册新暂存文件，纯项目条目保留原ID并从目录重建元数据。
规划、租约协调和提交共用精确草稿槽所有者校验；项目资源不取得暂存租约。完整浏览器流程、确认
丢失边界等仍按统一方案验收，不能据此宣称整体迁移完成。

既有会话已接纳输入的重试按精确客户端动作读取原用户输入或对应终结队列回执，不再从已释放草稿
租约取值。复用初始证明并核对正文及资源顺序，不把已冻结事件改回待准备。原输入中的资源出现按
“客户端动作＋原资源位置”定位，不能用相同ResourceId选取最近原模型。未知来源继承到新提示词时，
通过`unproven_resource_positions`显式标记；即使新提示词尚有首次准备资格，也只能投递地址。该标记
在冻结与重放后继续保留，首次地址投影不构成原媒体已知证明。纯项目输入区已传递该来源引用；
已恢复原回执不会调度新上传，真正尚未准备的项目上传延迟到V1提示词准备，不伪造上传完成或进度。

预上传引用的复用键至少包含内容 SHA-256、字节数、MIME、资源类型、Provider、服务入口、上传通道、
上传契约指纹、引用作用域、连接/会话作用域和 TTL。作者不能用“同供应商”或“同模型”代替这些精确
边界。媒体理解 Tool 继续由 AI 主动调用；附件兼容层不得自动执行 Tool，也不得把 Tool 的上传能力
推断成当前会话模型的原生输入能力。

### 独立媒体扩展的投递策略

模型代替理解和供应商专用操作的媒体准备已接入独立的 `MediaDeliveryPolicy`。不要从工具名称、
AI 参数或 `upload_channels` 数组非空推断用途或真实上传能力：`inline_data_url`、`inline_base64`
属于内联；仅已注册且签名契约闭合的网络原语属于真实上传；已有远程 URL 只走显式 URL 直通，
不会下载成 Base64。

独立扩展按图片、音频、视频、文件分别设置有上传和无上传的原始字节阈值。无上传默认 10 MiB，
有上传时的内联偏好默认零，累计 Base64 编码预算默认 16 MiB；与主会话的配置独立。真实上传接口
的 `max_bytes` 先于内联偏好生效，不能选择更大的后续接口、改走 Base64 或改标文件绕过它。
若操作同时声明真实上传和内联消费形态，满足对应阈值与预算时才允许选用已声明内联形态；没有
内联消费通道时不会凭空生成字段。超限明确返回无法理解或处理文件，不能用附件地址伪装成功。

整次操作的所有绑定先完成类型、大小、通道、预算及 Base64 形态预检，再读取或解码待物化内容并
执行上传。缓存命中与内联物化不产生虚假上传进度；多绑定操作不能套用单资源缓存而遗漏其他输入。
相同内容、同一上传契约且使用同一精确凭据连接的并发真实准备使用同一协调键：只允许一个调用方
产生网络副作用，其他调用方等待并共享其终态引用；跟随者取消只终止自身等待，所有者失败或被丢弃
会显式终结全部当前跟随者并释放键，使后续调用可以重新取得所有权。进行中键始终包含连接 ID，避免
一个账号的认证失败污染另一个账号；成功后的完成缓存仍单独遵循 `reference_scope` 声明，可在允许时
跨账号复用。已完成缓存的查询会在取得所有权后复查一次，封闭“首次未命中后、注册所有权前已有
其他上传完成”的竞态窗口。
主会话草稿及显式提交已消费独立的主会话策略；V1最终物化已按提示词顺序消费原身份及原上传回执，
草稿到提示词按出现交接原准备，混合项目和暂存附件保持各自原模型，但本节不能被当成完整浏览器
或全方案验收证明。真实上传回执保留于既有项目资源绑定，原提示词只保存不透明指纹；
后续回执追加不能覆盖原回执，旧无来源证明的绑定不能推断为原始上传或内联。

固定媒体理解工具接受精确`project-resource://ID`、当前会话持有的`attachment://ID`及显式本地文件
路径。项目资源只在调用会话所属项目中解析；附件ID继续校验会话所有权。本地相对路径以活动工作区
为基准，绝对路径及经`..`解析到项目外的路径在当前尚无文件权限系统时允许使用；未来权限功能必须
在规范路径进入读取前请求授权，而不能重新把路径硬编码为不可用。宿主通过强类型侧通道传递原文件名、
MIME、长度和摘要；显式路径只在宿主工具参数与物化阶段存在，上传、内联或引用替换完成后不得进入
上游JSON。项目实体按摘要命名，无扩展名时不能推断MIME。动态模型扩展也已接入这些输入，严格按同一冻结
Operation的`upload_bindings`遍历：标量、字符串数组、显式条目路径及判别值遵循声明顺序。宿主解析
与上传引擎共用遍历器，先校验全部输入形态，再查询任何项目资源；不递归猜测其他参数字段。
重复定位符可复用一次元数据查询，但每次出现仍独立计入策略预算。解析位于凭据读取之前，受同一
不可重置截止时间约束；异步解析后不再重新选择Operation或代际。固定工具额外核对固定模态，动态
绑定使用目录原模态，由准确绑定和通道验证支持范围。宿主MIME和Data URL同样不能绕过固定MIME或
声明多格式映射。`attachment://ID`由相同声明遍历入口交给调用会话及冻结工作区的托管索引；
未提升到该会话、其他会话持有、越界、删除中或大小变化的资源均拒绝，不按路径或最新文件缓存补找。
两种定位符各自具有明确权威，彼此不回退；显式本地路径则按文件系统规范路径解析，不伪装成附件所有权。
理解入口按输入类型校验所有权或可读性，并统一校验模态、普通文件、大小和摘要；准入后由物化器验证
实际正文。原有要求
立即验证完整性的索引API仍执行摘要校验，未放宽为仅元数据成功。

普通传输预构建只生成安全附件元数据，不读取附件、不查询最近上传缓存，也不提前执行另一套媒体
预算。主会话最终媒体由V1用户提示词参数及出现顺序决定。WebUI自动/手动远程压缩使用同一规划
快照的来源消息映射，通过共同准备器的只读模式恢复原投递；只为实际所选窗口按时序分配预算，
不发起新上传或改写原提示词。独立任务压缩没有原来源证明时保持明确未发送的附件元数据，不能
借最近模型或文件缓存补出原媒体身份；该入口的完整验收仍需继续。

精确请求捕获接受宿主冻结的提示词来源端口，使用与最终编码相同的执行及策略。已有原引用/内联
可恢复，真正新内联可读取预览，但需要新上传时明确返回尚不可精确捕获，不因诊断产生上传或
原身份写入。没有来源端口的含附件捕获不能将临时占位文本报告为最终请求。来源ID与公共消息位置
在组装时一同记录，不猜测系统消息前缀，也不按字典序或资源ID关联。此能力有隔离回归，尚不等同
完整浏览器捕获流程、所有压缩入口和上传生命周期已经验收。

V1新上传已经接入相同运行时进度观察器；历史恢复及缓存命中不创建进度行。已选上传契约的总超时
从凭据刷新开始统一约束物化与返回校验，不能在签名或响应阶段重新计时；单文件超时只停止自身
传输，父请求取消则停止当前上传。Future丢弃亦必须取消后台工作并终结进度。进度终态封闭且
观察器在锁外按序调用，迟到字节回调不能把已结束传输重开。会话上传失败在前端提示附件仍保留且
本轮未发送内容；取消不伪装失败。HTTP状态及签名/传输阶段现已强类型保留，并通过实际HTTP及V1
HTTPS回环验证；非成功响应不先读取正文，坏/停滞错误正文不能掩盖状态。缺失/无效引用与成功引用
独立校验，错误不包含供应商载荷或私有路径。文件创建型上传现在使用48KiB原始分块构造一个完整
JSON请求，保留精确Content-Length及原字段含义，不再为本地文件分配完整原字节、Base64和JSON。
上传接口内部的Base64封装不是对话内联，不套用512KiB内联阈值或拆成多个上传请求；实际接口硬
上限仍先校验。传输校验实际流式长度和原摘要，JSON闭合前发布强类型完成结果，提前HTTP 200不能
替代来源完成证明；进度统计原始字节而非编码体积。整回合前置准备/重准备界限、取消整链及完整
浏览器交互仍需继续验收，不以局部期限或上传体有界证明整个逻辑回合已经有界。

准备取消在后台元数据规划排队前建立，规划和物化在每个来源边界检查；异步来源I/O等待期间可取消，
同步内联读取按64KiB块协作停止。宿主在下一次可靠V1 API许可前最多因较新输入/配置自动重启3次，
模型与压缩策略刷新、自动压缩重启共享计数；成功许可后清零，不是整个回合的模型切换配额。
耗尽通过既有精确流所有权报告准备不稳定，保留原prompt及附件。上述局部回归已通过，但共同期限、
真实流取消及浏览器持续修改配置仍需整链验收。

全批预检后，最终内联预算与原生数量按最终请求中实际生成的原生内容推进；失败项不占额度，后项按原消息及附件
顺序重新使用相同冻结规则，不更换上传通道或改写历史原方式。数量逐消息，编码预算整个请求共用。
捕获与只读投影使用同一实际前缀；捕获可以读取已允许内联的更早来源，但在最终决定需要新上传时
拒绝，不产生上传或原身份写入。首次待准备且缺少已交入原证明的V1上传，成功结果现在先在原prompt
逐出现保存，再发布私有回执；外部模型变化仍拒绝旧传输，不丢弃已经确认的原上传身份。

该竞态的参数层现已增加`Preparing { resources }`，按原prompt资源位置保存已选或已完成事实，并以空位
明确区分尚未决定的意图；不提前声明首次请求路线。初始`prepared_resources`保持不可变，完成结果
只能幂等重复，不能用当前模型或最新回执覆盖。读取端已使用该状态恢复逐出现原方式，最终冻结必须
保留全部已完成结果；只有首次实际API仍可确定整条prompt的请求路线。真实上传已接入该写入接口，
已知完成结果的工作线程在父Future取消后仍持有原准备门，直到prompt证明及私有回执结算；Stop和
失败收尾等待相同门后才能提交终态。自身完成修订需要由正式磁盘投影工厂重捕获，外部修订采用
不可复位的失效标志，不能通过手改旧快照修订号放行。真实HTTPS成功后即刻切模型的回归已验证
旧请求被拒绝且A原引用跨重载保留；取消后的租约/停止顺序另有真实存储回归，完整浏览器仍待验收。

V1首次上传现在先持久声明`UploadPending`，保存精确原路线及上传通道/契约，再进入凭据或上传等待。
同一原运行时选择只能补齐一次精确回执；模型、连接、协议、通道或契约变化、已完成引用替换及
越过首次API边界的回填均拒绝。尚无回执的选择只能显示地址，B无论是否支持上传均不能重新上传，
回A也不借用同文件其他意图的回执。真实HTTPS正文已发送但尚未回复时取消的回归已经覆盖该行为，
包括磁盘重载、Stop、实际取消终态、输入区规划以及无新增凭据访问。

草稿转prompt保留未完成上传选择，并携带原草稿`draft_intent_id`。实际prompt注册在原资产索引所有者
内登记精确项目目录及资源ID，之后才释放草稿租约；不能提前猜测未指定项目的新会话最终所属项目。
上传完成只补齐同一原意图的首份私有回执，活动原租约或已登记交接至少存在其一才可接纳；相同作用域
移除重加的新意图不能被覆盖。已知成功结果的自有工作线程跨父RPC Future丢弃保留，网络本身仍由父持有。
原prompt的初始证明、已冻结状态及时间顺序不改写；回A时通过精确意图、原路线、来源、通道、契约、
作用域和有效期匹配私有回执，B始终地址。旧无关联的待回执仍只显示地址，不查最新同文件缓存。
真实HTTPS回归已验证：回复晚于B首次API冻结时，A原引用仍可恢复，整个已持久事件序列不变且无第二次
凭据读取。它不代表完整浏览器、所有RPC取消时点、存储故障、来源准备共同期限或代际替换矩阵已验收。

主会话V1及草稿的已选上传现使用`MediaUploadDeadline`，在当前文件进入实际来源准备时捕获原通道
时限，覆盖回放路径、来源身份读取、缓存校验、选择结算等待、凭据及物理上传；各阶段不重新计时。
草稿元数据/索引读取使用有界异步等待，超时不取消整个父请求，也不凭错误文本判定用户取消。
已经验证成功的上传结果在此等待之外可靠发布，不能因后续观察器或原子存储耗时而撤销已知回执。
草稿取消登记现由唯一守卫持有。普通准备的返回、失败、异常及Future丢弃均清理自身登记；已知成功
结果把同一守卫交给原发布工作线程，原RPC丢弃不能提前释放它。取消信号与结算确认分离：取消RPC
必须等待原所有者结算后才返回成功，不能只设置令牌便声称附件可释放。清理比较精确令牌身份，
不误删相同文本ID下的替换登记，不在等待期间持有注册表互斥锁；结算通道异常明确返回错误。
输入区显式移除须等取消和租约确认后才移除原对象；取消失败保留原传输标识、来源租约及重试入口，
不能由失败队列尾或其他附件成功取消解锁。未连接服务不等于后端确认无操作。租约布尔返回值仅报告
集合是否变化，`false`仍是幂等协调成功；只有缺少实际服务响应或RPC拒绝才是未确认。
草稿原意图在现有计划/待处理登记内共享单次执行许可，后续A视图观察原任务、B视图只投影地址。
视图清理通过独立丢弃RPC处理，不能取消原实体；用户取消才到达原实体并等待结算。无活动任务和原回执
时只能由显式重试授权再次启动，不能因切模型自动重传。当前已有真实登记/RPC及隔离React Hook回归，
实际来源寿命、待发送提示词整组编辑替换及完整浏览器上传仍须继续完成整链验收。
实体上传开始前还必须在资产索引唯一事务中取得进程内来源持有，并覆盖来源读取、网络及已知结果发布。
该持有不是草稿来源、回执或缓存，不得持久化为崩溃后残留的`staging_refs`。清理、草稿集合协调与孤立
删除必须识别活动读取者；最后读取者退出后仍按消息/会话/草稿的持久引用决定保留或原子删除。Windows
现有资产根使用物理规范路径作为锁身份，不能让路径别名绕过此所有者。

期限等待在构造外围Future前将阶段装箱，保留父拥有及丢弃取消，不扩大默认线程栈。原通道超时不是
凭空新增的全局回合时限；完整批次元数据规划及排队等待不被误称为每个文件已经开始物理上传。

### 向量 Batch 原生推理绑定

异步向量 Batch 必须作为 `native_inference_bindings[].batch` 的封闭子契约声明，并通过同一绑定的
`model_ids` 精确限定模型。未列入 `model_ids` 的模型、未声明该绑定的服务入口以及未知地域均不得
从同步 `embedding_create` 推断出 Batch 能力。当前已核实的首批实现为 OpenRouter 与阿里云 Model
Studio 北京、新加坡地域。阿里云两地均使用各自地域的 OpenAI 兼容 `/files` 与 `/batches` 入口；
新加坡仅对其当前目录已有的 `text-embedding-v3`、`text-embedding-v4` 声明向量 Batch。

带 `batch` 的条目是补充异步绑定，不参与同步 `embedding_create` 的动作选择；其每个 `model_ids`
模型必须同时能由一个不带 `batch` 的精确或操作通用绑定覆盖。同步绑定继续定义向量空间身份，Batch
绑定只冻结异步传输身份，禁止因为新增 Batch 声明而改变同步路由的 action binding。

`protocol` 必须声明精确线协议：`open_router_beta_v1` 只允许
`inline_json/inline_json`，`alibaba_open_ai_compatible_v1` 只允许
`jsonl_file/jsonl_files`。`inline_json/inline_json` 表示请求和终态结果均在 Batch API JSON 中；
`jsonl_file/jsonl_files` 表示先上传 UTF-8 JSONL，再以文件 ID 创建任务，完成后按输出/错误文件 ID
下载。文件型绑定对供应商已经公开的限制逐项声明，包括单任务条数、文件总字节、单行字节、账号文件
数量与总容量、上传/文件操作频率、并行任务数、创建/查询/列表对账/取消频率；供应商未公开的硬限制
保持为空，不能用宿主策略值冒充。绑定必须声明 `host_policy_maximum_*` 作为本进程的内存、结果下载与
单次执行的资源安全边界；激活时拒绝协议/传输不匹配、越过服务根目录或含 query/fragment 的端点路径。

OpenRouter 使用 `base_url_override=https://openrouter.ai/api/beta`，提交正文顶层字段必须按
  `endpoint`、`model`、`requests` 顺序编码。当前官方提交体不接收 `completion_window`，因此
  `provider_completion_window_seconds` 必须省略；官方公布的 30 天输入/结果保留期不是任务完成窗口。
  该服务不使用文件上传，结果只能从具体 Batch 状态响应内收集。

当前主机仅保留受签名清单中的 Batch 声明、类型与激活校验，没有异步向量 Batch 执行消费者。
声明存在不代表宿主已经实现提交、持久化、轮询、取消或恢复；通用同步向量推理继续按同步绑定执行。
本段契约归属于 Provider Manifest Schema 及激活校验；修改相关绑定字段时必须回读源码并同步本文。

官方证据：OpenRouter `https://openrouter.ai/docs/batch-quickstart`；阿里云 Batch 适用范围与流程
`https://help.aliyun.com/zh/model-studio/batch-inference`；Batch API
`https://help.aliyun.com/zh/model-studio/batch-interfaces-compatible-with-openai`；File API
`https://help.aliyun.com/zh/model-studio/openai-file-interface`。

每项操作从凭据解析与上传准备开始共享一个不可重置的绝对截止时间；同步请求与响应读取均受该
截止时间约束。异步任务冻结完整 target/operation/upload/credential/generation 快照。恢复、轮询与取消使用同一
快照；热更新只影响新任务。`generic_json_async_lifecycle` 必须显式声明 start/poll/cancel 方法、
路径、task/status/error/output JSON Pointer、终态集合、`poll_interval_ms`、`max_backoff_ms` 与
`timeout_ms`。连续瞬时观测失败使用指数退避，但绝不超过冻结的 `max_backoff_ms`；成功观测后计数
归零，任务总生命周期仍受 `timeout_ms` 封闭约束。瞬时观测只适用于供应商尚未终结任务时的凭据、
传输或状态读取失败；一旦状态命中 `success_statuses`，成功约束、跟随标识、跟随请求、二进制获取、
响应投影、输出解码或资源安全校验失败都必须立即形成明确终态错误，不能继续显示为任务仍在执行。

固定 `tool_targets` 与 `tool_target_templates` 使用同一套激活前验证：系统端点只允许无凭据、无
query/fragment 的 HTTPS URL，endpoint path 必须保持同源；输入 JSON Schema、输出资源 decoder、
固定/条件请求头、MCP 契约、凭据档和已注册执行原语均在模板展开前校验，模板不能成为绕过普通
目标安全边界的第二入口。

供应商返回 URL 型资源时，清单不声明下载宿主白名单：已配置的供应商返回的任何宿主与端口都直接使用，
CDN 或存储桶变更不需要软件更新。`https_url` 只接受 HTTPS，`public_http_url` 仅在清单显式声明时
接受 HTTP/HTTPS。URL 在响应解码、异步跟随请求和 Agent 宿主实际下载前分别重新校验 Scheme、
绝对宿主与不含凭据；实际下载跟随有限次重定向（`PROVIDER_TOOL_RESOURCE_REDIRECT_LIMIT`），每一跳都按同一规则重新校验，因此 `https_url`
不会经重定向降级为 HTTP。这些请求不附带供应商凭据，因此放开宿主不会泄露密钥。
签名上传返回的临时 OSS 目标与上传响应 URL 同样只校验 HTTPS 与不含凭据。

如果新模型只是复用上述原语和既有 Wire 契约，可仅更新 JSON。若它需要新的认证、签名、流式
解码、本地媒体转换或供应商特有二进制协议，则必须先实现并审核宿主原语，再允许 Manifest 引用。

函数工具参数投影由冻结路由中的 `tool_schema_projection` 唯一决定，不得从协议或模型名称猜测。
系统 Manifest 未声明该字段时即为 `lightweight`：递归消费但不向 Provider 输出
`anyOf`、`oneOf`、`allOf`、`$ref` 等组合或引用语法，仅保留基础类型、属性、必填项、数组元素、
枚举、描述和布尔形 `additionalProperties`。纯对象备选分支合并属性，互斥联合仅保留所有分支共同的
必填项，`allOf` 保留必填项并集；宿主依然使用未投影的规范 Schema 校验实际工具调用。`passthrough` 只用于经过明确验证、必须完全原样传递的特殊契约；
`strict` 使用递归 Gemini Slim 投影。当前内置清单只有 Gemini 模型规则显式声明 `strict`，其余系统
模型一律省略字段并继承轻量模式。自定义模型只在高级设置中提供三档选择，默认同样为轻量模式。
不得把 Gemini 的严格关键字过滤扩散到其他模型，也不得按供应商或模型名称硬编码单个工具修复。

所有模型都以直接函数工具调用宿主工具，清单与自定义模型都没有工具模式字段。上游 Codex 目录为
订阅模型标注的 `code_mode_only` 只是官方客户端偏好；直接工具在同一订阅上同样可用，Vulcan 不提供
JavaScript 代码模式。

`apply_patch` 默认与其他宿主工具使用标准函数调用契约：输入为含必填字符串 `patch` 的 JSON
对象，补丁语法由工具说明与宿主执行器校验。只有精确模型规则的 `codex_behavior` 明确声明
`apply_patch_tool_type: freeform` 时，原生 Codex Responses 路由才把它编码为带已注册 Lark Grammar
的自由文本工具；流解码器按冻结工具声明解析其字符串输入，并保留 custom 调用/输出配对。不得仅因
工具名、供应商名或“支持 Responses”推断自由文本能力。规范函数调用仍使用
`function_call`/`function_call_output`，再由公共转换链映射至 Chat、Anthropic 或 Gemini；旧版
custom 历史只按其原始配对重放，不得借此向无声明的新请求暴露 custom 工具。

### 7.1 Codex 模型行为与 Responses 路由特性

`model_rules[].codex_behavior` 是精确模型规则拥有的封闭契约，只能用于
`openai-codex-responses.v1`。它冻结 wire 方言、补丁/终端/搜索工具形态、并行调用、
图片细节、verbosity、多智能体版本、说明注入、Node 审核、压缩哈希、工具输出截断、WebSocket 偏好
和实验上下文。解析结果随模型绑定进入路由修订指纹；重试、续跑和热更新不能按同名模型重新推断。
第三方供应商即使提供 `gpt-6-astra` 等相同模型 ID，也不会自动继承 OpenAI 的 Lite 行为，除非其
自身精确规则具有独立验证证据并显式声明。

`wire_dialect: lite` 使用稳定 UUIDv5 前缀项、`additional_tools` 命名空间目录、
`reasoning.context: all_turns`、`parallel_tool_calls: false` 和专用内部请求头；顶层
`instructions` 与 `tools` 省略。HTTP、WebSocket 首包/增量包与独立压缩请求共享同一最终头投影。
由 Codex 宿主持久化的
`configuration_update` 还要求可信的宿主来源和版本化私有历史；没有该宿主时不得伪造普通输入项。

`protocol_routes[].responses_features` 持有传输和恢复契约：

- `remote_compaction: enabled_with_local_fallback` 是现有独立远端压缩 V1；`inline_v2` 表示流内
  `compaction_trigger` V2，但在宿主没有注册流式 V2 执行器时整条声明校验失败，不会偷用 V1；
- `websocket` 只在精确路由验证后启用；游标无效时只能按既有安全边界重建同一路由全量请求；
- `recovery: agent_space_result_and_usage_reconciliation_v1` 只允许 AgentSpace 的
  `chatgpt-plan` 入口声明。它把 `reconciliation_required` 标记为结果与用量待确认，保留请求和
  response 身份并禁止盲重放；没有供应商查询/幂等契约前不能称为自动重试。

### 7.2 推理回传契约

`protocol_routes[].reasoning_replay_contract` 声明该路由**端到端实现**了哪一种历史推理回传形态。
它不是展示字段：取值只允许本引擎同时具备请求投影、目标侧接受条件、最终线路断言和回归测试的形态。

| 取值 | 形态 | 已核实的端点事实 |
| --- | --- | --- |
| `deepseek_responses_plaintext_v1` | 有序原始项按序写入 `input`，每条历史推理必须含非空明文 `reasoning_text` 数组 | 明文 `content` 被接受；`summary` 与 `encrypted_content` 被忽略 |
| `deepseek_chat_reasoning_content_v1` | 一条原始助手消息形成**一条** Chat 助手消息，推理进 `reasoning_content`、正文进 `content`、并行调用进 `tool_calls` | 有工具调用且无正文时 `content: ""` 被接受 |
| `deepseek_anthropic_thinking_v1` | 按规范顺序生成 `thinking`／正文／`tool_use`；签名只在来源标记证明归属时回放原始载荷 | 端点下发签名但**不校验**：无签名、空文本、伪造签名均被接受 |

会话历史的思考只在原回放身份完整匹配时参与请求。该身份包含供应商、连接、路由指纹、协议及模型；
任一维度变化时，服务内核 `provider_v1_sink::resolve_projected_assistant_replay` 在请求投影中省略旧路由
的 `Reasoning` 项及原生回放项，保留历史正文、工具调用与工具结果，不再添加历史思考的说明标题。
纯思考回合过滤后为空时不生成空的助手请求消息。该策略只改变发往模型的请求，原持久化思考与界面
历史仍保留；工具结果内部引用的日志继续作为工具数据原样保留，不做关键词替换。回放身份完全匹配的
历史继续执行原路由契约；未冻结目标路由的状态投影不按猜测过滤。历史投影策略变化时须同步本节与
服务内核回归测试。

作者要求：

1. 声明该字段的路由必须把对应原语列入 `engine_contract.required_execution_primitives`，
   否则 `validate-all` 以 `engine_contract_missing_used_execution_primitive` 失败关闭；
2. 引擎注册表只公布已端到端实现的原语，因此清单声明一个引擎未注册的取值同样失败关闭；
3. 取值按**冻结线路形态**声明，不按供应商名称推断：同名供应商的另一条路由、或另一供应商的相同协议
   都不会自动继承；同一回放身份下，未声明的路由保持原有编解码行为；
4. 已验证事实与未验证未知必须分开记录。没有真实脱敏端点证据的签名或形态要求保持显式未知，
   不得据“协议支持某字段”推断其必填或可省略。

回传失败的用户可见后果是固定的：历史推理确实存在却无法在**当前已声明路由**上安全回放时，
派发前失败并给出固定文案「当前路由无法安全回传此会话的历史推理。请新建会话，或切回产生该历史的原路由。」。
该失败为终态、不可重试，也不会自动改用其他协议；机器可读原因由稳定的
`reasoning_replay_unreplayable` 子类别承载，诊断中不含推理正文、签名、凭据或原始请求体。

## 8. Fixtures 与测试证据

专用媒体模型使用操作自身的参数与资源限制，不使用通用文本会话的上下文窗口。没有声明
`conversation_respond`，且发现证据明确缺少文本输入或文本输出的模型，或仅声明
`speech_transcribe` 的模型，应跳过通用令牌限制检查及输出上限收敛；原始零值继续作为来源事实保留。
显式声明会话能力的混合模型和模态尚未明确的模型仍按通用规则检查，不能按模型名称推断豁免。

fixtures 不是“JSON 能解析”的占位文件，而是走生产编译器与消费者的可执行证据。当前支持两类：

- execution fixture：在严格本地 HTTP 服务上执行 Operation/Upload/Async 并匹配请求与终态；
- profile fixture：直接走生产 request/response/error/usage/discovery 规范化并用 JSON Pointer 断言。

作者要求：

1. 基线清单文件名必须安全解析到 manifests 目录，禁止路径穿越；
2. fixture 大小、交互次数、请求体和响应体受限；
3. 请求必须匹配精确方法、路径和声明字段；
4. 断言规范化结果，不只断言 HTTP 200；
5. 特殊供应商事实必须有真实脱敏证据；没有证据就保持 Unknown；
6. fixture 失败禁止通过放宽 Schema、删除断言或添加候选字段兜底解决。

运行：

```powershell
rtk cargo run -p vulcan-provider-manifest-cli -- fixture-check
rtk cargo test -p vulcan-provider-adapters
```

### 全目录模型规则校验

清单 Schema、编译校验与 `manifest_conformance.rs` 覆盖全部内置清单的结构和引用；后者还统一核对
固定分辨率菜单与可执行输入 Schema 的枚举是否一致。测试只校验清单是否符合格式与引用规则，
不保存清单内容的快照基线：模型事实的正确性由清单评审与证据引用负责，增删模型或修改事实不需要
同步任何测试文件。

## 9. CLI

```powershell
rtk cargo run -p vulcan-provider-manifest-cli -- validate-all
rtk cargo run -p vulcan-provider-manifest-cli -- validate <file-or-directory>
rtk cargo run -p vulcan-provider-manifest-cli -- compile <file>
rtk cargo run -p vulcan-provider-manifest-cli -- inspect <file>
rtk cargo run -p vulcan-provider-manifest-cli -- diff <old-file> <new-file>
rtk cargo run -p vulcan-provider-manifest-cli -- build-index <manifest-directory> --out <index-file> --key-id <key-id> --revision <revision>
rtk cargo run -p vulcan-provider-manifest-cli -- sign-index <index-file> --key-file <hex-32-byte-seed-file>
rtk cargo run -p vulcan-provider-manifest-cli -- verify-index <index-file> --signature <hex-signature> --public-key <hex-public-key>
rtk cargo run -p vulcan-provider-manifest-cli -- fixture-check
rtk cargo run -p vulcan-provider-manifest-cli -- dry-run-activate <manifest-file>
```

`validate-all` 先校验 `bundled-index.json` 的完整文件集和内容固定值，再执行 Draft 2020-12 Schema
与生产编译器校验；`inspect` 输出路由、Profile、Operation、Upload、Tool 和 provenance；`diff` 是
有界语义差异，不是文本 diff。退出码为：成功 0、操作失败 1、用法错误 2。CLI 不内置生产私钥、
不自动发布、不绕过 bundled trust ceiling。

修改随包基线时，使用显式、可复现的发布时间和 Index 修订重新生成本地完整性 Index：

```powershell
rtk cargo run -p vulcan-provider-manifest-cli -- build-index crates/vulcan-runtime-assets/resources/provider-manifests/manifests --revision <index-revision> --key-id bundled-package-integrity-v1 --issued-ms <unix-ms> --expires-ms 253402300799999 --relative-prefix manifests --out crates/vulcan-runtime-assets/resources/provider-manifests/bundled-index.json
```

该命令只生成本地随包清单，不替代远端 Index 的签名、验签和发布流程。生成后必须再次运行
`validate-all`，且不得手工只改其中一个摘要以掩盖目录漂移。

日常编辑随包清单后，用一条命令完成上述步骤：

```powershell
node scripts/provider-manifests-refresh.mjs            # 加 --dry-run 只列计划，加 --no-sync 跳过同步
```

脚本按 `bundled-index.json` 固定的摘要找出改动的清单；作者未提高 `revision` 的清单自动递增
`revision` 并刷新 `published_at`（只改这两行，保留手工排版）；随后以下一个 Index 修订重建
`bundled-index.json`、执行 `validate-all`，最后执行 `make.ps1 sync`。任一步失败即停止。

`display.directory_rank` 决定供应商在设置目录「可添加」区的默认顺序（数字小者在前，未设置者排在
最后）；用户已配置的供应商始终在前，并按用户自己的排序显示。`display.promotion.signup_url` 是
目录与介绍卡上的注册入口；无合作关系的供应商使用 `partnership_kind: "none"` 与中性披露键，不得
填写 `referral_url`。

发布门禁脚本：

```powershell
rtk proxy pwsh -NoProfile -File scripts/check_provider_manifest_release.ps1
```

## 10. 签名、同步与信任下限

在线清单服务必须按客户端版本和引擎能力投放兼容版本。远端不得把高版本 schema、新原语或更高
`minimum_manifest_engine` 发送给不支持的客户端；客户端仍需独立失败关闭校验。若收到不兼容清单，
唯一处理方式是拒绝激活并提示升级客户端，不得探测旧协议、回退代码 descriptor 或临时忽略未知字段。
客户端在 Index 和供应商清单请求中统一发送 `x-vulcan-client-version`、`x-vulcan-manifest-schema` 与
`x-vulcan-manifest-engine`；在线分发必须使用这些显式能力边界选择代际，不能依赖未声明的客户端猜测。

远端 Index 和供应商清单使用分离契约：

1. 从单一配置的 HTTPS Index URL 获取原始字节；
2. 禁止 redirect；Index 只允许同源安全相对 URL；
3. 在 JSON 解析前，用唯一受信 key 校验 Index 原始字节签名；
4. 校验时间窗、条目身份、字节数、SHA-256、最低引擎契约；
5. 下载供应商原始字节并先核对 Index pin，再解析和编译；
6. 校验 candidate 没有突破 bundled trust ceiling；
7. 写 Staging、Active、Previous 和 state 事务，随后暂存运行时代际；
8. 目录刷新与 Workbench 投影成功后才发布并提交；失败恢复精确 before-image。

bundled trust ceiling 禁止远端新增网络宿主、凭据角色、协议/执行/上传原语、宽松资源上限或其他
权限。以下字段也按随包基线冻结或只能收紧：

- `display.promotion` 的链接和披露信息；
- `credential_registration` 与凭据档 `account_usage_query`；
- operation 的语义动作、参数到 upload 的源绑定、tool target 与通用异步生命周期；
- upload channel 的允许宿主、路径、MIME、数量、大小、网络总超时、凭据角色和缓存时长；省略缓存表示禁用，
  远端不得把禁用改成启用，也不得延长时长；
- 响应资源 decoder 的下载 Host 后缀与异步 follow-up URL Host 后缀；
- 固定 header、服务源覆盖策略和 native inference binding。

远端数据更新可以新增/修改模型规则和受限数据事实，但不能把“热更新”变成远端代码执行，也
不能通过 operation、upload 或推广元数据获得随包清单没有审核过的权限。

304 只有在本地 Active 字节仍与持久化 revision/SHA-256 一致并能重新编译时才成立；否则无条件
重取。每个供应商有 single-flight，同一次 `sync_all` 共用一份已验签 Index 快照。

生产 Index URL、受信公钥、签名责任人和发布周期属于外部配置门。未配置时报告
`not_configured`，不能在代码中猜端点或密钥。

## 11. 激活、启动恢复、回滚与代际

系统持久化层包括 Staging、Active、Previous、Quarantine、逐供应商 state 和可恢复事务 journal。
自定义供应商使用独立 Manifest 文件、中央引用配置和单独事务 journal。

启动顺序：

1. 先恢复中断事务；
2. 重新校验 Active 的 provider/revision/SHA-256/Schema/trust ceiling；
3. Active 无效则隔离并验证 Previous；
4. Previous 有效则交换/恢复为 Active；
5. 两者无效才使用 Bundled；
6. 统一重建并发布一个不可变 generation，再刷新 Workbench。

显式回滚是 Active/Previous 的内容寻址交换，不是盲目复制。Service 使用
`service_rollback_provider_manifest` 暂存回滚，随后统一发布 runtime generation 与 Workbench；
任一投影或事件失败都会恢复磁盘和运行时精确前镜像并重新发布旧代际。

运行中请求、retry 和异步任务保留选择时的 `Arc` 快照；新 generation 只影响新选择边界。不要让
消费者分别读取“当前 provider”“当前 rule”“当前 protocol”，否则会形成跨代拼接。

## 12. 状态、诊断与 provenance

运维状态通过 typed Service/RPC 暴露：

- Bundled、Active、Previous 的 revision 与 SHA-256；
- 当前运行时 generation 和 active layer；
- 同步是否配置、最近 ETag/时间/错误分类；
- Quarantine 摘要；
- 同步、全量同步和显式回滚结果。

模型诊断必须能解释：最终协议、规则 ID、Manifest revision、能力字段来源、Profile 引用来源、
Operation/Tool declaration 来源和选择 snapshot。UI 不得再实现第二套规则解析器，只展示 Service
提供的强类型结果。

常见失败定位：

| 现象 | 首查 |
| --- | --- |
| 新模型文件已存在但 UI 不可见 | `model_catalog.models`、source policy、`enabled_models` 合并和 Workbench generation |
| 模型可见但不能执行 | Exact rule、唯一 selected protocol、verified route、credential scope |
| usage 为零或错位 | 模型最终引用的 usage Profile、fixture 与 response provenance |
| 同步返回 up-to-date 但本地缺 Active | state 与 Active pin；304 路径应强制重取 |
| 回滚后 UI 仍显示新模型 | 是否完成 generation 重建和 Workbench 发布，而非只交换文件 |
| retry 改走其他协议 | frozen retry route guard；这是 P0，禁止加 fallback |

## 13. 宿主硬边界台账

静态扫描命中必须人工分类，不能看到供应商名就删除，也不能看到“兼容”就永久保留。

| 位置/类别 | 当前理由 | 禁止用途 | 删除或缩减条件 |
| --- | --- | --- | --- |
| `project_custom_descriptor_manifest` | 把当前用户自有 `custom:` descriptor 投影为同代际 Manifest | 禁止接受系统供应商、缺少 `custom:` 前缀的输入或旧版单协议字段 | 自定义供应商改为直接编辑完整 Manifest 时删除 |
| 通用 `generic_json` Operation/Upload/Async 原语 | 执行全部已迁移供应商目标；权限来自已编译 Manifest 和内置 trust ceiling | 禁止按供应商名、模型名或候选字段分流 | 长期保留为封闭宿主原语，并持续由生产目标 fixture 覆盖审计 |

系统 Legacy allowlist、`legacy_descriptor_by_id`、`first_batch_legacy`、Alibaba/MiniMax/Tavily
供应商命名 adapter、静态系统 descriptor 目录和
`tool_catalog.rs` 已删除。发布门禁会拒绝这些文件重新出现，并要求全部生产目标的 adapter 都是
`generic_json`、每个目标恰好由一个可执行 fixture 覆盖。

允许长期存在的宿主硬边界包括：协议编解码器、Secret 投影、安全 URL/Host/header 规则、上传资源
上限、本地媒体读取、受控签名算法和不可配置的系统字段保护。

## 14. 作者完成检查表

- [ ] provider/file/revision/时间和 engine contract 正确；
- [ ] 服务、凭据、路由、Profile、Operation、Upload、Tool 引用完全闭合；
- [ ] 每个可执行模型有唯一协议，未知模型策略符合事实；
- [ ] presence 语义正确：省略、空值与 `null` 没有混用；
- [ ] 未写宿主保护请求根，未引入脚本/表达式/候选字段 fallback；
- [ ] 网络宿主、路径、MIME、字节数、数量、轮询与取消均有界；
- [ ] 特殊 request/response/error/usage 有可执行 fixture；
- [ ] 声明 `reasoning_replay_contract` 的路由已把该原语列入 `required_execution_primitives`，且其线路形态有真实脱敏端点证据或保持显式未知；
- [ ] 注册 OAuth 凭据已验证后续真实请求读取最新秘密修订版，401 恢复不切换账号、模型或协议；
- [ ] 图片/视频/文件模型的 upload 和 operation 绑定可在 JSON 中解释；
- [ ] `validate-all`、`fixture-check`、目标 crate 测试和 release gate 通过；
- [ ] 修改随包 Manifest 后已同步提升/重建 `bundled-index.json`，文件集合、字节数、摘要、修订和引擎契约一致；
- [ ] `inspect` 与 semantic `diff` 已人工复核；
- [ ] 新 revision 高于 bundled/active，未突破 trust ceiling；
- [ ] 签名私钥未进入仓库或日志；
- [ ] 同步后验证 runtime generation 与 Workbench，而非只看文件落盘；
- [ ] 回滚测试证明磁盘、runtime 与 UI 投影一起恢复；
- [ ] 任何供应商的新事实在取得证据前保持显式未知，没有根据分组、协议或相邻模型猜测。
