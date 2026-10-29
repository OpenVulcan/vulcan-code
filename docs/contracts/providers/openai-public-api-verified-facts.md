# OpenAI 官方 API 固定目录与图片参数

核对日期：2026-10-29。

本文记录本次接入证据和用户确认的配置范围。字段形状以 Manifest Schema、Rust 类型和编译器共同
校验结果为准。官方模型或图片接口更新、公共能力资源格式调整时，应重新核对本文、清单、公共表和
执行 fixture。

## 模型范围与来源

官方 `public-api` 仅准入以下无日期模型 ID；目录使用凭据发现结果与精确规则的交集。所有日期快照、
旧模型和其余发现模型均排除。Codex 订阅入口保留独立的发现策略、协议和模型行为声明。

| 系列 | 对话模型 |
| --- | --- |
| 5.6 | `gpt-5.6-sol`、`gpt-5.6-terra`、`gpt-5.6-luna` |
| 6 | `gpt-6-astra`、`gpt-6-sol`、`gpt-6-luna` |
| 6.1 | `gpt-6.1-sol`、`gpt-6.1-terra` |

上述模型按用户明确要求统一为标准 Responses、文件和图片输入、文本输出、1,050,000 上下文、128,000
最大输出，以及 `low / medium / high / xhigh / max` 思考档位。Sol 的官方资料确认窗口、输出和档位；
其余模型的统一配置来自用户明确指定，不据此声称已逐项验证所有模型的线上行为。

- [Sol 官方模型资料](https://developers.openai.com/api/docs/models/gpt-6.1-sol)
- [Terra 官方模型资料](https://developers.openai.com/api/docs/models/gpt-6.1-terra)
- [模型目录接口](https://developers.openai.com/api/reference/resources/models/methods/list)

本次认证目录查询返回 130 个条目，所有记录只有标识、对象类型、创建时间、归属和停用日期。官方目录
不提供窗口、输出上限或模态，不能直接把全部条目当成对话模型。单模型详情查询也未提供更多能力字段。

## 同步图片生成

媒体入口注册 `gpt-image-2`、`gpt-image-2.5-flare`、`gpt-image-2.5-sunburst` 三个模型，不注册日期快照。
三者只声明图片生成操作，不能进入文本对话选择。目标使用 `POST /images/generations`、Bearer API Key
和同步 JSON 执行；`stream` 固定为 `false`。

| 参数 | 注册范围 |
| --- | --- |
| `model` | 固定为当前目标自身模型 ID |
| `prompt` | 1～32,000 字符 |
| `n` | 1～10，默认 1，全部返回项都回收 |
| `quality` | Image 2：`auto / low / medium / high`；两个 2.5 模型额外支持 `xhigh / max` |
| `size` | `auto` 或 `WIDTHxHEIGHT`，约束见下文 |
| `background` | `auto / opaque / transparent` |
| `output_format` | `png / jpeg / webp`，默认 `png` |
| `output_compression` | 0～100，仅 JPEG/WebP 可用；PNG 必须省略 |
| `moderation` | `auto / low` |
| `safety_identifier` | 可选，最多 128 字符 |

透明背景要求 PNG 或 WebP；Image 2 的透明背景支持仍为预览功能。同步目标不开放流式局部图片参数，
也不开放已废弃的 `style`、`response_format`、`user`。

**自定义宽和高都必须是 16 的整数倍。** 同时满足：

1. 每条边不超过 3840 像素。
2. 宽高比位于 1:3～3:1，长边与短边之比不超过 3。
3. 总像素在 655,360～8,294,400 之间。
4. 超过 3,686,400 像素（2560×1440）属于实验范围。

这些限制已写入参数说明和媒体扩展档。JSON Schema 校验尺寸字符串格式；上游仍负责具体自定义尺寸的
几何约束，不会静默调整为另一尺寸。透明背景与编码、压缩与编码的互斥关系由输入 Schema 校验。

- [官方生成参数](https://developers.openai.com/api/reference/resources/images/methods/generate)
- [官方图片参数与尺寸约束](https://developers.openai.com/api/docs/guides/image-prompting)
- [官方图片生成指南](https://developers.openai.com/api/docs/guides/image-generation)

## 公共能力配置与执行边界

这十一条记录同步进入公共模型能力表。八个对话模型同步窗口、输出上限、输入输出模态、思考支持、
控制方式与档位；图片模型保留图片输出与禁用思考状态，不虚构文本会话的 Token 窗口。公共资源版本
提升为 2，运行时补齐可用字段，旧版本明确拒绝。无关模型记录与之前 TypeSafe 更新保持原有语义。

公共生成脚本同步读取输出与思考字段，并优先采用官方供应商声明，避免同名中转记录先覆盖官方模型
事实。API 的精确目录规则与图片工具参数仍由供应商清单持有；公共表只提供能力事实，不扩大准入名单。

图片响应按 `/data` 数组中的 `/b64_json` 解码全部记录，根据请求编码映射 PNG/JPEG/WebP MIME。任一项
缺失载体或解码失败时整次请求显式报错；数量、字节和输出类型边界继续生效。

本次验证使用本地 HTTP fixture 与离线目录投影。没有执行线上付费推理或图片生成，也没有重启、提交
或推送；资源同步与进程加载是不同步骤。

## 验证记录

| 审核范围 | 结果 |
| --- | --- |
| 清单与索引 | 23 个供应商通过 Schema、生产编译器及索引校验；仅 OpenAI 固定条目发生本次变化 |
| 目录与能力 | 对已抓取的 130 个模型执行生产解析和补全，得到 8 个对话模型与 3 个图片模型；公共表逐项一致 |
| 运行时权限 | 入口覆盖矩阵、默认绑定隔离和远端更新信任上限通过；配置 crate 的 93 项测试通过 |
| 图片契约 | 66 组生产参数 Schema 组合通过；三个新增本地请求 fixture 及全部 49 个 fixture 通过 |
| 工程与同步 | 适配器 856 项库测试通过；两个目标 crate 全目标检查、Clippy、变更文件格式及差异检查通过；源资源与同步产物字节一致 |

图片参数专项测试只用于本次验证，已删除临时测试源码，不将模型目录内容固化为长期测试快照。
新增的长期测试仅约束入口策略、字段格式、默认值优先级和完整资源数组解码。

本次增加了宿主解析字段；应用需重新构建并重启后加载这些逻辑。本记录不代表运行中的客户端已经
更新，也不代表线上推理、真实图片质量或透明通道已经验收。
