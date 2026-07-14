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

# SourceLuaRuntimeDir points at the official LuaSkills runtime payload installed by make deps-lua.
# SourceLuaRuntimeDir 指向 make deps-lua 安装的官方 LuaSkills 运行时载荷。
$SourceLuaRuntimeDir = Join-Path $ProjectDir "third_party\luaskills_runtime"

# SourceSystemPluginsDir contains every manifest-driven plugin bundled with Vulcan Code.
# SourceSystemPluginsDir 包含随 Vulcan Code 打包的所有清单驱动插件。
$SourceSystemPluginsDir = Join-Path $ProjectDir "plugins"

# SourcePluginBridgeDir contains the only LuaSkills System package used by Vulcan plugins.
# SourcePluginBridgeDir 包含 Vulcan 插件使用的唯一 LuaSkills System package。
$SourcePluginBridgeDir = Join-Path $ProjectDir "plugin-bridge\vulcan-plugin-bridge"

Set-Location $ProjectDir

# Test whether one PowerShell file-operation failure is a Windows sharing or lock violation.
# 判断一次 PowerShell 文件操作失败是否属于 Windows 共享冲突或锁冲突。
# ErrorRecord is the exact caught PowerShell error, including nested native exceptions.
# ErrorRecord 是捕获到的精确 PowerShell 错误，包含嵌套的本机异常。
# This function returns true only for Win32 error 32 or 33 and never masks other failures.
# 此函数仅对 Win32 错误 32 或 33 返回 true，绝不掩盖其他失败。
function Test-IsLockedFileError {
    param(
        [System.Management.Automation.ErrorRecord]$ErrorRecord
    )

    # CurrentException walks every nested exception because Copy-Item may wrap the native IOException.
    # CurrentException 遍历每层嵌套异常，因为 Copy-Item 可能包装本机 IOException。
    $CurrentException = $ErrorRecord.Exception
    while ($null -ne $CurrentException) {
        # NativeErrorCode extracts the low Win32 status word from the HRESULT.
        # NativeErrorCode 从 HRESULT 中提取低位 Win32 状态码。
        $NativeErrorCode = $CurrentException.HResult -band 0xFFFF
        if ($NativeErrorCode -eq 32 -or $NativeErrorCode -eq 33) {
            return $true
        }
        $CurrentException = $CurrentException.InnerException
    }
    return $false
}

# Copy one file while allowing an already-running application to retain a locked destination version.
# 复制单个文件，同时允许运行中的应用继续保留被锁定的目标版本。
# SourceFile is the exact source artifact and DestinationFile is its exact output path.
# SourceFile 是精确源制品，DestinationFile 是其精确输出路径。
# This function returns true when copied, false when skipped for a lock, and throws for every other error.
# 此函数复制成功时返回 true，因占用跳过时返回 false，其他任何错误都会抛出。
function Copy-FileAllowLockedDestination {
    param(
        [string]$SourceFile,
        [string]$DestinationFile
    )

    # DestinationParent is created before the guarded copy so missing-directory failures remain explicit.
    # DestinationParent 在受保护复制前创建，使目录缺失类失败保持显式。
    $DestinationParent = Split-Path $DestinationFile -Parent
    New-Item -ItemType Directory -Path $DestinationParent -Force | Out-Null
    try {
        Copy-Item -LiteralPath $SourceFile -Destination $DestinationFile -Force -ErrorAction Stop
        return $true
    }
    catch {
        if (Test-IsLockedFileError -ErrorRecord $_) {
            Write-Warning "Skipped locked output file; restart the running application before using the new artifact: $DestinationFile"
            return $false
        }
        throw
    }
}

# Remove one output item while allowing an already-running application to retain locked files.
# 删除单个输出项，同时允许运行中的应用继续保留被锁定文件。
# LiteralPath is the exact output path and Recurse enables directory cleanup.
# LiteralPath 是精确输出路径，Recurse 用于启用目录清理。
# This function returns true when removed, false for a lock, and throws for every other error.
# 此函数删除成功时返回 true，因占用跳过时返回 false，其他任何错误都会抛出。
function Remove-ItemAllowLocked {
    param(
        [string]$LiteralPath,
        [switch]$Recurse
    )

    try {
        Remove-Item -LiteralPath $LiteralPath -Recurse:$Recurse -Force -ErrorAction Stop
        return $true
    }
    catch {
        if (Test-IsLockedFileError -ErrorRecord $_) {
            Write-Warning "Skipped locked output cleanup; restart the running application to remove it: $LiteralPath"
            return $false
        }
        throw
    }
}

# Copy-DirectoryContents overlays one source directory onto one output directory.
# Copy-DirectoryContents 用于把一个源目录覆盖同步到一个输出目录。
# SourceDirectory is the directory whose immediate children should be copied.
# SourceDirectory 表示需要复制其直接子项的源目录。
# DestinationDirectory is the output directory that receives the copied children.
# DestinationDirectory 表示接收复制子项的输出目录。
# Clean removes the existing destination before copying.
# Clean 表示复制前先删除已有目标目录。
# This function skips only occupied files and throws when the source or any non-lock operation fails.
# 此函数仅跳过被占用文件，并在源目录或任何非占用操作失败时抛出异常。
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
        $null = Remove-ItemAllowLocked -LiteralPath $DestinationDirectory -Recurse
    }

    New-Item -ItemType Directory -Path $DestinationDirectory -Force | Out-Null
    # SourceItems is a stable recursive snapshot used to preserve empty directories and copy files individually.
    # SourceItems 是稳定的递归快照，用于保留空目录并逐文件复制。
    $SourceItems = @(Get-ChildItem -LiteralPath $SourceDirectory -Recurse -Force)
    foreach ($SourceItem in $SourceItems) {
        # RelativePath preserves the exact source tree below the declared root.
        # RelativePath 保留声明源根目录下的精确目录结构。
        $RelativePath = [System.IO.Path]::GetRelativePath($SourceDirectory, $SourceItem.FullName)
        # DestinationPath is the exact mirrored output path for the current source item.
        # DestinationPath 是当前源项对应的精确镜像输出路径。
        $DestinationPath = Join-Path $DestinationDirectory $RelativePath
        if ($SourceItem.PSIsContainer) {
            New-Item -ItemType Directory -Path $DestinationPath -Force | Out-Null
            continue
        }
        $null = Copy-FileAllowLockedDestination -SourceFile $SourceItem.FullName -DestinationFile $DestinationPath
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

# LegacyManagedRuntimeScriptsDir stores obsolete downloader scripts removed by native Rust bootstrap.
# LegacyManagedRuntimeScriptsDir 保存由 Rust 原生初始化取代的废弃下载脚本。
$LegacyManagedRuntimeScriptsDir = Join-Path $OutputLuaRuntimeDir "scripts\deps"

# LegacyManagedRuntimeDistributionDir is the obsolete LuaSkills-private Python/Node location.
# LegacyManagedRuntimeDistributionDir 是已经废弃的 LuaSkills 私有 Python/Node 位置。
$LegacyManagedRuntimeDistributionDir = Join-Path $OutputLuaRuntimeDir "dependencies\runtimes"

# OutputSystemPluginsDir is the Vulcan-owned runtime directory scanned by the plugin host.
# OutputSystemPluginsDir 是插件宿主扫描的 Vulcan 自有运行时目录。
$OutputSystemPluginsDir = Join-Path $ProjectDir "output\plugins"

# OutputPluginBridgeDir is the only plugin-related package deployed below system_lua_lib.
# OutputPluginBridgeDir 是部署到 system_lua_lib 下的唯一插件相关 package。
$OutputPluginBridgeDir = Join-Path $OutputLuaRuntimeDir "system_lua_lib\vulcan-plugin-bridge"

# ManagedSystemPluginIndexPath records only packages owned by this build script for safe stale cleanup.
# ManagedSystemPluginIndexPath 仅记录当前构建脚本拥有的插件包，用于安全清理陈旧项。
$ManagedSystemPluginIndexPath = Join-Path $OutputSystemPluginsDir ".vulcan-code-managed-plugins"

if (-not (Test-Path -LiteralPath $BuiltBinaryPath)) {
    throw "Built Tauri binary was not found: $BuiltBinaryPath"
}
if ($IsWindowsPlatform -and -not (Test-Path -LiteralPath $BuiltHelperPath)) {
    throw "Built Windows console helper was not found: $BuiltHelperPath"
}

Write-Host "==> Syncing packaged config..."
Copy-DirectoryContents -SourceDirectory $SourceConfigDir -DestinationDirectory $OutputConfigDir

Write-Host "==> Syncing packaged resources..."
Copy-DirectoryContents -SourceDirectory $SourceResourcesDir -DestinationDirectory $OutputResourcesDir

Write-Host "==> Syncing packaged system Agent Skills..."
Copy-DirectoryContents -SourceDirectory $SourceSystemPromptSkillsDir -DestinationDirectory $OutputSystemPromptSkillsDir -Clean

Write-Host "==> Syncing LuaSkills runtime payload..."
Copy-DirectoryContents -SourceDirectory $SourceLuaRuntimeDir -DestinationDirectory $OutputLuaRuntimeDir

if (Test-Path -LiteralPath $LegacyManagedRuntimeDistributionDir) {
    Write-Host "==> Removing obsolete LuaSkills-private managed runtimes..."
    $null = Remove-ItemAllowLocked -LiteralPath $LegacyManagedRuntimeDistributionDir -Recurse
}

if (Test-Path -LiteralPath $LegacyManagedRuntimeScriptsDir) {
    Write-Host "==> Removing obsolete managed runtime downloader scripts..."
    $null = Remove-ItemAllowLocked -LiteralPath $LegacyManagedRuntimeScriptsDir -Recurse
}

Write-Host "==> Syncing host System Plugin packages..."
New-Item -ItemType Directory -Path $OutputSystemPluginsDir -Force | Out-Null
$ResolvedOutputSystemPluginsDir = [System.IO.Path]::GetFullPath($OutputSystemPluginsDir).TrimEnd([System.IO.Path]::DirectorySeparatorChar)
$SourceSystemPluginNames = @(
    Get-ChildItem -LiteralPath $SourceSystemPluginsDir -Directory -Force |
        Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName "plugin.yaml") -PathType Leaf } |
        Select-Object -ExpandProperty Name
)
if ($SourceSystemPluginNames.Count -eq 0) {
    throw "No manifest-driven System Plugin packages were found in: $SourceSystemPluginsDir"
}
foreach ($SystemPluginName in $SourceSystemPluginNames) {
    if ($SystemPluginName -notmatch '^[a-z][a-z0-9_-]*$') {
        throw "Invalid System Plugin source directory name: $SystemPluginName"
    }
}
$PreviouslyManagedSystemPluginNames = @()
if (Test-Path -LiteralPath $ManagedSystemPluginIndexPath -PathType Leaf) {
    $PreviouslyManagedSystemPluginNames = @(
        Get-Content -LiteralPath $ManagedSystemPluginIndexPath |
            ForEach-Object { $_.Trim() } |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    )
}
foreach ($SystemPluginName in $PreviouslyManagedSystemPluginNames) {
    if ($SystemPluginName -notmatch '^[a-z][a-z0-9_-]*$') {
        throw "Invalid managed System Plugin index entry: $SystemPluginName"
    }
    if ($SourceSystemPluginNames -notcontains $SystemPluginName) {
        $DeployedPluginDir = Join-Path $OutputSystemPluginsDir $SystemPluginName
        $ResolvedDeployedPluginDir = [System.IO.Path]::GetFullPath($DeployedPluginDir)
        if (-not $ResolvedDeployedPluginDir.StartsWith($ResolvedOutputSystemPluginsDir + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to clean System Plugin path outside runtime root: $ResolvedDeployedPluginDir"
        }
        if (Test-Path -LiteralPath $ResolvedDeployedPluginDir) {
            $null = Remove-ItemAllowLocked -LiteralPath $ResolvedDeployedPluginDir -Recurse
        }
    }
}
foreach ($SystemPluginName in $SourceSystemPluginNames) {
    # DestinationSystemPluginDir receives an incremental overlay so debug builds preserve existing plugin files.
    # DestinationSystemPluginDir 接收增量覆盖，确保调试构建保留已有插件文件。
    $DestinationSystemPluginDir = Join-Path $OutputSystemPluginsDir $SystemPluginName
    # ResolvedDestinationSystemPluginDir proves the cleanup target remains below the runtime plugin root.
    # ResolvedDestinationSystemPluginDir 用于证明清理目标仍位于运行时插件根目录下。
    $ResolvedDestinationSystemPluginDir = [System.IO.Path]::GetFullPath($DestinationSystemPluginDir)
    if (-not $ResolvedDestinationSystemPluginDir.StartsWith($ResolvedOutputSystemPluginsDir + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to deploy System Plugin outside runtime root: $ResolvedDestinationSystemPluginDir"
    }
    Copy-DirectoryContents `
        -SourceDirectory (Join-Path $SourceSystemPluginsDir $SystemPluginName) `
        -DestinationDirectory $DestinationSystemPluginDir
}
# ASCII keeps the cross-shell ownership index free of a Windows PowerShell UTF-8 BOM.
# ASCII 确保跨 Shell 使用的所有权索引不会带有 Windows PowerShell UTF-8 BOM。
Set-Content -LiteralPath $ManagedSystemPluginIndexPath -Value ($SourceSystemPluginNames | Sort-Object) -Encoding ascii

Write-Host "==> Syncing fixed LuaSkills plugin bridge package..."
Copy-DirectoryContents -SourceDirectory $SourcePluginBridgeDir -DestinationDirectory $OutputPluginBridgeDir

# LegacyOutputRtkToolDir is removed because RTK is now distributed only inside its plugin package.
# LegacyOutputRtkToolDir 会被移除，因为 RTK 现在只通过其插件包分发。
if (Test-Path -LiteralPath $LegacyOutputRtkToolDir) {
    $null = Remove-ItemAllowLocked -LiteralPath $LegacyOutputRtkToolDir -Recurse
}
Get-ChildItem -LiteralPath $OutputToolsDir -File -Force -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -ieq "rtk" -or $_.Name -ieq "rtk.exe" } |
    ForEach-Object { $null = Remove-ItemAllowLocked -LiteralPath $_.FullName }
if ($IsWindowsPlatform) {
    # OutputRtkPluginBinary is the exact current-target artifact already copied with the plugin package.
    # OutputRtkPluginBinary 是随插件包一并复制的当前目标精确制品。
    $OutputRtkPluginBinary = Join-Path $OutputSystemPluginsDir "vulcan-rtk\bin\x86_64-pc-windows-msvc\rtk.exe"
    if (-not (Test-Path -LiteralPath $OutputRtkPluginBinary -PathType Leaf)) {
        throw "RTK plugin binary is missing; run make deps-rtk before packaging."
    }
}

# Copy the executable only after runtime payload synchronization so a locked debug binary cannot suppress plugin updates.
# 仅在运行时载荷同步完成后复制可执行文件，避免被占用的调试二进制阻断插件更新。
New-Item -ItemType Directory -Path $OutputBinDir -Force | Out-Null
# BinaryCopied records whether the running desktop process allowed the executable to be refreshed.
# BinaryCopied 记录运行中的桌面进程是否允许刷新可执行文件。
$BinaryCopied = Copy-FileAllowLockedDestination -SourceFile $BuiltBinaryPath -DestinationFile $OutputBinaryPath
if ($IsWindowsPlatform) {
    # HelperCopied records whether the optional Windows console helper was refreshed.
    # HelperCopied 记录可选 Windows 控制台辅助程序是否已刷新。
    $HelperCopied = Copy-FileAllowLockedDestination -SourceFile $BuiltHelperPath -DestinationFile $OutputHelperPath
}

if ($BinaryCopied) {
    Write-Host "==> Done. Binary: $OutputBinaryPath"
}
else {
    Write-Host "==> Done. Binary remained unchanged because it is in use: $OutputBinaryPath"
}
if ($IsWindowsPlatform) {
    if ($HelperCopied) {
        Write-Host "==> Done. Windows console helper: $OutputHelperPath"
    }
    else {
        Write-Host "==> Done. Windows console helper remained unchanged because it is in use: $OutputHelperPath"
    }
}
