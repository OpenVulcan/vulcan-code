# make.ps1 provides the PowerShell-native task entry for local build and run workflows.
# make.ps1 用于提供本地构建与运行工作流的 PowerShell 原生入口。
# CommandMode selects the top-level command such as sync, vite, build, run, release, or deps.
# CommandMode 用于选择顶层命令，例如 sync、vite、build、run、release 或 deps。
# CommandVariant carries a secondary mode such as a Vite action, release after run, or a dependency kind.
# CommandVariant 用于承接二级模式，例如 Vite 操作、run 后面的 release 或依赖类型。
param(
    [Parameter(Position = 0)]
    [string]$CommandMode = "",
    [Parameter(Position = 1)]
    [string]$CommandVariant = "",
    # RemainingArgs captures trailing command arguments for future command extensions.
    # RemainingArgs 用于承接未来命令扩展所需的尾随参数。
    [Parameter(Position = 2, ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArgs = @()
)

$ErrorActionPreference = "Stop"

# Relaunch under PowerShell 7 when available so the shared script set stays UTF-8 safe across hosts.
# 若检测到 PowerShell 7，则自动在其下重启，确保整套共享脚本在不同宿主下都能稳定处理 UTF-8。
if ($PSVersionTable.PSEdition -ne "Core") {
    $CurrentScriptPath = $MyInvocation.MyCommand.Path
    if (-not [string]::IsNullOrWhiteSpace($PSCommandPath)) {
        $CurrentScriptPath = $PSCommandPath
    }
    $PowerShellCore = Get-Command "pwsh" -ErrorAction SilentlyContinue
    if ($PowerShellCore -and -not [string]::IsNullOrWhiteSpace($CurrentScriptPath)) {
        $ForwardedArguments = @()
        if (-not [string]::IsNullOrWhiteSpace($CommandMode)) {
            $ForwardedArguments += $CommandMode
        }
        if (-not [string]::IsNullOrWhiteSpace($CommandVariant)) {
            $ForwardedArguments += $CommandVariant
        }
        $ForwardedArguments += $RemainingArgs
        & $PowerShellCore.Source -NoProfile -ExecutionPolicy Bypass -File $CurrentScriptPath @ForwardedArguments
        exit $LASTEXITCODE
    }
}

# ScriptDir stores the repository root so child scripts are always resolved from a stable base path.
# ScriptDir 用于保存仓库根目录，确保子脚本始终从稳定的基路径解析。
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition

# TauriBuildScriptPath points at the new Tauri/WebUI build script.
# TauriBuildScriptPath 用于指向新版 Tauri/WebUI 构建脚本。
$TauriBuildScriptPath = Join-Path $ScriptDir "scripts\build_tauri.ps1"

# TauriSyncScriptPath points at the dedicated runtime-resource synchronization script.
# TauriSyncScriptPath 用于指向专用的运行时资源同步脚本。
$TauriSyncScriptPath = Join-Path $ScriptDir "scripts\sync_tauri_runtime.ps1"

# ViteDevServerScriptPath points at the standalone WebUI development-server lifecycle script.
# ViteDevServerScriptPath 用于指向独立 WebUI 开发服务器生命周期脚本。
$ViteDevServerScriptPath = Join-Path $ScriptDir "scripts\vite_dev_server.ps1"

# TauriRunScriptPath points at the new Tauri/WebUI run script.
# TauriRunScriptPath 用于指向新版 Tauri/WebUI 运行脚本。
$TauriRunScriptPath = Join-Path $ScriptDir "scripts\run_tauri.ps1"

# HostDepsScriptPath points at the dedicated PowerShell host-dependency bootstrap script.
# HostDepsScriptPath 用于指向专用的 PowerShell 宿主依赖初始化脚本。
$HostDepsScriptPath = Join-Path $ScriptDir "scripts\install_host_deps.ps1"

# BundledToolsDepsScriptPath points at the latest sg and rg dependency installer.
# BundledToolsDepsScriptPath 用于指向最新版 sg 与 rg 依赖安装器。
$BundledToolsDepsScriptPath = Join-Path $ScriptDir "scripts\install_bundled_tools.ps1"

# LuaDepsScriptPath points at the dedicated PowerShell Lua dependency bootstrap script.
# LuaDepsScriptPath 用于指向专用的 PowerShell Lua 依赖初始化脚本。
$LuaDepsScriptPath = Join-Path $ScriptDir "scripts\install_lua_deps.ps1"

# CodebaseMemoryDepsScriptPath points at the pinned native CBM runtime installer.
# CodebaseMemoryDepsScriptPath 用于指向固定版本的原生 CBM 运行时安装器。
$CodebaseMemoryDepsScriptPath = Join-Path $ScriptDir "scripts\install_codebase_memory_runtime.ps1"

# VmmDepsScriptPath points at the explicit managed VMM dependency preparation script.
# VmmDepsScriptPath 用于指向显式准备托管 VMM 依赖的脚本。
$VmmDepsScriptPath = Join-Path $ScriptDir "scripts\package_vmm_runtime.ps1"

# RustDevToolsScriptPath points at the Rust workspace maintenance-tool installer.
# RustDevToolsScriptPath 用于指向 Rust 工作区维护工具安装器。
$RustDevToolsScriptPath = Join-Path $ScriptDir "scripts\install_rust_dev_tools.ps1"

# RustNoticesScriptPath points at the dedicated PowerShell notice-generation script for the About page.
# RustNoticesScriptPath 用于指向关于页面专用的 Rust 许可证声明生成脚本。
$RustNoticesScriptPath = Join-Path $ScriptDir "scripts\generate_rust_third_party_notices.ps1"

# RustDependencyCheckScriptPath points at the read-only Rust dependency-governance verifier.
# RustDependencyCheckScriptPath 指向只读的 Rust 依赖治理验证脚本。
$RustDependencyCheckScriptPath = Join-Path $ScriptDir "scripts\check_rust_dependency_policy.ps1"

# Normalize-Command converts nullable command text into a trimmed lower-case token so dispatch rules remain predictable.
# Normalize-Command 用于把可空命令文本转换成去空白的小写标记，确保分发规则稳定可预测。
# Value is the raw command-line token to normalize before dispatch.
# Value 表示分发前需要归一化的原始命令行标记。
function Normalize-Command {
    param(
        [AllowNull()]
        [string]$Value
    )

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return ""
    }

    return $Value.Trim().ToLowerInvariant()
}

# Invoke-TauriBuild forwards the current mode to scripts/build_tauri.ps1.
# Invoke-TauriBuild 用于把当前模式转发给 scripts/build_tauri.ps1。
# IsRelease controls whether the Tauri shell should target Cargo release output.
# IsRelease 用于控制 Tauri 桌面壳是否构建 Cargo release 产物。
function Invoke-TauriBuild {
    param(
        [bool]$IsRelease
    )

    if (-not (Test-Path -LiteralPath $TauriBuildScriptPath)) {
        throw "Missing Tauri build script: $TauriBuildScriptPath"
    }

    if ($IsRelease) {
        & $TauriBuildScriptPath -Release
    }
    else {
        & $TauriBuildScriptPath
    }

    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

# Invoke-TauriSync synchronizes runtime resources without compiling the application.
# Invoke-TauriSync 用于在不编译应用的情况下同步运行时资源。
# This function returns no value and exits with the child script status on failure.
# 此函数不返回值，并在子脚本失败时以其状态码退出。
function Invoke-TauriSync {
    if (-not (Test-Path -LiteralPath $TauriSyncScriptPath)) {
        throw "Missing Tauri runtime synchronization script: $TauriSyncScriptPath"
    }

    & $TauriSyncScriptPath
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

# Invoke-ViteDevServer delegates one explicit standalone Vite lifecycle action.
# Invoke-ViteDevServer 用于委托一个显式的独立 Vite 生命周期操作。
# Action selects start, stop, or status and the function exits with the child script status.
# Action 用于选择 start、stop 或 status，此函数以子脚本状态码退出。
function Invoke-ViteDevServer {
    param(
        [ValidateSet("start", "stop", "status")]
        [string]$Action
    )

    if (-not (Test-Path -LiteralPath $ViteDevServerScriptPath)) {
        throw "Missing Vite development-server script: $ViteDevServerScriptPath"
    }

    & $ViteDevServerScriptPath $Action
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

# Invoke-TauriRun forwards the current mode to scripts/run_tauri.ps1.
# Invoke-TauriRun 用于把当前模式转发给 scripts/run_tauri.ps1。
# IsRelease controls whether the runtime should execute a release Tauri shell.
# IsRelease 用于控制运行时是否执行 release Tauri 桌面壳。
function Invoke-TauriRun {
    param(
        [bool]$IsRelease
    )

    if (-not (Test-Path -LiteralPath $TauriRunScriptPath)) {
        throw "Missing Tauri run script: $TauriRunScriptPath"
    }

    if ($IsRelease) {
        & $TauriRunScriptPath -Release
    }
    else {
        & $TauriRunScriptPath
    }

    exit $LASTEXITCODE
}

# Invoke-TauriBuildAndRun builds the Tauri/WebUI shell first and then starts the matching runtime mode.
# Invoke-TauriBuildAndRun 用于先构建 Tauri/WebUI 桌面壳，再启动匹配的运行模式。
# IsRelease controls whether both build and run steps should target release output.
# IsRelease 用于控制构建与运行两个步骤是否都面向 release 产物。
function Invoke-TauriBuildAndRun {
    param(
        [bool]$IsRelease
    )

    Invoke-TauriBuild -IsRelease $IsRelease
    Invoke-TauriRun -IsRelease $IsRelease
}

# Invoke-DependencyInstall delegates dependency bootstrapping to the dedicated PowerShell scripts.
# Invoke-DependencyInstall 用于把依赖初始化委托给专用的 PowerShell 脚本。
# DependencyKind selects which dependency domain should be initialized.
# DependencyKind 用于选择要初始化的依赖域。
function Invoke-DependencyInstall {
    param(
        [ValidateSet("rust-tools", "host", "vldb", "rtk", "tools", "lua", "cbm", "vmm")]
        [string]$DependencyKind
    )

    $ScriptPath = switch ($DependencyKind) {
        "rust-tools" { $RustDevToolsScriptPath }
        "host" { $HostDepsScriptPath }
        "vldb" { $HostDepsScriptPath }
        "rtk"  { $HostDepsScriptPath }
        "tools" { $BundledToolsDepsScriptPath }
        "lua"  { $LuaDepsScriptPath }
        "cbm"  { $CodebaseMemoryDepsScriptPath }
        "vmm"  { $VmmDepsScriptPath }
        default { throw "Unsupported dependency kind: $DependencyKind" }
    }

    if (-not (Test-Path -LiteralPath $ScriptPath)) {
        throw "Missing dependency script: $ScriptPath"
    }

    $PowerShellCommand = Get-Command "pwsh" -ErrorAction SilentlyContinue
    $ScriptArguments = @()
    if ($DependencyKind -eq "vldb" -or $DependencyKind -eq "rtk") {
        $ScriptArguments += $DependencyKind
    }

    if ($PowerShellCommand) {
        & $PowerShellCommand.Source -NoProfile -ExecutionPolicy Bypass -File $ScriptPath @ScriptArguments
    }
    else {
        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $ScriptPath @ScriptArguments
    }
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

# Invoke-SystemInitialization installs Rust tooling first, then initializes every repository-owned dependency domain.
# Invoke-SystemInitialization 先安装 Rust 工具，再初始化仓库拥有的全部依赖域。
# Variant must be empty because init-system is the single ordered bootstrap contract for a fresh environment.
# Variant 必须为空，因为 init-system 是新环境的一键有序初始化契约。
function Invoke-SystemInitialization {
    param(
        [string]$Variant
    )

    if (-not [string]::IsNullOrWhiteSpace($Variant)) {
        throw "Unsupported init-system command: '$Variant'"
    }

    Invoke-DependencyInstall -DependencyKind "rust-tools"
    Invoke-DependencyInstall -DependencyKind "host"
    Invoke-DependencyInstall -DependencyKind "tools"
    Invoke-DependencyInstall -DependencyKind "lua"
    Invoke-DependencyInstall -DependencyKind "cbm"
    Invoke-DependencyInstall -DependencyKind "vmm"

    Write-Host "==> System initialization completed."
}

# Invoke-RustThirdPartyNoticeGeneration delegates About-page Rust notice generation to the dedicated PowerShell script.
# Invoke-RustThirdPartyNoticeGeneration 用于把关于页面的 Rust 声明生成委托给专用的 PowerShell 脚本。
function Invoke-RustThirdPartyNoticeGeneration {
    if (-not (Test-Path -LiteralPath $RustNoticesScriptPath)) {
        throw "Missing Rust notice generation script: $RustNoticesScriptPath"
    }

    & $RustNoticesScriptPath
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

# Invoke-RustDependencyCheck delegates the complete read-only dependency-governance verification.
# Invoke-RustDependencyCheck 用于委托完整的只读依赖治理验证。
function Invoke-RustDependencyCheck {
    if (-not (Test-Path -LiteralPath $RustDependencyCheckScriptPath)) {
        throw "Missing Rust dependency check script: $RustDependencyCheckScriptPath"
    }

    & $RustDependencyCheckScriptPath
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

# Show-Usage prints the supported command forms so contributors can quickly recover from invalid input.
# Show-Usage 用于输出支持的命令形式，方便贡献者在输入无效参数后快速恢复。
function Show-Usage {
    Write-Host "Usage:"
    Write-Host "  ./make sync        # synchronize runtime resources into output"
    Write-Host "  ./make vite start  # start the standalone WebUI Vite dev server"
    Write-Host "  ./make vite stop   # stop the standalone WebUI Vite dev server"
    Write-Host "  ./make vite status # show the standalone WebUI Vite dev server status"
    Write-Host "  ./make             # Tauri/WebUI debug build"
    Write-Host "  ./make build       # Tauri/WebUI debug build"
    Write-Host "  ./make br          # build and run Tauri/WebUI debug; Vite is managed separately"
    Write-Host "  ./make br release  # build and run Tauri/WebUI release"
    Write-Host "  ./make release     # Tauri/WebUI release build"
    Write-Host "  ./make licenses    # generate Rust third-party notices for the About page"
    Write-Host "  ./make check-deps  # verify Rust dependency policy without changing manifests or the lockfile"
    Write-Host "  ./make run         # run Tauri/WebUI debug build; Vite is managed separately"
    Write-Host "  ./make run release # run Tauri/WebUI release build"
    Write-Host "  ./make deps        # install host + LuaSkills + Codebase Memory + VMM runtime dependencies"
    Write-Host "  ./make deps rust-tools # install cargo-hakari, cargo-nextest, and cargo-deny"
    Write-Host "  ./make init-system # bootstrap Rust tools and all repository dependencies"
    Write-Host "  ./make deps host   # install host native dependencies only"
    Write-Host "  ./make deps vldb   # install vldb-controller host dependency only"
    Write-Host "  ./make deps rtk    # install RTK host dependency only"
    Write-Host "  ./make deps tools  # install the latest bundled sg and rg tools"
    Write-Host "  ./make deps lua    # install official LuaSkills runtime dependencies only"
    Write-Host "  ./make deps cbm    # install the pinned native Codebase Memory runtime only"
    Write-Host "  ./make deps vmm    # build and install the managed VMM runtime dependency only"
}

# NormalizedMode stores the canonical top-level command token used by the dispatcher below.
# NormalizedMode 用于保存下方分发器使用的规范化顶层命令标记。
$NormalizedMode = Normalize-Command -Value $CommandMode

# NormalizedVariant stores the canonical secondary token used to distinguish release runs from default runs.
# NormalizedVariant 用于保存规范化的二级标记，以区分 release 运行与默认运行。
$NormalizedVariant = Normalize-Command -Value $CommandVariant

if ($RemainingArgs.Count -gt 0) {
    Write-Error ("Unsupported trailing arguments: {0}" -f ($RemainingArgs -join " "))
    Show-Usage
    exit 1
}

switch ($NormalizedMode) {
    "" {
        Invoke-TauriBuild -IsRelease $false
    }
    "build" {
        if ($NormalizedVariant -eq "" -or $NormalizedVariant -eq "release") {
            Invoke-TauriBuild -IsRelease ($NormalizedVariant -eq "release")
        }
        else {
            Write-Error "Unsupported build command: '$CommandVariant'"
            Show-Usage
            exit 1
        }
    }
    "sync" {
        if (-not [string]::IsNullOrWhiteSpace($NormalizedVariant)) {
            Write-Error "Unsupported sync command: '$CommandVariant'"
            Show-Usage
            exit 1
        }
        Invoke-TauriSync
    }
    "vite" {
        if ($NormalizedVariant -eq "start" -or $NormalizedVariant -eq "stop" -or $NormalizedVariant -eq "status") {
            Invoke-ViteDevServer -Action $NormalizedVariant
        }
        else {
            Write-Error "Unsupported vite command: '$CommandVariant'"
            Show-Usage
            exit 1
        }
    }
    "release" {
        Invoke-TauriBuild -IsRelease $true
    }
    "br" {
        if ($NormalizedVariant -eq "" -or $NormalizedVariant -eq "release") {
            Invoke-TauriBuildAndRun -IsRelease ($NormalizedVariant -eq "release")
        }
        else {
            Write-Error "Unsupported br command: '$CommandVariant'"
            Show-Usage
            exit 1
        }
    }
    "licenses" {
        if (-not [string]::IsNullOrWhiteSpace($NormalizedVariant)) {
            Write-Error "Unsupported licenses command: '$CommandVariant'"
            Show-Usage
            exit 1
        }
        Invoke-RustThirdPartyNoticeGeneration
    }
    "check-deps" {
        if (-not [string]::IsNullOrWhiteSpace($NormalizedVariant)) {
            Write-Error "Unsupported check-deps command: '$CommandVariant'"
            Show-Usage
            exit 1
        }
        Invoke-RustDependencyCheck
    }
    "run" {
        if ($NormalizedVariant -eq "" -or $NormalizedVariant -eq "release") {
            Invoke-TauriRun -IsRelease ($NormalizedVariant -eq "release")
        }
        else {
            Write-Error "Unsupported run command: '$CommandVariant'"
            Show-Usage
            exit 1
        }
    }
    "init-system" {
        Invoke-SystemInitialization -Variant $NormalizedVariant
    }
    "deps" {
        switch ($NormalizedVariant) {
            "" {
                Invoke-DependencyInstall -DependencyKind "host"
                Invoke-DependencyInstall -DependencyKind "tools"
                Invoke-DependencyInstall -DependencyKind "lua"
                Invoke-DependencyInstall -DependencyKind "cbm"
                Invoke-DependencyInstall -DependencyKind "vmm"
            }
            "all" {
                Invoke-DependencyInstall -DependencyKind "host"
                Invoke-DependencyInstall -DependencyKind "tools"
                Invoke-DependencyInstall -DependencyKind "lua"
                Invoke-DependencyInstall -DependencyKind "cbm"
                Invoke-DependencyInstall -DependencyKind "vmm"
            }
            "host" {
                Invoke-DependencyInstall -DependencyKind "host"
            }
            "vldb" {
                Invoke-DependencyInstall -DependencyKind "vldb"
            }
            "rtk" {
                Invoke-DependencyInstall -DependencyKind "rtk"
            }
            "rust-tools" {
                Invoke-DependencyInstall -DependencyKind "rust-tools"
            }
            "tools" {
                Invoke-DependencyInstall -DependencyKind "tools"
            }
            "lua" {
                Invoke-DependencyInstall -DependencyKind "lua"
            }
            "cbm" {
                Invoke-DependencyInstall -DependencyKind "cbm"
            }
            "vmm" {
                Invoke-DependencyInstall -DependencyKind "vmm"
            }
            default {
                Write-Error "Unsupported deps command: '$CommandVariant'"
                Show-Usage
                exit 1
            }
        }
    }
    default {
        Write-Error "Unsupported command: '$CommandMode'"
        Show-Usage
        exit 1
    }
}

