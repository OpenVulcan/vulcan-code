# run_tauri.ps1 runs the compiled Tauri/WebUI main application without managing Vite.
# run_tauri.ps1 用于运行已编译的 Tauri/WebUI 主应用，但不管理 Vite。
# Usage:
# 用法：
#   .\run_tauri.ps1           # run debug build; start Vite separately with make vite start
#   .\run_tauri.ps1 -Release  # run release build from built WebUI assets

param(
    # Release selects the release output instead of the debug output.
    # Release 用于选择 release 产物而不是 debug 产物。
    [switch]$Release
)

$ErrorActionPreference = "Stop"

# Relaunch under PowerShell 7 when available so runtime process behavior stays predictable.
# 若检测到 PowerShell 7，则自动在其下重启，使运行时进程行为保持稳定可预测。
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
        # ForwardedArguments preserves only the explicitly requested runtime profile.
        # ForwardedArguments 仅保留显式请求的运行时 profile。
        $ForwardedArguments = @()
        if ($Release) {
            $ForwardedArguments += "-Release"
        }
        & $PowerShellCore.Source -NoProfile -ExecutionPolicy Bypass -File $CurrentScriptPath @ForwardedArguments
        exit $LASTEXITCODE
    }
}

# ProjectDir stores the repository root for stable output paths.
# ProjectDir 保存仓库根目录，确保输出路径稳定。
$ProjectDir = Split-Path $PSScriptRoot -Parent

# IsWindowsPlatform records whether the launched artifact requires the Windows executable suffix.
# IsWindowsPlatform 记录启动制品是否需要 Windows 可执行文件后缀。
$IsWindowsPlatform = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)

# BinaryFileName stores the platform-specific Tauri shell binary name.
# BinaryFileName 保存平台相关的 Tauri 桌面壳二进制名称。
$BinaryFileName = "vulcan-code-tauri"
if ($IsWindowsPlatform) {
    $BinaryFileName = "vulcan-code-tauri.exe"
}

# OutputBinDir stores the stable runtime binary directory produced by make build.
# OutputBinDir 保存 make build 产出的稳定运行时二进制目录。
$OutputBinDir = Join-Path $ProjectDir "output\bin"

# OutputBinaryPath stores the exact executable launched by this script.
# OutputBinaryPath 保存本脚本启动的精确可执行文件路径。
$OutputBinaryPath = Join-Path $OutputBinDir $BinaryFileName

if (-not (Test-Path -LiteralPath $OutputBinaryPath -PathType Leaf)) {
    # BuildCommand tells the user which build mode should produce the missing executable.
    # BuildCommand 告知用户应使用哪种构建模式产出缺失的可执行文件。
    $BuildCommand = if ($Release) { "./make.ps1 build release" } else { "./make.ps1 build" }
    throw "Missing built Tauri binary: $OutputBinaryPath. Run $BuildCommand first."
}

# ProfileName describes the selected runtime profile in command output.
# ProfileName 在命令输出中描述选定的运行时 profile。
$ProfileName = if ($Release) { "release" } else { "debug" }
Write-Host "==> Running Vulcan Code Tauri $ProfileName from output/bin..."
& $OutputBinaryPath
exit $LASTEXITCODE
