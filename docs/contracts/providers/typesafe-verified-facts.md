# TypeSafe 模型与多模态接入核验

## 归属与同步触发点

本文记录 2026-10-26 核对的官方事实，以及当前宿主的实现边界，不替代上游协议。
模型目录、图片规格或问题类型更新时，必须重新读取下列官方资料，并核对当前源码与测试：

- [模型与上下文](https://docs.typesafe.ai/models)
- [图片输入](https://docs.typesafe.ai/images)
- [边界框问题](https://docs.typesafe.ai/primitives/box)
- [机器可读接口定义](https://api.typesafe.ai/openapi.json)
- [请求限制与错误](https://docs.typesafe.ai/api/errors)

执行声明的权威仍为
`crates/vulcan-runtime-assets/resources/provider-manifests/manifests/typesafe.json`、
Provider Manifest Schema、生产编译器及相应执行器。

## 已确认的模型目录

| 模型标识 | 当前状态 | 官方图片能力 | 当前宿主执行能力 |
| --- | --- | --- | --- |
| `jev-1.14.0` | 最新稳定版，2026-10-14 发布 | 显式启用后的实验图片输入；支持 `box` | 文本 Choice 决策 |
| `jev-1.13.0` | 仍可用 | 显式启用后的实验图片输入；不支持 `box` | 文本 Choice 决策 |

两者均使用 Bearer 鉴权和 `POST https://api.typesafe.ai/v1/systemone`。
`jev-latest` 与 `jev-preview` 当前均指向 1.14；别名会移动，不能用于悄悄替换已选择的判定模型。
内置目录列出精确版本，保留 1.13，1.14 由用户在原有模型选择入口显式选择。
官方说明 `GET /v1/models` 当前主要返回别名，即使未列出精确版本，接口仍接受精确版本标识。
因此本供应商继续使用静态目录，不套用中转站的发现交集规则。

官方默认限制区分两个维度：整次请求为 64k，`state` 加任意一道问题为 32k。
当前单值 `context_limit` 无法表达这组双限制，本次保留既有的保守预算 `32768`，不将 64k
误当作每道问题的容量。显式发送 `TypeSafe-Danger-Zone-Long-Context: true` 可将两个限制均提升至
262k；本次不启用此实验选项，也不将其当作默认容量。

## 官方图片协议

图片是决策输入，不是图片生成，也不使该模型成为普通聊天模型。官方没有声明原生音频或视频输入。

1. 图片请求显式携带 `TypeSafe-Danger-Zone-Images: true`。
2. 请求顶层的 `assets` 是引用字符串到图片的映射；引用可位于 `state`、问题的
   `instructions` 或 `criteria` 中，包括嵌套结构和字符串内部。对象的键不会替换。
3. JSON 图片项使用 `type: image`、`format: png/jpg/jpeg` 和纯 Base64 的 `data_base64`；
   不使用带 `data:` 前缀的地址。
4. multipart 图片项使用 `part` 关联图片文件段。首段必须名为 `request`，内容为 JSON；
   每个文件段只归属一个图片项。multipart 是官方推荐的传输方式。
5. 每个图片项都必须被正文引用。引用可能在字符串内部匹配，必须生成不会与正文偶然碰撞的标识；
   同一引用多次出现时，每次展开的图片都会计入限制，不能仅按去重后的文件大小计算。

官方尺寸与字节约束：静态 PNG/JPEG，拒绝动画；每张最多 16 MiB；宽高各为 280 至 1568 像素；
请求正文最多 32 MiB；处理后的图片总量以及替换引用后的展开请求也分别受 32 MiB 限制。
单个引用最多 1024 个 UTF-8 字节，全部引用合计最多 65536 个字节。
服务端拒绝超限图片，不会自动缩放；宿主不能把超限失败悄悄改成文本判定。

`box` 返回 `top`、`bottom`、`left`、`right`，范围为 0 至 1，且左右、上下顺序必须成立。
官网示例给出数字置信度，但 OpenAPI 明确允许 `confidence: null`；实现时必须保留缺失置信度语义，
不能补零或沿用 Choice 的必填数值验证。

## 当前宿主缺口与接入方案

已沿实际调用链确认：

- `crates/vulcan-ai-contract/src/decision.rs`：`DecisionRequest` 没有图片资源；问题和响应均只定义 Choice。
- `crates/vulcan-llm-core/src/protocol_implementation/systemone.rs`：序列化器只写
  `model`、`state`、`questions`，解码器也只接受 Choice。
- `crates/vulcan-provider-adapters/src/native_inference.rs`：决策使用独立的
  `execute_decision`，不会经过普通对话附件与上传链。
- `crates/vulcan-service-kernel/src/permission_judge/cloud.rs`：当前权限判定只将工具与参数文本组成
  `state`，没有图片来源。

因此不能仅把 Manifest 的 `input_resources` 改成 `image`，也不能将图片字节塞入 `state` 假装已接通。
本次目录只声明当前可执行的文本决策。

推荐的后续实施边界：

1. 扩展供应商无关的决策请求，加入强类型图片来源与引用；复用现有资源所有权、原始字节和摘要校验。
   不接受未经审核的任意文件读取，也不把会话历史或项目附件自动交给权限判定器。
2. 在 System One 编码与执行链生成顶层 `assets`，通过声明过的路由能力发送实验请求头；
   只在图片请求启用，文本请求保持现有形状。明确选择 JSON Base64 或 multipart，并验证对应传输边界。
3. 为图片解码、尺寸、字节上限、引用是否存在与重复展开设置唯一常量及确定性校验。
   类型不支持、图片无效、越界或服务失败时显式报错，不降级为纯文本。
4. 图片 Choice 决策可先独立接入；`box` 则需同时增加问题、响应、归一化坐标及可空置信度契约。
   1.13 必须在派发前拒绝 Box，不能靠服务端尝试判断。
5. 使用本地严格 HTTP fixture 验证真实编码器的图片内容、请求头、引用位置、格式、边界及拒绝路径。
   再以用户授权的 TypeSafe 凭据完成图片 Choice 和 1.14 Box 的真实验收。
6. 完成请求、编码、解码、消费者和测试闭环后，才在相应模型清单中声明 `image`。
   权限审批阈值和流程不随多模态接入改变。

新增资源传入和响应类型涉及共享推理契约；如继续实施，应先确认图片判定的调用入口和是否包含 Box，
再按仓库重大决策约定推进，不将实验能力自动扩散至所有判定消费者。

## 本次验证边界

1.13 的真实文本调用证据沿用清单记录的 2026-10-10 测试。
1.14 的身份、接口及图片能力来自本次核对的官方文档和 OpenAPI；本次未进行付费线上推理，
没有 1.14 文本、图片或 Box 的真实调用验收，也未宣称这些调用已通过。
