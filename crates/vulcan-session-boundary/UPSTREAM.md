# Codex 会话层参考记录

## 来源

- 仓库：`D:\projects\codex`
- 远端：`https://github.com/openai/codex.git`
- 分支：`main`
- 提取提交：`e093d819826127be01354e2885c86d7fcbfb2897`
- 提取日期：`2026-06-07`
- 快照清理日期：`2026-06-08`

## 已参考范围

- `codex-rs/core/src/session`
- `codex-rs/core/src/agent`
- `codex-rs/core/src/agent/control`
- `codex-rs/core/src/tools/handlers/multi_agents`
- `codex-rs/core/src/tools/handlers/multi_agents_v2`
- `codex-rs/core/src/state`
- `codex-rs/core/src/tasks`
- `codex-rs/core/src/event_mapping.rs`
- `codex-rs/core/src/codex_thread.rs`
- `codex-rs/core/src/thread_manager.rs`
- `codex-rs/core/src/thread_rollout_truncation.rs`
- `codex-rs/core/src/turn_metadata.rs`
- `codex-rs/core/src/turn_timing.rs`
- `codex-rs/rollout/src`
- `codex-rs/rollout-trace/src`
- `codex-rs/app-server-protocol/src/protocol`
- `codex-rs/app-server/src/request_processors`
- `codex-rs/thread-store/src`
- `codex-rs/thread-store/src/local`

## 当前策略

本 crate 不再保存 Codex 原始源码快照，也不再暴露 `UPSTREAM_SOURCE_GROUPS` 这类轻量索引。原始快照已经确认未参与编译，也没有被生产链路调用；继续保留会造成“已经接入 Codex 控制流”的误判。

后续迁移只吸纳 Codex 的控制语义：`InputQueue`、`ActiveTurn`、`SessionTask`、`run_turn` 循环、`try_start_turn_if_idle`、子代理 spawn/wait/mailbox/abort 语义。Vulcan 不照搬 Codex 的 app-server、app-server-protocol、thread-store、rollout 单文件存储、rollout-trace、搜索、账号鉴权和 provider 调用层。

模型实际调用层、provider transport、LuaSkills、MCP、Native tools、VMM hooks、WebUI 和分段持久化继续使用 Vulcan 现有实现，并逐步接入新的 Vulcan 原生会话控制核心。
