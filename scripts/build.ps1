# build.ps1 packages vulcan-code-gui into one runnable output directory.
# build.ps1 用于把 vulcan-code-gui 打包到可运行的 output 目录。
# Usage:
# 用法：
#   .\build.ps1           # debug build
#   .\build.ps1 release   # release build
#   .\build.ps1 -Release  # release build

param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"

# Relaunch under PowerShell 7 when available so UTF-8 comments and modern syntax stay stable.
# 若检测到 PowerShell 7，则自动在其下重启，确保 UTF-8 注释与现代语法保持稳定。
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

# BaseOutDir stores the absolute output root so filesystem operations never depend on the caller's current directory.
# BaseOutDir 用于保存绝对 output 根目录，确保文件系统操作永远不依赖调用方当前目录。
$BaseOutDir = Join-Path $ProjectDir "output"

# LuaRuntimeOut stores the independent LuaSkills runtime root required by LuaSkills 0.4.6.
# LuaRuntimeOut 保存 LuaSkills 0.4.6 要求的独立 LuaSkills 运行时根目录。
$LuaRuntimeOut = Join-Path $BaseOutDir "lua_runtime"

$BinName = "vulcan-code-gui"
$BinaryFileName = $BinName
if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
    $BinaryFileName = "$BinName.exe"
}
$AppResourcesRoot = Join-Path $ProjectDir "apps\vulcan-code-gui\resources"
$AppLicenseResourcesRoot = Join-Path $AppResourcesRoot "licenses"
$RuntimeAssetsResourcesRoot = Join-Path $ProjectDir "crates\vulcan-runtime-assets\resources"
$PromptSystemSkillsRoot = Join-Path $ProjectDir "crates\vulcan-prompt-skills\src\assets\system"

if ($Release -or ($args.Count -gt 0 -and $args[0] -eq "release")) {
    $OutDir = Join-Path $BaseOutDir "bin"
    $CargoProfile = "release"
    $CargoArgs = @("--release")
    Write-Host "==> Building release..."
} else {
    $OutDir = Join-Path $BaseOutDir "debug"
    $CargoProfile = "debug"
    $CargoArgs = @()
    Write-Host "==> Building debug..."
}

# Build
& cargo build @CargoArgs

# Determine binary path
$BinExe = "target\$CargoProfile\$BinName.exe"
if (-not (Test-Path $BinExe)) {
    $BinExe = "target\$CargoProfile\$BinName"
}
if (-not (Test-Path $BinExe)) {
    Write-Error "Build failed: binary not found at $BinExe"
    exit 1
}

# Output directories
$RuntimeBinOut = Join-Path $LuaRuntimeOut "bin"
$LibsOut = Join-Path $LuaRuntimeOut "libs"
$SkillsOut = Join-Path $LuaRuntimeOut "skills"
$PkgOut = Join-Path $LuaRuntimeOut "lua_packages"
$ConfigOut = Join-Path $BaseOutDir "config"
$ResourcesOut = Join-Path $BaseOutDir "resources"
$LuaConfigOut = Join-Path $LuaRuntimeOut "config"
$LicensesOut = Join-Path $LuaRuntimeOut "licenses"
$DependenciesOut = Join-Path $LuaRuntimeOut "dependencies"
$DatabasesOut = Join-Path $LuaRuntimeOut "databases"
$StateOut = Join-Path $LuaRuntimeOut "state"
$TempOut = Join-Path $LuaRuntimeOut "temp"
$LogsOut = Join-Path $BaseOutDir "logs"
$LuaSkillsRuntimeRoot = Join-Path $ProjectDir "third_party\luaskills_runtime"

# LuaRuntimeRequiredDirectories lists the fixed LuaSkills 0.4.6 package directories that must exist after a full package sync.
# LuaRuntimeRequiredDirectories 列出完整包同步后仍必须存在的 LuaSkills 0.4.6 固定包目录。
$LuaRuntimeRequiredDirectories = @(
    $RuntimeBinOut,
    $LibsOut,
    $SkillsOut,
    $PkgOut,
    $LuaConfigOut,
    (Join-Path $LuaRuntimeOut "resources"),
    $LicensesOut,
    $DependenciesOut,
    $DatabasesOut,
    $StateOut,
    $TempOut,
    (Join-Path $TempOut "downloads"),
    (Join-Path $LuaRuntimeOut "system_lua_lib")
)

# LuaRuntimeCompatibilityDirectories lists host-created compatibility leaves under the independent Lua runtime package.
# LuaRuntimeCompatibilityDirectories 列出独立 Lua 运行时包下由宿主补齐的兼容叶子目录。
$LuaRuntimeCompatibilityDirectories = @(
    "$DependenciesOut\\shared\\tools",
    "$DependenciesOut\\shared\\lua",
    "$DependenciesOut\\shared\\ffi",
    "$DependenciesOut\\skill",
    "$DatabasesOut\\sqlite",
    "$DatabasesOut\\lancedb",
    "$StateOut\\skills"
)

# RustThirdPartyGeneratedLicenseFileNames stores the generated license-sidecar file names that should survive static resource syncs.
# RustThirdPartyGeneratedLicenseFileNames 用于保存需要跨静态资源同步保留下来的生成许可证文件名集合。
$RustThirdPartyGeneratedLicenseFileNames = @(
    "THIRD_PARTY_NOTICES.md",
    "THIRD_PARTY_NOTICES_INDEX.json"
)

# OutputLicenseResourcesDirectory points at the packaged output license directory that hosts both static and generated notices.
# OutputLicenseResourcesDirectory 指向同时承载静态与生成许可证文件的打包 output 许可证目录。
$OutputLicenseResourcesDirectory = Join-Path $ResourcesOut "licenses"

function Reset-DirectoryContents {
    <#
    .SYNOPSIS
    Ensure one directory exists and remove stale contents before a structured sync.
    确保目录存在，并在结构化同步前清理旧内容。

    .PARAMETER Path
    Directory path to reset.
    需要重置的目录路径。
    #>
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -ItemType Directory -Path $Path -Force | Out-Null
        return
    }

    Get-ChildItem -Force -LiteralPath $Path -ErrorAction SilentlyContinue | ForEach-Object {
        Remove-Item -LiteralPath $_.FullName -Recurse -Force
    }
}

function Copy-DirectoryContents {
    <#
    .SYNOPSIS
    Copy direct directory contents into a destination while preserving package layout.
    将目录直属内容复制到目标目录，并保持包布局。

    .PARAMETER Source
    Source directory.
    来源目录。

    .PARAMETER Destination
    Destination directory.
    目标目录。
    #>
    param(
        [string]$Source,
        [string]$Destination
    )

    if (-not (Test-Path -LiteralPath $Source)) {
        return $false
    }

    Reset-DirectoryContents -Path $Destination
    Get-ChildItem -Force -LiteralPath $Source | ForEach-Object {
        Copy-Item -Recurse -Force -LiteralPath $_.FullName -Destination $Destination
    }
    return $true
}

function Merge-DirectoryContents {
    <#
    .SYNOPSIS
    Copy direct directory contents into a destination without clearing previously merged assets.
    将目录直属内容复制到目标目录，并保留此前已合并的资产。

    .PARAMETER Source
    Source directory.
    来源目录。

    .PARAMETER Destination
    Destination directory.
    目标目录。
    #>
    param(
        [string]$Source,
        [string]$Destination
    )

    if (-not (Test-Path -LiteralPath $Source)) {
        return $false
    }

    if (-not (Test-Path -LiteralPath $Destination)) {
        New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    }

    Get-ChildItem -Force -LiteralPath $Source | ForEach-Object {
        Copy-Item -Recurse -Force -LiteralPath $_.FullName -Destination $Destination
    }
    return $true
}

function Merge-DirectoryContentsSkippingInstallMarkers {
    <#
    .SYNOPSIS
    Copy direct directory contents into a destination without clearing runtime-managed state or install markers.
    将目录直属内容复制到目标目录，且不清空运行期状态或复制安装标记。

    .PARAMETER Source
    Source directory.
    来源目录。

    .PARAMETER Destination
    Destination directory.
    目标目录。
    #>
    param(
        [string]$Source,
        [string]$Destination
    )

    if (-not (Test-Path -LiteralPath $Source)) {
        return $false
    }

    if (-not (Test-Path -LiteralPath $Destination)) {
        New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    }

    Get-ChildItem -Force -LiteralPath $Source | ForEach-Object {
        if (-not $_.PSIsContainer -and $_.Name -like ".installed-*") {
            return
        }
        Copy-Item -Recurse -Force -LiteralPath $_.FullName -Destination $Destination
    }
    return $true
}

# Sync-ApplicationLicenseResources refreshes static app license files while preserving dynamically generated Rust notices in output.
# Sync-ApplicationLicenseResources 用于刷新应用静态许可证文件，同时保留 output 中动态生成的 Rust 声明文件。
# Source points at the source-tree static license directory.
# Source 指向源码树中的静态许可证目录。
# Destination points at the output runtime license directory.
# Destination 指向 output 运行时许可证目录。
# PreservedGeneratedFileNames name the dynamically generated notice files that must survive the sync.
# PreservedGeneratedFileNames 表示必须在同步过程中保留下来的动态生成声明文件名集合。
function Sync-ApplicationLicenseResources {
    param(
        [string]$Source,
        [string]$Destination,
        [string[]]$PreservedGeneratedFileNames
    )

    if (-not (Test-Path -LiteralPath $Source)) {
        return $false
    }

    $PreservedGeneratedNoticeBytesByName = @{}
    foreach ($GeneratedFileName in $PreservedGeneratedFileNames) {
        $PreservedGeneratedNoticePath = Join-Path $Destination $GeneratedFileName
        if (Test-Path -LiteralPath $PreservedGeneratedNoticePath) {
            $PreservedGeneratedNoticeBytesByName[$GeneratedFileName] =
                [System.IO.File]::ReadAllBytes($PreservedGeneratedNoticePath)
        }
    }

    Copy-DirectoryContents -Source $Source -Destination $Destination | Out-Null

    foreach ($GeneratedFileName in $PreservedGeneratedNoticeBytesByName.Keys) {
        $PreservedGeneratedNoticePath = Join-Path $Destination $GeneratedFileName
        [System.IO.File]::WriteAllBytes(
            $PreservedGeneratedNoticePath,
            $PreservedGeneratedNoticeBytesByName[$GeneratedFileName]
        )
    }

    return $true
}

# Restore-PreservedGeneratedLicenseFiles rewrites any previously captured generated license files after broader resource syncs cleared output/resources.
# Restore-PreservedGeneratedLicenseFiles 用于在更大范围的资源同步清空 output/resources 后，重新写回此前捕获的生成许可证文件。
# DestinationDirectory points at output/resources/licenses where generated sidecars should be restored.
# DestinationDirectory 指向应恢复生成旁路文件的 output/resources/licenses 目录。
# FilesByName carries the previously captured file bytes keyed by their file names.
# FilesByName 承接按文件名索引的此前捕获字节内容。
function Restore-PreservedGeneratedLicenseFiles {
    param(
        [string]$DestinationDirectory,
        [hashtable]$FilesByName
    )

    if ($null -eq $FilesByName -or $FilesByName.Count -eq 0) {
        return
    }

    if (-not (Test-Path -LiteralPath $DestinationDirectory)) {
        New-Item -ItemType Directory -Path $DestinationDirectory -Force | Out-Null
    }

    foreach ($GeneratedFileName in $FilesByName.Keys) {
        [System.IO.File]::WriteAllBytes(
            (Join-Path $DestinationDirectory $GeneratedFileName),
            $FilesByName[$GeneratedFileName]
        )
    }
}

function Enable-OutputModelConfigForLocalTesting {
    <#
    .SYNOPSIS
    Enable model capabilities only in the built output runtime config for local smoke testing.
    仅在构建后的输出运行配置中启用模型能力，方便本地冒烟测试。

    .PARAMETER ConfigDirectory
    Output config directory that may contain model_config.yaml.
    可能包含 model_config.yaml 的输出配置目录。
    #>
    param([string]$ConfigDirectory)

    $ModelConfigOut = Join-Path $ConfigDirectory "model_config.yaml"
    if (-not (Test-Path -LiteralPath $ModelConfigOut)) {
        return
    }

    $Content = Get-Content -LiteralPath $ModelConfigOut -Raw
    $Content = $Content -replace '(?m)^  enabled:\s*false\s*$', '  enabled: true'
    $Content = $Content -replace '(?m)^    enabled:\s*false\s*$', '    enabled: true'
    Set-Content -LiteralPath $ModelConfigOut -Value $Content -Encoding UTF8
    Write-Host "==> Output model_config.yaml enabled for local model smoke tests"
}

# Ensure output directories exist
if (-not (Test-Path $BaseOutDir)) { New-Item -ItemType Directory -Path $BaseOutDir -Force | Out-Null }
if (-not (Test-Path $LuaRuntimeOut)) { New-Item -ItemType Directory -Path $LuaRuntimeOut -Force | Out-Null }
if (-not (Test-Path $OutDir)) { New-Item -ItemType Directory -Path $OutDir -Force | Out-Null }
foreach ($dir in ($LuaRuntimeRequiredDirectories + @($ConfigOut, $ResourcesOut, $LogsOut))) {
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
}
foreach ($dir in $LuaRuntimeCompatibilityDirectories) {
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
}

# PreservedGeneratedLicenseFiles captures previously generated output-side license files before any broader resource sync resets output/resources.
# PreservedGeneratedLicenseFiles 用于在任何更大范围的资源同步重置 output/resources 之前捕获已有的 output 侧生成许可证文件。
$PreservedGeneratedLicenseFiles = @{}
foreach ($GeneratedFileName in $RustThirdPartyGeneratedLicenseFileNames) {
    $GeneratedFilePath = Join-Path $OutputLicenseResourcesDirectory $GeneratedFileName
    if (Test-Path -LiteralPath $GeneratedFilePath) {
        $PreservedGeneratedLicenseFiles[$GeneratedFileName] =
            [System.IO.File]::ReadAllBytes($GeneratedFilePath)
    }
}

# Copy binary
Copy-Item -Force $BinExe (Join-Path $OutDir $BinaryFileName)
Write-Host "==> Binary copied to $OutDir\"

# Sync official LuaSkills runtime package root to output/lua_runtime/.
# 将官方 LuaSkills 运行时包根目录完整同步到 output/lua_runtime/。
# Build packaging treats third_party/luaskills_runtime as a caller-managed asset root and copies it as-is.
# 构建打包会把 third_party/luaskills_runtime 视为调用方自管的资产根目录，并按现状直接同步。
# Cross-platform validation is intentionally omitted because forks may replace lua_packages/runtime payloads with custom layouts.
# 这里有意不做跨平台校验，因为 fork 方可能会用自定义布局替换 lua_packages/runtime 载荷。
if (Test-Path -LiteralPath $LuaSkillsRuntimeRoot) {
    Merge-DirectoryContentsSkippingInstallMarkers `
        -Source $LuaSkillsRuntimeRoot `
        -Destination $LuaRuntimeOut | Out-Null
    Write-Host "==> LuaSkills runtime package synced to $LuaRuntimeOut\"
}
else {
    Write-Host "==> No third_party/luaskills_runtime found (run make deps first)"
}

# Ensure the full Lua runtime package keeps host-managed empty directories after the package root reset.
# 确保完整 Lua 运行时包在包根重置后仍保留宿主管理的空目录。
foreach ($dir in $LuaRuntimeRequiredDirectories) {
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
}
foreach ($dir in $LuaRuntimeCompatibilityDirectories) {
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
}

# Sync shared runtime resources that are consumed outside the GUI crate.
# 同步 GUI crate 之外也会消费的共享运行时资源。
if (Test-Path (Join-Path $RuntimeAssetsResourcesRoot "providers")) {
    Copy-DirectoryContents -Source (Join-Path $RuntimeAssetsResourcesRoot "providers") -Destination (Join-Path $ResourcesOut "providers") | Out-Null
    Write-Host "==> Runtime provider catalog synced to $(Join-Path $ResourcesOut 'providers')\"
}
else {
    Write-Host "==> No runtime provider catalog directory found"
}

if (Test-Path (Join-Path $RuntimeAssetsResourcesRoot "ai-capability-providers")) {
    Copy-DirectoryContents -Source (Join-Path $RuntimeAssetsResourcesRoot "ai-capability-providers") -Destination (Join-Path $ResourcesOut "ai-capability-providers") | Out-Null
    Write-Host "==> Runtime AI capability provider catalog synced to $(Join-Path $ResourcesOut 'ai-capability-providers')\"
}
else {
    Write-Host "==> No runtime AI capability provider catalog directory found"
}

if (Test-Path (Join-Path $RuntimeAssetsResourcesRoot "agent-prompts")) {
    Copy-DirectoryContents -Source (Join-Path $RuntimeAssetsResourcesRoot "agent-prompts") -Destination (Join-Path $ResourcesOut "agent-prompts") | Out-Null
    Write-Host "==> Runtime agent prompt catalog synced to $(Join-Path $ResourcesOut 'agent-prompts')\"
}
else {
    Write-Host "==> No runtime agent prompt catalog directory found"
}

if (Test-Path (Join-Path $RuntimeAssetsResourcesRoot "system-prompts")) {
    Copy-DirectoryContents -Source (Join-Path $RuntimeAssetsResourcesRoot "system-prompts") -Destination (Join-Path $ResourcesOut "system-prompts") | Out-Null
    Write-Host "==> Runtime system prompt catalog synced to $(Join-Path $ResourcesOut 'system-prompts')\"
}
else {
    Write-Host "==> No runtime system prompt catalog directory found"
}

if (Test-Path -LiteralPath $PromptSystemSkillsRoot) {
    $PromptSystemSkillsOut = Join-Path (Join-Path $ResourcesOut "prompt-skills") "system"
    Copy-DirectoryContents -Source $PromptSystemSkillsRoot -Destination $PromptSystemSkillsOut | Out-Null
    Write-Host "==> System prompt skills synced to $PromptSystemSkillsOut\"
}
else {
    Write-Host "==> No system prompt skills source directory found"
}

if (Test-Path (Join-Path $RuntimeAssetsResourcesRoot "tokenizers")) {
    Copy-DirectoryContents -Source (Join-Path $RuntimeAssetsResourcesRoot "tokenizers") -Destination (Join-Path $ResourcesOut "tokenizers") | Out-Null
    Write-Host "==> Runtime tokenizer resources synced to $(Join-Path $ResourcesOut 'tokenizers')\"
}
else {
    Write-Host "==> No runtime tokenizer resource directory found"
}

if (Test-Path -LiteralPath $AppLicenseResourcesRoot) {
    Sync-ApplicationLicenseResources `
        -Source $AppLicenseResourcesRoot `
        -Destination $OutputLicenseResourcesDirectory `
        -PreservedGeneratedFileNames $RustThirdPartyGeneratedLicenseFileNames | Out-Null
    Restore-PreservedGeneratedLicenseFiles `
        -DestinationDirectory $OutputLicenseResourcesDirectory `
        -FilesByName $PreservedGeneratedLicenseFiles
    Write-Host "==> App license resources synced to $(Join-Path $ResourcesOut 'licenses')\\"
}
else {
    Restore-PreservedGeneratedLicenseFiles `
        -DestinationDirectory $OutputLicenseResourcesDirectory `
        -FilesByName $PreservedGeneratedLicenseFiles
    Write-Host "==> No app license resource directory found"
}

if (Test-Path (Join-Path $AppResourcesRoot "vulcan-mark.svg")) {
    Copy-Item -Force (Join-Path $AppResourcesRoot "vulcan-mark.svg") (Join-Path $ResourcesOut "vulcan-mark.svg")
    Write-Host "==> App logo asset synced to $ResourcesOut\"
}

# Sync base config files to output/config/
# 同步基础配置文件到 output/config/
if (Test-Path "config") {
    if (-not (Test-Path $ConfigOut)) { New-Item -ItemType Directory -Path $ConfigOut -Force | Out-Null }
    Copy-Item -Force -Recurse "config\*" "$ConfigOut\"
    if (Test-Path "config\luaskills_root_skills.yaml") {
        Copy-Item -Force "config\luaskills_root_skills.yaml" "$LuaConfigOut\luaskills_root_skills.yaml"
    }
    Enable-OutputModelConfigForLocalTesting -ConfigDirectory $ConfigOut
    Write-Host "==> Base configs synced to $ConfigOut\"
}
else {
    Write-Host "==> No config directory found"
}

# Sync legacy runtime config files to output/config/ as an additive fallback.
# 将旧版 runtime 配置文件追加同步到 output/config/ 作为兼容回退。
if (Test-Path "runtime\configs") {
    Merge-DirectoryContents -Source "runtime\configs" -Destination $ConfigOut | Out-Null
    Enable-OutputModelConfigForLocalTesting -ConfigDirectory $ConfigOut
    Write-Host "==> Legacy runtime configs merged into $ConfigOut\"
}

# Sync runtime shared resources to output/resources/
if (Test-Path "runtime\resources") {
    Merge-DirectoryContents -Source "runtime\resources" -Destination $ResourcesOut | Out-Null
    Write-Host "==> Runtime shared resources synced to $ResourcesOut\"
}
else {
    Write-Host "==> No runtime/resources directory found"
}

# Sync runtime state records to output/lua_runtime/state/
# 同步运行时状态记录到 output/lua_runtime/state/
if (Test-Path "runtime\state") {
    Copy-Item -Force -Recurse "runtime\state\*" "$StateOut\"
    Write-Host "==> Runtime state synced to $StateOut\"
}
else {
    Write-Host "==> No runtime/state directory found"
}

# Sync runtime Lua skills to output/lua_runtime/skills/
# 同步运行时 Lua 技能到 output/lua_runtime/skills/
if (Test-Path "runtime\skills") {
    Merge-DirectoryContents -Source "runtime\skills" -Destination $SkillsOut | Out-Null
    Write-Host "==> Runtime Lua skills synced to $SkillsOut\"
}
else {
    Write-Host "==> No runtime/skills directory found"
}

# Prepare output/lua_runtime/bin/ as the LuaSkills 0.4.6 host runtime tool directory.
# 将 output/lua_runtime/bin/ 准备为 LuaSkills 0.4.6 宿主运行时工具目录。
$HostToolsOut = $RuntimeBinOut
if (-not (Test-Path $HostToolsOut)) { New-Item -ItemType Directory -Path $HostToolsOut -Force | Out-Null }
Write-Host "==> Host tool output directory prepared at $HostToolsOut\"

# Copy the host-installed vldb-controller executable to output/bin/ when the dependency bootstrap has prepared it.
$ControllerOut = Join-Path $BaseOutDir "bin"
if (-not (Test-Path $ControllerOut)) { New-Item -ItemType Directory -Path $ControllerOut -Force | Out-Null }
$ControllerBinaryName = "vldb-controller"
if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
    $ControllerBinaryName = "vldb-controller.exe"
}
$ControllerBinarySource = Join-Path $ProjectDir "third_party\vldb_controller\bin\$ControllerBinaryName"
if (Test-Path $ControllerBinarySource) {
    if (-not (Get-Item -LiteralPath $ControllerBinarySource).PSIsContainer) {
        try {
            Copy-Item -Force $ControllerBinarySource "$ControllerOut\$ControllerBinaryName" -ErrorAction Stop
            Write-Host "==> vldb-controller synced to $ControllerOut\"
        } catch {
            Write-Warning "vldb-controller is currently running or locked; keeping the existing output binary and continuing."
        }
    } else {
        Write-Error "vldb-controller source path is not a file: $ControllerBinarySource"
        exit 1
    }
} else {
    Write-Host "==> No third_party/vldb_controller/bin/$ControllerBinaryName found"
}

Write-Host "==> Done. Binary: $(Join-Path $OutDir $BinaryFileName)"
