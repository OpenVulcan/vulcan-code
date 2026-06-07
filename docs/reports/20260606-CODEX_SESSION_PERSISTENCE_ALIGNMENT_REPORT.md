# Codex 会话持久化对齐调整报告

## 1. 结论摘要

本次对 `D:\projects\codex` 与当前 `D:\openvulcan\vulcan-code` 的会话持久化链路做了源码级验证。结论是：

1. Codex 的核心事实源是单个 append-only rollout JSONL 文件，位于 `~/.codex/sessions/YYYY/MM/DD/rollout-YYYY-MM-DDThh-mm-ss-<thread_id>.jsonl`；归档后移动到 `~/.codex/archived_sessions/<same-file-name>.jsonl`。
2. Codex 的 SQLite state DB 与 `session_index.jsonl` 都是索引 / 读模型 / 兼容层，不是 transcript 事实源；路径读取会校验 JSONL 首条 `SessionMeta`，DB 不可信时回退文件扫描并修复 DB。
3. Codex 的常规运行写入是按 `RolloutItem` 追加：`SessionMeta`、`ResponseItem`、`EventMsg`、`Compacted`、`TurnContext`。它不会在 turn 结束后用完整内存状态重建 rollout 文件。
4. 当前 Vulcan 的实现虽然有 append 分支，但入口仍是“完整 `conversation_state.records` 投影成完整 messages，再从完整 messages 重建 event rows，再读取旧 JSONL 做前缀比较”。一旦前缀不匹配，就删除整个会话事件目录并重写。
5. 当前 Vulcan 的“runtime-state-only”路径只覆盖 SQLite `session_runtime_state`，不覆盖 event store 里的 `runtime_state.json` 与 `context/current.json`；加载时文件缓存又会覆盖 SQLite 字段，因此非常容易出现状态错乱。
6. 建议对齐 Codex 的 append-only 事实源语义，但不要完全复制 Codex 的文件路径模型；Vulcan 应采用项目化 session 目录：`~/.vulcan/code/sessions/<project_tag>/<YYYY-MM-DD>-<session_id>/metadata.json`、`sXXX.jsonl`、`state.json`、`task/<subagent_session_id>/metadata.json`、`task/<subagent_session_id>/sXXX.jsonl` 等，归档移动到 `~/.vulcan/code/archived_sessions/<project_tag>/<YYYY-MM-DD>-<session_id>/`。
7. 现有 FTS 搜索也应从全局库迁移到 session 库：全局库只做 session catalog / routing / 粗粒度统计，具体全文倒排索引由主 session 目录内的 `indexes/search.sqlite` 承载。搜索范围只包含主会话用户 prompt 与 agent assistant 输出，不包含思考内容、子代理详细日志、工具输出噪声。
8. 最终实现不做旧数据库、旧 session、旧 event store 兼容。功能完成后直接删除除原始配置文件以外的旧 SQL、session、event store、FTS 数据，以新 schema 全新开始。

## 2. Codex 源码验证范围

主要验证文件：

- `D:\projects\codex\codex-rs\rollout\src\lib.rs`
- `D:\projects\codex\codex-rs\rollout\src\recorder.rs`
- `D:\projects\codex\codex-rs\rollout\src\policy.rs`
- `D:\projects\codex\codex-rs\rollout\src\list.rs`
- `D:\projects\codex\codex-rs\rollout\src\metadata.rs`
- `D:\projects\codex\codex-rs\rollout\src\state_db.rs`
- `D:\projects\codex\codex-rs\rollout\src\session_index.rs`
- `D:\projects\codex\codex-rs\protocol\src\protocol.rs`
- `D:\projects\codex\codex-rs\core\src\session\mod.rs`
- `D:\projects\codex\codex-rs\core\src\session\rollout_reconstruction.rs`
- `D:\projects\codex\codex-rs\app-server-protocol\src\protocol\thread_history.rs`
- `D:\projects\codex\codex-rs\thread-store\src\local\archive_thread.rs`
- `D:\projects\codex\codex-rs\thread-store\src\local\unarchive_thread.rs`
- `D:\projects\codex\codex-rs\thread-store\src\local\read_thread.rs`
- `D:\projects\codex\codex-rs\thread-store\src\local\update_thread_metadata.rs`

当前仓库主要验证文件：

- `D:\openvulcan\vulcan-code\apps\vulcan-code-gui\src\app_shell\pending_prompt_queue.rs`
- `D:\openvulcan\vulcan-code\apps\vulcan-code-gui\src\app_shell\session_transcript_runtime.rs`
- `D:\openvulcan\vulcan-code\apps\vulcan-code-gui\src\app_shell\mod.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-context\src\protocol.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-workspace-store\src\workspace_store\persistence.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-workspace-store\src\workspace_store\event_store.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-workspace-store\src\workspace_store\session_load.rs`

## 3. Codex 当前会话持久化流程

### 3.1 目录布局

Codex 在 `rollout/src/lib.rs` 中定义：

- `SESSIONS_SUBDIR = "sessions"`
- `ARCHIVED_SESSIONS_SUBDIR = "archived_sessions"`

新会话路径在 `rollout/src/recorder.rs::precompute_log_file_info` 中生成：

```text
~/.codex/sessions/YYYY/MM/DD/rollout-YYYY-MM-DDThh-mm-ss-<thread_id>.jsonl
```

归档路径在 `thread-store/src/local/archive_thread.rs` 中处理：把 active rollout 文件移动到：

```text
~/.codex/archived_sessions/<same-file-name>.jsonl
```

反归档在 `thread-store/src/local/unarchive_thread.rs` 中通过文件名里的日期还原到：

```text
~/.codex/sessions/YYYY/MM/DD/<same-file-name>.jsonl
```

### 3.2 JSONL 行格式

Codex 的每一行是 `RolloutLine`：

```json
{
  "timestamp": "...",
  "type": "...",
  "payload": { "...": "..." }
}
```

`RolloutItem` 有五类：

- `SessionMeta(SessionMetaLine)`
- `ResponseItem(ResponseItem)`
- `Compacted(CompactedItem)`
- `TurnContext(TurnContextItem)`
- `EventMsg(EventMsg)`

`SessionMeta` 记录 session id、cwd、source、model_provider、base_instructions、dynamic_tools、memory_mode 等会话级信息。第一条 `SessionMeta` 是 canonical thread id 来源。

### 3.3 写入模型

Codex 的 `RolloutRecorder` 是后台 writer：

1. 新建会话时只预计算路径和 `SessionMeta`，不立即创建文件。
2. `record_canonical_items` 只把 `RolloutItem` 放入 bounded channel。
3. `persist` / `flush` / `shutdown` 是 durability barrier。
4. 首次 materialize 时写 `SessionMeta`。
5. 之后 `JsonlWriter::write_rollout_item` 每次 append 一行 JSONL。
6. 写失败时保留 `pending_items`，丢弃 file handle，下一次 barrier 重新打开并重试。

关键性质：Codex 的正常路径没有“turn 结束重建全量 JSONL”。

### 3.4 哪些内容会被持久化

`rollout/src/policy.rs` 负责过滤：

- `ResponseItem`：保存 message、reasoning、shell/tool call、tool output、web search、image generation、compaction、context compaction 等；不保存 `CompactionTrigger` / `Other`。
- `EventMsg`：有限模式保存 user/agent/reasoning、patch end、token count、goal updated、context compacted、MCP tool end、rollback、turn started/complete/aborted 等；扩展模式额外保存错误、exec command end、图片查看、协作 agent、动态工具等。
- `Compacted`、`TurnContext`、`SessionMeta` 永远保存。

### 3.5 运行时触发点

在 `core/src/session/mod.rs` 中：

- `send_event_raw` 将 `EventMsg` 包装为 `RolloutItem::EventMsg` 并持久化。
- `record_conversation_items` 先写内存 history，再把 `ResponseItem` 转为 `RolloutItem::ResponseItem` 持久化。
- `replace_compacted_history` 在压缩后追加 `RolloutItem::Compacted`，如果存在新的 baseline，再追加 `RolloutItem::TurnContext`。
- `record_context_updates_and_set_reference_context_item` 每个真实用户 turn 持久化一个 `TurnContext`，用于 resume/fork 恢复最新上下文基线。

### 3.6 读取和列表

列表路径：

- active：`sessions` 下按 `YYYY/MM/DD` 遍历。
- archived：`archived_sessions` 下 flat 遍历。
- 列表优先走 SQLite state DB；没有 DB 或 DB 路径失效时扫描文件。
- 文件扫描摘要只读头部若干行，拿 `SessionMeta`、preview、first user message。

按 id 查找：

- 先用 DB path。
- 若文件存在，还要读取首部 `SessionMeta` 校验 thread id。
- DB stale / mismatch 时回退文件搜索，并触发 read-repair 更新 DB。

完整 history：

- `thread-store/src/local/read_thread.rs::attach_history_if_requested` 调用 `RolloutRecorder::load_rollout_items`。
- `load_rollout_items` 当前会 `read_to_string` 整个 JSONL 文件并逐行解析。

这意味着 Codex 的列表性能被 DB 和头部读取缓解，但 resume / full history 仍有单文件全量读取问题。

### 3.7 压缩节点提取和恢复逻辑

`core/src/session/rollout_reconstruction.rs::reconstruct_history_from_rollout` 是核心。

恢复流程：

1. 从 `rollout_items` 末尾向前扫描。
2. 遇到 `Compacted`：
   - 如果带 `replacement_history`，它就是一个完整历史基线。
   - 记录 `rollout_suffix = compacted_index + 1..`，即只需要重放压缩点之后的 tail。
   - 反向扫描语义上会把更旧的 `TurnContext` baseline 清掉，除非同一段里有更晚的 `TurnContext` 重新建立 baseline。
3. 遇到 `ThreadRolledBack`：累计要丢弃的最新用户 turn 数。
4. 遇到 `TurnComplete` / `TurnAborted` / `TurnStarted`：用于界定反向扫描中的 turn segment。
5. 遇到 `UserMessage` 或可作为用户边界的 `ResponseItem`：该 segment 计为真实用户 turn。
6. 遇到 `TurnContext`：
   - 提取 `previous_turn_settings`：上一轮模型、realtime 状态。
   - 提取 `reference_context_item`：最新可恢复上下文基线。
7. 一旦同时拿到：
   - 最新 surviving `replacement_history`
   - 最新 surviving previous turn settings
   - reference context 状态
   就可以停止继续向前扫描。
8. 然后用 `replacement_history` 初始化 `ContextManager`，再正向重放 `rollout_suffix`：
   - `ResponseItem` 进入 history。
   - `Compacted(replacement_history)` 理论上替换 history。
   - legacy `Compacted` 没有 replacement_history 时使用旧兼容逻辑重建。
   - `ThreadRolledBack` 正向 drop last N user turns。
   - `EventMsg`、`TurnContext`、`SessionMeta` 不直接进入模型 history。

这个设计天然支持未来 lazy reverse loader。对我们的分段方案来说，应该按 segment 从后往前扫，找到最新有效 `Compacted(replacement_history)` 后停止，而不是加载全部段。

### 3.8 显示历史构建

`app-server-protocol/src/protocol/thread_history.rs::build_turns_from_rollout_items` 把 `RolloutItem` 转成 UI `Turn`：

- `EventMsg` 进入 `handle_event`。
- `Compacted` 进入 `handle_compacted`。
- `ResponseItem` 只处理特殊 hook prompt 用户消息。
- `TurnContext` 与 `SessionMeta` 对显示 turn 忽略。

显示层和模型 history 恢复是两条 reducer：显示 reducer 从 event stream 还原 UI turn，模型 reducer 从 `ResponseItem` / `Compacted` / rollback 还原模型上下文。

### 3.9 元数据更新

Codex 不直接修改原 JSONL 行：

- 会话名：追加到根目录 `session_index.jsonl`，读时从文件末尾找最新 name。
- memory mode：读取首部 `SessionMeta`，修改 memory_mode 后追加新的 `SessionMeta` 行。
- git metadata：读取首部 `SessionMeta`，更新 git 后追加新的 `SessionMeta` 行。
- SQLite state DB 同步更新，但失败多数是 best-effort，不阻断 JSONL durability。

注意：`load_rollout_items` 用第一条 `SessionMeta` 作为 canonical thread id；但 metadata extraction 会扫描所有 items，并用最新追加的 `SessionMeta` 得出 memory mode 等字段。

## 4. 当前 Vulcan 会话持久化流程

### 4.1 用户 prompt 发起

`pending_prompt_queue.rs::begin_session_run_from_prompt_entry`：

1. 确保 runtime state loaded。
2. snapshot 上一轮上下文。
3. `append_user_prompt_to_session_conversation` 向 protocol message state 追加用户消息。
4. `begin_shared_prompt_turn_with_runtime_state` 建立 hook/precheck 状态。
5. 非 delegate completion 时，`push_visible_session_record_message_with_metadata` 追加可见用户记录。
6. 如果进入 precheck，会立即 `persist_single_session_update_task` 保存用户输入。

`continue_session_run_after_precheck`：

1. precheck 后继续 run。
2. 如果 hook 改写了 prompt，`patch_latest_prechecked_user_prompt` 修改最新用户记录。
3. 对 runs_user_prompt_precheck 的请求再次触发 `persist_single_session_update_task`。

这里已经存在“历史记录可能被 patch”的语义，这与纯 append-only 冲突。需要改成追加 `PromptPatched` / `RecordSuperseded` 类事件，显示 reducer 取最新版本，而不是原地改旧记录。

### 4.2 turn 结束

`session_transcript_runtime.rs::apply_execution_run_report`：

1. 将报告里的 `conversation_state` 写入 `runtime_state.conversation_state`，但保留原 `records`。
2. `sync_session_messages_from_conversation_state` 从 `conversation_state.records` 全量投影显示 messages。
3. 成功时追加 assistant output、summary、turn stats 等可见/隐藏记录。
4. 执行 hook / VMM writeback / maybe compaction。
5. 最后调用 `persist_single_session_update_task`。

关键问题：turn 结束后走的是“单会话完整保存任务”，不是“追加本 turn 新事件”。

### 4.3 checkpoint

`apply_execution_conversation_checkpoint`：

1. 更新 `runtime_state.conversation_state`。
2. `sync_session_messages_from_conversation_state` 全量投影内存可见状态。
3. 调用 `persist_single_session_runtime_state_task`。

`persist_single_session_runtime_state_task` 最终只调用 `workspace_store::persist_single_session_runtime_state_update`，只覆盖 SQLite `session_runtime_state` 行。

问题：event store 下的 `runtime_state.json` 与 `context/current.json` 不会被同步覆盖。

### 4.4 workspace store 保存

`persistence.rs::persist_single_session_update`：

1. upsert session row。
2. 如果 runtime_state 存在，优先用 `conversation_state_to_session_messages(&runtime_state.conversation_state)` 重建完整 messages；只有 rebuilt 为空且传入 messages 非空时才 fallback。
3. 调用 `persist_session_event_store`。
4. 更新 event store projection。
5. 覆盖 SQLite runtime_state。

`persistence.rs::persist_workspace_snapshot` 更大范围地：

1. upsert projects/sessions。
2. `replace_session_event_stores`。
3. `replace_session_compactions`。
4. `replace_session_runtime_state`。

`replace_session_event_stores` 对需要写的 session 逐个调用 `persist_session_event_store`，然后删除并重建 SQLite 投影表：`workspace_history_search_docs`、`session_turn_index`、`session_message_chunks`、`session_messages` 等。

### 4.5 event store 写入

`event_store.rs::persist_session_event_store`：

1. 根据完整 messages 调用 `build_persisted_session_event_rows`，按 position 生成完整 seq / turn_index / turn_id。
2. `try_append_session_event_store_if_prefix_matches`：
   - 读取现有 `events/000001.jsonl` 全部 rows。
   - 如果旧 rows 比新 rows 多，失败。
   - 如果任意前缀 row 的语义字段不一致，失败。
   - 前缀匹配才 append 新 suffix。
3. 如果 append 失败，`recreate_session_store_directory` 删除整个会话 event store 目录。
4. 全量写入新的 `events/000001.jsonl`。
5. 无论 append 还是重写，都会覆盖：
   - `runtime_state.json`
   - `context/current.json`
   - compaction checkpoints
   - manifest

问题本质：这是“全量快照重建 + 尝试优化成 append”，不是“append-only event log”。

### 4.6 加载

`session_load.rs::load_event_store_window_messages`：

1. 根据 SQLite locator 与 turn rows 找 event store。
2. 调用 `load_session_event_store` 全量读取 event rows。
3. 再从全量 rows 里筛选 selected turn。

`hydrate_session_runtime_state`：

1. 读 SQLite runtime_state row。
2. 再读 event store 文件侧 `runtime_state.json`。
3. 再读 `context/current.json` 或 snapshot。
4. 文件侧 cache 中多个字段会优先覆盖 SQLite：active agent/provider/model、prompt overrides、context injection、VMM、profile sync、provider usage、ui_state、conversation_state 等。

因此 checkpoint 只写 SQLite 的路径会被旧文件缓存反向覆盖，形成“刚保存状态但下次恢复又回旧值”的错乱。

## 5. 当前错乱根因

### 5.1 事实源分裂

当前事实源同时存在：

- SQLite session rows。
- SQLite `session_runtime_state`。
- event store `events/000001.jsonl`。
- event store `runtime_state.json`。
- event store `context/current.json`。
- `session_messages` / `session_turn_index` / `workspace_history_search_docs` 投影表。
- 内存里的 `conversation_state.records` 与 `session_messages`。

这些层不是严格单向派生关系，存在互相覆盖。

### 5.2 turn 结束全量重建

用户指出“会话一旦结束一个 turn，会对持久化内容进行重建”是准确的。当前 `persist_single_session_update` 会从 runtime_state 的完整 conversation state 重建 messages，再生成完整 event rows。即使后续 append 分支成功，也已经做了完整 O(n) 构建与旧文件 O(n) 前缀比较。

### 5.3 任何旧记录变化都会触发整目录重建

例如 precheck 改写最新 user prompt、流式记录补 delta、metadata 变化、created_at 变化、record id 修复等，只要旧 rows 与新 rows 前缀不一致，就会删除整个 event store 目录并重写。

这会带来：

- IO 放大。
- 历史文件短暂不存在或不完整的窗口。
- blob/checkpoint/runtime/context 一起被删除重建。
- 并发 persist 任务互相覆盖风险。

### 5.4 runtime-state-only 与文件侧 context 不一致

checkpoint 只写 SQLite，但加载时文件侧 `runtime_state.json` / `context/current.json` 又优先覆盖 SQLite，导致“状态覆盖保存”不完整。

### 5.5 显示记录和模型上下文记录混在一个重建入口

Codex 明确区分：

- `EventMsg` / display reducer：用于 UI turn。
- `ResponseItem` / compaction reducer：用于模型 history。
- `TurnContext`：用于恢复上下文基线。
- `SessionMeta`：用于 session metadata。

当前 Vulcan 主要把 `SessionMessageEntry` 同时当作显示、搜索、turn index、context 派生来源，导致所有视图都被迫随完整 messages 重建。

## 6. 目标方案：匹配 Codex 语义，但采用 Vulcan 项目化分段目录

### 6.1 目录布局

建议采用：

```text
~/.vulcan/code/sessions/<project_tag>/<YYYY-MM-DD>-<session_id>/
  metadata.json
  s001.jsonl
  s002.jsonl
  s003.jsonl
  state.json
  indexes/
    search.sqlite
    projection.sqlite 或其他可重建读模型
  task/
    <subagent_session_id_1>/
      metadata.json
      s001.jsonl
      state.json
    <subagent_session_id_2>/
      metadata.json
      s001.jsonl
      state.json
  blobs/
    ...
```

归档：

```text
~/.vulcan/code/archived_sessions/<project_tag>/<YYYY-MM-DD>-<session_id>/
  metadata.json
  s001.jsonl
  s002.jsonl
  task/
    <subagent_session_id>/
      metadata.json
      s001.jsonl
      state.json
  ...
```

反归档时保持同一个 `<project_tag>/<YYYY-MM-DD>-<session_id>` 目录名，从 `archived_sessions` 移回 `sessions`。

路径设计原则：

- `project_tag` 是一等目录维度，用于隔离不同项目的会话集合。
- `<YYYY-MM-DD>-<session_id>` 保留创建日期，避免 Codex 那种深层 `YYYY/MM/DD` 日期树，同时让人工排查时仍可按日期识别。
- active 与 archived 目录结构保持同构，只移动整个 session 目录，不改 segment 内容。
- `metadata.json` 记录分段索引与会话元信息；`state.json` 记录可覆盖运行时状态；`sXXX.jsonl` 是 append-only 会话事件段。
- `task/<subagent_session_id>/` 是子代理 task session 目录，使用与主会话同构的分段存储：`metadata.json`、`sXXX.jsonl`、`state.json`；子代理不提升为顶层 session。
- `indexes/search.sqlite` 是主 session 私有 FTS 库，随主会话 segment 增量更新；全局库不再保存会话正文 FTS 倒排数据。子代理 task 目录不需要自己的 `search.sqlite`。

### 6.2 metadata.json

建议 `metadata.json` 是当前 session 目录的可覆盖元数据，记录分段索引和最新状态：

```json
{
  "schema_version": 1,
  "session_id": "...",
  "project_tag": "...",
  "created_at": "...",
  "updated_at": "...",
  "archived_at": null,
  "project_id": "...",
  "title": "...",
  "title_source": "...",
  "canonical_source": "segmented_rollout",
  "active_segment": "s003.jsonl",
  "task_dir": "task",
  "next_global_seq": 8573,
  "segment_policy": {
    "max_lines": 3000,
    "max_bytes": 8388608
  },
  "segments": [
    {
      "file": "s001.jsonl",
      "start_seq": 0,
      "line_count": 3000,
      "byte_len": 1234567,
      "first_timestamp": "...",
      "last_timestamp": "...",
      "first_turn_index": 0,
      "last_turn_index": 42,
      "last_compaction_seq": 2400
    },
    {
      "file": "s002.jsonl",
      "start_seq": 3000,
      "line_count": 3000,
      "byte_len": 1234567,
      "first_timestamp": "...",
      "last_timestamp": "...",
      "first_turn_index": 43,
      "last_turn_index": 97,
      "last_compaction_seq": null
    },
    {
      "file": "s003.jsonl",
      "start_seq": 6000,
      "line_count": 2573,
      "byte_len": 987654,
      "first_timestamp": "...",
      "last_timestamp": "...",
      "first_turn_index": 98,
      "last_turn_index": 130,
      "last_compaction_seq": 8120
    }
  ],
  "tasks": [
    {
      "session_id": "<subagent_session_id>",
      "dir": "task/<subagent_session_id>",
      "active_segment": "task/<subagent_session_id>/s001.jsonl",
      "metadata_file": "task/<subagent_session_id>/metadata.json",
      "agent_kind": "main_clone",
      "agent_profile": {
        "agent_md_path": null,
        "agent_id": null,
        "agent_name": null
      },
      "parent_start_seq": 8421,
      "parent_end_seq": 8488,
      "status": "completed",
      "line_count": 316,
      "created_at": "...",
      "updated_at": "..."
    }
  ]
}
```

用户示例中的：

```text
s001.jsonl 3000
s002.jsonl 3000
s003.jsonl 2573
```

应直接成为 metadata 的 segment line_count。最后一个 segment 就是最新显示部分的首选读取对象；如果 UI 需要更多 turn，再按 metadata 从后往前补段。

### 6.3 JSONL 事件模型

建议采用 Codex `RolloutItem` 语义，而不是继续以 `SessionMessageEntry` 作为唯一事实源。

Vulcan 可定义等价事件：

- `SessionMeta`
- `DisplayEvent` 或 `EventMsg`
- `ResponseItem` / `ModelHistoryItem`
- `Compacted`
- `TurnContext`
- `RecordPatched` / `RecordSuperseded`
- `ThreadRolledBack`
- `TurnStarted`
- `TurnComplete`
- `TurnAborted`
- `SubagentTaskMeta`
- `SubagentTaskStarted`
- `SubagentTaskEventLinked`
- `SubagentTaskCompleted`

每个子代理 task session 与主会话遵循同一套分段原则。`task/<subagent_session_id>/metadata.json` 是可覆盖元数据；`task/<subagent_session_id>/s001.jsonl` 的第一行必须是 canonical `SubagentTaskMeta`，用于描述这个子代理是谁、从哪里派生、使用哪套提示词。父会话 `metadata.json.tasks[]` 只保留该 metadata 的轻量镜像，方便列表和状态展示；完整事实源仍在子代理 task session 目录内。

`SubagentTaskMeta` 建议包含：

```json
{
  "seq": 0,
  "timestamp": "...",
  "type": "subagent_task_meta",
  "payload": {
    "task_session_id": "<subagent_session_id>",
    "parent_session_id": "<parent_session_id>",
    "parent_project_tag": "<project_tag>",
    "agent_kind": "main_clone",
    "agent_profile": {
      "agent_id": null,
      "agent_name": null,
      "agent_md_path": null,
      "prompt_source": "main_session"
    },
    "spawned_by": {
      "parent_turn_id": "...",
      "parent_event_seq": 8421,
      "tool_call_id": "..."
    },
    "created_at": "..."
  }
}
```

`agent_kind` 至少需要覆盖：

- `main_clone`：主代理克隆体，继承主会话的系统提示词、工具策略、模型上下文与项目规则基线。
- `agent_md`：使用某个特定 `agent.md` / agent profile 文件作为提示词来源的子代理。
- `custom`：由运行时动态构造提示词或临时 agent profile 的子代理。

当 `agent_kind = "agent_md"` 时，`agent_profile.agent_md_path` 必须写入规范化后的路径或稳定资源标识；如果有 agent id/name，也应同步写入。这样查询子代理运行信息时，不仅能从父会话跳到 task 日志，也能知道该子代理是主代理克隆体，还是某个特定 agent.md 驱动的代理。

子代理关联事件必须写入父会话 `sXXX.jsonl`，用于把父会话显示记录、搜索结果和子代理 task 日志关联起来。建议 payload 至少包含：

```json
{
  "task_session_id": "<subagent_session_id>",
  "task_dir": "task/<subagent_session_id>",
  "task_active_segment": "task/<subagent_session_id>/s001.jsonl",
  "task_metadata_file": "task/<subagent_session_id>/metadata.json",
  "agent_kind": "main_clone",
  "agent_md_path": null,
  "agent_id": "...",
  "agent_role": "...",
  "parent_turn_id": "...",
  "parent_event_seq": 8421,
  "status": "running"
}
```

子代理自身的详细运行事件写入其 task session 分段目录：

```text
~/.vulcan/code/sessions/<project_tag>/<YYYY-MM-DD>-<parent_session_id>/task/<subagent_session_id>/
  metadata.json
  s001.jsonl
  s002.jsonl
  state.json
```

子代理 task session 也采用 append-only segment 行模型，但它的作用域是父 session 下的 task，不参与顶层 session 列表，也不进入 FTS 搜索范围。除首段第一行 `SubagentTaskMeta` 外，后续行记录该子代理的运行事件、工具调用、输出、错误、完成状态、压缩节点与统计信息。子代理如果产生超长任务，同样按 `s001.jsonl`、`s002.jsonl`、`s003.jsonl` rollover。

子代理 task session 与主会话共用压缩和恢复语义：允许写入 `Compacted`、`TurnContext`、summary / stats 等节点，以兼容主代理的自动压缩、上下文恢复和存储维护方式。

主会话 segment 每行建议带全局 seq；子代理 task segment 每行使用 task-local seq，并通过 `SubagentTaskMeta.spawned_by.parent_event_seq` 与父会话全局 seq 关联：

```json
{
  "seq": 8572,
  "timestamp": "...",
  "type": "event_msg",
  "payload": { "...": "..." }
}
```

Codex 原始 `RolloutLine` 没有 seq，靠文件顺序。我们因为分段，需要显式 seq，便于：

- 跨 segment 拼接。
- SQLite / 搜索索引增量 upsert。
- 断电后校验 tail。
- 分段倒扫压缩锚点。

### 6.4 写入规则

正确规则应是：

1. 会话显示部分 JSONL 只追加，不由完整 messages 重建。
2. 会话状态覆盖写 `state.json` / `metadata.json`。
3. SQLite 只做 read model，可在新 schema 数据范围内丢弃重建。
4. 搜索索引、turn index、message projection 从新 append 的事件增量派生；其中 FTS 搜索索引进入主 session 本地库，不进入全局库。
   FTS 只索引主会话的 `UserPrompt` / 用户 transcript 与 agent `AssistantOutput`；不索引 thinking、reasoning raw content、tool call/output、子代理 task 详细日志。
5. prompt 被 hook 改写时，不修改旧 user record，而是追加 `RecordPatched` 或 `PromptTransformed` 事件。
6. rollback / edit / rerun 不删除历史段，追加 `ThreadRolledBack` / `ForkedFrom` / `ReplacementHistory` 标记，由 reducer 决定当前视图。
7. 子代理运行详情追加到父 session 目录下的 `task/<subagent_session_id>/sXXX.jsonl`；task 首段首行写 `SubagentTaskMeta`，父会话主 JSONL 只追加关联事件和摘要事件。
8. 只有文件损坏修复、明确 compact-storage maintenance，才允许 segment 重写或截断；不为旧数据迁移保留常规重写路径。

### 6.5 读取规则

列表：

- 优先读 state DB。
- DB 不存在时读每个 session 目录的 `metadata.json`。
- 不读 segment body。

显示：

- 默认读 `meta.active_segment`。
- 如果最新 segment 不足以填满 UI window，再按 meta 从后向前读取前一个 segment。
- reducer 只为显示窗口构建 turn，不要求读全量 session。
- 遇到 `SubagentTask*` 关联事件时，先显示父会话中的摘要 / 状态；用户展开子代理详情时，再按 `task_dir` 与 task `metadata.json` 懒加载对应 task segment window。

恢复模型上下文：

- 从最后 segment 末尾开始反向扫描。
- 找最新 surviving `Compacted(replacement_history)`。
- 同时找 latest `TurnContext` / previous turn settings / rollback 边界。
- 找到恢复锚点后，正向重放锚点之后的 suffix。
- 如果没有压缩锚点，再按 token/turn budget 从后向前扫描到足够上下文；极端情况下才全量读取。

导出：

- 按 `metadata.segments` 顺序拼接 `sXXX.jsonl`。
- 如需完整导出子代理运行信息，同时导出 `task/<subagent_session_id>/metadata.json`、`task/<subagent_session_id>/sXXX.jsonl` 与 `state.json`，并保留父会话中的 `task_session_id` 关联。

归档：

- 移动整个 session 目录到 `archived_sessions`。
- 覆盖 `metadata.archived_at`。
- 更新 DB archived flag。
- 不改 segment 内容。

## 7. 与 Codex 语义的逐项匹配

| Codex 语义 | Vulcan 建议匹配方式 |
|---|---|
| `sessions` / `archived_sessions` | 保持同名顶层语义，但根目录为 `~/.vulcan/code` |
| active 按日期目录组织 | 改为 `sessions/<project_tag>/<YYYY-MM-DD>-<session_id>/`，日期放在 session 目录名里 |
| archived flat | 改为 `archived_sessions/<project_tag>/<YYYY-MM-DD>-<session_id>/`，与 active 同构 |
| 一个 thread 有一个 rollout 事实源 | 一个 session 目录是事实源，目录内分段 JSONL 是事件事实源 |
| rollout JSONL append-only | segment JSONL append-only |
| 首条 `SessionMeta` canonical | `metadata.json` canonical + `s001.jsonl` 首条 `SessionMeta` 审计 |
| state DB 是索引 | SQLite 只做可重建 read model |
| 会话名 append-only index | 进入 `metadata.json` 覆盖 + append audit，是否另设索引文件由搜索需求决定 |
| memory mode/git 追加 `SessionMeta` | 建议覆盖 `metadata.json`，必要时追加 `SessionMetaUpdated` 审计事件 |
| `Compacted(replacement_history)` 恢复基线 | 分段倒扫 `Compacted` |
| `TurnContext` 恢复 runtime baseline | 每个真实 user turn 追加 `TurnContext` |
| rollback 追加 marker | 追加 rollback marker，不删历史段 |
| 子代理运行信息 | 写入父 session 目录下 `task/<subagent_session_id>/sXXX.jsonl`，首段首行为 `SubagentTaskMeta`，父会话 JSONL 追加关联事件 |

## 8. 需要改造的模块边界

### 8.1 新增持久化抽象

建议新增一个独立 crate 或 workspace-store 子模块：

```text
crates/vulcan-workspace-store/src/workspace_store/segmented_session_store.rs
```

职责：

- 创建 session 目录。
- 追加事件。
- segment rollover。
- 原子覆盖 meta/state。
- segment 倒序扫描。
- segment 正序 replay。
- task session 分段 append、rollover 与按 id 懒加载。
- archive/unarchive 移动目录。
- rebuild 新 schema read model。

### 8.2 替换持久化入口

需要将当前入口拆成三类：

1. `append_session_events(session_id, events)`
   - 用户 prompt。
   - assistant output。
   - tool events。
   - summary / stats。
   - compaction marker。

2. `overwrite_session_state(session_id, state)`
   - runtime state。
   - context current。
   - continuation。
   - VMM/profile sync。

3. `update_session_meta(session_id, meta_patch)`
   - title。
   - archived flag。
   - model/provider summary。
   - updated_at。

旧的 `persist_single_session_update` 不应继续作为 turn 结束默认路径。

### 8.3 reducer 分层

建议显式拆出三个 reducer：

1. Display reducer：事件流 -> UI turn / visible transcript。
2. Model history reducer：`ResponseItem` + `Compacted` + rollback -> provider context history。
3. Runtime reducer：`TurnContext` + state snapshot -> runtime baseline。

当前 `SessionMessageEntry` 应降级为投影模型，而不是事实源。

### 8.4 SQLite 与 FTS 角色调整

全局 SQLite 只保存：

- session 列表元数据。
- 最近窗口缓存。
- project tag、session path、archived flag、updated_at、title、preview、首条用户消息、segment summary 等粗粒度字段。
- 可用于搜索路由的轻量统计，例如某 session 是否有可搜索内容、最近索引更新时间、FTS 库版本。

每个 session 目录内应有自己的 session 库：

```text
~/.vulcan/code/sessions/<project_tag>/<YYYY-MM-DD>-<session_id>/indexes/search.sqlite
```

session 库职责：

- 保存该主 session 的 FTS5 表，例如 `session_search_fts`。
- 保存该 session 的 message projection / turn index / segment cursor / task cursor / task segment cursor / task metadata projection。
- FTS 只从主会话 `sXXX.jsonl` 中的用户 prompt 与 assistant 输出增量更新。
- task projection 从 `task/<subagent_session_id>/sXXX.jsonl` 增量更新，但 task 详细内容不进入 FTS。
- 删除或损坏后只需重放该 session 的 segments 重建，不影响其他 session。

全局搜索流程建议：

1. 先查全局 catalog，按 `project_tag`、archived 状态、时间范围、模型、标题等条件缩小 session 集合。
2. 对候选 session 并发查询各自的 `indexes/search.sqlite`。
3. 汇总并按 BM25 / 时间 / session 权重排序。
4. 点击主会话结果时通过 `metadata.json` 的 segment 索引定位对应 `seq`，再读取必要 segment window。
5. 子代理详细日志不作为全文搜索命中来源；从父会话结果或子代理状态入口进入时，通过 `task_session_id` 与 `task_dir` 定位 `task/<subagent_session_id>/metadata.json`，再读取对应 task segment window。

这样可以避免一个全局 FTS 表承载所有会话正文，也避免把思考内容、工具噪声和子代理长日志纳入全文搜索。全局库缺失或不一致时应能从 session 目录重建 catalog；session 库缺失或不一致时应能从该 session 的 `metadata.json`、主会话 `sXXX.jsonl` 与 `task/<subagent_session_id>/sXXX.jsonl` 重建。

### 8.5 全新初始化策略

最终实现完成后，不做旧数据兼容迁移。启动策略改为：

1. 保留原始配置文件，例如 provider 配置、用户设置、项目配置、工具配置等。
2. 删除旧 SQL 数据库、旧 session 目录、旧 event store 目录、旧 FTS / search projection 数据。
3. 首次启动时创建新的 `~/.vulcan/code/sessions` 与 `~/.vulcan/code/archived_sessions` 目录。
4. 首次启动时创建新的全局 catalog 库。
5. 新 session 创建时初始化 `metadata.json`、`state.json`、`s001.jsonl`、`task/` 和主 session 本地 `indexes/search.sqlite`；新子代理创建时初始化 `task/<subagent_session_id>/metadata.json`、`state.json` 与 `s001.jsonl`。
6. 新 schema 内仍保留 rebuild 能力：全局 catalog 可从新 session 目录重建，session 本地 FTS / turn index / task index 可从该 session 的主会话 `sXXX.jsonl` 与 `task/<subagent_session_id>/sXXX.jsonl` 重建。

明确不做：

- 不读取旧 `events/000001.jsonl`。
- 不把旧 SQLite 表迁移到新 catalog。
- 不把旧 `workspace_history_search_docs` / `session_messages` / `session_turn_index` 转换为新 session 库。
- 不保留旧 store 的双读逻辑。
- 不为旧 session 提供自动导入入口。

## 9. 推荐实施顺序

### 阶段一：停止继续扩大错乱

1. 修复 runtime-state-only：checkpoint 覆盖 SQLite 的同时，也覆盖文件侧 `runtime_state.json` 与 `context/current.json`，或停止在加载时让旧文件缓存覆盖新 SQLite。
2. 将 turn 结束后的保存拆分：新增 append API，先让新 turn 事件直接 append 到现有 JSONL，不再完整重建。
3. 禁止常规 persist 删除整个 event store 目录；只有显式 repair/maintenance 才允许。

### 阶段二：引入 Codex 风格事件事实源

1. 定义 Vulcan `RolloutItem` 等价结构。
2. 将用户输入、assistant 输出、tool call、summary/stats、compaction、turn context 都改成事件追加。
3. `SessionMessageEntry` 从事件 reducer 派生。
4. prompt patch 改成 append patch event。

### 阶段三：分段 session 目录

1. 新建主会话 `metadata.json` / `sXXX.jsonl` 写入器，以及子代理 task session `metadata.json` / `sXXX.jsonl` 写入器。
2. 实现 3000 行或 bytes 阈值 rollover。
3. 实现 active segment tail read。
4. 实现倒序 segment scan，恢复 compaction anchor。
5. 实现 archive/unarchive 移动目录。

### 阶段四：read model 和全新初始化

1. SQLite read model 改成从 append 增量更新，并把 FTS 从全局库迁移到 session 本地库。
2. 提供新 schema rebuild 命令。
3. 提供开发完成后的清库启动说明：删除旧 SQL、旧 session、旧 event store、旧 FTS，只保留原始配置。
4. 加一致性审计：metadata segments line_count、文件实际行数、last seq、DB locator。

## 10. 验收标准

1. 一个正常 turn 结束后，已有 segment 文件只增加新行，不发生整文件重写。
2. `state.json` / `metadata.json` 覆盖写，显示 JSONL 不被覆盖。
3. checkpoint 后重启，会恢复 checkpoint 时的最新 runtime/context，不被旧文件缓存覆盖。
4. prompt hook 改写不会修改历史 user 行，只追加 patch event；显示 reducer 正确展示最新 prompt。
5. archive/unarchive 只移动 session 目录并更新 meta/DB，不改 segment。
6. 长会话列表不读取 segment body。
7. 长会话打开默认只读最后 segment。
8. resume 从最后 segment 倒扫，找到 `Compacted(replacement_history)` 后只重放 suffix。
9. 子代理运行信息写入父 session 的 `task/<subagent_session_id>/sXXX.jsonl`；`task/<subagent_session_id>/s001.jsonl` 首行存在 `SubagentTaskMeta`，明确标识 `agent_kind` 是主代理克隆体、特定 `agent.md` 代理或动态 custom 代理；父会话主 JSONL 中存在可解析的 `task_session_id` / `task_dir` 关联事件，能从父会话查询到子代理详情。
10. FTS 只索引主会话用户 prompt 和 agent assistant 输出，不索引思考内容、工具输出、子代理 task 详细日志。
11. 全局 SQLite 删除后可以从新 schema 的 `sessions` / `archived_sessions` 完整重建 session catalog；任一 session 的 `indexes/search.sqlite` 删除后可以从该 session 的主 segments 重建 FTS，并从主 segments 与 task segments 重建 turn/task projection。
12. 删除旧 SQL、旧 session、旧 event store 后，应用可以用原始配置文件全新启动，并创建新 schema 数据；不存在旧数据兼容读取路径。

## 11. 风险与注意事项

1. 这是存储事实源调整，属于核心架构变更，实施前需要确认新 schema。
2. 必须避免双事实源并存；最终状态是旧 store 完全删除，新写只进 segmented store。
3. reducer 必须有确定性测试：同一事件流多次 replay 结果完全一致。
4. 分段 meta 更新要原子化：先写临时 meta，再 rename。
5. segment append 失败时不能更新 meta line_count。
6. 需要处理最后一行半写入：读取时允许忽略尾部损坏行，并通过 meta 审计提示 repair。
7. blob 外置需要跟 seq 绑定，不能在重写时改变路径。
8. 并发写入必须单 session 单 writer，避免两个任务同时 append 同一个 segment。

## 12. 最终建议

建议不要继续在现有 `persist_session_event_store` 上做补丁式修复。它的基本模型是“完整快照重建 + 前缀匹配优化”，与目标的“追加事件事实源 + 覆盖状态文件 + 可重建读模型”方向相反。

最优路径是：

1. 短期先修 `runtime-state-only` 与文件侧 cache 不一致。
2. 中期引入 Codex 风格 append-only rollout event。
3. 长期落到 session directory + segmented JSONL + meta index。

这样既能匹配 Codex 不乱改历史 JSONL 的稳定性，也能避免 Codex 单 session 单巨型文件导致的后期加载和修改成本。
