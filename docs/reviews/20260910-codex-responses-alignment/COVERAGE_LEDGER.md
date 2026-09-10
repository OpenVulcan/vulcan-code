# Codex Responses 最新协议对齐覆盖台账

状态：实施与验证中

本台账只覆盖 `docs/plan/20260910-03-CODEX_RESPONSES_LATEST_ALIGNMENT_PLAN.md` 对应任务。VCE、Context Engine、许可证生成、文件工具及其他并行会话改动均不属于本任务；测试失败只有在能够沿本任务调用链归因时才进入修复范围。

## 一、范围与证据基线

| 项目 | 基线或边界 | 状态 |
| --- | --- | --- |
| 上游 Codex | `D:\openvulcan\third_git\codex`，提交 `ddea03ad049142943bdbf13e937b1d67e8c1ba0c` | 已冻结 |
| 本地仓库 | 当前脏工作区，保护其他并行会话改动 | 持续检查 |
| AgentSpace 现场 | 用户提供的脱敏追踪记录；不把测试密钥写入源码、文档或测试输出 | 已遵守 |
| 发布边界 | 不提交、不推送、不发布、不修改真实凭证或外部数据库 | 已遵守 |

## 二、功能覆盖矩阵

| 编号 | 对齐项 | 主要承接位置 | 验证证据 | 状态 |
| --- | --- | --- | --- | --- |
| C01 | `CodexModelBehavior` 封闭类型、继承、冻结与指纹 | provider manifest schema/binding/inheritance/selection/route | 定向协议测试、manifest 校验 | 已实现，待总验收 |
| C02 | Lite 请求体：稳定项目 ID、`additional_tools`、命名空间、推理上下文、并行门禁 | `request_encoding.rs` | `codex_lite_projection_matches_stable_prefix_and_namespace_contract` | 已实现，待总验收 |
| C03 | Lite 专用请求头及 HTTP/WS/compact 一致性 | `request_headers.rs`、传输测试 | 请求头定向测试 | 待实现 |
| C04 | JSON Schema 与 Rust 清单契约一致 | manifest v2 schema | schema/fixture/release 校验 | 已实现，待总验收 |
| C05 | 自由文本工具输入按工具声明解析，不统一伪装为补丁 | ai-contract tool、OpenAI stream decoder | JSON/freeform/apply_patch 单测 | 待实现 |
| C06 | `code_mode_only` 独立 JavaScript 宿主与权限边界 | engine primitive、runtime tool bridge | 宿主生命周期与拒绝路径测试 | 待决策性实现；不得虚报可用 |
| C07 | 消息 phase、`end_turn`、`configuration_update` 事件语义 | ai-contract event、OpenAI stream、runtime accumulator | 事件序列与持久化测试 | 待实现 |
| C08 | 私有上下文与历史版本门禁 | replay/history/session contract | 重放与旧客户端拒绝测试 | 待实现 |
| C09 | HTTP/WS 首包与后续轮次重建一致性 | request encoding、responses websocket | HTTP/WS 对照测试 | 部分已有，待补齐 |
| C10 | 远程压缩 V1 与 V2 `compaction_trigger` 分流 | responses compact、stream runtime | V1/V2 定向测试 | 待实现 |
| C11 | AgentSpace 专属 `reconciliation_required`：结果与用量待确认、禁止盲重试 | ai-contract error、streaming、retry、telemetry、WebUI | Rust 定向测试与前端全量测试 | 已实现，待清单声明与总验收 |
| C12 | Provider/模型诊断展示实际 Codex 行为与不可用原因 | public AI settings、WebUI | 投影与界面测试 | 待实现 |
| C13 | 凭证启用/停用等价于运行时不存在 | configuration/activation/selection/settings | 后端与前端定向测试 | 并行工作区已有实现，待归因验收 |
| C14 | 静态模型目录不显示且拒绝“同步模型” | discovery/settings/WebUI | 后端拒绝与前端隐藏测试 | 并行工作区已有实现，待归因验收 |
| C15 | AgentSpace 不因同名模型自动继承 OpenAI Lite/code-mode 行为 | AgentSpace manifest、路由冻结 | manifest 与路由测试 | 待显式验证 |
| C16 | 资源索引与打包资源同步 | bundled index、`make.ps1 sync` | release 检查 | 待执行 |

## 三、已取得验证证据

| 日期 | 命令或测试 | 结果 | 归因 |
| --- | --- | --- | --- |
| 2026-09-10 | `cargo check -p vulcan-ai-contract -p vulcan-agent-runtime` | 通过 | 本任务 |
| 2026-09-10 | `cargo test -p vulcan-agent-runtime reconciliation -- --test-threads=1` | 2 项通过 | 本任务 |
| 2026-09-10 | `cargo test -p vulcan-agent-runtime response_identity_survives_usage_update_and_failure -- --test-threads=1` | 1 项通过 | 本任务 |
| 2026-09-10 | `pnpm test -- conversation-system-error-card.test.tsx` | 180 个文件通过、1 个跳过；1101 项通过、1 项跳过 | 本任务；共享测试运行器执行了前端全量测试 |

## 四、连续零问题审核计数

只有完整覆盖本台账且没有发现本任务问题的一轮才计数。发现问题并修复后计数归零。

| 轮次 | 审核焦点 | 结论 | 计数 |
| --- | --- | --- | --- |
| 待开始 | 清单、协议、运行时、工具与媒体、状态与发布 | 尚未进入最终审核 | 0/5 |

## 五、排除项与异常记录

- 其他并行会话拥有的 VCE、Context Engine、许可证、文件工具及其测试，不在本任务修复范围。
- AgentSpace 尚无请求历史查询或幂等重放契约证据，因此本任务只实现“结果与用量待确认”，不宣称自动重试已解决。
- 未具备独立 JavaScript 宿主时，不得把 `code_mode_only` 路由标记为可执行；必须实现宿主或显式报告能力不可用。
