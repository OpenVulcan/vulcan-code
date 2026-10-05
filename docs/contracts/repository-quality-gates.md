# 仓库合入检查与平台保护

## 永久检查入口

工作流为 `.github/workflows/quality-gates.yml`，适用于主分支推送、合并请求、手动执行，以及每周一台湾时间 03:23 的完整检查。Actions 引用固定提交；Node 与 pnpm 版本取自已有受管运行时清单，不增加第二份版本常量。未推送的本地工作流不会触发远程验证。

每次先执行 `Classify changes`，由 `scripts/ci-scope.mjs` 读取真实 Git 差异。推送比较事件 `before` 与 `after`，覆盖同一次推送的全部提交；PR 使用基准与提案的合并基点。关闭重命名折叠，旧路径和新路径同时参与分类。文件列表以 NUL 分隔，不依赖 GitHub 有数量上限的路径过滤或 webhook 提交数组。

| 变更范围 | 必需作业 |
| --- | --- |
| `apps/vulcan-code-web/**` | 前端 |
| `plugins/*/ui/**`、`plugins/*/locales/**` | 前端与内置插件界面测试 |
| `apps/vulcan-code-desktop/**` | 桌面 TypeScript 构建及单元测试 |
| 普通 `docs/**/*.md`、根 README、AGENTS、LICENSE、CHANGELOG | 分类自测与汇总 |
| 插件清单、Lua、Node 运行时及插件下其他文件 | 前端、Rust 格式与运行时回归 |
| 普通 `crates/**`、服务应用、Cargo 清单/锁、工具链、Cargo/nextest 配置 | Rust 格式与回归 |
| service-api、ai-contract、workbench-model、provider-config、terminal-host-protocol、plugin-runtime，以及契约目录 JSON/YAML/proto | 前端、桌面及 Rust |
| 工作流、构建脚本、make 入口、顶层 schemas、受管运行时版本清单 | 所有作业 |
| 未归类路径、首次推送、无法取得的旧推送基准 | 所有作业，并在日志说明原因 |
| 手动、周期执行 | 所有作业及完整诊断 |

多个路径取并集。运行资源中的 Markdown、插件提示词属于产品输入，不能按扩展名当成普通文档。修改共享协议时必须同步检查分类规则；上表只是概述，规则权威为 `ci-scope.mjs`，新增目录必须补充规则及正反回归。当前 Rust 归为一个检查域，尚未实现逐 crate 的反向依赖测试裁剪。

工作流入口不使用 `paths` 跳过整个工作流，避免必需状态永远停留在 Pending。`Required quality gates` 使用 `always()`：分类必须成功，已选作业必须成功，未选作业必须明确跳过；缺失、失败、取消和意外跳过均拒绝。不能只把所有 skipped 当成成功。新增作业必须同时更新选择、输出、汇总依赖及测试。

## 作业内容与耗时控制

- 构建脚本：三个平台的锁定工具联网安装、离线重装、下载摘要拒绝、目录发布故障、桌面布局、受管 Node 与浏览器契约测试。
- 前端：冻结安装、完整常规 Vitest、内置插件 UI 测试、静态/类型/架构检查、生产构建与包体预算。检查器自身的故意违规夹具只在检查器、配置或依赖变化时运行；正式检查始终执行。UI 审计目录中的浏览器专用测试仍由各专项验收入口执行。
- 桌面：冻结安装、编译全部 TypeScript 及单元测试；此作业不下载 Electron 可执行程序，不代替真实桌面启动验收。
- Rust 格式：独立提前执行，失败后不启动昂贵 Rust 作业。
- Rust：Windows 全工作区 `cargo check --locked --workspace --all-targets`，随后执行既有数据库、六项隔离回调、清单派生契约、内置插件真实运行时、日志、模型协议及宿主数据库回归。共享资源保持串行，编译并发保留仓库默认限制。
- 内核复杂度：完整执行时保留 Clippy 诊断，参数数量和类型复杂度不再通过 `-D` 将所有业务提交阻断；真实编译错误仍然失败。未添加警告全局屏蔽。

CI 使用现有安装器准备必需的 protoc、nextest、sg/rg、Lua 载荷与受管 Node，再显式 `make sync`。不执行整个开发机 `init-system`，不安装当前检查未使用的 hakari、deny、about 或宿主 RTK。nextest 改为官方精确版本二进制，版本和 SHA-256 统一锁定于 `scripts/build-inputs.lock.json`，每次安装都重新验证缓存字节及实际版本。完整开发初始化语义见 [开发环境](development-environment.md)。

pnpm 按各应用锁文件缓存下载内容；下载归档按系统、架构和构建输入锁缓存。Rust 使用固定提交的 `Swatinem/rust-cache`，自动区分工具链、清单、锁文件和编译环境，只保留依赖产物并清理增量缓存；仅 main 写入共享缓存，失败时保留已编译的依赖以减少重复成本。用户配置、完整 `output` 和凭证不进入缓存。首次冷编译仍可能较长，不承诺未经测量的总耗时。

## 本地验证与故障归属

本次检查价值审查移除了两个已失去当前产品归属的用例：输入区旧累计用量提示、父级工具卡旧子智能体 Tooltip。当前累计统计的作用域、缺失值和子会话胶囊由 `session-lifetime-usage.test.tsx` 覆盖。目标卡不再断言内部指标节点必须恰好八个；仍检查目标全文、后端预算用量、缓存比率和总量。粘贴测试等待真实的菜单关闭焦点恢复，保留原生命令及选区断言；统计测试保留非生成任务不得展示 Token 速度的业务边界。

后续评审测试时，以现行用户行为、数据正确性或失败隔离为保留理由。已删除功能、重复检查、内部结构计数应移除或合并；有效失败必须修复，不能用宽泛 skip、重试直到偶然成功或自动放宽预算处理。前端常规测试在本次 Windows 本地实测约 92 秒，主要耗时问题来自此前无差别启动的 Rust 初始化与冷编译，因此暂不引入可能漏掉动态加载、多语言和插件边界的前端逐文件裁剪。

审查快照：此前 33 次 Quality gates 中 Rust 作业有 20 次失败，其中 10 次格式失败、9 次参数数量复杂度失败、1 次 Runner 自身崩溃；前端没有失败。纯文档提交 `2cce409d` 对应的运行 [37230968402](https://github.com/OpenVulcan/vulcan-code/actions/runs/37230968402) 仍执行约 26 分钟初始化和约 21 分钟 Clippy。以上是本次改造前历史数据，不代表新流程远程耗时。

```text
node --test scripts/ci-scope.test.mjs
node scripts/ci-scope.mjs apps/vulcan-code-web/src/example.tsx
node scripts/ci-scope.mjs docs/contracts/example.md
node scripts/install-locked-tools.mjs nextest
node scripts/install-locked-tools.mjs nextest --offline
pnpm --dir apps/vulcan-code-web exec vitest run --maxWorkers=4
pnpm --dir apps/vulcan-code-web run build
pnpm --dir apps/vulcan-code-desktop run build
pnpm --dir apps/vulcan-code-desktop run test:unit
```

分类测试包含多提交推送、PR 合并基点、跨目录重命名、历史缺失、未知路径及错误汇总。工作流发布前另用 `actionlint` 检查语法、表达式和依赖图。工具安装和脚本检查通过，不等于全量产品测试通过；现有业务测试或包体预算失败必须单独报告，禁止通过删除断言、静默跳过或提高预算来伪造绿色状态。

这些检查不是完整产品验收：没有宣称覆盖所有 Rust 测试、所有 UI 交互或 Linux/macOS 上的真实桌面启动。完整发布仍需对应宿主的编译、同步、打包与启动验证。

## 历史平台阻塞证据

2026-09-26 查询 OpenVulcan/vulcan-code：仓库为私有，当前账号权限包含 admin=true。以下两个只读接口均返回 HTTP 403：

- GET /repos/OpenVulcan/vulcan-code/branches/main/protection
- GET /repos/OpenVulcan/vulcan-code/rulesets

返回原文为：Upgrade to GitHub Pro or make this repository public to enable this feature.

因此阻塞来自仓库套餐或可见性，不是缺少本地代码，也不是可以通过重复写接口绕过的权限问题。本次没有修改仓库可见性或购买套餐，不能宣称已启用强制合入保护。

## 平台能力具备后的配置

1. 先推送工作流，让三个类别的检查至少成功运行一次，确认检查名称出现。
2. 在仓库 Settings → Rules → Rulesets 为默认分支 main 建立活动规则。
3. 要求通过合并请求更新，至少一名审核者批准；新提交使旧审核失效，要求解决会话讨论。
4. 开启必需状态检查，选择 Required quality gates，并要求合并前分支保持最新。
5. 禁止强制推送与删除默认分支，不为普通开发身份设置绕过权限。
6. 使用故意失败的临时合并请求验证被阻止；修复后确认检查通过才允许合并。不可仅根据界面已勾选就记为验收通过。

本地提交仍按当前用户明确授权进行。启用规则后，后续主分支更新应遵循规则要求的合并请求流程。
