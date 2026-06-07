# Codex 会话层提取记录

## 来源

- 仓库：`D:\projects\codex`
- 远端：`https://github.com/openai/codex.git`
- 分支：`main`
- 提取提交：`b89ce9a2bcedcfddf3a48f387b7912d602d6d87c`
- 提取日期：`2026-06-07`

## 当前提取范围

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

本 crate 当前只保存 Codex 最新会话执行、rollout、thread-store、多代理控制、app-server 协议、thread/turn 请求处理、事件映射、trace reducer 与历史展示构建的原始文件快照，并提供轻量索引。原始文件位于 `upstream/`，暂不作为 Rust module 编译。

后续迁移时应逐步把这些文件拆成 Vulcan 可编译模块，并把 LuaSkills、VMM、MCP、WebUI、分段存储等 Vulcan 扩展作为 adapter 接入，而不是改写 Codex 核心调度契约。

模型实际调用层、provider transport 与账号鉴权层暂不纳入本次提取；这些部分继续使用 Vulcan 现有实现，通过 adapter 接到 Codex 会话契约。
