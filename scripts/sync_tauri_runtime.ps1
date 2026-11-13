# sync_tauri_runtime.ps1 synchronizes runtime resources into the stable output tree.
# sync_tauri_runtime.ps1 用于把运行时资源同步到稳定的 output 目录树。
# Usage:
# 用法：
#   .\sync_tauri_runtime.ps1           # synchronize the debug distribution (output)
#   .\sync_tauri_runtime.ps1 -Release  # synchronize the release distribution (output-release)

param(
    # Release selects the release distribution instead of the debug one.
    # Release 用于选择正式版发行目录而不是调试版发行目录。
    [switch]$Release
)

$ErrorActionPreference = "Stop"

# Relaunch under PowerShell 7 when available so path and UTF-8 behavior matches the build entrypoint.
# 若检测到 PowerShell 7，则自动在其下重启，使路径与 UTF-8 行为和构建入口保持一致。
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
        # ForwardedArguments preserves only the explicitly requested distribution profile.
        # ForwardedArguments 仅保留显式请求的发行模式。
        $ForwardedArguments = @()
        if ($Release) { $ForwardedArguments += "-Release" }
        & $PowerShellCore.Source -NoProfile -ExecutionPolicy Bypass -File $CurrentScriptPath @ForwardedArguments
        exit $LASTEXITCODE
    }
}

# ProjectDir stores the repository root for stable source and output paths.
# ProjectDir 保存仓库根目录，确保源目录与输出目录路径稳定。
$ProjectDir = Split-Path $PSScriptRoot -Parent

# IsWindowsPlatform records whether synchronization runs on a Windows host.
# IsWindowsPlatform 记录同步是否运行在 Windows 宿主上。
$IsWindowsPlatform = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)

# OutputPathComparison selects platform-correct path boundary comparison semantics.
# OutputPathComparison 选择符合当前平台语义的路径边界比较方式。
$OutputPathComparison = if ($IsWindowsPlatform) { [System.StringComparison]::OrdinalIgnoreCase } else { [System.StringComparison]::Ordinal }

# SourceConfigDir points at repository-owned packaged configuration templates.
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

# BundledToolsArchitecture stores the normalized current-host architecture.
# BundledToolsArchitecture 保存规范化后的当前宿主架构。
$BundledToolsArchitecture = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString().ToLowerInvariant()

# BundledToolsArchitectureKey maps the runtime architecture to upstream target spelling.
# BundledToolsArchitectureKey 把运行时架构映射为上游目标拼写。
$BundledToolsArchitectureKey = switch ($BundledToolsArchitecture) {
    "x64" { "x86_64" }
    "arm64" { "aarch64" }
    default { throw "Unsupported bundled-tools synchronization architecture: $BundledToolsArchitecture" }
}

# BundledToolsTarget selects the exact dependency payload prepared by make deps tools.
# BundledToolsTarget 选择由 make deps tools 准备的精确依赖载荷。
$BundledToolsTarget = if ($IsWindowsPlatform) {
    "$BundledToolsArchitectureKey-pc-windows-msvc"
}
elseif ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)) {
    "$BundledToolsArchitectureKey-apple-darwin"
}
elseif ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)) {
    "$BundledToolsArchitectureKey-unknown-linux-gnu"
}
else {
    throw "Unsupported bundled-tools synchronization operating system."
}

# SourceBundledToolsDir points at the verified current-target sg and rg payload.
# SourceBundledToolsDir 指向已校验的当前目标 sg 与 rg 载荷。
$SourceBundledToolsDir = Join-Path $ProjectDir "third_party\tools\$BundledToolsTarget"

# SourceVulcanPluginsDir contains every manifest-driven plugin bundled with Vulcan Code.
# SourceVulcanPluginsDir 包含随 Vulcan Code 打包的所有清单驱动插件。
$SourceVulcanPluginsDir = Join-Path $ProjectDir "plugins"

# SourcePluginBridgeDir contains the only LuaSkills System package used by Vulcan plugins.
# SourcePluginBridgeDir 包含 Vulcan 插件使用的唯一 LuaSkills System package。
$SourcePluginBridgeDir = Join-Path $ProjectDir "plugin-bridge\vulcan-plugin-bridge"

# SyncProfileName selects the distribution this synchronization owns; the other profile's tree is never touched.
# SyncProfileName 选择本次同步拥有的发行目录；绝不触碰另一个模式的目录树。
$SyncProfileName = if ($Release) { "release" } else { "debug" }

# OutputRootDir stores the only directory tree that synchronization may clean.
# OutputRootDir 保存同步流程允许执行清理的唯一目录树。
# The directory name and the isolated-root validation have one owner, desktop-runtime.mjs, shared with build and run.
# 目录名与独立根校验只有 desktop-runtime.mjs 一个归属，与构建和运行共用。
$OutputRootDir = (& node (Join-Path $PSScriptRoot "desktop-runtime.mjs") root $SyncProfileName | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or -not [System.IO.Path]::IsPathFullyQualified($OutputRootDir)) {
    throw "Could not resolve the $SyncProfileName distribution root."
}

# OutputConfigDir stores packaged configuration read by distribution runtime launches.
# OutputConfigDir 保存发行运行时启动时读取的打包配置目录。
$OutputConfigDir = Join-Path $OutputRootDir "config"

# ResolvedOutputRootDir stores the normalized cleanup boundary used by deletion guards.
# ResolvedOutputRootDir 保存删除保护使用的规范化清理边界。
$ResolvedOutputRootDir = [System.IO.Path]::GetFullPath($OutputRootDir).TrimEnd([System.IO.Path]::DirectorySeparatorChar)

# OutputResourcesDir stores the packaged resource catalog read by output/bin runtime launches.
# OutputResourcesDir 保存 output/bin 运行时启动读取的打包资源目录。
$OutputResourcesDir = Join-Path $OutputRootDir "resources"

# OutputSystemPromptSkillsDir stores packaged system Agent Skills read directly by the runtime.
# OutputSystemPromptSkillsDir 保存运行时直接读取的打包系统 Agent Skills。
$OutputSystemPromptSkillsDir = Join-Path $OutputResourcesDir "prompt-skills\system"

# OutputLuaRuntimeDir stores the application-owned LuaSkills runtime root.
# OutputLuaRuntimeDir 保存应用持有的 LuaSkills 运行时根目录。
$OutputLuaRuntimeDir = Join-Path $OutputRootDir "lua_runtime"

# OutputBundledToolsDir is the runtime-owned sg, rg, and rtk distribution root.
# OutputBundledToolsDir 是运行时持有的 sg、rg 与 rtk 发行根目录。
$OutputBundledToolsDir = Join-Path $OutputRootDir "tools"

# LegacyManagedRuntimeScriptsDir stores obsolete downloader scripts removed by native Rust bootstrap.
# LegacyManagedRuntimeScriptsDir 保存由 Rust 原生初始化取代的废弃下载脚本。
$LegacyManagedRuntimeScriptsDir = Join-Path $OutputLuaRuntimeDir "scripts\deps"

# LegacyManagedRuntimeDistributionDir is the obsolete LuaSkills-private Python/Node location.
# LegacyManagedRuntimeDistributionDir 是已经废弃的 LuaSkills 私有 Python/Node 位置。
$LegacyManagedRuntimeDistributionDir = Join-Path $OutputLuaRuntimeDir "dependencies\runtimes"

# OutputVulcanPluginsDir is the Vulcan-owned runtime directory scanned by the plugin host.
# OutputVulcanPluginsDir 是插件宿主扫描的 Vulcan 自有运行时目录。
$OutputVulcanPluginsDir = Join-Path $OutputRootDir "plugins"

# OutputPluginBridgeDir is the only plugin-related package deployed below system_lua_lib.
# OutputPluginBridgeDir 是部署到 system_lua_lib 下的唯一插件相关 package。
$OutputPluginBridgeDir = Join-Path $OutputLuaRuntimeDir "system_lua_lib\vulcan-plugin-bridge"

# ManagedVulcanPluginIndexPath records only packages owned by this sync script for safe stale cleanup.
# ManagedVulcanPluginIndexPath 仅记录当前同步脚本拥有的插件包，用于安全清理陈旧项。
$ManagedVulcanPluginIndexPath = Join-Path $OutputVulcanPluginsDir ".vulcan-code-managed-plugins"

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
            Write-Warning "Skipped locked output file; restart the running application before using the new resource: $DestinationFile"
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

    # ResolvedLiteralPath proves the cleanup target remains below the repository output root.
    # ResolvedLiteralPath 用于证明清理目标仍位于仓库 output 根目录下。
    $ResolvedLiteralPath = [System.IO.Path]::GetFullPath($LiteralPath)
    if (-not $ResolvedLiteralPath.StartsWith($ResolvedOutputRootDir + [System.IO.Path]::DirectorySeparatorChar, $OutputPathComparison)) {
        throw "Refusing to clean path outside output: $ResolvedLiteralPath"
    }

    try {
        Remove-Item -LiteralPath $ResolvedLiteralPath -Recurse:$Recurse -Force -ErrorAction Stop
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

# Prepare the bundled browser before copying plugins; all downloads belong to this build step.
# 复制插件前准备内置浏览器；全部下载仅属于此构建步骤。
& node (Join-Path $ProjectDir "scripts/browser-runtime.mjs") prepare $OutputRootDir
if ($LASTEXITCODE -ne 0) { throw "Built-in browser preparation failed." }

Write-Host "==> Syncing packaged config..."
Copy-DirectoryContents -SourceDirectory $SourceConfigDir -DestinationDirectory $OutputConfigDir

Write-Host "==> Syncing packaged resources..."
Copy-DirectoryContents -SourceDirectory $SourceResourcesDir -DestinationDirectory $OutputResourcesDir

Write-Host "==> Syncing packaged system Agent Skills..."
Copy-DirectoryContents -SourceDirectory $SourceSystemPromptSkillsDir -DestinationDirectory $OutputSystemPromptSkillsDir -Clean

Write-Host "==> Syncing LuaSkills runtime payload..."
if ($BundledToolsTarget.EndsWith('-apple-darwin')) {
    . (Join-Path $PSScriptRoot 'macos-lua-runtime.ps1')
    Confirm-MacLuaRuntime -Root $SourceLuaRuntimeDir
    # Replace only native directories so an older sync cannot leave broken modules or libraries behind.
    # 仅替换原生目录，防止旧同步留下失效的模块或共享库。
    foreach ($NativeDirectory in @('lua_packages/lib/lua', 'libs')) {
        # NativeOutput is an exact descendant of the validated distribution root.
        # NativeOutput 是经过校验的发行根目录下的精确路径。
        $NativeOutput = Join-Path $OutputLuaRuntimeDir $NativeDirectory
        if ((Test-Path -LiteralPath $NativeOutput) -and -not (Remove-ItemAllowLocked -LiteralPath $NativeOutput -Recurse)) {
            throw "Could not replace macOS Lua native directory: $NativeOutput"
        }
    }
}
Copy-DirectoryContents -SourceDirectory $SourceLuaRuntimeDir -DestinationDirectory $OutputLuaRuntimeDir
if ($BundledToolsTarget.EndsWith('-apple-darwin')) {
    Confirm-MacLuaRuntime -Root $OutputLuaRuntimeDir
    Test-MacLuaNativeLoading -Root $OutputLuaRuntimeDir
}

# BundledToolBinarySuffix selects the executable suffix required by the current host.
# BundledToolBinarySuffix 选择当前宿主所需的可执行文件后缀。
$BundledToolBinarySuffix = if ($IsWindowsPlatform) { ".exe" } else { "" }
# BundledToolNames are the tools the build-input lock declares for this host.
# BundledToolNames 是构建输入锁文件为当前宿主声明的工具。
$BundledToolNames = ((& node (Join-Path $ProjectDir "scripts/install-locked-tools.mjs") bundled-tool-ids | Out-String).Trim()) -split ' '
if ($LASTEXITCODE -ne 0) { throw "Could not resolve the bundled tools for this host." }
# BundledToolName iterates the exact commands required by the runtime PATH contract.
# BundledToolName 遍历运行时 PATH 契约要求的精确命令。
foreach ($BundledToolName in $BundledToolNames) {
    # SourceBundledToolBinary is the exact verified dependency required before synchronization.
    # SourceBundledToolBinary 是同步前必需的精确已校验依赖。
    $SourceBundledToolBinary = Join-Path $SourceBundledToolsDir "bin\$BundledToolName$BundledToolBinarySuffix"
    if (-not (Test-Path -LiteralPath $SourceBundledToolBinary -PathType Leaf)) {
        throw "Bundled tool '$BundledToolName' is missing; run make deps tools before synchronization."
    }
}
Write-Host "==> Syncing bundled tools ($($BundledToolNames -join ', '))..."
Copy-DirectoryContents -SourceDirectory $SourceBundledToolsDir -DestinationDirectory $OutputBundledToolsDir -Clean

# SourceJudgeRuntimeDir is the lock-verified judge runtime of this host, or empty when the lock declares none.
# SourceJudgeRuntimeDir 是按锁文件校验过的本宿主判定执行库；锁文件未声明时为空。
$SourceJudgeRuntimeDir = (& node (Join-Path $ProjectDir "scripts/install-locked-tools.mjs") verify-judge-runtime | Out-String).Trim()
if ($LASTEXITCODE -ne 0) { throw "Judge runtime verification failed." }
# OutputJudgeRuntimeDir is the runtime-owned ONNX Runtime and WebGPU plugin root.
# OutputJudgeRuntimeDir 是运行时持有的 ONNX Runtime 与 WebGPU 插件根目录。
$OutputJudgeRuntimeDir = Join-Path $OutputRootDir "judge-runtime"
if ($SourceJudgeRuntimeDir) {
    Write-Host "==> Syncing bundled judge runtime..."
    Copy-DirectoryContents -SourceDirectory $SourceJudgeRuntimeDir -DestinationDirectory $OutputJudgeRuntimeDir -Clean
}
elseif (Test-Path -LiteralPath $OutputJudgeRuntimeDir) {
    $null = Remove-ItemAllowLocked -LiteralPath $OutputJudgeRuntimeDir -Recurse
}

if (Test-Path -LiteralPath $LegacyManagedRuntimeDistributionDir) {
    Write-Host "==> Removing obsolete LuaSkills-private managed runtimes..."
    $null = Remove-ItemAllowLocked -LiteralPath $LegacyManagedRuntimeDistributionDir -Recurse
}

if (Test-Path -LiteralPath $LegacyManagedRuntimeScriptsDir) {
    Write-Host "==> Removing obsolete managed runtime downloader scripts..."
    $null = Remove-ItemAllowLocked -LiteralPath $LegacyManagedRuntimeScriptsDir -Recurse
}

Write-Host "==> Syncing host Vulcan Plugin packages..."
New-Item -ItemType Directory -Path $OutputVulcanPluginsDir -Force | Out-Null
# ResolvedOutputVulcanPluginsDir stores the normalized plugin output root used by cleanup boundary checks.
# ResolvedOutputVulcanPluginsDir 保存清理边界检查使用的规范化插件输出根目录。
$ResolvedOutputVulcanPluginsDir = [System.IO.Path]::GetFullPath($OutputVulcanPluginsDir).TrimEnd([System.IO.Path]::DirectorySeparatorChar)
# SourceVulcanPluginNames stores the validated set of manifest-driven plugin package names.
# SourceVulcanPluginNames 保存经过验证的清单驱动插件包名称集合。
$SourceVulcanPluginNames = @(
    Get-ChildItem -LiteralPath $SourceVulcanPluginsDir -Directory -Force |
        Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName "plugin.yaml") -PathType Leaf } |
        Select-Object -ExpandProperty Name
)
if ($SourceVulcanPluginNames.Count -eq 0) {
    throw "No manifest-driven Vulcan Plugin packages were found in: $SourceVulcanPluginsDir"
}
# VulcanPluginName iterates each manifest-owned plugin name during validation and deployment.
# VulcanPluginName 在验证与部署期间遍历每个清单持有的插件名称。
foreach ($VulcanPluginName in $SourceVulcanPluginNames) {
    if ($VulcanPluginName -notmatch '^[a-z][a-z0-9_-]*$') {
        throw "Invalid Vulcan Plugin source directory name: $VulcanPluginName"
    }
}
# PreviouslyManagedVulcanPluginNames stores packages previously owned by this synchronization workflow.
# PreviouslyManagedVulcanPluginNames 保存此前由此同步流程管理的插件包。
$PreviouslyManagedVulcanPluginNames = @()
if (Test-Path -LiteralPath $ManagedVulcanPluginIndexPath -PathType Leaf) {
    $PreviouslyManagedVulcanPluginNames = @(
        Get-Content -LiteralPath $ManagedVulcanPluginIndexPath |
            ForEach-Object { $_.Trim() } |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    )
}
foreach ($VulcanPluginName in $PreviouslyManagedVulcanPluginNames) {
    if ($VulcanPluginName -notmatch '^[a-z][a-z0-9_-]*$') {
        throw "Invalid managed Vulcan Plugin index entry: $VulcanPluginName"
    }
    if ($SourceVulcanPluginNames -notcontains $VulcanPluginName) {
        # DeployedPluginDir is the exact stale plugin directory considered for removal.
        # DeployedPluginDir 是准备移除的精确陈旧插件目录。
        $DeployedPluginDir = Join-Path $OutputVulcanPluginsDir $VulcanPluginName
        # ResolvedDeployedPluginDir proves the stale package remains below the plugin output root.
        # ResolvedDeployedPluginDir 用于证明陈旧插件包仍位于插件输出根目录下。
        $ResolvedDeployedPluginDir = [System.IO.Path]::GetFullPath($DeployedPluginDir)
        if (-not $ResolvedDeployedPluginDir.StartsWith($ResolvedOutputVulcanPluginsDir + [System.IO.Path]::DirectorySeparatorChar, $OutputPathComparison)) {
            throw "Refusing to clean Vulcan Plugin path outside runtime root: $ResolvedDeployedPluginDir"
        }
        if (Test-Path -LiteralPath $ResolvedDeployedPluginDir) {
            $null = Remove-ItemAllowLocked -LiteralPath $ResolvedDeployedPluginDir -Recurse
        }
    }
}
foreach ($VulcanPluginName in $SourceVulcanPluginNames) {
    # DestinationVulcanPluginDir receives an incremental overlay so synchronization preserves unmanaged files.
    # DestinationVulcanPluginDir 接收增量覆盖，确保同步过程保留未管理文件。
    $DestinationVulcanPluginDir = Join-Path $OutputVulcanPluginsDir $VulcanPluginName
    # ResolvedDestinationVulcanPluginDir proves the deployment target remains below the runtime plugin root.
    # ResolvedDestinationVulcanPluginDir 用于证明部署目标仍位于运行时插件根目录下。
    $ResolvedDestinationVulcanPluginDir = [System.IO.Path]::GetFullPath($DestinationVulcanPluginDir)
    if (-not $ResolvedDestinationVulcanPluginDir.StartsWith($ResolvedOutputVulcanPluginsDir + [System.IO.Path]::DirectorySeparatorChar, $OutputPathComparison)) {
        throw "Refusing to deploy Vulcan Plugin outside runtime root: $ResolvedDestinationVulcanPluginDir"
    }
    # The built-in browser owns its complete npm tree; old versions must not survive an overlay.
    # 内置浏览器拥有完整 npm 目录；旧版本文件不能在覆盖同步后残留。
    if ($VulcanPluginName -eq "vulcan-browser") {
        $BrowserModules = Join-Path $ResolvedDestinationVulcanPluginDir "node/node_modules"
        if (Test-Path -LiteralPath $BrowserModules) {
            if (-not (Remove-ItemAllowLocked -LiteralPath $BrowserModules -Recurse)) {
                throw "Cannot replace the built-in browser dependency tree. Close the application and retry synchronization."
            }
        }
    }
    Copy-DirectoryContents `
        -SourceDirectory (Join-Path $SourceVulcanPluginsDir $VulcanPluginName) `
        -DestinationDirectory $DestinationVulcanPluginDir
}
# ASCII keeps the cross-shell ownership index free of a Windows PowerShell UTF-8 BOM.
# ASCII 确保跨 Shell 使用的所有权索引不会带有 Windows PowerShell UTF-8 BOM。
Set-Content -LiteralPath $ManagedVulcanPluginIndexPath -Value ($SourceVulcanPluginNames | Sort-Object) -Encoding ascii

Write-Host "==> Syncing fixed LuaSkills plugin bridge package..."
Copy-DirectoryContents -SourceDirectory $SourcePluginBridgeDir -DestinationDirectory $OutputPluginBridgeDir

Write-Host "==> Done. Runtime resources synchronized into: $OutputRootDir"
