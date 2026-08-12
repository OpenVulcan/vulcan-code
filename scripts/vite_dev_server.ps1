# vite_dev_server.ps1 manages the standalone WebUI Vite development server.
# vite_dev_server.ps1 用于管理独立的 WebUI Vite 开发服务器。

param(
    # Action selects the exact lifecycle operation performed by this invocation.
    # Action 用于选择本次调用执行的精确生命周期操作。
    [Parameter(Position = 0, Mandatory = $true)]
    [ValidateSet("start", "stop", "status")]
    [string]$Action
)

$ErrorActionPreference = "Stop"

# Relaunch under PowerShell 7 when available so process and UTF-8 behavior remains predictable.
# 若检测到 PowerShell 7，则自动在其下重启，使进程与 UTF-8 行为保持稳定可预测。
if ($PSVersionTable.PSEdition -ne "Core") {
    # CurrentScriptPath stores the exact script path forwarded to PowerShell 7.
    # CurrentScriptPath 保存转发给 PowerShell 7 的精确脚本路径。
    $CurrentScriptPath = $MyInvocation.MyCommand.Path
    if (-not [string]::IsNullOrWhiteSpace($PSCommandPath)) {
        $CurrentScriptPath = $PSCommandPath
    }
    # PowerShellCore stores the discovered PowerShell 7 executable, when available.
    # PowerShellCore 保存检测到的 PowerShell 7 可执行文件（如果存在）。
    $PowerShellCore = Get-Command "pwsh" -ErrorAction SilentlyContinue
    if ($PowerShellCore -and -not [string]::IsNullOrWhiteSpace($CurrentScriptPath)) {
        & $PowerShellCore.Source -NoProfile -ExecutionPolicy Bypass -File $CurrentScriptPath $Action
        exit $LASTEXITCODE
    }
}

# ProjectDir stores the repository root for stable project and runtime-state paths.
# ProjectDir 保存仓库根目录，确保工程与运行时状态路径稳定。
$ProjectDir = Split-Path $PSScriptRoot -Parent

# WebAppDir points at the Vite React WebUI project.
# WebAppDir 指向 Vite React WebUI 工程。
$WebAppDir = Join-Path $ProjectDir "apps\vulcan-code-web"

# ViteEntryPath points at the project-local Vite CLI started as the tracked process.
# ViteEntryPath 指向作为受跟踪进程启动的项目本地 Vite CLI。
$ViteEntryPath = Join-Path $WebAppDir "node_modules\vite\bin\vite.js"

# RuntimeStateDir stores ignored local lifecycle state and logs.
# RuntimeStateDir 保存被忽略的本地生命周期状态与日志。
$RuntimeStateDir = Join-Path $ProjectDir ".vulcan\temp"

# ViteStatePath stores the managed process id and exact process start time.
# ViteStatePath 保存受管进程标识与精确进程启动时间。
$ViteStatePath = Join-Path $RuntimeStateDir "vite-dev-server.json"

# ViteAddress stores the only loopback address accepted for the managed server.
# ViteAddress 保存受管服务器唯一允许使用的回环地址。
$ViteAddress = "127.0.0.1"

# VitePort stores the fixed development-server port shared with the Tauri dev URL.
# VitePort 保存与 Tauri 开发 URL 共用的固定开发服务器端口。
$VitePort = 17601

Set-Location $ProjectDir

# Get-ViteState reads and validates the persisted managed-process identity.
# Get-ViteState 用于读取并验证持久化的受管进程身份。
# This function accepts no parameters and returns null when no state exists or a validated state object otherwise.
# 此函数不接收参数；状态不存在时返回 null，否则返回经过验证的状态对象。
function Get-ViteState {
    if (-not (Test-Path -LiteralPath $ViteStatePath -PathType Leaf)) {
        return $null
    }

    # State parses the exact JSON object written after a successful process start.
    # State 解析进程成功启动后写入的精确 JSON 对象。
    $State = Get-Content -Raw -LiteralPath $ViteStatePath | ConvertFrom-Json
    if (
        $null -eq $State.pid -or
        [int]$State.pid -le 0 -or
        [string]::IsNullOrWhiteSpace([string]$State.process_start_time_utc)
    ) {
        throw "Invalid Vite lifecycle state file: $ViteStatePath"
    }
    return $State
}

# Get-ExactProcess returns the process only when both its id and start time match persisted state.
# Get-ExactProcess 仅在进程标识与启动时间都匹配持久化状态时返回该进程。
# State supplies pid and process_start_time_utc; the function returns the exact process or null.
# State 提供 pid 与 process_start_time_utc；此函数返回精确进程或 null。
function Get-ExactProcess {
    param(
        [Parameter(Mandatory = $true)]
        [object]$State
    )

    # Process is the current operating-system process found under the persisted id.
    # Process 是根据持久化标识找到的当前操作系统进程。
    $Process = Get-Process -Id ([int]$State.pid) -ErrorAction SilentlyContinue
    if ($null -eq $Process) {
        return $null
    }
    try {
        # ActualStartTimeUtc is the round-trip process identity that prevents pid-reuse mistakes.
        # ActualStartTimeUtc 是防止进程标识复用误判的往返格式进程身份。
        $ActualStartTimeUtc = $Process.StartTime.ToUniversalTime().ToString("O")
        # ExpectedStartTimeUtc normalizes PowerShell 7's automatic ISO-date JSON conversion back to round-trip text.
        # ExpectedStartTimeUtc 把 PowerShell 7 对 ISO 日期 JSON 的自动转换重新规范化为往返格式文本。
        $ExpectedStartTimeUtc = if ($State.process_start_time_utc -is [datetime]) {
            $State.process_start_time_utc.ToUniversalTime().ToString("O")
        }
        else {
            [datetime]::Parse(
                [string]$State.process_start_time_utc,
                [System.Globalization.CultureInfo]::InvariantCulture,
                [System.Globalization.DateTimeStyles]::RoundtripKind
            ).ToUniversalTime().ToString("O")
        }
        if ($ActualStartTimeUtc -ne $ExpectedStartTimeUtc) {
            return $null
        }
    }
    catch {
        return $null
    }
    return $Process
}

# Test-TcpEndpointReady checks whether the fixed Vite loopback endpoint accepts connections.
# Test-TcpEndpointReady 用于检查固定 Vite 回环端点是否接受连接。
# This function accepts no parameters and returns true only for a reachable endpoint.
# 此函数不接收参数，仅在端点可连接时返回 true。
function Test-TcpEndpointReady {
    # TcpClient owns the bounded connection probe and is always disposed before returning.
    # TcpClient 持有有界连接探测，并始终在返回前释放。
    $TcpClient = [System.Net.Sockets.TcpClient]::new()
    try {
        # ConnectTask represents the asynchronous loopback connection attempt.
        # ConnectTask 表示异步回环连接尝试。
        $ConnectTask = $TcpClient.ConnectAsync($ViteAddress, $VitePort)
        return $ConnectTask.Wait(300) -and $TcpClient.Connected
    }
    catch {
        return $false
    }
    finally {
        $TcpClient.Dispose()
    }
}

# Test-WindowsTcpListenerOwner verifies that the fixed listener belongs to one exact process id.
# Test-WindowsTcpListenerOwner 用于验证固定监听器是否归属于一个精确进程标识。
# OwnerProcessId is the only allowed listener owner; the function returns true for an exact native netstat match.
# OwnerProcessId 是唯一允许的监听器所有者；此函数仅在原生 netstat 精确匹配时返回 true。
function Test-WindowsTcpListenerOwner {
    param(
        [Parameter(Mandatory = $true)]
        [int]$OwnerProcessId
    )

    # NetstatPath points at the native executable used for bounded listener ownership inspection.
    # NetstatPath 指向用于有界监听归属检查的本机可执行文件。
    $NetstatPath = Join-Path $env:SystemRoot "System32\netstat.exe"
    # ExpectedEndpoint is the exact local endpoint emitted for the configured Vite listener.
    # ExpectedEndpoint 是配置后 Vite 监听器输出的准确本地端点。
    $ExpectedEndpoint = "${ViteAddress}:$VitePort"
    # NetstatLines captures the native TCP table used for exact ownership validation.
    # NetstatLines 捕获用于准确归属验证的本机 TCP 表。
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

# Test-ManagedListener verifies that the exact tracked process owns the ready Vite endpoint.
# Test-ManagedListener 用于验证精确受跟踪进程是否持有已就绪的 Vite 端点。
# Process supplies the verified process identity; the function returns true only for a ready owned listener.
# Process 提供经过验证的进程身份；此函数仅在监听就绪且归属正确时返回 true。
function Test-ManagedListener {
    param(
        [Parameter(Mandatory = $true)]
        [System.Diagnostics.Process]$Process
    )

    if (-not (Test-TcpEndpointReady)) {
        return $false
    }
    if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
        return Test-WindowsTcpListenerOwner -OwnerProcessId $Process.Id
    }
    return $true
}

# Remove-ViteState deletes only the generated lifecycle state file when present.
# Remove-ViteState 仅在存在时删除生成的生命周期状态文件。
# This function accepts no parameters and returns no value.
# 此函数不接收参数，也不返回值。
function Remove-ViteState {
    if (Test-Path -LiteralPath $ViteStatePath -PathType Leaf) {
        Remove-Item -LiteralPath $ViteStatePath -Force
    }
}

# Start-ViteDevServer starts one detached Vite process and persists its exact identity.
# Start-ViteDevServer 用于启动一个独立 Vite 进程并持久化其精确身份。
# This function accepts no parameters and returns zero on success.
# 此函数不接收参数，成功时返回零。
function Start-ViteDevServer {
    # ExistingState is the persisted identity from a previous standalone start, when present.
    # ExistingState 是此前独立启动留下的持久化身份（如果存在）。
    $ExistingState = Get-ViteState
    if ($null -ne $ExistingState) {
        # ExistingProcess is accepted only when pid and process start time both match.
        # ExistingProcess 仅在进程标识与进程启动时间同时匹配时才被接受。
        $ExistingProcess = Get-ExactProcess -State $ExistingState
        if ($null -ne $ExistingProcess) {
            if (Test-ManagedListener -Process $ExistingProcess) {
                Write-Host "Vite dev server is already running: pid=$($ExistingProcess.Id), url=http://${ViteAddress}:$VitePort"
                return 0
            }
            throw "Tracked Vite process $($ExistingProcess.Id) exists but its listener is not ready. Run 'make vite stop' before starting again."
        }
        Remove-ViteState
    }

    if (Test-TcpEndpointReady) {
        throw "Port $VitePort is already occupied by a process not managed by this repository."
    }

    # PnpmCommand resolves the package manager used only when WebUI dependencies are missing.
    # PnpmCommand 解析仅在 WebUI 依赖缺失时使用的包管理器。
    $PnpmCommand = Get-Command "pnpm.cmd" -ErrorAction SilentlyContinue
    if (-not $PnpmCommand) {
        $PnpmCommand = Get-Command "pnpm" -ErrorAction SilentlyContinue
    }
    if (-not $PnpmCommand) {
        throw "pnpm not found. Install pnpm before starting the Vite dev server."
    }

    if (-not (Test-Path -LiteralPath (Join-Path $WebAppDir "node_modules") -PathType Container)) {
        Write-Host "==> Installing WebUI dependencies..."
        & $PnpmCommand.Source --dir $WebAppDir install
        if ($LASTEXITCODE -ne 0) {
            return $LASTEXITCODE
        }
    }

    # NodeCommand resolves the exact Node.js runtime that owns the listener process.
    # NodeCommand 解析持有监听器进程的精确 Node.js 运行时。
    $NodeCommand = Get-Command "node.exe" -ErrorAction SilentlyContinue
    if (-not $NodeCommand) {
        $NodeCommand = Get-Command "node" -ErrorAction SilentlyContinue
    }
    if (-not $NodeCommand) {
        throw "Node.js not found. Install Node.js before starting the Vite dev server."
    }
    if (-not (Test-Path -LiteralPath $ViteEntryPath -PathType Leaf)) {
        throw "Missing project-local Vite entrypoint: $ViteEntryPath"
    }

    New-Item -ItemType Directory -Path $RuntimeStateDir -Force | Out-Null
    Write-Host "==> Starting standalone WebUI dev server at http://${ViteAddress}:$VitePort ..."
    # ViteProcess is the detached Node.js process that directly runs the project-local Vite CLI.
    # ViteProcess 是直接运行项目本地 Vite CLI 的独立 Node.js 进程。
    $ViteProcess = Start-Process `
        -FilePath $NodeCommand.Source `
        -ArgumentList @("`"$ViteEntryPath`"", "--host", $ViteAddress, "--port", [string]$VitePort, "--strictPort") `
        -WorkingDirectory $WebAppDir `
        -WindowStyle Hidden `
        -PassThru

    # PersistedState binds the operating-system pid to the exact process creation time.
    # PersistedState 把操作系统进程标识绑定到精确进程创建时间。
    $PersistedState = [ordered]@{
        pid                    = $ViteProcess.Id
        process_start_time_utc = $ViteProcess.StartTime.ToUniversalTime().ToString("O")
        address                = $ViteAddress
        port                   = $VitePort
    }
    $PersistedState | ConvertTo-Json | Set-Content -LiteralPath $ViteStatePath -Encoding utf8

    # Deadline bounds readiness waiting so failed starts never hang the Make command.
    # Deadline 限制就绪等待时间，确保启动失败不会无限阻塞 Make 命令。
    $Deadline = (Get-Date).AddSeconds(20)
    do {
        Start-Sleep -Milliseconds 250
        if ($ViteProcess.HasExited) {
            Remove-ViteState
            throw "Vite exited before becoming ready at http://${ViteAddress}:$VitePort."
        }
    } while (-not (Test-ManagedListener -Process $ViteProcess) -and (Get-Date) -lt $Deadline)

    if (-not (Test-ManagedListener -Process $ViteProcess)) {
        if (-not $ViteProcess.HasExited) {
            Stop-Process -Id $ViteProcess.Id -Force
        }
        Remove-ViteState
        throw "Vite did not become ready at http://${ViteAddress}:$VitePort."
    }

    Write-Host "Vite dev server started: pid=$($ViteProcess.Id), url=http://${ViteAddress}:$VitePort"
    return 0
}

# Stop-ViteDevServer stops only the exact process identity persisted by standalone start.
# Stop-ViteDevServer 仅停止独立启动所持久化的精确进程身份。
# This function accepts no parameters and returns zero when the managed server is stopped or already absent.
# 此函数不接收参数；受管服务器停止或已不存在时返回零。
function Stop-ViteDevServer {
    # ExistingState is the only authority for selecting a process that may be stopped.
    # ExistingState 是选择允许停止进程的唯一权威依据。
    $ExistingState = Get-ViteState
    if ($null -eq $ExistingState) {
        if (Test-TcpEndpointReady) {
            throw "Port $VitePort is occupied, but no managed Vite state exists; refusing to stop an unrelated process."
        }
        Write-Host "Vite dev server is already stopped."
        return 0
    }

    # ExistingProcess is accepted only when both persisted identity fields still match.
    # ExistingProcess 仅在两个持久化身份字段仍匹配时才被接受。
    $ExistingProcess = Get-ExactProcess -State $ExistingState
    if ($null -eq $ExistingProcess) {
        Remove-ViteState
        if (Test-TcpEndpointReady) {
            throw "Managed Vite state was stale, but port $VitePort is occupied by another process; nothing was stopped."
        }
        Write-Host "Removed stale Vite state; the dev server was already stopped."
        return 0
    }

    Write-Host "==> Stopping standalone WebUI dev server: pid=$($ExistingProcess.Id) ..."
    Stop-Process -Id $ExistingProcess.Id
    if (-not $ExistingProcess.WaitForExit(10000)) {
        # RefreshedProcess revalidates pid and start time before the forced termination fallback.
        # RefreshedProcess 在强制终止回退前重新验证进程标识与启动时间。
        $RefreshedProcess = Get-ExactProcess -State $ExistingState
        if ($null -ne $RefreshedProcess) {
            Stop-Process -Id $RefreshedProcess.Id -Force
            $null = $RefreshedProcess.WaitForExit(5000)
        }
    }
    Remove-ViteState

    if (Test-TcpEndpointReady) {
        throw "Managed Vite process stopped, but port $VitePort remains occupied by another process."
    }
    Write-Host "Vite dev server stopped."
    return 0
}

# Show-ViteDevServerStatus reports running, stopped, unhealthy, stale, or unmanaged state without mutation.
# Show-ViteDevServerStatus 用于无状态变更地报告运行、停止、异常、陈旧或非受管状态。
# This function accepts no parameters and returns zero for running or stopped, one for inconsistent state.
# 此函数不接收参数；运行或停止时返回零，状态不一致时返回一。
function Show-ViteDevServerStatus {
    # ExistingState is the persisted managed-process identity, when present.
    # ExistingState 是持久化的受管进程身份（如果存在）。
    $ExistingState = Get-ViteState
    if ($null -eq $ExistingState) {
        if (Test-TcpEndpointReady) {
            Write-Host "Vite dev server status: unmanaged listener at http://${ViteAddress}:$VitePort"
            return 1
        }
        Write-Host "Vite dev server status: stopped"
        return 0
    }

    # ExistingProcess is the exact current process matching persisted pid and start time.
    # ExistingProcess 是同时匹配持久化进程标识与启动时间的精确当前进程。
    $ExistingProcess = Get-ExactProcess -State $ExistingState
    if ($null -eq $ExistingProcess) {
        Write-Host "Vite dev server status: stale state, tracked process is absent"
        return 1
    }
    if (-not (Test-ManagedListener -Process $ExistingProcess)) {
        Write-Host "Vite dev server status: unhealthy, pid=$($ExistingProcess.Id), listener not ready"
        return 1
    }
    Write-Host "Vite dev server status: running, pid=$($ExistingProcess.Id), url=http://${ViteAddress}:$VitePort"
    return 0
}

# ExitCode stores the selected lifecycle operation result returned to Make.
# ExitCode 保存返回给 Make 的选定生命周期操作结果。
$ExitCode = switch ($Action) {
    "start" { Start-ViteDevServer }
    "stop" { Stop-ViteDevServer }
    "status" { Show-ViteDevServerStatus }
}
exit $ExitCode
