# Verify publication ownership with disposable processes; never stop the user's distribution.
# 使用可丢弃进程验证发布归属；绝不停止用户发行实例。
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'distribution_processes.ps1')

# Parse the integration entrypoints without running builds or modifying output.
# 解析集成入口，不执行构建或修改输出。
foreach ($Script in @('build_tauri.ps1', 'core_debug_cycle.ps1', 'distribution_processes.ps1', '../make.ps1')) {
    $Tokens = $null
    $Errors = $null
    [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $Script), [ref]$Tokens, [ref]$Errors) | Out-Null
    if ($Errors.Count) { throw ($Errors | Out-String) }
}

# TestRoot isolates two same-name processes in sibling directories sharing a prefix.
# TestRoot 隔离名称相同、目录前缀相同但归属不同的两个进程。
$TemporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$TestRoot = Join-Path $TemporaryRoot ('vulcan-publication-test-' + [Guid]::NewGuid().ToString('N'))
$OwnedRoot = Join-Path $TestRoot 'output'
# ForeignRoot is named like the real release distribution, the sibling debug and release must never stop.
# ForeignRoot 与真实的正式版发行目录同名，调试版与正式版绝不能互相停止。
$ForeignRoot = Join-Path $TestRoot 'output-release'
$Processes = @()
try {
    New-Item -ItemType Directory -Path $OwnedRoot, $ForeignRoot | Out-Null
    foreach ($Root in @($OwnedRoot, $ForeignRoot)) {
        # Executable is a renamed OS utility used solely to hold a real Windows executable lock.
        # Executable 是重命名的系统工具，仅用于持有真实 Windows 可执行文件锁。
        $Executable = Join-Path $Root 'vulcan-code-service.exe'
        Copy-Item -LiteralPath (Join-Path $env:SystemRoot 'System32/ping.exe') -Destination $Executable
        $Processes += Start-Process -FilePath $Executable -ArgumentList '-t', '127.0.0.1' -WindowStyle Hidden -PassThru
    }
    # Ordinary builds must reject owners before changing files, while leaving both processes alive.
    # 普通构建必须在修改文件前拒绝占用，同时保留两个进程。
    $Rejected = $false
    try { Prepare-DistributionPublication -DistributionRoot $OwnedRoot } catch {
        if ($_.Exception.Message -notlike 'Distribution is still running*') { throw }
        $Rejected = $true
    }
    if (-not $Rejected) { throw 'Ordinary build accepted a running distribution.' }
    foreach ($Process in $Processes) {
        $Process.Refresh()
        if ($Process.HasExited) { throw 'Ordinary build stopped a process.' }
    }
    Prepare-DistributionPublication -DistributionRoot $OwnedRoot -Restart
    if (-not $Processes[0].WaitForExit(0)) { throw 'Restart did not stop its distribution.' }
    $Processes[1].Refresh()
    if ($Processes[1].HasExited) { throw 'Restart stopped the sibling release distribution.' }
    # Copying over the executable proves the real sharing violation has been released.
    # 覆盖可执行文件证明实际共享占用已经释放。
    Copy-Item -LiteralPath (Join-Path $env:SystemRoot 'System32/ping.exe') -Destination (Join-Path $OwnedRoot 'vulcan-code-service.exe') -Force
    Prepare-DistributionPublication -DistributionRoot $OwnedRoot -Restart
    # A directory junction must not redirect process ownership to another distribution.
    # 目录连接不得把进程归属重定向到另一个发行实例。
    $Link = Join-Path $TestRoot 'linked'
    New-Item -ItemType Junction -Path $Link -Target $ForeignRoot | Out-Null
    $Rejected = $false
    try { Prepare-DistributionPublication -DistributionRoot $Link -Restart } catch {
        if ($_.Exception.Message -notlike 'Distribution root must not be a directory link*') { throw }
        $Rejected = $true
    }
    if (-not $Rejected) { throw 'Linked distribution root was accepted.' }
    Remove-Item -LiteralPath $Link -Force
    # The reverse direction: restarting the release distribution must leave the debug one running.
    # 反方向：重启正式版发行目录时，调试版必须保持运行。
    $DebugExecutable = Join-Path $OwnedRoot 'vulcan-code-service.exe'
    $Processes += Start-Process -FilePath $DebugExecutable -ArgumentList '-t', '127.0.0.1' -WindowStyle Hidden -PassThru
    Prepare-DistributionPublication -DistributionRoot $ForeignRoot -Restart
    if (-not $Processes[1].WaitForExit(0)) { throw 'Restart did not stop the release distribution.' }
    $Processes[2].Refresh()
    if ($Processes[2].HasExited) { throw 'Restarting the release distribution stopped the debug distribution.' }
    Write-Host 'PASS: script syntax, ordinary-build rejection, exact ownership, debug/release isolation in both directions, lock release, idle restart, junction rejection.'
}
finally {
    # Cleanup only the process objects and the exact temporary directory created by this test.
    # 仅清理本测试创建的进程对象与精确临时目录。
    foreach ($Process in $Processes) {
        $Process.Refresh()
        if (-not $Process.HasExited) { Stop-Process -InputObject $Process -Force; $Process.WaitForExit() }
        $Process.Dispose()
    }
    $ResolvedRoot = [IO.Path]::GetFullPath($TestRoot)
    if (-not $ResolvedRoot.StartsWith($TemporaryRoot.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw "Unsafe temporary cleanup path: $ResolvedRoot"
    }
    if (Test-Path -LiteralPath $ResolvedRoot) { Remove-Item -LiteralPath $ResolvedRoot -Recurse -Force }
}
