# 代码模式宿主进程来源与修改说明

本 crate 是独立的代码模式宿主进程 `vulcan-code-mode-host`（stdio 或 TCP 上的 gRPC 服务），搬运自 OpenAI Codex：

- 上游仓库：`openai/codex`（https://github.com/openai/codex）
- 固定提交：`c419fd01a70dd1d05356da443ac44328bda2e6f4`
- 上游路径：`codex-rs/code-mode-host/`（除 `BUILD.bazel` 外全部文件）
- 上游许可证：Apache-2.0，完整文本见同目录 `LICENSE-APACHE`
- 本 crate 许可证：`MIT AND Apache-2.0`；搬运部分继续受 Apache-2.0 约束

## 改动标记

- 每个与上游不同的源文件，文件头均有 `// Vendored from openai/codex (...)` 与
  `// Modified for Vulcan Code` 两行说明；与上游完全一致的文件不加文件头。
- 具体改动位置以 `VULCAN MODIFICATION:` 注释标出。
- 全部文件统一做了以下机械改名，不再逐处标记：`codex_code_mode*` → `vulcan_code_mode*`，
  `codex-code-mode-host` → `vulcan-code-mode-host`；改名后导入顺序由 rustfmt 重新排列。线路协议名 `codex.code_mode.v1` 与 gRPC 元数据键保持不变。

## 本地改动

- `src/main.rs`：删除 `--otel-trace-listen`、`--otel-trace-exporter` 与 `codex_otel` 追踪层，只保留 `--listen` 与 stderr 日志。
- `src/grpc/mod.rs`、`src/grpc/routing.rs`：链路上下文改用 `vulcan_code_mode_protocol::trace_context` 的空实现。
- 新增 `tests/support/cargo_bin.rs`，替代 `codex_utils_cargo_bin`：宿主路径取自 `CARGO_BIN_EXE_vulcan-code-mode-host`；`write_executable` 从 `codex-rs/utils/cargo-bin/src/executable.rs` 原样复制。
- 删除 `tests/grpc/network_policy_tests.rs`：它验证的是未搬运的 `codex_http_client` 代理策略。
- 位于独立工作区 `native/code-mode-host`（静态 C 运行时，原因见 `vulcan-code-mode-runtime/NOTICE.md`），不依赖 `workspace-hack`。

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
