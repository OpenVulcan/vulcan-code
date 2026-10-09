# build_tauri.ps1 builds the Core, WebUI, desktop, and shared Chromium distribution.
# build_tauri.ps1 构建 Core、WebUI、桌面及共用 Chromium 发行物。
# Usage:
# 用法：
#   .\build_tauri.ps1           # debug build
#   .\build_tauri.ps1 -Release  # release build
#
# This legacy filename is retained by make; the service and workbench are separate processes.
# make 保留此旧文件名；服务和工作台是独立进程。

param(
    [switch]$Release,
    # Restart authorizes releasing this distribution's processes after successful compilation.
    # Restart 授权在编译成功后释放本发行实例的进程。
    [switch]$Restart
)

$ErrorActionPreference = "Stop"

# Relaunch under PowerShell 7 when available so frontend and Rust commands share UTF-8 behavior.
# 若检测到 PowerShell 7，则自动在其下重启，确保前端与 Rust 命令共享稳定的 UTF-8 行为。
if ($PSVersionTable.PSEdition -ne "Core") {
    $CurrentScriptPath = $MyInvocation.MyCommand.Path
    if (-not [string]::IsNullOrWhiteSpace($PSCommandPath)) {
        $CurrentScriptPath = $PSCommandPath
    }
    $PowerShellCore = Get-Command "pwsh" -ErrorAction SilentlyContinue
    if ($PowerShellCore -and -not [string]::IsNullOrWhiteSpace($CurrentScriptPath)) {
        $ForwardedArguments = @()
        if ($Release) {
            $ForwardedArguments += "-Release"
        }
        if ($Restart) { $ForwardedArguments += "-Restart" }
        & $PowerShellCore.Source -NoProfile -ExecutionPolicy Bypass -File $CurrentScriptPath @ForwardedArguments
        exit $LASTEXITCODE
    }
}

# ProjectDir stores the repository root for stable child command paths.
# ProjectDir 保存仓库根目录，确保子命令路径稳定。
$ProjectDir = Split-Path $PSScriptRoot -Parent

# Load the shared successful-build artifact tracker for both Cargo profiles.
# 加载两个 Cargo 模式共用的成功构建制品跟踪器。
. (Join-Path $PSScriptRoot 'cargo_artifact_cleanup.ps1')
. (Join-Path $PSScriptRoot 'distribution_processes.ps1')

# WebAppDir points at the Vite React WebUI project.
# WebAppDir 指向 Vite React WebUI 工程。
$WebAppDir = Join-Path $ProjectDir "apps\vulcan-code-web"

# DesktopAppDir points at the Electron GUI and browser-host package.
# DesktopAppDir 指向 Electron GUI 与浏览器宿主包。
$DesktopAppDir = Join-Path $ProjectDir "apps\vulcan-code-desktop"

# TauriAppDir points at the Tauri desktop shell crate.
# TauriAppDir 指向 Tauri 桌面壳 crate。
$TauriAppDir = Join-Path $ProjectDir "apps\vulcan-code-service"

# IsWindowsPlatform records whether the current build target is the Windows host platform.
# IsWindowsPlatform 记录当前构建目标是否为 Windows 宿主平台。
$IsWindowsPlatform = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)

Set-Location $ProjectDir

# Validate the lock even when release dependencies were installed previously.
# 即使发布依赖以前已安装，也必须验证锁文件。
if ($Release -or -not (Test-Path -LiteralPath (Join-Path $WebAppDir "node_modules"))) {
    pnpm --dir $WebAppDir install --frozen-lockfile
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
if ($Release) {

    # Release builds package the static WebUI assets into the desktop shell.
    # Release 构建会把静态 WebUI 产物打包进桌面壳。
    Write-Host "==> Building WebUI for release..."
    pnpm --dir $WebAppDir run build
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }

    # CargoBuildArguments selects the release shell, terminal host and terminal client from the locked registry graph.
    # CargoBuildArguments 从锁定的注册表依赖图选择 release 桌面壳、终端宿主与终端客户端。
    $CargoBuildArguments = @('build', '--locked', '-p', 'vulcan-code-service', '-p', 'vulcan-terminal-host', '-p', 'vulcan-judge-host', '-p', 'vulcan-code-tui', '--release')
}
else {
    # CargoBuildArguments selects the debug shell, terminal host and terminal client from the locked registry graph.
    # CargoBuildArguments 从锁定的注册表依赖图选择 debug 桌面壳、终端宿主与终端客户端。
    $CargoBuildArguments = @('build', '--locked', '-p', 'vulcan-code-service', '-p', 'vulcan-terminal-host', '-p', 'vulcan-judge-host', '-p', 'vulcan-code-tui')
}

# BuildProfileName names the selected artifact profile in diagnostics.
# BuildProfileName 在诊断中标识所选制品 profile。
$BuildProfileName = if ($Release) { 'release' } else { 'debug' }
# Debug also ships static WebUI for direct Core access; the GUI still selects Vite in debug.
# 调试同样携带静态网页以支持直接访问 Core；调试 GUI 仍选择 Vite。
if (-not $Release) {
    pnpm --dir $WebAppDir run build
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

# The packaged runtime must always consume desktop JavaScript built from the current source tree.
# 打包运行时必须始终使用由当前源码树构建的桌面 JavaScript。
if ($Release -or -not (Test-Path -LiteralPath (Join-Path $DesktopAppDir "node_modules"))) {
    Write-Host "==> Installing desktop dependencies..."
    pnpm --dir $DesktopAppDir install --frozen-lockfile
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
Write-Host "==> Building Electron desktop..."
pnpm --dir $DesktopAppDir run build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Building Vulcan Core $BuildProfileName..."
# CargoBuildResult retains the exit status and authoritative artifact paths for cleanup and publishing.
# CargoBuildResult 保留退出状态及权威制品路径，供清理和发布使用。
$CargoBuildResult = Invoke-TrackedCargoBuild -BuildArguments $CargoBuildArguments
if ($CargoBuildResult.ExitCode -ne 0) {
    exit $CargoBuildResult.ExitCode
}

# BinaryFileName stores the platform-specific Tauri shell binary name.
# BinaryFileName 保存平台相关的 Tauri 桌面壳二进制名称。
$BinaryFileName = "vulcan-code-service"
if ($IsWindowsPlatform) {
    $BinaryFileName = "vulcan-code-service.exe"
}

# BuiltBinaryPath uses Cargo's executable path, including configured target triples.
# BuiltBinaryPath 使用 Cargo 的可执行文件路径，包括配置的目标三元组。
$BuiltBinaryPath = [string]($CargoBuildResult.Artifacts | Where-Object { $_.target.name -eq 'vulcan-code-service' -and $_.target.kind -contains 'bin' }).executable

# TerminalHostFileName stores the platform-specific isolated terminal-host binary name.
# TerminalHostFileName 保存平台相关的隔离终端宿主二进制名称。
$TerminalHostFileName = "vulcan-terminal-host"
if ($IsWindowsPlatform) {
    $TerminalHostFileName = "vulcan-terminal-host.exe"
}

# BuiltTerminalHostPath stores the terminal host built for the shell's Cargo profile.
# BuiltTerminalHostPath 保存与桌面壳 Cargo profile 一致的终端宿主制品路径。
$BuiltTerminalHostPath = [string]($CargoBuildResult.Artifacts | Where-Object { $_.target.name -eq 'vulcan-terminal-host' -and $_.target.kind -contains 'bin' }).executable

# JudgeHostFileName stores the platform-specific isolated permission-judge worker name.
# JudgeHostFileName 保存平台相关的隔离权限判定 worker 名称。
$JudgeHostFileName = "vulcan-judge-host"
if ($IsWindowsPlatform) {
    $JudgeHostFileName = "vulcan-judge-host.exe"
}

# BuiltJudgeHostPath stores the judge worker built for the shell's Cargo profile.
# BuiltJudgeHostPath 保存与桌面壳 Cargo profile 一致的判定 worker 制品路径。
$BuiltJudgeHostPath = [string]($CargoBuildResult.Artifacts | Where-Object { $_.target.name -eq 'vulcan-judge-host' -and $_.target.kind -contains 'bin' }).executable

# TuiFileName stores the platform-specific terminal client binary name.
# TuiFileName 保存平台相关的终端客户端二进制名称。
$TuiFileName = "vulcan-tui"
if ($IsWindowsPlatform) {
    $TuiFileName = "vulcan-tui.exe"
}

# BuiltTuiPath stores the terminal client built for the shell's Cargo profile.
# BuiltTuiPath 保存与桌面壳 Cargo profile 一致的终端客户端制品路径。
$BuiltTuiPath = [string]($CargoBuildResult.Artifacts | Where-Object { $_.target.name -eq 'vulcan-tui' -and $_.target.kind -contains 'bin' }).executable

# DistributionRoot selects the same validated output boundary as sync and desktop packaging.
# DistributionRoot 选择与同步和桌面打包相同的已验证输出边界。
$DistributionRoot = (& node (Join-Path $PSScriptRoot "desktop-runtime.mjs") root | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or -not [IO.Path]::IsPathFullyQualified($DistributionRoot)) {
    throw "Could not resolve the desktop distribution root."
}
# OutputBinDir stores the runtime executables beneath the selected distribution root.
# OutputBinDir 保存选定发行根目录下的运行时可执行文件。
$OutputBinDir = Join-Path $DistributionRoot "bin"

# OutputBinaryPath stores the copied executable path that users and run scripts should launch.
# OutputBinaryPath 保存用户与运行脚本应启动的已复制可执行文件路径。
$OutputBinaryPath = Join-Path $OutputBinDir $BinaryFileName

# OutputTerminalHostPath stores the isolated terminal host beside the shell binary.
# OutputTerminalHostPath 保存复制到桌面壳旁边的隔离终端宿主路径。
$OutputTerminalHostPath = Join-Path $OutputBinDir $TerminalHostFileName

# OutputJudgeHostPath stores the isolated judge worker beside the shell binary.
# OutputJudgeHostPath 保存复制到桌面壳旁边的隔离判定 worker 路径。
$OutputJudgeHostPath = Join-Path $OutputBinDir $JudgeHostFileName

# OutputTuiPath stores the terminal client the tray opens, beside the shell binary.
# OutputTuiPath 保存托盘打开的终端客户端路径，位于桌面壳旁边。
$OutputTuiPath = Join-Path $OutputBinDir $TuiFileName

if (-not (Test-Path -LiteralPath $BuiltBinaryPath)) {
    throw "Built Core binary was not found: $BuiltBinaryPath"
}
if (-not (Test-Path -LiteralPath $BuiltTerminalHostPath)) {
    throw "Built terminal host was not found: $BuiltTerminalHostPath"
}
if (-not (Test-Path -LiteralPath $BuiltJudgeHostPath)) {
    throw "Built judge host was not found: $BuiltJudgeHostPath"
}
if (-not (Test-Path -LiteralPath $BuiltTuiPath)) {
    throw "Built terminal client was not found: $BuiltTuiPath"
}

# Publish compiled executables into the stable output directory used by make run.
# 把已编译可执行文件发布到 make run 使用的稳定 output 目录。
if ($IsWindowsPlatform) {
    Prepare-DistributionPublication -DistributionRoot $DistributionRoot -Restart:$Restart
}
New-Item -ItemType Directory -Path $OutputBinDir -Force | Out-Null
# ObsoleteWindowsConsoleHelperPath identifies the removed pre-unification terminal executable.
# ObsoleteWindowsConsoleHelperPath 标识跨平台统一前已移除的旧终端可执行文件。
$ObsoleteWindowsConsoleHelperPath = Join-Path $OutputBinDir "vulcan-windows-console-helper.exe"
if (Test-Path -LiteralPath $ObsoleteWindowsConsoleHelperPath) {
    # Removing this generated artifact prevents stale dual-host layouts after an in-place upgrade.
    # 删除此生成制品可避免原地升级后继续残留双宿主目录布局。
    Remove-Item -LiteralPath $ObsoleteWindowsConsoleHelperPath -Force
}
# Copy failures abort publication immediately.
# 复制失败时立即终止发布。
Copy-Item -LiteralPath $BuiltBinaryPath -Destination $OutputBinaryPath -Force -ErrorAction Stop
Copy-Item -LiteralPath $BuiltTerminalHostPath -Destination $OutputTerminalHostPath -Force -ErrorAction Stop
Copy-Item -LiteralPath $BuiltJudgeHostPath -Destination $OutputJudgeHostPath -Force -ErrorAction Stop
Copy-Item -LiteralPath $BuiltTuiPath -Destination $OutputTuiPath -Force -ErrorAction Stop
& node (Join-Path $PSScriptRoot 'desktop-runtime.mjs') package $BuildProfileName
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "==> Distribution published: $DistributionRoot"
