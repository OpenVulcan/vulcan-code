# 用量统计随包模型清单

随包共 **211** 个模型组，其中 **187** 组有美元或人民币参考价，**24** 组待配置。用量记录中的其他模型会另外进入费用表，也可以在搜索栏输入完整模型 ID 后直接编辑。

费用表按模型组共用一份配置。匹配时取最后一个斜杠后的模型 ID，忽略英文字母大小写。先匹配精确型号或别名，再按连字符边界匹配最长的已知型号段。例如 glm-5.3-flash-xxx 优先匹配 glm-5.3-flash。日期、Preview 和 Claude 兼容写法也按规则归组。别名栏只列需要单独映射的名称。

每百万 Token 的输入、输出、缓存读和缓存写都可以分别填写美元和人民币。只填一个币种时，按统计页的统一汇率折算另一个币种。两个币种都填写时，各自采用填写值。
可用精确供应商 ID 为模型绑定独立价格。该供应商优先使用专属价格，其余请求沿用共享价格。模板会将基础价、谷价和上下文档位复制到草稿，保存后可单独编辑。
启用谷价后，以请求终止时间和模型配置的固定 UTC 偏移匹配每周时段，区间包含开始时间，不含结束时间。时段之外使用基础价。
上下文分档以单次请求的总输入 Token 选档。等于阈值时使用前一档，超过阈值后整笔请求使用新档。基础价和谷价可分别配置多个档位。
百炼媒体模型的按张、按秒及按字符刊例，以及百炼专属的 MiniMax M2.5 报价，见 [官网价格核查](official-price-audit-2026-11-10.md#百炼-token-plan-补齐项)。

| 定价模型组 | 随包参考价 | 已声明别名 |
| --- | --- | --- |
| `beam-free` | USD | — |
| `claude-fable-5` | USD | `claude-5-fable-20260609` |
| `claude-fable-5-1` | USD | `fable-5.1`、`fable5-1` |
| `claude-haiku-4-5` | USD | — |
| `claude-haiku-5-5` | USD | — |
| `claude-mythos-5` | USD | — |
| `claude-opus-4` | USD | — |
| `claude-opus-4-1` | USD | — |
| `claude-opus-4-5` | USD | — |
| `claude-opus-4-6` | USD | — |
| `claude-opus-4-7` | USD | — |
| `claude-opus-4-8` | USD | — |
| `claude-opus-5` | USD | — |
| `claude-opus-5-5` | USD | — |
| `claude-sonnet-4` | USD | — |
| `claude-sonnet-4-5` | USD | — |
| `claude-sonnet-4-6` | USD | — |
| `claude-sonnet-5` | USD | — |
| `claude-sonnet-5-5` | USD | — |
| `codestral-2508` | USD | — |
| `codex-mini` | USD | — |
| `command-a` | USD | — |
| `crouton-alpha-free` | USD | — |
| `decision-model` | USD | — |
| `deepseek-reasoner` | USD | — |
| `deepseek-v3.1` | USD | — |
| `deepseek-v3.2` | USD | — |
| `deepseek-v4-flash` | CNY | — |
| `deepseek-v4-pro` | USD / CNY | — |
| `deepseek-v4.1-flash` | USD / CNY | `deepseek-flash` |
| `devstral-2-2512` | USD | — |
| `devstral-medium` | USD | — |
| `devstral-small-1.1` | USD | — |
| `devstral-small-2-2512` | USD | — |
| `doubao-seed-2-0-code` | USD | — |
| `doubao-seed-2-0-lite` | USD | — |
| `doubao-seed-2-0-mini` | USD | — |
| `doubao-seed-2-0-pro` | USD | — |
| `doubao-seed-code` | USD | — |
| `egg-alpha` | 待配置 | — |
| `fugu-ultra` | USD | — |
| `gemini-2.5-flash` | USD | — |
| `gemini-2.5-flash-lite` | USD | — |
| `gemini-2.5-pro` | USD | — |
| `gemini-3-flash` | USD | — |
| `gemini-3-pro` | USD | — |
| `gemini-3.1-flash-image` | 待配置 | `nano-banana-2` |
| `gemini-3.1-flash-lite` | USD | — |
| `gemini-3.1-flash-lite-image` | 待配置 | `nano-banana-2-lite` |
| `gemini-3.1-pro` | USD | — |
| `gemini-3.5-flash` | USD | — |
| `gemini-3.5-flash-lite` | USD | — |
| `gemini-3.6-flash` | USD | — |
| `gemini-3.7-flash` | USD | — |
| `gemini-3.8-flash` | USD | — |
| `gemini-4-argon` | USD | — |
| `glm-4.6` | USD | — |
| `glm-4.7` | USD | — |
| `glm-5` | USD | — |
| `glm-5-turbo` | USD | — |
| `glm-5.1` | USD | — |
| `glm-5.2` | USD | — |
| `glm-5.2-fast` | USD / CNY | — |
| `glm-5.3` | USD | — |
| `glm-5.3-flash` | USD | — |
| `glm-5.3-flashx` | USD | — |
| `glm-5v-turbo` | USD | — |
| `gpt-5.4` | USD | — |
| `gpt-5.4-mini` | USD | — |
| `gpt-5.4-nano` | USD | — |
| `gpt-5.5` | USD | `gpt-5-5` |
| `gpt-5.6-luna` | USD | `gpt-5-6-luna` |
| `gpt-5.6-sol` | USD | `gpt-5-6-sol` |
| `gpt-5.6-terra` | USD | `gpt-5-6-terra` |
| `gpt-6-astra` | USD | — |
| `gpt-6-luna` | USD | — |
| `gpt-6-sol` | USD | — |
| `gpt-6.1-sol` | USD | `gpt-6-1-sol` |
| `gpt-6.1-terra` | USD | `gpt-6-1-terra` |
| `gpt-image-2` | 待配置 | — |
| `gpt-image-2.5-flare` | 待配置 | — |
| `gpt-image-2.5-sunburst` | 待配置 | — |
| `gpt-transcribe` | 待配置 | — |
| `grok-3` | USD | — |
| `grok-3-mini` | USD | — |
| `grok-4` | USD | — |
| `grok-4-1` | USD | — |
| `grok-4.20` | USD | — |
| `grok-4.3` | USD | — |
| `grok-4.5` | USD | — |
| `grok-4.5-build` | USD | — |
| `grok-4.6` | USD | — |
| `grok-4.7` | USD | — |
| `grok-build-0.1` | USD | — |
| `grok-code-fast-1` | USD | — |
| `grok-imagine-image` | 待配置 | — |
| `grok-imagine-image-quality` | 待配置 | — |
| `grok-imagine-video` | 待配置 | — |
| `grok-imagine-video-1.5` | 待配置 | — |
| `happyhorse-1.1-i2v` | CNY（按媒体单位） | — |
| `happyhorse-1.1-r2v` | CNY（按媒体单位） | — |
| `happyhorse-1.1-t2v` | CNY（按媒体单位） | — |
| `hy3` | USD | `hunyuan-hy3` |
| `hy3-paid` | USD | — |
| `hy4` | CNY | — |
| `iceman-contributor-free` | USD | — |
| `inkling` | USD | — |
| `inkling-small` | USD | — |
| `jev-1.13.0` | USD | `jev-1.13-20260917` |
| `jev-1.14.0` | USD | — |
| `kimi-for-coding` | 待配置 | — |
| `kimi-for-coding-highspeed` | 待配置 | — |
| `kimi-k2-0905` | USD | — |
| `kimi-k2-thinking` | USD | — |
| `kimi-k2-turbo` | USD | — |
| `kimi-k2.5` | USD | — |
| `kimi-k2.6` | USD | — |
| `kimi-k2.7-code` | USD | — |
| `kimi-k2.7-code-highspeed` | USD | — |
| `kimi-k3` | USD / CNY | `k3` |
| `kimi-k3-256k` | 待配置 | `k3-256k` |
| `kimi-k3.1-pro` | USD | — |
| `laguna-s-2.1-free` | USD | — |
| `ling-3.0-flash-sante:free` | USD | — |
| `longcat-2.0` | USD / CNY | — |
| `longcat-2.0:free` | USD | — |
| `magistral-medium` | USD | — |
| `magistral-small` | USD | — |
| `mimo-v2-flash` | USD | — |
| `mimo-v2-pro` | USD | — |
| `mimo-v2.5` | USD | — |
| `mimo-v2.5-asr` | 待配置 | — |
| `mimo-v2.5-pro` | USD | — |
| `mimo-v2.5-tts` | USD / CNY | — |
| `mimo-v2.5-tts-voiceclone` | USD / CNY | — |
| `mimo-v2.5-tts-voicedesign` | USD / CNY | — |
| `mimo-v2.6-flash` | USD / CNY | — |
| `mimo-v2.6-pro` | USD / CNY | — |
| `mimo-v2.6-pro-ultraspeed` | USD / CNY | — |
| `minimax-m2.7` | USD | — |
| `minimax-m2.7-highspeed` | USD | — |
| `minimax-m3` | USD | — |
| `minimax-m3.1-flash` | USD | — |
| `mistral-large-3-2512` | USD | — |
| `mistral-medium-3.1` | USD | — |
| `mistral-medium-3.5` | USD | — |
| `mistral-small-3.2-24b` | USD | — |
| `mistral-small-4` | USD | — |
| `moonshot-v1-128k` | 待配置 | — |
| `moonshot-v1-128k-vision` | USD | — |
| `moonshot-v1-32k` | 待配置 | — |
| `moonshot-v1-32k-vision` | USD | — |
| `moonshot-v1-8k` | 待配置 | — |
| `moonshot-v1-8k-vision` | USD | — |
| `muse-spark-1.1` | USD | — |
| `muse-spark-1.2` | USD | — |
| `muse-spark-1.2-contributor` | USD | — |
| `muse-spark-1.3` | USD | — |
| `muse-spark-1.3-contributor` | USD | — |
| `muse-wonder` | USD | — |
| `nemotron-3-ultra-550b-a55b` | USD | — |
| `neohorse-1-9b` | CNY | — |
| `o3` | USD | — |
| `o3-mini` | USD | — |
| `o3-pro` | USD | — |
| `o4-mini` | USD | — |
| `qwen-audio-3.0-asr-flash` | CNY（按媒体单位） | — |
| `qwen-audio-3.0-tts-plus` | CNY（按媒体单位） | — |
| `qwen-image-2.0` | CNY（按媒体单位） | — |
| `qwen-image-2.0-pro` | CNY（按媒体单位） | — |
| `qwen-image-3.0-pro` | CNY（按媒体单位） | — |
| `qwen3-235b-a22b` | USD | — |
| `qwen3-32b` | USD | — |
| `qwen3-coder-480b-a35b-instruct` | USD | `qwen3-coder-480b` |
| `qwen3-coder-flash` | USD | — |
| `qwen3-coder-next` | USD | — |
| `qwen3-coder-plus` | USD | — |
| `qwen3-max` | USD | — |
| `qwen3-rerank` | CNY | — |
| `qwen3.5-flash` | CNY | — |
| `qwen3.5-plus` | USD | — |
| `qwen3.6-flash` | USD | — |
| `qwen3.6-max` | USD | — |
| `qwen3.6-plus` | USD | — |
| `qwen3.7-flash` | CNY | — |
| `qwen3.7-max` | USD | — |
| `qwen3.7-max-2026-06-08` | CNY | — |
| `qwen3.7-plus` | USD | — |
| `qwen3.7-text-embedding` | CNY | — |
| `qwen3.7-text-embedding-flash` | CNY | — |
| `qwen3.7-text-rerank` | CNY | — |
| `qwen3.8-27b` | CNY | — |
| `qwen3.8-flash` | CNY | — |
| `qwen3.8-max` | USD / CNY | — |
| `qwen4-max` | 待配置 | `qwen4.0-max-preview` |
| `qwq-32b` | USD | — |
| `qwq-plus` | USD | — |
| `seed-2.1-pro` | USD / CNY | `doubao-seed-2-1-pro` |
| `seed-2.1-turbo` | USD / CNY | `doubao-seed-2-1-turbo` |
| `seedream-5.0` | 待配置 | — |
| `seedream-5.0-pro` | 待配置 | — |
| `step-3.5-flash` | USD | — |
| `step-3.7-flash` | USD | — |
| `step-5` | CNY | — |
| `text-embedding-v1` | 待配置 | — |
| `text-embedding-v2` | 待配置 | — |
| `text-embedding-v3` | 待配置 | — |
| `text-embedding-v4` | CNY | — |
| `wan2.7-image` | CNY（按媒体单位） | — |
| `wan2.7-image-pro` | CNY（按媒体单位） | — |
| `wan3.0-video` | CNY（按媒体单位） | — |

## GPT 费用表清理清单

下列 **54** 个旧条目已移出费用表。数字 GPT 型号从 `gpt-5.4` 起保留。可确定且仍保留主型号的历史用量继续按主型号归组。原始用量保留，已退役或无法确定具体型号时不套用其他型号的参考价。

| 已移除的独立条目 | 历史用量的定价归组 |
| --- | --- |
| `gpt-4.1` | 版本低于保留下限 |
| `gpt-4.1-mini` | 版本低于保留下限 |
| `gpt-4.1-nano` | 版本低于保留下限 |
| `gpt-5` | 版本低于保留下限 |
| `gpt-5-codex` | 版本低于保留下限 |
| `gpt-5-codex-high` | 版本低于保留下限 |
| `gpt-5-codex-low` | 版本低于保留下限 |
| `gpt-5-codex-medium` | 版本低于保留下限 |
| `gpt-5-codex-mini` | 版本低于保留下限 |
| `gpt-5-codex-mini-high` | 版本低于保留下限 |
| `gpt-5-codex-mini-medium` | 版本低于保留下限 |
| `gpt-5-high` | 版本低于保留下限 |
| `gpt-5-low` | 版本低于保留下限 |
| `gpt-5-medium` | 版本低于保留下限 |
| `gpt-5-mini` | 版本低于保留下限 |
| `gpt-5-minimal` | 版本低于保留下限 |
| `gpt-5-nano` | 版本低于保留下限 |
| `gpt-5.1` | 版本低于保留下限 |
| `gpt-5.1-codex` | 版本低于保留下限 |
| `gpt-5.1-codex-max` | 版本低于保留下限 |
| `gpt-5.1-codex-max-high` | 版本低于保留下限 |
| `gpt-5.1-codex-max-xhigh` | 版本低于保留下限 |
| `gpt-5.1-codex-mini` | 版本低于保留下限 |
| `gpt-5.1-high` | 版本低于保留下限 |
| `gpt-5.1-low` | 版本低于保留下限 |
| `gpt-5.1-medium` | 版本低于保留下限 |
| `gpt-5.1-minimal` | 版本低于保留下限 |
| `gpt-5.2` | 版本低于保留下限 |
| `gpt-5.2-codex` | 版本低于保留下限 |
| `gpt-5.2-codex-high` | 版本低于保留下限 |
| `gpt-5.2-codex-low` | 版本低于保留下限 |
| `gpt-5.2-codex-medium` | 版本低于保留下限 |
| `gpt-5.2-codex-xhigh` | 版本低于保留下限 |
| `gpt-5.2-high` | 版本低于保留下限 |
| `gpt-5.2-low` | 版本低于保留下限 |
| `gpt-5.2-medium` | 版本低于保留下限 |
| `gpt-5.2-xhigh` | 版本低于保留下限 |
| `gpt-5.3-codex` | 版本低于保留下限 |
| `gpt-5.3-codex-high` | 版本低于保留下限 |
| `gpt-5.3-codex-low` | 版本低于保留下限 |
| `gpt-5.3-codex-medium` | 版本低于保留下限 |
| `gpt-5.3-codex-spark` | 版本低于保留下限 |
| `gpt-5.3-codex-xhigh` | 版本低于保留下限 |
| `gpt-5.5-high` | `gpt-5.5` |
| `gpt-5.5-low` | `gpt-5.5` |
| `gpt-5.5-medium` | `gpt-5.5` |
| `gpt-5.5-minimal` | `gpt-5.5` |
| `gpt-5.5-xhigh` | `gpt-5.5` |
| `gpt-5.6` | 无对应主型号 |
| `gpt-5.6-high` | 无对应主型号 |
| `gpt-5.6-low` | 无对应主型号 |
| `gpt-5.6-medium` | 无对应主型号 |
| `gpt-5.6-minimal` | 无对应主型号 |
| `gpt-5.6-xhigh` | 无对应主型号 |

MiniMax M 系列从 M2.7 起保留。移除的随包报价包括 `minimax-m2`、`minimax-m2.1`、`minimax-m2.1-lightning`、`minimax-m2.5`、`minimax-m2.5-lightning`。
