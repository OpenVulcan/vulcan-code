# 文件覆盖台账

## 使用规则

每轮审核必须覆盖所有“本任务修改”文件，并记录源码检查、测试覆盖和结果。新增文件应在实现阶段即时补入；其他会话的文件不得因本任务审核被顺带修改。

| 文件 | 归属 | 关注点 | 实现检查 | 测试检查 | 审核轮次 |
| --- | --- | --- | --- | --- | --- |
| `crates/vulcan-host-tools/Cargo.toml` | 本任务 | 规范解析器依赖 | 预审已核对为本地工作区依赖 | 7 crate `cargo check --locked` 通过 | 0/5 |
| `crates/vulcan-host-tools/src/file_runtime.rs` | 本任务 | 生命周期形状、提交前校验、事务边界 | 预审完成合同与副作用顺序检查 | 完整库 105 项通过 | 0/5 |
| `crates/vulcan-core/src/types.rs` | 与其他会话重叠，仅本任务枚举变体 | 强类型致命持久化错误 | 预审已核对精确枚举边界 | Core 完整库 43 项通过 | 0/5 |
| `crates/vulcan-core/src/controller/tool_execution.rs` | 本任务 | 单工具传播、并发兄弟汇合 | 预审完成全部致命分支检查 | Core 完整库 43 项通过 | 0/5 |
| `crates/vulcan-core/src/controller/tests/tool_batches.rs` | 本任务 | 请求次数、批次准备、兄弟完成 | 预审核对精确断言 | 新增用例及完整库通过 | 0/5 |
| `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge.rs` | 本任务 | 带原因拒绝、超时与注册表错误 | 预审核对所有屏障结果 | 屏障定向测试通过 | 0/5 |
| `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge/tests.rs` | 本任务 | ToolFinished 原始错误穿透 | 预审核对真实事件顺序 | 精确拒绝原因测试通过 | 0/5 |
| `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_stream_executor.rs` | 本任务 | 屏障错误到 Core 映射 | 预审核对错误分类 | 工具桥与 Core 回归通过 | 0/5 |
| `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_retry.rs` | 与其他会话重叠，仅本任务匹配臂和测试 | 禁止透明重试 | 预审已核对本任务边界 | 精确非重试测试通过；全量 844 项通过、3 项范围外失败 | 0/5 |
| `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_stream_runtime.rs` | 本任务 | 非 Provider 结构化失败分类 | 预审核对报告透传 | Agent 编译与定向测试通过 | 0/5 |
| `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_execution.rs` | 本任务 | 子任务屏障错误字符串边界 | 预审核对错误透传 | Agent 编译与定向测试通过 | 0/5 |
| `crates/vulcan-agent-runtime/src/tooling/native/tests/file_tools.rs` | 本任务 | 真实公开工具结果贯穿 V1 | 预审核对真实宿主载荷 | 定向贯穿测试通过 | 0/5 |
| `crates/vulcan-session-core/src/v1/document/recovery.rs` | 本任务 | 原子失败终态、幂等、关闭 Turn | 预审核对单 revision 事务 | Session Core 完整库 284 项通过 | 0/5 |
| `crates/vulcan-session-core/src/v1/document/document_tests/api.rs` | 本任务 | ApiStats、DisplayNote、Continue、新根 | 预审核对活动 API 与终态后失败 | 新增用例及完整库通过 | 0/5 |
| `crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs` | 本任务 | create+hunks 拒绝、兄弟终态、终态失败事实 | 预审核对可靠提交与实时投影顺序 | 相关精确测试通过 | 0/5 |
| `crates/vulcan-service-kernel/src/agent_stream/stream_task.rs` | 本任务 | 首错、迟到准备、兄弟收尾、失败持久化重试 | 预审核对 checkpoint 门禁 | 相关精确测试通过 | 0/5 |
| `crates/vulcan-service-kernel/src/agent_stream/stream_task/finished.rs` | 本任务 | API 终态是否足以覆盖最终报告 | 预审核对终态判定真值表 | 精确判定测试通过 | 0/5 |
| `crates/vulcan-service-kernel/src/agent_stream/detached_subagent.rs` | 本任务 | 分离子任务原始拒绝原因 | 预审核对失败旁路 | 7 crate `cargo check --locked` 通过 | 0/5 |
| `crates/vulcan-service-kernel/src/context_compaction/streaming.rs` | 本任务 | 压缩流先拒绝屏障再处理附加错误 | 预审核对返回顺序 | 7 crate `cargo check --locked` 通过 | 0/5 |
| `crates/vulcan-service-kernel/src/tests/session_kernel_tests.rs` | 本任务 | 活动 API、终态后失败与首次 flush 重试 | 预审核对持久化和重载断言 | 相关精确测试通过 | 0/5 |
| `docs/file-tool-changeset-and-failure-lifecycle.md` | 本任务 | 稳定跨层契约 | 预审已对照当前源码同步 | 文档人工核对 | 0/5 |
| `docs/plan/20260910-04-FILE_CHANGESET_V1_FAILURE_RECOVERY.md` | 本任务 | 实施计划与执行总结 | 预审已核对实施边界 | 不适用 | 0/5 |
| `docs/reviews/20260910-file-changeset-v1-failure-recovery/AUDIT_REPORT.md` | 本任务 | 五轮审核与范围外证据 | 预审重置历史已记录 | 不适用 | 0/5 |
| `docs/reviews/20260910-file-changeset-v1-failure-recovery/FILE_COVERAGE.md` | 本任务 | 逐文件覆盖记录 | 正式计数前已归零 | 不适用 | 0/5 |
