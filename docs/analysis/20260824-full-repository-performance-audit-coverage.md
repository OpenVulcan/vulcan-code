# Vulcan Code 全代码仓库审核覆盖台账

> 状态：审核完成  
> 审核日期：2026-08-24

## 1. 覆盖规则

- `S0-清单确认`：确认路径、语言、规模与归属。
- `S1-结构审核`：完成结构读取和规则扫描。
- `S2-实现审核`：检查关键实现、调用关系和数据流。
- `S3-验证完成`：候选问题已确认或排除。

## 2. 当前统计

| 项目 | 数量 |
| --- | ---: |
| Git 跟踪文件 | 2,313 |
| 纳入审核源文件 | 1,460 |
| 已完成 S0 | 0 |
| 已完成 S1 | 1,408 |
| 已完成 S2 | 3 |
| 已完成 S3 | 49 |

## 3. 分批台账

完整逐文件清单按批次记录具体文件、覆盖等级、审核方法、关联问题和验证边界。

### B0：仓库清单与结构地图

- 状态：完成
- 已完成：Git 跟踪清单、语言统计、全局 AST 映射、PowerShell 语法解析、样式/入口构建检查和逐文件归类。
- 覆盖构成：原始可执行代码 1,416 个，另补充纳入 43 个 CSS 与 1 个生产 HTML 入口，共 1,460 个源文件。


## 4. 逐文件清单

> 下列清单是最终审核覆盖。所有文件已至少达到 `S1`；`S2/S3` 仅表示该文件中的候选路径达到相应深度，不代表文件每一行均做了运行时基准。

### B1

| 状态 | 文件 | 语言 | 行数 | 关联记录 |
| --- | --- | --- | ---: | --- |
| S1 | `apps/vulcan-code-tauri/build.rs` | rs | 16 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-tauri/src/desktop_shell.rs` | rs | 325 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-tauri/src/main.rs` | rs | 19 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-ai-contract/src/account_usage.rs` | rs | 159 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-ai-contract/src/cache.rs` | rs | 160 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-ai-contract/src/capability.rs` | rs | 309 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-ai-contract/src/content.rs` | rs | 118 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-ai-contract/src/continuation.rs` | rs | 37 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-ai-contract/src/error.rs` | rs | 198 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-ai-contract/src/event.rs` | rs | 620 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-ai-contract/src/inference.rs` | rs | 1456 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-ai-contract/src/lib.rs` | rs | 34 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-ai-contract/src/model_parameter.rs` | rs | 1075 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-ai-contract/src/operation.rs` | rs | 105 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-ai-contract/src/replay.rs` | rs | 339 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-ai-contract/src/request.rs` | rs | 174 | P-003 大型请求重复克隆的载荷边界已确认 |
| S1 | `crates/vulcan-ai-contract/src/resource.rs` | rs | 133 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-ai-contract/src/tool.rs` | rs | 97 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-ai-contract/src/usage.rs` | rs | 502 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-change-set/src/lib.rs` | rs | 622 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-change-set/src/tests.rs` | rs | 210 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-change-set/src/types.rs` | rs | 334 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-change-set/src/verification.rs` | rs | 205 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-core/src/controller.rs` | rs | 127 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-core/src/controller/completion.rs` | rs | 89 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-core/src/controller/lifecycle.rs` | rs | 178 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-core/src/controller/metadata.rs` | rs | 220 | 已核对附件元数据序列化；Clippy 确认 Value clone 冗余，见 P-009 |
| S1 | `crates/vulcan-core/src/controller/provider_turns.rs` | rs | 81 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-core/src/controller/run_loop.rs` | rs | 573 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-core/src/controller/tests.rs` | rs | 649 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-core/src/controller/tests/interventions.rs` | rs | 496 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-core/src/controller/tests/loop_flow.rs` | rs | 426 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-core/src/controller/tests/streaming.rs` | rs | 270 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-core/src/controller/tests/tool_batches.rs` | rs | 635 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-core/src/controller/tool_execution.rs` | rs | 278 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-core/src/controller/transcript.rs` | rs | 526 | 已核对用户回合写入；Clippy 确认附件 Vec clone 冗余，见 P-009 |
| S1 | `crates/vulcan-core/src/lib.rs` | rs | 7 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-core/src/provider.rs` | rs | 70 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-core/src/tool.rs` | rs | 91 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-core/src/types.rs` | rs | 1441 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-main-session/src/lib.rs` | rs | 74 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-main-session/src/projection.rs` | rs | 20 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-main-session/src/router.rs` | rs | 13 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-main-session/src/runtime_policy.rs` | rs | 21 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-main-session/src/store.rs` | rs | 17 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-main-session/src/task_entry.rs` | rs | 72 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-process-lifecycle/src/lib.rs` | rs | 8 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-process-lifecycle/src/windows_process_job.rs` | rs | 103 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/cache/invalidation.rs` | rs | 53 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/cache/keys.rs` | rs | 16 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/cache/memory.rs` | rs | 95 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/cache/mod.rs` | rs | 15 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/cache/report.rs` | rs | 17 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/composer/diagnostics.rs` | rs | 66 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/composer/engine.rs` | rs | 292 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/composer/mod.rs` | rs | 18 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/composer/request.rs` | rs | 51 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/composer/resources.rs` | rs | 13 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/composer/response.rs` | rs | 69 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/error.rs` | rs | 35 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/ids.rs` | rs | 7 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/lib.rs` | rs | 77 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/node/mod.rs` | rs | 275 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/output_style.rs` | rs | 307 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/personality/catalog.rs` | rs | 168 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/personality/defaults.rs` | rs | 13 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/personality/mod.rs` | rs | 9 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/session/baseline.rs` | rs | 28 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/session/compaction.rs` | rs | 184 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/session/delta_state.rs` | rs | 43 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/session/fingerprint.rs` | rs | 19 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/session/mod.rs` | rs | 20 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/source/hashing.rs` | rs | 10 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/source/loader.rs` | rs | 101 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/source/mod.rs` | rs | 21 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/source/project_prompt.rs` | rs | 222 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/source/project_rules.rs` | rs | 186 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/source/snapshot.rs` | rs | 245 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/source/version.rs` | rs | 24 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/strategy/compaction.rs` | rs | 23 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/strategy/mod.rs` | rs | 66 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/strategy/ordering.rs` | rs | 13 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/strategy/primary.rs` | rs | 34 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/strategy/subagent_clone.rs` | rs | 34 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/strategy/subagent_functional.rs` | rs | 35 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/strategy/traits.rs` | rs | 15 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/tests/mod.rs` | rs | 727 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/tool_help/builtin.rs` | rs | 52 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/tool_help/bundle.rs` | rs | 115 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/tool_help/mod.rs` | rs | 12 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/tool_help/registry.rs` | rs | 140 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-composer/src/types.rs` | rs | 186 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-skills/src/assets/system/skill-creator/scripts/generate_vulcan_yaml.py` | py | 219 | AST 与 Python 语法扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-skills/src/assets/system/skill-creator/scripts/init_skill.py` | py | 406 | AST 与 Python 语法扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-skills/src/assets/system/skill-creator/scripts/quick_validate.py` | py | 101 | AST 与 Python 语法扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-skills/src/assets/system/skill-installer/scripts/github_utils.py` | py | 21 | AST 与 Python 语法扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-skills/src/assets/system/skill-installer/scripts/install-skill-from-github.py` | py | 313 | AST 与 Python 语法扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-skills/src/assets/system/skill-installer/scripts/install-skill-from-local.py` | py | 93 | AST 与 Python 语法扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-skills/src/assets/system/skill-installer/scripts/list-skills.py` | py | 111 | AST 与 Python 语法扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-skills/src/install.rs` | rs | 479 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-skills/src/lib.rs` | rs | 2747 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-prompt-skills/src/tests.rs` | rs | 1168 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-boundary/src/adapter.rs` | rs | 9 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-boundary/src/contract.rs` | rs | 192 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-boundary/src/lib.rs` | rs | 53 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-boundary/src/memory.rs` | rs | 144 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-boundary/src/runner.rs` | rs | 361 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-boundary/src/runtime.rs` | rs | 111 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-boundary/src/storage.rs` | rs | 39 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-boundary/src/subagent.rs` | rs | 262 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-boundary/src/vulcan_projection.rs` | rs | 494 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-core/src/child_agent_policy.rs` | rs | 223 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-core/src/compactor.rs` | rs | 80 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-core/src/config.rs` | rs | 457 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-core/src/context.rs` | rs | 96 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-core/src/event.rs` | rs | 86 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-core/src/executor.rs` | rs | 169 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-core/src/lib.rs` | rs | 152 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-core/src/model.rs` | rs | 126 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-core/src/policy.rs` | rs | 145 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-core/src/router.rs` | rs | 92 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-core/src/runtime_kernel.rs` | rs | 780 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-core/src/segmented_file_io.rs` | rs | 172 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-core/src/selector.rs` | rs | 66 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-core/src/store.rs` | rs | 74 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-core/src/subagent_registry.rs` | rs | 1959 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-session-core/src/turn_input_queue.rs` | rs | 526 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-subagent-core/src/lib.rs` | rs | 168 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/atomic_file.rs` | rs | 443 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/catalog.rs` | rs | 3639 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/catalog/capability_policy.rs` | rs | 192 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/catalog/directory_scan.rs` | rs | 201 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/catalog/path_rules.rs` | rs | 370 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/catalog/sidecar_protocol.rs` | rs | 682 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/compaction.rs` | rs | 136 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/config_resolver.rs` | rs | 386 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/lib.rs` | rs | 887 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/model.rs` | rs | 400 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/profile.rs` | rs | 73 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/router.rs` | rs | 74 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/scheduler.rs` | rs | 101 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/segmented_store.rs` | rs | 49 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/segmented_store/helpers.rs` | rs | 111 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/segmented_store/invocations.rs` | rs | 245 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/segmented_store/io.rs` | rs | 870 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/segmented_store/mirror.rs` | rs | 574 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/segmented_store/records.rs` | rs | 339 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/segmented_store/store_impl.rs` | rs | 409 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/segmented_store/tests.rs` | rs | 1883 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/segmented_store/traits.rs` | rs | 504 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-task-session/src/store.rs` | rs | 60 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-terminal-host-protocol/src/lib.rs` | rs | 632 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/activity.rs` | rs | 78 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/lib.rs` | rs | 98 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/project.rs` | rs | 123 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/runtime_state.rs` | rs | 92 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/runtime_state/execution_retry.rs` | rs | 605 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/runtime_state/internal_queue.rs` | rs | 253 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/runtime_state/media_references.rs` | rs | 493 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/runtime_state/panel_queue.rs` | rs | 323 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/runtime_state/planning.rs` | rs | 114 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/runtime_state/prompt_models.rs` | rs | 266 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/runtime_state/prompt_source.rs` | rs | 283 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/runtime_state/provider_usage.rs` | rs | 3056 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/runtime_state/session_entry.rs` | rs | 438 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/runtime_state/tests.rs` | rs | 693 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/runtime_state/vmm_state.rs` | rs | 105 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/session.rs` | rs | 287 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/session_addon.rs` | rs | 1235 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/session_addon/tests.rs` | rs | 208 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/subagent.rs` | rs | 41 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/subagent/metadata.rs` | rs | 1265 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/subagent/models.rs` | rs | 87 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/subagent/navigation.rs` | rs | 128 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/subagent/panel_state.rs` | rs | 301 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/subagent/parsing.rs` | rs | 67 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/subagent/store.rs` | rs | 80 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/subagent/tests.rs` | rs | 102 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/subagent/tests/metadata.rs` | rs | 1162 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/subagent/tests/navigation_store.rs` | rs | 157 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/subagent/tests/panel_state.rs` | rs | 438 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workbench-model/src/turn_stats.rs` | rs | 668 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |

### B2

| 状态 | 文件 | 语言 | 行数 | 关联记录 |
| --- | --- | --- | ---: | --- |
| S1 | `crates/vulcan-ai-protocols/src/anthropic.rs` | rs | 1864 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-ai-protocols/src/cohere.rs` | rs | 813 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-ai-protocols/src/gemini.rs` | rs | 1156 | P-002 包装协议重复解析已确认 |
| S1 | `crates/vulcan-ai-protocols/src/interactions.rs` | rs | 1185 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-ai-protocols/src/lib.rs` | rs | 243 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-ai-protocols/src/openai.rs` | rs | 2053 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-ai-protocols/src/openai_stream.rs` | rs | 3439 | P-002 协议解码重复解析已确认 |
| S1 | `crates/vulcan-ai-protocols/src/registry.rs` | rs | 237 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-ai-protocols/src/shared.rs` | rs | 243 | P-002 WebSocket Value 往返已确认 |
| S1 | `crates/vulcan-host-tools/src/command_session_runtime.rs` | rs | 929 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-host-tools/src/command_session_runtime/pipe.rs` | rs | 270 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-host-tools/src/command_session_runtime/pty.rs` | rs | 418 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-host-tools/src/command_session_runtime/runtime.rs` | rs | 687 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-host-tools/src/command_session_runtime/state.rs` | rs | 346 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-host-tools/src/command_session_runtime/tests.rs` | rs | 621 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-host-tools/src/file_runtime.rs` | rs | 3602 | 已核对 Clippy 性能提示与文件错误上下文；确认无效表达式和重复 collect，见 P-009 |
| S1 | `crates/vulcan-host-tools/src/lib.rs` | rs | 17 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-host-tools/src/output_decode.rs` | rs | 474 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-host-tools/src/process_creation.rs` | rs | 107 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-host-tools/src/project_temp_maintenance.rs` | rs | 48 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-host-tools/src/project_terminal_runtime.rs` | rs | 1813 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-host-tools/src/public_name.rs` | rs | 228 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-host-tools/src/result_format/mod.rs` | rs | 734 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-host-tools/src/result_format/templates.rs` | rs | 69 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-host-tools/src/shell_host.rs` | rs | 630 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-host-tools/src/terminal_host_client.rs` | rs | 530 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-llm-core/src/lib.rs` | rs | 537 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-llm-core/src/reasoning.rs` | rs | 41 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-llm-core/src/reasoning_bridge.rs` | rs | 135 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-llm-core/src/responses_to_anthropic.rs` | rs | 666 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-llm-core/src/responses_to_chat.rs` | rs | 496 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-llm-core/src/responses_to_gemini.rs` | rs | 875 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-llm-core/src/responses_websocket.rs` | rs | 659 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-llm-core/src/usage.rs` | rs | 2088 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/client.rs` | rs | 5695 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/common_model_capabilities.rs` | rs | 744 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/configuration.rs` | rs | 5539 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/connection_endpoint.rs` | rs | 296 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/connection_graph.rs` | rs | 363 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/credentials.rs` | rs | 850 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/custom_model.rs` | rs | 1357 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/custom_provider.rs` | rs | 1165 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/discovery.rs` | rs | 4887 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/discovery_signature.rs` | rs | 123 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-provider-adapters/src/execution.rs` | rs | 12701 | P-002 HTTP/WebSocket 流记录重复转换已确认 |
| S3 | `crates/vulcan-provider-adapters/src/graph.rs` | rs | 1887 | 已核对 CredentialAuthorization 变体；确认 296 字节枚举提示，当前低频配置用途 |
| S1 | `crates/vulcan-provider-adapters/src/lib.rs` | rs | 61 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/media.rs` | rs | 161 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/native_inference.rs` | rs | 1498 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/oauth.rs` | rs | 4061 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/provider_manifest/binding.rs` | rs | 692 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/provider_manifest/compiler.rs` | rs | 2689 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/provider_manifest/custom_projection.rs` | rs | 379 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/provider_manifest/inheritance.rs` | rs | 617 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/provider_manifest/matcher.rs` | rs | 158 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/provider_manifest/mod.rs` | rs | 321 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/provider_manifest/provenance.rs` | rs | 146 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/provider_manifest/schema.rs` | rs | 2118 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/provider_manifest/scope.rs` | rs | 317 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/provider_manifest/tests.rs` | rs | 2416 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/provider_manifest/validation.rs` | rs | 9464 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/provider_manifest_fixture.rs` | rs | 1740 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/provider_operational_telemetry.rs` | rs | 691 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/provider_task.rs` | rs | 2262 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/provider_tool_execution.rs` | rs | 4420 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/reasoning_catalog.rs` | rs | 387 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/registry.rs` | rs | 1035 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/runtime.rs` | rs | 20920 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/src/upload_engine.rs` | rs | 3318 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/tests/bundled_manifests.rs` | rs | 1827 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-adapters/tests/golden_snapshot.rs` | rs | 66 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-config/src/atomic_file.rs` | rs | 333 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-config/src/config.rs` | rs | 24 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-provider-config/src/config/configuration_io.rs` | rs | 1009 | 已核对配置路径 helper；确认两处 PathBuf 冗余复制，见 P-009 |
| S1 | `crates/vulcan-provider-config/src/config/git_commit.rs` | rs | 167 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-config/src/config/reasoning.rs` | rs | 107 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-config/src/config/retry.rs` | rs | 285 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-config/src/config/runtime_persistence.rs` | rs | 1028 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-config/src/config/tests/project_configuration.rs` | rs | 129 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-config/src/custom_manifest.rs` | rs | 2481 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-config/src/lib.rs` | rs | 16 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-config/src/manifest_store.rs` | rs | 3199 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-provider-config/src/manifest_sync.rs` | rs | 3305 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-tools-manager/src/lib.rs` | rs | 41 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-tools-manager/src/manager.rs` | rs | 107 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-tools-manager/src/public_names.rs` | rs | 199 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-tools-manager/src/request_streaming.rs` | rs | 376 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-tools-manager/src/schema_projection.rs` | rs | 1010 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-tools-manager/src/tests.rs` | rs | 668 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-tools-manager/src/types.rs` | rs | 258 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |

### B3

| 状态 | 文件 | 语言 | 行数 | 关联记录 |
| --- | --- | --- | ---: | --- |
| S1 | `crates/vulcan-agent-runtime/src/agent_profile.rs` | rs | 678 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/config.rs` | rs | 1017 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/context_injection_runtime.rs` | rs | 372 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/context_request_mode.rs` | rs | 55 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/conversation_hooks.rs` | rs | 107 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/conversation_hooks/completion.rs` | rs | 203 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/conversation_hooks/helpers.rs` | rs | 343 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/conversation_hooks/precheck.rs` | rs | 159 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/conversation_hooks/profile.rs` | rs | 230 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/conversation_hooks/runtime.rs` | rs | 666 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/conversation_hooks/tests.rs` | rs | 691 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/conversation_hooks/types.rs` | rs | 287 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/events.rs` | rs | 435 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/execution_models.rs` | rs | 1181 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/execution_preparation.rs` | rs | 651 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/execution_preparation/codebase_tool_routing_prompt.rs` | rs | 329 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/execution_preparation/tests.rs` | rs | 846 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/execution_preparation/tool_exposure.rs` | rs | 680 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/extension_generation.rs` | rs | 82 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/mod.rs` | rs | 91 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/prompt_compaction.rs` | rs | 1614 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/prompt_compaction/pruning.rs` | rs | 290 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/prompt_compaction/tests.rs` | rs | 676 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/prompt_composer_runtime.rs` | rs | 957 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider.rs` | rs | 479 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_attachment.rs` | rs | 165 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_continuation.rs` | rs | 48 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_prompt_loader.rs` | rs | 502 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_protocol_tool_definition.rs` | rs | 41 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_reasoning.rs` | rs | 484 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_request_bridge_conversation.rs` | rs | 87 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_request_bridge_messages.rs` | rs | 34 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_request_bridge_messages/conversions.rs` | rs | 152 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_request_bridge_messages/projection.rs` | rs | 728 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_request_bridge_messages/tests.rs` | rs | 860 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_request_bridge_records.rs` | rs | 195 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_request_bridge_tools.rs` | rs | 21 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_request_controller_config.rs` | rs | 30 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_request_preview.rs` | rs | 713 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_request_snapshot_builder.rs` | rs | 150 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_request_snapshot_preview.rs` | rs | 125 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_request_snapshot_support.rs` | rs | 30 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_request_telemetry.rs` | rs | 2735 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_runtime_context.rs` | rs | 1353 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_runtime_dispatch.rs` | rs | 1118 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_runtime_inputs.rs` | rs | 27 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_stream.rs` | rs | 189 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_stream_projection.rs` | rs | 194 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_system_update_lowering.rs` | rs | 259 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_tool_event_format.rs` | rs | 112 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport.rs` | rs | 4 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_entry_inputs.rs` | rs | 279 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_live_delta.rs` | rs | 9 | 已确认全部 Provider live delta 固定使用 UnboundedSender，见 P-012 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_request.rs` | rs | 264 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_request_budget.rs` | rs | 1433 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_retry.rs` | rs | 1444 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_stream_executor.rs` | rs | 252 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_stream_runtime.rs` | rs | 1637 | 已核对压缩/上下文超限边界；确认完整状态冗余 clone 与重复转换，见 P-009 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_tool_context.rs` | rs | 54 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_vulcan.rs` | rs | 4805 | 已核对正文、思考与工具片段逐事件发送点；确认无背压来源，见 P-012 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_visible_agent_messages.rs` | rs | 116 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/question_runtime.rs` | rs | 1251 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime.rs` | rs | 963 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime/tests.rs` | rs | 373 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_boundary_guard.rs` | rs | 500 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_request_capture.rs` | rs | 126 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream.rs` | rs | 309 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_bootstrap.rs` | rs | 167 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_bootstrap_core.rs` | rs | 378 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_bootstrap_resources.rs` | rs | 110 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_delta.rs` | rs | 1580 | 已核对特殊流事件入状态；确认拥有值再次 clone，见 P-009 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_driver.rs` | rs | 36 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_driver_loop.rs` | rs | 391 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_finalize.rs` | rs | 621 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_intervention.rs` | rs | 139 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_launch.rs` | rs | 210 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_launch_payload.rs` | rs | 852 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_preparation.rs` | rs | 142 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_preparation_failure.rs` | rs | 54 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_preparation_resources.rs` | rs | 85 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_setup.rs` | rs | 191 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/session_goal_prompt.rs` | rs | 140 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime.rs` | rs | 1178 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs` | rs | 1451 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/arguments.rs` | rs | 896 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/controller_config.rs` | rs | 125 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_methods.rs` | rs | 1320 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_results.rs` | rs | 321 | 已核对缓存任务双投影；确认多处 String 冗余 clone，见 P-009 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/lifecycle_methods.rs` | rs | 986 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/parent_events.rs` | rs | 463 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/persistence_methods.rs` | rs | 2837 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/request_records.rs` | rs | 705 | 已核对任务记录所有权；确认 snapshot/prompt 等冗余 clone 候选，见 P-009 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_execution.rs` | rs | 1489 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_launch.rs` | rs | 556 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_state.rs` | rs | 523 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/status_events.rs` | rs | 447 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests.rs` | rs | 224 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs` | rs | 3500 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/preparation_config.rs` | rs | 1787 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/registry_mailbox.rs` | rs | 1999 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/runtime_flow.rs` | rs | 3324 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/tool_args.rs` | rs | 422 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tool_surface.rs` | rs | 1458 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/tool_request_argument_assembler.rs` | rs | 177 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/tool_request_stream.rs` | rs | 1807 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/tool_request_stream/json_lexer.rs` | rs | 998 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge.rs` | rs | 779 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge/tests.rs` | rs | 1498 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/vmm_hooks.rs` | rs | 1752 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/agent_runtime/vulcan_plugin_hooks.rs` | rs | 326 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/attachment_asset_index.rs` | rs | 68 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/attachment_asset_index/cleanup.rs` | rs | 383 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/attachment_asset_index/lifecycle.rs` | rs | 602 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/attachment_asset_index/media_references.rs` | rs | 338 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/attachment_asset_index/replay.rs` | rs | 519 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/attachment_asset_index/storage.rs` | rs | 275 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/attachment_asset_index/tests.rs` | rs | 804 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/attachment_asset_index/timestamp.rs` | rs | 41 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/attachment_asset_index/types.rs` | rs | 310 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/attachment_assets.rs` | rs | 1811 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/context_budget.rs` | rs | 390 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/conversation_protocol.rs` | rs | 118 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/lib.rs` | rs | 61 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/luaskills_runtime.rs` | rs | 1249 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/luaskills_runtime/runlua.rs` | rs | 1635 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/luaskills_runtime/schema.rs` | rs | 226 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/luaskills_runtime/types.rs` | rs | 399 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/mcp/config.rs` | rs | 988 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/mcp/managed_codebase_memory.rs` | rs | 531 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/mcp/mod.rs` | rs | 32 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/mcp/plugin.rs` | rs | 543 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/mcp/registry.rs` | rs | 238 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/mcp/runtime.rs` | rs | 39 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/mcp/runtime/diagnostics.rs` | rs | 456 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/mcp/runtime/handle.rs` | rs | 2285 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/mcp/runtime/snapshot.rs` | rs | 148 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/mcp/runtime/tests.rs` | rs | 527 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/mcp/runtime/tools.rs` | rs | 742 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/media_understanding.rs` | rs | 1501 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/provider_input_normalizer.rs` | rs | 635 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/provider_tool_bridge.rs` | rs | 2012 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/session_event_runtime.rs` | rs | 85 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/session_event_runtime/checkpoint.rs` | rs | 163 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/session_event_runtime/event_projection.rs` | rs | 320 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/session_event_runtime/open_tool_records.rs` | rs | 229 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/session_event_runtime/streaming_records.rs` | rs | 258 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/session_event_runtime/terminal_report.rs` | rs | 668 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/session_event_runtime/tests.rs` | rs | 2327 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/session_event_runtime/tool_display_payloads.rs` | rs | 430 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/session_event_runtime/tool_event_records.rs` | rs | 850 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/session_event_runtime/user_prompt.rs` | rs | 103 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/skills/mod.rs` | rs | 9 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/skills/prompt.rs` | rs | 38 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/skills/session.rs` | rs | 335 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/bridge.rs` | rs | 465 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/bridge/descriptors.rs` | rs | 156 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/bridge/luaskills_dispatch.rs` | rs | 293 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/bridge/tests.rs` | rs | 337 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/mod.rs` | rs | 18 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native.rs` | rs | 434 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/file.rs` | rs | 155 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/file_edit.rs` | rs | 810 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/goal.rs` | rs | 707 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/knowledge_base.rs` | rs | 425 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/prompt_skill.rs` | rs | 110 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/registry.rs` | rs | 189 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/runtime_config.rs` | rs | 258 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/shell_command.rs` | rs | 1022 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/shell_session.rs` | rs | 693 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/submit_plan.rs` | rs | 1052 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/terminal.rs` | rs | 553 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/terminal/input.rs` | rs | 95 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/terminal/interaction.rs` | rs | 322 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/terminal/lifecycle.rs` | rs | 330 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/terminal/output.rs` | rs | 191 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/terminal/render.rs` | rs | 175 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/terminal/shell.rs` | rs | 146 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/terminal/wait.rs` | rs | 479 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/tests.rs` | rs | 171 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/tests/file_tools.rs` | rs | 357 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/tests/knowledge_base.rs` | rs | 88 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/tests/planning.rs` | rs | 479 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/tests/shell.rs` | rs | 643 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/tests/terminal.rs` | rs | 575 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/todolist.rs` | rs | 183 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/tooling/native/vmm.rs` | rs | 639 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/turn_stats.rs` | rs | 873 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/turn_stats/tests.rs` | rs | 1406 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/workbench_prompt_turn.rs` | rs | 1347 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/workbench_prompt_turn/tests.rs` | rs | 1062 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/workbench_prompt_turn/transcript.rs` | rs | 231 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-agent-runtime/src/workspace_path.rs` | rs | 117 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context/src/assembly.rs` | rs | 655 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context/src/budget.rs` | rs | 1012 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context/src/lib.rs` | rs | 1009 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context/src/protocol.rs` | rs | 33 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context/src/protocol/identity.rs` | rs | 368 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-context/src/protocol/projection.rs` | rs | 1203 | P-001 调用链与复杂度已确认 |
| S1 | `crates/vulcan-context/src/protocol/tests.rs` | rs | 905 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context/src/protocol/types.rs` | rs | 200 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context/src/request_mode.rs` | rs | 430 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context/src/runtime.rs` | rs | 28 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context/src/runtime/codec.rs` | rs | 125 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context/src/runtime/events.rs` | rs | 835 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context/src/runtime/identity.rs` | rs | 25 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context/src/runtime/projection.rs` | rs | 463 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context/src/runtime/projection_error.rs` | rs | 112 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context/src/runtime/replay.rs` | rs | 706 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context/src/runtime/replay_error.rs` | rs | 470 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context/src/runtime/tests.rs` | rs | 1888 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context/src/transcript.rs` | rs | 343 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context-compaction/src/lib.rs` | rs | 30 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context-compaction/src/planning.rs` | rs | 34 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context-compaction/src/planning/budget.rs` | rs | 328 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context-compaction/src/planning/models.rs` | rs | 301 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context-compaction/src/planning/plan.rs` | rs | 81 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context-compaction/src/planning/summary.rs` | rs | 500 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context-compaction/src/planning/tail.rs` | rs | 400 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context-compaction/src/planning/tests.rs` | rs | 1231 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context-compaction/src/planning/time.rs` | rs | 11 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context-compaction/src/preview.rs` | rs | 330 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-context-compaction/src/rewrite.rs` | rs | 2160 | 已核对压缩结果与保留集合；Clippy 确认两处冗余 clone，见 P-009 |
| S1 | `crates/vulcan-context-injection/src/context_injection/render.rs` | rs | 312 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context-injection/src/context_injection/state.rs` | rs | 452 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context-injection/src/context_injection/tests.rs` | rs | 342 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context-injection/src/context_injection/types.rs` | rs | 348 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context-injection/src/context_injection/util.rs` | rs | 34 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-context-injection/src/lib.rs` | rs | 25 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-file-watcher/src/adapters/mod.rs` | rs | 12 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-file-watcher/src/adapters/project_rules.rs` | rs | 109 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-file-watcher/src/adapters/prompt_resources.rs` | rs | 147 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-file-watcher/src/adapters/routing.rs` | rs | 71 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-file-watcher/src/event.rs` | rs | 34 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-file-watcher/src/fs/mod.rs` | rs | 6 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-file-watcher/src/fs/polling.rs` | rs | 176 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-file-watcher/src/lib.rs` | rs | 24 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S2 | `crates/vulcan-file-watcher/src/live.rs` | rs | 958 | 已核对 notify 回调与异步消费；发现无界原始事件队列，作为 P-012 次级建议待压力验证 |
| S1 | `crates/vulcan-file-watcher/src/live/tests.rs` | rs | 188 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-file-watcher/src/tests.rs` | rs | 206 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-file-watcher/src/traits.rs` | rs | 27 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-file-watcher/src/watcher.rs` | rs | 43 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-hello/src/lib.rs` | rs | 18 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-hooks/src/dispatch.rs` | rs | 350 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-hooks/src/error.rs` | rs | 106 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-hooks/src/events.rs` | rs | 136 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-hooks/src/handler.rs` | rs | 56 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-hooks/src/lib.rs` | rs | 34 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-hooks/src/registry.rs` | rs | 196 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-hooks/src/tests.rs` | rs | 475 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-hooks/src/types.rs` | rs | 615 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-luaskills-config/src/atomic_file.rs` | rs | 129 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-luaskills-config/src/lib.rs` | rs | 698 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-luaskills-prompt/src/lib.rs` | rs | 608 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-mcp-runtime/src/auth.rs` | rs | 370 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-mcp-runtime/src/error.rs` | rs | 45 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-mcp-runtime/src/http.rs` | rs | 328 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-mcp-runtime/src/http_support.rs` | rs | 74 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-mcp-runtime/src/launch.rs` | rs | 159 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-mcp-runtime/src/lib.rs` | rs | 35 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-mcp-runtime/src/rmcp_support.rs` | rs | 512 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-mcp-runtime/src/sse.rs` | rs | 902 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-mcp-runtime/src/stdio.rs` | rs | 582 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-mcp-runtime/src/transport.rs` | rs | 186 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-plugin-runtime/build.rs` | rs | 18 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-plugin-runtime/src/atomic_file.rs` | rs | 147 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-plugin-runtime/src/catalog.rs` | rs | 3824 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-plugin-runtime/src/host_services.rs` | rs | 217 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-plugin-runtime/src/lib.rs` | rs | 39 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-plugin-runtime/src/manifest.rs` | rs | 880 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-plugin-runtime/src/runtime.rs` | rs | 9716 | P-004 临时租约重复 I/O 与验证已确认 |
| S1 | `crates/vulcan-provider-manifest-cli/src/main.rs` | rs | 1117 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-runtime-assets/src/lib.rs` | rs | 719 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-runtime-assets/src/text_decode.rs` | rs | 108 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/attachments.rs` | rs | 194 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/command_events.rs` | rs | 423 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/conversation.rs` | rs | 1351 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/conversation_hydration.rs` | rs | 102 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/git_settings.rs` | rs | 63 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/host_tools.rs` | rs | 34 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/inference.rs` | rs | 889 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/knowledge_base.rs` | rs | 578 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/lib.rs` | rs | 513 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/media.rs` | rs | 348 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/planning_state.rs` | rs | 274 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/provider_retry.rs` | rs | 88 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/responses.rs` | rs | 374 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/settings.rs` | rs | 782 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/skills_catalog.rs` | rs | 1511 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/startup.rs` | rs | 130 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/subagents.rs` | rs | 143 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/vmm.rs` | rs | 1127 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/webui_rpc.rs` | rs | 307 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/webui_service.rs` | rs | 155 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/webui_static.rs` | rs | 264 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/workbench.rs` | rs | 243 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-api/src/workspace_context.rs` | rs | 598 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-host/src/lib.rs` | rs | 3086 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_skills_watcher.rs` | rs | 201 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream.rs` | rs | 24 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/checkpoints.rs` | rs | 381 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/detached_subagent.rs` | rs | 1027 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/mailbox.rs` | rs | 521 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/projection.rs` | rs | 4075 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/provider_telemetry.rs` | rs | 301 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/runtime_hook_dispatch.rs` | rs | 49 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/stream_task.rs` | rs | 1897 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/stream_task/content_delta.rs` | rs | 50 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/stream_task/conversation_checkpoint.rs` | rs | 2569 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/stream_task/finished.rs` | rs | 807 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/stream_task/launch_failure.rs` | rs | 396 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/stream_task/prepared.rs` | rs | 361 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/stream_task/provider_route_switch.rs` | rs | 297 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/stream_task/request_phase.rs` | rs | 205 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/stream_task/request_trace.rs` | rs | 282 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/stream_task/runtime_event.rs` | rs | 458 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/stream_task/stream_exit.rs` | rs | 190 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/agent_stream/workbench_sync.rs` | rs | 561 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/ai_credential_secret_store.rs` | rs | 558 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/attachments.rs` | rs | 39 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/attachments/capabilities.rs` | rs | 175 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/attachments/cleanup.rs` | rs | 438 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/attachments/conversion.rs` | rs | 529 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/attachments/staging.rs` | rs | 617 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/codebase_memory_controller.rs` | rs | 2563 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/command_sessions.rs` | rs | 644 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/configured_inference.rs` | rs | 7126 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-service-kernel/src/context_compaction.rs` | rs | 182 | P-001 输出窗口全量重建已确认 |
| S1 | `crates/vulcan-service-kernel/src/context_compaction/auto_compaction.rs` | rs | 904 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/context_compaction/budget.rs` | rs | 222 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/context_compaction/completion.rs` | rs | 816 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-service-kernel/src/context_compaction/draft.rs` | rs | 649 | P-001 逐增量克隆与合并已确认 |
| S1 | `crates/vulcan-service-kernel/src/context_compaction/messages.rs` | rs | 297 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/context_compaction/persist_display.rs` | rs | 293 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-service-kernel/src/context_compaction/streaming.rs` | rs | 455 | P-001 逐事件调用链已确认 |
| S1 | `crates/vulcan-service-kernel/src/context_compaction/tests.rs` | rs | 1285 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/conversation_actions.rs` | rs | 333 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/conversation_continuation.rs` | rs | 1492 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-service-kernel/src/conversation_display.rs` | rs | 673 | P-001 全量合并与线性查重已确认 |
| S1 | `crates/vulcan-service-kernel/src/conversation_history.rs` | rs | 1493 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/conversation_history/tool_detail.rs` | rs | 326 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/conversation_message_projection.rs` | rs | 259 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/conversation_run_services.rs` | rs | 1228 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/conversation_submission.rs` | rs | 5751 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/conversation_submission/activation.rs` | rs | 481 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/conversation_submission/edit.rs` | rs | 681 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/conversation_submission/preflight.rs` | rs | 625 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/conversation_submission/queue.rs` | rs | 482 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/conversation_submission/runtime_options.rs` | rs | 322 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/execution_report_memory.rs` | rs | 1499 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/fallback_workbench_projection.rs` | rs | 173 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/actions.rs` | rs | 219 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/command.rs` | rs | 196 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/commit_action.rs` | rs | 210 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/commit_summary.rs` | rs | 721 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/detail.rs` | rs | 213 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/diff.rs` | rs | 1141 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/diff_text.rs` | rs | 36 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/diff_text/hunk_scoping.rs` | rs | 140 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/diff_text/large_filter.rs` | rs | 96 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/diff_text/line_counts.rs` | rs | 447 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/diff_text/pathspec.rs` | rs | 35 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/diff_text/stats.rs` | rs | 118 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/diff_text/summary_parser.rs` | rs | 315 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/mod.rs` | rs | 43 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/model.rs` | rs | 636 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/repository.rs` | rs | 351 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/search.rs` | rs | 140 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/search/cache.rs` | rs | 344 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/search/index.rs` | rs | 422 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/search/input.rs` | rs | 66 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/search/tests.rs` | rs | 120 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/selection.rs` | rs | 193 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/settings.rs` | rs | 229 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git/snapshot.rs` | rs | 138 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git_diff_services.rs` | rs | 625 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/git_watcher.rs` | rs | 274 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/host_tools_settings.rs` | rs | 42 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/inference_access.rs` | rs | 604 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/inference_service.rs` | rs | 2258 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/knowledge_base_services.rs` | rs | 4023 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/lib.rs` | rs | 670 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/managed_memory_mesh_maintenance.rs` | rs | 787 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/managed_memory_mesh_runtime.rs` | rs | 2905 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/mcp_settings.rs` | rs | 410 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/media_upload.rs` | rs | 2765 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/output_style.rs` | rs | 160 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/pending_prompt_services.rs` | rs | 631 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/pending_prompts.rs` | rs | 1076 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/plugin_mcp_secret_store.rs` | rs | 130 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/plugin_services.rs` | rs | 353 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/plugin_storage.rs` | rs | 1382 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/plugin_ui_events.rs` | rs | 57 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/plugin_vldb.rs` | rs | 361 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/project_conversation_services.rs` | rs | 918 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/project_conversation_services/conversation_services.rs` | rs | 928 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/project_conversation_services/project_services.rs` | rs | 1179 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/project_directories.rs` | rs | 554 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/project_terminals.rs` | rs | 1063 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/provider_manifest_sync.rs` | rs | 9042 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/provider_settings.rs` | rs | 369 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/provider_usage_migration.rs` | rs | 734 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/provider_usage_runtime.rs` | rs | 1765 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/public_ai_settings.rs` | rs | 16111 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/rpc_dispatch.rs` | rs | 251 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/rpc_dispatch/configuration.rs` | rs | 529 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/rpc_dispatch/conversation_runtime.rs` | rs | 414 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/rpc_dispatch/extensibility.rs` | rs | 437 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/rpc_dispatch/knowledge_base.rs` | rs | 213 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/rpc_dispatch/project_conversation.rs` | rs | 151 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-service-kernel/src/rpc_dispatch/workspace_io.rs` | rs | 151 | 已核对工作区文件 RPC 调用链；确认同步遍历直接位于异步 dispatch 路径 |
| S1 | `crates/vulcan-service-kernel/src/runtime_summary_projection.rs` | rs | 181 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/runtime_workspace_helpers.rs` | rs | 1576 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/service_bootstrap.rs` | rs | 2008 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/service_bootstrap/error.rs` | rs | 64 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/service_bootstrap/recovery.rs` | rs | 201 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/service_bootstrap/startup_snapshot.rs` | rs | 74 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/service_bootstrap/startup_updates.rs` | rs | 243 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/service_bridge_io.rs` | rs | 942 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/service_diagnostics_paths.rs` | rs | 136 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/service_state_access.rs` | rs | 1033 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/session_goal_lifecycle.rs` | rs | 45 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/session_goal_lifecycle/accounting.rs` | rs | 273 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/session_goal_lifecycle/budget_guidance.rs` | rs | 158 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/session_goal_lifecycle/prompt_context.rs` | rs | 370 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/session_goal_lifecycle/state_cache.rs` | rs | 202 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/session_goal_lifecycle/summary_parse.rs` | rs | 251 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/session_goal_lifecycle/validation.rs` | rs | 137 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/session_goals.rs` | rs | 509 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/session_hydration.rs` | rs | 41 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/session_hydration/context_marker.rs` | rs | 93 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/session_hydration/loading.rs` | rs | 1354 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/session_hydration/run_state_preservation.rs` | rs | 406 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/session_hydration/tests.rs` | rs | 250 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/session_plan_runtime.rs` | rs | 597 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/session_runtime_kernel.rs` | rs | 1526 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/settings_provider_projection.rs` | rs | 885 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/settings_runtime_projection.rs` | rs | 1318 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/skill_catalog_projection.rs` | rs | 1254 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/skills_services.rs` | rs | 4706 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/skills_services/vulcan_plugin_lifecycle.rs` | rs | 1215 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/state.rs` | rs | 1453 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/subagent_event_ingest.rs` | rs | 1032 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/subagent_panels.rs` | rs | 555 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs` | rs | 651 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/task_mirror_status_projection.rs` | rs | 687 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests.rs` | rs | 72 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/active_session_state_tests.rs` | rs | 958 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/conversation_selection_tests.rs` | rs | 1168 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/conversation_submission_activation_tests.rs` | rs | 313 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/conversation_submission_preflight_tests.rs` | rs | 1078 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/conversation_submission_queue_tests.rs` | rs | 382 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/conversation_tool_detail_tests.rs` | rs | 193 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/detached_subagent_tests.rs` | rs | 3771 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/execution_retry_tests.rs` | rs | 2786 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/goal_todolist_tests.rs` | rs | 1150 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S2 | `crates/vulcan-service-kernel/src/tests/hot_path_scan_guard_tests.rs` | rs | 842 | 已核对现有热路径护栏及 P-001 覆盖缺口 |
| S1 | `crates/vulcan-service-kernel/src/tests/mailbox_tests.rs` | rs | 2097 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/mcp_settings_tests.rs` | rs | 79 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/pending_question_projection_tests.rs` | rs | 397 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/prompt_runtime_state_tests.rs` | rs | 1470 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/rpc_diff_tests.rs` | rs | 667 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/session_kernel_tests.rs` | rs | 2789 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/settings_projection_tests.rs` | rs | 347 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/stop_task_mirror_tests.rs` | rs | 821 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/subagent_fixed_model_tests.rs` | rs | 272 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs` | rs | 2428 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/test_support.rs` | rs | 847 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/tests/thinking_completion_tests.rs` | rs | 232 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/vmm.rs` | rs | 48 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/vmm/config.rs` | rs | 900 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/vmm/external_health.rs` | rs | 159 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/vmm/health_monitor.rs` | rs | 187 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/vmm/management_http.rs` | rs | 896 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/vmm/memory_actions.rs` | rs | 1296 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/vmm/snapshot.rs` | rs | 1188 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/vmm/static_profile.rs` | rs | 176 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/vmm/tests.rs` | rs | 814 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/vmm_hooks.rs` | rs | 994 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/vmm_runtime_audit.rs` | rs | 526 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/vmm_session_controller.rs` | rs | 998 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/webui_prompt_context.rs` | rs | 434 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/webui_prompt_lifecycle.rs` | rs | 71 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/webui_prompt_lifecycle/pending_dispatch.rs` | rs | 493 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/webui_prompt_lifecycle/pending_prepare.rs` | rs | 597 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/webui_prompt_lifecycle/precheck_continue.rs` | rs | 1752 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/webui_turn_chaining.rs` | rs | 348 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/workbench_context_usage.rs` | rs | 596 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/workbench_model_routes.rs` | rs | 437 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/workbench_model_routes/openrouter_routes.rs` | rs | 154 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/workbench_refresh.rs` | rs | 1640 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/workbench_session_panels.rs` | rs | 1404 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/workbench_surface_projection.rs` | rs | 454 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-service-kernel/src/workspace_files.rs` | rs | 262 | 已核对列表枚举与 WalkBuilder 配置；确认同步文件遍历缺少阻塞边界，见 P-005 |
| S3 | `crates/vulcan-service-kernel/src/workspace_files/entry_summary.rs` | rs | 210 | 已核对条目摘要与预览后代深扫；确认逐条 canonicalize/metadata，见 P-005 |
| S1 | `crates/vulcan-service-kernel/src/workspace_files/file_kind.rs` | rs | 20 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/workspace_files/path_metadata.rs` | rs | 380 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-service-kernel/src/workspace_files/path_security.rs` | rs | 121 | 已核对安全边界；确认 Windows 根字符串与小写转换按条目重复，见 P-005 |
| S1 | `crates/vulcan-service-kernel/src/workspace_files/read_response.rs` | rs | 166 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/workspace_files/tests.rs` | rs | 378 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/workspace_persistence_versions.rs` | rs | 1511 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-kernel/src/workspace_session_persistence.rs` | rs | 2650 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-runtime/build.rs` | rs | 191 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-runtime/src/lib.rs` | rs | 10 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-runtime/src/luaskills_bridge.rs` | rs | 1686 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-runtime/src/managed_runtime_bootstrap.rs` | rs | 1038 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-runtime/src/managed_runtime_installer.rs` | rs | 1629 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-runtime/src/mcp_bridge.rs` | rs | 503 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-service-runtime/src/vmm_management.rs` | rs | 837 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-terminal-host/src/main.rs` | rs | 758 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-terminal-host/src/process_job.rs` | rs | 243 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-terminal-host/tests/protocol_e2e.rs` | rs | 268 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-tools-luaskills-call/src/lib.rs` | rs | 352 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-vmm/build.rs` | rs | 15 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-vmm/src/adapter.rs` | rs | 829 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-vmm/src/client.rs` | rs | 191 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-vmm/src/config.rs` | rs | 1088 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-vmm/src/lib.rs` | rs | 65 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-vmm/src/managed.rs` | rs | 674 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-vmm/src/protocol.rs` | rs | 33 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-vmm/src/protocol/management.rs` | rs | 365 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-vmm/src/protocol/memory.rs` | rs | 502 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-vmm/src/protocol/profile.rs` | rs | 267 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-vmm/src/session_state.rs` | rs | 3127 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-vmm/src/transport.rs` | rs | 938 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-vmm/src/transport/responses.rs` | rs | 464 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-vmm/tests/managed_contract_fixture.rs` | rs | 19 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-workspace-store/src/data_plane.rs` | rs | 1146 | 已核对应用根路径 helper；确认 PathBuf 冗余复制，见 P-009 |
| S1 | `crates/vulcan-workspace-store/src/knowledge_base/markdown.rs` | rs | 393 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/knowledge_base/mod.rs` | rs | 29 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/knowledge_base/models.rs` | rs | 1741 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs` | rs | 10518 | 已核对导入启动、扫描、处理、暂存与提交链；确认同步处理和整库聚合，见 P-008 |
| S2 | `crates/vulcan-workspace-store/src/knowledge_base/schema.rs` | rs | 220 | 已核对知识库 staging 表结构与导入写入归属，关联 P-008 |
| S1 | `crates/vulcan-workspace-store/src/lib.rs` | rs | 24 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/provider_usage.rs` | rs | 254 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/ai_credentials.rs` | rs | 483 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/boundary_store.rs` | rs | 176 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/execution_retry_store.rs` | rs | 1777 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/git_preferences.rs` | rs | 104 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/goal_store.rs` | rs | 1230 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/loading.rs` | rs | 1521 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/mod.rs` | rs | 877 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/models.rs` | rs | 241 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/persistence.rs` | rs | 75 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/persistence/edit_fork.rs` | rs | 1587 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/persistence/index.rs` | rs | 873 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/persistence/locators.rs` | rs | 75 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/persistence/projects.rs` | rs | 1153 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/persistence/runtime_tail.rs` | rs | 248 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/persistence/session_lifecycle.rs` | rs | 688 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/persistence/snapshot.rs` | rs | 506 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/persistence/tests.rs` | rs | 172 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/provider_usage.rs` | rs | 2484 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/queries.rs` | rs | 117 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/runtime_logs.rs` | rs | 604 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/runtime_state_codec.rs` | rs | 130 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store.rs` | rs | 318 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/message_events.rs` | rs | 1196 | 已核对增量尾部与游标写入链；避免重读非活动分段，未确认问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/message_mapping.rs` | rs | 294 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/record_types.rs` | rs | 298 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/runtime_backup.rs` | rs | 588 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/runtime_io.rs` | rs | 872 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/runtime_log.rs` | rs | 1805 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/segment_io.rs` | rs | 1925 | 已核对分段读写和驻留游标；活动尾部有界，未确认问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/segment_types.rs` | rs | 381 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/session_fork.rs` | rs | 537 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/session_management.rs` | rs | 450 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/session_reading.rs` | rs | 1141 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/state_documents.rs` | rs | 354 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/task_types.rs` | rs | 249 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S3 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/append_events.rs` | rs | 978 | 分段追加测试已核对；覆盖非活动分段不重读等关键不变量 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/atomic_io.rs` | rs | 150 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/metadata_archive.rs` | rs | 719 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/mod.rs` | rs | 235 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/reverse_replay.rs` | rs | 313 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/rollback_updates.rs` | rs | 840 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/runtime_rebuild.rs` | rs | 943 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/runtime_sync.rs` | rs | 1920 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/session_lifecycle.rs` | rs | 870 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/window_and_state.rs` | rs | 363 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/vmm_state.rs` | rs | 723 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/serialization.rs` | rs | 295 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/session_addon_store.rs` | rs | 96 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/session_load.rs` | rs | 90 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/session_load/hydration.rs` | rs | 943 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/session_load/locator.rs` | rs | 53 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/session_load/pages.rs` | rs | 128 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/session_load/runtime_state.rs` | rs | 327 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/session_load/tests.rs` | rs | 819 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/session_load/tool_detail_hydration.rs` | rs | 486 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/session_load/types.rs` | rs | 40 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/session_load/windows.rs` | rs | 401 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `crates/vulcan-workspace-store/src/workspace_store/tests.rs` | rs | 1780 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |

### B4

| 状态 | 文件 | 语言 | 行数 | 关联记录 |
| --- | --- | --- | ---: | --- |
| S1 | `apps/vulcan-code-web/scripts/benchmark-streaming-markdown.test.ts` | ts | 115 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S3 | `apps/vulcan-code-web/scripts/check-build-budget.mjs` | mjs | 118 | 已核对测量实现并在全新生产构建后复现 CSS 预算失败，见 P-006 |
| S3 | `apps/vulcan-code-web/scripts/check-build-budget.test.mjs` | mjs | 24 | 架构测试批次通过；预算比较行为已验证 |
| S3 | `apps/vulcan-code-web/scripts/check-ui-architecture.mjs` | mjs | 86 | AST 与脚本测试已核对；当前架构规则检查通过 |
| S3 | `apps/vulcan-code-web/scripts/check-ui-architecture.test.mjs` | mjs | 47 | Node 测试 3 项通过 |
| S3 | `apps/vulcan-code-web/scripts/generate-file-icon-collection.mjs` | mjs | 52 | AST 已核对；生产构建成功生成 45 个图标并通过类型构建 |
| S1 | `apps/vulcan-code-web/src/app/BootStartupScreen.tsx` | tsx | 210 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/app/WebuiApp.tsx` | tsx | 176 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S3 | `apps/vulcan-code-web/src/app/WorkbenchApp.tsx` | tsx | 732 | 已核对 delta flush 与 React 投影旁路；纯正文增量不会重建 Workbench，已排除候选 |
| S1 | `apps/vulcan-code-web/src/components/ask/AskQuestionDialog.tsx` | tsx | 935 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-attachment-controls.tsx` | tsx | 512 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-attachment-media.test.ts` | ts | 42 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-attachment-media.ts` | ts | 86 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-attachments.ts` | ts | 295 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-capability-controls.test.ts` | ts | 376 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-capability-controls.tsx` | tsx | 1616 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-controls.tsx` | tsx | 17 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-control-types.ts` | ts | 25 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-dock.tsx` | tsx | 336 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-knowledge-control.test.ts` | ts | 90 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-knowledge-control.tsx` | tsx | 272 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-media-upload-context.ts` | ts | 18 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-model.ts` | ts | 259 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel.tsx` | tsx | 810 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/attachment-preview-strip.tsx` | tsx | 67 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/goal-dialog.tsx` | tsx | 84 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/goal-strip.tsx` | tsx | 168 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/goal-token-format.test.ts` | ts | 31 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/goal-token-format.ts` | ts | 68 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/helpers.test.ts` | ts | 100 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/helpers.ts` | ts | 294 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/model-secondary-route-control.tsx` | tsx | 252 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/progress-capsule.tsx` | tsx | 96 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/toolbar-right.test.tsx` | tsx | 142 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/toolbar-right.tsx` | tsx | 126 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-attachment-preparation.test.ts` | ts | 153 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-attachment-preparation.ts` | ts | 827 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-draft-session-settings.test.ts` | ts | 104 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-draft-session-settings.ts` | ts | 271 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-goal-controller.ts` | ts | 251 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-model-selection.test.ts` | ts | 44 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-model-selection.ts` | ts | 431 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-plan-mode.ts` | ts | 182 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-send-controller.ts` | ts | 309 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-session-draft.ts` | ts | 241 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-slash-commands.ts` | ts | 224 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-persistence-queue.test.ts` | ts | 114 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-persistence-queue.ts` | ts | 76 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-prompt-controls.tsx` | tsx | 263 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/composer-sortable.ts` | ts | 24 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/model-picker-control.test.ts` | ts | 239 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/composer/model-picker-control.tsx` | tsx | 779 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S3 | `apps/vulcan-code-web/src/components/conversation/chat-area.tsx` | tsx | 617 | 已核对隐藏思考投影；确认渲染期 JSON 身份稳定器，见 P-007 |
| S1 | `apps/vulcan-code-web/src/components/conversation/chat-area/composer-dock-layer.tsx` | tsx | 126 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/chat-area/conversation-scroll-controller.test.ts` | ts | 45 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/chat-area/conversation-scroll-controller.ts` | ts | 694 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/chat-area/plan-actions.ts` | ts | 102 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/chat-area/plan-artifact.ts` | ts | 60 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/chat-area/surface-layout-memory.ts` | ts | 122 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/chat-area/types.ts` | ts | 198 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/chat-area/user-prompt-edit-dialog.tsx` | tsx | 70 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/chat-area/user-prompt-edit-state.ts` | ts | 118 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/chat-area/visual-state.ts` | ts | 324 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/chat-area-scroll.test.ts` | ts | 204 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/chat-area-scroll.ts` | ts | 247 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/continuation-blocked-reason.test.ts` | ts | 150 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/continuation-blocked-reason.ts` | ts | 86 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/continuation-claim-reconciliation.test.ts` | ts | 170 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/continuation-claim-reconciliation.ts` | ts | 120 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-block-renderer.tsx` | tsx | 421 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-blocks.tsx` | tsx | 771 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-change-set-diff-preview.tsx` | tsx | 60 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-chrome.tsx` | tsx | 95 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-classifiers.ts` | ts | 79 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-compaction-progress.test.ts` | ts | 43 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-continuation-architecture.test.ts` | ts | 76 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-continue-control.test.tsx` | tsx | 43 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-continue-control.tsx` | tsx | 88 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-detail.tsx` | tsx | 487 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-diagram-frame.tsx` | tsx | 112 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-file-type-icon.tsx` | tsx | 129 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-generated-media.ts` | ts | 95 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-layout-events.ts` | ts | 67 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-layout-restore.ts` | ts | 62 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-markdown.test.tsx` | tsx | 34 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-markdown.tsx` | tsx | 764 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-measured-collapse.tsx` | tsx | 537 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-mermaid.tsx` | tsx | 196 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-metadata.ts` | ts | 62 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-model.ts` | ts | 196 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-plan-artifact.tsx` | tsx | 274 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-provider-retry-card.test.tsx` | tsx | 59 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-provider-retry-card.tsx` | tsx | 82 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-render-items.tsx` | tsx | 1176 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-runtime.test.ts` | ts | 96 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-runtime.ts` | ts | 96 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-scroll-architecture.test.ts` | ts | 97 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-svg-preview.tsx` | tsx | 103 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-system-error-card.test.tsx` | tsx | 135 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-system-error-card.tsx` | tsx | 402 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-terminal-model.ts` | ts | 125 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-terminal-output.tsx` | tsx | 327 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-timeline.test.ts` | ts | 198 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-timeline.ts` | ts | 548 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-timeline-projection-store.test.ts` | ts | 130 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-timeline-projection-store.ts` | ts | 296 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-tool-execution.tsx` | tsx | 309 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-tool-execution-media.ts` | ts | 97 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-tool-execution-model.test.ts` | ts | 179 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-tool-execution-model.ts` | ts | 1121 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-tool-execution-types.ts` | ts | 53 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-tool-metadata.ts` | ts | 395 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-tool-presentation-card.tsx` | tsx | 2657 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-tool-presentations.ts` | ts | 97 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-transcript.ts` | ts | 371 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-transcript-parser.test.ts` | ts | 93 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-transcript-parser.ts` | ts | 604 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-turn-seat.tsx` | tsx | 162 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-turn-stats-card.test.tsx` | tsx | 142 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/conversation-turn-stats-card.tsx` | tsx | 366 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S3 | `apps/vulcan-code-web/src/components/conversation/conversation-workspace.tsx` | tsx | 868 | 已核对 timeline/group/store 投影；确认两处 JSON 身份稳定器，见 P-007 |
| S1 | `apps/vulcan-code-web/src/components/conversation/session-todolist-panel.tsx` | tsx | 1578 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/session-todolist-panel/terminal-section.tsx` | tsx | 186 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/streaming-markdown-boundary.test.ts` | ts | 227 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/streaming-markdown-boundary.ts` | ts | 599 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/streaming-markdown-document.tsx` | tsx | 153 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/streaming-markdown-render-session.test.ts` | ts | 220 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S3 | `apps/vulcan-code-web/src/components/conversation/streaming-markdown-render-session.ts` | ts | 395 | 已核对增量 Markdown 尾部算法；尾部有界且仅处理新增片段，未确认问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/streaming-thinking-layout.test.ts` | ts | 45 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/streaming-thinking-layout.ts` | ts | 68 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/streaming-thinking-scroll.test.ts` | ts | 28 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/streaming-thinking-scroll.ts` | ts | 21 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/terminal/conversation-read-only-terminal-surface.test.ts` | ts | 45 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/terminal/conversation-read-only-terminal-surface.tsx` | tsx | 306 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/terminal-stop-confirm-dialog.tsx` | tsx | 72 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/thinking-display.ts` | ts | 161 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/thinking-render-path.test.ts` | ts | 45 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/thinking-render-path.ts` | ts | 30 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/tool-presentations/ask-question-presentation.ts` | ts | 206 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/tool-presentations/command-execution-presentation.ts` | ts | 656 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/tool-presentations/file-change-diff-presentation.ts` | ts | 239 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/tool-presentations/file-change-presentation.test.ts` | ts | 493 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/tool-presentations/file-change-presentation.ts` | ts | 1133 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/tool-presentations/lua-execution-presentation.ts` | ts | 202 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/tool-presentations/markdown-output-presentation.ts` | ts | 115 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/tool-presentations/presentation-value-readers.ts` | ts | 121 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/tool-presentations/provider-tool-presentation.test.ts` | ts | 58 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/tool-presentations/provider-tool-presentation.ts` | ts | 325 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/tool-presentations/subagent-orchestration-presentation.ts` | ts | 753 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/tool-presentations/types.ts` | ts | 600 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/transient-reasoning-status.test.ts` | ts | 46 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/conversation/use-streaming-thinking-viewport.ts` | ts | 165 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/dev/visual-lab-settings-area.tsx` | tsx | 629 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/agents/agent-area-header.ts` | ts | 23 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/agents/agent-area-types.ts` | ts | 24 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/agents/AgentsArea.tsx` | tsx | 2010 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/agent-skills/agent-skill-management-dialogs.tsx` | tsx | 571 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/agent-skills/AgentSkillsArea.tsx` | tsx | 618 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/agent-skills/agent-skills-types.ts` | ts | 52 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/feature-area-types.ts` | ts | 10 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/feature-shell.tsx` | tsx | 344 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/feature-shell-content.tsx` | tsx | 155 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/feature-shell-sidebar.tsx` | tsx | 545 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/featureText.ts` | ts | 5 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/knowledge/knowledge-base-area.test.ts` | ts | 52 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/knowledge/KnowledgeBaseArea.tsx` | tsx | 1209 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/knowledge/knowledge-import-progress.test.ts` | ts | 75 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/knowledge/KnowledgeLibraryCreateDialog.test.ts` | ts | 86 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/knowledge/KnowledgeLibraryCreateDialog.tsx` | tsx | 234 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/knowledge/knowledge-library-viewer.test.ts` | ts | 76 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/knowledge/KnowledgeLibraryViewerDialog.tsx` | tsx | 538 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/knowledge/KnowledgeOperationProgress.tsx` | tsx | 167 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/knowledge/knowledge-package-dialogs.test.ts` | ts | 47 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/knowledge/KnowledgePackageDialogs.tsx` | tsx | 322 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/memory/MemoryArea.test.ts` | ts | 80 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/memory/MemoryArea.tsx` | tsx | 429 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/memory/use-memory-area-controller.ts` | ts | 780 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/memory/vmm-advanced-page.tsx` | tsx | 403 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/memory/vmm-content-page.tsx` | tsx | 434 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/memory/vmm-data-page.tsx` | tsx | 1127 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/memory/vmm-identity-project-page.test.ts` | ts | 15 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/memory/vmm-identity-project-page.tsx` | tsx | 666 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/memory/vmm-inference-settings.test.ts` | ts | 102 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/memory/vmm-inference-settings.tsx` | tsx | 1113 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/memory/vmm-maintenance-dialog.tsx` | tsx | 278 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/memory/vmm-memory-sections.tsx` | tsx | 70 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/memory/vmm-overview-page.tsx` | tsx | 446 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/memory/vmm-profile-page.tsx` | tsx | 413 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/memory/vmm-project-settings-fields.tsx` | tsx | 75 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/memory/vmm-rules-page.tsx` | tsx | 274 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/memory/vmm-setup-dialog.tsx` | tsx | 539 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/context-basics-settings.tsx` | tsx | 321 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/credential-model-access-dialog.tsx` | tsx | 419 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/custom-model-template-card.tsx` | tsx | 208 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/custom-provider-change-summary-dialog.tsx` | tsx | 74 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/custom-provider-model-defaults-card.tsx` | tsx | 368 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/custom-provider-model-drafts.test.ts` | ts | 99 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/custom-provider-model-drafts.ts` | ts | 79 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/custom-provider-models-dialog.tsx` | tsx | 1880 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/custom-provider-models-state.test.ts` | ts | 271 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/custom-provider-models-state.ts` | ts | 236 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/custom-provider-protocol-drafts.test.ts` | ts | 378 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/custom-provider-protocol-drafts.ts` | ts | 461 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/custom-provider-save-state.test.ts` | ts | 101 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/custom-provider-save-state.ts` | ts | 28 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/direct-provider-model-settings.test.ts` | ts | 191 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/direct-provider-model-settings.ts` | ts | 104 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/mcp-server-dialog.tsx` | tsx | 287 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/mcp-settings-area.tsx` | tsx | 117 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/misc-settings-areas.tsx` | tsx | 543 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/notification-settings-area.tsx` | tsx | 210 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-icon.test.tsx` | tsx | 75 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-icon.tsx` | tsx | 246 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-list-settings.tsx` | tsx | 1294 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-management-settings.tsx` | tsx | 1956 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-manifest-status.test.ts` | ts | 184 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-manifest-status.tsx` | tsx | 669 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-model-filter.test.ts` | ts | 28 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-model-filter.ts` | ts | 46 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-model-modalities.test.ts` | ts | 70 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-model-modalities.ts` | ts | 60 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-model-modality-summary.tsx` | tsx | 177 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-model-routes.test.ts` | ts | 277 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-model-routes.ts` | ts | 143 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-model-settings.test.ts` | ts | 87 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-model-settings.tsx` | tsx | 577 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-ordering.test.ts` | ts | 74 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-ordering.ts` | ts | 105 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-presentation.test.ts` | ts | 80 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-presentation.ts` | ts | 156 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-retry-settings.tsx` | tsx | 333 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-settings-utils.ts` | ts | 60 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-tool-selection-settings.test.ts` | ts | 78 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-tool-selection-settings.tsx` | tsx | 915 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/provider-tool-test-dialog.tsx` | tsx | 904 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/public-ai-internal-overrides-dialog.tsx` | tsx | 756 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/public-ai-model-capability-dialog.test.ts` | ts | 90 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/public-ai-model-capability-dialog.tsx` | tsx | 793 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/public-ai-model-overrides-dialog.tsx` | tsx | 540 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/settings-action-types.ts` | ts | 163 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/settings-diagnostics-area.test.tsx` | tsx | 50 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/settings-diagnostics-area.tsx` | tsx | 126 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/SettingsFeatureContent.tsx` | tsx | 127 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/settings-navigation.test.ts` | ts | 44 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/settings-navigation.tsx` | tsx | 165 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/settings/settings-runtime-panels.tsx` | tsx | 29 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/skills/lua-skill-card.test.ts` | ts | 38 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/skills/lua-skill-card.tsx` | tsx | 364 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/skills/lua-skill-config-dialog.test.ts` | ts | 114 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/skills/lua-skill-config-dialog.tsx` | tsx | 730 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/skills/lua-skills-panel.tsx` | tsx | 323 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S3 | `apps/vulcan-code-web/src/components/features/skills/lua-skills-settings-panel.tsx` | tsx | 876 | 已核对设置克隆、相等性与校验；记录低频 JSON 往返，前端约束校验已排除为必要 UX |
| S1 | `apps/vulcan-code-web/src/components/features/skills/skill-area-header.ts` | ts | 58 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/skills/skill-area-types.ts` | ts | 221 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/skills/SkillsArea.tsx` | tsx | 59 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/skills/vulcan-plugins-panel.tsx` | tsx | 745 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/features/source-origin-link.tsx` | tsx | 107 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/media/generated-media.tsx` | tsx | 182 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/media/generated-media-player.tsx` | tsx | 489 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/media/generated-media-types.ts` | ts | 57 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/media/generated-media-utils.tsx` | tsx | 107 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/plugins/dynamic-plugin-mount-panel.tsx` | tsx | 311 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/plugins/plugin-host-components.tsx` | tsx | 701 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/product/usage/index.ts` | ts | 21 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/product/usage/usage-panel.tsx` | tsx | 297 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/folder-picker-dialog.tsx` | tsx | 281 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-action-dialogs.tsx` | tsx | 180 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-custom-icon.tsx` | tsx | 60 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-icon-catalog-loader.ts` | ts | 31 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-icon-catalog-lucide.ts` | ts | 37 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-icon-catalog-material.ts` | ts | 20 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-icon-catalog-skill.ts` | ts | 20 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-icon-contract.test.ts` | ts | 51 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-icon-contract.ts` | ts | 124 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-icon-dialog.tsx` | tsx | 369 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-icon-model.tsx` | tsx | 255 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-icons.tsx` | tsx | 31 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-knowledge-bases-section.tsx` | tsx | 189 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-settings-dialog.tsx` | tsx | 890 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-settings-sections.tsx` | tsx | 631 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-settings-types.ts` | ts | 120 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-settings-utils.tsx` | tsx | 153 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/ProjectSidebar.test.tsx` | tsx | 54 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/ProjectSidebar.tsx` | tsx | 503 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-sidebar-conversation-row.tsx` | tsx | 245 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-sidebar-menus.tsx` | tsx | 449 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-sidebar-project-row.tsx` | tsx | 199 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-sidebar-sortable.ts` | ts | 24 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-sidebar-time.ts` | ts | 93 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/project-sidebar-types.ts` | ts | 14 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/terminal/project-floating-terminal-layer.test.ts` | ts | 379 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/terminal/project-floating-terminal-layer.tsx` | tsx | 1375 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/project/terminal/project-terminal-input-buffer.ts` | ts | 105 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/public-ai/credential-usage-overview.test.ts` | ts | 174 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/public-ai/credential-usage-overview.tsx` | tsx | 707 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/shared/model-builtin-capabilities.test.ts` | ts | 25 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/shared/model-builtin-capabilities.tsx` | tsx | 144 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/shared/status.tsx` | tsx | 65 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/shell/app-chrome.tsx` | tsx | 436 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/shell/startup-download-progress.tsx` | tsx | 100 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/shell/startup-logo-mark.tsx` | tsx | 153 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/shell/startup-screen.tsx` | tsx | 118 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/sidebar/sidebar-profile-menus.tsx` | tsx | 134 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/terminal/terminal-display-listener.ts` | ts | 26 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/terminal/terminal-viewport.tsx` | tsx | 66 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/terminal/terminal-viewport-runtime.tsx` | tsx | 196 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/terminal/use-terminal-display-connection-epoch.ts` | ts | 59 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/accordion.tsx` | tsx | 87 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/alert.tsx` | tsx | 88 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/alert-dialog.tsx` | tsx | 238 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/badge.tsx` | tsx | 57 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/button.test.tsx` | tsx | 30 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/button.tsx` | tsx | 99 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/card.tsx` | tsx | 124 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/checkbox.tsx` | tsx | 35 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/choice-controls.tsx` | tsx | 225 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/combobox.tsx` | tsx | 133 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/command.tsx` | tsx | 213 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/context-menu.tsx` | tsx | 296 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/dialog.test.ts` | ts | 28 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/dialog.tsx` | tsx | 217 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/dropdown-menu.test.tsx` | tsx | 92 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/dropdown-menu.tsx` | tsx | 338 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/empty.tsx` | tsx | 127 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/field.tsx` | tsx | 258 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/floating-surface.test.ts` | ts | 30 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/floating-surface.ts` | ts | 55 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/form-controls.test.tsx` | tsx | 58 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/hover-card.tsx` | tsx | 62 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/input.tsx` | tsx | 44 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/input-group.tsx` | tsx | 170 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/label.tsx` | tsx | 24 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/navigation-controls.test.tsx` | tsx | 43 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/navigation-item.tsx` | tsx | 79 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/popover.tsx` | tsx | 115 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/progress.tsx` | tsx | 31 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/progress-ring.tsx` | tsx | 78 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/radio-group.tsx` | tsx | 48 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/scroll-area.tsx` | tsx | 53 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/select.tsx` | tsx | 214 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/separator.tsx` | tsx | 30 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/sheet.tsx` | tsx | 164 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/sidebar.tsx` | tsx | 283 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/sonner.tsx` | tsx | 26 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/spinner.tsx` | tsx | 16 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/surface.test.tsx` | tsx | 37 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/surface.tsx` | tsx | 60 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/switch.tsx` | tsx | 31 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/table.tsx` | tsx | 130 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/tabs.tsx` | tsx | 100 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/textarea.tsx` | tsx | 43 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/ui/tooltip.tsx` | tsx | 113 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/adaptive-tool-panel.tsx` | tsx | 169 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/draft-submission-state.test.ts` | ts | 83 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/draft-submission-state.ts` | ts | 84 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/media-upload-progress-panel.tsx` | tsx | 62 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/media-upload-progress-state.test.ts` | ts | 70 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/media-upload-progress-state.ts` | ts | 45 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/project-floating-layer.test.ts` | ts | 157 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/project-floating-layer.tsx` | tsx | 941 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workbench-dialogs.tsx` | tsx | 271 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx` | tsx | 2231 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workbench-layout/conversation-actions.ts` | ts | 969 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workbench-layout/conversation-hydration.ts` | ts | 338 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workbench-layout/empty-workbench-route.test.ts` | ts | 67 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workbench-layout/empty-workbench-route.ts` | ts | 56 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workbench-layout/project-actions.tsx` | tsx | 500 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workbench-layout/session-actions.ts` | ts | 271 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workbench-layout/shell-chrome.test.ts` | ts | 15 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workbench-layout/shell-chrome.tsx` | tsx | 176 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workbench-layout/types.ts` | ts | 92 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workbench-layout/use-stop-conversation-run-decision.ts` | ts | 120 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/WorkspaceContextPanel.tsx` | tsx | 1222 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/WorkspaceFilesPanelHost.tsx` | tsx | 1020 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workspace-files-panel-request-ownership.ts` | ts | 167 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workspace-files-preview.tsx` | tsx | 378 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/WorkspaceFileTreeDrawer.tsx` | tsx | 547 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workspace-input-owner.test.ts` | ts | 30 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workspace-input-owner.ts` | ts | 79 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/WorkspaceMemoryPanel.tsx` | tsx | 945 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workspace-memory-panel-request-ownership.ts` | ts | 193 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workspace-memory-settings.tsx` | tsx | 125 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workspace-page-descriptor.ts` | ts | 66 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workspace-resizable-body.tsx` | tsx | 600 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/WorkspaceReviewPanelHost.tsx` | tsx | 975 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workspace-review-panel-request-ownership.ts` | ts | 239 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workspace-shell.tsx` | tsx | 1606 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workspace-shell/context-tabs.ts` | ts | 292 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workspace-shell/git-summary.ts` | ts | 140 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workspace-shell/types.ts` | ts | 384 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workspace-skills-settings.tsx` | tsx | 191 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workspace-title-menu.tsx` | tsx | 145 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/components/workspace/workspace-tool-panel-menu.tsx` | tsx | 75 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-file-card.tsx` | tsx | 984 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-file-card/load-state.tsx` | tsx | 399 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-highlight.ts` | ts | 944 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-highlight-worker.ts` | ts | 341 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-parse-worker.ts` | ts | 50 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-parse-worker-client.ts` | ts | 231 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-render-html-core.ts` | ts | 897 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-render-hunk-controls.ts` | ts | 149 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-render-index.ts` | ts | 458 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-render-inline-html.ts` | ts | 202 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-render-worker.ts` | ts | 448 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-render-worker-client.ts` | ts | 11 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-render-worker-client/cache-keys.ts` | ts | 86 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-render-worker-client/constants.ts` | ts | 27 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-render-worker-client/fingerprints.ts` | ts | 223 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-render-worker-client/highlighted-spans.ts` | ts | 283 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-render-worker-client/hooks.ts` | ts | 544 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-render-worker-client/search-payload.ts` | ts | 126 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-render-worker-client/types.ts` | ts | 296 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-render-worker-client/worker-runtime.ts` | ts | 224 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/DiffReviewPanel.tsx` | tsx | 1 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/DiffReviewPanel/file-tree.ts` | ts | 170 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/DiffReviewPanel/floating-git-actions.tsx` | tsx | 128 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/DiffReviewPanel/panel.tsx` | tsx | 663 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/DiffReviewPanel/preferences.ts` | ts | 99 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/DiffReviewPanel/search.ts` | ts | 733 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/DiffReviewPanel/types.ts` | ts | 5 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-review-toolbar.tsx` | tsx | 698 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-search.ts` | ts | 614 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-search-worker.ts` | ts | 185 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-search-worker-client.ts` | ts | 395 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-shiki-shared.ts` | ts | 343 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-shiki-worker-loaders.ts` | ts | 74 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/diff-virtual-layout.ts` | ts | 658 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/limits.ts` | ts | 170 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/parseDiff.ts` | ts | 864 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/target-diff-dom.ts` | ts | 743 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/target-diff-model.ts` | ts | 863 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/target-diff-model/builder.ts` | ts | 177 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/target-diff-model/types.ts` | ts | 266 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/types.ts` | ts | 685 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/diff/workspace-review-model.ts` | ts | 777 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/generated/file-icon-collection.ts` | ts | 223 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/i18n.tsx` | tsx | 90 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/i18n-messages.ts` | ts | 8596 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/lib/provider-tool-capabilities.ts` | ts | 41 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/lib/utils.ts` | ts | 6 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/main.tsx` | tsx | 10 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/notifications/coordinator.test.ts` | ts | 99 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/notifications/coordinator.ts` | ts | 185 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/notifications/endpoints/local-shell-endpoint.test.ts` | ts | 55 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/notifications/endpoints/local-shell-endpoint.ts` | ts | 87 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/notifications/intent.test.ts` | ts | 39 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/notifications/intent.ts` | ts | 50 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/notifications/policy.test.ts` | ts | 156 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/notifications/policy.ts` | ts | 80 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/notifications/preferences.test.ts` | ts | 54 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/notifications/preferences.ts` | ts | 89 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/notifications/presentation-context.ts` | ts | 29 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/notifications/test-support/notification-test-shell.ts` | ts | 109 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/notifications/types.ts` | ts | 177 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/resources/localMedia.ts` | ts | 72 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/browser-fallback-diff.ts` | ts | 550 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/browser-fallback-workbench.ts` | ts | 750 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/browser-service-discovery.ts` | ts | 355 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/browserTransport.test.ts` | ts | 107 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/browserTransport.ts` | ts | 458 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/client.ts` | ts | 53 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/codebase-memory-service.ts` | ts | 91 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/conversation-actions-service.ts` | ts | 732 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/conversation-tool-detail-service.ts` | ts | 79 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/index.ts` | ts | 23 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/knowledge-base-service.ts` | ts | 331 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/memory-actions-service.ts` | ts | 231 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/plugin-ui-events.ts` | ts | 111 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/plugin-ui-module-runtime.ts` | ts | 130 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/project-actions-service.ts` | ts | 230 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/protocol.ts` | ts | 105 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/settings-actions-service.ts` | ts | 806 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/skills-actions-service.ts` | ts | 808 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/terminal-service.ts` | ts | 141 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/webui-service-runtime.ts` | ts | 47 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/workbench-snapshot-service.test.ts` | ts | 79 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/service/workbench-snapshot-service.ts` | ts | 405 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/shell/browserShell.ts` | ts | 141 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/shell/client.ts` | ts | 10 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/shell/index.ts` | ts | 12 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/shell/protocol.ts` | ts | 115 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/shell/runtime.ts` | ts | 5 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/shell/tauriShell.ts` | ts | 197 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/state/conversation-live-stream-context.tsx` | tsx | 139 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/state/conversation-live-stream-store.test.ts` | ts | 530 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S3 | `apps/vulcan-code-web/src/state/conversation-live-stream-store.ts` | ts | 801 | 已核对正文增量存储；链式片段避免逐 delta 复制完整正文，未确认问题 |
| S1 | `apps/vulcan-code-web/src/state/workbench-message-merge.test.ts` | ts | 55 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S3 | `apps/vulcan-code-web/src/state/workbench-message-merge.ts` | ts | 704 | 已核对消息 upsert；单次扫描配合 Map，未确认重复全局转换 |
| S1 | `apps/vulcan-code-web/src/state/workbench-preview-actions.ts` | ts | 165 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/state/workbench-selectors.ts` | ts | 21 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/state/workbench-state.test.ts` | ts | 258 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/state/workbench-state.ts` | ts | 1549 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/state/workbench-stream-projection.test.ts` | ts | 193 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/state/workbench-stream-projection.ts` | ts | 57 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/types.ts` | ts | 13 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/types/composer-skills.ts` | ts | 1764 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/types/conversation.ts` | ts | 868 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/types/knowledge-base.ts` | ts | 624 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/types/memory-vmm.ts` | ts | 732 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/types/planning-questions.ts` | ts | 237 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/types/projects-files.ts` | ts | 297 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/types/public-ai.ts` | ts | 1815 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/types/runtime-context.ts` | ts | 256 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/types/service-events.ts` | ts | 13 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/types/settings-providers.ts` | ts | 375 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/types/startup.ts` | ts | 122 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/types/terminal.ts` | ts | 151 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/types/workbench.ts` | ts | 453 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/vite-env.d.ts` | ts | 8 | 全仓 AST、TypeScript 构建、Vitest 与跨仓渲染/转换规则扫描完成；未确认独立问题 |
| S3 | `apps/vulcan-code-web/vite.config.ts` | ts | 71 | 已核对构建配置并完成生产构建；构建成功，CSS 预算失败见 P-006 |

### B5

| 状态 | 文件 | 语言 | 行数 | 关联记录 |
| --- | --- | --- | ---: | --- |
| S1 | `make.ps1` | ps1 | 469 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S1 | `make.sh` | sh | 311 | AST 与脚本流程规则扫描完成；本机无 Bash 运行时 |
| S1 | `plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua` | lua | 438 | AST 与 Plugin Runtime 测试链扫描完成；本机无独立 Lua 解释器 |
| S1 | `plugins/context7/runtime/plugin.bundle.lua` | lua | 1870 | AST 与 Plugin Runtime 测试链扫描完成；本机无独立 Lua 解释器 |
| S1 | `plugins/context7/ui/index.mjs` | mjs | 522 | AST、Node 语法与脚本规则扫描完成；未确认独立问题 |
| S3 | `plugins/vulcan-debug-runtime/node/sidecar.mjs` | mjs | 649 | AST、语法与协议缓冲链已核对；确认 DAP 分片二次复制，见 P-011 |
| S3 | `plugins/vulcan-debug-runtime/node/sidecar.test.mjs` | mjs | 521 | Node 测试 4 项通过；确认缺少大正文碎片化覆盖，见 P-011 |
| S1 | `plugins/vulcan-debug-runtime/runtime/plugin.bundle.lua` | lua | 82 | AST 与 Plugin Runtime 测试链扫描完成；本机无独立 Lua 解释器 |
| S1 | `plugins/vulcan-rtk/runtime/plugin.bundle.lua` | lua | 612 | AST 与 Plugin Runtime 测试链扫描完成；本机无独立 Lua 解释器 |
| S1 | `plugins/vulcan-rtk/ui/index.mjs` | mjs | 625 | AST、Node 语法与脚本规则扫描完成；未确认独立问题 |
| S1 | `plugins/vulcan-usage-statistics/pricing/tools/convert-cc-switch-pricing.mjs` | mjs | 451 | AST、Node 语法与脚本规则扫描完成；未确认独立问题 |
| S1 | `plugins/vulcan-usage-statistics/runtime/plugin.bundle.lua` | lua | 934 | AST 与 Plugin Runtime 测试链扫描完成；本机无独立 Lua 解释器 |
| S1 | `plugins/vulcan-usage-statistics/ui/index.mjs` | mjs | 949 | AST、Node 语法与脚本规则扫描完成；未确认独立问题 |
| S1 | `scripts/audit_cliproxy_protocol_changes.ps1` | ps1 | 177 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S1 | `scripts/build_tauri.ps1` | ps1 | 237 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S1 | `scripts/build_tauri.sh` | sh | 143 | AST 与脚本流程规则扫描完成；本机无 Bash 运行时 |
| S1 | `scripts/check_provider_manifest_release.ps1` | ps1 | 291 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S1 | `scripts/check_rust_dependency_policy.ps1` | ps1 | 331 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S1 | `scripts/configure_controller_profile.ps1` | ps1 | 100 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S1 | `scripts/configure_controller_profile.sh` | sh | 118 | AST 与脚本流程规则扫描完成；本机无 Bash 运行时 |
| S1 | `scripts/generate_rust_third_party_notices.ps1` | ps1 | 309 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S1 | `scripts/install_bundled_tools.ps1` | ps1 | 531 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S1 | `scripts/install_bundled_tools.sh` | sh | 714 | AST 与脚本流程规则扫描完成；本机无 Bash 运行时 |
| S1 | `scripts/install_codebase_memory_runtime.ps1` | ps1 | 319 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S1 | `scripts/install_host_deps.ps1` | ps1 | 504 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S1 | `scripts/install_lua_deps.ps1` | ps1 | 597 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S1 | `scripts/install_rust_dev_tools.ps1` | ps1 | 121 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S1 | `scripts/install_rust_dev_tools.sh` | sh | 140 | AST 与脚本流程规则扫描完成；本机无 Bash 运行时 |
| S1 | `scripts/package_codebase_memory_runtime.ps1` | ps1 | 142 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S1 | `scripts/package_vmm_runtime.ps1` | ps1 | 332 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S1 | `scripts/run_tauri.ps1` | ps1 | 92 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S1 | `scripts/run_tauri.sh` | sh | 81 | AST 与脚本流程规则扫描完成；本机无 Bash 运行时 |
| S1 | `scripts/sync_codebase_memory_runtime.ps1` | ps1 | 347 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S1 | `scripts/sync_common_model_capabilities.ps1` | ps1 | 128 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S3 | `scripts/sync_tauri_runtime.ps1` | ps1 | 401 | PowerShell 7 语法通过；已核对完整同步链和当前载荷规模，确认无变化快路径缺失，见 P-010 |
| S3 | `scripts/sync_tauri_runtime.sh` | sh | 333 | AST 已核对；Shell 对等实现同样无条件 cp 完整资源根，见 P-010 |
| S1 | `scripts/test_context7_plugin.lua` | lua | 594 | AST 与 Plugin Runtime 测试链扫描完成；本机无独立 Lua 解释器 |
| S1 | `scripts/test_vulcan_plugin_lua_modules.lua` | lua | 138 | AST 与 Plugin Runtime 测试链扫描完成；本机无独立 Lua 解释器 |
| S1 | `scripts/vite_dev_server.ps1` | ps1 | 402 | PowerShell 7 语法与脚本 I/O/流程规则扫描完成；未确认独立问题 |
| S1 | `scripts/vite_dev_server.sh` | sh | 327 | AST 与脚本流程规则扫描完成；本机无 Bash 运行时 |
| S1 | `workspace-hack/build.rs` | rs | 2 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |
| S1 | `workspace-hack/src/lib.rs` | rs | 1 | 全仓 AST、Clippy 生产构建及跨仓性能规则扫描完成；未确认独立问题 |

### B6：样式与入口源文件

| 状态 | 文件 | 语言 | 行数 | 关联记录 |
| --- | --- | --- | ---: | --- |
| S1 | `apps/vulcan-code-web/index.html` | html | 12 | 生产入口构建与资源引用检查完成；未确认独立问题 |
| S1 | `apps/vulcan-code-web/src/boot-styles.css` | css | 1260 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/components/shared/model-builtin-capabilities.css` | css | 61 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/diff/diff-review/file-list.css` | css | 338 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/diff/diff-review/file-tree.css` | css | 252 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/diff/diff-review/placeholders.css` | css | 93 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/diff/diff-review/plain-renderer-base.css` | css | 259 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/diff/diff-review/plain-renderer-lines.css` | css | 267 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/diff/diff-review/render-window.css` | css | 147 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/diff/diff-review/shell.css` | css | 122 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/diff/diff-review/toolbar.css` | css | 451 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/diff/diffReview.css` | css | 8 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles.css` | css | 380 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/composer-capabilities.css` | css | 292 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/composer-controls.css` | css | 535 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/composer-layout.css` | css | 599 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/composer-pending.css` | css | 449 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/composer-selectors.css` | css | 310 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/composer-textarea.css` | css | 612 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/context-injection.css` | css | 405 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/conversation.css` | css | 140 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/features.css` | css | 282 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/markdown.css` | css | 336 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/media.css` | css | 393 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/memory.css` | css | 39 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/plan-artifact.css` | css | 214 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/plugin-host-components.css` | css | 504 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/project-folder-picker.css` | css | 159 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/session-tools.css` | css | 752 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/settings.css` | css | 1495 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/shell.css` | css | 578 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/sidebar-base.css` | css | 511 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/sidebar-tree.css` | css | 631 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/skill-cards.css` | css | 2019 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/skill-management.css` | css | 623 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/special-blocks.css` | css | 1046 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/terminal.css` | css | 548 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/tool-presentations.css` | css | 735 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/visual-lab.css` | css | 203 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/workspace-context.css` | css | 416 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/workspace-files.css` | css | 213 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/workspace-header.css` | css | 150 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/styles/workspace-layout.css` | css | 246 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
| S1 | `apps/vulcan-code-web/src/ui/foundations/tokens.css` | css | 41 | CSS 导入归属、生产构建与预算规则扫描完成；关联 P-006 |
