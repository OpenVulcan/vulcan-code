# Provider Adapter Runtime 重构执行记录

## 任务来源

- 执行计划：`docs/plan/20260609-01-RUNTIME_JSONL_PROVIDER_ADAPTER_REARCHITECTURE.md`
- 目标：完整落地 runtime 事实层、Provider 协议转换、长期/动态注入分层、恢复迁移、压缩备份与验证闭环。

## 当前进度

### 2026-06-09

1. 已读取执行计划，确认本次为核心会话架构改造。
2. 已确认必须使用本文件作为持续进度记录，后续遗忘时从计划文件和本文件恢复上下文。
3. 正在执行阶段一：现状梳理。

## 阶段状态

- 阶段一：现状梳理，进行中。
- 阶段二：Runtime Schema 落地，未开始。
- 阶段三：Provider 请求改造，未开始。
- 阶段四：注入链路改造，未开始。
- 阶段五：旧会话迁移与恢复，未开始。
- 阶段六：上下文压缩备份，未开始。
- 阶段七：测试与验证，未开始。

## 当前发现

1. Provider 请求构建入口集中在 `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_turn_request.rs` 与各 Provider HTTP adapter。
2. 现有 `provider_request_bridge_messages.rs` 已有孤儿工具结果和未完成工具批次降级逻辑，但输入仍来自当前 conversation messages。
3. `SessionRuntimeStateEntry` 已有 `conversation_state`，说明内存侧存在规范会话状态雏形；本次需要补独立 runtime 落盘日志与优先恢复链路。
4. 当前长期注入和动态注入已在 `CoreProviderContextInjection` / `ResolvedInjectionView` 中分离，但 `apply_context_injection_overlay` 会把 persistent blocks 注入为 user 历史消息，这需要后续改为 system prompt layer。
5. 分段展示会话存储在 `vulcan-workspace-store/src/workspace_store/segmented_session_store.rs`，metadata 已维护 segments，不能硬编码 `s001.jsonl`。

## 已完成修改

1. 新增 `crates/vulcan-context/src/runtime.rs`：
   - 定义 runtime JSONL 事件模型。
   - 定义 user、assistant、tool batch、tool call、tool result、provider checkpoint、compaction、repair、injection applied 事件。
   - 提供 `runtime_log_lines_from_conversation_state` 和 `conversation_state_from_runtime_log_lines`。
   - 增加工具批次与孤儿工具结果投影测试。
2. 更新 `crates/vulcan-context/src/lib.rs` 导出 runtime 类型和投影函数。
3. 更新 `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store.rs`：
   - 增加 `runtime_metadata.json`、`runtime.index.json`、`runtime/r001.jsonl` 相关常量。
   - 增加 runtime segment metadata 与 runtime index 类型。
   - 增加 runtime JSONL 重建、读取、索引重建、缺失时从展示分段反推生成的函数。
4. 更新 `crates/vulcan-workspace-store/src/workspace_store/persistence.rs`：
   - 单会话 runtime state checkpoint 保存时同步生成 runtime 日志。
   - 分段 session tail 保存 runtime state 时同步生成 runtime 日志。
5. 更新 `crates/vulcan-workspace-store/src/workspace_store/session_load.rs`：
   - 加载时优先确保并读取 runtime 日志。
   - runtime 缺失时从展示分段 metadata replay 反推，并立即生成 runtime 日志。
   - state 继续保留 provider/model/UI 等轻量字段，conversation window 由 runtime 优先覆盖。
6. 更新 `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_turn_request.rs`：
   - 长期 persistent 注入改为拼入 provider-facing system prompt。
   - 动态 transient 注入继续只前置到当前逻辑 user prompt。
   - 调整测试，确认 persistent 不再污染 user 历史。
7. 更新 `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_anthropic_http.rs`：
   - 在 Anthropic 请求发送前增加工具结果拓扑修复。
   - 合法并行 tool_result 继续保持原生 `tool_result` 批次。
   - 缺少紧邻 assistant `tool_use` 或 id 不匹配的 tool_result 降级为普通 user text，避免跨 Provider 切换后触发 400。
8. 更新 `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store.rs`：
   - 增加 runtime JSONL 容错读取，坏行会先备份原文件，再清洗重写有效行。
   - 增加 runtime metadata/index 损坏修复后的刷新逻辑。
   - runtime 全损坏且展示分段可 replay 时，自动从展示分段反推重建 runtime。
   - 增加上下文压缩前 runtime 备份，备份目录最多保留 5 份。
9. 更新 `crates/vulcan-workspace-store/src/workspace_store/mod.rs`：
   - 暴露 `backup_session_runtime_log_before_compaction`，由 session_id 查找真实分段会话目录，不依赖固定 `s001`。
10. 更新 GUI 与 WebUI 压缩入口：
   - `apps/vulcan-code-gui/src/app_shell/session_transcript_runtime.rs` 在压缩修改会话前备份 runtime 日志。
   - `apps/vulcan-code-tauri/src/service_bridge.rs` 在 WebUI 压缩完成写入前备份 runtime 日志，并通过 trace 记录结果。
11. 更新 `crates/vulcan-context-injection/src/lib.rs`：
   - 增加按 `InjectionKind + tag` 移除整个注入分组的通用方法。
   - 增加按 `InjectionKind + dedupe_key` 移除具体注入项的方法。
   - 增加按 `InjectionKind + metadata 字符串字段` 移除具体注入项的方法。
   - 补充测试，确认动态注入移除不会误删长期画像注入。
12. 更新 `crates/vulcan-agent-runtime/src/agent_runtime/context_injection_runtime.rs`：
   - 旧版 VMM 画像清理改为复用共享状态层的 `Persistent + tag` 移除能力，不再直接操作内部 map。
13. 更新 runtime 日志保存策略：
   - 新增 `sync_runtime_log_from_conversation_state`，正常前缀匹配时追加尾部事件。
   - 如果 runtime 缺失、读取失败、历史前缀不匹配或发生回滚/改写，则自动回退完整重建。
   - `persistence.rs` 的 runtime checkpoint 与 tail update 写入路径已切换到 sync 策略。
14. 更新 `crates/vulcan-context/src/runtime.rs`：
   - 修复并行工具调用批次投影时只保留首个工具结果计数的问题。
   - 批次关闭阶段改为按 `tool_batch_id` 聚合所有工具调用项的结果计数，保证 Anthropic 批次拓扑可正确还原。
15. 完成计划文件闭环：
   - 已在计划文件末尾追加「执行变更总结」。
   - 已将计划文件移动到 `docs/completed/20260609/01-RUNTIME_JSONL_PROVIDER_ADAPTER_REARCHITECTURE.md`。

## 已执行验证

1. `cargo check -p vulcan-context`：通过。
2. `cargo check -p vulcan-workspace-store`：通过。
3. `cargo check -p vulcan-agent-runtime`：通过。
4. `cargo test -p vulcan-workspace-store runtime_log_ -- --nocapture`：通过。
5. `cargo test -p vulcan-agent-runtime provider_transport_turn_request -- --nocapture`：通过。
6. `cargo test -p vulcan-agent-runtime provider_transport_anthropic_http -- --nocapture`：通过。
7. `cargo test -p vulcan-workspace-store runtime_log_ -- --nocapture`：新增损坏恢复与备份测试后通过。
8. `cargo check -p vulcan-workspace-store`：通过。
9. `cargo check -p vulcan-code-gui`：通过。
10. `cargo check -p vulcan-code-tauri`：通过。
11. `cargo test -p vulcan-context-injection`：通过。
12. `cargo check -p vulcan-agent-runtime`：通过。
13. `cargo test -p vulcan-workspace-store runtime_log_ -- --nocapture`：新增追加/重建同步策略测试后通过。
14. `cargo check -p vulcan-workspace-store`：通过。
15. `cargo test -p vulcan-context`：修复并行工具批次计数后通过。
16. `cargo test -p vulcan-workspace-store runtime_log_ -- --nocapture`：runtime 投影联动复测通过。
17. `cargo check -p vulcan-code-gui`：runtime 投影联动后复测通过。
18. `cargo check -p vulcan-code-tauri`：runtime 投影联动后复测通过。
19. `cargo check -p vulcan-agent-runtime`：最终审核复测通过。
20. `cargo check -p vulcan-workspace-store`：最终审核复测通过。
21. `git diff --check`：无空白错误；仅输出当前工作区 CRLF 提示。

## 下一步

1. 已完成全部计划执行、验证和计划文件闭环。
