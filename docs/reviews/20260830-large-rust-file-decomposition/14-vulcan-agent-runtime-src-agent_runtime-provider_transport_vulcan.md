# 14：crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_vulcan.rs 拆分记录

## 基线

- 原始路径：`crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_vulcan.rs`。
- 原始行数：5557。
- 原始内容摘要（SHA-256）：`b7c12172c81404cf3993e9cba5e2fbc5f040c1aa63fe2146058f8dfc4d328b8d`。
- 基线提交：`111540689b777ddc1229a6f17293a42818f2155e`。
- 当前状态：已完成结构拆分、有效测试迁移、逐文件审核和复验；等待全局审核。

## 职责、类型与调用证据

本文件同时承担控制器适配器状态、规范请求构建、流式持久化屏障与分派、诊断追踪、响应累积／重放、输入资源预算和生成资源落盘。CodeKit 定位 102 个函数／方法节点；原 37 项测试基线全部通过。普通执行和捕获入口由 `provider_runtime_dispatch.rs` 调用；`prompt_compaction.rs` 复用消息投影、工具投影、原生工具和用量转换；传输重试运行时引用同账号 OAuth 刷新标识。

已阅读供应商接入流程与作者手册。此次只调整代码归属，不改变模型协议、凭据租约刷新边界、持久化屏障、清单资源或安全策略。

## 拆分决策与充分性

- 为什么需要拆分：5557 行并非单一适配器实现，输入媒体预算、生成输出、追踪持久化与回放累积各有独立类型和契约；共同堆积使请求／响应方向和副作用边界难以识别。
- 哪些部分不需要拆分及原因：唯一适配器状态和构造器保留根文件；流式实现中的准备、分派、传输观察和完成屏障保持同一个流程，不机械打散步骤；响应累积器及其重放项状态保持同模块。
- 预期模块与类型归属：`request`（请求和工具投影）、`streaming`（流执行）、`trace`（诊断）、`accumulator`（响应与用量）、`message_projection`（规范历史／重放及压缩来源）、`input_resources`（请求媒体预算与约束）、`generated_resources`（生成图片落盘）。原对外入口、类型和函数通过原路径保留；测试按被测模块就近组织。
- 当前拆分是否足够：足够。根文件 394 行，只保留唯一适配器、入口、构造及共享失败投影；7 个生产子模块为 232～862 行，分别持有完整职责。流式屏障顺序和累积器状态机保持完整，不继续按事件分支或固定行数切分。

## 测试保全与匹配

- 原始测试清单、数量、属性和夹具：37 项测试（含异步用例）均通过；记录型增量接收器与累积器夹具只服务响应测试，随同迁移。
- 测试迁移映射：保留 36 项有效测试，见 [逐项映射](14-TEST-MOVES.md)；生产归属、可见性和逐符号映射见 [生产迁移记录](14-PRODUCTION-MOVES.md)。
- 经用户授权移除 1 项无效测试：`provider_response_trace_body_uses_the_smaller_exact_encoding`（原 4062～4083 行）只调用 `#[cfg(test)] encode_provider_response_trace_body`（原 383～407 行）。全仓源码检索确认该函数没有生产调用；实际 `persist_provider_api_trace_snapshot` 原 892 行无条件写入 `body_base64`。旧测试只验证一份为测试而保留的 UTF-8／Base64 择小实现，与当前写入契约无关，因此同时移除测试与测试专用函数，不回退生产实现、不删除其他追踪覆盖。可从基线提交恢复。
- 测试发现与实际执行：拆分后 36 项真实发现全名与预期映射完全一致；36 项全部通过，0 失败、0 忽略。原 4 个异步测试属性保留。

## 实施与验证

原文件 5557→394 行，新增 7 个生产模块和 8 个就近测试文件；原生产状态、签名、协议和序列化字段保留。

| 生产模块 | 生产行数 | 对应测试行数 | 有效用例数 |
| --- | ---: | ---: | ---: |
| 根适配器／入口 | 394 | 65 | 3 |
| `request` | 353 | 74 | 2 |
| `streaming` | 553 | 21 | 1 |
| `trace` | 500 | 127 | 5 |
| `accumulator` | 862 | 669 | 10 |
| `message_projection` | 484 | 290 | 3 |
| `input_resources` | 613 | 314 | 9 |
| `generated_resources` | 232 | 105 | 3 |

实际验证均通过 `rtk` 执行：

- `cargo check -p vulcan-agent-runtime --all-targets`：通过。
- `cargo test -p vulcan-agent-runtime --lib provider_transport_vulcan:: -- --list`：36 项，全名映射无遗漏、无额外项。
- 相同过滤器使用 `--test-threads=1 --quiet`：36 项全部通过。
- `cargo test -p vulcan-agent-runtime --lib prompt_compaction:: -- --test-threads=1 --quiet`：18 项通过，覆盖消息／用量／工具投影消费者。
- `cargo test -p vulcan-agent-runtime --lib provider_transport_stream_runtime:: -- --test-threads=1 --quiet`：9 项通过，覆盖重试运行时消费者。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo clippy -p vulcan-agent-runtime --lib -- -D warnings`：通过。
- 本目标全部 16 个文件定向 `rustfmt --check --edition 2024 --config skip_children=true` 与 `git diff --check`：通过。

普通测试保留既有 Windows 链接器创建 `.lib/.exp` 的提示，未压制；它不属于测试失败。未调用真实凭据 Provider、未重启桌面应用；此次没有更改资源或协议，不把编译／单元回归结果当作真实网络验收。

## 拆分后审核与自动修复

1. **完整性**：原 113 个类型／函数／方法中，明确移除无效测试和测试专用实现各 1；其余 111 个逐符号词法等价，新增 0、行为差异 0。72 个有效生产符号完整保留。
2. **结构准确性**：根仍唯一持有适配器与尝试索引共享句柄；输入预算类型、累积器类型及其相关方法随职责归属，未引入循环兄弟依赖或重复状态。两个 trait 实现仍绑定原适配器。
3. **关键顺序**：准备许可、可靠起始、最新凭据读取、真实分派、传输观察持久化、响应累积与可靠终态的完整函数原样迁移；追踪排空不恢复语义投影。未改 401 恢复或协议选择规则。
4. **测试匹配**：有效测试按被测模块就近放置，夹具只迁移到其唯一消费者 `accumulator/tests.rs`；32 个普通测试、4 个异步测试均可发现并通过。未通过修改断言、增加忽略或扩大生产字段可见性适配测试。
5. **接口与路径**：原消费者导入路径保持；原代码没有因移动而失效的 `include!`／文件位置宏。序列化字段及字面量核对未变化。
6. **自动修复**：确认并移除 1 对脱离生产的旧测试／辅助实现，详见上文；拆分首次编译、测试及严格生产检查均通过，无新增行为问题需要修复。该逐文件闭环不计入全局轮次，全局仍为 0／5。
