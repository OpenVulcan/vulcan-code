# 模型官网定价核查

2026-11-10 复查原有的 34 个待配置组，补齐 9 组，移除 1 组。目前共有 195 个费用组，其中 171 组有随包 Token 参考价，24 组待配置。费用组的适用平台、档位和币种见 [价格数据](../upstream/official-model-pricing.v1.json)。

随包参考价是核查日的静态快照。长上下文、缓存保留期、限时优惠、地域、模式和销售渠道不同，都可能改变实际账单。目录只收录一种可核实的参考档位，按张、按秒及存储时长报价保留在核查结果中。峰谷价按请求结束时间和所设时区估算，法定节假日沿用周规则。

Gemini 3 Pro 和 Grok 4.1 Fast 的归组沿用各自的历史参考价。Google 已关闭 Gemini 3 Pro Preview，xAI 将退役的 Grok 4.1 Fast 调用名转到 Grok 4.3 并按 4.3 收费。腾讯云 TokenHub 将 DeepSeek V4 Flash 和 0731 列在同一人民币报价行；DeepSeek 官方将旧 V4 Flash 请求转到 V4.1 Flash 1017 并按新版收费。 [Google 退役公告](https://ai.google.dev/gemini-api/docs/deprecations?authuser=6) · [xAI 退役公告](https://docs.x.ai/developers/migration/may-15-retirement) · [腾讯云 TokenHub](https://cloud.tencent.com/document/product/1823/130055/) · [DeepSeek 价目](https://api-docs.deepseek.com/quick_start/pricing/)

## 全表归组复核

| 比对项 | 费用组处理 |
| --- | --- |
| Claude Haiku 4.5、Opus 4.5/4.6、Sonnet 4.6 的有日期与无日期条目 | 同一小版本的价格相同，日期写法由规则自动归组。只有日期写法的 Opus 4/4.1、Sonnet 4/4.5 将原报价移到无日期费用组。 [Claude 型号版本规则](https://platform.claude.com/docs/en/about-claude/models/model-ids-and-versions) |
| Claude Haiku 5.5、Opus 4.8/5.5、Sonnet 5.5 的点号与连字符，及 Fable、Opus、Sonnet 的 Thinking 写法 | 使用连字符的主型号。版本点号、日期和 Thinking 写法自动命中，Thinking 用量计入对应模型。 [Claude 思考模式](https://platform.claude.com/docs/en/build-with-claude/thinking) |
| `claude-haiku-5` | Anthropic 公开型号目录没有独立的 Haiku 5 条目。已从费用目录和计费候选型号段移除，保留 Haiku 4.5 和 5.5 的各自费用组。 [Anthropic 模型目录](https://platform.claude.com/docs/en/models/overview) |
| DeepSeek V3.2、V4 Flash、V4 Pro、V4.1 Flash | V3.2 的完整日期写法自动匹配。V4 Pro 的 0813 与主型号报价相同。按用户指定，V4 Flash 的 Vision Exp 和 Fast 共享 V4 Flash 0731 组的人民币参考价，V4.1 Flash Eco 共享 V4.1 Flash 组的参考价。火山方舟的经济版实际有独立收费。 [DeepSeek 发布记录](https://api-docs.deepseek.com/updates/) · [腾讯云 TokenHub](https://cloud.tencent.com/document/product/1823/130055/) · [火山方舟价格](https://console.volcengine.com/ark/region%3Acn-beijing/docs/model-pricing) |
| 豆包 Seed 2.0 Code、Gemini 3 Flash、Gemini 3.1 Pro/Flash-Lite、GLM 5.2 Fast、Hy4 | 末尾 Preview 或 Preview Latest 统一归到主型号。已有报价的合并前核对了各项参考费率。 |
| Step 5、Qwen 3.6 Max、Moonshot 8K/32K/128K Vision | 原来只有 Preview 写法，改用去后缀的主型号。有报价的 Step 5 同时迁移原报价。 |
| Seed 2.1 Pro/Turbo、Qwen 3.8 Max | 同型号的人民币和美元参考单价合到对应主型号。阿里云确认 0902 是 Qwen 3.8 Max 的同价快照。 [火山方舟型号记录](https://docs.volcengine.com/docs/ark/agent-plan-personal-model-release?lang=en) · [阿里云升级通知](https://www.alibabacloud.com/en/notice/model_studio_update_notice_for_qwen38max_models_863?_p_lc=1) |
| Hy3、Kimi K3、Jev 1.13、Step 3.5 Flash、Qwen3-Coder 480B | 核对目录型号与参考价后，分别将 `hunyuan-hy3`、`k3`、`jev-1.13-20260917`、`step-3.5-flash-2603` 和 `qwen3-coder-480b` 归到对应主型号。 |
| Nano Banana 2 与 Nano Banana 2 Lite | 分别归到 Gemini 3.1 Flash Image 和 Flash Lite Image。图片与文字的计费口径分别保留。 [Google 图像模型](https://ai.google.dev/gemini-api/docs/image-generation?authuser=108&hl=en) |
| Qwen 3.7 Max 的 06-08、MiniMax M2.7 Highspeed、Kimi K2.7 Code Highspeed、Muse Contributor | 保留独立价格。Qwen 3.7 Max 的通用名与 06-08 对应不同快照，其余路线的参考费率与基础型号不同。 [阿里云价目](https://www.alibabacloud.com/help/en/model-studio/model-pricing) · [MiniMax 价目](https://platform.minimax.io/subscribe/token-plan?tab=api-enterprise) |
| Devstral Medium/2、Kimi K3 的 256K 路线、DeepSeek Reasoner | Devstral 是不同版本。`k3-256k` 与 `kimi-k3-256k` 共享一组独立的 256K 路线配置。历史 DeepSeek Reasoner 曾随官方路由切换，不统一套用某一代的价格。 [Mistral 型号目录](https://docs.mistral.ai/models) · [Kimi Code 模型说明](https://www.kimi.com/en/help/kimi-code/models) · [DeepSeek 发布记录](https://api-docs.deepseek.com/updates/) |

## 本轮补齐的参考价

单价按每百万 Token 计。缓存写入列中的「—」表示来源未单列费用。

| 模型组 | 币种 | 输入 | 输出 | 缓存命中 | 缓存写入 | 适用平台 |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| `glm-5.2-fast` | USD | 3 | 10.25 | 0.50 | — | [Command Code](https://commandcode.ai/models/glm-5-2-fast) |
| `glm-5.2-fast` | CNY | 16 | 56 | 4 | — | [阿里云百炼北京](https://help.aliyun.cn/zh/model-studio/glm-5-2-fast) |
| `fugu-ultra` | USD | 5 | 30 | 0.50 | — | [Command Code](https://commandcode.ai/models/fugu-ultra) · [Sakana 标准档](https://console.sakana.ai/pricing) · [OpenRouter 0615 快照](https://openrouter.ai/sakana/fugu-ultra-20260615) |
| `hy3-paid` | USD | 0.14 | 0.58 | 0.035 | — | [Command Code](https://commandcode.ai/models) |
| `nemotron-3-ultra-550b-a55b` | USD | 0.60 | 2.40 | 0.12 | — | [Command Code](https://commandcode.ai/models/nemotron-3-ultra-550b-a55b) |
| `qwen3.6-max` | USD | 1.30 | 7.80 | 0.26 | 1.63 | [Command Code](https://commandcode.ai/models)。百炼另有历史人民币刊例，其路线已列入 [退役公告](https://www.alibabacloud.com/en/notice/model_studio_notice_on_the_postponed_retirement_of_selected_legacy_models_7d0?_p_lc=1)。 |
| `neohorse-1-9b` | CNY | 0.60 | 1 | 0.10 | — | [基元律动公开价目](https://tokenrhythm.studio/api/models) 的刊例价，符合条件的充值账号当前限免。 |
| `moonshot-v1-8k-vision` | USD | 0.20 | 2 | — | — | [Merge Gateway](https://docs.merge.dev/merge-gateway/models/catalog) |
| `moonshot-v1-32k-vision` | USD | 1 | 3 | — | — | [Merge Gateway](https://docs.merge.dev/merge-gateway/models/catalog) |
| `moonshot-v1-128k-vision` | USD | 2 | 5 | — | — | [Merge Gateway](https://docs.merge.dev/merge-gateway/models/catalog) |

GLM 5.2 Fast 的美元与人民币采用两家平台分别公布的单价。Moonshot V1 Vision Preview 在 2025-01-13 上线，符合收录时间要求。当前 Kimi API 价目已不列出该系列，目录采用 Merge Gateway 标注的具体路线参考价。 [Kimi 发布记录](https://platform.kimi.com/blog/posts/changelog) · [Kimi API 价目](https://platform.kimi.ai/docs/pricing/chat)

## 待配置项

| 未加入随包 Token 参考价的模型组 | 官网核查结果 |
| --- | --- |
| `egg-alpha` | 基元律动确认向符合条件的付费账号开放灰度试用。核查时公开价目接口未返回此型号的单价。 [基元律动公告](https://tokenrhythm.studio/docs/updates) · [公开价目接口](https://tokenrhythm.studio/api/models) |
| `gemini-3.1-flash-lite-image` | Gemini 3.1 Flash Lite Image 的图片输出为 $30/百万图片 Token，1K 图约 $0.0336。文字和图片使用不同费率。 [Google](https://ai.google.dev/gemini-api/docs/pricing) |
| `gemini-3.1-flash-image` | Gemini 3.1 Flash Image 的图片输出为 $60/百万图片 Token，1K 图约 $0.067，另有文字输出费率。 [Google](https://ai.google.dev/gemini-api/docs/pricing) |
| `gpt-image-2` | 每百万图片输入 $4、图片输出 $15、图片缓存 $1。文字输入 $2.50、文字缓存 $0.625。 [OpenAI](https://developers.openai.com/api/docs/pricing) |
| `gpt-image-2.5-flare`、`gpt-image-2.5-sunburst` | 每百万图片输入 $8、图片输出 $30、图片缓存 $2。每百万文字输入 $5、文字缓存 $1.25。 [Flare](https://developers.openai.com/api/docs/models/gpt-image-2.5-flare) · [Sunburst](https://developers.openai.com/api/docs/models/gpt-image-2.5-sunburst) |
| `gpt-transcribe` | 转录按音频时长计费，官网列价 $0.0045/分钟。 [OpenAI](https://developers.openai.com/api/docs/models/gpt-transcribe) |
| `grok-imagine-image` | 图片输入 $0.002/张，图片输出 $0.02/张。 [xAI](https://docs.x.ai/developers/pricing) |
| `grok-imagine-image-quality` | 原路线于 2026-11-02 退役。兼容请求转到 Imagine Image 2.0 Low，1K 输出 $0.04/张，2K 输出 $0.06/张，编辑输入 $0.01/张。 [xAI 迁移公告](https://docs.x.ai/developers/migration/imagine-image-quality-nov-2) |
| `grok-imagine-video`、`grok-imagine-video-1.5` | 视频按秒和分辨率计价。480p 输出分别从 $0.05/秒和 $0.08/秒起。 [xAI](https://docs.x.ai/developers/pricing) |
| `kimi-k3-256k`、`kimi-for-coding`、`kimi-for-coding-highspeed` | Kimi Code 按会员和额度计。三条路线分别对应固定 256K 的 K3、K2.8 Preview、K2.7 Code HighSpeed，额度消耗随路线变化。 [Kimi Code 模型说明](https://www.kimi.com/en/help/kimi-code/models) · [会员说明](https://www.kimi.com/en/help/kimi-code/membership-guide) |
| `mimo-v2.5-asr` | 按输入音频时长计费，国内 ¥0.50/小时，海外 $0.074/小时。当前费用估算按 Token 计，因此该型号保留待配置。 [小米 MiMo API 价目](https://mimo.mi.com/docs/price) |
| `moonshot-v1-8k`、`moonshot-v1-32k`、`moonshot-v1-128k` | 文本版系列于 2024-01-31 公测，早于收录起点。Merge Gateway 的每百万输入/输出参考价依次为 $0.20/$2、$1/$3、$2/$5。 [Kimi 发布记录](https://platform.kimi.com/blog/posts/kimi-latest) · [Merge Gateway](https://docs.merge.dev/merge-gateway/models/catalog) |
| `qwen-image-2.0`、`wan2.7-image` | 百炼中国内地每张 ¥0.20，其他地域有各自单价。 [Qwen Image](https://help.aliyun.com/zh/model-studio/qwen-image-2-0) · [Wan](https://help.aliyun.com/zh/model-studio/wan2-7-image) |
| `seedream-5.0`、`seedream-5.0-pro` | OpenDesign 的托管图片路线使用套餐额度。火山方舟独立 API 参考价为 Lite ¥0.22/张，Pro 单图 ¥0.30 或 ¥0.60，图层拆分 ¥0.15 或 ¥0.30/层，Pro 自第 2 张输入图起 ¥0.02/张。 [OpenDesign](https://open-design.ai/pricing/) · [火山方舟](https://console.volcengine.com/ark/region%3Acn-beijing/docs/model-pricing) |
| `text-embedding-v1`、`text-embedding-v2`、`text-embedding-v3` | 按 2025-01-01 起收录的规则保留待配置。百炼北京同步单价为 v1/v2 每百万输入 ¥0.70、v3 ¥0.50。 [百炼价格](https://help.aliyun.com/zh/model-studio/text-embedding-synchronous-api) · [发布记录](https://help.aliyun.com/zh/model-studio/newly-released-models) |

生效目录移除了 9 条 2025-01-01 之前公开的旧参考价，包括 Claude 3.5 Haiku/Sonnet、Cohere Command R/R+、OpenAI o1/o1-mini、DeepSeek Chat/V3 和 Gemini 2.0 Flash。Gemini 2.0 Flash 以首次公开预览的日期计。旧用量和用户覆盖价继续由原有存储保存。 [移除依据](../upstream/official-model-pricing.v1.json)

## MiMo 官方 API 接入补充

复核确认 V2.6 Flash 和 Pro 的实时参考价与官网一致。V2.6 Pro Ultraspeed 使用独立的双币种费用组。每百万 Token 的输入、输出和缓存命中价分别为国内 ¥30、¥60、¥0.25，海外 $4.35、$8.70、$0.036。核查日缓存写入限时免费。 [小米 MiMo API 价目](https://mimo.mi.com/docs/price)

`mimo-v2.5-tts`、`mimo-v2.5-tts-voiceclone` 和 `mimo-v2.5-tts-voicedesign` 在核查日限时免费，随包参考价记录当日的零费率。官网尚未公布活动结束后的价格。Token Plan 使用独立的积分额度。 [小米 MiMo API 价目](https://mimo.mi.com/docs/price) · [Token Plan 额度规则](https://mimo.mi.com/docs/en-US/quick-start/faq/token-plan/Usage%26Quota)

## 上下文分档

- GPT-5.5、5.6 Sol/Terra/Luna、6 Astra/Sol/Luna、6.1 Sol/Terra 使用 272,000 输入 Token 阈值，超过后整笔请求使用长上下文价。5.6 Sol 默认记录常规价，官方当期促销截止 2026-11-30。 [GPT-5.5](https://developers.openai.com/api/docs/models/gpt-5.5) · [GPT-5.6 Sol](https://developers.openai.com/api/docs/models/gpt-5.6-sol) · [GPT-6.1 Sol](https://developers.openai.com/api/docs/models/gpt-6.1-sol)
- Claude Haiku 5.5 超过 100,000 输入 Token 后使用长上下文价。阈值包含普通输入、缓存读和缓存写。参考价采用 5 分钟缓存写入档。 [Anthropic 定价](https://platform.claude.com/docs/en/about-claude/pricing)
- Kimi Code 的 `k3-256k` 固定在 256K，`k3` 可随会员等级使用 1M，上下文路线涉及不同额度消耗。Kimi API 公布的 `kimi-k3` 为 1M 标准按量报价，当前公开价目未提供 256K 前后的两套现金单价。具有单独报价的渠道可为通用 K3 配置自己的分档。 [Kimi Code 模型说明](https://www.kimi.com/en/help/kimi-code/models) · [Kimi API 定价](https://platform.kimi.ai/docs/pricing/chat)

## 百炼 Token Plan 补齐项

已逐项核对个人版和团队版的模型。两种套餐都有 Token 用量时，按模型的标准按量参考价估算；用户配置价格覆盖时沿用覆盖。套餐实际消耗请查看百炼控制台的 Credits 明细。

- `decision-model-preview` 按现有 Preview 归组匹配 `decision-model`。北京和新加坡目前均限时免费，官网未公布长期单价或截止日。 [决策模型价格](https://www.alibabacloud.com/help/en/model-studio/decision-model-preview)
- `MiniMax-M2.5` 仍在百炼团队版清单中，因此仅在 `alibaba` 供应商范围补参考价。百炼北京刊例每百万 Token 的输入、输出、缓存读分别为 **¥2.1 / ¥8.4 / ¥0.42**，美元刊例分别为 **$0.304 / $1.213 / $0.061**，不支持显式缓存写。共享清单的 MiniMax M2.7 下限保持有效。 [团队版模型](https://www.alibabacloud.com/help/en/model-studio/token-plan-team-overview) · [人民币刊例](https://help.aliyun.com/zh/model-studio/minimax-m2-5) · [美元刊例](https://www.alibabacloud.com/help/en/model-studio/minimax-m2-5)

媒体参考价在「模型定价」中按原单位展示。用量库目前记录 Token，不能据此推算实际的媒体张数、时长或字符数。

| 模型 | 单位或档位 | 北京（CNY） | 新加坡（CNY） |
| --- | --- | ---: | ---: |
| `qwen-audio-3.0-asr-flash` | 每秒输入音频 | 0.00022 | 0.00026 |
| `qwen-audio-3.0-tts-plus` | 每万输入字符 | 1.4 | 1.49884 |
| `qwen-image-2.0` | 每张输出 | 0.2 | 0.256873 |
| `qwen-image-2.0-pro` | 每张输出 | 0.5 | 0.550443 |
| `qwen-image-3.0-pro` | 每张：输入 / 1K 输出 / 2K 输出 | 0.02 / 0.25 / 0.5 | 0.022483 / 0.299768 / 0.562065 |
| `wan2.7-image` | 每张输出 | 0.2 | 0.224826 |
| `wan2.7-image-pro` | 每张输出 | 0.5 | 0.562065 |
| `happyhorse-1.1-t2v`、`happyhorse-1.1-i2v`、`happyhorse-1.1-r2v` | 每秒输出：480p / 720p / 1080p | 0.45 / 0.9 / 1.2 | 0.524594 / 1.049188 / 1.348956 |
| `wan3.0-video` | 每秒输出：480p / 720p / 1080p | 0.3 / 0.6 / 1.2 | 0.37471 / 0.74942 / 1.49884 |

[百炼按量价目](https://help.aliyun.com/zh/model-studio/model-pricing) · [语音识别价目](https://help.aliyun.com/zh/model-studio/qwen-audio-3-0-asr-flash) · [万相视频价目](https://help.aliyun.com/zh/model-studio/wan3-0-video)

### 百炼 DeepSeek 专属费率

以下是每百万 Token 的人民币刊例，顺序为「非缓存输入 / 输出 / 缓存命中」。套餐和按量入口都按实际接入地区匹配标准参考价；只录入人民币的条目由统一汇率换算美元。仅使用隐式缓存，缓存写入不单列收费。

| 模型 | 北京忙时 | 北京闲时 | 新加坡忙时 | 新加坡闲时 |
| --- | --- | --- | --- | --- |
| `deepseek-v4.1-flash`、`deepseek-v4.1-flash-1017` | 2 / 8 / 0.2 | 1 / 4 / 0.1 | 2.188 / 8.75 / 0.219 | 1.094 / 4.375 / 0.109 |
| `deepseek-v4-pro` | 12 / 24 / 1（全天） | 同左 | 17.986 / 35.972 / 1.499（全天） | 同左 |
| `deepseek-v4-pro-0813` | 9 / 27 / 0.9 | 4.5 / 13.5 / 0.45 | 9.625 / 28.875 / 0.963 | 4.813 / 14.438 / 0.481 |
| `deepseek-v4-flash-0731` | 3 / 9 / 0.3 | 1.5 / 4.5 / 0.15 | 3.208 / 9.625 / 0.3208 | 1.604 / 4.813 / 0.1604 |

百炼闲时为 UTC+8 每天 22:00 至次日 08:00；周末使用同样的时段。V4 Pro 原版与 0813 在百炼分别计费；V4.1 Flash 和 1017 则复用同一份价格。0731 缓存价按百炼公布的当期输入价 10% 计算。百炼的模型定价目录显示北京参考价，统计根据接入入口选用对应地区报价。

[V4.1 Flash 及 1017](https://help.aliyun.com/zh/model-studio/deepseek-v4-1-flash) · [V4 Pro 及 0813](https://help.aliyun.com/zh/model-studio/deepseek-v4-pro) · [百炼按量价目](https://help.aliyun.com/zh/model-studio/model-pricing) · [上下文缓存规则](https://help.aliyun.com/zh/model-studio/context-cache)
