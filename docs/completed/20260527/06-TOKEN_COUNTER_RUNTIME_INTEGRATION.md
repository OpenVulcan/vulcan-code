# Token 预算计算器运行时接入计划

## 任务目标

本轮目标是将已合入 workspace 的 `vulcan-token-counter` 从“独立库已存在”推进到“主运行时实际使用”。接入必须满足系统初始化阶段构建并持有预算计算器，后续上下文预算、请求预算和压缩判断只复用已初始化实例，不允许在热路径反复创建 tokenizer。资源下载脚本需要明确默认落点，并提供仓库根入口以方便维护。

## 详细执行步骤

1. 梳理并调整 tokenizer 资源落点
   - 将运行时使用的 tokenizer 资源统一放入 `crates/vulcan-runtime-assets/resources/tokenizers`。
   - 修改构建同步脚本，将 tokenizer 资源同步到 `output/resources/tokenizers`。
   - 在 `vulcan-runtime-assets` 中新增 tokenizer 资源目录解析函数。

2. 增加仓库根脚本入口
   - 保留 `crates/vulcan-token-counter/scripts/fetch_tokenizers.py` 作为真实实现。
   - 在根 `scripts/` 下新增薄包装脚本，默认把下载结果写入 runtime-assets 的 tokenizer 资源目录。
   - 避免复制下载与转换逻辑，降低后续维护风险。

3. 初始化并持有预算计算器
   - 在 GUI 运行时层新增预算计算器运行时包装，持有 `Arc<TokenCounter>` 与初始化诊断信息。
   - 在系统初始化时创建该包装，并在 `RuntimeFoundation` 刷新时复用。
   - 如果初始化失败，保留诊断并回退到旧启发式估算，避免运行时崩溃，同时让问题可见。

4. 接入上下文预算估算路径
   - 在 `vulcan-context` 增加可注入的 Token 估算器接口。
   - 将 `build_request_context_budget_snapshot` 和 `build_projected_session_context_budget_snapshot` 改为使用可选估算器。
   - GUI 侧通过已初始化预算计算器提供估算器，替代旧的纯字符启发式热路径。

5. 验证与归档
   - 运行格式化、相关 crate 检查与 token/context 关键测试。
   - 验证 tokenizer 资源路径解析、脚本入口与预算测试。
   - 追加执行变更总结，归档计划文件，并提交中文结构化 commit。

## 技术选型

- `vulcan-token-counter` 继续作为独立模型感知 Token 统计 crate。
- `vulcan-runtime-assets` 作为运行时资源统一来源，不让生产环境依赖 token-counter crate 源码目录。
- GUI 运行时通过 `Arc<TokenCounter>` 共享预算计算器，避免 tokenizer 在预算热路径重复构建。
- 根脚本采用薄包装模式，真实下载逻辑不迁移，避免脚本职责分裂。

## 验收标准

- `output/resources/tokenizers` 能由构建脚本从 runtime-assets 同步生成。
- 系统初始化阶段能构建预算计算器，运行时刷新不会重复创建。
- 请求预算和会话预算可使用已初始化的 token counter 估算文本 token。
- 根 `scripts/` 下存在可直接调用的 tokenizer 下载入口，并默认输出到 runtime-assets。
- 相关格式化、编译和关键测试通过。

## 执行变更总结

### 1. 核心修复与调整概述

- 已将 tokenizer 资源从 `vulcan-token-counter` 精确迁移到 `vulcan-runtime-assets`，避免生产运行时依赖计数库源码资源目录。
- 已保留 crate 内下载脚本作为真实实现，并新增根 `scripts/fetch_tokenizers.py` 薄包装入口，默认输出到 runtime-assets。
- 已新增 GUI 侧 `TokenBudgetRuntime`，在运行时基础架构初始化阶段预热并持有共享 `TokenCounter`，运行时刷新复用同一实例。
- 已在请求预算、终态请求快照回填和会话投影预算中接入可选 Token 估算器；初始化失败时自动回退旧启发式估算。
- 已将预算运行时初始化状态显示到运行时占位面板与设置页执行运行时卡片，便于资源路径诊断。

### 2. 📂文件变更清单（区分新增/修改/删除）

- 新增：`apps/vulcan-code-gui/src/agent_runtime/token_budget_runtime.rs`。
- 新增：`scripts/fetch_tokenizers.py`。
- 修改：`Cargo.toml`、`Cargo.lock`、`apps/vulcan-code-gui/Cargo.toml`。
- 修改：GUI 运行时启动、bootstrap、driver、finalize、compaction 与会话预算相关模块。
- 修改：`crates/vulcan-context/src/budget.rs` 与 `crates/vulcan-context/src/lib.rs`。
- 修改：`crates/vulcan-runtime-assets/src/lib.rs`。
- 修改：`crates/vulcan-token-counter/src/lib.rs`、`README.md` 与下载脚本帮助文案。
- 修改：`scripts/build.ps1`，增加 tokenizer 资源同步。
- 迁移：`crates/vulcan-token-counter/resources/tokenizers/**` 到 `crates/vulcan-runtime-assets/resources/tokenizers/**`。

### 3. 💻关键代码调整详情（方法变动、核心逻辑更替）

- `TokenBudgetRuntime::initialize` 使用 `vulcan_runtime_assets::resolve_tokenizer_resource_directory` 定位资源并调用 `TokenCounter::warm_up` 完成启动期初始化。
- `ContextTokenEstimator` 抽象让 `vulcan-context` 不直接依赖 GUI 或 token-counter crate，同时允许运行时注入真实模型感知估算器。
- `build_request_context_budget_snapshot_with_estimator` 与 `build_projected_session_context_budget_snapshot_with_estimator` 优先使用已初始化估算器，失败时回退 `estimate_mixed_language_text_tokens`。
- 运行时流 bootstrap 与 finalize 都传入同一个预算估算器，保证初始请求快照和最终 Provider 回填快照口径一致。
- 会话级 `resolve_session_context_budget_snapshot` 也改为复用共享预算运行时，压缩触发前的预算判断不再只依赖字符启发式。

### 4. ⚠️遗留问题与注意事项

- `TokenBudgetRuntime` 当前启动期只预热 OpenAI 核心 tokenizer，精确 `.tkz` tokenizer 仍保持按模型懒加载，避免初始化阶段一次性加载全部大资源。
- 如果后续发布包不经过 `scripts/build.ps1`，需要确保 `crates/vulcan-runtime-assets/resources/tokenizers` 被同步到最终运行目录。
- 本轮未把真实下载实现整体迁移到根脚本，原因是薄包装更利于保持 crate 自包含能力，同时满足仓库根入口需求。
- 已验证命令：`cargo fmt --package vulcan-code-gui --package vulcan-context --package vulcan-runtime-assets --package vulcan-token-counter`、`cargo check -p vulcan-token-counter -p vulcan-context -p vulcan-runtime-assets -p vulcan-code-gui`、`cargo test -p vulcan-token-counter`、`cargo test -p vulcan-context`、`cargo test -p vulcan-code-gui runtime_prompt_layers_for_context_compaction_use_developer_layer`、`python scripts/fetch_tokenizers.py --help`、`python crates/vulcan-token-counter/scripts/fetch_tokenizers.py --help`。
