# make.ps1 provides the PowerShell-native task entry for local build and run workflows.
# make.ps1 用于提供本地构建与运行工作流的 PowerShell 原生入口。
# CommandMode selects the top-level command such as build, run, release, or deps.
# CommandMode 用于选择顶层命令，例如 build、run、release 或 deps。
# CommandVariant carries the optional secondary mode such as release after run or host/lua after deps.
# CommandVariant 用于承接可选的二级模式，例如 run 后面的 release，或 deps 后面的 host/lua。
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

# TauriRunScriptPath points at the new Tauri/WebUI run script.
# TauriRunScriptPath 用于指向新版 Tauri/WebUI 运行脚本。
$TauriRunScriptPath = Join-Path $ScriptDir "scripts\run_tauri.ps1"

# HostDepsScriptPath points at the dedicated PowerShell host-dependency bootstrap script.
# HostDepsScriptPath 用于指向专用的 PowerShell 宿主依赖初始化脚本。
$HostDepsScriptPath = Join-Path $ScriptDir "scripts\install_host_deps.ps1"

# LuaDepsScriptPath points at the dedicated PowerShell Lua dependency bootstrap script.
# LuaDepsScriptPath 用于指向专用的 PowerShell Lua 依赖初始化脚本。
$LuaDepsScriptPath = Join-Path $ScriptDir "scripts\install_lua_deps.ps1"

# RustNoticesScriptPath points at the dedicated PowerShell notice-generation script for the About page.
# RustNoticesScriptPath 用于指向关于页面专用的 Rust 许可证声明生成脚本。
$RustNoticesScriptPath = Join-Path $ScriptDir "scripts\generate_rust_third_party_notices.ps1"

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
        [ValidateSet("host", "vldb", "rtk", "lua")]
        [string]$DependencyKind
    )

    $ScriptPath = switch ($DependencyKind) {
        "host" { $HostDepsScriptPath }
        "vldb" { $HostDepsScriptPath }
        "rtk"  { $HostDepsScriptPath }
        "lua"  { $LuaDepsScriptPath }
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

# Show-Usage prints the supported command forms so contributors can quickly recover from invalid input.
# Show-Usage 用于输出支持的命令形式，方便贡献者在输入无效参数后快速恢复。
function Show-Usage {
    Write-Host "Usage:"
    Write-Host "  ./make             # Tauri/WebUI debug build"
    Write-Host "  ./make build       # Tauri/WebUI debug build"
    Write-Host "  ./make br          # build and run Tauri/WebUI debug"
    Write-Host "  ./make br release  # build and run Tauri/WebUI release"
    Write-Host "  ./make release     # Tauri/WebUI release build"
    Write-Host "  ./make licenses    # generate Rust third-party notices for the About page"
    Write-Host "  ./make run         # run Tauri/WebUI debug build"
    Write-Host "  ./make run release # run Tauri/WebUI release build"
    Write-Host "  ./make deps        # install host + official LuaSkills runtime dependencies"
    Write-Host "  ./make deps host   # install host native dependencies only"
    Write-Host "  ./make deps vldb   # install vldb-controller host dependency only"
    Write-Host "  ./make deps rtk    # install RTK host dependency only"
    Write-Host "  ./make deps lua    # install official LuaSkills runtime dependencies only"
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
    "deps" {
        switch ($NormalizedVariant) {
            "" {
                Invoke-DependencyInstall -DependencyKind "host"
                Invoke-DependencyInstall -DependencyKind "lua"
            }
            "all" {
                Invoke-DependencyInstall -DependencyKind "host"
                Invoke-DependencyInstall -DependencyKind "lua"
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
            "lua" {
                Invoke-DependencyInstall -DependencyKind "lua"
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

