# Vulcan Code Agent Notes

本文件只保存任何会话进入仓库后都必须知道的长期约定。专项实现细节放在按需文档中；任务命中对应
条件时才读取，不能把某次会话纪要、版本快照或调试步骤继续堆回本文件。

## 一、所有会话必须遵守

1. **事实先于实现。** 代码路径、类型、字段、协议、配置来源和调用链必须以当前源码、Schema、测试
   和真实运行证据为准；历史计划、完成记录和旧会话只能作为线索。
2. **保护并行工作与脏工作树。** 只修改本任务拥有的文件和逻辑。发现错误属于其他并行会话或未修改
   模块时，记录边界并停止越界修复。
3. **区分 AI 与用户文案。** 面向 AI 的 tool name、description、Schema、参数说明和工具错误使用
   英文；面向用户的 WebUI 与桌面壳文案可以使用中文。
4. **工具能力必须走完整暴露链。** “已实现”不等于“AI 可见”；新增或修改工具时必须核对注册、请求
   构建、模式可见性、策略过滤、执行和结果投影。
5. **不得静默改写用户配置。** 系统默认优先使用运行时内存默认值；不能在启动或刷新时偷偷回写用户
   持久化策略、模型用途或工具选择。
6. **声明后直接执行。** 模型级协议、工具目标和运行时代际一旦冻结就按声明使用；错误直接暴露，
   不探测备用协议、不静默降级、不用旧连接字段覆盖新权威。
7. **历史状态与实时状态分层。** 一次性结果可以依赖历史记录；长生命周期任务、命令和异步 Provider
   工作必须保留按稳定 ID 查询实时状态的路径。
8. **源码资源与运行产物分离。** 只修改 `crates/vulcan-runtime-assets/resources/` 下的权威资源，不直接
   编辑 `output/`。资源、配置、技能或运行时依赖变化后显式执行 `make.ps1 sync`；`build`、`br` 和
   `release` 不负责同步。
9. **版本以实时依赖为准。** 任何库、协议或运行时版本先读取当前 `Cargo.toml`、`Cargo.lock`、Schema
   和生成配置，不从本文档标题、日期或历史记录推断。
10. **按需文档是条件性强制要求。** 任务命中下列领域时，必须先阅读对应链接，再开始修改。

## 二、按需文档入口

| 任务条件 | 必须先读 |
| --- | --- |
| 新增、迁移或维护系统供应商、模型协议、Provider Tool、媒体扩展 | [`新增系统供应商流程.md`](docs/contracts/adding-system-providers.md) 与 [`Provider Manifest 作者手册`](docs/contracts/provider-manifest-authoring.md) |
| 修改 `shell_command`、`shell_session`、`terminal_session` 或命令会话状态 | [`工具与终端运行时约定.md`](docs/contracts/tooling-and-terminal-runtime.md) |
| 修改 `ask`、提问状态机、Ask WebUI 或 Markdown 预览/渲染 | [`Ask 与 Markdown 交互约定.md`](docs/contracts/ask-and-markdown-interaction.md) |
| 修改 LuaSkills 版本、技能配置、Schema、宿主注入或管理链路 | 先读根 [`Cargo.toml`](Cargo.toml) 与 [`Cargo.lock`](Cargo.lock)，再读 [`LuaSkills 技能包配置接入指南`](docs/contracts/luaskills-skill-config-guide.md)；实现以 [`luaskills_runtime`](crates/vulcan-agent-runtime/src/luaskills_runtime/) 和 [`tooling/bridge`](crates/vulcan-agent-runtime/src/tooling/bridge/) 当前源码为准 |

## 三、文档维护规则

- 专项契约变化时，同步更新对应按需文档和源码测试；不要把完整实现细节复制回 `AGENTS.md`。
- 链接目标已经删除、移动或与当前源码不一致时，必须在同一任务中修正或移除链接。
- 已完成计划可以保存决策证据，但不能作为当前运行时契约的唯一来源。

## 四、测试与构建资源约束

1. **默认并发上限。** 仓库 `.cargo/config.toml` 将 Cargo 编译并发限制为 `build.jobs = 4`，常规 Rust 测试 harness 限制为 `RUST_TEST_THREADS = 4`；不得在验证命令中无故移除或绕过这两个默认值。
2. **nextest 约束。** `.config/nextest.toml` 将默认 `test-threads` 限制为 4；使用 nextest 时保留该配置，若临时提高并发必须明确说明原因与资源风险。
3. **共享资源测试必须串行。** 涉及共享 Provider Manifest、服务内核注册表、端口或其他进程级资源的测试，使用 `--test-threads=1`；nextest 使用 `cargo nextest run --test-threads=1`，避免并行测试互相争用导致假失败。
4. **优先小范围验证。** 修改后先执行目标 crate 的 `cargo check` 或定向测试，再决定是否运行工作区级测试；工作区全量测试可能同时启动大量测试进程，不得默认无约束执行。
5. **doctest 单独执行。** `cargo nextest` 不覆盖 doctest；需要验证文档测试时必须额外执行 `cargo test --doc`。
6. **新环境入口。** 新开发环境优先执行 `make init-system`，不要要求会话手工拼接宿主、Lua、VMM、代码语义索引和 Rust 测试工具安装命令；初始化入口的变更必须同步更新 `docs/contracts/development-environment.md`。
