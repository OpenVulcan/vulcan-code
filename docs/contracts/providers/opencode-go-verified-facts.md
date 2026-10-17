# OpenCode Go 账号授权与额度查询

核对日期：2026-10-17。本文记录上游契约及本仓映射；字段形状仍以实际 Schema、Rust 类型及生产客户端为准。
上游 Console 更新设备授权、推理入口、工作区规则或 Go 计量结构时，必须重新核对本文与对应合成协议测试。

## 证据来源

- OpenCode `v2` 固定提交 `4617210822bdbb31e5749751afbfb4584ccedac8`，
  `packages/core/src/plugin/provider/opencode.ts`：设备授权、组织作用域、刷新、用户及组织读取。
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

Console API Key 使用 `oc_sk_` 前缀及标准 `ApiKey` 材料；该 Key 已绑定工作区，
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

Console 入口保留 `go-oauth-api` 身份，提供 `account-oauth` 与 `console-api-key` 两个凭据档。
两者共用已定义的目录和协议，新增 Key 不改写既有连接的入口或凭据档。
两类凭据均使用 `https://opencode.ai/inference/go`；OAuth 发送 Bearer 令牌、
`x-org-id` 与当前会话自己的 `x-opencode-session`。不经重定向转发凭据，不将新令牌发往旧 Go 网关。
普通开放模型走 `/openai/v1/chat/completions`，GPT 5.6 Luna 走 `/openai/v1/responses`。

当前 Console 目录取既有已定义模型、核对当日 Go 套餐成员和线上模型身份的交集，共 8 项：
DeepSeek V4 Pro、GLM-5.2、GLM-5.3、GPT-5.6 Luna、Kimi K2.7 Code、Kimi K3、MiniMax M3、Qwen3.8 Max。
所有协议均按官方逐模型端点绑定；不从模型名称猜测。
旧 `go-api/api-key` 入口、目录、模型选择与持久连接保持原有身份，不能将 Key 自动改写为 OAuth。
旧 `sk-` Key 未完成新版 Console 用量接口的真实验证，旧入口仍不声明用量支持；
已验证的 `oc_sk_` Key 应选择 Console 入口的 API Key 凭据档。

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
