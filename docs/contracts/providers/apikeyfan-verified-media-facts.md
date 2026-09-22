# APIKEY.FAN 媒体能力已验证事实

> 文档性质：供应商事实记录，不是修改实施计划。
>
> 本文是 `manifests/apikeyfan.json` 中图片与视频相关声明、门控与证据引用的**唯一权威来源**。
> 内容抽取自会话记录 `docs/plan/20260813-04-APIKEYFUN_VERIFIED_PROVIDER_FACTS.md`（该文件未随仓库分发）
> 中仍然生效的核心事实，并补充供应商自带 skill 包提供的证据。
>
> 证据分层，不得互相替代：
>
> - **用户确认规则**：用户明确的供应商规则。
> - **中转实测**：对 `api.apikey.fan` 的真实调用结果。
> - **官方资料**：模型厂商官方文档，只约束模型自身能力，不约束中转入口。
> - **供应商 skill 包**：供应商自己发布的 `image-gen` skill（Node 与 Python 两版脚本 + API 配方）。
> - **本仓库决策**：由上述证据推出的产品边界。
>
> 未列入本文的能力一律视为未知：**不得据分组名称、相邻模型或相似接口推测**。

<a id="key-group"></a>
## 一、Key 分组与模型发现（用户确认规则）

1. APIKEY.FAN 是中转供应商，覆盖 Claude、ChatGPT/Codex、Grok、Gemini、智谱、Kimi、DeepSeek、MiniMax 等分组。
2. 单个 Key 只属于一个分组；同一供应商下不同 Key 可发现完全不同的模型集合。
3. 实际启用集合为：`GET {base}/v1/models` 返回的模型 ID ∩ 本地预设支持名单。
4. 模型身份只以 `data[].id` 为权威；其余字段（`object`、`created`、`owned_by`、`type`、`display_name`）只作可选元数据，不得作为启用条件。
5. 目录返回但不在本地名单内的模型必须忽略：不展示、不启用、不推测参数。

<a id="image-gating"></a>
## 二、图片能力门控（用户确认规则）

- 图片生成与图片编辑是**两项独立**的 AI 扩展能力，必须分别声明、授权与调用。
- 两项扩展是否暴露，由**当前 Key 经白名单过滤后的有效模型集合**决定：当其中包含 `gpt-5.6` 模型时，认定该 Key 支持基于 `gpt-image-2` 的图片生成与图片编辑。
- `gpt-image-2` 不要求出现在 `/v1/models` 目录中。
- `gpt-image-2` **不得作为普通对话模型加入模型选择器**。
- 图片生成扩展不得仅因请求携带参考图，就在运行时改道 `/v1/images/edits`。

> 直接推论：门控必须表达为「Key 的有效模型集合中存在 `gpt-5.6` 系模型」，
> 而不能表达为「目录中存在图片模型」。若把图片模型写进某个入口的固定目录、再以图片模型自身 ID 作门控，
> 就把「该 Key 的分组是否支持生图」这一真实条件退化成恒真，等于对不支持的分组也暴露工具。

<a id="generation-contract"></a>
## 三、GPT 图片生成接口（中转实测）

- `POST {base}/v1/images/generations`，`Content-Type: application/json`，`model: "gpt-image-2"`。
- 已实测参数：`prompt`、`n`、`size`、`quality`、`stream`、`response_format`；当前验证使用 `stream: true` 与 `response_format: "b64_json"`。
- 实测尺寸行为：

| 请求尺寸 | 质量 | 实际 PNG 尺寸 | 实际像素数 | 结果 |
| --- | --- | --- | ---: | --- |
| `1024x1024` | 未单独指定高质量 | `1254x1254` | 1,572,516 | 成功 |
| `2048x1152` | `high` | `1672x941` | 1,573,352 | 成功，多次一致 |
| `3840x2160` | `high` | `1672x941` | 1,573,352 | 成功，未输出真实 4K |
| `256x512` | `high` | `887x1774` | 1,573,538 | 成功，小尺寸被自动放大 |

- 结论：`size` 主要决定目标宽高比，不承诺按请求像素原尺寸输出；横屏、方图、竖屏都稳定在约 157 万像素预算；`quality: "high"` 被接受但不突破该预算；最终宽高必须以解码后的实际文件为准。
- **带参考图的生成尚未验证**：其在 apikey.fun 上的实际入口、图片字段与请求格式未知，**禁止由图片编辑接口反向推导**。
- 供应商 skill 包补充：其默认图片模型 ID 为 `gpt-image-2.5-sunburst`，并允许通过 `--model` 传入网关上其他图片模型（如 `grok-imagine-image-quality`、`grok-imagine-image`）；`gpt-image-2.5-flare` 由用户提供，尚无第三方佐证。

<a id="edit-boundary"></a>
## 四、图片编辑：为何本仓库不暴露（证据 + 决策）

- 产品语义（用户确认）：图片编辑是「原图附加 `mask` 后对指定区域修改」，而不是「任何携带参考图的生成请求」。
- 中转接口形态（实测）：`POST {base}/v1/images/edits`，`multipart/form-data`，原图字段 `image`，遮罩字段 `mask`。
- **遮罩规则、尺寸约束与透明通道要求从未用真实带 `mask` 请求验证。**
- 已做过一次**不带 `mask`** 的连通性实测（把 `128x128` PNG 整体改为蓝色）：HTTP 200、`text/event-stream`，输出 `1254x1254` PNG。该结果只证明接口允许无遮罩的整图变换，**不改变上述产品边界**。
- 供应商 skill 包（本次补充证据）：全目录检索 `mask` **零命中**；其「图生图」与「文+图生图」都只是重复提交 `image` 字段，没有遮罩参数。
- **本仓库决策**：既然遮罩语义在中转侧既无文档化契约也无验证，本清单**只暴露图片生成，不暴露图片编辑**；`gpt-image-2.edit` target、对应 operation、mask 上传通道与已无人使用的 `http_multipart_sse` 依赖均已移除。

<a id="sse-contract"></a>
## 五、SSE 与取图契约

- 已实测的取图位置：`event.b64_json`、`event.data[0].b64_json`、`event.item.result`。
- 已文档化的事件类型：`image_generation.partial_image`、`image_generation.completed`。
- 多次生成实测均只收到 `image_generation.completed`，**从未观察到局部图片事件**；保留局部事件解析是为了遵守接口契约，不得写成「必然返回局部图」。
- 供应商 skill 包（本次补充证据）：其客户端除上述位置外，还跳过 `response.image_generation_call.partial_image`，并把 `b64_json`／`base64`／`image_base64` 递归进 `data[]`、`images[]`、`output[]`、`response.output[]`、`item` 取图。这些位置已按路径展开为 decoder。

<a id="grok-media"></a>
## 六、Grok 媒体（中转实测 + 用户确认）

**图片生成**

- `POST {base}/v1/images/generations`，`Content-Type: application/json`；响应是**一次性 JSON，不是 SSE**，图片位于 `data[0].b64_json`，同时返回 `mime_type`。
- 使用 Grok 原生参数 `aspect_ratio`、`resolution`，**不使用** GPT 路径的 `size`、`quality`、`stream`。
- `grok-imagine-image-quality` 是**独立模型 ID**，原始计费刻度约为普通模型的 2.5 倍（实测 500,000,000 与 200,000,000 ticks），不得建模为普通模型上的 `quality` 参数。
- 实测两模型均返回 `image/jpeg`、`1280x720`。

**视频生成**

- 启动：`POST {base}/v1/videos/generations`，返回 `request_id`。
- 轮询：`GET {base}/v1/videos/{request_id}`，直到 `status` 为 `done`／`failed`。
- 取内容：完成后 `GET {base}/v1/videos/{request_id}/content` 返回二进制。
- 线上字段名 `resolution`（`480p`／`720p`）与 `duration`（`1`–`15` 秒整数），**默认时长 8 秒**。
- 单图生视频：线上结构 `image: {"url": "..."}`，**不发** `aspect_ratio`，输出画幅由输入图决定（实测 `1280x720` MP4、约 2.04 秒、H.264 + AAC）。
- 多参考生视频：`reference_images` ≤ 7、`reference_audios` ≤ 3，二者至少一类，且必填 `prompt` 与 `aspect_ratio`。
- `grok-imagine-video-1.5` 是 Grok Build 的固定视频模型名；apikey.fan 目录当前返回 `grok-imagine-video`，**两者不得提前视为同一模型**。

<a id="unknown"></a>
## 七、必须保持未知的内容

- 带 `mask` 的局部编辑、文件限制、多图行为与错误格式。
- 带参考图的 GPT 生图入口、图片字段与请求格式。
- `grok-imagine-edit`：实测两次均 HTTP 404（`not_found_error`），仅出现在目录中，后台无可调用模型——不得再作为候选。
- `grok-imagine-video` 的多参考图、参考音频、15 秒边界与 `480p` 输出的真实调用结果，以及失败、取消、超时与内容下载的错误格式。
- `grok-imagine-image` 与 `grok-imagine-image-quality` 除单样本、原始计费刻度之外的稳定能力差异。
- `gpt-image-2.5-sunburst` 与 `gpt-image-2.5-flare` 的官方能力与尺寸行为，除「供应商 skill 包将其作为默认图片模型」之外。

在取得真实调用样例或完成实际请求验证之前，以上内容必须保持未知。
