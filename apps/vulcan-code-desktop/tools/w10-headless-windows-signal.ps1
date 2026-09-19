# Launch one isolated Core with a private hidden Windows console and deliver a real control event.
# 在专用隐藏 Windows 控制台启动隔离 Core，并投递真实控制事件。
param(
    [int]$SignalPid = 0,
    [string]$ServiceExecutable = '',
    [string]$AssetRoot = '',
    [switch]$ExpectFirstShutdownFailure
)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

// The native console launcher keeps exact ownership of the child it signals.
// 原生控制台启动器只向自己持有的精确子进程投递信号。
public static class IsolatedConsoleSignal {
    // Retain the exact child process handle so its exit code remains observable after Ctrl-C.
    // 保留精确子进程句柄，使 Ctrl-C 后仍可读取其退出码。
    private static IntPtr ownedProcessHandle = IntPtr.Zero;
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct STARTUPINFO {
        public int cb;
        public string lpReserved;
        public string lpDesktop;
        public string lpTitle;
        public int dwX;
        public int dwY;
        public int dwXSize;
        public int dwYSize;
        public int dwXCountChars;
        public int dwYCountChars;
        public int dwFillAttribute;
        public int dwFlags;
        public short wShowWindow;
        public short cbReserved2;
        public IntPtr lpReserved2;
        public IntPtr hStdInput;
        public IntPtr hStdOutput;
        public IntPtr hStdError;
    }
    [StructLayout(LayoutKind.Sequential)]
    public struct PROCESS_INFORMATION {
        public IntPtr hProcess;
        public IntPtr hThread;
        public int dwProcessId;
        public int dwThreadId;
    }
    [DllImport("kernel32.dll", EntryPoint = "CreateProcessW", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CreateProcess(string application, StringBuilder commandLine,
        IntPtr processAttributes, IntPtr threadAttributes, bool inheritHandles, uint flags,
        IntPtr environment, string currentDirectory, ref STARTUPINFO startup,
        out PROCESS_INFORMATION process);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GetExitCodeProcess(IntPtr handle, out uint code);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool AttachConsole(uint processId);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool FreeConsole();
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool SetConsoleCtrlHandler(IntPtr handler, bool add);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GenerateConsoleCtrlEvent(uint eventType, uint processGroupId);

    // Start gives the child a hidden private console and returns its exact PID.
    // Start 为子进程提供隐藏的专用控制台，并返回精确 PID。
    public static int Start(string executable, string testRoot, string currentDirectory) {
        var startup = new STARTUPINFO();
        startup.cb = Marshal.SizeOf<STARTUPINFO>();
        startup.dwFlags = 0x00000001;
        startup.wShowWindow = 0;
        var command = new StringBuilder("\"" + executable + "\" --headless --test-root=\"" + testRoot + "\"");
        PROCESS_INFORMATION process;
        if (!CreateProcess(executable, command, IntPtr.Zero, IntPtr.Zero, false, 0x00000010,
            IntPtr.Zero, currentDirectory, ref startup, out process)) {
            throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
        }
        CloseHandle(process.hThread);
        ownedProcessHandle = process.hProcess;
        return process.dwProcessId;
    }

    // ExitCode reads the exact owned process handle after it has exited.
    // ExitCode 在精确持有的子进程退出后读取其退出码。
    public static uint ExitCode() {
        uint code;
        if (!GetExitCodeProcess(ownedProcessHandle, out code))
            throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
        CloseHandle(ownedProcessHandle);
        ownedProcessHandle = IntPtr.Zero;
        return code;
    }

    // Interrupt attaches to only the child console and broadcasts Ctrl-C there.
    // Interrupt 只附加该子进程的控制台，并在其中广播 Ctrl-C。
    public static void Interrupt(int processId) {
        // The invoking PowerShell may already be attached to its own hidden console.
        // 调用方 PowerShell 可能已附加自己的隐藏控制台。
        FreeConsole();
        if (!AttachConsole((uint)processId))
            throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
        try {
            if (!SetConsoleCtrlHandler(IntPtr.Zero, true))
                throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
            if (!GenerateConsoleCtrlEvent(0, 0))
                throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
        } finally {
            FreeConsole();
            SetConsoleCtrlHandler(IntPtr.Zero, false);
        }
    }
}
'@
if ($SignalPid -ne 0) {
    [IsolatedConsoleSignal]::Interrupt($SignalPid)
    exit 0
}
$workspace = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../../..')).Path
if (-not [IO.Path]::IsPathFullyQualified($ServiceExecutable)) {
    throw 'ServiceExecutable must be an explicit absolute path'
}
$executable = (Resolve-Path -LiteralPath $ServiceExecutable).Path
$testRoot = Join-Path $workspace ('target\w11-headless-signal-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
if ($AssetRoot -ne '') {
    if (-not [IO.Path]::IsPathFullyQualified($AssetRoot)) { throw 'AssetRoot must be absolute' }
    $env:VULCAN_CODE_OUTPUT_ROOT = (Resolve-Path -LiteralPath $AssetRoot).Path
}
if ($ExpectFirstShutdownFailure) {
    $env:VULCAN_TEST_FAIL_FIRST_HEADLESS_SHUTDOWN = '1'
} else {
    Remove-Item -LiteralPath 'Env:VULCAN_TEST_FAIL_FIRST_HEADLESS_SHUTDOWN' -ErrorAction SilentlyContinue
}
$childPid = 0
try {
    $childPid = [IsolatedConsoleSignal]::Start($executable, $testRoot, $workspace)
    $child = [System.Diagnostics.Process]::GetProcessById($childPid)
    $discoveryPath = Join-Path $testRoot 'user-vulcan\code\webui-service.json'
    $deadline = [DateTime]::UtcNow.AddSeconds(120)
    $discovery = $null
    while ([DateTime]::UtcNow -lt $deadline) {
        if ($child.HasExited) { throw "Core exited before ready: $($child.ExitCode)" }
        if (Test-Path -LiteralPath $discoveryPath -PathType Leaf) {
            try {
                $discovery = Get-Content -LiteralPath $discoveryPath -Raw | ConvertFrom-Json
                if ($discovery.status -eq 'ready' -and $discovery.pid -eq $childPid) { break }
            } catch { }
        }
        Start-Sleep -Milliseconds 100
    }
    if ($null -eq $discovery -or $discovery.status -ne 'ready') { throw 'Core did not become ready' }
    $headers = @{ 'x-vulcan-service-token' = $discovery.serviceToken }
    $liveStatus = Invoke-RestMethod -Uri ($discovery.baseUrl + '/v1/service/status') -Headers $headers -TimeoutSec 3
    if ($liveStatus.pid -ne $childPid -or $liveStatus.lifecycle -ne 'ready') { throw 'Core readiness did not match the child' }
    # Signal generation runs in a hidden helper because Windows broadcasts control events to the sender too.
    # 信号在隐藏辅助进程中生成，因为 Windows 也会把控制事件广播给发送者。
    $sender = Start-Process -FilePath (Get-Command pwsh).Source -ArgumentList @(
        '-NoProfile', '-File', $PSCommandPath, '-SignalPid', [string]$childPid
    ) -WindowStyle Hidden -Wait -PassThru
    if ($ExpectFirstShutdownFailure) {
        Start-Sleep -Milliseconds 1000
        if ($child.HasExited) { throw 'Core exited after an injected shutdown failure' }
        $stillReady = Invoke-RestMethod -Uri ($discovery.baseUrl + '/v1/service/status') -Headers $headers -TimeoutSec 3
        if ($stillReady.pid -ne $childPid -or $stillReady.lifecycle -ne 'ready') {
            throw 'Core did not retain its ready owner after a recoverable shutdown failure'
        }
        $sender = Start-Process -FilePath (Get-Command pwsh).Source -ArgumentList @(
            '-NoProfile', '-File', $PSCommandPath, '-SignalPid', [string]$childPid
        ) -WindowStyle Hidden -Wait -PassThru
    }
    if (-not $child.WaitForExit(30000)) { throw 'Core ignored real Ctrl-C for 30 seconds' }
    $child.Refresh()
    $exitCode = [IsolatedConsoleSignal]::ExitCode()
    Write-Output "SIGNAL_DIAGNOSTIC sender=$($sender.ExitCode) hasExited=$($child.HasExited) exitCode=$exitCode"
    if ($exitCode -ne 0) { throw "Core shutdown exit code $exitCode" }
    $portClosed = $false
    try {
        Invoke-WebRequest -Uri ($discovery.baseUrl + '/health') -TimeoutSec 2 | Out-Null
    } catch {
        $portClosed = $true
    }
    if (-not $portClosed) { throw 'Core HTTP port remained open after process exit' }
    # A successful drain must revoke the exact discovery generation before process exit.
    # 成功排空必须在进程退出前撤销精确的发现记录代次。
    if (Test-Path -LiteralPath $discoveryPath) { throw 'Core left a ready discovery record after successful shutdown' }
    Write-Output "HEADLESS_SIGNAL_PASSED pid=$childPid exit=0 portClosed=$portClosed discoveryRemoved=True firstFailureRetained=$($ExpectFirstShutdownFailure.IsPresent)"
} finally {
    if ($childPid -ne 0) {
        try {
            $child = [System.Diagnostics.Process]::GetProcessById($childPid)
            if (-not $child.HasExited) { $child.Kill($true); $child.WaitForExit(10000) | Out-Null }
        } catch [System.ArgumentException] { }
    }
    $resolved = (Resolve-Path -LiteralPath $testRoot).Path
    if (-not $resolved.StartsWith((Join-Path $workspace 'target\w11-headless-signal-'))) {
        throw 'Refusing cleanup outside the test root'
    }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
