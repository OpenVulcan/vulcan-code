#requires -Version 7.0

param(
    [Parameter(Mandatory = $true)]
    [string]$BinaryPath,
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [string]$Platform = "",
    [string]$DestinationRoot = (Join-Path $PSScriptRoot "..\output\dependencies\runtimes\codebase-memory")
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Version)) {
    throw "Codebase Memory runtime version must not be empty."
}
$Version = $Version.Trim()
if ($Version.StartsWith("v", [System.StringComparison]::OrdinalIgnoreCase)) {
    $Version = $Version.Substring(1)
}

# osKey matches Rust's std::env::consts::OS values.
# osKey 与 Rust 的 std::env::consts::OS 值保持一致。
$osKey = if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
    "windows"
} elseif ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)) {
    "linux"
} elseif ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)) {
    "macos"
} else {
    throw "Unsupported Codebase Memory runtime operating system."
}

# archKey matches Rust's std::env::consts::ARCH values.
# archKey 与 Rust 的 std::env::consts::ARCH 值保持一致。
$archKey = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
    "X64" { "x86_64" }
    "Arm64" { "aarch64" }
    default { throw "Unsupported Codebase Memory runtime architecture." }
}

# nativePlatform is the only platform a native package operation may declare.
# nativePlatform 是原生打包操作唯一可以声明的平台。
$nativePlatform = "$osKey-$archKey"
if ([string]::IsNullOrWhiteSpace($Platform)) {
    $Platform = $nativePlatform
}

# supportedPlatforms is the exact Vulcan Code distribution matrix.
# supportedPlatforms 是 Vulcan Code 精确支持的分发矩阵。
$supportedPlatforms = @(
    "linux-x86_64",
    "linux-aarch64",
    "macos-x86_64",
    "macos-aarch64",
    "windows-x86_64"
)
if ($Platform -notin $supportedPlatforms) {
    throw "Unsupported Vulcan Codebase Memory runtime platform: $Platform"
}
if ($Platform -ne $nativePlatform) {
    throw "Vulcan Codebase Memory runtime must be packaged on its native platform: expected $nativePlatform, received $Platform."
}

# sourceBinary is the exact Vulcan-specific sidecar artifact selected for packaging.
# sourceBinary 是为打包选择的精确 Vulcan 专用 Sidecar 产物。
$sourceBinary = (Resolve-Path -LiteralPath $BinaryPath).Path
$sourceBinaryItem = Get-Item -LiteralPath $sourceBinary
if ($sourceBinaryItem.PSIsContainer) {
    throw "Codebase Memory runtime binary path must reference a file."
}

# expectedSourceName prevents a public or launcher payload from entering the host distribution.
# expectedSourceName 防止公共版或 launcher 载荷进入宿主分发目录。
$expectedSourceName = if ($osKey -eq "windows") {
    "vulcan-codebase-memory-mcp.exe"
} else {
    "vulcan-codebase-memory-mcp"
}
if ($sourceBinaryItem.Name -cne $expectedSourceName) {
    throw "Codebase Memory runtime must use the dedicated product name '$expectedSourceName': $sourceBinary"
}

# versionOutput proves that the file itself carries the manifest version.
# versionOutput 用于证明文件自身携带的版本与清单版本一致。
$versionOutput = (& $sourceBinary --version 2>&1 | Out-String).Trim()
if ($LASTEXITCODE -ne 0) {
    throw "Unable to query the Vulcan Codebase Memory runtime version: $sourceBinary"
}
$expectedVersionOutput = "vulcan-codebase-memory-mcp $Version"
if ($versionOutput -cne $expectedVersionOutput) {
    throw "Codebase Memory runtime version output '$versionOutput' does not match '$expectedVersionOutput'."
}

# destinationDirectory is the host-owned managed distribution directory.
# destinationDirectory 是宿主持有的托管分发目录。
$destinationDirectory = [System.IO.Path]::GetFullPath($DestinationRoot)
New-Item -ItemType Directory -Force -Path $destinationDirectory | Out-Null

# destinationBinaryName is the only dedicated product name accepted by the host.
# destinationBinaryName 是宿主接受的唯一专版产品名称。
$destinationBinaryName = $expectedSourceName
$destinationBinary = Join-Path $destinationDirectory $destinationBinaryName

# staleRuntimeNames enumerates only known runtime files below the exact distribution directory.
# staleRuntimeNames 只枚举精确分发目录中的已知运行时文件。
$staleRuntimeNames = @(
    "vulcan-codebase-memory-mcp",
    "vulcan-codebase-memory-mcp.exe",
    "codebase-memory-mcp",
    "codebase-memory-mcp.exe",
    "manifest.json.tmp"
)
foreach ($staleRuntimeName in $staleRuntimeNames) {
    $staleRuntimePath = Join-Path $destinationDirectory $staleRuntimeName
    if (Test-Path -LiteralPath $staleRuntimePath -PathType Leaf) {
        Remove-Item -LiteralPath $staleRuntimePath -Force
    }
}
Copy-Item -LiteralPath $sourceBinary -Destination $destinationBinary -Force

# digest pins the exact copied bytes rather than the source path metadata.
# digest 固定精确复制后的字节，而不是源路径元数据。
$digest = (Get-FileHash -LiteralPath $destinationBinary -Algorithm SHA256).Hash.ToLowerInvariant()

# manifest records the complete host validation contract consumed before process startup.
# manifest 记录进程启动前由宿主消费的完整校验契约。
$manifest = [ordered]@{
    protocol = "vulcan.codebase-memory/2"
    version = $Version
    platform = $Platform
    executable = $destinationBinaryName
    sha256 = $digest
}
$manifestPath = Join-Path $destinationDirectory "manifest.json"
$temporaryManifestPath = "$manifestPath.tmp"
$manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $temporaryManifestPath -Encoding utf8NoBOM
[System.IO.File]::Move($temporaryManifestPath, $manifestPath, $true)

Write-Output "Packaged Codebase Memory runtime: $destinationBinary"
Write-Output "Manifest: $manifestPath"
Write-Output "SHA-256: $digest"
