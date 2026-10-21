# 代码模式客户端来源与修改说明

本 crate 是服务端驱动 `vulcan-code-mode-host` 的代码模式客户端（gRPC 会话与进程宿主管理），搬运自 OpenAI Codex：

- 上游仓库：`openai/codex`（https://github.com/openai/codex）
- 固定提交：`c419fd01a70dd1d05356da443ac44328bda2e6f4`
- 上游路径：`codex-rs/code-mode/`（除 `BUILD.bazel` 外全部文件）
- 上游许可证：Apache-2.0，完整文本见同目录 `LICENSE-APACHE`
- 本 crate 许可证：`MIT AND Apache-2.0`；搬运部分继续受 Apache-2.0 约束

## 改动标记

- 每个与上游不同的源文件，文件头均有 `// Vendored from openai/codex (...)` 与
  `// Modified for Vulcan Code` 两行说明；与上游完全一致的文件不加文件头。
- 具体改动位置以 `VULCAN MODIFICATION:` 注释标出。
- 全部文件统一做了以下机械改名，不再逐处标记：`codex_code_mode*` → `vulcan_code_mode*`，
  `codex-code-mode-host` → `vulcan-code-mode-host`；改名后导入顺序由 rustfmt 重新排列。线路协议名 `codex.code_mode.v1` 与 gRPC 元数据键保持不变。

## 本地改动

- `src/grpc_session/transport.rs`：URL 端点不再经过 `codex_http_client`（代理、自定义 CA 与按账号网络策略），改用 tonic 自带连接；URL 校验规则与上游一致，响应头之前的连接失败仍按上游归类为 `Unavailable`，以便会话准入重试。
- `Cargo.toml`：不依赖 `workspace-hack`，因为本 crate 同时由独立的宿主工作区 `native/code-mode-host` 以静态 C 运行时构建。
- `src/grpc_session/mod.rs`：删除 `with_http_client_factory` 构造函数。
- `src/remote_session.rs`：删除依赖 `codex_install_context` 的 `Default` 实现；`ProcessOwnedCodeModeSession::new` 改为显式接收宿主路径。
- `src/remote_session/connection.rs`：删除对 Codex 启动密钥环境变量的清理（Vulcan 没有这类变量，V8 沙盒内脚本也读不到环境变量）。
- 链路上下文改用 `vulcan_code_mode_protocol::trace_context` 的空实现；`ToolName` 改用协议 crate 中的副本。

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
