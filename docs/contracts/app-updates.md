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

- 自动检查：服务启动约 60 秒后首次检查，之后每小时检查一次；从上次检查完成时算起。更新器忙碌时等空闲，已有就绪版本时仍继续检查。间隔只在 `vulcan-app-update` 的 `CHECK_INTERVAL` 定义。
- 顺序固定：指针 → 版本 → 清单 → 发布说明；全部经内置 Ed25519 公钥校验。只接受高于当前版本的版本。
- 稳定版始终检查；预览通道开启时同时检查预览版并取较高者。
- 下载源：`auto` 先国际源再中国镜像，`international`、`china` 只用对应源。
- 安装方式由 `vulcan-app-update` 的 `install_method` 决定。组装按摘要复用运行版本和上次就绪版本的文件，
  同样适用于跨安装纪元。已有就绪版本时，Range 请求只覆盖缺少的压缩帧。旧就绪目录保留到新目标持久化之后。
  - 同一纪元且清单含全部已安装组件：增量更新（`kind: incremental`）。下载阶段只组装，用户确认更新重启时才切换并写入试运行标记。
  - 安装根含 `setup/installation.json`（`SETUP_DIR`/`SETUP_STATE_FILE`，定义在 `install_root.rs`，只有
    `vulcan-code-setup` 写入）时该根归安装器所有。纪元升高时为 `kind: setup`：下载阶段同样只组装
    `versions/<新版本>`，但不切换、不写试运行标记；重启时由根目录自带的安装器 `setup/<SETUP_PROGRAM>`
    切换并刷新它在版本目录之外安装的内容（见下文“经安装器完成”）。根目录缺少安装器程序或清单缺少已安装
    组件时不可由应用安装（`installable: false`、`setupManaged: true`），界面只提示运行
    `vulcan-code-setup update`。安装器拥有的根目录绝不运行 Inno 安装包，避免产生第二个卸载项并使安装器状态失真。
  - 旧 Windows 根目录由 Inno 安装包创建。纪元升高且清单含全部已安装组件与 Windows 兼容更新包
    （清单字段 `innoUpdate`）时同为 `kind: setup`：
    下载阶段先组装 `versions/<新版本>`（不切换、不写试运行标记），再下载更新包，下载量为变化的文件加更新包；
    重启时按完整安装包的方式运行更新包（见下文“经 Inno 更新包完成”）。更新包与完整安装包一样，SHA-256 来自
    签名清单，Authenticode 签名者须为发布证书；组装后更新包下载或校验失败时，已组装的版本留给下一次尝试复用，
    或在下一次服务启动时清理。只有清单没有更新包（它出现之前发布的版本）或缺少已安装组件时才下载完整 Inno
    安装包（`kind: full`）；两者都没有时不可由应用安装。
  - 不归任何安装程序所有的 Linux/macOS 根目录在版本目录之外没有需要刷新的内容，纪元升高时同增量更新；
    只有清单缺少已安装组件时才不可由应用安装，此时提供下载页 `https://openvulcan.com/download`。
- 回退提示仍在时，提议的版本若正是启动失败的版本，仍可提议但标记 `previouslyFailed`；用户关闭提示后不再标记。
- **被跳过版本的说明**：版本清单的可选字段 `previousVersion` 写明上一发布（`release-plan.mjs` 取更新记录
  `releases.json` 中的下一项，`release.yml` 以 `--previous-version` 传给发布工具）。旧客户端与 openvulcan-web
  静态同步忽略该字段，因此不递增清单格式。Core 从提议版本起沿该字段回溯，逐个按签名读取清单、按摘要读取说明，
  直到不高于已安装版本，结果以 `skippedReleases`（从新到旧）给出，最多 `MAX_SKIPPED_RELEASES` 个。这些说明只用于
  介绍更新：缺少链接（在该字段之前发布的清单）、某个发布无法读取或达到上限时，回溯结束并把
  `skippedReleasesComplete` 设为 false，提议照常给出，界面提示更早的说明未能显示。回溯依赖旧版本对象长期保留在
  两个下载源中（主源不可变、镜像不删除）。

## 设置

- `preview` 默认关闭，`route` 默认为 `auto`，`autoDownload` 默认开启。默认值只在内存中补全；显式保存时才写入
  `appUpdate`。旧 `checkInterval` 值保留在配置文件中，本版按固定的一小时调度。
- 启用自动下载时，检查发现可安装的签名发布后直接进入下载。WebUI 和 TUI 在准备完成后提示「更新已就绪」；
  手动模式继续提示发现更新并等待下载操作。关闭开关会停止正在进行的自动下载，已准备好的版本仍可由用户安装。
- 保存始终提交完整设置文档。
- 设置页自上而下为：当前版本（检查与修复）、更新设置、找到的更新（下载或重启，以及从提议版本起从新到旧的每个版本说明）。更新对话框与设置页共用同一组版本说明组件。
- WebUI 的「软件更新」下设「软件更新」和「更新记录」两个页签；旧的更新记录入口打开第二个页签。更新记录仍读取随包说明。

## 切换、重启与试运行

1. 就绪时在 `updates/prepared-update.json` 记录目标及原始签名清单，重开后离线复核。启动器仍读取当前版本指针。
   用户确认「更新并重启」时，增量路径才切换当前版本（Windows `current.json`，
   其他平台 `current` 符号链接），并写入 `updates/startup-trial.json`：`version`、`previous`、
   `attempts: 0` 与 `max_attempts`（取自 `MAX_STARTUP_ATTEMPTS`，当前为 2）。
2. 重启会停止所有运行中的后台命令任务：WebUI 在重启前以 `service_running_shell_job_count` 读取数量，大于 0 时
   先给出警告，由用户在警告中选择“仍然重启”；计数失败不阻止更新。用户确认重启后，服务发布 `service.restarting`（`reason` 为 `update`、`installer` 或 `setup`），正常排空后
   以相同参数加 `--await-owner-exit` 分离启动新服务；新服务最多等待 120 秒取得实例锁。在 systemd 用户
   服务中改用 `systemd-run --user` 重新安装用户单元，单元的 `ExecStart` 使用稳定路径 `<root>/current/<rel>`。
   是否运行在该单元中只按 `/proc/self/cgroup` 的叶节点是否为本单元名判断，不看 `INVOCATION_ID`：GNOME Shell、
   plasmashell 等以 systemd 服务运行的桌面外壳会把它传给所启动的应用，桌面端启动的 Core 因此也带有它。
   用户服务只有用户级一种（`--install-user-service`，Linux root 同样用自己的 systemd 用户实例）：Windows 为
   HKCU `Run` 值（Core 每次启动刷新为当前版本路径），macOS 为指向稳定路径的 LaunchAgent。
   WebUI 和 TUI 的确认请求带 `expectedVersion`；就绪版本已被替换时 Core 拒绝该请求。
3. 桌面端等待旧 Core 进程退出后，经稳定入口重新启动（Windows 为根目录启动器 `VulcanCode.exe`，
   其他平台为 `<root>/current/<rel>`）。完整安装时桌面端只退出，安装包以
   `/SILENT /SUPPRESSMSGBOXES /NORESTART /RELAUNCH=1` 运行，结束后由 `[Run]` 中的
   `RelaunchRequested` 启动应用。TUI 与浏览器自动重连。
   新服务、安装包与重新启动的桌面端都不继承旧进程的 `VULCAN_RUNTIME_LAYOUT`：该变量指向旧版本的布局清单，
   继承后新版本会报告新版本号却从旧版本目录提供 WebUI 等资源；新进程改为读取自身可执行文件旁的清单。
   **经安装器完成**（`kind: setup`，事件 `reason` 为 `setup`）：服务排空后启动本版本服务程序的辅助模式
   `--run-setup-update <版本> <通道> <detached|user-unit> <原服务参数…>`（`setup_update.rs`；在 systemd
   用户单元中经 `systemd-run --user` 启动到单元之外）。辅助进程最多等待 `OWNER_EXIT_WAIT` 直到数据根空闲，
   然后无人值守运行安装器（`setup_command`：稳定版为 `update --yes --prefix <根>`，预览版为
   `install --yes --channel preview --prefix <根>`，因为安装器否则只查看安装时记录的通道，这会把安装器记录的
   通道改为预览），输出写入 `updates/setup-update.log`。安装器发现该版本已有安装记录便跳过下载，只切换、
   刷新集成并清理；安装器在应用更新前也会检查自身版本并自动更新。安装器未成功退出或当前版本未达到组装的版本时写入
   `updates/setup-failure.json`（`SETUP_FAILURE_FILE`），下一次服务启动以重启错误报告一次后删除。之后若数据根
   仍空闲（安装器注册用户服务时已启动服务），辅助进程以原参数启动当前版本的服务；原服务在用户单元中时改为
   `--install-user-service`。桌面端等待旧 Core 退出后继续等待下一个 Core（最多 15 分钟），再从稳定入口重新
   启动；TUI 与浏览器自动重连。此路径没有试运行标记，与 Inno 安装包相同。未确认重启时已组装的版本保留为
   待安装版本。
   **经 Windows 兼容入口完成**（`kind: setup` 且根目录不归安装器所有，事件 `reason` 为 `installer`）：
   旧 Core 以 `INSTALLER_ARGUMENTS` 分离启动签名资产。统一安装器从 `updates/installers/<版本资产>` 获取安装根和
   确切目标版本，验证该版本的签名清单，使用已组装的 `versions/<版本>` 或组装缺失的文件，并从根启动器重新打开应用。
   更早的 Core 读取 `downloads` 的 `inno` 项并以相同参数启动 `-setup.exe`，缺少目标版本时由同一入口完成组装。
   历史版本提供的 Inno 资产仍按其原有方式运行。兼容资产随每个目标版本发布，以支持直接跳过中间版本。
4. Windows 启动器每次桌面启动计一次尝试；`previous` 或试运行版本的服务可执行文件（各自 `runtime-layout.json`
   的 `serviceExecutable`）正在运行时本次既不计数也不回退，因为桌面端会复用该 Core，新版本并未被启动
   （运行判断依据 Windows 对运行中映像写访问的共享冲突，其他结果按未运行处理）；`attempts` 达到 `max_attempts` 而标记仍在时，把
   `current.json` 指回 `previous`，写入 `updates/rollback.json`（`failed`、`restored`）并删除试运行标记。
   试运行版本已不是当前版本时（例如随后运行了完整安装包）直接删除标记。服务正持有安装锁时本次不处理。
   上一版本目录已不存在时启动器报错，不猜测其他版本。
5. 新服务启动时：删除属于自己的试运行标记；回退标记的 `restored` 等于自身版本时保留并在快照中提示，
   否则删除；当前版本等于自身时清理旧版本，保留当前版本、最近一个旧版本和已验证的就绪版本。

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
- **入口脚本**：两个脚本先从平台默认目录或 `--prefix` 查找由独立安装器管理的现有安装，找到时直接运行其安装器。
  对未明确选择自更新行为的 `install` 或 `update` 传入 `--self-update`，使旧安装器也能自动升级。首次安装或
  本地安装器缺失时，按 `scripts/entry-scripts.mjs` 填入的下载源（`AUTO_ORIGINS`）、摘要与 Windows 签名证书指纹
  下载并校验。首次下载时 `--route china` 只用中国大陆镜像，`--route international` 只用国际源，
  `--route auto` 或省略参数时先国际源再中国镜像。从镜像获取脚本后，可用
  `curl -fsSL https://static.openvulcan.cn/vulcancode/install.sh | sh -s -- --route china`
  或 `& ([scriptblock]::Create((irm https://static.openvulcan.cn/vulcancode/install.ps1))) --route china`
  让入口脚本和安装器都只走国内源。两个节点提供内容相同的签名脚本。
  Windows 的 `irm | iex` 在安装器成功后将持久用户 PATH 合入当前会话；类 Unix 的 `curl | sh`
  在子 Shell 执行，当前终端继承的 PATH 不会变动。 `promote-setup.yml` 把
  stable 安装器指针所列的脚本按摘要校验后放到主下载源顶层（缓存 60 秒）；openvulcan-web 把 `setup/` 与顶层
  脚本同步到中国镜像。
- **线路选择与自更新**：交互式 `install`、`update` 在访问网络前先选择自动、国际或中国大陆节点，默认高亮
  该安装目录已保存的线路或自动。显式 `--route` 跳过选择页。无人值守沿用显式参数、已保存线路或自动。
  同一线路用于安装器自更新、产品清单及组件下载，保存至安装器的安装记录，并在自动重启时传递给新进程。
  安装器读取通道（preview 同时读 stable）的已签名安装器指针，与自身版本比较。
  有新版本时自动下载、校验、替换并重启，无需确认。显式 `--no-self-update` 跳过。替换后保留原安装参数并用
  `--no-self-update` 防止重复检查。组装前仍按所装版本的 `minSetupVersion` 检查，不满足即停止。
- **Windows 下载及兼容资产**：产品发布验证 stable 安装器指针、安装器摘要与 Authenticode，并探测安装器兼容性。
  `-setup.exe` 为签名的 Windows 图形子系统程序，内嵌同一签名安装器，在独立的临时目录解包并无窗口启动。
  双击打开 Iced 图形界面；旧版更新器调用时保留原资产名称、路径、参数和退出状态。`-update.exe` 直接使用同一份签名安装器。
  图形窗口使用自绘标题栏，启动尺寸按主屏工作区及系统缩放调整。标题栏和面板空白处可拖动，控件和滚动内容保持独立交互。
  交互式终端默认使用 TUI，`--ui gui` 显式进入图形界面，`--yes` 保持无人值守。`--gui` 控制是否安装桌面组件。
- **安装选项**（实现权威 `args.rs`、`integrate.rs`）：所有平台都可选 GUI（`--gui`，Linux 无桌面环境时仍可安装，
  只是无法运行）、开始菜单/应用菜单（`--menu`）、桌面快捷方式（`--desktop-shortcut`）、PATH（`--path`）、用户服务
  （`--service`，调用 Core `--install-user-service`）与 Linux linger（`--linger`，经 `loginctl enable-linger`，
  失败时提示 sudo；只在本程序启用时记录并在卸载时撤销）。端口范围（`--port-range`）调用 Core
  `--set-port-range` 写入 Core 拥有的 `service-settings.json`，安装器不保存副本。旧的 `--mode`、`--no-shortcut`
  继续接受。root 安装与无 systemd 用户实例时给出说明，不注册系统级服务。
- **类 Unix PATH 与命令行安装**：选择 `--path` 后，安装器在用户 Shell 的启动文件加入受管区块，在新 Shell
  导出 `~/.local/bin`。Bash 使用生效的登录配置和 `.bashrc`，Zsh 使用 `.zprofile` 与 `.zshrc`，fish 使用用户的
  `fish/conf.d`，POSIX 登录 Shell 使用 `.profile`。重复安装复用原区块，关闭 PATH 集成或卸载时只移除该安装
  的区块。纯命令行安装结束后显示服务注册、前台启动和停止、适用的服务管理器命令，以及从已安装 Core
  的 `--describe` 读取的 WebUI 端口范围。服务启动输出显示实际绑定地址。
- **首次安装的 LuaSkills Hub**：安装器从已组装 Core 的 `--describe` 取得用户数据目录。登记快捷方式或服务前，
  若安装根属于首次安装且用户尚无 LuaSkills 运行时设置，则按安装线路初始化官方 Hub。国内入口选国内 Hub，国际线路选国际 Hub，
  自动线路跟随这次产品下载选中的来源。已有 LuaSkills 设置、升级与旧安装接管保留原值。Hub 地址和设置格式由
  `vulcan-luaskills-config` 管理。
- **Windows 桌面集成**：GUI 安装时把 `bin/vulcan-code-launcher.exe` 复制为 `<root>\VulcanCode.exe`，快捷方式用
  运行布局（schema 4）的 `desktopIdentity` 写入 AppUserModelID 与通知激活 CLSID，与 Inno 安装包一致；同时把
  安装器放入 PATH 并作为卸载入口的 `DisplayIcon`/卸载命令。完成与 `status` 时告知卸载命令。
- **Windows 接管**：读取固定 Inno AppId 的当前用户注册项，并核对安装位置、卸载程序与当前版本记录。
  GUI、TUI 和旧客户端更新入口复用同一安装操作，保留原目录、用户数据和可识别的集成。写入安装器状态后停用旧卸载程序及注册项。
  新的 `UninstallString` 显式传递安装目录和 `--ui=gui`，`QuietUninstallString` 使用同一目录及 `--yes`。
  卸载仅清除确实指向当前根目录的快捷方式和通知注册。
- **安装记录**：`installation.json` 为 v4，新增 Shell PATH 区块所在文件。读取 v1 至 v3 时先备份原字节到带原版本号的文件，再升级写回。
  只记录本程序创建的文件、目录与系统集成，卸载时只撤销这些项。
- **卸载**：Windows 无法删除运行中的程序，因此确认后、任何改动之前先结束程序位于安装根目录下的进程（桌面应用、
  服务、TUI），结束不了即停止并保持安装完整；当前版本已缺失时直接移除指向本根目录 `versions` 的登录 `Run` 值。
  安装器以文件而非管道接收 Core 的输出：Windows 上 `--install-user-service` 启动的服务继承父进程的输出句柄，
  管道会一直保持打开；同理，Windows 上安装器以显式句柄列表启动 Core，只交出 NUL 与这两个文件（Windows
  PowerShell 等会把其输出的额外可继承副本交给安装器），调用方经管道读取安装器输出时不会被服务挂住。

## Linux 和 macOS 图形安装包

- Linux 从文件管理器运行对应架构的 AppImage。下载后如果文件管理器未保留执行权限，可在文件属性中允许执行。
  运行依赖桌面会话和可用的 FUSE。macOS 从 Finder 打开 DMG，再打开里面的 `Vulcan Code Setup.app`。
- 安装、更新和卸载与终端共用同一个引擎，读取同一套签名发布对象。可以保留图形安装包，重新打开后管理现有安装。
  图形包内嵌同架构的独立安装器，安装时写入现有的 `setup/` 位置，已有的独立安装器由其自身升级流程维护。
  macOS 独立安装器同时包含图形管理器。Linux 图形包额外写入一个管理器，静态终端安装器仍可单独使用。
  便携图形安装包通过下载新版替换。
- 三个平台的图形安装器在变更安装前，按版本布局中的可执行文件路径检查当前安装下的桌面、TUI 与 Core。
  审阅后先提示正常退出，重新检查仍有进程时提供单独的强制关闭确认。强制关闭只针对已确认的进程，
  再次检查全部退出后继续。下载完成、切换版本之前会重复检查。
- 打包时先构建 `vulcan-code-setup`，再以 `VULCAN_SETUP_CONSOLE_IMAGE` 指向同架构的终端产物，
  用 `--features desktop-package --bin vulcan-code-setup-gui` 构建图形产物。Linux 图形产物在 GNU 目标构建。
  `sh scripts/package-setup-gui.sh linux` 接收图形产物、AppImage 输出路径、打包工具和锁定的 AppImage 运行时。
  `sh scripts/package-setup-gui.sh macos` 接收 ARM64、x64 图形产物及 DMG 输出路径，在 macOS 合成为通用安装包。
  常规 macOS 分发使用 Developer ID 签名与 Apple 公证。允许分发未公证包时，需要明确授权并在下载页注明。
- 独立安装器发布到 Preview 后，运行 `release-setup-desktop.yml` 构建 AppImage 和通用 DMG。本工作流仅支持
  临时签名的 DMG，需为当次发布明确启用 `allow_unnotarized`。三个产物全部校验后上传，再把它们的名称、摘要和大小
  写入已签名 Preview 指针的 `desktop` 字段。`promote-setup.yml` 将同一组产物推进到 Stable。

## 安装后的 vcode 命令

- 裸 `vcode` 与不受支持的命令显示帮助及安装管理器版本。用 `vcode tui` 或 `vcode gui` 启动对应客户端。
- `vcode web` 打开经健康探测确认的本机 WebUI，`vcode start` 只启动 Core 并显示网址。两者均复用已运行的 Core，
  冷启动时在桌面会话选择托盘模式，在无桌面会话选择无头模式。`vcode stop` 只停止当前安装目录中的 Core，
  会中断运行中的任务，桌面和终端窗口保持打开。已连接过的新版 TUI 在断线后只尝试重连。
- `vcode manager` 与 `vcode manager tui` 在终端管理安装和服务登记。`vcode manager gui` 打开已安装的图形管理器，
  macOS 从终端命令或 DMG 安装后都可使用，Linux 需要通过 AppImage 安装管理器。
  `vcode update` 更新产品。选择不加入 PATH 时仍可运行安装目录下的 `cli/vcode`（Windows 为
  `setup/vcode.exe`）。此前版本用来升级的 `setup/` 内部程序路径继续保留。

## Linux 桌面应用的 AppArmor 配置

- **原因**：Ubuntu 23.10 及以后（`/proc/sys/kernel/apparmor_restrict_unprivileged_userns` 为 `1`）只允许
  AppArmor 配置许可的程序使用非特权用户命名空间；程序继承启动者的配置，因此桌面应用从桌面图标能启动，
  从程序坞或应用菜单启动时 Electron 沙箱失败退出。唯一不降级的做法是一次性以 root 安装
  `/etc/apparmor.d/vulcan-code`（`flags=(unconfined)` 加 `userns,`），它附着到 `<已解析根>/versions/*/<桌面程序>`，
  因此之后每个版本的应用内更新都无需再次提权。`--no-sandbox` 属于降级，不得使用。
- **唯一实现**：配置文本、路径常量、比较（`Current`/`Missing`/`Outdated`/`Foreign`）与命令都在
  `vulcan-install-core` 的 `apparmor.rs`；标记行 `GENERATED_MARKER` 与布局文件名 `RUNTIME_LAYOUT_FILE` 在
  `install_root.rs`；桌面组件名 `DESKTOP_COMPONENT` 在 `names.rs`。安装器与 Core 写出的字节完全相同。
- **安装器**：安装桌面组件且限制开启时，经系统鉴权安装并记录为 `Item::AppArmorProfile`。终端界面使用
  `sudo`，图形界面使用 `pkexec --disable-internal-agent`，由桌面 Polkit 代理提供授权窗口。授权失败时
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
- Linux 窗口图标来自打包应用 `gui/` 旁的 `icon.png`，macOS 运行中的程序坞图标来自同目录的 `icon-macos.png`
  （`desktop-runtime.mjs` 从 `apps/vulcan-code-service/icons/` 复制，桌面端 `app-icon.ts` 读取）；Windows 图标来自
  程序资源。
- macOS 发行物把 Electron.app 改名为 `desktop/runtime/Vulcan Code.app`（`desktop-runtime.mjs` 的 `MACOS_BUNDLE`），
  按模式改写 `Info.plist` 的名称与标识、以 `icon-macos.png` 生成应用包图标并临时签名；程序坞、菜单栏与退出后
  固定在程序坞的图标都取自该应用包。可执行文件 `Contents/MacOS/Electron` 与辅助应用保留 Electron 名称，Electron
  按这些名称查找辅助进程，不得改名。修改应用包名称会改变 `desktopExecutable`，须递增 `installEpoch`。
- 安装器对象键（`setup_pointer_key`、`setup_file_key`）与产品对象键一样，在 `layout.rs` 与 openvulcan-web 的
  `server/internal/staticsync/format.go` 中各写一次，修改时两处同时改。
- 入口脚本在安装器发布时内嵌下载源；修改 `AUTO_ORIGINS` 后须发布新的安装器版本，脚本才会使用新下载源。

- 试运行与回退标记的文件名、字段在 `install_root.rs` 与启动器 `trial.rs` 中各写一次（启动器只依赖
  `serde_json`），修改时两处同时改；尝试上限只在 `MAX_STARTUP_ATTEMPTS` 定义并随标记写出。
- `/RELAUNCH=1` 仍由 `INSTALLER_ARGUMENTS` 发送，统一安装器通过 `inno::run_legacy_update` 接收；历史 Inno 包由
  `scripts/windows-installer.mjs` 的 `RelaunchRequested` 接收。
- Inno 更新包的文件名在 `vulcan-install-core` 的 `InnoUpdate::file_name` 与 `windows-installer.mjs` 的
  `updateAssetName` 中各写一次，完整安装包的文件名同理在 `DownloadKind::allowed_files` 与 `installerAssetName`；
  `release.yml` 按 `-setup.exe` 与 `-update.exe` 后缀把两者分别交给 `--download inno=` 与 `--inno-update`，
  GitHub Release 只附带完整安装包。更新包是清单的独立可选字段而非下载类型，因为旧客户端会拒绝未知的下载
  类型与文件名，却忽略未知字段；不得把它改为下载类型。安装记录文件名在 `RECORD_FILE_NAME` 与
  `installRecordFile` 中各写一次。修改任一侧时同时修改另一侧。
- 安装器程序名 `SETUP_PROGRAM` 只在 `install_root.rs` 定义，安装器 `integrate.rs` 重新导出它。`setup_command`
  依赖安装器 `update`/`install` 的 `--yes`、`--prefix`、`--channel` 选项，以及“版本目录已有安装记录时跳过下载”
  （安装器 `install.rs` 的 `assemble`）；修改这些行为时同时修改 `setup_command`。
- 重启原因 `setup` 在 `ServiceRestartReason`、WebUI `ServiceRestartReason` 与桌面端 `update-restart.ts`
  中各写一次，修改时三处同时改。
- 启动器、安装器模板、注册表、PATH、快捷方式或安装布局变化时递增 `product.json` 的 `installEpoch`
  （方案第九节），旧安装由此走跨纪元更新（按上文 `install_method` 组装后交由安装器或 Inno 更新包完成，
  必要时退回完整安装包）。安装器自身写入的快捷方式与注册表项随安装器版本发布，不触发
  `installEpoch`；运行布局新增可选字段（如 `desktopIdentity`）而启动器不校验其版本时同样不触发。
- 运行布局的 `desktopIdentity` 由 `scripts/desktop-runtime.mjs` 从桌面端的通知身份常量写出，是安装器读取 AUMID 与
  通知 CLSID 的唯一来源；修改身份值时只改桌面端常量。
- TUI 调用一对一镜像 `apps/vulcan-code-web/src/service/app-update-service.ts`，见
  [`tui-service-adaptation.md`](tui-service-adaptation.md)。
