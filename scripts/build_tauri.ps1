# build_tauri.ps1 builds the new Tauri/WebUI main application.
# build_tauri.ps1 用于构建新的 Tauri/WebUI 主应用。
# Usage:
# 用法：
#   .\build_tauri.ps1           # debug build
#   .\build_tauri.ps1 -Release  # release build

param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"

# Relaunch under PowerShell 7 when available so frontend and Rust commands share UTF-8 behavior.
# 若检测到 PowerShell 7，则自动在其下重启，确保前端与 Rust 命令共享稳定的 UTF-8 行为。
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

# TauriAppDir points at the Tauri desktop shell crate.
# TauriAppDir 指向 Tauri 桌面壳 crate。
$TauriAppDir = Join-Path $ProjectDir "apps\vulcan-code-tauri"

Set-Location $ProjectDir

if (-not (Test-Path -LiteralPath (Join-Path $WebAppDir "node_modules"))) {
    Write-Host "==> Installing WebUI dependencies..."
    pnpm --dir $WebAppDir install
}

Write-Host "==> Building WebUI..."
pnpm --dir $WebAppDir run build
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

if ($Release) {
    Write-Host "==> Building Tauri shell release..."
    cargo build -p vulcan-code-tauri --release
}
else {
    Write-Host "==> Building Tauri shell debug..."
    cargo build -p vulcan-code-tauri
}
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

# ProfileName selects the Cargo profile directory that contains the built shell.
# ProfileName 选择包含已构建桌面壳的 Cargo profile 目录。
$ProfileName = if ($Release) { "release" } else { "debug" }

# BinaryFileName stores the platform-specific Tauri shell binary name.
# BinaryFileName 保存平台相关的 Tauri 桌面壳二进制名称。
$BinaryFileName = "vulcan-code-tauri"
if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
    $BinaryFileName = "vulcan-code-tauri.exe"
}

$BuiltBinaryPath = Join-Path $ProjectDir "target\$ProfileName\$BinaryFileName"
Write-Host "==> Done. Binary: $BuiltBinaryPath"
