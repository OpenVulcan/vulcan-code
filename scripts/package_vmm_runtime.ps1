param(
    [string]$ProjectDir = "",
    [string]$VmmSourceDir = $env:VULCAN_VMM_SOURCE_DIR
)

# Build and package the sibling VulcanMemoryMesh managed runtime.
# 构建并打包同级 VulcanMemoryMesh 托管运行时。

$ErrorActionPreference = "Stop"

# Resolve the owning Vulcan Code repository when callers omit it.
# 调用方省略时解析所属 Vulcan Code 仓库。

if ([string]::IsNullOrWhiteSpace($ProjectDir)) {
    $ProjectDir = Split-Path -Parent $PSScriptRoot
}

# Resolve the default sibling source checkout when no explicit source path is configured.
# 未配置显式源码路径时解析默认的同级源码检出目录。

if ([string]::IsNullOrWhiteSpace($VmmSourceDir)) {
    $VmmSourceDir = Join-Path (Split-Path -Parent $ProjectDir) "VulcanMemoryMesh"
}
$VmmSourceDir = [System.IO.Path]::GetFullPath($VmmSourceDir)

# Validate the exact build entrypoint before invoking another repository.
# 调用另一个仓库前校验精确构建入口。

$VmmBuildScript = Join-Path $VmmSourceDir "make.bat"
if (-not (Test-Path -LiteralPath $VmmBuildScript -PathType Leaf)) {
    throw "VulcanMemoryMesh build entrypoint was not found: $VmmBuildScript"
}

Write-Host "==> Building managed VulcanMemoryMesh runtime..."
Push-Location $VmmSourceDir
try {
    & $VmmBuildScript build
    if ($LASTEXITCODE -ne 0) {
        throw "VulcanMemoryMesh managed runtime build failed with exit code $LASTEXITCODE."
    }
}
finally {
    Pop-Location
}

# Resolve platform-specific source and destination binaries.
# 解析平台相关的源二进制与目标二进制。

$RunningOnWindows = if (Get-Variable -Name IsWindows -ErrorAction SilentlyContinue) {
    $IsWindows
}
else {
    [System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT
}
$OutputBinDir = Join-Path $ProjectDir "output\bin"
New-Item -ItemType Directory -Path $OutputBinDir -Force | Out-Null

# Copy both the online runtime and its isolated maintenance companion.
# 同时复制在线运行时与隔离的维护配套程序。

$VmmBinaryNames = if ($RunningOnWindows) {
    @("vmm-local.exe", "vmm-migrate.exe")
}
else {
    @("vmm-local", "vmm-migrate")
}
foreach ($VmmBinaryName in $VmmBinaryNames) {
    $VmmSourceBinary = Join-Path $VmmSourceDir "output\bin\$VmmBinaryName"
    $VmmOutputBinary = Join-Path $OutputBinDir $VmmBinaryName
    if (-not (Test-Path -LiteralPath $VmmSourceBinary -PathType Leaf)) {
        throw "Built VulcanMemoryMesh binary was not found: $VmmSourceBinary"
    }
    Copy-Item -LiteralPath $VmmSourceBinary -Destination $VmmOutputBinary -Force
}

# Copy the packaged VMM layout required for asset and native-library discovery.
# 复制 VMM 资产与原生库发现所需的打包布局。

$OutputConfigDir = Join-Path $ProjectDir "output\configs"
New-Item -ItemType Directory -Path $OutputConfigDir -Force | Out-Null
$ResolvedOutputConfigDir = [System.IO.Path]::GetFullPath($OutputConfigDir)
foreach ($ConfigFileName in @("base.yaml", "config.yaml")) {
    $SourceConfigFile = Join-Path $VmmSourceDir "output\configs\$ConfigFileName"
    if (-not (Test-Path -LiteralPath $SourceConfigFile -PathType Leaf)) {
        throw "VulcanMemoryMesh packaged config file was not found: $SourceConfigFile"
    }
    Copy-Item -LiteralPath $SourceConfigFile -Destination (Join-Path $OutputConfigDir $ConfigFileName) -Force
}
foreach ($AssetDirectoryName in @("noise_rules", "pii_rules", "prompts")) {
    $SourceAssetDirectory = Join-Path $VmmSourceDir "output\configs\$AssetDirectoryName"
    $DestinationAssetCandidate = Join-Path $ResolvedOutputConfigDir $AssetDirectoryName
    $DestinationAssetDirectory = [System.IO.Path]::GetFullPath($DestinationAssetCandidate)
    if (-not (Test-Path -LiteralPath $SourceAssetDirectory -PathType Container)) {
        throw "VulcanMemoryMesh managed asset directory was not found: $SourceAssetDirectory"
    }
    # Verify recursive replacement remains inside the intended packaged configuration directory.
    # 校验递归替换始终位于预期的打包配置目录内。
    $PathSeparators = @(
        [System.IO.Path]::DirectorySeparatorChar,
        [System.IO.Path]::AltDirectorySeparatorChar
    )
    $ExpectedPrefix = $ResolvedOutputConfigDir.TrimEnd([char[]]$PathSeparators) + [System.IO.Path]::DirectorySeparatorChar
    if (-not $DestinationAssetDirectory.StartsWith($ExpectedPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to replace a VMM asset directory outside the package root: $DestinationAssetDirectory"
    }
    if (Test-Path -LiteralPath $DestinationAssetDirectory) {
        Remove-Item -LiteralPath $DestinationAssetDirectory -Recurse -Force
    }
    Copy-Item -LiteralPath $SourceAssetDirectory -Destination $DestinationAssetDirectory -Recurse -Force
}

Write-Host "==> Managed VulcanMemoryMesh runtime and maintenance companion packaged in: $OutputBinDir"
