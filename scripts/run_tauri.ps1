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

Set-Location $ProjectDir

# RunningOnWindows records whether Windows-specific process and listener controls are available.
# RunningOnWindows 记录当前是否可用 Windows 专用进程与监听控制。
$RunningOnWindows = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)

# BinaryFileName stores the platform-specific Tauri shell binary name.
# BinaryFileName 保存平台相关的 Tauri 桌面壳二进制名称。
$BinaryFileName = "vulcan-code-tauri"
if ($RunningOnWindows) {
    $BinaryFileName = "vulcan-code-tauri.exe"
}

# Test-WindowsTcpListenerOwner verifies that a loopback listener belongs to the exact process started by this run.
# Test-WindowsTcpListenerOwner 验证回环监听器是否归属于本次运行启动的准确进程。
# Address identifies the loopback address to match in native netstat output.
# Address 指定需要在原生 netstat 输出中匹配的回环地址。
# Port identifies the required TCP listener port.
# Port 指定必须验证的 TCP 监听端口。
# OwnerProcessId identifies the only process allowed to own the listener.
# OwnerProcessId 指定唯一允许持有该监听器的进程。
# Returns true only when netstat reports the requested listener and exact owner process id.
# 仅当 netstat 报告目标监听器且进程标识完全一致时返回 true。
function Test-WindowsTcpListenerOwner {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Address,
        [Parameter(Mandatory = $true)]
        [int]$Port,
        [Parameter(Mandatory = $true)]
        [int]$OwnerProcessId
    )

    # NetstatPath points at the native executable and avoids CIM/WMI process enumeration entirely.
    # NetstatPath 指向原生可执行文件，完全避免 CIM/WMI 进程枚举。
    $NetstatPath = Join-Path $env:SystemRoot "System32\netstat.exe"
    # ExpectedEndpoint is the exact local endpoint emitted for the configured Vite listener.
    # ExpectedEndpoint 是配置后 Vite 监听器输出的准确本地端点。
    $ExpectedEndpoint = "${Address}:$Port"
    # NetstatLines captures the bounded native TCP table used for exact ownership validation.
    # NetstatLines 捕获用于准确归属验证的有界原生 TCP 表。
    $NetstatLines = @(& $NetstatPath -ano -p tcp)

    foreach ($Line in $NetstatLines) {
        # Columns normalizes locale-independent endpoint and pid columns from one TCP row.
        # Columns 从单个 TCP 行中标准化与区域无关的端点与进程列。
        $Columns = @($Line.Trim() -split "\s+")
        if (
            $Columns.Count -ge 5 -and
            $Columns[0] -eq "TCP" -and
            $Columns[1] -eq $ExpectedEndpoint -and
            $Columns[2] -eq "0.0.0.0:0" -and
            $Columns[-1] -eq [string]$OwnerProcessId
        ) {
            return $true
        }
    }

    return $false
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

# NodeCommand resolves the exact Node.js runtime used to launch Vite without an intermediate pnpm process tree.
# NodeCommand 解析直接启动 Vite 的 Node.js 运行时，避免产生中间 pnpm 进程树。
$NodeCommand = Get-Command "node.exe" -ErrorAction SilentlyContinue
if (-not $NodeCommand) {
    $NodeCommand = Get-Command "node" -ErrorAction SilentlyContinue
}
if (-not $NodeCommand) {
    throw "Node.js not found. Install Node.js before running the Tauri WebUI."
}

if (-not (Test-Path -LiteralPath (Join-Path $WebAppDir "node_modules"))) {
    Write-Host "==> Installing WebUI dependencies..."
    & $PnpmCommand.Source --dir $WebAppDir install
}

# ViteEntryPath points at the project-local Vite CLI so the tracked process is the actual dev server.
# ViteEntryPath 指向项目本地 Vite CLI，确保被跟踪的进程就是实际开发服务器。
$ViteEntryPath = Join-Path $WebAppDir "node_modules\vite\bin\vite.js"
if (-not (Test-Path -LiteralPath $ViteEntryPath)) {
    throw "Missing project-local Vite entrypoint: $ViteEntryPath. Run pnpm --dir $WebAppDir install first."
}

# ViteProcess stores the background frontend dev server process for the debug Tauri shell.
# ViteProcess 保存 debug Tauri 桌面壳使用的前端开发服务器后台进程。
$ViteProcess = $null

# RunExitCode defaults to failure until the output binary exits with a concrete status.
# RunExitCode 在 output 二进制返回明确状态前默认表示失败。
$RunExitCode = 1

try {
    Write-Host "==> Starting WebUI dev server at http://127.0.0.1:17601 ..."
    $ViteProcess = Start-Process `
        -FilePath $NodeCommand.Source `
        -ArgumentList @("`"$ViteEntryPath`"", "--host", "127.0.0.1", "--port", "17601", "--strictPort") `
        -WorkingDirectory $WebAppDir `
        -WindowStyle Hidden `
        -PassThru

    # Wait for the dev server port to become reachable before launching Tauri.
    # 等待开发服务器端口可连接后再启动 Tauri。
    $Deadline = (Get-Date).AddSeconds(20)
    do {
        Start-Sleep -Milliseconds 250
        if ($ViteProcess.HasExited) {
            throw "WebUI dev server exited before port 17601 became ready. The port may already be occupied by a stale process."
        }
        $PortReady = $false
        try {
            $TcpClient = [System.Net.Sockets.TcpClient]::new()
            $ConnectTask = $TcpClient.ConnectAsync("127.0.0.1", 17601)
            $PortReady = $ConnectTask.Wait(250) -and $TcpClient.Connected
            $TcpClient.Dispose()
            if ($PortReady -and $RunningOnWindows) {
                $PortReady = Test-WindowsTcpListenerOwner -Address "127.0.0.1" -Port 17601 -OwnerProcessId $ViteProcess.Id
            }
        }
        catch {
            $PortReady = $false
        }
    } while (-not $PortReady -and (Get-Date) -lt $Deadline)

    if (-not $PortReady) {
        throw "WebUI dev server did not become ready at http://127.0.0.1:17601"
    }

    Write-Host "==> Running Vulcan Code Tauri debug from output/bin..."
    & $OutputBinaryPath
    $RunExitCode = $LASTEXITCODE
}
finally {
    if ($null -ne $ViteProcess -and -not $ViteProcess.HasExited) {
        Stop-Process -Id $ViteProcess.Id -Force
    }
}

exit $RunExitCode
