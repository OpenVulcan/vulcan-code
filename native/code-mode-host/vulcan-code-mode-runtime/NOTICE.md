# 代码模式 V8 运行时来源与修改说明

本 crate 是代码模式的 V8 JavaScript 运行时，搬运自 OpenAI Codex，只由 `vulcan-code-mode-host` 链接：

- 上游仓库：`openai/codex`（https://github.com/openai/codex）
- 固定提交：`c419fd01a70dd1d05356da443ac44328bda2e6f4`
- 上游路径：`codex-rs/code-mode-runtime/`（除 `BUILD.bazel` 外全部文件）
- 上游许可证：Apache-2.0，完整文本见同目录 `LICENSE-APACHE`
- 本 crate 许可证：`MIT AND Apache-2.0`；搬运部分继续受 Apache-2.0 约束

## 改动标记

- 每个与上游不同的源文件，文件头均有 `// Vendored from openai/codex (...)` 与
  `// Modified for Vulcan Code` 两行说明；与上游完全一致的文件不加文件头。
- 具体改动位置以 `VULCAN MODIFICATION:` 注释标出。
- 全部文件统一做了以下机械改名，不再逐处标记：`codex_code_mode*` → `vulcan_code_mode*`，
  `codex-code-mode-host` → `vulcan-code-mode-host`；改名后导入顺序由 rustfmt 重新排列。线路协议名 `codex.code_mode.v1` 与 gRPC 元数据键保持不变。

## 本地改动

- `src/session_runtime/mod.rs`：删除向嵌套工具回调传递 OpenTelemetry 上下文的字段与 `with_context` 调用（Vulcan 不导出链路追踪）。
- `src/runtime/audio.rs`：音频大小上限改从 `vulcan-code-mode-protocol` 引入。
- 其余文件只把 `codex_protocol::ToolName` 改为 `vulcan_code_mode_protocol::ToolName`。
- V8 版本与 `v8_enable_sandbox` 特性保持上游的 `=152.2.0`。
- 位于独立工作区 `native/code-mode-host`：沙盒 V8 预编译库内嵌分配器垫片，必须与上游一样以静态 C 运行时（`+crt-static`）链接，因此不加入根工作区、不依赖 `workspace-hack`；该工作区的 `clippy.toml` 复制自上游 `codex-rs/clippy.toml` 的相关键。

## 上游 NOTICE 原文

```text
OpenAI Codex
Copyright 2025 OpenAI

This project includes code derived from [Ratatui](https://github.com/ratatui/ratatui), licensed under the MIT license.
Copyright (c) 2016-2022 Florian Dehau
Copyright (c) 2023-2025 The Ratatui Developers
```

## 同步上游

先用固定提交与新提交对比 `codex-rs/<crate>/`，再把差异按文件合入；合入后保留所有
`VULCAN MODIFICATION:` 改动，更新本文件的固定提交，并运行四个 code-mode crate 的全部测试。
不得整目录覆盖。
