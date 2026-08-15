# LLM 核心第三方来源说明

## 基线

| 项目 | 本地仓库 | 固定提交 | 许可证 | 采用范围 |
| --- | --- | --- | --- | --- |
| OpenAI Codex | `D:\openvulcan\third_git\codex` | `5c44f110649f8811546745bb1635ba0b44a1639e` | Apache-2.0（`LICENSE`、`NOTICE`） | Responses HTTP/SSE/WS、错误、重试、压缩与回合控制的行为和实现参考 |
| cc-switch | `D:\openvulcan\third_git\cc-switch` | `0345fad6048eed65b3423bedc8ce5711320ddfc3` | MIT（`LICENSE`） | OpenAI Chat、Anthropic、供应商 reasoning、流式工具调用与默认配置的行为和 fixture 参考 |
| CLIProxyAPI | `D:\openvulcan\third_git\CLIProxyAPI` | `6e92e3e60e64e99b6d41a08388340e7435493350` | MIT（`LICENSE`） | Gemini、Google Code Assist、Responses 兼容和图片生成 fixture 参考 |

以上固定提交均为本地仓库当前完整提交哈希；许可证文件已经按表中路径核验。发布审计仍需保留对应许可证与 NOTICE 内容，不能只保留本清单。

## Vulcan 目标模块映射

| Vulcan 目标 | 上游来源 | 采用类别 | 作用 |
| --- | --- | --- | --- |
| `crates/vulcan-llm-core/src/responses_to_chat.rs` | cc-switch `src-tauri/src/proxy/providers/transform_codex_chat.rs` | 实质改写与行为参考 | Responses 请求转换为 OpenAI Chat-compatible 请求 |
| `crates/vulcan-llm-core/src/responses_to_anthropic.rs` | cc-switch `src-tauri/src/proxy/providers/transform_codex_anthropic.rs`、`streaming_codex_anthropic.rs` | 实质改写与行为参考 | Responses 与 Anthropic Messages 语义桥接 |
| `crates/vulcan-llm-core/src/responses_to_gemini.rs` | CLIProxyAPI `sdk/translator/`、`internal/translator/`；Vulcan 现有 Gemini profile | 行为与 fixture 参考 | Responses 转 Gemini generateContent/Code Assist |
| `crates/vulcan-llm-core/src/reasoning_bridge.rs` | cc-switch `src-tauri/src/proxy/providers/transform_codex_chat.rs`、`transform_codex_anthropic.rs` | 行为参考 | reasoning item、签名与供应商字段桥接 |
| `crates/vulcan-llm-core/src/responses_websocket.rs` | Codex `codex-rs/codex-api/src/endpoint/responses_websocket.rs`、`responses.rs`、`sse/responses.rs` | 搬运后适配 | 标准 Responses HTTP/SSE/WS 通讯与事件状态机 |
| `crates/vulcan-llm-core/src/usage.rs` | cc-switch `src-tauri/src/services/provider/usage.rs`；Vulcan `vulcan-ai-contract` | 行为参考与 Vulcan 原生实现 | 原始 usage 提取、来源路径与流式合并；不承担价格计算 |
| `plugins/vulcan-usage-statistics/pricing/` | cc-switch `src-tauri/src/database/schema.rs` 的 `Database::seed_model_pricing` | 固定发行版机械提取并插件化适配 | 仅为统计插件提供参考价目表；主 crate 不包含该目录且内核不计算或存储价格 |
| `crates/vulcan-service-kernel/src/context_compaction/` | Codex `codex-rs/codex-api/src/endpoint/compact.rs` 与 core compaction 回合控制 | 行为参考与 Vulcan 原生实现 | 回合中安全检查点、远端压缩、本地重写和继续执行 |

## 同步规则

1. 不运行时加载第三方应用配置或数据库；只把核验后的事实转成版本化 Vulcan Profile。
2. 直接搬运或实质改写的文件必须在文件头或相邻来源清单中保留项目、源路径、固定提交、许可证和 Vulcan 修改说明。
3. 上游更新只能通过 fixture 差异和人工审核进入，不进行无边界代码覆盖。
4. Codex 协议行为以官方 OpenAI Responses 文档和固定源码共同验证；冲突时不凭记忆选择。
