# OpenCode 供应商配置与模型来源分析

- 分析目标：`D:\projects\OpenCode\opencode`
- 输出日期：2026-05-07
- 配套明细：`D:\openvulcan\vulcan-code\docs\opencode-provider-catalog-20260507.json`

## 1. 结论先看

- `opencode` 的 Provider 并不是写死在单一列表里直接调用，而是按“`models.dev` 基线 -> 插件模型补丁 -> 配置文件 -> 环境变量 -> 本地 auth 存储 -> 插件 auth loader -> provider 自定义 loader -> 再次覆盖配置 -> 少量动态发现”的顺序合成。核心在 `packages/opencode/src/provider/provider.ts:1086-1409`。
- 供应商元数据与大部分模型列表来自 `https://models.dev/api.json`，本地缓存 5 分钟，并在后台每 60 分钟刷新。核心在 `packages/opencode/src/provider/models.ts:105-186`。
- 真正“主动拉模型列表”的只有 `GitHub Copilot`；`GitLab Duo` 属于“静态基线 + 动态补齐”；自定义 Provider 完全依赖用户手工填写模型列表。
- Provider 配置本身并不写死，`provider.<id>` 支持 `api / npm / env / whitelist / blacklist / options / models`，模型级还支持 `provider.api / provider.npm / options / headers / variants`，所以接口地址、API Key、baseURL、headers、模型名单都能改。核心在 `packages/opencode/src/config/provider.ts:4-93`。
- 配置入口分成四类：配置文件、CLI、TUI/Web、HTTP API。HTTP API 只负责“列 Provider / 取认证方式 / 发起 OAuth / 回调 OAuth”，并不直接写死模型。核心在 `packages/opencode/src/server/routes/instance/provider.ts:14-122`。

## 2. 配置文件加载与写回流程

### 2.1 全局配置文件候选

`packages/opencode/src/config/config.ts:304-312` 的 `globalConfigFile()` 会按顺序选择：

1. `~/.config/opencode/opencode.jsonc`
2. `~/.config/opencode/opencode.json`
3. `~/.config/opencode/config.json`

### 2.2 项目配置与扩展目录

- `packages/opencode/src/config/paths.ts:12-23`：向上查找 `opencode.jsonc` / `opencode.json`。
- `packages/opencode/src/config/paths.ts:25-43`：向上查找 `.opencode` 目录，同时合并全局配置目录、家目录 `.opencode`、可选的 `OPENCODE_CONFIG_DIR`。
- `packages/opencode/src/config/config.ts:527-588`：把这些文件与目录配置依次合并进最终配置。

### 2.3 环境变量与远端配置

- `packages/opencode/src/config/config.ts:521-523`：`OPENCODE_CONFIG` 指向额外配置文件。
- `packages/opencode/src/config/config.ts:590-597`：`OPENCODE_CONFIG_CONTENT` 直接注入配置内容。
- `packages/opencode/src/config/config.ts:500-508`：认证项里若存在 `wellknown`，会请求 `${url}/.well-known/opencode` 合并远端配置。
- `packages/opencode/src/config/config.ts:618-637`：若存在 console/org 上下文，会请求 `${url}/api/config` 合并组织级配置，并记录 `consoleManagedProviders`。

### 2.4 写回方式

- `packages/opencode/src/config/config.ts:738-743`：`Config.update()` 写当前目录下的 `config.json`。
- `packages/opencode/src/config/config.ts:750-771`：`Config.updateGlobal()` 写回全局配置文件，并尽量保持 JSONC 结构。

## 3. Provider 配置结构

核心 Schema 在 `packages/opencode/src/config/provider.ts:4-93`。可配置字段重点如下：

| 层级 | 字段 | 说明 |
| --- | --- | --- |
| Provider | `api` | Provider 级默认接口地址覆盖 |
| Provider | `npm` | 指定 AI SDK Provider 包 |
| Provider | `env` | 认证环境变量名数组 |
| Provider | `whitelist` / `blacklist` | 模型白名单 / 黑名单 |
| Provider | `options.apiKey` | 直接在配置里给 API Key |
| Provider | `options.baseURL` | 强制覆盖运行时接口基址 |
| Provider | `options.enterpriseUrl` | GitHub Copilot Enterprise 场景 |
| Provider | `options.timeout` / `options.chunkTimeout` | 请求总超时 / SSE chunk 超时 |
| Provider | `models` | 模型级覆盖或追加 |
| Model | `id` / `name` / `family` / `release_date` | 模型标识与展示信息 |
| Model | `attachment` / `reasoning` / `temperature` / `tool_call` | 能力开关 |
| Model | `limit.context` / `limit.input` / `limit.output` | 上下文、输入、输出上限 |
| Model | `provider.api` / `provider.npm` | 模型级接口地址 / SDK provider 覆盖 |
| Model | `options` / `headers` / `variants` | 模型请求体选项、请求头和推理变体 |

## 4. Provider 合成顺序

核心在 `packages/opencode/src/provider/provider.ts:1086-1409`。实际顺序如下：

1. 读取 `models.dev` 数据库并转换为运行时 `Provider.Info` / `Provider.Model`。
2. 先执行插件 `provider.models()`，让插件在读取配置前就能修改基线模型集合。典型是 GitHub Copilot。
3. 读取 `cfg.provider`，把自定义 Provider 与模型补丁扩展进数据库。
4. 扫描环境变量；若 `provider.env` 中任一变量存在，则把该 Provider 标记为 `source=env`。代码在 `provider.ts:1270-1276`。
5. 扫描本地 auth 存储；若存在 API Key，则标记为 `source=api`。代码在 `provider.ts:1278-1289`。
6. 执行插件 auth loader，把 OAuth token / enterprise 参数之类转换成运行时 `options`。代码在 `provider.ts:1291-1309`。
7. 执行内置 `custom(dep)` loader，为特殊 Provider 增加动态 `getModel / vars / options / discoverModels`。代码在 `provider.ts:1312-1328`。
8. 再次覆盖配置文件里的 `name / env / options`，确保用户配置优先级最高。代码在 `provider.ts:1331-1338`。
9. 目前只对 `gitlab` 运行动态模型发现。代码在 `provider.ts:1341-1355`。
10. 过滤 `alpha / deprecated` 模型和用户配置的 whitelist / blacklist，并为 reasoning 模型补变体。代码在 `provider.ts:1364-1409`。

## 5. 运行时接口地址如何确定

最终请求地址不只看一处，而是 `resolveSDK()` 在 `packages/opencode/src/provider/provider.ts:1415-1453` 里按这个优先级解析：

1. `provider.options.baseURL`
2. `model.provider.api`
3. `provider.api`
4. `models.dev` 自带的 provider `api`

之后还会：

- 套 `varsLoader`，把 `${AZURE_RESOURCE_NAME}`、`${CLOUDFLARE_ACCOUNT_ID}` 这种变量替换掉。
- 再套一层环境变量 `${ENV_NAME}` 替换。
- 若 `options.apiKey` 为空且 Provider 已经拿到 `key`，则自动回填 `apiKey`。
- 最后合并模型级请求头 `model.headers`。

## 6. 配置入口流程

### 6.1 HTTP API

路由在 `packages/opencode/src/server/routes/instance/provider.ts:14-122`：

- `GET /provider`：列出可用 Provider + 已连接 Provider + 默认模型。
- `GET /provider/auth`：返回每个 Provider 可用的认证方式。
- `POST /provider/:providerID/oauth/authorize`：发起 OAuth。
- `POST /provider/:providerID/oauth/callback`：处理 OAuth 回调。

### 6.2 ProviderAuth 服务

`packages/opencode/src/provider/auth.ts:30-190` 做三件事：

1. 从插件 `auth` hook 生成认证方式列表。
2. 处理 `oauth` 与 `api` 两种认证流程。
3. 成功后把结果写入 `auth` 存储：API Key 写成 `type=api`，OAuth 写成 `type=oauth`。

### 6.3 CLI

- `packages/opencode/src/cli/cmd/providers.ts` 提供 `providers list / login / logout`。
- `login` 会先刷新 `models.dev`，再按插件暴露的认证方式走 OAuth 或 API Key 流程。
- `packages/opencode/src/cli/cmd/models.ts:29-38` 提供手动刷新模型数据库与查看模型列表。

### 6.4 TUI / Web

- `packages/opencode/src/cli/cmd/tui/component/dialog-provider.tsx:25-114`：TUI 的连接入口，会在认证成功后跳到模型选择对话框。
- `packages/app/src/components/settings-providers.tsx:29-230`：Web/App 设置页，区分已连接、热门 Provider、自定义 Provider。
- `packages/app/src/components/dialog-connect-provider.tsx`：连接标准 Provider。
- `packages/app/src/components/dialog-custom-provider.tsx` 与 `dialog-custom-provider-form.ts:51-150`：连接自定义 OpenAI-compatible Provider。

## 7. 自定义 Provider 配置流程

自定义 Provider 不是“从 provider 目录里挑一项”，而是用户直接创建新的 `provider.<id>` 条目。核心在 `packages/app/src/components/dialog-custom-provider-form.ts:51-150`。

表单要求：

- `providerID`
- `name`
- `baseURL`
- `apiKey` 或 `{env:ENV_VAR}`
- `models[]`（每一行至少 `id + name`）
- `headers[]`

写回结果形态：

```json
{
  "provider": {
    "my-provider": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "My Provider",
      "env": ["MY_PROVIDER_API_KEY"],
      "options": {
        "baseURL": "https://example.com/v1",
        "headers": {
          "X-Foo": "bar"
        }
      },
      "models": {
        "my-model": {
          "name": "My Model"
        }
      }
    }
  }
}
```

如果 API Key 不是 `{env:...}`，还会额外写进 auth 存储。

## 8. 模型列表来源：主动还是被动

| 场景 | 入口 | 行为 | 结论 |
| --- | --- | --- | --- |
| GitHub Copilot | `packages/opencode/src/plugin/github-copilot/models.ts:157-195` | 请求 `${baseURL}/models`，把返回结果转成运行时模型，并保留本地已有模型的名称/选项 | 主动拉列表，失败时回退静态表 |
| GitLab Duo | `packages/opencode/src/provider/provider.ts:611-688` | 先使用 `models.dev` 基线，再调用 `discoverWorkflowModels()` 补充 workflow 模型 | 半主动，属于“静态基线 + 动态补齐” |
| 自定义 Provider | `packages/app/src/components/dialog-custom-provider-form.ts:51-150` | 用户手工输入 `providerID / name / baseURL / apiKey / headers / models` 写入配置 | 完全被动，模型列表由用户手填 |

补充说明：`models.dev` 的本地缓存逻辑在 `packages/opencode/src/provider/models.ts:105-186`，默认缓存文件位于 opencode 数据目录，缓存 TTL 为 5 分钟，并每 60 分钟后台刷新。

## 9. 被动模型的上下文 / 工具 / 思考如何开启

- 被动模型的基础能力字段来自 `models.dev`：例如 `reasoning`、`tool_call`、`limit.context`、`limit.output`、`modalities`。它们会在 `fromModelsDevModel()` 转成运行时 `capabilities`，位置在 `packages/opencode/src/provider/provider.ts:989-1038`。
- “思考怎么开”并不全写在模型表本身，而是 `packages/opencode/src/provider/transform.ts` 根据 `provider npm + model id + release_date + reasoning capability` 生成 `variants()` 与默认 `options()`。
- 所以你要看一个被动模型是否支持上下文、工具、思考，最稳的判断路径是：

1. 看 `docs/opencode-provider-catalog-20260507.json` 中该模型的 `context_limit / output_limit / tool_call / reasoning`。
2. 再结合 `transform.ts` 的 provider 规则看它会不会自动补 `thinking` / `reasoningEffort` / `reasoningSummary` / `promptCacheKey`。

重点默认规则见下表：

| Provider | 运行时补丁 / 默认能力 | 位置 |
| --- | --- | --- |
| Anthropic | 默认附加 `anthropic-beta` 头；reasoning 模型生成 `thinking` 变体；非 Claude anthropic SDK 默认 `toolStreaming=false` | `provider.ts:139-151` / `transform.ts:638-692, 852-857` |
| OpenAI | `responses()` 优先；`gpt-5` 默认 `reasoningEffort=medium`，支持时自动带 `reasoningSummary=auto`；默认 `store=false` | `provider.ts:177-183` / `transform.ts:939-970` |
| Azure | 缺省走 `responses()`；无 `baseURL` 时要求 `resourceName`；默认 `store=false` 且注入 `promptCacheKey` | `provider.ts:201-241` / `transform.ts:868-885` |
| Google / Vertex | reasoning 模型默认开启 `thinkingConfig.includeThoughts=true`；Gemini 3 默认 `thinkingLevel=high` | `transform.ts:902-913` |
| OpenRouter / LLMGateway | 默认 `usage.include=true`；Gemini 3 默认 `reasoning.effort=high`；注入 `prompt_cache_key` | `transform.ts:872-879, 976-979` |
| Alibaba China | reasoning 模型默认 `enable_thinking=true`（排除 `kimi-k2-thinking`） | `transform.ts:926-937` |
| Z.AI / ZhipuAI | OpenAI-compatible 路径默认注入 `thinking={type: enabled, clear_thinking: false}` | `transform.ts:889-898` |
| GitHub Copilot | OAuth 后自定义 `fetch`/headers；按模型决定 `responses()` / `chat()` / `messages()`；还会远端拉 `/models` | `plugin/github-copilot/copilot.ts:60-173` / `provider.ts:191-199` |
| GitLab Duo | 根据 `GITLAB_INSTANCE_URL` 与 Token 运行，并可追加动态 workflow 模型 | `provider.ts:550-688` |

## 10. 认证插件与额外限制

- OpenAI / Codex 插件：`packages/opencode/src/plugin/codex.ts:14-17, 362-598`。支持 `ChatGPT Pro/Plus (browser)`、`ChatGPT Pro/Plus (headless)`、`Manually enter API Key` 三种方式；OAuth 模式会把模型过滤到 `gpt-5.5 / gpt-5.2 / gpt-5.3-codex / gpt-5.4 / gpt-5.4-mini`。
- GitHub Copilot 插件：`packages/opencode/src/plugin/github-copilot/copilot.ts:27-292`。支持 GitHub.com / GitHub Enterprise 两条 OAuth 线，Enterprise 会把 API 地址切到 `https://copilot-api.<domain>`。
- Azure 插件：`packages/opencode/src/plugin/azure.ts:3-19`。若环境变量里没有 `AZURE_RESOURCE_NAME`，认证时会额外提示输入 Resource Name。
- Cloudflare Workers AI / AI Gateway：`packages/opencode/src/plugin/cloudflare.ts:3-63`。会在认证时提示 `accountId` / `gatewayId`。

## 11. 全部 Provider 总表

> 说明：这里只列 Provider 级总表。每个 Provider 下的完整模型名单、上下文、tool、reasoning 能力已写入 `docs/opencode-provider-catalog-20260507.json`。

| Provider ID | 名称 | npm Provider | 默认接口地址 | 环境变量 | 模型数 | 模型列表来源 | 备注 |
| --- | --- | --- | --- | --- | ---: | --- | --- |
| 302ai | 302.AI | @ai-sdk/openai-compatible | https://api.302.ai/v1 | 302AI_API_KEY | 97 | 被动静态 | - |
| abacus | Abacus | @ai-sdk/openai-compatible | https://routellm.abacus.ai/v1 | ABACUS_API_KEY | 65 | 被动静态 | - |
| abliteration-ai | abliteration.ai | @ai-sdk/openai-compatible | https://api.abliteration.ai/v1 | ABLIT_KEY | 1 | 被动静态 | - |
| aihubmix | AIHubMix | @aihubmix/ai-sdk-provider | 运行时拼装 / SDK 默认 | AIHUBMIX_API_KEY | 38 | 被动静态 | - |
| alibaba | Alibaba | @ai-sdk/openai-compatible | https://dashscope-intl.aliyuncs.com/compatible-mode/v1 | DASHSCOPE_API_KEY | 48 | 被动静态 | - |
| alibaba-cn | Alibaba (China) | @ai-sdk/openai-compatible | https://dashscope.aliyuncs.com/compatible-mode/v1 | DASHSCOPE_API_KEY | 80 | 被动静态 | reasoning 模型默认 enable_thinking=true（除了 kimi-k2-thinking）以返回 reasoning_content |
| alibaba-coding-plan | Alibaba Coding Plan | @ai-sdk/openai-compatible | https://coding-intl.dashscope.aliyuncs.com/v1 | ALIBABA_CODING_PLAN_API_KEY | 9 | 被动静态 | - |
| alibaba-coding-plan-cn | Alibaba Coding Plan (China) | @ai-sdk/openai-compatible | https://coding.dashscope.aliyuncs.com/v1 | ALIBABA_CODING_PLAN_API_KEY | 9 | 被动静态 | - |
| amazon-bedrock | Amazon Bedrock | @ai-sdk/amazon-bedrock | 运行时拼装 / SDK 默认 | AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION, AWS_BEARER_TOKEN_BEDROCK | 94 | 被动静态 | 支持 region/profile/endpoint(baseURL)/bearer token 组合；部分模型会补 region 前缀 |
| anthropic | Anthropic | @ai-sdk/anthropic | 运行时拼装 / SDK 默认 | ANTHROPIC_API_KEY | 23 | 被动静态 | 默认附加 anthropic-beta 头以启用 interleaved thinking / fine-grained tool streaming |
| azure | Azure | @ai-sdk/azure | 运行时拼装 / SDK 默认 | AZURE_RESOURCE_NAME, AZURE_API_KEY | 108 | 被动静态 | 优先使用 provider.options.baseURL；否则需要 resourceName 拼接 Azure OpenAI 地址 |
| azure-cognitive-services | Azure Cognitive Services | @ai-sdk/azure | 运行时拼装 / SDK 默认 | AZURE_COGNITIVE_SERVICES_RESOURCE_NAME, AZURE_COGNITIVE_SERVICES_API_KEY | 101 | 被动静态 | 通过 AZURE_COGNITIVE_SERVICES_RESOURCE_NAME 动态拼接 baseURL |
| bailing | Bailing | @ai-sdk/openai-compatible | https://api.tbox.cn/api/llm/v1/chat/completions | BAILING_API_TOKEN | 2 | 被动静态 | - |
| baseten | Baseten | @ai-sdk/openai-compatible | https://inference.baseten.co/v1 | BASETEN_API_KEY | 14 | 被动静态 | - |
| berget | Berget.AI | @ai-sdk/openai-compatible | https://api.berget.ai/v1 | BERGET_API_KEY | 6 | 被动静态 | - |
| cerebras | Cerebras | @ai-sdk/cerebras | 运行时拼装 / SDK 默认 | CEREBRAS_API_KEY | 4 | 被动静态 | - |
| chutes | Chutes | @ai-sdk/openai-compatible | https://llm.chutes.ai/v1 | CHUTES_API_KEY | 70 | 被动静态 | - |
| clarifai | Clarifai | @ai-sdk/openai-compatible | https://api.clarifai.com/v2/ext/openai/v1 | CLARIFAI_PAT | 12 | 被动静态 | - |
| cloudferro-sherlock | CloudFerro Sherlock | @ai-sdk/openai-compatible | https://api-sherlock.cloudferro.com/openai/v1/ | CLOUDFERRO_SHERLOCK_API_KEY | 5 | 被动静态 | - |
| cloudflare-ai-gateway | Cloudflare AI Gateway | ai-gateway-provider | 运行时拼装 / SDK 默认 | CLOUDFLARE_API_TOKEN, CLOUDFLARE_ACCOUNT_ID, CLOUDFLARE_GATEWAY_ID | 75 | 被动静态 | 需要 Account ID + Gateway ID + API Token；provider ID 统一走 ai-gateway-provider |
| cloudflare-workers-ai | Cloudflare Workers AI | @ai-sdk/openai-compatible | https://api.cloudflare.com/client/v4/accounts/${CLOUDFLARE_ACCOUNT_ID}/ai/v1 | CLOUDFLARE_ACCOUNT_ID, CLOUDFLARE_API_KEY | 8 | 被动静态 | 需要 Account ID + API Key，URL 中包含账户路径变量 |
| cohere | Cohere | @ai-sdk/cohere | 运行时拼装 / SDK 默认 | COHERE_API_KEY | 12 | 被动静态 | - |
| cortecs | Cortecs | @ai-sdk/openai-compatible | https://api.cortecs.ai/v1 | CORTECS_API_KEY | 49 | 被动静态 | - |
| deepinfra | Deep Infra | @ai-sdk/deepinfra | 运行时拼装 / SDK 默认 | DEEPINFRA_API_KEY | 37 | 被动静态 | - |
| deepseek | DeepSeek | @ai-sdk/openai-compatible | https://api.deepseek.com | DEEPSEEK_API_KEY | 4 | 被动静态 | - |
| digitalocean | DigitalOcean | @ai-sdk/openai-compatible | https://inference.do-ai.run/v1 | DIGITALOCEAN_ACCESS_TOKEN | 64 | 被动静态 | - |
| dinference | DInference | @ai-sdk/openai-compatible | https://api.dinference.com/v1 | DINFERENCE_API_KEY | 5 | 被动静态 | - |
| drun | D.Run (China) | @ai-sdk/openai-compatible | https://chat.d.run/v1 | DRUN_API_KEY | 3 | 被动静态 | - |
| evroc | evroc | @ai-sdk/openai-compatible | https://models.think.evroc.com/v1 | EVROC_API_KEY | 13 | 被动静态 | - |
| fastrouter | FastRouter | @ai-sdk/openai-compatible | https://go.fastrouter.ai/api/v1 | FASTROUTER_API_KEY | 15 | 被动静态 | - |
| fireworks-ai | Fireworks AI | @ai-sdk/openai-compatible | https://api.fireworks.ai/inference/v1/ | FIREWORKS_API_KEY | 19 | 被动静态 | - |
| friendli | Friendli | @ai-sdk/openai-compatible | https://api.friendli.ai/serverless/v1 | FRIENDLI_TOKEN | 6 | 被动静态 | - |
| frogbot | FrogBot | @ai-sdk/openai-compatible | https://app.frogbot.ai/api/v1 | FROGBOT_API_KEY | 26 | 被动静态 | - |
| github-copilot | GitHub Copilot | @ai-sdk/openai-compatible | https://api.githubcopilot.com | GITHUB_TOKEN | 27 | 主动拉取 | OAuth 成功后会注入自定义 fetch / headers，并用 refresh token 访问 Copilot API |
| github-models | GitHub Models | @ai-sdk/openai-compatible | https://models.github.ai/inference | GITHUB_TOKEN | 55 | 被动静态 | - |
| gitlab | GitLab Duo | gitlab-ai-provider | 运行时拼装 / SDK 默认 | GITLAB_TOKEN | 15 | 半主动 | packages/opencode/src/provider/provider.ts 的 discoverModels() 只对 gitlab 生效, 动态发现失败不会阻塞已有静态模型列表 |
| google | Google | @ai-sdk/google | 运行时拼装 / SDK 默认 | GOOGLE_GENERATIVE_AI_API_KEY, GEMINI_API_KEY | 37 | 被动静态 | reasoning 模型默认注入 thinkingConfig.includeThoughts=true；Gemini 3 默认 thinkingLevel=high |
| google-vertex | Vertex | @ai-sdk/google-vertex | 运行时拼装 / SDK 默认 | GOOGLE_VERTEX_PROJECT, GOOGLE_VERTEX_LOCATION, GOOGLE_APPLICATION_CREDENTIALS | 42 | 被动静态 | 只有存在 project / credentials 时才 autoload |
| google-vertex-anthropic | Vertex (Anthropic) | @ai-sdk/google-vertex/anthropic | 运行时拼装 / SDK 默认 | GOOGLE_VERTEX_PROJECT, GOOGLE_VERTEX_LOCATION, GOOGLE_APPLICATION_CREDENTIALS | 12 | 被动静态 | 只有存在 project / credentials 时才 autoload |
| groq | Groq | @ai-sdk/groq | 运行时拼装 / SDK 默认 | GROQ_API_KEY | 27 | 被动静态 | - |
| helicone | Helicone | @ai-sdk/openai-compatible | https://ai-gateway.helicone.ai/v1 | HELICONE_API_KEY | 90 | 被动静态 | - |
| hpc-ai | HPC-AI | @ai-sdk/openai-compatible | https://api.hpc-ai.com/inference/v1 | HPC_AI_API_KEY | 3 | 被动静态 | - |
| huggingface | Hugging Face | @ai-sdk/openai-compatible | https://router.huggingface.co/v1 | HF_TOKEN | 24 | 被动静态 | - |
| iflowcn | iFlow | @ai-sdk/openai-compatible | https://apis.iflow.cn/v1 | IFLOW_API_KEY | 14 | 被动静态 | - |
| inception | Inception | @ai-sdk/openai-compatible | https://api.inceptionlabs.ai/v1/ | INCEPTION_API_KEY | 2 | 被动静态 | - |
| inference | Inference | @ai-sdk/openai-compatible | https://inference.net/v1 | INFERENCE_API_KEY | 9 | 被动静态 | - |
| io-net | IO.NET | @ai-sdk/openai-compatible | https://api.intelligence.io.solutions/api/v1 | IOINTELLIGENCE_API_KEY | 17 | 被动静态 | - |
| jiekou | Jiekou.AI | @ai-sdk/openai-compatible | https://api.jiekou.ai/openai | JIEKOU_API_KEY | 61 | 被动静态 | - |
| kilo | Kilo Gateway | @ai-sdk/openai-compatible | https://api.kilo.ai/api/gateway | KILO_API_KEY | 355 | 被动静态 | - |
| kimi-for-coding | Kimi For Coding | @ai-sdk/anthropic | https://api.kimi.com/coding/v1 | KIMI_API_KEY | 3 | 被动静态 | - |
| kiro | Kiro | kiro-acp-ai-provider | https://q.us-east-1.amazonaws.com | KIRO_API_KEY | 12 | 被动静态 | - |
| kuae-cloud-coding-plan | KUAE Cloud Coding Plan | @ai-sdk/openai-compatible | https://coding-plan-endpoint.kuaecloud.net/v1 | KUAE_API_KEY | 1 | 被动静态 | - |
| llama | Llama | @ai-sdk/openai-compatible | https://api.llama.com/compat/v1/ | LLAMA_API_KEY | 7 | 被动静态 | - |
| llmgateway | LLM Gateway | @ai-sdk/openai-compatible | https://api.llmgateway.io/v1 | LLMGATEWAY_API_KEY | 190 | 被动静态 | - |
| lmstudio | LMStudio | @ai-sdk/openai-compatible | http://127.0.0.1:1234/v1 | LMSTUDIO_API_KEY | 3 | 被动静态 | - |
| lucidquery | LucidQuery AI | @ai-sdk/openai-compatible | https://lucidquery.com/api/v1 | LUCIDQUERY_API_KEY | 2 | 被动静态 | - |
| meganova | Meganova | @ai-sdk/openai-compatible | https://api.meganova.ai/v1 | MEGANOVA_API_KEY | 19 | 被动静态 | - |
| minimax | MiniMax (minimax.io) | @ai-sdk/anthropic | https://api.minimax.io/anthropic/v1 | MINIMAX_API_KEY | 6 | 被动静态 | - |
| minimax-cn | MiniMax (minimaxi.com) | @ai-sdk/anthropic | https://api.minimaxi.com/anthropic/v1 | MINIMAX_API_KEY | 6 | 被动静态 | - |
| minimax-cn-coding-plan | MiniMax Coding Plan (minimaxi.com) | @ai-sdk/anthropic | https://api.minimaxi.com/anthropic/v1 | MINIMAX_API_KEY | 6 | 被动静态 | - |
| minimax-coding-plan | MiniMax Coding Plan (minimax.io) | @ai-sdk/anthropic | https://api.minimax.io/anthropic/v1 | MINIMAX_API_KEY | 6 | 被动静态 | - |
| mistral | Mistral | @ai-sdk/mistral | 运行时拼装 / SDK 默认 | MISTRAL_API_KEY | 28 | 被动静态 | - |
| mixlayer | Mixlayer | @ai-sdk/openai-compatible | https://models.mixlayer.ai/v1 | MIXLAYER_API_KEY | 5 | 被动静态 | - |
| moark | Moark | @ai-sdk/openai-compatible | https://moark.com/v1 | MOARK_API_KEY | 2 | 被动静态 | - |
| modelscope | ModelScope | @ai-sdk/openai-compatible | https://api-inference.modelscope.cn/v1 | MODELSCOPE_API_KEY | 7 | 被动静态 | - |
| moonshotai | Moonshot AI | @ai-sdk/openai-compatible | https://api.moonshot.ai/v1 | MOONSHOT_API_KEY | 7 | 被动静态 | - |
| moonshotai-cn | Moonshot AI (China) | @ai-sdk/openai-compatible | https://api.moonshot.cn/v1 | MOONSHOT_API_KEY | 7 | 被动静态 | - |
| morph | Morph | @ai-sdk/openai-compatible | https://api.morphllm.com/v1 | MORPH_API_KEY | 3 | 被动静态 | - |
| nano-gpt | NanoGPT | @ai-sdk/openai-compatible | https://nano-gpt.com/api/v1 | NANO_GPT_API_KEY | 525 | 被动静态 | - |
| nebius | Nebius Token Factory | @ai-sdk/openai-compatible | https://api.tokenfactory.nebius.com/v1 | NEBIUS_API_KEY | 49 | 被动静态 | - |
| neuralwatt | Neuralwatt | @ai-sdk/openai-compatible | https://api.neuralwatt.com/v1 | NEURALWATT_API_KEY | 14 | 被动静态 | - |
| nova | Nova | @ai-sdk/openai-compatible | https://api.nova.amazon.com/v1 | NOVA_API_KEY | 2 | 被动静态 | - |
| novita-ai | NovitaAI | @ai-sdk/openai-compatible | https://api.novita.ai/openai | NOVITA_API_KEY | 99 | 被动静态 | - |
| nvidia | Nvidia | @ai-sdk/openai-compatible | https://integrate.api.nvidia.com/v1 | NVIDIA_API_KEY | 82 | 被动静态 | - |
| ollama-cloud | Ollama Cloud | @ai-sdk/openai-compatible | https://ollama.com/v1 | OLLAMA_API_KEY | 39 | 被动静态 | - |
| openai | OpenAI | @ai-sdk/openai | 运行时拼装 / SDK 默认 | OPENAI_API_KEY | 52 | 被动静态 | 优先走 responses() 模式 |
| opencode | OpenCode Zen | @ai-sdk/openai-compatible | https://opencode.ai/zen/v1 | OPENCODE_API_KEY | 58 | 被动静态 | 未登录时会过滤收费模型，仅保留零成本模型，并注入 apiKey=public |
| opencode-go | OpenCode Go | @ai-sdk/openai-compatible | https://opencode.ai/zen/go/v1 | OPENCODE_API_KEY | 14 | 被动静态 | - |
| openrouter | OpenRouter | @openrouter/ai-sdk-provider | https://openrouter.ai/api/v1 | OPENROUTER_API_KEY | 186 | 被动静态 | 默认包含 usage；Gemini 3 在 openrouter 下默认 reasoning.effort=high；会注入 prompt_cache_key |
| ovhcloud | OVHcloud AI Endpoints | @ai-sdk/openai-compatible | https://oai.endpoints.kepler.ai.cloud.ovh.net/v1 | OVHCLOUD_API_KEY | 11 | 被动静态 | - |
| perplexity | Perplexity | @ai-sdk/perplexity | 运行时拼装 / SDK 默认 | PERPLEXITY_API_KEY | 4 | 被动静态 | - |
| perplexity-agent | Perplexity Agent | @ai-sdk/openai | https://api.perplexity.ai/v1 | PERPLEXITY_API_KEY | 18 | 被动静态 | - |
| poe | Poe | @ai-sdk/openai-compatible | https://api.poe.com/v1 | POE_API_KEY | 132 | 被动静态 | - |
| privatemode-ai | Privatemode AI | @ai-sdk/openai-compatible | http://localhost:8080/v1 | PRIVATEMODE_API_KEY, PRIVATEMODE_ENDPOINT | 5 | 被动静态 | - |
| qihang-ai | QiHang | @ai-sdk/openai-compatible | https://api.qhaigc.net/v1 | QIHANG_API_KEY | 9 | 被动静态 | - |
| qiniu-ai | Qiniu | @ai-sdk/openai-compatible | https://api.qnaigc.com/v1 | QINIU_API_KEY | 91 | 被动静态 | - |
| regolo-ai | Regolo AI | @ai-sdk/openai-compatible | https://api.regolo.ai/v1 | REGOLO_API_KEY | 13 | 被动静态 | - |
| requesty | Requesty | @ai-sdk/openai-compatible | https://router.requesty.ai/v1 | REQUESTY_API_KEY | 38 | 被动静态 | - |
| sap-ai-core | SAP AI Core | @jerome-benoit/sap-ai-provider-v2 | 运行时拼装 / SDK 默认 | AICORE_SERVICE_KEY | 23 | 被动静态 | 依赖 AICORE_SERVICE_KEY，支持 deploymentId / resourceGroup 等扩展参数 |
| scaleway | Scaleway | @ai-sdk/openai-compatible | https://api.scaleway.ai/v1 | SCALEWAY_API_KEY | 16 | 被动静态 | - |
| siliconflow | SiliconFlow | @ai-sdk/openai-compatible | https://api.siliconflow.com/v1 | SILICONFLOW_API_KEY | 74 | 被动静态 | - |
| siliconflow-cn | SiliconFlow (China) | @ai-sdk/openai-compatible | https://api.siliconflow.cn/v1 | SILICONFLOW_CN_API_KEY | 81 | 被动静态 | - |
| stackit | STACKIT | @ai-sdk/openai-compatible | https://api.openai-compat.model-serving.eu01.onstackit.cloud/v1 | STACKIT_API_KEY | 8 | 被动静态 | - |
| stepfun | StepFun | @ai-sdk/openai-compatible | https://api.stepfun.com/v1 | STEPFUN_API_KEY | 4 | 被动静态 | - |
| submodel | submodel | @ai-sdk/openai-compatible | https://llm.submodel.ai/v1 | SUBMODEL_INSTAGEN_ACCESS_KEY | 9 | 被动静态 | - |
| synthetic | Synthetic | @ai-sdk/openai-compatible | https://api.synthetic.new/openai/v1 | SYNTHETIC_API_KEY | 33 | 被动静态 | - |
| tencent-coding-plan | Tencent Coding Plan (China) | @ai-sdk/openai-compatible | https://api.lkeap.cloud.tencent.com/coding/v3 | TENCENT_CODING_PLAN_API_KEY | 8 | 被动静态 | - |
| tencent-tokenhub | Tencent TokenHub | @ai-sdk/openai-compatible | https://tokenhub.tencentmaas.com/v1 | TENCENT_TOKENHUB_API_KEY | 1 | 被动静态 | - |
| the-grid-ai | The Grid AI | @ai-sdk/openai-compatible | https://api.thegrid.ai/v1 | THEGRIDAI_API_KEY | 3 | 被动静态 | - |
| togetherai | Together AI | @ai-sdk/togetherai | 运行时拼装 / SDK 默认 | TOGETHER_API_KEY | 18 | 被动静态 | - |
| upstage | Upstage | @ai-sdk/openai-compatible | https://api.upstage.ai/v1/solar | UPSTAGE_API_KEY | 3 | 被动静态 | - |
| v0 | v0 | @ai-sdk/vercel | 运行时拼装 / SDK 默认 | V0_API_KEY | 3 | 被动静态 | - |
| venice | Venice AI | venice-ai-sdk-provider | 运行时拼装 / SDK 默认 | VENICE_API_KEY | 64 | 被动静态 | 默认注入 promptCacheKey；small model 会 disableThinking=true |
| vercel | Vercel AI Gateway | @ai-sdk/gateway | 运行时拼装 / SDK 默认 | AI_GATEWAY_API_KEY | 247 | 被动静态 | - |
| vivgrid | Vivgrid | @ai-sdk/openai | https://api.vivgrid.com/v1 | VIVGRID_API_KEY | 13 | 被动静态 | - |
| vultr | Vultr | @ai-sdk/openai-compatible | https://api.vultrinference.com/v1 | VULTR_API_KEY | 5 | 被动静态 | - |
| wafer.ai | Wafer | @ai-sdk/openai-compatible | https://pass.wafer.ai/v1 | WAFER_API_KEY | 3 | 被动静态 | - |
| wandb | Weights & Biases | @ai-sdk/openai-compatible | https://api.inference.wandb.ai/v1 | WANDB_API_KEY | 18 | 被动静态 | - |
| xai | xAI | @ai-sdk/xai | 运行时拼装 / SDK 默认 | XAI_API_KEY | 26 | 被动静态 | 优先走 responses() 模式 |
| xiaomi | Xiaomi | @ai-sdk/openai-compatible | https://api.xiaomimimo.com/v1 | XIAOMI_API_KEY | 5 | 被动静态 | - |
| xiaomi-token-plan-ams | Xiaomi Token Plan (Europe) | @ai-sdk/openai-compatible | https://token-plan-ams.xiaomimimo.com/v1 | XIAOMI_API_KEY | 5 | 被动静态 | - |
| xiaomi-token-plan-cn | Xiaomi Token Plan (China) | @ai-sdk/openai-compatible | https://token-plan-cn.xiaomimimo.com/v1 | XIAOMI_API_KEY | 5 | 被动静态 | - |
| xiaomi-token-plan-sgp | Xiaomi Token Plan (Singapore) | @ai-sdk/openai-compatible | https://token-plan-sgp.xiaomimimo.com/v1 | XIAOMI_API_KEY | 5 | 被动静态 | - |
| zai | Z.AI | @ai-sdk/openai-compatible | https://api.z.ai/api/paas/v4 | ZHIPU_API_KEY | 13 | 被动静态 | 默认 thinking={type: enabled, clear_thinking: false} |
| zai-coding-plan | Z.AI Coding Plan | @ai-sdk/openai-compatible | https://api.z.ai/api/coding/paas/v4 | ZHIPU_API_KEY | 5 | 被动静态 | - |
| zenmux | ZenMux | @ai-sdk/openai-compatible | https://zenmux.ai/api/v1 | ZENMUX_API_KEY | 96 | 被动静态 | - |
| zhipuai | Zhipu AI | @ai-sdk/openai-compatible | https://open.bigmodel.cn/api/paas/v4 | ZHIPU_API_KEY | 12 | 被动静态 | 默认 thinking={type: enabled, clear_thinking: false} |
| zhipuai-coding-plan | Zhipu AI Coding Plan | @ai-sdk/openai-compatible | https://open.bigmodel.cn/api/coding/paas/v4 | ZHIPU_API_KEY | 5 | 被动静态 | - |

## 12. 我对接入你们当前 AI Coding GUI 的建议

- 不要把 Provider 和模型写死在代码里，应该学习 `opencode` 的模式，拆成“Provider 实例配置 + 模型来源 + auth 存储 + 运行时覆盖”。
- 模型配置器至少要支持：选择供应商、填写 `baseURL`、填写/引用 API Key、拉取模型列表、手工补模型、黑白名单、请求头。
- 对 reasoning / tool / context 的展示不要只看模型名，要把模型静态能力和 ProviderTransform 这种运行时默认规则合并后再展示。
- 对 OpenAI-compatible Provider，要优先保留 `baseURL + apiKey + headers + models[]` 的通用入口；这类 Provider 在 `opencode` 生态里占绝大多数。
