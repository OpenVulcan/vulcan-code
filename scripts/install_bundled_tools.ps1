# install_bundled_tools.ps1 downloads the latest supported command-line tools for the current host.
# install_bundled_tools.ps1 为当前宿主下载最新的受支持命令行工具。

# ErrorActionPreference makes every dependency failure terminate the installer.
# ErrorActionPreference 使每个依赖失败都终止安装器。
$ErrorActionPreference = "Stop"

# ProjectDir stores the repository root for stable dependency paths.
# ProjectDir 保存仓库根目录，确保依赖路径稳定。
$ProjectDir = Split-Path $PSScriptRoot -Parent

# ToolsRoot stores target-specific downloaded tool payloads outside source control.
# ToolsRoot 保存目标平台专属的已下载工具载荷，且不纳入源码版本控制。
$ToolsRoot = Join-Path $ProjectDir "third_party\tools"

# AstGrepRepo identifies the authoritative ast-grep GitHub repository.
# AstGrepRepo 标识权威的 ast-grep GitHub 仓库。
$AstGrepRepo = "ast-grep/ast-grep"

# RipgrepRepo identifies the authoritative ripgrep GitHub repository.
# RipgrepRepo 标识权威的 ripgrep GitHub 仓库。
$RipgrepRepo = "BurntSushi/ripgrep"

# GitHubHeaders provides a stable API user agent for release discovery.
# GitHubHeaders 为发行版查询提供稳定的 API User-Agent。
$GitHubHeaders = @{ "User-Agent" = "Vulcan-Code-Bundled-Tools" }

# IsWindowsPlatform records whether the dependency bootstrap runs on Windows.
# IsWindowsPlatform 记录依赖初始化是否运行在 Windows。
$IsWindowsPlatform = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)

Set-Location $ProjectDir

# Ensures one exact directory exists.
# 确保一个精确目录存在。
#
# Parameters:
# 参数：
# Path is the directory to create when absent.
# Path 是缺失时需要创建的目录。
#
# Returns:
# 返回值：
# This function returns no value and throws when directory creation fails.
# 此函数不返回值，并在目录创建失败时抛出错误。
function Ensure-Directory {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        New-Item -ItemType Directory -Path $Path -Force | Out-Null
    }
}

# Returns the normalized architecture key used by upstream release assets.
# 返回上游发行制品使用的规范化架构键。
#
# Returns:
# 返回值：
# The current process architecture as x86_64 or aarch64.
# 当前进程架构，值为 x86_64 或 aarch64。
function Get-CurrentArchitectureKey {
    # ArchitectureName contains the runtime process architecture in lower case.
    # ArchitectureName 保存小写形式的运行时进程架构。
    $ArchitectureName = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString().ToLowerInvariant()
    switch ($ArchitectureName) {
        "x64" { return "x86_64" }
        "arm64" { return "aarch64" }
        default { throw "Unsupported bundled-tool architecture: $ArchitectureName" }
    }
}

# Resolves the canonical target directory shared by sg and rg payloads.
# 解析 sg 与 rg 载荷共用的规范目标目录。
#
# Returns:
# 返回值：
# The Rust-style target triple for the current host.
# 当前宿主对应的 Rust 风格目标三元组。
function Get-CurrentToolTarget {
    # ArchitectureKey is the normalized architecture selected for release assets.
    # ArchitectureKey 是为发行制品选择的规范化架构。
    $ArchitectureKey = Get-CurrentArchitectureKey
    if ($IsWindowsPlatform) {
        return "$ArchitectureKey-pc-windows-msvc"
    }
    if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)) {
        return "$ArchitectureKey-apple-darwin"
    }
    if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)) {
        return "$ArchitectureKey-unknown-linux-gnu"
    }
    throw "Unsupported bundled-tool operating system."
}

# Queries one repository's current non-prerelease GitHub Latest Release.
# 查询一个仓库当前非预发布的 GitHub Latest Release。
#
# Parameters:
# 参数：
# Repo is the owner/name GitHub repository identifier.
# Repo 是 owner/name 形式的 GitHub 仓库标识。
#
# Returns:
# 返回值：
# The decoded GitHub release response with a non-empty tag and asset list.
# 返回包含非空标签与资产列表的已解码 GitHub 发行响应。
function Get-LatestGitHubRelease {
    param([string]$Repo)

    # ApiUrl addresses the authoritative Latest Release endpoint.
    # ApiUrl 指向权威的 Latest Release 端点。
    $ApiUrl = "https://api.github.com/repos/$Repo/releases/latest"
    # Release contains the decoded release metadata returned by GitHub.
    # Release 保存 GitHub 返回的已解码发行元数据。
    $Release = Invoke-RestMethod -Uri $ApiUrl -Headers $GitHubHeaders -UseBasicParsing
    if ([string]::IsNullOrWhiteSpace([string]$Release.tag_name)) {
        throw "GitHub Latest Release for '$Repo' did not provide a tag name."
    }
    if (@($Release.assets).Count -eq 0) {
        throw "GitHub Latest Release '$($Release.tag_name)' for '$Repo' has no downloadable assets."
    }
    return $Release
}

# Selects the exact release asset and executable name for one tool on the current host.
# 为当前宿主上的一个工具选择精确发行资产与可执行文件名。
#
# Parameters:
# 参数：
# ToolId is either sg or rg; ReleaseTag is the discovered Latest Release tag.
# ToolId 为 sg 或 rg；ReleaseTag 为查询得到的 Latest Release 标签。
#
# Returns:
# 返回值：
# A hashtable containing the repository target, archive name, archive kind, and executable name.
# 返回包含仓库目标、压缩包名称、压缩类型与可执行文件名的哈希表。
function Get-ToolAssetSpec {
    param(
        [ValidateSet("sg", "rg")]
        [string]$ToolId,
        [string]$ReleaseTag
    )

    # ToolTarget is the canonical installation target directory.
    # ToolTarget 是规范的安装目标目录。
    $ToolTarget = Get-CurrentToolTarget
    # BinarySuffix selects the Windows executable suffix only on Windows.
    # BinarySuffix 仅在 Windows 上选择可执行文件后缀。
    $BinarySuffix = if ($IsWindowsPlatform) { ".exe" } else { "" }

    if ($ToolId -eq "sg") {
        return @{
            target = $ToolTarget
            asset_name = "app-$ToolTarget.zip"
            archive_kind = "zip"
            source_binary_name = "ast-grep$BinarySuffix"
            binary_name = "sg$BinarySuffix"
        }
    }

    # RipgrepAssetTarget selects a static Linux archive while keeping the installation target stable.
    # RipgrepAssetTarget 在保持安装目标稳定的同时，为 Linux 选择静态归档。
    $RipgrepAssetTarget = $ToolTarget
    if ($ToolTarget -eq "x86_64-unknown-linux-gnu") {
        $RipgrepAssetTarget = "x86_64-unknown-linux-musl"
    }
    elseif ($ToolTarget -eq "aarch64-unknown-linux-gnu") {
        $RipgrepAssetTarget = "aarch64-unknown-linux-musl"
    }
    # ArchiveKind selects zip for Windows and tar.gz for Unix-like hosts.
    # ArchiveKind 为 Windows 选择 zip，为 Unix-like 宿主选择 tar.gz。
    $ArchiveKind = if ($IsWindowsPlatform) { "zip" } else { "tar.gz" }
    return @{
        target = $ToolTarget
        asset_name = "ripgrep-$ReleaseTag-$RipgrepAssetTarget.$ArchiveKind"
        archive_kind = $ArchiveKind
        source_binary_name = "rg$BinarySuffix"
        binary_name = "rg$BinarySuffix"
    }
}

# Extracts one mandatory SHA-256 value from GitHub release-asset metadata.
# 从 GitHub 发行资产元数据中提取一个必需的 SHA-256 值。
#
# Parameters:
# 参数：
# Asset is the exact GitHub release asset selected for installation.
# Asset 是为安装选择的精确 GitHub 发行资产。
#
# Returns:
# 返回值：
# The lowercase 64-character SHA-256 digest without the algorithm prefix.
# 返回不含算法前缀的小写 64 字符 SHA-256 摘要。
function Get-ReleaseAssetSha256 {
    param($Asset)

    # DigestText contains GitHub's algorithm-prefixed digest value.
    # DigestText 保存 GitHub 带算法前缀的摘要值。
    $DigestText = [string]$Asset.digest
    if ($DigestText -notmatch '^sha256:([0-9a-fA-F]{64})$') {
        throw "Release asset '$($Asset.name)' does not provide a valid SHA-256 digest."
    }
    return $Matches[1].ToLowerInvariant()
}

# Verifies one downloaded archive against the authoritative GitHub SHA-256 digest.
# 根据 GitHub 权威 SHA-256 摘要校验一个已下载压缩包。
#
# Parameters:
# 参数：
# Path is the downloaded file; ExpectedHash is the required digest; Label identifies diagnostics.
# Path 是已下载文件；ExpectedHash 是必需摘要；Label 用于标识诊断信息。
#
# Returns:
# 返回值：
# This function returns no value and throws on any mismatch.
# 此函数不返回值，并在任何不匹配时抛出错误。
function Assert-Sha256 {
    param(
        [string]$Path,
        [string]$ExpectedHash,
        [string]$Label
    )

    # HashStream opens the exact artifact for a PowerShell-version-independent hash calculation.
    # HashStream 打开精确制品，以执行不依赖 PowerShell 版本的哈希计算。
    $HashStream = [System.IO.File]::OpenRead($Path)
    try {
        # Sha256 owns the managed SHA-256 implementation.
        # Sha256 持有托管 SHA-256 实现。
        $Sha256 = [System.Security.Cryptography.SHA256]::Create()
        try {
            # HashBytes contains the exact digest bytes for the artifact.
            # HashBytes 保存该制品的精确摘要字节。
            $HashBytes = $Sha256.ComputeHash($HashStream)
        }
        finally {
            $Sha256.Dispose()
        }
    }
    finally {
        $HashStream.Dispose()
    }
    # ActualHash uses the same lowercase hexadecimal representation as GitHub metadata.
    # ActualHash 使用与 GitHub 元数据相同的小写十六进制表示。
    $ActualHash = [System.BitConverter]::ToString($HashBytes).Replace("-", "").ToLowerInvariant()
    if ($ActualHash -ne $ExpectedHash) {
        throw "Checksum mismatch for $Label. Expected $ExpectedHash, got $ActualHash"
    }
}

# Returns an available tar executable for tar.gz extraction.
# 返回一个可用于解压 tar.gz 的 tar 可执行文件。
#
# Returns:
# 返回值：
# The exact tar executable path discovered on the current host.
# 当前宿主上发现的精确 tar 可执行文件路径。
function Get-TarExecutable {
    if ($IsWindowsPlatform) {
        # SystemTar is the Windows inbox tar executable candidate.
        # SystemTar 是 Windows 内置 tar 可执行文件候选。
        $SystemTar = Join-Path $env:SystemRoot "System32\tar.exe"
        if (Test-Path -LiteralPath $SystemTar -PathType Leaf) {
            return $SystemTar
        }
    }
    # CommandName iterates platform-neutral and Windows tar command spellings.
    # CommandName 遍历平台无关与 Windows tar 命令拼写。
    foreach ($CommandName in @("tar", "tar.exe")) {
        # TarCommand is one PATH-resolved tar candidate.
        # TarCommand 是一个从 PATH 解析出的 tar 候选。
        $TarCommand = Get-Command $CommandName -ErrorAction SilentlyContinue
        if ($TarCommand) {
            return $TarCommand.Source
        }
    }
    throw "tar is required to extract the selected bundled-tool archive."
}

# Makes one installed Unix-like executable runnable by child shells.
# 使一个已安装的 Unix-like 可执行文件可由子 shell 运行。
#
# Parameters:
# 参数：
# Path is the exact installed executable.
# Path 是精确的已安装可执行文件。
#
# Returns:
# 返回值：
# This function returns no value and throws when chmod fails.
# 此函数不返回值，并在 chmod 失败时抛出错误。
function Set-ToolExecutablePermission {
    param([string]$Path)

    if ($IsWindowsPlatform) {
        return
    }
    # ChmodCommand is the required Unix permission utility.
    # ChmodCommand 是必需的 Unix 权限工具。
    $ChmodCommand = Get-Command "chmod" -ErrorAction Stop
    & $ChmodCommand.Source "755" $Path
    if ($LASTEXITCODE -ne 0) {
        throw "chmod failed for bundled tool: $Path"
    }
}

# Validates that one installed executable reports the discovered release version.
# 校验一个已安装可执行文件是否报告查询得到的发行版本。
#
# Parameters:
# 参数：
# BinaryPath is the executable; ReleaseTag is the expected Latest Release tag; ToolId labels errors.
# BinaryPath 是可执行文件；ReleaseTag 是预期 Latest Release 标签；ToolId 用于标识错误。
#
# Returns:
# 返回值：
# This function returns no value and throws when execution or version matching fails.
# 此函数不返回值，并在执行或版本匹配失败时抛出错误。
function Assert-ToolVersion {
    param(
        [string]$BinaryPath,
        [string]$ReleaseTag,
        [string]$ToolId
    )

    # VersionOutput captures stdout and stderr because upstream CLIs may choose either stream.
    # VersionOutput 同时捕获标准输出与标准错误，因为上游 CLI 可能选择任一流。
    $VersionOutput = (& $BinaryPath --version 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) {
        throw "Bundled tool '$ToolId' failed its --version check: $VersionOutput"
    }
    # ExpectedVersion removes an optional v prefix used by some GitHub tags.
    # ExpectedVersion 去除部分 GitHub 标签使用的可选 v 前缀。
    $ExpectedVersion = $ReleaseTag.TrimStart('v')
    if ($VersionOutput -notmatch [regex]::Escape($ExpectedVersion)) {
        throw "Bundled tool '$ToolId' reported '$VersionOutput' instead of release '$ReleaseTag'."
    }
}

# Downloads one tag-pinned license file into the shared bundled-tools license directory.
# 把一个固定到标签的许可证文件下载到共享内建工具许可证目录。
#
# Parameters:
# 参数：
# Repo identifies the repository; ReleaseTag pins the source; SourceName and DestinationPath select the file.
# Repo 标识仓库；ReleaseTag 固定源码；SourceName 与 DestinationPath 选择文件。
#
# Returns:
# 返回值：
# This function returns no value and throws when download fails.
# 此函数不返回值，并在下载失败时抛出错误。
function Install-ToolLicense {
    param(
        [string]$Repo,
        [string]$ReleaseTag,
        [string]$SourceName,
        [string]$DestinationPath
    )

    # LicenseUrl points at the exact release-tagged upstream license file.
    # LicenseUrl 指向精确发行标签下的上游许可证文件。
    $LicenseUrl = "https://raw.githubusercontent.com/$Repo/$ReleaseTag/$SourceName"
    Invoke-WebRequest -Uri $LicenseUrl -Headers $GitHubHeaders -OutFile $DestinationPath -UseBasicParsing
}

# Installs one latest-release CLI into the current target payload directory.
# 把一个 Latest Release CLI 安装到当前目标载荷目录。
#
# Parameters:
# 参数：
# ToolId is sg or rg; Repo is its authoritative upstream repository.
# ToolId 为 sg 或 rg；Repo 为其权威上游仓库。
#
# Returns:
# 返回值：
# This function returns no value and leaves a verified binary plus metadata and licenses.
# 此函数不返回值，并留下已校验的二进制、元数据与许可证。
function Install-LatestTool {
    param(
        [ValidateSet("sg", "rg")]
        [string]$ToolId,
        [string]$Repo
    )

    Write-Host "==> Resolving latest $ToolId release from $Repo..."
    # Release contains the authoritative Latest Release metadata.
    # Release 保存权威 Latest Release 元数据。
    $Release = Get-LatestGitHubRelease -Repo $Repo
    # ReleaseTag is the exact upstream version selected for this invocation.
    # ReleaseTag 是本次调用选择的精确上游版本。
    $ReleaseTag = [string]$Release.tag_name
    # AssetSpec selects the current-host archive and executable.
    # AssetSpec 选择当前宿主的压缩包与可执行文件。
    $AssetSpec = Get-ToolAssetSpec -ToolId $ToolId -ReleaseTag $ReleaseTag
    # Asset is the one exact release asset whose name matches the platform specification.
    # Asset 是名称匹配平台规格的唯一精确发行资产。
    $Asset = @($Release.assets | Where-Object { $_.name -eq $AssetSpec.asset_name }) | Select-Object -First 1
    if (-not $Asset) {
        # AvailableAssets lists upstream names for an actionable platform failure.
        # AvailableAssets 列出上游名称，以便平台失败时采取行动。
        $AvailableAssets = (@($Release.assets) | ForEach-Object { $_.name }) -join ", "
        throw "Latest $ToolId release '$ReleaseTag' does not contain '$($AssetSpec.asset_name)'. Available assets: $AvailableAssets"
    }
    # ExpectedHash is the mandatory GitHub-computed archive digest.
    # ExpectedHash 是 GitHub 计算的必需压缩包摘要。
    $ExpectedHash = Get-ReleaseAssetSha256 -Asset $Asset
    # TargetRoot stores all current-platform tool payloads.
    # TargetRoot 保存全部当前平台工具载荷。
    $TargetRoot = Join-Path $ToolsRoot $AssetSpec.target
    # TargetBinDir is synchronized directly into output/tools/bin.
    # TargetBinDir 会被直接同步到 output/tools/bin。
    $TargetBinDir = Join-Path $TargetRoot "bin"
    # LicenseRoot stores distributable upstream license texts by tool.
    # LicenseRoot 按工具保存可分发的上游许可证文本。
    $LicenseRoot = Join-Path $TargetRoot "licenses\$ToolId"
    # BinaryDestination is the exact installed command resolved through PATH.
    # BinaryDestination 是通过 PATH 解析的精确已安装命令。
    $BinaryDestination = Join-Path $TargetBinDir $AssetSpec.binary_name
    # MetadataPath records the resolved latest tag and verified archive identity.
    # MetadataPath 记录已解析的最新标签与已校验压缩包身份。
    $MetadataPath = Join-Path $TargetRoot "$ToolId.json"
    Ensure-Directory -Path $TargetBinDir
    Ensure-Directory -Path $LicenseRoot

    # ExistingMetadata contains the previous successful installation record when readable.
    # ExistingMetadata 保存可读取的上次成功安装记录。
    $ExistingMetadata = $null
    if (Test-Path -LiteralPath $MetadataPath -PathType Leaf) {
        try {
            $ExistingMetadata = Get-Content -LiteralPath $MetadataPath -Raw | ConvertFrom-Json
        }
        catch {
            $ExistingMetadata = $null
        }
    }
    # BinaryIsCurrent proves both metadata identity and executable version before skipping download.
    # BinaryIsCurrent 在跳过下载前同时证明元数据身份与可执行文件版本。
    $BinaryIsCurrent = $false
    if (
        $ExistingMetadata -and
        [string]$ExistingMetadata.tag -eq $ReleaseTag -and
        [int]$ExistingMetadata.schema_version -eq 2 -and
        [string]$ExistingMetadata.asset -eq [string]$AssetSpec.asset_name -and
        [string]$ExistingMetadata.source_binary -eq [string]$AssetSpec.source_binary_name -and
        [string]$ExistingMetadata.sha256 -eq $ExpectedHash -and
        (Test-Path -LiteralPath $BinaryDestination -PathType Leaf)
    ) {
        try {
            Assert-ToolVersion -BinaryPath $BinaryDestination -ReleaseTag $ReleaseTag -ToolId $ToolId
            $BinaryIsCurrent = $true
        }
        catch {
            $BinaryIsCurrent = $false
        }
    }

    if (-not $BinaryIsCurrent) {
        # TempRoot isolates this exact download and extraction transaction.
        # TempRoot 隔离本次精确下载与解压事务。
        $TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("vulcan_tools_{0}" -f [Guid]::NewGuid().ToString("N"))
        Ensure-Directory -Path $TempRoot
        try {
            # ArchivePath is the local verified release archive.
            # ArchivePath 是本地已校验发行压缩包。
            $ArchivePath = Join-Path $TempRoot $AssetSpec.asset_name
            # ExtractRoot contains only files expanded from the verified archive.
            # ExtractRoot 仅包含从已校验压缩包展开的文件。
            $ExtractRoot = Join-Path $TempRoot "expanded"
            Ensure-Directory -Path $ExtractRoot
            Write-Host "==> Downloading $ToolId ${ReleaseTag}: $($AssetSpec.asset_name)"
            Invoke-WebRequest -Uri $Asset.browser_download_url -Headers $GitHubHeaders -OutFile $ArchivePath -UseBasicParsing
            Assert-Sha256 -Path $ArchivePath -ExpectedHash $ExpectedHash -Label $AssetSpec.asset_name
            if ($AssetSpec.archive_kind -eq "zip") {
                Expand-Archive -LiteralPath $ArchivePath -DestinationPath $ExtractRoot -Force
            }
            else {
                # TarExecutable is the exact extractor used for this tar.gz asset.
                # TarExecutable 是本次 tar.gz 制品使用的精确解压器。
                $TarExecutable = Get-TarExecutable
                & $TarExecutable -xzf $ArchivePath -C $ExtractRoot
                if ($LASTEXITCODE -ne 0) {
                    throw "tar extraction failed for $($AssetSpec.asset_name)."
                }
            }
            # BinarySource is the exact executable discovered inside the verified archive.
            # BinarySource 是在已校验压缩包内发现的精确可执行文件。
            $BinarySource = Get-ChildItem -LiteralPath $ExtractRoot -Recurse -File -Filter $AssetSpec.source_binary_name | Select-Object -First 1
            if (-not $BinarySource) {
                throw "Executable '$($AssetSpec.source_binary_name)' was not found in '$($AssetSpec.asset_name)'."
            }
            Copy-Item -LiteralPath $BinarySource.FullName -Destination $BinaryDestination -Force
            Set-ToolExecutablePermission -Path $BinaryDestination
            Assert-ToolVersion -BinaryPath $BinaryDestination -ReleaseTag $ReleaseTag -ToolId $ToolId
            # Metadata is the durable successful-installation record written only after validation.
            # Metadata 是仅在校验完成后写入的持久成功安装记录。
            $Metadata = [ordered]@{
                schema_version = 2
                tool = $ToolId
                repository = $Repo
                tag = $ReleaseTag
                asset = [string]$AssetSpec.asset_name
                source_binary = [string]$AssetSpec.source_binary_name
                sha256 = $ExpectedHash
            }
            $Metadata | ConvertTo-Json | Set-Content -LiteralPath $MetadataPath -Encoding UTF8
        }
        finally {
            if (Test-Path -LiteralPath $TempRoot) {
                Remove-Item -LiteralPath $TempRoot -Recurse -Force -ErrorAction SilentlyContinue
            }
        }
    }
    else {
        Write-Host "==> $ToolId is already current ($ReleaseTag)."
    }

    if ($ToolId -eq "sg") {
        Install-ToolLicense -Repo $Repo -ReleaseTag $ReleaseTag -SourceName "LICENSE" -DestinationPath (Join-Path $LicenseRoot "LICENSE")
    }
    else {
        Install-ToolLicense -Repo $Repo -ReleaseTag $ReleaseTag -SourceName "LICENSE-MIT" -DestinationPath (Join-Path $LicenseRoot "LICENSE-MIT")
        Install-ToolLicense -Repo $Repo -ReleaseTag $ReleaseTag -SourceName "UNLICENSE" -DestinationPath (Join-Path $LicenseRoot "UNLICENSE")
    }
    Write-Host "==> $ToolId $ReleaseTag installed: $BinaryDestination"
}

Install-LatestTool -ToolId "sg" -Repo $AstGrepRepo
Install-LatestTool -ToolId "rg" -Repo $RipgrepRepo

Write-Host "==> Latest bundled tools installed under: $ToolsRoot"
