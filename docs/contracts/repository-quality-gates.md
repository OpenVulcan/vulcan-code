# 仓库合入检查与平台保护

## 永久检查入口

工作流为 `.github/workflows/quality-gates.yml`。二〇二六年十月五日按用户要求，在正式开源前暂停主分支推送、合并请求和周期自动触发，仅保留 `workflow_dispatch` 手动入口。普通提交不启动任何 CI 作业；手动执行仍运行完整检查，且仍受组织账单限制。缓存、分类规则及所有检查实现保留，本地验证入口继续可用。

正式开源时需要显式恢复以下触发器；仅修改仓库可见性不会自动恢复。恢复后，周期检查为每周一台湾时间 03:23，推送与合并请求仍按下表选择所需作业：

```yaml
on:
  pull_request:
  push:
    branches: [main]
  workflow_dispatch:
  schedule:
    - cron: '23 19 * * 0'
```

恢复时先确认仓库公开状态与 Actions 可用性，再提交触发器变更并核对首次运行。暂停期间不能将“未运行”视为门禁通过；如果配置了必需状态检查，还应在正式启用时核对对应规则。Actions 引用固定提交；Node 与 pnpm 版本取自已有受管运行时清单，不增加第二份版本常量。

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

pnpm 按各应用锁文件缓存下载内容；下载归档按系统、架构和构建输入锁缓存。Rust 使用固定提交的 `actions/cache/restore` 与 `actions/cache/save`，对 Cargo 下载内容及独立的 `target/ci-rust` 编译产物执行分阶段恢复、保存；仅 main 写入共享缓存。用户配置、完整 `output`、凭证与开发机的普通 `target/debug` 不进入该缓存。首次冷编译仍可能较长，不承诺未经测量的总耗时。

### Rust 重依赖编译缓存

缓存实现权威为工作流和 `scripts/ci-rust-cache.mjs`。此前把缓存保存留到整个 Rust 作业的收尾阶段；二〇二六年十月五日复核时，远程缓存列表只有工具下载与 pnpm，没有已保存的 Rust 编译缓存。配置了缓存步骤不等于已经建立可复用的缓存。

1. 使用当前实际选中的 `rustc -vV`、编译环境、系统和架构划分缓存空间。不以提交 SHA 或运行编号创建新空间，也不因未使用的预装 Rust 工具链更新而失效。
2. 同一空间内以 Cargo 清单、锁文件、工具链配置、Cargo 配置、受管构建输入及 CI 执行规则求摘要。优先恢复同一输入的完整快照，其次恢复同一输入的数据库检查点，最后允许同编译环境的旧输入快照；所有恢复产物仍由 Cargo 指纹检查，依赖或源码变化必须重新编译相应单元。
3. 未命中完整快照时，先构建数据库测试在运行时加载的 LanceDB 原生库 `vulcan-lancedb-native`，再对真正要运行的 `vulcan-workspace-store` 库测试执行 `cargo test --no-run`。LanceDB 只编译进该库，成功后立即保存数据库检查点，再执行全工作区检查和其他回归。数据库测试步骤本身仍先构建该库，使早于该库的缓存也能运行。后续正常数据库测试继续执行。
4. 按后续真实回归使用的包、目标与特性编译其余测试程序，成功后用另一个不可变键保存完整编译快照，再运行测试。缓存保存不依赖业务测试结果，业务失败不会使已经成功的编译失效。数据库检查点和完整快照不能使用同一个键，否则早期快照会阻止完整产物写入。后续失败或取消不会删除已经上传成功的快照；发生在首次检查点上传前的取消仍会损失尚未保存的编译。
5. CI 统一使用 `CARGO_PROFILE_DEV_DEBUG=0`、`CARGO_INCREMENTAL=0` 和隔离的目标目录，减少重依赖调试符号及不适合临时 runner 的增量数据。默认 test 继承 dev；未关闭调试断言、整数溢出检查或减少已有测试。根清单对内核包显式声明的 limited 调试信息覆盖仍保留。开发机和 release 构建参数不变。
6. 每次运行在作业摘要显示精确命中状态、实际恢复键和请求键。精确命中只跳过预编译准备与重复上传，不能跳过类型检查、行为测试、资源同步或汇总门禁。只读 PR 可以使用 main 缓存，不负责写入共享快照。
7. 并发控制按检查域分组。新 WebUI 推送不能取消或排队等待旧 Rust 作业；主分支正在执行的 Rust 作业保留至完成，其后等待的同域推送合并为最新待执行项。PR 的过时 Rust 作业仍可取消，前端等快速检查也继续取消同域过时作业。这样连续推送不再反复中断主分支的首次缓存建立。

完整快照保留工作区编译产物以供 Cargo 校验复用，并非只缓存 registry 下载。键不随普通源码提交增长；锁定输入变化才产生新的一组快照。GitHub 仍可能按容量与访问策略淘汰归档，因此需要观察真实压缩体积和恢复耗时；不能宣称永久命中，也不能把本地热构建当成远程缓存验收。

上游依据：[独立保存动作](https://github.com/actions/cache/blob/0057852bfaa89a56745cba8c7296529d2fc39830/save/README.md)、[恢复与前缀匹配](https://github.com/actions/cache/blob/0057852bfaa89a56745cba8c7296529d2fc39830/restore/README.md)、[Cargo 配置继承与覆盖](https://doc.rust-lang.org/cargo/reference/profiles.html)、[按作业控制并发](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency)。升级这些动作或调整编译参数时，必须复核上述约束和缓存身份回归。

## 本地验证与故障归属

托管 CI 暂停期间，提交前执行 `make check`（Windows 亦可 `./make check`）。它运行 `scripts/check.mjs` 中的静态门禁：`cargo fmt --check`、全工作区 `clippy --all-targets -D warnings`、WebUI 的 ESLint、TypeScript 与界面架构检查，以及桌面壳 TypeScript；测试仍由 `make test` 负责。CI 恢复时，其 Rust clippy 步骤应改为调用同一脚本，使门禁只在一处定义。

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

## 二〇二六年十月五日前端远程失败复核

运行 [37286539757 的前端作业](https://github.com/OpenVulcan/vulcan-code/actions/runs/37286539757/job/111686803443) 在行为测试阶段出现十项失败，耗时约 316 秒。失败断言写死中文，实际结果使用 runner 的英文语言；涉及维护前置条件、其他项目回执隔离、差异正文省略和终端历史截断。改为通过确定的消息键及参数构造期望值，保留传输调用、项目身份、回执保留、取消重试、行数与序列等业务断言，不删除这些仍有价值的测试。

普通测试的公共准备仅加载中英目录；语言目录完整性、插件语言切换及德语确认词等专项测试自行加载所需目录。这样每个普通隔离测试文件不再重复加载额外九份大目录，全部十一种语言的覆盖仍保留。测试若需要其他语言，必须在自身测试中等待 `ensureLocaleCatalog`，不能依赖其他测试的执行顺序。验证业务行为时不得把系统语言当成固定中文；验证特定译文时应明确设置对应语言。

本地使用提交 `a05000cd` 的隔离源码副本，仅叠加本轮测试修复，在 Node 进程及测试子进程的设备语言均为 `en-US` 时执行完整前端测试：344 个测试文件通过、1 个原有文件跳过，2243 项通过、1 项原有跳过，耗时 82.01 秒。完整 ESLint、TypeScript 和界面架构检查通过。这里的 Windows 本地耗时不能与 GitHub Linux runner 的 316 秒直接当作同机性能对比；新的远程耗时仍待测量。

同一隔离副本的生产构建成功，但 CSS 原始体积为 502290 字节，超过既有 500000 字节预算；gzip 为 73761 字节，仍低于 74000 字节预算。截图中的作业尚未执行到预算检查，所以此项属于另一个已确认阻塞。未提高预算、删除检查或混入其他会话的界面修改；预算依据见 [前端质量门禁](webui-quality-gates.md)。

上述运行的 Rust 必需依赖准备为 13 秒，工作区类型检查约 22 分钟，首组数据库测试步骤约 38 分钟；截至本轮复核其余 Rust 回归仍在运行。测试步骤耗时包含冷编译，不能当成测试函数的纯执行时间，也不能把初始化改善等同于整个作业已完成加速验收。

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
