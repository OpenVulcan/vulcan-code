# Build and restart only the Windows Debug Core while the Electron workbench stays open.
# 仅构建并重启 Windows Debug Core，同时保持 Electron 工作台打开。
param(
    # Action chooses compilation alone or compilation followed by a guarded Core restart.
    # Action 选择单独编译，或编译后受保护地重启 Core。
    [Parameter(Mandatory = $true)]
    [ValidateSet('build', 'restart')]
    [string]$Action
)

$ErrorActionPreference = 'Stop'

# ProjectDir is the repository root used for Cargo and the desktop runtime resolver.
# ProjectDir 是 Cargo 与桌面运行时解析器共同使用的仓库根目录。
$ProjectDir = Split-Path $PSScriptRoot -Parent
Set-Location $ProjectDir

if (-not [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
    throw 'The Core-only Debug restart workflow currently supports Windows only.'
}

# RuntimeScriptPath is the existing manifest-aware distribution resolver.
# RuntimeScriptPath 是现有的清单感知发行目录解析器。
$RuntimeScriptPath = Join-Path $PSScriptRoot 'desktop-runtime.mjs'

# OutputRoot is the exact generated distribution selected by the existing runtime entrypoint.
# OutputRoot 是现有运行入口选定的精确生成发行目录。
$OutputRoot = (& node $RuntimeScriptPath root).Trim()
if ($LASTEXITCODE -ne 0 -or -not [IO.Path]::IsPathFullyQualified($OutputRoot)) {
    throw 'Could not resolve the built desktop distribution root.'
}

# ManifestPath selects the existing Debug layout without rebuilding Electron or WebUI.
# ManifestPath 选定现有 Debug 布局，不重建 Electron 或 WebUI。
$ManifestPath = Join-Path $OutputRoot 'runtime-layout.json'
if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
    throw "Missing desktop runtime layout: $ManifestPath. Run 'make build' once first."
}

# Layout is the authoritative runtime layout already used by the GUI and service.
# Layout 是 GUI 与服务已经共同使用的权威运行布局。
$Layout = Get-Content -Raw -LiteralPath $ManifestPath -Encoding UTF8 | ConvertFrom-Json
if ($Layout.schemaVersion -ne 1 -or $Layout.profile -ne 'debug' -or $Layout.platform -ne 'win32' -or
    $Layout.serviceExecutable -ne 'bin/vulcan-code-service.exe') {
    throw "Core-only Debug build requires an existing Windows Debug layout: $ManifestPath"
}

# OutputCorePath is the service executable that the running desktop shell expects.
# OutputCorePath 是正在运行的桌面壳所期望的服务可执行文件。
$OutputCorePath = Join-Path $OutputRoot 'bin\vulcan-code-service.exe'
if (-not (Test-Path -LiteralPath $OutputCorePath -PathType Leaf)) {
    throw "Missing built Core executable: $OutputCorePath. Run 'make build' once first."
}

# ExistingDescription freezes the GUI-visible protocol and user data root before replacement.
# ExistingDescription 在替换前冻结 GUI 可见协议与用户数据根目录。
$ExistingDescription = (& $OutputCorePath --describe | Out-String | ConvertFrom-Json)
if ($LASTEXITCODE -ne 0 -or $ExistingDescription.profile -ne 'debug') {
    throw "Existing Core is not a valid Debug executable: $OutputCorePath"
}

# Invoke-CoreCargoBuild compiles only the service package and returns Cargo's exact executable path.
# Invoke-CoreCargoBuild 只编译服务包，并返回 Cargo 报告的精确可执行文件路径。
# It never updates the shared full-build artifact baseline, whose dependency set includes Electron's terminal host.
# 它不更新全量构建共用的制品基线，因为该基线还包含 Electron 所需的终端宿主依赖。
function Invoke-CoreCargoBuild {
    # BuildState retains the reported binary and terminal success record across pipeline callbacks.
    # BuildState 在管道回调之间保留 Cargo 报告的二进制路径与终态。
    $BuildState = @{ executable = $null; finished = $false; protocolValid = $true }
    & cargo build -p vulcan-code-service --message-format=json | ForEach-Object {
        # Line is one Cargo JSON message or diagnostic line.
        # Line 是一条 Cargo JSON 消息或诊断文本。
        $Line = [string]$_
        if (-not $Line.StartsWith('{')) {
            Write-Host $Line
            return
        }
        try {
            # Message is parsed only from Cargo's declared JSON output.
            # Message 仅从 Cargo 声明的 JSON 输出中解析。
            $Message = $Line | ConvertFrom-Json -ErrorAction Stop
        }
        catch {
            $BuildState.protocolValid = $false
            Write-Host $Line
            return
        }
        switch ($Message.reason) {
            'compiler-artifact' {
                if ($Message.target.name -eq 'vulcan-code-service' -and $Message.target.kind -contains 'bin') {
                    $BuildState.executable = [string]$Message.executable
                }
            }
            'compiler-message' { Write-Host $Message.message.rendered }
            'build-finished' { $BuildState.finished = [bool]$Message.success }
        }
    }
    # BuildExitCode must be captured before another native command changes LASTEXITCODE.
    # BuildExitCode 必须在其他原生命令改变 LASTEXITCODE 前取得。
    $BuildExitCode = $LASTEXITCODE
    if ($BuildExitCode -ne 0 -or -not $BuildState.finished -or -not $BuildState.protocolValid -or
        [string]::IsNullOrWhiteSpace($BuildState.executable) -or
        -not (Test-Path -LiteralPath $BuildState.executable -PathType Leaf)) {
        throw "Core-only Cargo build failed or did not report one valid executable (exit=$BuildExitCode)."
    }
    return [IO.Path]::GetFullPath($BuildState.executable)
}

# Get-RunningOutputCore returns only processes executing this layout's exact Core binary.
# Get-RunningOutputCore 只返回正在执行本布局精确 Core 二进制文件的进程。
# It never selects a process by name alone, so a Release or isolated test owner is left untouched.
# 它绝不只凭进程名选择目标，因此不会碰触 Release 或隔离测试持有者。
function Get-RunningOutputCore {
    # Processes are all same-name OS processes; executable path establishes exact ownership.
    # Processes 是系统中所有同名进程；可执行文件路径确定精确归属。
    $Processes = @(Get-Process -Name 'vulcan-code-service' -ErrorAction SilentlyContinue)
    return @($Processes | Where-Object {
        try {
            -not [string]::IsNullOrWhiteSpace($_.Path) -and
                [IO.Path]::GetFullPath($_.Path).Equals($OutputCorePath, [StringComparison]::OrdinalIgnoreCase)
        }
        catch { $false }
    })
}

# Assert-NoForeignCoreOwner rejects a live Debug owner whose executable is outside this layout.
# Assert-NoForeignCoreOwner 拒绝可执行文件不属于本布局的活动 Debug 持有者。
# The discovery PID is only a clue; the live same-name executable path must match this layout.
# 发现文档 PID 只是线索；同名活动进程的可执行路径必须匹配本布局。
function Assert-NoForeignCoreOwner {
    # DiscoveryPath belongs to the existing executable's fixed Debug data root.
    # DiscoveryPath 属于现有可执行文件固定的 Debug 数据根目录。
    $DiscoveryPath = [string]$ExistingDescription.discoveryPath
    if (-not (Test-Path -LiteralPath $DiscoveryPath -PathType Leaf)) { return }
    try {
        # Discovery is untrusted until its PID and process name identify a live Core.
        # Discovery 在 PID 与进程名确认活动 Core 前不被信任。
        $Discovery = Get-Content -Raw -LiteralPath $DiscoveryPath -Encoding UTF8 | ConvertFrom-Json
        $OwnerProcess = Get-Process -Id ([int]$Discovery.pid) -ErrorAction SilentlyContinue
    }
    catch { return }
    if ($null -eq $OwnerProcess -or $OwnerProcess.ProcessName -ne 'vulcan-code-service') { return }
    if ([string]::IsNullOrWhiteSpace($OwnerProcess.Path)) {
        throw "A Debug Core owner is active but its executable path cannot be verified (PID $($OwnerProcess.Id))."
    }
    if (-not [IO.Path]::GetFullPath($OwnerProcess.Path).Equals($OutputCorePath, [StringComparison]::OrdinalIgnoreCase)) {
        throw "A different Core executable owns the Debug data root (PID $($OwnerProcess.Id)); stop it before this restart."
    }
}

# BuiltCorePath is the fresh Cargo artifact; Core-only build does not publish it while an owner is running.
# BuiltCorePath 是最新 Cargo 制品；仅构建模式不会在持有者运行时发布它。
$BuiltCorePath = Invoke-CoreCargoBuild

# BuiltDescription confirms the new binary still implements the GUI's frozen launch protocol.
# BuiltDescription 确认新二进制仍实现 GUI 已冻结的启动协议。
$BuiltDescription = (& $BuiltCorePath --describe | Out-String | ConvertFrom-Json)
if ($LASTEXITCODE -ne 0 -or $BuiltDescription.profile -ne 'debug' -or
    $BuiltDescription.protocolVersion -ne $ExistingDescription.protocolVersion -or
    $BuiltDescription.serviceName -ne $ExistingDescription.serviceName -or
    $BuiltDescription.discoveryPath -ne $ExistingDescription.discoveryPath -or
    $BuiltDescription.dataRoot -ne $ExistingDescription.dataRoot -or
    $BuiltDescription.lockPath -ne $ExistingDescription.lockPath -or
    $BuiltDescription.defaultWebUiMode -ne $ExistingDescription.defaultWebUiMode -or
    $BuiltDescription.viteUrl -ne $ExistingDescription.viteUrl -or
    ($BuiltDescription.modes -join ',') -ne ($ExistingDescription.modes -join ',')) {
    throw 'The new Core launch contract differs from the running desktop layout; use a full make build.'
}

if ($Action -eq 'build') {
    Write-Host "Debug Core compiled without changing GUI, Vite, or the running Core: $BuiltCorePath"
    exit 0
}

Assert-NoForeignCoreOwner

# Deadline bounds the wait for the existing tray owner to drain its sessions.
# Deadline 限定等待现有托盘持有者排空会话的时间。
$Deadline = (Get-Date).AddSeconds(120)
if (@(Get-RunningOutputCore).Count -gt 0) {
    Write-Host 'Debug Core is still running. Click its tray icon > Quit Service; the GUI can stay open.'
    while (@(Get-RunningOutputCore).Count -gt 0 -and (Get-Date) -lt $Deadline) {
        Start-Sleep -Milliseconds 500
    }
    if (@(Get-RunningOutputCore).Count -gt 0) {
        throw 'Debug Core did not exit after Quit Service. The old executable was not replaced.'
    }
}

Assert-NoForeignCoreOwner

# ReplacementId gives this attempt unique same-volume staging and rollback paths.
# ReplacementId 为本次尝试提供唯一的同卷暂存与回退路径。
$ReplacementId = [Guid]::NewGuid().ToString('N')

# StagedPath holds a complete copy before any live output file is replaced.
# StagedPath 在替换实际输出文件前保存完整副本。
$StagedPath = "$OutputCorePath.$ReplacementId.next"

# BackupPath preserves the previous Debug Core until the new process proves ready.
# BackupPath 保留旧 Debug Core，直到新进程证明已就绪。
$BackupPath = "$OutputCorePath.$ReplacementId.previous"

# FailedPath temporarily retains the failed replacement while restoring the previous Core.
# FailedPath 在恢复旧 Core 时暂存失败的新制品。
$FailedPath = "$OutputCorePath.$ReplacementId.failed"
Copy-Item -LiteralPath $BuiltCorePath -Destination $StagedPath -ErrorAction Stop
if ((Get-FileHash -LiteralPath $BuiltCorePath -Algorithm SHA256).Hash -ne
    (Get-FileHash -LiteralPath $StagedPath -Algorithm SHA256).Hash) {
    Remove-Item -LiteralPath $StagedPath -Force
    throw 'The staged Core binary differs from the Cargo artifact; output was not replaced.'
}
try {
    [IO.File]::Replace($StagedPath, $OutputCorePath, $BackupPath, $true)
}
finally {
    if (Test-Path -LiteralPath $StagedPath -PathType Leaf) {
        Remove-Item -LiteralPath $StagedPath -Force
    }
}

# PriorLayoutEnvironment restores the caller's runtime selection after the child inherits it.
# PriorLayoutEnvironment 在子进程继承运行布局后恢复调用者原有选择。
$PriorLayoutEnvironment = [Environment]::GetEnvironmentVariable('VULCAN_RUNTIME_LAYOUT', 'Process')
try {
    [Environment]::SetEnvironmentVariable('VULCAN_RUNTIME_LAYOUT', $ManifestPath, 'Process')
    # StartedCore runs independently of this Make invocation and owns one visible Debug console.
    # StartedCore 独立于本次 Make 调用运行，并持有一个可见的 Debug 控制台。
    $StartedCore = Start-Process -FilePath $OutputCorePath -ArgumentList '--gui-launch' `
        -WorkingDirectory $OutputRoot -WindowStyle Normal -PassThru
}
catch {
    # No child was started, so restoring the previous binary cannot interrupt a live owner.
    # 子进程未能启动，因此恢复旧二进制不会打断活动持有者。
    [IO.File]::Replace($BackupPath, $OutputCorePath, $FailedPath, $true)
    Remove-Item -LiteralPath $FailedPath -Force
    throw
}
finally {
    [Environment]::SetEnvironmentVariable('VULCAN_RUNTIME_LAYOUT', $PriorLayoutEnvironment, 'Process')
}

# ReadyDeadline bounds the new owner's discovery and health handshake.
# ReadyDeadline 限定新持有者发布发现与健康应答的时间。
$ReadyDeadline = (Get-Date).AddSeconds(30)

# Ready confirms both the launched PID and the service's own health identity.
# Ready 同时确认已启动 PID 与服务自身的健康身份。
$Ready = $false
while ((Get-Date) -lt $ReadyDeadline) {
    $StartedCore.Refresh()
    if ($StartedCore.HasExited) { break }
    if (Test-Path -LiteralPath $ExistingDescription.discoveryPath -PathType Leaf) {
        try {
            # Discovery and Health must describe the same process generation, including its token.
            # Discovery 与 Health 必须描述同一进程代次，包含相同令牌。
            $Discovery = Get-Content -Raw -LiteralPath $ExistingDescription.discoveryPath -Encoding UTF8 | ConvertFrom-Json
            if ($Discovery.pid -eq $StartedCore.Id -and $Discovery.status -eq 'ready' -and
                $Discovery.baseUrl -match '^http://127\.0\.0\.1:\d+$') {
                $Health = Invoke-RestMethod -Uri "$($Discovery.baseUrl)/health" -TimeoutSec 2
                $Ready = $Health.status -eq 'ready' -and $Health.pid -eq $Discovery.pid -and
                    $Health.startedAt -eq $Discovery.startedAt -and
                    $Health.serviceToken -eq $Discovery.serviceToken
                if ($Ready) { break }
            }
        }
        catch { }
    }
    Start-Sleep -Milliseconds 250
}
if (-not $Ready) {
    $StartedCore.Refresh()
    if ($StartedCore.HasExited) {
        # A failed new process no longer holds the file; restore the previous known startup artifact.
        # 新进程失败退出后不再持有文件；恢复此前已知可启动的制品。
        [IO.File]::Replace($BackupPath, $OutputCorePath, $FailedPath, $true)
        Remove-Item -LiteralPath $FailedPath -Force
        throw "New Debug Core exited before becoming ready; the previous binary was restored (exit=$($StartedCore.ExitCode))."
    }
    throw "New Debug Core did not publish matching ready health. Previous binary retained at $BackupPath; inspect the Core console."
}

Remove-Item -LiteralPath $BackupPath -Force
Write-Host "Debug Core restarted: pid=$($StartedCore.Id), url=$($Discovery.baseUrl). Electron GUI and Vite were not restarted."
