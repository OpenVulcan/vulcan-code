# 自托管运行器

本文记录 CI（`ci.yml`）与发布（`release.yml`）如何使用维护者自有的机器。实现权威是
`scripts/ci-runners.mjs`（池、标签、选择与远程注册）、`.github/workflows/runners.yml`（每次运行选择一次）、
`.github/actions/self-hosted-workspace/`（保留编译结果与清理）和 `scripts/self-hosted-runner.sh`
（在机器上安装或移除运行器）。

## 池与标签

- **池是一个构建平台**，目前只有 `macos-x64` 与 `macos-arm64`（`ci-runners.mjs` 的 `POOLS`）。机器以
  `vulcan-<池>` 标签注册到本仓库，例如 `vulcan-macos-x64`；标签只由 `poolLabel` 生成。
- **只有 `ci.yml` 与 `release.yml` 使用池。** 它们调用 `runners.yml` 得到 `pools`，作业运行于
  `fromJSON(pools)[<池>] || <托管镜像>`。其他工作流（`release-setup.yml`、`build-smoke.yml` 等）不写池标签，
  始终使用托管镜像。组织为 GitHub 免费版，无法按工作流限制运行器组，因此"只给这两个工作流用"由标签保证。
- **当前使用池的作业**：发布的 `Build (macos-x64)`、`Build (macos-arm64)` 各用同名池；CI 的
  `Build scripts (macos-15)`、`Service and setup check (macos-15)` 使用别名 `macos`（`ALIASES`），
  取 `macos-arm64`、`macos-x64` 中第一个有运行器在线的池。这些检查编译 macOS 平台代码，两种架构效果相同。
- **Linux 与 Windows 不进入池。** Linux 发布构建须在 Ubuntu 22.04 上编译，以保持 glibc 2.35 兼容；
  Windows 构建持有 release 环境的签名密钥。新增池前必须先解决这些约束，再修改 `POOLS` 与作业。

## 选择与回退

`runners` 输入（发布与手动 CI 均有，默认 `auto`）：

| 模式 | 行为 |
| --- | --- |
| `auto` | 池中有运行器**在线**时使用该池，否则使用托管镜像。需要仓库密钥 `RUNNER_STATUS_TOKEN`；缺少该密钥或查询失败时给出警告并全部使用托管镜像。 |
| `hosted` | 全部使用托管镜像。 |
| `self-hosted` | 不检查状态，全部使用池；没有在线运行器时作业排队等待（GitHub 最长等待 24 小时）。 |

- `GITHUB_TOKEN` 无法读取运行器状态，因此 `auto` 依赖 `RUNNER_STATUS_TOKEN`：一个细粒度个人令牌，只授权
  本仓库，权限只有 **Administration: Read-only**。由维护者自己创建并执行
  `gh secret set RUNNER_STATUS_TOKEN --repo OpenVulcan/vulcan-code` 存入；令牌值不经过任何会话。
- 选择发生在运行开始时。之后运行器离线、或运行器正忙，作业会排队等待；需要立即完成时取消该运行，
  以 `runners: hosted` 重新运行。
- 一台机器一次只运行一个作业；同一次运行中分到同一池的多个作业依次执行。

## 编译结果的保留

- 自托管作业检出时 `clean: false`，随后 `self-hosted-workspace` 的 `prepare` 删除 `target/` 与
  `third_party/build-input-cache/` 之外的全部未跟踪文件。发行目录 `output-release/` 等每次重新生成，
  不会把上一版的旧文件打进新包。
- `target/` 跨运行保留，Cargo 按指纹只重编输入变化的 crate；发布的 `rebuild` 输入为真时先删除 `target/`。
- 作业末尾的 `tidy` 执行 `make prune`（发布为 `make prune release`），每个工作区 crate 保留最新单元，
  第三方单元不动；随后 `target/` 超过 150 GB（`target-cap-gb`）时整个删除，下次完整编译。
  `make prune` 由 PowerShell 实现，机器上需要 `pwsh`（注册脚本会安装）。
- 自托管时不使用 `Swatinem/rust-cache`，它会裁剪保留的 `target/`。

## 添加或移除机器

前置条件（需要管理员权限，由机器使用者执行）：macOS 安装 Xcode 命令行工具（`xcode-select --install`），
接电源时不休眠（`sudo pmset -c sleep 0 disksleep 0`），并开启"远程登录"以便维护者经 SSH 操作；
运行器以 LaunchAgent 运行，机器重启后需要该用户登录（或开启自动登录）才会上线。

在维护者机器上（已用 `gh` 登录、可经 SSH 免密登录目标机器）：

```bash
node scripts/ci-runners.mjs register <用户@主机> <macos-x64|macos-arm64> [运行器名称]
node scripts/ci-runners.mjs unregister <用户@主机>
```

`register` 用维护者的 `gh` 取得一次性注册令牌，经标准输入交给目标机器上的
`scripts/self-hosted-runner.sh install`：安装 PowerShell（若缺少）与最新运行器（均按 GitHub 公布的
SHA-256 校验），以池标签注册到本仓库，并安装为服务。令牌不打印、不写入命令行历史。无法 SSH 时，
可在机器上直接运行该脚本，并从标准输入提供在仓库 Settings → Actions → Runners 页面取得的令牌。

用 Apple 芯片机器服务 `macos-x64` 池时须让整个作业在 Rosetta 下运行（x86_64 的 Node 与 Rust 工具链）；
构建脚本按宿主平台选择产物，交叉编译不受支持。

## 安全

- 仓库公开。所有工作流都只能手动或由发布触发，没有 `pull_request` 触发；但外部贡献者的拉取请求可以新增
  工作流并写上池标签。仓库的外部贡献者工作流审批应设为"所有外部贡献者"（Settings → Actions →
  Fork pull request workflows），使这类运行必须先经维护者批准。
- 池作业不使用任何密钥；签名与发布只在托管机器上的 release 环境作业中进行。机器上只有运行器自身的凭据。
