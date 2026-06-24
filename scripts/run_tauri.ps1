# run_tauri.ps1 runs the new Tauri/WebUI main application.
# run_tauri.ps1 用于运行新的 Tauri/WebUI 主应用。
# Usage:
# 用法：
#   .\run_tauri.ps1           # run debug build with Vite dev server
#   .\run_tauri.ps1 -Release  # run release build from built WebUI assets

param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"

# Relaunch under PowerShell 7 when available so background process handling stays predictable.
# 若检测到 PowerShell 7，则自动在其下重启，确保后台进程处理行为可预测。
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
        & $PowerShellCore.Source -NoProfile -ExecutionPolicy Bypass -File $CurrentScriptPath @ForwardedArguments
        exit $LASTEXITCODE
    }
}

# ProjectDir stores the repository root for stable child command paths.
# ProjectDir 保存仓库根目录，确保子命令路径稳定。
$ProjectDir = Split-Path $PSScriptRoot -Parent

# WebAppDir points at the Vite React WebUI project.
# WebAppDir 指向 Vite React WebUI 工程。
$WebAppDir = Join-Path $ProjectDir "apps\vulcan-code-web"

# WebAppDirPattern is used to locate child Vite processes for cleanup.
# WebAppDirPattern 用于定位需要清理的 Vite 子进程。
$WebAppDirPattern = "*$WebAppDir*"

Set-Location $ProjectDir

# BinaryFileName stores the platform-specific Tauri shell binary name.
# BinaryFileName 保存平台相关的 Tauri 桌面壳二进制名称。
$BinaryFileName = "vulcan-code-tauri"
if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
    $BinaryFileName = "vulcan-code-tauri.exe"
}

# OutputBinDir stores the stable runtime binary output directory produced by make build.
# OutputBinDir 保存 make build 产出的稳定运行时二进制输出目录。
$OutputBinDir = Join-Path $ProjectDir "output\bin"

# OutputBinaryPath stores the exact executable that this script launches.
# OutputBinaryPath 保存本脚本启动的确切可执行文件路径。
$OutputBinaryPath = Join-Path $OutputBinDir $BinaryFileName

if (-not (Test-Path -LiteralPath $OutputBinaryPath)) {
    # BuildCommand tells the user which build mode should produce the missing runtime executable.
    # BuildCommand 告知用户应使用哪种构建模式产出缺失的运行时可执行文件。
    $BuildCommand = if ($Release) { "./make.ps1 build release" } else { "./make.ps1 build" }
    throw "Missing built Tauri binary: $OutputBinaryPath. Run $BuildCommand first."
}

if ($Release) {
    Write-Host "==> Running Vulcan Code Tauri release from output/bin..."
    & $OutputBinaryPath
    exit $LASTEXITCODE
}

# PnpmCommand resolves the pnpm executable used for the debug frontend dev server.
# PnpmCommand 解析 debug 前端开发服务器使用的 pnpm 可执行文件。
$PnpmCommand = Get-Command "pnpm.cmd" -ErrorAction SilentlyContinue
if (-not $PnpmCommand) {
    $PnpmCommand = Get-Command "pnpm" -ErrorAction SilentlyContinue
}
if (-not $PnpmCommand) {
    throw "pnpm not found. Install pnpm before running the Tauri WebUI."
}

if (-not (Test-Path -LiteralPath (Join-Path $WebAppDir "node_modules"))) {
    Write-Host "==> Installing WebUI dependencies..."
    & $PnpmCommand.Source --dir $WebAppDir install
}

# ViteProcess stores the background frontend dev server process for the debug Tauri shell.
# ViteProcess 保存 debug Tauri 桌面壳使用的前端开发服务器后台进程。
$ViteProcess = $null

# RunExitCode defaults to failure until the output binary exits with a concrete status.
# RunExitCode 在 output 二进制返回明确状态前默认表示失败。
$RunExitCode = 1

# ViteStartedProcessIds records existing WebUI node processes before launch.
# ViteStartedProcessIds 记录启动前已经存在的 WebUI node 进程。
$ViteStartedProcessIds = @(
    Get-CimInstance Win32_Process |
        Where-Object { $_.Name -eq "node.exe" -and $_.CommandLine -like $WebAppDirPattern } |
        Select-Object -ExpandProperty ProcessId
)

try {
    Write-Host "==> Starting WebUI dev server at http://127.0.0.1:5173 ..."
    $ViteProcess = Start-Process `
        -FilePath $PnpmCommand.Source `
        -ArgumentList @("--dir", $WebAppDir, "run", "dev", "--host", "127.0.0.1", "--port", "5173") `
        -WorkingDirectory $ProjectDir `
        -WindowStyle Hidden `
        -PassThru

    # Wait for the dev server port to become reachable before launching Tauri.
    # 等待开发服务器端口可连接后再启动 Tauri。
    $Deadline = (Get-Date).AddSeconds(20)
    do {
        Start-Sleep -Milliseconds 250
        $PortReady = $false
        try {
            $TcpClient = [System.Net.Sockets.TcpClient]::new()
            $ConnectTask = $TcpClient.ConnectAsync("127.0.0.1", 5173)
            $PortReady = $ConnectTask.Wait(250) -and $TcpClient.Connected
            $TcpClient.Dispose()
        }
        catch {
            $PortReady = $false
        }
    } while (-not $PortReady -and (Get-Date) -lt $Deadline)

    if (-not $PortReady) {
        throw "WebUI dev server did not become ready at http://127.0.0.1:5173"
    }

    Write-Host "==> Running Vulcan Code Tauri debug from output/bin..."
    & $OutputBinaryPath
    $RunExitCode = $LASTEXITCODE
}
finally {
    if ($null -ne $ViteProcess -and -not $ViteProcess.HasExited) {
        Stop-Process -Id $ViteProcess.Id -Force
    }

    # Stop only WebUI node children created by this script so unrelated node processes survive.
    # 只停止本脚本创建的 WebUI node 子进程，避免影响无关 node 进程。
    Get-CimInstance Win32_Process |
        Where-Object {
            $_.Name -eq "node.exe" -and
            $_.CommandLine -like $WebAppDirPattern -and
            $ViteStartedProcessIds -notcontains $_.ProcessId
        } |
        ForEach-Object {
            Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue
        }
}

exit $RunExitCode
