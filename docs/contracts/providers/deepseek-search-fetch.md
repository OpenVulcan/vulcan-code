# DeepSeek 独立搜索与网页抓取

核对日期：2026-10-18。

## 契约归属与同步条件

上游契约归 DeepSeek 官方文档所有；接口路径、鉴权、字段、限制、响应及收费规则变化时，必须重新核对：

- [使用指南](https://api-docs.deepseek.com/zh-cn/guides/search/)；
- [Search API](https://api-docs.deepseek.com/zh-cn/api/search/)；
- [Fetch API](https://api-docs.deepseek.com/zh-cn/api/fetch/)。

本地可发布字段以 `provider-manifest-v2.schema.json` 和 Rust 编译器为准。执行声明的唯一来源是
`provider-manifests/manifests/deepseek.json`，不得再维护按供应商名称分流的搜索执行器。

## 配置与能力归属

`deepseek` 的 `official-api` 连接使用既有 `api-key` 凭据档，自动提供 `official-api.search` 与
`official-api.fetch` 两个可选目标；不需要另建搜索账号或填写第二份 Key。禁用或待校验的连接不发布目标。

Search 接入现有 `web_search`，Fetch 接入现有 `web_extract`。系统和会话继续保存各自已有的单选目标及
开关。增加候选不会回写这些设置，也不依据其他供应商中同名的 DeepSeek 模型推断独立接口可用性。
当前会话模型已有原生搜索时，继续遵守既有原生搜索优先的工具暴露规则。

## 请求与响应映射

两个接口均使用 `https://api.deepseek.com`、POST JSON 和同一份 `Authorization: Bearer` 凭据。

| 能力 | 路径 | 本地参数与响应 |
| --- | --- | --- |
| Search | `/v1/tools/search` | `query`、`max_results`、可选 `ori_question` 和 `localization`；将 `max_results` 映射为 `count`，返回有序标题、链接与长摘要 |
| Fetch | `/v1/tools/fetch` | 单个 `url`；将响应 `content` 原样作为 Markdown 文本返回 |

Search 的 `query` 为 1～128 个 Unicode 字符且不能全为空白，条数为 1～50，默认 10。
可选原始问题允许 `null`，字符串长度为 1～128。查询中的站点、语言、时间及引号操作符原样传递。
本地搜索固定发送 `need_content=false`，使用官方面向模型的 `summary`；需要全文时由 Fetch 获取。
不能读取默认恒为空的 `content` 作为搜索摘要，也不能猜测候选响应字段。

搜索结果中的 HTTP(S) 地址仅作为定位符返回，不触发宿主下载；仍拒绝带用户名、密码及非网页方案的地址。
资源下载和结构化提取的既有 HTTPS 限制继续由各自边界校验。

Fetch 接受不超过 8192 字符、包含主机且不含用户名或密码的 HTTP/HTTPS 地址。上游只返回 `title` 与
Markdown `content`，因此复用文本输出契约，不伪造包含来源 URL 的响应数组。空正文按既有文本解码
契约拒绝。目标网页由 DeepSeek 服务端访问，宿主的带凭据请求始终发往冻结的 API 入口。

两个操作分别具有 60 秒宿主总超时。成功请求可能计费，声明为非幂等操作，不在失败后探测其他协议或
自动重放。HTTP 错误沿用通用的状态、错误码和脱敏消息投影。

## 收费与限速事实

指南核对时，两个独立接口均按成功且非空的请求计费，每 1000 次人民币 5 元；搜索条数不影响单次价格。
搜索无结果、抓取无正文、失败或超时不计费。两个接口分别按账号限制 5 QPS，Fetch 另外按目标域名限速。
这些是官方服务规则，宿主不根据搜索返回量虚构模型 token 用量。

指南还说明，现有 DeepSeek Harness、Claude Code、Codex 服务端工具渠道的相关调用于北京时间
2026-11-01 00:00 起按请求收费；该日期说明不能用于推断独立接口在此之前免费。

## 验证证据

- `deepseek-search-generic.json`：实际生产请求及响应 Profile、默认条数、可空原始问题、Bearer 头、
  HTTP 搜索链接、长摘要及空正文的规范化。
- `deepseek-fetch-generic.json`：单 URL 请求、Bearer 头、HTTP 网页地址及 Markdown 正文透传。
- Rust 行为测试：连接未配置、已配置、禁用及待校验时的目标准入，Unicode 长度、条数、URL 凭据和
  URL 长度边界，以及搜索定位符和资源下载之间的校验边界。

这些 fixture 在本地严格 HTTP 服务上验证生产执行器、请求与响应 Profile 的线路契约。

2026-10-18 使用用户授权的 `DEEPSEEK_API_KEY`，按生产声明的请求结构分别调用两个官方独立接口
一次，无重试：Search 返回 HTTP 200 和 3 条结果，三条 `summary` 均非空，`content` 均为空；
Fetch 返回 HTTP 200 和 6402 字符的 Markdown 正文。真实调用仅保存状态与长度等脱敏元数据，
不保存密钥或网页正文。该验收验证真实账户和接口响应；桌面界面的实际调用另行验证。

同日补充无工具 Chat 验证：使用 `deepseek-flash` 请求 `/chat/completions`，请求体仅包含模型、
用户消息、非流式开关与输出上限，不注册工具，不设置工具选择或搜索开关，也不调用独立接口。
要求实际联网查询 VS Code 最新正式发布版本，返回 HTTP 200，正常结束，未返回工具调用；
模型明确说明当前会话没有联网搜索工具，未执行查询。该结果只描述这次 Chat 请求，不能据此
断言其他官方服务端工具渠道的行为。两个内置 DeepSeek 模型的正常默认协议仍为 `/responses`。

## 交付检查

2026-10-18 完成以下验证，各项测试数量分别记录，不相加统计：

- DeepSeek 定向测试 29 项通过；通用 JSON 同步执行测试 6 项通过；工具目标准入测试 5 项通过。
- Agent 工具桥接测试 21 项通过，包含原生搜索优先、正文与搜索结果呈现及资源安全边界。
- 清单发布门禁通过：22 个供应商声明有效，107 个生产目标具备覆盖，45 个执行 fixture 通过，
  索引构建、签名及验签通过。
- 执行 `make.ps1 sync`，72 个清单相关资源文件与 `output` 逐字节一致；
  执行 `make.ps1 core-build`，Debug Core 编译通过；格式及差异空白检查通过。

最终连续五轮聚焦审核均无本次范围内遗留问题：

1. 清单与索引：修订、原始字节长度、SHA-256、执行原语及映射声明一致；仅更新 DeepSeek 索引项。
2. 目录与协议：既有模型、会话协议、能力、凭据及上传声明与提交基线一致，原有映射未改写。
3. 运行时权威：复用冻结的连接与凭据引用，禁用及待校验连接不发布目标，执行使用声明的总超时，
   不增加备用协议或自动重试。
4. 工具暴露：服务快照、设置页与会话控件按能力接收新目标；已选择并开启时进入现有 AI 工具链；
   Fetch 正文进入文本结果投影，搜索摘要进入结构化搜索投影，资源下载仍保留既有限制。
5. 状态与交付：选项读取不回写保存设置，测试、发布门禁、资源同步与编译均通过。

当前运行中的 Core 未重启；新执行代码需要重新启动 Core 后加载。未将静态界面链路检查或直接
接口实测记为桌面端到端验收。
