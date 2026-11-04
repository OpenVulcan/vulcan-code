# 仓库检查

## 检查入口

| 工作流 | 触发 | 作用 |
| --- | --- | --- |
| `.github/workflows/ci.yml` | 发布工作流调用（`workflow_call`）、手动 | 发布前的跨平台检查，每次检查全部内容 |
| `.github/workflows/full-tests.yml` | 手动、可被调用 | 完整测试集，发布需要比快速集更多的验证时使用 |
| `build-smoke`、`signing-smoke`、`distribution-smoke` | 手动 | 发布前的构建、签名与分发实测，见发布计划 |
| `.github/workflows/release.yml` | 手动（输入版本，可选 `rebuild`） | 构建五个平台、签名、上传到 R2 并指向 preview 通道，另建 GitHub Release 草稿；输入未变的程序取自上次已发布、已签名的版本，不重新编译（`scripts/release-reuse.mjs`），`rebuild` 强制全部编译 |
| `.github/workflows/promote.yml` | 发布 GitHub Release、手动 | 将 stable（或 preview）通道指向已上传的版本，不重新构建 |

本仓库由单人维护，自 2026-11-03 起 CI 只作为发布前的跨平台检查：推送与合并不触发任何运行，也不使用合并请求
流程或分支保护。日常提交由本地检查负责（见下文“本地验证与故障归属”）；CI 回答的是“被发布的提交能否在每个
系统上构建并通过测试”。发布工作流与构建同时调用 `ci.yml`，任一作业失败则不签署版本清单、不上传；也可手动启动。
`ci.yml` 不按变更分类，每次运行全部作业，整次运行的结论即检查结果，没有单独的汇总作业。
Actions 引用固定提交；Node 与 pnpm 版本取自受管运行时清单
`crates/vulcan-service-runtime/managed-runtimes.json`，不增加第二份版本常量。

### `ci.yml` 作业

| 作业 | 平台 | 内容 |
| --- | --- | --- |
| Build scripts | ubuntu、windows、macos | 锁定工具联网安装后离线重装；`node --test scripts/*.test.mjs scripts/*.test.cjs` |
| Frontend | ubuntu | 冻结安装、Vitest、内置插件 UI 测试、ESLint、类型、架构、生产构建与包体预算；检查器故意违规夹具每次都运行 |
| Desktop | windows | 冻结安装、编译 TypeScript 及单元测试；不下载 Electron，不代替真实桌面启动验收 |
| Static checks | ubuntu | `node scripts/check.mjs rustfmt repository`：格式、文案断言与后端 CJK 棘轮、测试主目录隔离 |
| Rust clippy | ubuntu | `node scripts/check.mjs clippy`：全工作区全目标，警告即错误 |
| Rust fast tests | ubuntu | `node scripts/test-rust.mjs fast`：nextest `fast` 配置，排除隔离清单 |
| Service check | windows、macos | `cargo check -p vulcan-code-service`：Rust 测试只在 Linux 运行，这里检查其余两个系统的平台专属代码 |

Rust 作业的准备集中在 `.github/actions/rust-setup`：释放 Linux 磁盘、声明的 Node、`install_linux_system_deps.sh`、固定工具链、锁定的 protoc 与依赖缓存。测试作业另外安装 nextest、内置 sg/rg/rtk、Lua 载荷与受管 Node；不执行整个开发机 `init-system`。LuaSkills 发布查询带只读作业令牌，避免共享出口地址触发匿名限流。

CI 统一设置 `CARGO_INCREMENTAL=0`、`CARGO_PROFILE_DEV_DEBUG=0`（测试继承 dev，不关闭断言）与 `CARGO_BUILD_JOBS=4`（托管运行器为 4 vCPU、16 GB；仓库默认的 16 按 32 线程工作站设定）。nextest 线程数与串行组仍取自 `.config/nextest.toml`。clippy 与平台检查作业超时为 60 分钟；测试作业冷缓存要编译隔离 LanceDB 库与工作区测试两套依赖图，超时为 150 分钟——超时会让编译器仍在写入 `target/`，缓存保存失败，之后每轮都会保持冷编译。

### Rust 缓存

使用固定提交的 `Swatinem/rust-cache`：只缓存 Cargo 下载与第三方构建产物，工作区 crate 在保存前清除，因为新检出的源文件时间戳总会触发重新编译。rust-cache 保存前会清除清单位于仓库根目录内的所有包，因此第三方补丁不能放在仓库内：`lance-bitpacking` 补丁放在独立仓库 `OpenVulcan/lance-bitpacking`，根清单的 `[patch.crates-io]` 按提交固定引用，`deny.toml` 只允许该 Git 来源。补丁曾放在 `vendor/` 时，它与其上全部 LanceDB crate 每轮在两个依赖图中重编，热缓存下约多 7–11 分钟。只有 main 上的运行写入，其他引用（如标签）的运行只读。CI 快速测试与完整测试编译相同的依赖图，通过 `shared-key: tests` 在每个平台共用一份缓存；Clippy 与平台检查按作业各自一份。GitHub 按容量与访问策略淘汰归档，不能宣称永久命中，也不能把本地热构建当成远程缓存验收。

### 完整测试与隔离清单

`full-tests.yml` 手动启动（也可由发布工作流调用），在 Windows 与 Linux 上执行 `node scripts/test-rust.mjs full`（全部 nextest、`scripts/test-isolated-callbacks.mjs` 与 `cargo test --doc`），运行前执行 `make sync`；Linux 另跑插件清单 `schema-check`、`manifest_authoring` 与内置插件真实运行时。随后单独运行被隔离的测试，只报告结果、不使运行失败。

隔离清单 `.config/ci-quarantine.json` 只记录已确认不稳定的测试，每项精确给出包名、测试路径、原因、加入日期与负责人，由 `scripts/ci-quarantine.mjs` 校验并生成 nextest 过滤表达式。`fast` 与 `full` 都排除这些测试，`quarantine` 模式只运行它们；本地与 CI 读取同一份清单。修好后移出清单。不能把真实失败放入隔离清单来换取绿色状态。

## 本地验证与故障归属

提交前执行 `make check`（Windows 亦可 `./make check`），即不带参数的 `scripts/check.mjs`，依次运行全部分组：`rustfmt`、`clippy`、`web`（ESLint、TypeScript、界面架构）、`desktop`（桌面壳 TypeScript）与 `repository`。CI 按分组调用同一脚本，检查只在一处定义。推送前在本地完成这些检查，不依赖 CI 发现问题。测试由 `make test`（`fast`）与 `make test full` 负责。

后续评审测试时，以现行用户行为、数据正确性或失败隔离为保留理由。已删除功能、重复检查、内部结构计数应移除或合并；有效失败必须修复，不能用宽泛 skip、重试直到偶然成功或自动放宽预算处理。

```text
node --test scripts/ci-quarantine.test.mjs
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
