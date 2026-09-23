# Shared Windows publication guard; dot-sourcing this file never stops processes.
# Windows 发布共用守卫；加载本文件不会停止任何进程。

# Maximum time to release an owned process before publication must fail.
# 发布失败前等待所属进程释放的最长时间。
$OutputProcessExitTimeoutMilliseconds = 10000

# Get processes whose executable is inside DistributionRoot; returns process objects, never name matches.
# 获取可执行文件位于 DistributionRoot 内的进程；返回进程对象，绝不按名称匹配。
function Get-DistributionProcesses {
    param([Parameter(Mandatory = $true)][string]$DistributionRoot)
    # RootPrefix includes the separator so similarly named sibling directories cannot match.
    # RootPrefix 包含分隔符，避免匹配名称相似的同级目录。
    $RootPrefix = [IO.Path]::GetFullPath($DistributionRoot).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    foreach ($Candidate in Get-Process) {
        # ExecutablePath is provided by the OS; inaccessible foreign processes are never selected.
        # ExecutablePath 由系统提供；无法访问的外部进程不会入选。
        $ExecutablePath = $Candidate.Path
        if ($ExecutablePath -and [IO.Path]::GetFullPath($ExecutablePath).StartsWith($RootPrefix, [StringComparison]::OrdinalIgnoreCase)) {
            if ($Candidate.Id -eq $PID) { throw 'The publisher must not run from the distribution being replaced.' }
            $Candidate
        }
    }
}

# Assert or release processes before writing a distribution; Restart authorizes stopping its exact owners.
# 写入发行目录前检查或释放进程；Restart 授权停止精确归属该目录的进程。
# DistributionRoot is the validated build output; returns only after all owners have exited, otherwise throws.
# DistributionRoot 是已验证的构建输出；全部所属进程退出后返回，否则抛出错误。
function Prepare-DistributionPublication {
    param([Parameter(Mandatory = $true)][string]$DistributionRoot, [switch]$Restart)
    # Reject directory links before deciding process ownership, including a redirected output root.
    # 判断进程归属前拒绝目录链接，包括被重定向的输出根目录。
    $RootItem = Get-Item -LiteralPath $DistributionRoot -ErrorAction SilentlyContinue
    if ($RootItem -and ($RootItem.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "Distribution root must not be a directory link: $DistributionRoot"
    }
    # Deadline bounds the entire release, including any children spawned while parents exit.
    # Deadline 限定整个释放过程，包括父进程退出期间新建的子进程。
    $Deadline = [DateTime]::UtcNow.AddMilliseconds($OutputProcessExitTimeoutMilliseconds)
    do {
        # Owners is a fresh snapshot so detached browser hosts are also collected after Core exits.
        # Owners 是新快照，因此核心退出后仍可收集独立浏览器宿主。
        $Owners = @(Get-DistributionProcesses -DistributionRoot $DistributionRoot)
        if ($Owners.Count -eq 0) { return }
        if (-not $Restart) {
            throw "Distribution is still running (PIDs: $($Owners.Id -join ', ')). Use 'make br' to stop it after compilation and publish the new build."
        }
        foreach ($Owner in $Owners) {
            $Owner.Refresh()
            if ($Owner.HasExited) { continue }
            Write-Host "Stopping distribution process: PID $($Owner.Id), $($Owner.Path)"
            Stop-Process -InputObject $Owner -Force -ErrorAction Stop
            # Remaining is derived from the shared deadline rather than granting each child a new timeout.
            # Remaining 从共享截止时间派生，不为每个子进程重新授予超时。
            $Remaining = [Math]::Max(0, [int]($Deadline - [DateTime]::UtcNow).TotalMilliseconds)
            if (-not $Owner.WaitForExit($Remaining)) { throw "Distribution process did not exit: PID $($Owner.Id)" }
        }
    } while ([DateTime]::UtcNow -lt $Deadline)
    if (@(Get-DistributionProcesses -DistributionRoot $DistributionRoot).Count -ne 0) {
        throw 'Distribution still has running processes; publication was not started.'
    }
}
