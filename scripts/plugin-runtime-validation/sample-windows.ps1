# Launch one known consumer and sample its actual Windows process; return its original exit code.
# 启动单个已知消费者并采样其真实 Windows 进程；返回其原退出码。
# Every path is supplied explicitly; samples observe working set, private bytes, threads, handles and immediate children.
# 全部路径显式提供；采样观察工作集、私有字节、线程、句柄及直接子进程。
param(
    # Exact already-built consumer executable.
    # 精确已构建消费者可执行文件。
    [Parameter(Mandatory = $true)][string]$Executable,
    # Exact frozen workload JSON supplied to the consumer.
    # 传入消费者的精确冻结负载 JSON。
    [Parameter(Mandatory = $true)][string]$Configuration,
    # New raw invocation JSONL path owned by the consumer.
    # 消费者拥有的新原始调用 JSONL 路径。
    [Parameter(Mandatory = $true)][string]$InvocationOutput,
    # New observational process JSONL path owned by this sampler.
    # 此采样器拥有的新进程观察 JSONL 路径。
    [Parameter(Mandatory = $true)][string]$ResourceOutput,
    # New native stdout artifact path.
    # 新原生标准输出产物路径。
    [Parameter(Mandatory = $true)][string]$Stdout,
    # New native stderr artifact path.
    # 新原生标准错误产物路径。
    [Parameter(Mandatory = $true)][string]$Stderr,
    # Frozen gate JSON containing explicit benchmark and soak sampling intervals.
    # 包含显式基准及长稳采样间隔的冻结门禁 JSON。
    [Parameter(Mandatory = $true)][string]$Gates,
    # Outer process budget derived once by execute.py from the exact frozen workload limits.
    # execute.py 从精确冻结负载上限唯一派生的外层进程预算。
    [Parameter(Mandatory = $true)][long]$MaximumProcessDurationMs
)

# Sampling failures remain explicit and never produce a successful resource gate.
# 采样失败保持明确，绝不产生成功资源门禁。
$ErrorActionPreference = 'Stop'
# Resolve the exact supplied executable without probing alternate paths.
# 解析精确给定可执行文件，不探测备用路径。
$executablePath = (Resolve-Path -LiteralPath $Executable).Path
# Resolve the exact supplied frozen workload file.
# 解析精确给定冻结负载文件。
$configurationPath = (Resolve-Path -LiteralPath $Configuration).Path
# Decode the actual workload solely to select its declared sampling phase.
# 解码真实负载，仅用于选择其声明采样阶段。
$workload = Get-Content -LiteralPath $configurationPath -Encoding UTF8 -Raw | ConvertFrom-Json
# Decode the supplied gate rather than supplying timing defaults.
# 解码给定门禁，不提供时间默认值。
$gate = Get-Content -LiteralPath $Gates -Encoding UTF8 -Raw | ConvertFrom-Json
# Derive the actual interval from the exact declared benchmark or soak phase.
# 从精确声明基准或长稳阶段派生实际间隔。
$intervalMs = if ($workload.soak) { $gate.soak_sample_interval_ms } else { $gate.benchmark_sample_interval_ms }
if ($intervalMs -le 0) { throw 'Sampling interval must be explicitly positive.' }
if ($MaximumProcessDurationMs -le 0) { throw 'Outer process budget must be explicitly positive.' }
# Resolve every new output once against the caller directory before changing the child's working directory.
# 在改变子进程工作目录之前，相对调用者目录唯一解析全部新输出。
$InvocationOutput = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($InvocationOutput)
$ResourceOutput = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($ResourceOutput)
$Stdout = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Stdout)
$Stderr = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Stderr)
# Reject collisions between the four independent output owners before starting the process.
# 在启动进程前拒绝四个独立输出所有者之间的路径碰撞。
$uniqueOutputPaths = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
# Refuse existing outputs so a rerun cannot overwrite actual earlier evidence.
# 拒绝既有输出，避免重跑覆盖真实先前证据。
foreach ($outputPath in @($InvocationOutput, $ResourceOutput, $Stdout, $Stderr)) {
    if (-not $uniqueOutputPaths.Add($outputPath)) { throw "Output paths must be distinct: $outputPath" }
    if (Test-Path -LiteralPath $outputPath) { throw "Output already exists: $outputPath" }
    if (-not (Test-Path -LiteralPath (Split-Path -Parent $outputPath) -PathType Container)) { throw "Output parent directory is missing: $outputPath" }
}
# Use UTF-8 without a BOM for interoperable JSONL observations.
# 使用无字节顺序标记的 UTF-8 保存可互操作 JSONL 观察。
$encoding = New-Object System.Text.UTF8Encoding($false)
# Quote the two exact file arguments for Windows Start-Process argument serialization.
# 为 Windows Start-Process 参数序列化引用两个精确文件参数。
$consumerArguments = @(('"' + $configurationPath + '"'), ('"' + $InvocationOutput + '"'))
# Require an already-attached parent console before inheriting it; zero means an explicit launch precondition failure.
# 在继承之前要求父级已附加控制台；零明确表示启动前置条件失败。
Add-Type -TypeDefinition @'
/// Query only the calling sampler's native console membership; this type never creates or attaches a console.
/// 仅查询调用采样器的原生控制台成员；此类型绝不创建或附加控制台。
public static class PluginRuntimeValidationConsole
{
    /// Fill processIds up to capacity; return the actual attached console process count, or zero on failure.
    /// 向 processIds 写入最多 capacity 个编号；返回已附加控制台真实进程数，失败返回零。
    [System.Runtime.InteropServices.DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
    public static extern uint GetConsoleProcessList([System.Runtime.InteropServices.Out] uint[] processIds, uint capacity);
}
'@
# A single-element buffer suffices because the API returns the full membership count even when the buffer is smaller.
# 单元素缓冲区足够，因为 API 在缓冲区不足时仍返回完整成员数量。
$attachedConsoleProcessIds = New-Object uint32[] 1
# Positive native membership proves the exact parent console exists; do not infer it from window visibility.
# 正原生成员数证明精确父控制台存在；不从窗口可见性推断。
$attachedConsoleProcessCount = [PluginRuntimeValidationConsole]::GetConsoleProcessList($attachedConsoleProcessIds, 1)
if ($attachedConsoleProcessCount -eq 0) {
    throw ('Sampler requires an already-attached parent console; Win32 error: ' + [System.Runtime.InteropServices.Marshal]::GetLastWin32Error())
}
# Own the native process handle from creation; Start-Process's detached result can lose short-lived exit status.
# 从创建时拥有原生进程句柄；Start-Process 的分离结果可能丢失短寿命退出状态。
$consumer = New-Object System.Diagnostics.Process
$consumer.StartInfo.FileName = $executablePath
$consumer.StartInfo.Arguments = $consumerArguments -join ' '
$consumer.StartInfo.WorkingDirectory = Split-Path -Parent $InvocationOutput
$consumer.StartInfo.UseShellExecute = $false
# Inherit the verified parent console instead of creating a separate hidden console host; redirected streams remain explicit.
# 继承已核验父控制台，不创建独立隐藏控制台宿主；重定向流仍保持显式。
$consumer.StartInfo.CreateNoWindow = $false
$consumer.StartInfo.RedirectStandardOutput = $true
$consumer.StartInfo.RedirectStandardError = $true
# Measure observation time independently from the consumer's invoke latency.
# 独立于消费者调用时延测量观察时间。
$observationClock = [System.Diagnostics.Stopwatch]::StartNew()
# Preserve observational failure separately from the native exit so successful cleanup cannot hide it.
# 将观察失败与原生退出分开保留，避免成功清理掩盖采样故障。
$samplingFailure = $null
# Record an explicit forced stop only if the frozen outer process deadline actually expires.
# 仅在冻结外层进程截止时间实际耗尽时记录明确强制停止。
$forcedTermination = $false
# Distinguish failed process creation from a live handle that must always be reaped.
# 区分进程创建失败与始终必须回收的活动句柄。
$consumerStarted = $false
try {
    if (-not $consumer.Start()) { throw 'Native consumer failed to start.' }
    $consumerStarted = $true
    # Read both small terminal streams asynchronously so neither redirected pipe can block the original consumer.
    # 异步读取两个小型终态流，避免任一重定向管道阻塞原消费者。
    $stdoutTask = $consumer.StandardOutput.ReadToEndAsync()
    # Preserve the actual native error stream independently from stdout and invocation records.
    # 独立于标准输出及调用记录保留真实原生错误流。
    $stderrTask = $consumer.StandardError.ReadToEndAsync()
    while (-not $consumer.HasExited) {
        if ($observationClock.ElapsedMilliseconds -ge $MaximumProcessDurationMs) {
            $samplingFailure = 'Native consumer exceeded the frozen outer process deadline.'
            $forcedTermination = $true
            $consumer.Kill()
            break
        }
        if ($null -ne $samplingFailure) {
            # Stop only observation after failure; keep waiting for the same original consumer to drain and close.
            # 故障之后仅停止观察，继续等待同一原消费者排空并关闭。
            [void]$consumer.WaitForExit([int][Math]::Max(0, [Math]::Min($intervalMs, $MaximumProcessDurationMs - $observationClock.ElapsedMilliseconds)))
            continue
        }
        try {
            $consumer.Refresh()
            if ($consumer.HasExited) { break }
            # Capture live counters before the slower child inventory; null counters never stand in for zero usage.
            # 在较慢子进程清单前捕获活动计数；空计数绝不替代零用量。
            $liveCounters = [ordered]@{ event = 'process_sample'; elapsed_ms = $observationClock.ElapsedMilliseconds; pid = $consumer.Id; working_set_bytes = $consumer.WorkingSet64; private_bytes = $consumer.PrivateMemorySize64; threads = $consumer.Threads.Count; handles = $consumer.HandleCount; interval_ms = $intervalMs; inherited_parent_console_processes = $attachedConsoleProcessCount }
            if ($null -eq $liveCounters.working_set_bytes -or $null -eq $liveCounters.handles) {
                if ($consumer.HasExited) { break }
                throw 'Live process counters are unavailable.'
            }
            # Query immediate children of this exact original PID; no unrelated processes contribute.
            # 查询此精确原进程编号的直接子进程；无关进程不参与。
            # Bound the query by the remaining caller budget so an observer failure cannot wait forever.
            # 使用调用者剩余预算限制查询，避免观察器故障无限等待。
            $remainingQuerySeconds = [uint32][Math]::Max(1, [Math]::Ceiling(($MaximumProcessDurationMs - $observationClock.ElapsedMilliseconds) / 1000))
            $children = @(Get-CimInstance Win32_Process -Filter ("ParentProcessId = " + $consumer.Id) -OperationTimeoutSec $remainingQuerySeconds)
            # Retain actual sampled counters, with their explicit sampled rather than continuous scope.
            # 保留真实采样计数，明确其采样范围而非连续范围。
            $sample = $liveCounters
            $sample.immediate_child_processes = $children.Count
            # Preserve actual child identities so a nonzero count can be traced rather than guessed or reclassified.
            # 保留真实子进程身份，使非零计数可追溯，不猜测或重分类。
            $sample.immediate_children = @($children | Select-Object ProcessId, ParentProcessId, Name)
            $sample.child_observation_elapsed_ms = $observationClock.ElapsedMilliseconds
            [System.IO.File]::AppendAllText($ResourceOutput, (($sample | ConvertTo-Json -Compress) + [Environment]::NewLine), $encoding)
        } catch {
            # Preserve a genuine observation error even when the process exits concurrently with the failing query.
            # 即使进程与失败查询并发退出，也保留真实观察错误。
            $samplingFailure = $_.Exception.Message
        }
        if (-not $consumer.HasExited) {
            [void]$consumer.WaitForExit([int][Math]::Max(0, [Math]::Min($intervalMs, $MaximumProcessDurationMs - $observationClock.ElapsedMilliseconds)))
        }
    }
    $consumer.WaitForExit()
    # Preserve the actual terminal status even when the consumer correctly fails a validation gate.
    # 即使消费者正确判定验收门禁失败，也保留真实终态。
    $terminal = [ordered]@{ event = 'process_exit'; elapsed_ms = $observationClock.ElapsedMilliseconds; pid = $consumer.Id; exit_code = $consumer.ExitCode; sampling_failure = $samplingFailure; forced_termination = $forcedTermination }
    if ($null -eq $terminal.exit_code) { throw 'Original native process exit code is unavailable.' }
    try {
        [System.IO.File]::WriteAllText($Stdout, $stdoutTask.GetAwaiter().GetResult(), $encoding)
    } catch {
        $samplingFailure = "$samplingFailure Native stdout output: $($_.Exception.Message)"
    }
    try {
        [System.IO.File]::WriteAllText($Stderr, $stderrTask.GetAwaiter().GetResult(), $encoding)
    } catch {
        $samplingFailure = "$samplingFailure Native stderr output: $($_.Exception.Message)"
    }
    $terminal.sampling_failure = $samplingFailure
    try {
        [System.IO.File]::AppendAllText($ResourceOutput, (($terminal | ConvertTo-Json -Compress) + [Environment]::NewLine), $encoding)
    } catch {
        $samplingFailure = "$samplingFailure Resource terminal output: $($_.Exception.Message)"
        $terminal.sampling_failure = $samplingFailure
    }
    # Emit terminal evidence independently of the resource file, which itself may be the failed observer output.
    # 独立于资源文件输出终态证据，资源文件自身可能就是失败的观察输出。
    Write-Output ($terminal | ConvertTo-Json -Compress)
    if ($null -ne $samplingFailure) { exit 1 }
    exit $consumer.ExitCode
} finally {
    # Never abandon the already-started native process when output collection or the observer itself fails.
    # 输出收集或观察器自身失败时，绝不遗弃已经启动的原生进程。
    if ($consumerStarted -and -not $consumer.HasExited) {
        $consumer.Kill()
        $consumer.WaitForExit()
    }
    $consumer.Dispose()
}
