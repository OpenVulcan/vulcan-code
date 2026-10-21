# 代码模式协议来源与修改说明

本 crate 是代码模式（Code Mode）会话契约、工具描述与 gRPC 协议，搬运自 OpenAI Codex：

- 上游仓库：`openai/codex`（https://github.com/openai/codex）
- 固定提交：`c419fd01a70dd1d05356da443ac44328bda2e6f4`
- 上游路径：`codex-rs/code-mode-protocol/`（除 `BUILD.bazel` 外全部文件）
- 上游许可证：Apache-2.0，完整文本见同目录 `LICENSE-APACHE`
- 本 crate 许可证：`MIT AND Apache-2.0`；搬运部分继续受 Apache-2.0 约束

## 改动标记

- 每个与上游不同的源文件，文件头均有 `// Vendored from openai/codex (...)` 与
  `// Modified for Vulcan Code` 两行说明；与上游完全一致的文件不加文件头。
- 具体改动位置以 `VULCAN MODIFICATION:` 注释标出。
- 全部文件统一做了以下机械改名，不再逐处标记：`codex_code_mode*` → `vulcan_code_mode*`，
  `codex-code-mode-host` → `vulcan-code-mode-host`；改名后导入顺序由 rustfmt 重新排列。线路协议名 `codex.code_mode.v1` 与 gRPC 元数据键保持不变。

## 本地改动

- 新增 `src/upstream_types.rs`：从 `codex-rs/protocol` 原样复制 `ToolName`、`DEFAULT_FUNCTION_NAMESPACE`、`CodeModeToolMessages`、`ToolMessage` 与 `MAX_PROMPT_AUDIO_INPUT_BYTES`，去掉 `ts_rs` 与 `schemars` 派生，以免依赖 Codex 协议 crate。
- 新增 `src/trace_context.rs`：替代 `codex_otel` 的 W3C 链路上下文函数，全部返回空值；`traceparent` 元数据与 proto 字段仍保留在线路协议中。
- `src/grpc/mod.rs` 与 `build.rs`：删除 Bazel 专用的 `cfg(codex_bazel)` 分支，只走 Cargo 生成代码。
- 其余文件只把 `codex_protocol` 导入改为本 crate 的同名类型。
- `Cargo.toml`：不依赖 `workspace-hack`，因为本 crate 同时由独立的宿主工作区 `native/code-mode-host` 以静态 C 运行时构建。

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
