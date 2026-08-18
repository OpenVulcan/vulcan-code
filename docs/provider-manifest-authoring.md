# Provider Manifest 作者、测试与运维手册

本手册对应：

- `docs/新增系统供应商流程.md`；
- `docs/completed/20260806/01-HOT_UPDATABLE_PROVIDER_MANIFEST_AND_MODEL_PROTOCOL_ROUTING.md`；
- `docs/completed/20260807/02-PROVIDER_MANIFEST_FULL_REMEDIATION_AND_FIVE_PASS_AUDIT.md`；
- `docs/completed/20260808/01-PROVIDER_MANIFEST_FINAL_GAP_REMEDIATION.md`。

它说明如何只通过受签名 JSON 发布供应商与模型事实，以及何时必须修改宿主原语。本文不替代
`manifest.schema.json`；字段形状以 Schema 和 Rust 编译器的共同校验结果为准。

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

### 1.1 自定义供应商文件位置、格式与迁移

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

启动迁移只接受两种明确输入：当前 `vulcan.custom-provider-manifest.v1`，以及历史 descriptor-only
文档（根 `id` 必须精确等于文件身份，且不得出现 `provider_manifest`）。历史解析由冻结的 v0 fixture
验证，不根据未知字段或当前默认值猜测版本。

首次需要迁移时，宿主必须先原子创建
`provider-manifests/custom-provider-migration-backup.v1.json`。该备份精确保存迁移前的中央配置文本与
当时全部独立自定义 Manifest，总上限为 64 MiB，一旦存在就只校验并复用，绝不覆盖。这是持久的最初
before-image，不是日志的短期代替品。

中央旧配置和独立文件随后通过
`provider-manifests/custom-provider-transaction.v1.json` 保存完整 before/target 镜像；进程中断后先重放
同一 target 镜像，再执行补水。目标镜像落盘并重新加载验证成功后，宿主写入上限 4 KiB 的
`provider-manifests/custom-provider-migration-complete.v1.json`；其中 SHA-256 必须精确绑定不可变备份。
如果数据事务已成功但标记写入前崩溃，下次启动只能在当前加载器已证明不再存在旧格式来源后补写标记。
标记指向不同备份、任何文件缺失、身份不符、Schema 错误或事务恢复失败都会阻止激活，不会静默回退到
另一供应商或丢弃该供应商。

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

## 3. 服务入口、目录来源与模型热新增

一个 `service_entry_id` 表示一个真实产品入口，例如公开 API、Coding Plan 或 Provider Tools。入口可拥有：

- 独立 `base_url`、固定非秘密请求头和请求 projector；
- 一个或多个 `credential_profiles`；
- `model_catalog`；
- 一个或多个已验证 `protocol_routes`；
- 可选入口级模型默认覆盖。

`request_projector` 只允许供应商无关的三种封闭原语：

- `direct`：协议编码后不进入供应商兼容投影；自定义供应商固定使用此值；
- `compatible`：供应商差异完全由冻结的 Request Profile 表达；
- `routed_compatible`：在 `compatible` 基础上允许已注册且明确禁止 fallback 的二级路由。

不得新增以供应商命名的 projector。新增字段差异应使用已注册 Mapping Transformer；新增 wire 协议
或新的二级路由握手才属于需要升级宿主的引擎能力。

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

`model_catalog.discovery_kind: manifest_static` 表示不发远程目录请求，模型身份、顺序与基础能力完全
由当前已编译 Manifest 持有。它的封闭约束为：

- `model_catalog.models` 必须出现；非空数组是权威身份与顺序，空数组明确表示没有模型；
- 每个身份必须有覆盖精确服务入口与凭据档的模型规则，否则整份清单编译失败；
- `path`、路由级发现路径与发现基础 URL 均禁止，避免非网络目录伪装网络权限；
- 模型从空能力开始，只应用 Manifest 规则，不继承 Rust 静态表或公共模型目录；
- 规则的 `supported_protocols` 决定该模型实际产生哪些协议变体，未声明协议不会被试发或降级；
- 新模型必须同时增加身份和规则；只增加身份会封闭失败。

其他远程/手工发现类别可以在没有 Manifest 身份清单时省略 `models`；一旦提供非空或空数组，
其 presence 语义仍分别表示“显式身份清单”或“显式没有 Manifest 身份”，不能靠宿主猜测补齐。

新增普通模型且不升级软件的标准步骤：

1. 在目标入口的 `model_catalog.models` 添加精确 `model_id` 和 `display_name`；
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

模型规则中的 `usage_profile_refs`、`request_profile_refs`、`response_profile_refs` 与
`operation_refs` 同样遵循上述 presence 语义。显式空 `usage_profile_refs` 清除供应商专属统计特例
后仍恢复所选协议的标准 usage 基线；显式空 request/response/operation 引用则清为空集合。若只是要
继承路由或供应商默认，必须省略字段，不要写样板空数组。

## 5. 模型级唯一协议与自定义供应商规则

系统供应商的协议只来自编译后的 Manifest：

- 命中规则时必须得到一个 `selected_protocol`；
- 未命中规则时，只有 `default_binding_allowed` 和显式默认协议才能绑定；
- 旧连接上的 `selected_protocol` 不参与系统模型绑定；
- 一个服务可以同时声明多个协议 route，但当上游按模型系列分配入口时，每条 Exact 模型规则仍只能
  声明该系列真实支持的单一协议；服务级 route 集合不是模型级可任选协议集合；
- 请求开始后冻结 `provider_id + service_entry_id + credential_profile_id + model_id +
  protocol_profile_id + manifest_revision`；
- retry 只允许使用冻结身份，协议或清单修订变化直接报错；
- 失败后不测试第二协议、不降级、不静默切换。

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

Upload 在任何网络副作用之前完成全部资源预检。网络通道必须使用允许的 HTTPS 宿主和绝对安全
路径；每个通道必须声明精确 MIME 允许列表及有界数量，本地字节通道还必须声明有界字节数，
网络通道必须声明 `timeout_ms`，其截止时间覆盖签名握手、文件传输与响应解码的完整生命周期。
`remote_url_passthrough` 固定只允许不含凭据、端口和 fragment 的 HTTPS URL，并必须通过
`remote_url_host_suffixes` 声明精确宿主或点前缀子域后缀。资源上限不能由远端更新放宽。签名上传
没有已注册签名原语时必须失败，不能猜签名算法。Multipart/File Create 的 method、file field、
响应 JSON Pointer、引用模板、固定字段冲突和缓存时长均在激活前闭合验证，不留到请求期猜测。

每项操作从凭据解析与上传准备开始共享一个不可重置的绝对截止时间；同步请求与响应读取均受该
截止时间约束。异步任务冻结完整 target/operation/upload/credential/generation 快照。恢复、轮询与取消使用同一
快照；热更新只影响新任务。`generic_json_async_lifecycle` 必须显式声明 start/poll/cancel 方法、
路径、task/status/error/output JSON Pointer、终态集合、`poll_interval_ms`、`max_backoff_ms` 与
`timeout_ms`。连续瞬时观测失败使用指数退避，但绝不超过冻结的 `max_backoff_ms`；成功观测后计数
归零，任务总生命周期仍受 `timeout_ms` 封闭约束。

固定 `tool_targets` 与 `tool_target_templates` 使用同一套激活前验证：系统端点只允许无凭据、无
query/fragment 的 HTTPS URL，endpoint path 必须保持同源；输入 JSON Schema、输出资源 decoder、
固定/条件请求头、MCP 契约、凭据档和已注册执行原语均在模板展开前校验，模板不能成为绕过普通
目标安全边界的第二入口。

供应商返回 URL 型资源时，响应 decoder 必须声明 `download_host_suffixes`；异步 batch follow-up
必须独立声明 `url_host_suffixes`。`https_url` 只接受 HTTPS，`public_http_url` 仅在清单显式声明时
接受 HTTP/HTTPS。URL 在响应解码、异步跟随请求和 Agent 宿主实际下载前分别重新校验：禁止凭据、
片段、未授权 Host 和不安全后缀。通用 URL 资源只允许 Scheme 默认端口；异步 batch follow-up
还可使用与已验证、冻结启动端点的 Scheme、Host 和有效端口全部精确一致的 Origin。该例外来自请求目标，
绝不从响应 URL 学习端口；任何其他非默认端口仍失败关闭。远端签名更新不能扩大这些 Host 集合；新增下载域名仍需软件更新
和安全审核。

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

## 8. Fixtures 与测试证据

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
