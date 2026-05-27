# vulcan-token-counter

`vulcan-token-counter` 是一个独立的上下文预算 crate。它现在采用“精确 `.tkz` 优先 + OpenAI 代理兜底”策略：OpenAI 模型使用 OpenAI BPE 精确统计，已内置 `.tkz` 的主流模型使用对应 tokenizer 精确统计，缺少资源的模型再使用 OpenAI BPE 代理并挂安全倍率。生产环境应由宿主把统一资源目录传给 `TokenCounter::from_assets_dir` 或 `TokenCounter::from_resources_dir`，`TokenCounter::new()` 默认使用 workspace 内 `vulcan-runtime-assets` 的 tokenizer 资源目录。

## 设计目标

- 构造阶段只做资源路径校验，不解析大词表，避免系统初始化卡顿。
- 默认 OpenAI 运行阶段使用 `wordchipper` + `.tiktoken`，同时提供实验性的 OpenAI `tokie` + `.tkz` 后端用于加载速度对比。
- Qwen、DeepSeek、MiniMax、GLM、Hunyuan、ERNIE、Llama、Mistral、Phi、Gemma、Nemotron 等模型族优先使用本地 `.tkz` 精确资源。
- 不再引入 HF `tokenizers`、`fastokens`、`tiktoken-rs` 或 SentencePiece 链路。
- 非 OpenAI 模型不追求首次本地 100% 精确，而是给上下文裁切提供稳定、快速、偏保守的预算值。
- 后续真实 API 调用返回的 usage tokens 仍应作为绝对准确来源。

## 路由策略

- OpenAI 新模型：`gpt-5`、`gpt-4.5`、`gpt-4.1`、`gpt-4o`、`o1/o3/o4`、`codex` 使用 `o200k_base` 精确统计。
- OpenAI 旧模型：`gpt-4-turbo`、`gpt-4`、`gpt-3.5` 使用 `cl100k_base` 精确统计。
- 精确东方组：`qwen`、`qwen coder`、`deepseek`、`deepseek r1`、`minimax`、`glm`、`hunyuan/hy3`、`ernie/baidu` 优先使用 `.tkz`；缺资源时回退 `o200k_base * 1.08`。
- 精确西方组：`llama`、`mistral`、`phi`、`nvidia/nemotron` 优先使用 `.tkz`；缺资源时回退 `o200k_base * 1.05`。
- 仍走代理的东方组：`kimi`、`mimo`、`seed`、`doubao` 使用 `o200k_base * 1.08`。
- 仍走代理的西方组：`grok` 等使用 `o200k_base * 1.05`。
- Claude：使用 `cl100k_base * 1.10`。
- Gemma：优先使用 Gemma `.tkz` 精确统计；缺资源时回退 `o200k_base * 1.08`。
- Gemini：使用 Gemma 4 `.tkz` 作为本地适配代理并乘以 `1.08`，精度标记为 `Proxy`，不会伪装成 Gemini 账单级精确。
- 未知模型：使用 `o200k_base * 1.15`。

## 资源目录

默认 `wordchipper` 后端需要两个 OpenAI `.tiktoken` 资源文件；实验性 `tokie` 后端需要对应 `.tkz` 文件：

```text
crates/vulcan-runtime-assets/resources/tokenizers/openai/
├── cl100k_base.tiktoken
├── cl100k_base.tkz
├── o200k_base.tiktoken
└── o200k_base.tkz

crates/vulcan-runtime-assets/resources/tokenizers/exact/
├── deepseek/tokenizer.tkz
├── deepseek_r1/tokenizer.tkz
├── ernie/tokenizer.tkz
├── gemma/tokenizer.tkz
├── glm/tokenizer.tkz
├── hunyuan/tokenizer.tkz
├── llama/tokenizer.tkz
├── minimax/tokenizer.tkz
├── mistral/tokenizer.tkz
├── nemotron/tokenizer.tkz
├── phi/tokenizer.tkz
├── qwen/tokenizer.tkz
└── qwen_coder/tokenizer.tkz
```

这些文件位于 crate 自己的资源目录下，不依赖其他 crate，也不会访问网络。`.tkz` 文件来自 `tokie` 生态的二进制 tokenizer 格式，仅用于本 crate 内部的可选后端实验。

注意：实测 OpenAI `.tkz` 后端加载速度明显更快，但 OpenAI 计数与默认 `wordchipper` 会有小幅差异，因此 `TokenizerBackend::Tokie` 返回的 OpenAI 原生路由精度会标记为 `Experimental`，不会伪装成账单级 `Exact`。非 OpenAI 模型族的 `.tkz` 资源来自官方 `tokiers` 或由对应 HF `tokenizer.json` 离线转换，运行时只读取 `.tkz`。如果某个仓库不提供标准 HF `tokenizer.json`，例如 `xai-org/grok-2` 当前只提供非标准 `tokenizer.tok.json`，以及 `moonshotai/Kimi-K2.6` 当前只公开 `tokenizer_config.json`，本 crate 不做强行适配，直接走通用代理。

## 使用方式

```rust
use vulcan_token_counter::TokenCounter;

let counter = TokenCounter::new()?;
let estimate = counter.estimate("doubao-seed-2.0-pro", "你好，世界")?;

println!("tokens = {}", estimate.tokens);
# Ok::<(), vulcan_token_counter::TokenCounterError>(())
```

生产宿主推荐显式传入项目统一资源目录：

```rust
use vulcan_token_counter::TokenCounter;

let counter = TokenCounter::from_assets_dir(app_tokenizer_resource_dir)?;
let estimate = counter.estimate("Qwen/Qwen3.6-35B-A3B", "你好，世界")?;

println!("tokens = {}", estimate.tokens);
# Ok::<(), vulcan_token_counter::TokenCounterError>(())
```

如果宿主希望在启动后显式预热，调用：

```rust
let counter = vulcan_token_counter::TokenCounter::new()?;
counter.warm_up()?;
# Ok::<(), vulcan_token_counter::TokenCounterError>(())
```

如果宿主希望保持最快启动，可以不调用 `warm_up()`，首次计数时只会加载实际路由需要的 OpenAI BPE。

如果需要对比 `.tkz` 后端：

```rust
use vulcan_token_counter::{TokenCounter, TokenizerBackend};

let counter = TokenCounter::new_with_backend(TokenizerBackend::Tokie)?;
let estimate = counter.estimate("gpt-5-codex", "hello world")?;

println!("tokens = {}", estimate.tokens);
# Ok::<(), vulcan_token_counter::TokenCounterError>(())
```

本 crate 提供同文件对比测量：

```text
cargo run -p vulcan-token-counter --release --example measure -- crates/vulcan-token-counter/src/lib.rs
```

如果需要刷新或切换资源来源，使用脚本的强制重建模式：

```text
python scripts/fetch_tokenizers.py --force
```

## 为什么不再保留 HF 精确 tokenizer

之前的实现能够加载很多 `tokenizer.json/bin`，但单个大型 tokenizer 的构建仍可能达到秒级，用户体感很差。对于上下文预算来说，首次本地估算只需要“足够稳”，后续 API usage 才是绝对准确值。因此当前实现主动放弃重型精确层，换取：

- 更小的依赖面。
- 更少的跨平台动态库风险。
- 更快的构造和模型切换体感。
- 更清晰的预算代理语义。

如果未来确实需要再提升非 OpenAI 代理精度，建议优先做编译期 codegen：把少量 `.tiktoken` rank 文件转换成 Rust 静态表，而不是重新引入运行时 JSON/HF tokenizer 加载链路。
