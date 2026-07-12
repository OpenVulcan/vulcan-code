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

# IsWindowsPlatform records whether the current build target is the Windows host platform.
# IsWindowsPlatform 记录当前构建目标是否为 Windows 宿主平台。
$IsWindowsPlatform = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)

# SourceConfigDir points at the repository-owned packaged configuration templates.
# SourceConfigDir 指向仓库持有的打包配置模板目录。
$SourceConfigDir = Join-Path $ProjectDir "config"

# SourceResourcesDir points at the repository-owned runtime resource catalog.
# SourceResourcesDir 指向仓库持有的运行时资源目录。
$SourceResourcesDir = Join-Path $ProjectDir "crates\vulcan-runtime-assets\resources"

# SourceSystemPromptSkillsDir points at repository-owned system Agent Skills.
# SourceSystemPromptSkillsDir 指向仓库持有的系统 Agent Skills 目录。
$SourceSystemPromptSkillsDir = Join-Path $ProjectDir "crates\vulcan-prompt-skills\src\assets\system"

# SourceRtkBinDir points at the RTK executable installed by make deps.
# SourceRtkBinDir 指向 make deps 安装的 RTK 可执行文件目录。
$SourceRtkBinDir = Join-Path $ProjectDir "third_party\rtk\bin"

# SourceLuaRuntimeDir points at the official LuaSkills runtime payload installed by make deps-lua.
# SourceLuaRuntimeDir 指向 make deps-lua 安装的官方 LuaSkills 运行时载荷。
$SourceLuaRuntimeDir = Join-Path $ProjectDir "third_party\luaskills_runtime"

# SourceManagedRuntimeScriptsDir points at the tag-pinned managed runtime bootstrap assets.
# SourceManagedRuntimeScriptsDir 指向按标签固定的受管运行时初始化资产。
$SourceManagedRuntimeScriptsDir = Join-Path $ProjectDir "scripts\deps"

# SourceLuaDebugPluginDir points at the host-owned debug System Plugin bundled with Vulcan Code.
# SourceLuaDebugPluginDir 指向随 Vulcan Code 打包的宿主自有调试 System Plugin。
$SourceLuaDebugPluginDir = Join-Path $ProjectDir "crates\vulcan-runtime-assets\luaskills-system-plugins\vulcan-debug-runtime"

Set-Location $ProjectDir

# Copy-DirectoryContents overlays one source directory onto one output directory.
# Copy-DirectoryContents 用于把一个源目录覆盖同步到一个输出目录。
# SourceDirectory is the directory whose immediate children should be copied.
# SourceDirectory 表示需要复制其直接子项的源目录。
# DestinationDirectory is the output directory that receives the copied children.
# DestinationDirectory 表示接收复制子项的输出目录。
# Clean removes the existing destination before copying.
# Clean 表示复制前先删除已有目标目录。
# This function returns no value and throws when the source directory is unavailable.
# 此函数不返回值，并在源目录不可用时抛出异常。
function Copy-DirectoryContents {
    param(
        [string]$SourceDirectory,
        [string]$DestinationDirectory,
        [switch]$Clean
    )

    if (-not (Test-Path -LiteralPath $SourceDirectory -PathType Container)) {
        throw "Missing source directory: $SourceDirectory"
    }

    if ($Clean -and (Test-Path -LiteralPath $DestinationDirectory)) {
        Remove-Item -LiteralPath $DestinationDirectory -Recurse -Force
    }

    New-Item -ItemType Directory -Path $DestinationDirectory -Force | Out-Null
    Get-ChildItem -LiteralPath $SourceDirectory -Force | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination $DestinationDirectory -Recurse -Force
    }
}

if ($Release) {
    if (-not (Test-Path -LiteralPath (Join-Path $WebAppDir "node_modules"))) {
        Write-Host "==> Installing WebUI dependencies..."
        pnpm --dir $WebAppDir install
    }

    # Release builds package the static WebUI assets into the desktop shell.
    # Release 构建会把静态 WebUI 产物打包进桌面壳。
    Write-Host "==> Building WebUI for release..."
    pnpm --dir $WebAppDir run build
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }

    Write-Host "==> Building Tauri shell release..."
    if ($IsWindowsPlatform) {
        cargo build -p vulcan-code-tauri -p vulcan-windows-console-helper --release --features custom-protocol
    }
    else {
        cargo build -p vulcan-code-tauri --release --features custom-protocol
    }
}
else {
    Write-Host "==> Building Tauri shell debug..."
    if ($IsWindowsPlatform) {
        cargo build -p vulcan-code-tauri -p vulcan-windows-console-helper
    }
    else {
        cargo build -p vulcan-code-tauri
    }
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
if ($IsWindowsPlatform) {
    $BinaryFileName = "vulcan-code-tauri.exe"
}

$BuiltBinaryPath = Join-Path $ProjectDir "target\$ProfileName\$BinaryFileName"

# HelperFileName stores the platform-specific Windows console helper binary name.
# HelperFileName 保存平台相关的 Windows 控制台 helper 二进制名称。
$HelperFileName = "vulcan-windows-console-helper"
if ($IsWindowsPlatform) {
    $HelperFileName = "vulcan-windows-console-helper.exe"
}

# BuiltHelperPath stores the helper executable built for the same Cargo profile as the shell.
# BuiltHelperPath 保存与桌面壳相同 Cargo profile 构建出的 helper 可执行文件。
$BuiltHelperPath = Join-Path $ProjectDir "target\$ProfileName\$HelperFileName"

# OutputBinDir stores the stable runtime binary output directory used by make run.
# OutputBinDir 保存 make run 使用的稳定运行时二进制输出目录。
$OutputBinDir = Join-Path $ProjectDir "output\bin"

# OutputBinaryPath stores the copied executable path that users and run scripts should launch.
# OutputBinaryPath 保存用户与运行脚本应启动的已复制可执行文件路径。
$OutputBinaryPath = Join-Path $OutputBinDir $BinaryFileName

# OutputHelperPath stores the copied Windows console helper beside the shell binary.
# OutputHelperPath 保存复制到桌面壳旁边的 Windows 控制台 helper。
$OutputHelperPath = Join-Path $OutputBinDir $HelperFileName

# OutputConfigDir stores the packaged configuration read by output/bin runtime launches.
# OutputConfigDir 保存 output/bin 运行时启动读取的打包配置目录。
$OutputConfigDir = Join-Path $ProjectDir "output\config"

# OutputResourcesDir stores the packaged resource catalog read by output/bin runtime launches.
# OutputResourcesDir 保存 output/bin 运行时启动读取的打包资源目录。
$OutputResourcesDir = Join-Path $ProjectDir "output\resources"

# OutputSystemPromptSkillsDir stores packaged system Agent Skills read directly by the runtime.
# OutputSystemPromptSkillsDir 保存运行时直接读取的打包系统 Agent Skills。
$OutputSystemPromptSkillsDir = Join-Path $OutputResourcesDir "prompt-skills\system"

# OutputToolsDir stores packaged auxiliary host tool executables.
# OutputToolsDir 保存打包后的辅助宿主工具可执行文件。
$OutputToolsDir = Join-Path $ProjectDir "output\tools"

# LegacyOutputRtkToolDir stores the previous nested RTK output directory for cleanup.
# LegacyOutputRtkToolDir 保存旧版嵌套 RTK 输出目录，用于清理。
$LegacyOutputRtkToolDir = Join-Path $OutputToolsDir "rtk"

# OutputLuaRuntimeDir stores the application-owned LuaSkills runtime root.
# OutputLuaRuntimeDir 保存应用持有的 LuaSkills 运行时根目录。
$OutputLuaRuntimeDir = Join-Path $ProjectDir "output\lua_runtime"

# OutputManagedRuntimeScriptsDir stores bootstrap scripts executed during service startup.
# OutputManagedRuntimeScriptsDir 保存服务启动期间执行的初始化脚本。
$OutputManagedRuntimeScriptsDir = Join-Path $OutputLuaRuntimeDir "scripts\deps"

# OutputLuaDebugPluginDir is the exact host-owned System Plugin package discovered by LuaSkills.
# OutputLuaDebugPluginDir 是 LuaSkills 发现的精确宿主自有 System Plugin 包。
$OutputLuaDebugPluginDir = Join-Path $OutputLuaRuntimeDir "system_lua_lib\vulcan-debug-runtime"

if (-not (Test-Path -LiteralPath $BuiltBinaryPath)) {
    throw "Built Tauri binary was not found: $BuiltBinaryPath"
}
if ($IsWindowsPlatform -and -not (Test-Path -LiteralPath $BuiltHelperPath)) {
    throw "Built Windows console helper was not found: $BuiltHelperPath"
}

New-Item -ItemType Directory -Path $OutputBinDir -Force | Out-Null
Copy-Item -LiteralPath $BuiltBinaryPath -Destination $OutputBinaryPath -Force
if ($IsWindowsPlatform) {
    Copy-Item -LiteralPath $BuiltHelperPath -Destination $OutputHelperPath -Force
}

Write-Host "==> Syncing packaged config..."
Copy-DirectoryContents -SourceDirectory $SourceConfigDir -DestinationDirectory $OutputConfigDir

Write-Host "==> Syncing packaged resources..."
Copy-DirectoryContents -SourceDirectory $SourceResourcesDir -DestinationDirectory $OutputResourcesDir

Write-Host "==> Syncing packaged system Agent Skills..."
Copy-DirectoryContents -SourceDirectory $SourceSystemPromptSkillsDir -DestinationDirectory $OutputSystemPromptSkillsDir -Clean

Write-Host "==> Syncing LuaSkills runtime payload..."
Copy-DirectoryContents -SourceDirectory $SourceLuaRuntimeDir -DestinationDirectory $OutputLuaRuntimeDir

Write-Host "==> Syncing managed runtime bootstrap assets..."
Copy-DirectoryContents -SourceDirectory $SourceManagedRuntimeScriptsDir -DestinationDirectory $OutputManagedRuntimeScriptsDir -Clean

Write-Host "==> Syncing host System Plugin packages..."
Copy-DirectoryContents -SourceDirectory $SourceLuaDebugPluginDir -DestinationDirectory $OutputLuaDebugPluginDir -Clean

if (Test-Path -LiteralPath $SourceRtkBinDir -PathType Container) {
    Write-Host "==> Syncing packaged RTK host tool..."
    if (Test-Path -LiteralPath $LegacyOutputRtkToolDir) {
        Remove-Item -LiteralPath $LegacyOutputRtkToolDir -Recurse -Force
    }
    New-Item -ItemType Directory -Path $OutputToolsDir -Force | Out-Null
    Get-ChildItem -LiteralPath $SourceRtkBinDir -File -Force | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $OutputToolsDir $_.Name) -Force
    }
} else {
    Write-Host "==> RTK host tool not installed; run make deps-host to package it."
}

Write-Host "==> Done. Binary: $OutputBinaryPath"
if ($IsWindowsPlatform) {
    Write-Host "==> Done. Windows console helper: $OutputHelperPath"
}
