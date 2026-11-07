# 应用内更新

本文记录已安装 Vulcan Code 的应用内更新契约。实现权威是 `crates/vulcan-app-update/`（流程）、
`crates/vulcan-install-core/`（布局、标记、校验、组装、修复）、`crates/vulcan-service-api/src/app_update.rs`
（线协议）与 `apps/vulcan-code-launcher/src/trial.rs`（启动试运行）。设计依据见发布方案第八、九节；
方案只记录决策，当前行为以源码为准。

## 职责划分

- **Core 独占更新逻辑**：检查、下载、校验、组装、切换、修复、清理都在服务进程内，经
  `service_app_update_*` 等 8 个 RPC 方法暴露。客户端（WebUI、桌面、TUI）只显示快照、发起确认，
  不自行下载或改写安装目录。
- **一次只运行一个操作**：检查、下载、修复、重启互斥；进行中的下载或修复每 500 毫秒报告一次进度。
  取消保留已下载的 `.partial` 文件，下次继续。
- **快照即状态**：每个 RPC 都以完整 `AppUpdateSnapshot` 应答；状态每次变化都发布
  `app.update.changed`。客户端不缓存派生状态，以最近快照为准。
- **不可更新时如实说明**：调试构建、不在 `versions/<版本>` 布局中、缺少或无效的
  `.vulcan-install.json`、不支持的平台，均在 `unavailableReason` 中给出原因，客户端不提供任何更新操作。
  旧版 `install.sh` 以摘要命名代际目录，经它安装的 Linux/macOS 副本显示“未安装布局”，由安装器
  `vulcan-code-setup` 接管后改为 `versions/<版本>`。

## 检查与提议

- 自动检查：服务启动约 60 秒后首次检查，之后按设置 `checkInterval` 的间隔（1、6、12、24 小时，默认 6 小时）再检查；间隔从上一次检查（自动或手动）完成时算起。调度每分钟醒来一次判断是否到期，修改间隔无需重启即可生效；更新器忙碌或已有等待重启的版本时跳过。间隔时长只在 `vulcan-app-update` 的 `check_interval` 定义。
- 顺序固定：指针 → 版本 → 清单 → 发布说明；全部经内置 Ed25519 公钥校验。只接受高于当前版本的版本。
- 稳定版始终检查；预览通道开启时同时检查预览版并取较高者。
- 下载源：`auto` 先国际源再中国镜像，`international`、`china` 只用对应源。
- 安装纪元升高或缺少已安装组件时为完整更新：Windows 下载 Inno 安装包（SHA-256 来自签名清单，
  Authenticode 签名者须为发布证书），Linux/macOS 只提供下载页 `https://openvulcan.com/download`。
  安装根含 `setup/installation.json`（`SETUP_DIR`/`SETUP_STATE_FILE`，定义在 `install_root.rs`，只有
  `vulcan-code-setup` 写入）时该根归安装器所有：完整更新一律不可由应用安装（`installable: false`、
  `setupManaged: true`），界面只提示运行 `vulcan-code-setup update`，不提供下载页或 Inno 安装包，避免
  产生第二个卸载项并使安装器状态失真。增量更新不受影响。
  其余情况为增量更新，只下载摘要不在本地安装记录中的文件。
- 回退提示仍在时，提议的版本若正是启动失败的版本，仍可提议但标记 `previouslyFailed`；用户关闭提示后不再标记。
- **被跳过版本的说明**：版本清单的可选字段 `previousVersion` 写明上一发布（`release-plan.mjs` 取更新记录
  `releases.json` 中的下一项，`release.yml` 以 `--previous-version` 传给发布工具）。旧客户端与 openvulcan-web
  静态同步忽略该字段，因此不递增清单格式。Core 从提议版本起沿该字段回溯，逐个按签名读取清单、按摘要读取说明，
  直到不高于已安装版本，结果以 `skippedReleases`（从新到旧）给出，最多 `MAX_SKIPPED_RELEASES` 个。这些说明只用于
  介绍更新：缺少链接（在该字段之前发布的清单）、某个发布无法读取或达到上限时，回溯结束并把
  `skippedReleasesComplete` 设为 false，提议照常给出，界面提示更早的说明未能显示。回溯依赖旧版本对象长期保留在
  两个下载源中（主源不可变、镜像不删除）。

## 设置

- 只有 `preview`（默认关闭）、`route`（默认 `auto`）与 `checkInterval`（默认 `sixHours`）三项。默认值只在内存中补全；只有用户显式保存时才写入
  `appUpdate`，值等于默认时不写出（第一节第 5 条）。
- 保存始终提交完整设置文档。
- 设置页自上而下为：当前版本（检查与修复）、更新设置、找到的更新（下载或重启，以及从提议版本起从新到旧的每个版本说明）。更新对话框与设置页共用同一组版本说明组件。

## 切换、重启与试运行

1. 增量更新在运行版本旁组装 `versions/<新版本>`，然后原子切换当前版本（Windows `current.json`，
   其他平台 `current` 符号链接），并写入 `updates/startup-trial.json`：`version`、`previous`、
   `attempts: 0` 与 `max_attempts`（取自 `MAX_STARTUP_ATTEMPTS`，当前为 2）。
2. 重启会停止所有运行中的后台命令任务：WebUI 在重启前以 `service_running_shell_job_count` 读取数量，大于 0 时
   先给出警告，由用户在警告中选择“仍然重启”；计数失败不阻止更新。用户确认重启后，服务发布 `service.restarting`（`reason` 为 `update` 或 `installer`），正常排空后
   以相同参数加 `--await-owner-exit` 分离启动新服务；新服务最多等待 120 秒取得实例锁。在 systemd 用户
   服务中改用 `systemd-run --user` 重新安装用户单元，单元的 `ExecStart` 使用稳定路径 `<root>/current/<rel>`。
   是否运行在该单元中只按 `/proc/self/cgroup` 的叶节点是否为本单元名判断，不看 `INVOCATION_ID`：GNOME Shell、
   plasmashell 等以 systemd 服务运行的桌面外壳会把它传给所启动的应用，桌面端启动的 Core 因此也带有它。
   用户服务只有用户级一种（`--install-user-service`，Linux root 同样用自己的 systemd 用户实例）：Windows 为
   HKCU `Run` 值（Core 每次启动刷新为当前版本路径），macOS 为指向稳定路径的 LaunchAgent。
3. 桌面端等待旧 Core 进程退出后，经稳定入口重新启动（Windows 为根目录启动器 `VulcanCode.exe`，
   其他平台为 `<root>/current/<rel>`）。完整安装时桌面端只退出，安装包以
   `/SILENT /SUPPRESSMSGBOXES /NORESTART /RELAUNCH=1` 运行，结束后由 `[Run]` 中的
   `RelaunchRequested` 启动应用。TUI 与浏览器自动重连。
   新服务、安装包与重新启动的桌面端都不继承旧进程的 `VULCAN_RUNTIME_LAYOUT`：该变量指向旧版本的布局清单，
   继承后新版本会报告新版本号却从旧版本目录提供 WebUI 等资源；新进程改为读取自身可执行文件旁的清单。
4. Windows 启动器每次桌面启动计一次尝试；`previous` 或试运行版本的服务可执行文件（各自 `runtime-layout.json`
   的 `serviceExecutable`）正在运行时本次既不计数也不回退，因为桌面端会复用该 Core，新版本并未被启动
   （运行判断依据 Windows 对运行中映像写访问的共享冲突，其他结果按未运行处理）；`attempts` 达到 `max_attempts` 而标记仍在时，把
   `current.json` 指回 `previous`，写入 `updates/rollback.json`（`failed`、`restored`）并删除试运行标记。
   试运行版本已不是当前版本时（例如随后运行了完整安装包）直接删除标记。服务正持有安装锁时本次不处理。
   上一版本目录已不存在时启动器报错，不猜测其他版本。
5. 新服务启动时：删除属于自己的试运行标记；回退标记的 `restored` 等于自身版本时保留并在快照中提示，
   否则删除；当前版本等于自身时清理旧版本（只保留当前版本与比它低的最新一个版本）；当前版本高于自身时
   标记为已就绪。

## 修复

修复按签名清单与组件索引核对安装记录中的每个文件，只重新下载缺失或摘要不符的文件，结果以
`repairedFiles` 报告。

## 安装器的独立发布与自更新

安装器 `apps/vulcan-code-setup` 独立于产品发布（方案 18.6），实现权威是 `vulcan-install-core` 的
`setup.rs`、`layout.rs`，发布工具的 `setup.rs`，以及 `release-setup.yml`、`promote-setup.yml`。

- **版本**：只在 `apps/vulcan-code-setup/Cargo.toml` 中定义（不跟随工作区版本），只有安装器本身变化时递增；
  产品发布不构建安装器。
- **文件与指针**：某安装器版本的各平台程序 `vulcan-code-setup-<版本>-<平台>[.exe]` 与填好的 `install.sh`、
  `install.ps1` 位于 `setup/<版本>/`，不可变；已签名指针 `setup/channels/<通道>.json`（与产品指针同一把 Ed25519
  密钥）列出版本及每个文件的 SHA-256 与大小，必须恰好覆盖五个平台与两个脚本，stable 不得指向预发布版本。
  `release-setup.yml` 先发布到 preview，`promote-setup.yml` 校验 preview 指针后为同一组文件签署 stable 指针。
- **入口脚本**：两个脚本在安装器发布时由 `scripts/entry-scripts.mjs` 填入版本、下载源（`AUTO_ORIGINS`）、各平台
  摘要与 Windows 签名证书指纹；它们只下载并校验安装器，再把全部参数原样交给它。程序无法修改启动它的 Shell
  的环境，因此 `install.ps1` 在安装器成功后把当前会话缺少的用户 PATH 条目加入会话（`irm | iex` 时即用户自己的
  窗口，命令立即可用）；Linux 与 macOS 的 `curl | sh` 运行在子 Shell 中，`~/.local/bin` 不在 PATH 时安装器给出
  本 Shell 可执行的 `export` 命令。`promote-setup.yml` 把
  stable 安装器指针所列的脚本按摘要校验后放到主下载源顶层（缓存 60 秒）；openvulcan-web 把 `setup/` 与顶层
  脚本同步到中国镜像。
- **自更新**：`install`、`update` 之前读取所选通道（preview 同时读 stable）的安装器指针，只与自身版本比较；
  产品更新不改变安装器字节，也不提示更新安装器。交互时询问，`--self-update` 直接更新，`--yes` 或无终端时只在
  最新产品版本的 `minSetupVersion` 高于自身时更新，`--no-self-update` 跳过。替换后以相同参数加
  `--no-self-update` 重新运行。组装前仍按所装版本的 `minSetupVersion` 检查，不满足即停止。
- **Inno 安装包**：产品发布在构建前下载 stable 安装器指针与 Windows 程序，经 `vulcan-release-tool setup-check`
  校验后按原样装入 `{app}\setup\vulcan-code-setup.exe`（不再签名），卸载时删除；stable 安装器指针缺失或校验
  失败时产品发布停止。
- **安装选项**（实现权威 `args.rs`、`integrate.rs`）：所有平台都可选 GUI（`--gui`，Linux 无桌面环境时仍可安装，
  只是无法运行）、开始菜单/应用菜单（`--menu`）、桌面快捷方式（`--desktop-shortcut`）、PATH（`--path`）、用户服务
  （`--service`，调用 Core `--install-user-service`）与 Linux linger（`--linger`，经 `loginctl enable-linger`，
  失败时提示 sudo；只在本程序启用时记录并在卸载时撤销）。端口范围（`--port-range`）调用 Core
  `--set-port-range` 写入 Core 拥有的 `service-settings.json`，安装器不保存副本。旧的 `--mode`、`--no-shortcut`
  继续接受。root 安装与无 systemd 用户实例时给出说明，不注册系统级服务。
- **Windows 桌面集成**：GUI 安装时把 `bin/vulcan-code-launcher.exe` 复制为 `<root>\VulcanCode.exe`，快捷方式用
  运行布局（schema 4）的 `desktopIdentity` 写入 AppUserModelID 与通知激活 CLSID，与 Inno 安装包一致；同时把
  安装器放入 PATH 并作为卸载入口的 `DisplayIcon`/卸载命令。完成与 `status` 时告知卸载命令。
- **安装记录**：`installation.json` 为 v2；读取 v1 时先把原字节保存为 `installation.json.v1`，再升级写回。
  只记录本程序创建的文件、目录与系统集成，卸载时只撤销这些项。
- **卸载**：Windows 无法删除运行中的程序，因此确认后、任何改动之前先结束程序位于安装根目录下的进程（桌面应用、
  服务、TUI），结束不了即停止并保持安装完整；当前版本已缺失时直接移除指向本根目录 `versions` 的登录 `Run` 值。
  安装器以文件而非管道接收 Core 的输出：Windows 上 `--install-user-service` 启动的服务继承父进程的输出句柄，
  管道会一直保持打开；同理，Windows 上安装器以显式句柄列表启动 Core，只交出 NUL 与这两个文件（Windows
  PowerShell 等会把其输出的额外可继承副本交给安装器），调用方经管道读取安装器输出时不会被服务挂住。

## Linux 桌面应用的 AppArmor 配置

- **原因**：Ubuntu 23.10 及以后（`/proc/sys/kernel/apparmor_restrict_unprivileged_userns` 为 `1`）只允许
  AppArmor 配置许可的程序使用非特权用户命名空间；程序继承启动者的配置，因此桌面应用从桌面图标能启动，
  从程序坞或应用菜单启动时 Electron 沙箱失败退出。唯一不降级的做法是一次性以 root 安装
  `/etc/apparmor.d/vulcan-code`（`flags=(unconfined)` 加 `userns,`），它附着到 `<已解析根>/versions/*/<桌面程序>`，
  因此之后每个版本的应用内更新都无需再次提权。`--no-sandbox` 属于降级，不得使用。
- **唯一实现**：配置文本、路径常量、比较（`Current`/`Missing`/`Outdated`/`Foreign`）与命令都在
  `vulcan-install-core` 的 `apparmor.rs`；标记行 `GENERATED_MARKER` 与布局文件名 `RUNTIME_LAYOUT_FILE` 在
  `install_root.rs`；桌面组件名 `DESKTOP_COMPONENT` 在 `names.rs`。安装器与 Core 写出的字节完全相同。
- **安装器**：GUI 安装且限制开启时，在终端中经 `sudo` 安装并记录为 `Item::AppArmorProfile`；拒绝密码时
  只在提示中留下命令。无标记的同名文件属于他人，不覆盖、不删除。卸载时，未被记录但带标记且指向本根目录的
  配置（由 Core 安装）同样移除。
- **Core**：只在 Linux、已安装 `gui` 组件、限制开启且配置为 `Missing` 或 `Outdated` 时，快照给出
  `desktopProfile`（`status`、`installing`、`command`、`error`）；`Current` 与 `Foreign` 不提示。启动时把配置
  写到 `updates/vulcan-code.apparmor`，使 `command`（`sudo` 命令行）可直接运行。`service_install_desktop_profile`
  以 `pkexec /bin/sh -c` 一次完成复制与加载（root 下直接运行），密码由桌面会话的 polkit 代理询问；不占用
  更新器的互斥操作，进行中再次调用被拒绝。失败（未给密码、无认证代理、缺少 `pkexec`）只写入 `error`，
  不重试、不改用其他方式。以 systemd 用户服务运行的 Core 不在登录会话内，polkit 能否找到代理以真机为准，
  找不到时用户使用 `command`。
- **界面**：WebUI 每个页面提示一次（toast，指向更新设置页）；更新设置页与 TUI 更新页显示说明、安装按钮、
  终端命令与失败原因。

## 同步触发点

- 安装器 `integrate.rs` 的 `DESKTOP_WINDOW_CLASS`（菜单项的 `StartupWMClass`）与 `DESKTOP_ENTRY_FILE` 都派生自
  `desktop_app_id!`（`vulcan-code`）。`scripts/desktop-runtime.mjs` 写入打包应用 `package.json` 的 `desktopName`
  必须等于该 ID 加 `.desktop`：Electron 由 `desktopName` 得出窗口类（X11 上为 `WM_CLASS "vulcan-code"`）与
  Wayland 应用 ID，而不是取 `package.json` 的 `name`。不一致时 GNOME 会为窗口另显示一个通用图标。修改任一侧时
  两处同时改。
- Linux 窗口与 macOS 程序坞图标来自打包应用 `gui/` 旁的 `icon.png`（`desktop-runtime.mjs` 从
  `apps/vulcan-code-service/icons/icon.png` 复制，桌面端 `app-icon.ts` 读取）；Windows 图标来自程序资源。
- 安装器对象键（`setup_pointer_key`、`setup_file_key`）与产品对象键一样，在 `layout.rs` 与 openvulcan-web 的
  `server/internal/staticsync/format.go` 中各写一次，修改时两处同时改。
- 入口脚本在安装器发布时内嵌下载源；修改 `AUTO_ORIGINS` 后须发布新的安装器版本，脚本才会使用新下载源。

- 试运行与回退标记的文件名、字段在 `install_root.rs` 与启动器 `trial.rs` 中各写一次（启动器只依赖
  `serde_json`），修改时两处同时改；尝试上限只在 `MAX_STARTUP_ATTEMPTS` 定义并随标记写出。
- `/RELAUNCH=1` 在 `INSTALLER_ARGUMENTS` 与 `scripts/windows-installer.mjs` 的 `RelaunchRequested` 中成对出现。
- 启动器、安装器模板、注册表、PATH、快捷方式或安装布局变化时递增 `product.json` 的 `installEpoch`
  （方案第九节），旧安装由此走完整更新。安装器自身写入的快捷方式与注册表项随安装器版本发布，不触发
  `installEpoch`；运行布局新增可选字段（如 `desktopIdentity`）而启动器不校验其版本时同样不触发。
- 运行布局的 `desktopIdentity` 由 `scripts/desktop-runtime.mjs` 从桌面端的通知身份常量写出，是安装器读取 AUMID 与
  通知 CLSID 的唯一来源；修改身份值时只改桌面端常量。
- TUI 调用一对一镜像 `apps/vulcan-code-web/src/service/app-update-service.ts`，见
  [`tui-service-adaptation.md`](tui-service-adaptation.md)。
