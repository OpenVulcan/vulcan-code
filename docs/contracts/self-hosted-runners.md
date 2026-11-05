# 自托管运行器

本文记录 CI（`ci.yml`）与发布（`release.yml`）如何使用维护者自有的机器。实现权威是
`scripts/ci-runners.mjs`（池、标签、选择与远程注册）、`.github/workflows/runners.yml`（每次运行选择一次）、
`.github/actions/self-hosted-workspace/`（保留编译结果与清理）以及 `scripts/self-hosted-runner.sh`、
`scripts/self-hosted-runner.ps1`（在 macOS/Linux 与 Windows 机器上安装或移除运行器）。

## 池与标签

- **池是一个构建平台**：`macos-x64`、`macos-arm64`、`linux-x64`、`windows-x64`（`ci-runners.mjs` 的 `POOLS`）。机器以
  `vulcan-<池>` 标签注册到本仓库，例如 `vulcan-macos-x64`；标签只由 `poolLabel` 生成。
- **只有 `ci.yml` 与 `release.yml` 使用池。** 它们调用 `runners.yml` 得到 `pools`，作业运行于
  `fromJSON(pools)[<池>] || <托管镜像>`。其他工作流（`release-setup.yml`、`build-smoke.yml` 等）不写池标签，
  始终使用托管镜像。组织为 GitHub 免费版，无法按工作流限制运行器组，因此"只给这两个工作流用"由标签保证。
- **当前使用池的作业**：
  - 发布：`Build (macos-x64)`、`Build (macos-arm64)`、`Build (linux-x64)` 各用同名池；
    `Build and sign (windows-x64)` 用 `windows-x64`。
  - CI：`Rust clippy`、`Rust fast tests` 用 `linux-x64`；`Service and setup check (windows-2025)` 用
    `windows-x64`；`Build scripts (macos-15)`、`Service and setup check (macos-15)` 用别名 `macos`
    （`ALIASES`），取 `macos-arm64`、`macos-x64` 中第一个有运行器在线的池。
  - 其余较轻的作业保留托管镜像，与池作业并行，不在同一台机器上排队。
- **`linux-x64` 机器必须是 Ubuntu 22.04。** 发布归档链接构建机器的 glibc，须保持 2.35 以支持旧发行版；
  发布作业在自托管 Linux 机器上先检查 `getconf GNU_LIBC_VERSION` 等于矩阵的 `glibc`，不符即失败。
  在 Windows 上用独立的 WSL 发行版提供（`wsl --install Ubuntu-22.04 --name <名称> --location <目录>`），
  不使用日常开发的发行版：后者会随系统升级改变 glibc，且作业的清理与依赖安装不应触及它。
- **`linux-arm64` 不进入池。** 构建脚本只产出其所在机器的平台（Node、Electron、浏览器、LuaJIT 与运行时
  布局均取宿主架构），交叉编译不受支持。
- **`windows-x64` 机器运行发布的签名作业。** 该作业属于需审批的 release 环境，签名密钥只在该作业中出现；
  缺少固定版本的 SimplySign Desktop 时签名步骤会安装它，因此运行器须在管理员终端中启动。

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
- **手动启动的运行器**（`--manual`，Windows 上始终如此）不安装服务，只在其启动脚本运行期间在线，关闭终端即
  离线，`auto` 随之回到托管镜像。用 Ctrl+C 停止，运行器会立即报告离线；强行结束进程时 GitHub 需要几分钟
  才判定离线，期间开始的运行会排队，按上面的方法取消重跑。

## 编译结果的保留

- 同一台运行器上本仓库的所有作业共用一个工作目录，因此**每个可能分到池的作业**都必须以 `clean: false`
  检出并执行 `prepare`；任何一个以默认方式检出的作业都会删除其他作业保留的 `target/`。
- 自托管作业检出时 `clean: false`，随后 `self-hosted-workspace` 的 `prepare` 删除 `target/`、
  `third_party/` 与 `.cargo/protoc/` 之外的全部未跟踪文件。发行目录 `output-release/` 与 `node_modules/`
  等每次重新生成，不会把上一版的旧文件打进新包。
- `target/` 跨运行保留，Cargo 按指纹只重编输入变化的 crate。
- `third_party/` 跨运行保留，前提是其中每个安装器都按锁定版本或摘要识别已有内容，只在其变化时重新下载或
  构建：浏览器组件与 Electron 缓存按版本分目录，受管 Node/pnpm、Inno Setup 按版本目录与身份清单，LuaSkills
  运行时按版本标记，随附工具、protoc 与判定执行库由 `install-locked-tools.mjs` 核对记录的归档摘要与实际版本
  输出（判定执行库逐文件核对摘要）。新增 `third_party/` 下的安装器必须同样核对，否则不能依赖此保留。
- 发布的 `rebuild` 输入为真时，`prepare` 删除 `target/`、`.cargo/protoc/` 与 `third_party/` 中除
  `build-input-cache/`（按摘要保存）以外的内容，全部重新编译与安装。
- 编译 Rust 的作业末尾的 `tidy` 执行 `make prune`（发布为 `make prune release`；Windows 上为 `make.ps1`），每个工作区 crate 保留最新单元，
  第三方单元不动；随后 `target/` 超过 150 GB（`target-cap-gb`）时整个删除，下次完整编译。
  `make prune` 由 PowerShell 实现，机器上需要 `pwsh`（注册脚本会安装）。
- 自托管时不使用 `Swatinem/rust-cache`，它会裁剪保留的 `target/`。

## 添加或移除机器

前置条件（需要管理员权限，由机器使用者执行）：
- 所有机器：运行器的 PATH 上要有 Node。作业在 `setup-node` 之前用 `node` 读取声明的运行时版本，因此
  运行器自身缺少的工具在机器上补齐，不由工作流代替安装。
- macOS：安装 Xcode 命令行工具（`xcode-select --install`），接电源时不休眠
  （`sudo pmset -c sleep 0 disksleep 0`），并开启"远程登录"以便维护者经 SSH 操作；运行器以 LaunchAgent
  运行，机器重启后需要该用户登录（或开启自动登录）才会上线。服务的 PATH 取自注册时写出的 `.path`
  （含 `~/.local/bin`），登录 shell 中的 Node 不一定在其中，可把它链接到 `~/.local/bin`。
- Linux（含 WSL 发行版）：Ubuntu 22.04，以 root 执行一次 `sh scripts/install_linux_system_deps.sh` 并安装
  `git curl unzip xz-utils zstd`，并把 Node 官方 Linux 归档（按其 `SHASUMS256.txt` 校验）解压后链接到
  `/usr/local/bin`；作业不使用 sudo，依赖齐全时该脚本不再调用包管理器。
- Windows：已安装 Git for Windows、PowerShell 7、Node 与 Rust 工具链，并完成本仓库的 `make init-system`。
  `shell: bash` 的步骤必须得到 Git Bash，而 Windows 可能把 WSL 启动器 `bash.exe` 排在前面，所以注册时
  写出 `start.cmd`，由它把 Git Bash 放在 PATH 最前再启动运行器。工作区为 `<目录>\w`，路径较短。

在维护者机器上（已用 `gh` 登录、可经 SSH 免密登录目标机器）：

```bash
node scripts/ci-runners.mjs register <位置> <池> [--name <名称>] [--dir <目录>] [--manual]
node scripts/ci-runners.mjs unregister <位置> [--dir <目录>]
```

`<位置>` 为 `用户@主机`（SSH）、`wsl:<发行版>`（本机的 WSL 发行版）或 `local`（本机）。
`register` 用维护者的 `gh` 取得一次性注册令牌，经标准输入交给目标机器上的
`scripts/self-hosted-runner.sh install`（本机为 Windows 时是 `self-hosted-runner.ps1`）：安装 PowerShell
（若缺少）与最新运行器（均按 GitHub 公布的 SHA-256 校验），以池标签注册到本仓库，并安装为服务；带
`--manual` 时不安装服务，而是写出启动脚本：Windows 在终端运行 `<目录>\start.cmd`；Linux（含 WSL）运行
`<目录>/start.sh`，WSL 即 `wsl -d <发行版> -- ~/actions-runner/vulcan-code/start.sh`。不要直接运行 `run.sh`：
它沿用启动终端的 PATH，`~/.local/bin` 与 `~/.cargo/bin` 可能不在其中，只有服务与 `start.sh` 会载入注册时
记录的 `.path`。令牌不打印、不写入命令行历史。无法 SSH 时，
可在机器上直接运行该脚本，并从标准输入提供在仓库 Settings → Actions → Runners 页面取得的令牌。

用 Apple 芯片机器服务 `macos-x64` 池时须让整个作业在 Rosetta 下运行（x86_64 的 Node 与 Rust 工具链）；
构建脚本按宿主平台选择产物，交叉编译不受支持。

## 安全

- 仓库公开。所有工作流都只能手动或由发布触发，没有 `pull_request` 触发；但外部贡献者的拉取请求可以新增
  工作流并写上池标签。仓库的外部贡献者工作流审批应设为"所有外部贡献者"（Settings → Actions →
  Fork pull request workflows），使这类运行必须先经维护者批准。
- 除 `windows-x64` 的签名作业外，池作业不使用任何密钥。签名作业属于需审批的 release 环境，外部贡献者的
  拉取请求取不到环境密钥；发布（上传、签名索引、GitHub Release）仍只在托管机器上进行。
