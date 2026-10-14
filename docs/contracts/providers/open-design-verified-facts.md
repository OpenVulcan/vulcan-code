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
  `open_design_wallet`，以独立 `controlKey` 查询美元钱包余额，不把套餐窗口等同于钱包余额。
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
  `pending` 按服务端间隔继续；`expired` 终止；`approved` 必须包含两把密钥、账户身份和
  精确的生产 `apiUrl`、`linkUrl`。未知状态、错误响应和重定向都直接失败。
- 官方 CLI 的本地捕获显示模型目录请求为 `GET /v1/models?all=true`，使用 `Bearer runtimeKey`。
  `runtimeKey` 只通过推理值访问器投影到模型发现和推理；钱包专用访问器选择 `controlKey`。
  两者没有已验证的刷新协议，不制造 Refresh Token 或过期时间。失效后重新授权。
- 钱包：`GET https://amr-api.open-design.ai/api/v1/wallet/balance`，
  `Authorization: Bearer <controlKey>`。金额来自字符串 `balanceUsd`，固定币种 USD；
  保留所有十进制位，拒绝数字类型、负数、指数及非法小数。`updatedAt` 是上游更新时间，
  宿主快照的 `observed_at` 使用实际查询时间。没有证据的用量与套餐窗口保持空。
- 已提供的测试 API Key 访问正式钱包接口返回 401，因此保留 API Key 档的余额不支持声明。
  Auth 钱包查询的 401/403 明确要求重新登录；其他错误保留为暂时不可用，不回显可能包含凭据的上游正文。
- 成对凭据使用 `open_design_auth` 类型并由既有事务原子保存；运行密钥、控制密钥、
  用户 ID、邮箱分别写入已有类型化列。SQLite 新增该类型的完整性约束，应用结构版本为 2。
  遵循仓库 V0.1 规则：初始化前拒绝旧版本，不迁移、不覆盖旧数据库的版本号或内容。

[脱敏授权证据](open-design-auth-20261015-evidence.json) 区分正式服务响应、官方 CLI 的模拟
协议验证和本地回归。正式服务已验证创建 201、等待轮询 200 与到期状态 `expired`；
本次设备挑战未获账户批准，真实登录完成及余额值仍未验收。仓库不保存设备秘密、实际密钥、
账户身份或实际余额。

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

- OpenDesign 定向测试通过，覆盖生产模型投影、设备创建和轮询、取消后迟到响应、到期、
  不可信目标、秘密不回显、控制凭据选择及十进制金额格式。
- 实际 SQLite 凭据测试通过，涵盖 OpenDesign 双密钥往返读写；数据库结构测试通过，
  拒绝旧版本时保留既有版本号和记录。
- `vulcan-provider-adapters`、`vulcan-service-kernel`、`vulcan-workspace-store` 的
  `cargo check --all-targets` 通过；清单符合性与随包加载 23 项测试通过。
- 前端 TypeScript、变更文件 ESLint、余额展示 12 项既有测试通过；
  后端中文字符串及文案断言检查通过。
- 源清单、索引及 Schema 已通过 `make.ps1 sync` 同步，产物与权威资源字节一致。
- 发布门禁通过：22 份供应商清单、42 个执行夹具，以及临时签名、验签和摘要校验。
