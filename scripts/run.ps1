# run.ps1 launches one packaged vulcan-code-gui binary from the output directory.
# run.ps1 用于从 output 目录启动一个已打包的 vulcan-code-gui 二进制。
# Usage:
# 用法：
#   .\run.ps1           # run debug build
#   .\run.ps1 release   # run release build
#   .\run.ps1 -Release  # run release build

param(
    [switch]$Release
)

# Relaunch under PowerShell 7 when available so the packaged debug helpers behave consistently.
# 若检测到 PowerShell 7，则自动在其下重启，确保打包调试辅助脚本行为一致。
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
        elseif ($args.Count -gt 0) {
            $ForwardedArguments += $args
        }
        & $PowerShellCore.Source -NoProfile -ExecutionPolicy Bypass -File $CurrentScriptPath @ForwardedArguments
        exit $LASTEXITCODE
    }
}

$ProjectDir = Split-Path $PSScriptRoot -Parent
Set-Location $ProjectDir

$BinaryFileName = "vulcan-code-gui"
if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
    $BinaryFileName = "vulcan-code-gui.exe"
}

$OutputRoot = Join-Path $ProjectDir "output"
if ($Release -or ($args.Count -gt 0 -and $args[0] -eq "release")) {
    $BinPath = Join-Path $ProjectDir "output\bin\$BinaryFileName"
}
else {
    $BinPath = Join-Path $ProjectDir "output\debug\$BinaryFileName"
}

if (-not (Test-Path $BinPath)) {
    Write-Host "==> Binary not found: $BinPath"
    Write-Host "==> Run 'make' or 'make release' first."
    exit 1
}

if (Test-Path (Join-Path $OutputRoot "bin")) {
    $env:PATH = "$(Join-Path $OutputRoot 'bin');$env:PATH"
}
$env:VULCAN_CODE_OUTPUT_ROOT = $OutputRoot

Write-Host "==> Running vulcan-code-gui ($BinPath)..."
Write-Host ""

& $BinPath
$code = $LASTEXITCODE

Write-Host ""
Write-Host "==> Process exited with code $code."
exit $code
