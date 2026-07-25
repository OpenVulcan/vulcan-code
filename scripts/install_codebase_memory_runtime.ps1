#requires -Version 7.0

# DestinationRoot receives the verified native Codebase Memory runtime dependency.
# DestinationRoot 接收经过校验的原生 Codebase Memory 运行时依赖。
param(
    [string]$DestinationRoot = (Join-Path $PSScriptRoot "..\third_party\codebase_memory_runtime")
)

$ErrorActionPreference = "Stop"

# ProjectDir is the stable Vulcan Code repository root.
# ProjectDir 是稳定的 Vulcan Code 仓库根目录。
$ProjectDir = Split-Path $PSScriptRoot -Parent

# RuntimeRepository is the only upstream repository accepted by this installer.
# RuntimeRepository 是此安装器接受的唯一上游仓库。
$RuntimeRepository = "OpenVulcan/vulcan-codebase-memory-mcp"

# RuntimeTag pins the exact published release used by Vulcan Code.
# RuntimeTag 固定 Vulcan Code 使用的精确发布版本。
$RuntimeTag = "v0.1.0"

# RuntimeVersion is the byte-exact version expected from the packaged executable.
# RuntimeVersion 是打包可执行文件必须返回的字节精确版本。
$RuntimeVersion = "0.1.0"

# ChecksumsAssetName identifies the signed-off release checksum list.
# ChecksumsAssetName 标识已确认的发布校验和清单。
$ChecksumsAssetName = "checksums.txt"

# ChecksumsAssetSha256 pins the checksum-list bytes recorded by the official GitHub Release.
# ChecksumsAssetSha256 固定 GitHub 官方 Release 记录的校验和清单字节。
$ChecksumsAssetSha256 = "9c95f11959ecafc55f26ee2410a9e8801fdbcfef5279cdd7a83eee5a1e31287a"

# PackageRuntimeScript validates the extracted binary and creates the host manifest.
# PackageRuntimeScript 校验解压后的二进制并生成宿主清单。
$PackageRuntimeScript = Join-Path $PSScriptRoot "package_codebase_memory_runtime.ps1"

# Get-FileSha256 returns the lowercase SHA-256 digest for one exact file.
# Get-FileSha256 返回一个精确文件的小写 SHA-256 摘要。
#
# Path is the exact file whose bytes are hashed.
# Path 是需要计算字节摘要的精确文件。
#
# Returns one lowercase 64-character hexadecimal digest.
# 返回一个小写的 64 字符十六进制摘要。
function Get-FileSha256 {
    param(
        [string]$Path
    )

    # HashStream owns a read-only handle for the exact artifact.
    # HashStream 持有精确制品的只读句柄。
    $HashStream = [System.IO.File]::OpenRead($Path)
    try {
        # Sha256 owns the managed digest implementation.
        # Sha256 持有托管摘要实现。
        $Sha256 = [System.Security.Cryptography.SHA256]::Create()
        try {
            # HashBytes contains the digest computed from the complete stream.
            # HashBytes 保存从完整流计算得到的摘要。
            $HashBytes = $Sha256.ComputeHash($HashStream)
        }
        finally {
            $Sha256.Dispose()
        }
    }
    finally {
        $HashStream.Dispose()
    }

    return [System.BitConverter]::ToString($HashBytes).Replace("-", "").ToLowerInvariant()
}

# Assert-FileSha256 rejects an artifact whose bytes differ from the pinned digest.
# Assert-FileSha256 拒绝字节与固定摘要不一致的制品。
#
# Path is the exact downloaded or installed file, ExpectedHash is its pinned digest, and Label identifies errors.
# Path 是精确的下载或安装文件，ExpectedHash 是固定摘要，Label 用于标识错误。
#
# This function returns no value and throws on mismatch.
# 此函数不返回值，并在摘要不匹配时抛出错误。
function Assert-FileSha256 {
    param(
        [string]$Path,
        [string]$ExpectedHash,
        [string]$Label
    )

    # ActualHash is calculated from local bytes rather than remote metadata.
    # ActualHash 根据本地字节计算，而不是信任远端元数据。
    $ActualHash = Get-FileSha256 -Path $Path
    if ($ActualHash -cne $ExpectedHash.ToLowerInvariant()) {
        throw "Checksum mismatch for $Label. Expected $ExpectedHash, got $ActualHash."
    }
}

# Get-NativeRuntimeAsset resolves the only release asset valid for the current native host.
# Get-NativeRuntimeAsset 解析当前原生宿主唯一有效的发布资产。
#
# Returns a table containing platform, archive name, archive kind, and executable name.
# 返回包含平台、压缩包名称、压缩类型和可执行文件名的表。
function Get-NativeRuntimeAsset {
    # OsKey matches Rust std::env::consts::OS and the release asset naming contract.
    # OsKey 与 Rust std::env::consts::OS 及发布资产命名契约一致。
    $OsKey = if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
        "windows"
    }
    elseif ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)) {
        "linux"
    }
    elseif ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)) {
        "macos"
    }
    else {
        throw "Unsupported Codebase Memory runtime operating system."
    }

    # ArchKey matches Rust std::env::consts::ARCH and the release asset naming contract.
    # ArchKey 与 Rust std::env::consts::ARCH 及发布资产命名契约一致。
    $ArchKey = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
        "X64" { "x86_64" }
        "Arm64" { "aarch64" }
        default { throw "Unsupported Codebase Memory runtime architecture." }
    }

    if ($OsKey -eq "windows" -and $ArchKey -ne "x86_64") {
        throw "The v0.1.0 Codebase Memory release does not provide a Windows $ArchKey asset."
    }

    # Platform is the exact key consumed by the Vulcan Code runtime manifest.
    # Platform 是 Vulcan Code 运行时清单消费的精确键。
    $Platform = "$OsKey-$ArchKey"

    # ArchiveExtension selects the release packaging format for the current host.
    # ArchiveExtension 选择当前宿主对应的发布打包格式。
    $ArchiveExtension = if ($OsKey -eq "windows") { ".zip" } else { ".tar.gz" }

    # ExecutableName is the only dedicated product name accepted by the host.
    # ExecutableName 是宿主接受的唯一专版产品名称。
    $ExecutableName = if ($OsKey -eq "windows") {
        "vulcan-codebase-memory-mcp.exe"
    }
    else {
        "vulcan-codebase-memory-mcp"
    }

    return @{
        platform = $Platform
        asset_name = "vulcan-codebase-memory-mcp-$Platform$ArchiveExtension"
        archive_extension = $ArchiveExtension
        executable_name = $ExecutableName
    }
}

# Test-InstalledRuntimeCurrent verifies whether the dependency cache already contains the pinned runtime.
# Test-InstalledRuntimeCurrent 验证依赖缓存是否已经包含固定版本运行时。
#
# Root is the dependency cache directory and Asset describes the current native platform.
# Root 是依赖缓存目录，Asset 描述当前原生平台。
#
# Returns true only when manifest, executable, digest, platform, and embedded version all match.
# 仅当清单、可执行文件、摘要、平台和内嵌版本全部匹配时返回 true。
function Test-InstalledRuntimeCurrent {
    param(
        [string]$Root,
        [hashtable]$Asset
    )

    # ManifestPath is the cached host-validation manifest.
    # ManifestPath 是缓存的宿主校验清单。
    $ManifestPath = Join-Path $Root "manifest.json"

    # BinaryPath is the cached native executable.
    # BinaryPath 是缓存的原生可执行文件。
    $BinaryPath = Join-Path $Root $Asset.executable_name
    if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf) -or
        -not (Test-Path -LiteralPath $BinaryPath -PathType Leaf)) {
        return $false
    }

    try {
        # Manifest is parsed from the exact cache file used by build synchronization.
        # Manifest 从构建同步使用的精确缓存文件解析。
        $Manifest = Get-Content -LiteralPath $ManifestPath -Raw -Encoding utf8 | ConvertFrom-Json
        if ($Manifest.protocol -cne "vulcan.codebase-memory/1" -or
            $Manifest.version -cne $RuntimeVersion -or
            $Manifest.platform -cne $Asset.platform -or
            $Manifest.executable -cne $Asset.executable_name -or
            $Manifest.sha256 -cne (Get-FileSha256 -Path $BinaryPath)) {
            return $false
        }

        # VersionOutput proves the cached file itself belongs to the pinned release.
        # VersionOutput 证明缓存文件自身属于固定发布版本。
        $VersionOutput = (& $BinaryPath --version 2>&1 | Out-String).Trim()
        return $LASTEXITCODE -eq 0 -and
            $VersionOutput -ceq "vulcan-codebase-memory-mcp $RuntimeVersion"
    }
    catch {
        return $false
    }
}

if (-not (Test-Path -LiteralPath $PackageRuntimeScript -PathType Leaf)) {
    throw "Missing Codebase Memory packaging script: $PackageRuntimeScript"
}

# NativeAsset is the exact current-platform release selection.
# NativeAsset 是精确的当前平台发布资产选择。
$NativeAsset = Get-NativeRuntimeAsset

# ResolvedDestinationRoot is normalized once before cache inspection and packaging.
# ResolvedDestinationRoot 在缓存检查与打包前统一规范化。
$ResolvedDestinationRoot = [System.IO.Path]::GetFullPath($DestinationRoot)
if (Test-InstalledRuntimeCurrent -Root $ResolvedDestinationRoot -Asset $NativeAsset) {
    Write-Host "==> Codebase Memory runtime already installed ($RuntimeTag, $($NativeAsset.platform))."
    return
}

# TemporaryRoot is a unique bounded directory used only for this installation attempt.
# TemporaryRoot 是仅供本次安装使用的唯一受限临时目录。
$TemporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) (
    "vulcan-codebase-memory-{0}" -f [System.Guid]::NewGuid().ToString("N")
)
New-Item -ItemType Directory -Path $TemporaryRoot -Force | Out-Null

try {
    # ChecksumsPath stores the downloaded checksum list before trust validation.
    # ChecksumsPath 在可信校验前保存下载的校验和清单。
    $ChecksumsPath = Join-Path $TemporaryRoot $ChecksumsAssetName

    # ArchivePath stores the selected current-platform release archive.
    # ArchivePath 保存选定的当前平台发布压缩包。
    $ArchivePath = Join-Path $TemporaryRoot $NativeAsset.asset_name

    # ExtractRoot contains only files extracted from the verified release archive.
    # ExtractRoot 仅包含从已校验发布压缩包解压的文件。
    $ExtractRoot = Join-Path $TemporaryRoot "extract"
    New-Item -ItemType Directory -Path $ExtractRoot -Force | Out-Null

    # ReleaseBaseUrl is pinned to the exact repository and tag.
    # ReleaseBaseUrl 固定到精确仓库与标签。
    $ReleaseBaseUrl = "https://github.com/$RuntimeRepository/releases/download/$RuntimeTag"

    Write-Host "==> Downloading Codebase Memory release checksums: $RuntimeTag"
    Invoke-WebRequest -Uri "$ReleaseBaseUrl/$ChecksumsAssetName" -OutFile $ChecksumsPath -Headers @{
        "User-Agent" = "Vulcan-Code-Dependency-Installer"
    }
    Assert-FileSha256 -Path $ChecksumsPath -ExpectedHash $ChecksumsAssetSha256 -Label $ChecksumsAssetName

    # EscapedAssetName makes the checksum-line parser byte-specific to one asset.
    # EscapedAssetName 使校验和行解析器字节精确匹配单个资产。
    $EscapedAssetName = [System.Text.RegularExpressions.Regex]::Escape($NativeAsset.asset_name)

    # ChecksumMatches must contain exactly one standard sha256sum line for the selected asset.
    # ChecksumMatches 必须只包含一条选定资产的标准 sha256sum 记录。
    $ChecksumMatches = @(
        Get-Content -LiteralPath $ChecksumsPath -Encoding ascii |
            Where-Object { $_ -match "^([0-9a-f]{64})  $EscapedAssetName$" }
    )
    if ($ChecksumMatches.Count -ne 1) {
        throw "Expected exactly one checksum entry for $($NativeAsset.asset_name)."
    }

    # ExpectedArchiveHash is captured from the trusted release checksum list.
    # ExpectedArchiveHash 从可信发布校验和清单中提取。
    $null = $ChecksumMatches[0] -match "^([0-9a-f]{64})  "
    $ExpectedArchiveHash = $Matches[1]

    Write-Host "==> Downloading Codebase Memory runtime: $($NativeAsset.asset_name)"
    Invoke-WebRequest -Uri "$ReleaseBaseUrl/$($NativeAsset.asset_name)" -OutFile $ArchivePath -Headers @{
        "User-Agent" = "Vulcan-Code-Dependency-Installer"
    }
    Assert-FileSha256 -Path $ArchivePath -ExpectedHash $ExpectedArchiveHash -Label $NativeAsset.asset_name

    if ($NativeAsset.archive_extension -eq ".zip") {
        Expand-Archive -LiteralPath $ArchivePath -DestinationPath $ExtractRoot -Force
    }
    else {
        # TarCommand is required only for the tar.gz assets used by Linux and macOS.
        # TarCommand 仅用于 Linux 与 macOS 的 tar.gz 资产。
        $TarCommand = Get-Command "tar" -ErrorAction Stop
        & $TarCommand.Source -xzf $ArchivePath -C $ExtractRoot
        if ($LASTEXITCODE -ne 0) {
            throw "Unable to extract Codebase Memory runtime archive: $ArchivePath"
        }
    }

    # BinaryCandidates proves the archive contains exactly one dedicated runtime executable.
    # BinaryCandidates 证明压缩包只包含一个专版运行时可执行文件。
    $BinaryCandidates = @(
        Get-ChildItem -LiteralPath $ExtractRoot -Recurse -File |
            Where-Object { $_.Name -ceq $NativeAsset.executable_name }
    )
    if ($BinaryCandidates.Count -ne 1) {
        throw "Expected exactly one '$($NativeAsset.executable_name)' in $($NativeAsset.asset_name)."
    }

    & $PackageRuntimeScript `
        -BinaryPath $BinaryCandidates[0].FullName `
        -Version $RuntimeVersion `
        -Platform $NativeAsset.platform `
        -DestinationRoot $ResolvedDestinationRoot
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }

    if (-not (Test-InstalledRuntimeCurrent -Root $ResolvedDestinationRoot -Asset $NativeAsset)) {
        throw "Installed Codebase Memory runtime failed final verification."
    }

    Write-Host "==> Codebase Memory runtime dependency ready: $ResolvedDestinationRoot"
}
finally {
    if (Test-Path -LiteralPath $TemporaryRoot) {
        Remove-Item -LiteralPath $TemporaryRoot -Recurse -Force
    }
}
