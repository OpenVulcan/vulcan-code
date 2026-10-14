# OpenDesign 套餐与供应商接入证据

本记录对应 2026-10-14 的官方页面、指定客户端源码和测试凭据实测。清单字段以
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

- 供应商 ID：`open-design`；服务入口：`design-plan`；凭据档：`api-key`。
- 基地址：`https://amr-link.open-design.ai/v1`；唯一已选协议：OpenAI Chat Completions。
- 鉴权：`Authorization: Bearer <credential>`；凭据格式前缀：`sk-amr-`。
- 目录采用 `discovery_only` 与精确规则交集。没有规则的远端模型不会被猜测启用；
  Key 不再返回的模型不会由本地清单补回。
- 官方营销目录列出 12 个模型，测试 Key 实际只返回 11 个。
  **Hunyuan H4 Preview 未返回，未取得精确 ID、容量及可执行协议证据，因此不创建可用规则。**
- 原生账单经登录后的 Vela CLI 读取，当前 API Key 接入未取得对应查询协议；
  `account_usage_query` 明确为 `unsupported`，不复用其他供应商的余额接口。
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
