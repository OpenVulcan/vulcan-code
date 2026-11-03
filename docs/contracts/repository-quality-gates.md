# 仓库合入检查与平台保护

## 检查入口

| 工作流 | 触发 | 作用 |
| --- | --- | --- |
| `.github/workflows/ci.yml` | 合并请求（目标 main）、每天台湾时间 02:47（main 有新提交时）、手动 | 必过检查，分支保护只要求 `Required checks` 一项 |
| `.github/workflows/nightly.yml` | 每天台湾时间 03:17（main 有新提交时）、手动 | 完整测试集，不阻塞合并；失败时创建或更新跟踪 issue |
| `build-smoke`、`signing-smoke`、`distribution-smoke` | 手动 | 发布前的构建、签名与分发实测，见发布计划 |

仓库自 2026-11-02 起公开，CI 于 2026-11-03 恢复自动触发。推送 main 不触发任何运行：一次运行长于推送间隔，逐次触发
只会排队且跳过中间提交。定时运行先查询同一工作流在 main 上最近一次成功运行的提交，与当前提交相同即不运行任何检查
（`ci.yml` 由分类器输出全部跳过，`nightly.yml` 由 `pending` 作业跳过完整测试），因此 main 没有新提交时不消耗运行器；
上次失败的提交会在下一次定时运行时重试。发布前手动触发一次 CI，确认被发布提交的必过检查为绿色。
Actions 引用固定提交；Node 与 pnpm 版本取自受管运行时清单
`crates/vulcan-service-runtime/managed-runtimes.json`，不增加第二份版本常量。

### 变更分类

每次先执行 `Classify changes`，由 `scripts/ci-scope.mjs` 读取真实 Git 差异。推送比较事件 `before` 与 `after`，覆盖同一次推送的全部提交；PR 使用基准与提案的合并基点。关闭重命名折叠，旧路径和新路径同时参与分类。文件列表以 NUL 分隔，不依赖 GitHub 有数量上限的路径过滤或 webhook 提交数组。

| 变更范围 | 选中的检查 |
| --- | --- |
| `apps/vulcan-code-web/**` | 前端 |
| `plugins/*/ui/**`、`plugins/*/locales/**` | 前端与内置插件界面测试 |
| `apps/vulcan-code-desktop/**` | 桌面 |
| 普通 `docs/**/*.md`、根 README、AGENTS、LICENSE、CHANGELOG | 仅分类与汇总 |
| 插件清单、Lua、Node 运行时及插件下其他文件 | 前端与 Rust |
| 普通 `crates/**`、服务与 TUI 应用、Cargo 清单/锁、工具链、Cargo/nextest 配置、隔离清单 | Rust |
| service-api、ai-contract、workbench-model、provider-config、terminal-host-protocol、plugin-runtime、i18n，以及契约目录 JSON/YAML/proto | 前端、桌面与 Rust |
| 工作流、构建脚本、make 入口、顶层 schemas、受管运行时版本清单 | 全部 |
| 未归类路径、首次推送、无法取得的旧推送基准、手动执行、定时执行的新提交 | 全部，并在日志说明原因 |
| 定时执行且提交已在上次成功运行中通过 | 无 |

任一检查被选中时，静态检查同时被选中：它覆盖 Rust 与 TypeScript 源码，只需数秒。多个路径取并集。运行资源中的 Markdown、插件提示词属于产品输入，不能按扩展名当成普通文档。上表只是概述，规则权威为 `ci-scope.mjs`，新增目录必须补充规则及正反回归。

工作流入口不使用 `paths` 跳过整个工作流，避免必需状态永远停留在 Pending。`Required checks` 使用 `always()`：分类必须成功，已选作业必须成功，未选作业必须明确跳过；缺失、失败、取消和意外跳过均拒绝。作业与选择标记的对应关系是 `ci-scope.mjs` 的 `GATES`；新增必过作业必须同时更新 `GATES`、工作流 `needs` 及测试。

### `ci.yml` 作业

| 作业 | 平台 | 内容 |
| --- | --- | --- |
| Build scripts | ubuntu、windows、macos | 锁定工具联网安装后离线重装；`node --test scripts/*.test.mjs scripts/*.test.cjs` |
| Frontend | ubuntu | 冻结安装、Vitest、内置插件 UI 测试、ESLint、类型、架构、生产构建与包体预算；检查器故意违规夹具只在检查器、配置或依赖变化时运行 |
| Desktop | windows | 冻结安装、编译 TypeScript 及单元测试；不下载 Electron，不代替真实桌面启动验收 |
| Static checks | ubuntu | `node scripts/check.mjs rustfmt repository`：格式、文案断言与后端 CJK 棘轮、测试主目录隔离 |
| Rust clippy | ubuntu | `node scripts/check.mjs clippy`：全工作区全目标，警告即错误 |
| Rust fast tests | ubuntu | `node scripts/test-rust.mjs fast`：nextest `fast` 配置，排除隔离清单 |
| Windows service check | windows | `cargo check -p vulcan-code-service`；**暂不必过**，连续两周全绿后加入 `Required checks` |
| Required checks | ubuntu | 汇总，分支保护唯一必过项 |

Rust 作业的准备集中在 `.github/actions/rust-setup`：释放 Linux 磁盘、声明的 Node、`install_linux_system_deps.sh`、固定工具链、锁定的 protoc 与依赖缓存。测试作业另外安装 nextest、内置 sg/rg/rtk、Lua 载荷与受管 Node；不执行整个开发机 `init-system`。LuaSkills 发布查询带只读作业令牌，避免共享出口地址触发匿名限流。

合并请求的新提交取消旧运行；main 的运行不取消，排队的推送合并为最新一项，以保证缓存写入完成。CI 统一设置 `CARGO_INCREMENTAL=0`、`CARGO_PROFILE_DEV_DEBUG=0`（测试继承 dev，不关闭断言）与 `CARGO_BUILD_JOBS=4`（托管运行器为 4 vCPU、16 GB；仓库默认的 16 按 32 线程工作站设定）。nextest 线程数与串行组仍取自 `.config/nextest.toml`。clippy 与 Windows 检查作业超时为 60 分钟；测试作业冷缓存要编译隔离 LanceDB 库与工作区测试两套依赖图，超时为 150 分钟——超时会让编译器仍在写入 `target/`，缓存保存失败，之后每轮都会保持冷编译；验收目标是热缓存下必过项总耗时不超过 30 分钟。

### Rust 缓存

使用固定提交的 `Swatinem/rust-cache`：只缓存 Cargo 下载与第三方构建产物，工作区 crate 在保存前清除，因为新检出的源文件时间戳总会触发重新编译。`vendor/` 下的补丁 crate 是路径依赖，同样按时间戳判断；不处理时它与其上全部 LanceDB crate 每轮重编（热缓存下约占 10 分钟）。因此准备步骤把 `vendor/` 文件时间设为固定的旧值，并把 vendor 树哈希放进缓存键：vendor 内容变化时整个缓存失效，绝不恢复用旧源码编译的产物。只有 main 写入，合并请求只读。CI 快速测试与夜间完整测试编译相同的依赖图，通过 `shared-key: tests` 在每个平台共用一份缓存；Clippy 与 Windows 检查按作业各自一份。GitHub 按容量与访问策略淘汰归档，不能宣称永久命中，也不能把本地热构建当成远程缓存验收。

### 夜间与隔离清单

`nightly.yml` 在 Windows 与 Linux 上执行 `node scripts/test-rust.mjs full`（全部 nextest、`scripts/test-isolated-callbacks.mjs` 与 `cargo test --doc`），运行前执行 `make sync`；Linux 另跑插件清单 `schema-check`、`manifest_authoring` 与内置插件真实运行时。随后单独运行被隔离的测试，只报告结果、不使运行失败。任一步失败时，`report` 作业为标题为 `Nightly CI failure` 的 issue 追加评论，不存在则新建。

隔离清单 `.config/ci-quarantine.json` 只记录已确认不稳定的测试，每项精确给出包名、测试路径、原因、加入日期与负责人，由 `scripts/ci-quarantine.mjs` 校验并生成 nextest 过滤表达式。`fast` 与 `full` 都排除这些测试，`quarantine` 模式只运行它们；本地与 CI 读取同一份清单。修好后移出清单。不能把真实失败放入隔离清单来换取绿色状态。

## 本地验证与故障归属

提交前执行 `make check`（Windows 亦可 `./make check`），即不带参数的 `scripts/check.mjs`，依次运行全部分组：`rustfmt`、`clippy`、`web`（ESLint、TypeScript、界面架构）、`desktop`（桌面壳 TypeScript）与 `repository`。CI 按分组调用同一脚本，门禁只在一处定义。测试由 `make test`（`fast`）与 `make test full` 负责。

后续评审测试时，以现行用户行为、数据正确性或失败隔离为保留理由。已删除功能、重复检查、内部结构计数应移除或合并；有效失败必须修复，不能用宽泛 skip、重试直到偶然成功或自动放宽预算处理。

```text
node --test scripts/ci-scope.test.mjs scripts/ci-quarantine.test.mjs
node scripts/ci-scope.mjs apps/vulcan-code-web/src/example.tsx
node scripts/check.mjs rustfmt repository
node scripts/test-rust.mjs fast
node scripts/install-locked-tools.mjs nextest
node scripts/install-locked-tools.mjs nextest --offline
```

工作流发布前另用 `actionlint` 检查语法、表达式和依赖图。工具安装和脚本检查通过，不等于全量产品测试通过；现有业务测试或包体预算失败必须单独报告，禁止通过删除断言、静默跳过或提高预算来伪造绿色状态。

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

1. 先手动触发或通过合并请求运行工作流，让检查至少成功运行一次，确认检查名称出现。
2. 在仓库 Settings → Rules → Rulesets 为默认分支 main 建立活动规则。
3. 要求通过合并请求更新，至少一名审核者批准；新提交使旧审核失效，要求解决会话讨论。
4. 开启必需状态检查，选择 Required checks，并要求合并前分支保持最新。
5. 禁止强制推送与删除默认分支，不为普通开发身份设置绕过权限。
6. 使用故意失败的临时合并请求验证被阻止；修复后确认检查通过才允许合并。不可仅根据界面已勾选就记为验收通过。

本地提交仍按当前用户明确授权进行。启用规则后，后续主分支更新应遵循规则要求的合并请求流程。
