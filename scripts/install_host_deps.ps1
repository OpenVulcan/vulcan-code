# install_host_deps.ps1 - Install host dependencies and bundled plugin artifacts.
# install_host_deps.ps1 - 安装宿主依赖与内建插件制品。
# Developer/build use only. End users do not need to invoke this manually.
# 仅供开发与构建使用，最终用户无需手动调用。
# Usage: powershell -ExecutionPolicy Bypass -File scripts/install_host_deps.ps1

param(
    [ValidateSet("all", "rtk")]
    [string]$Kind = "all"
)

$ErrorActionPreference = "Stop"
# Use the same host-aware extractor as the Lua runtime installer.
# 与 Lua 运行时安装器使用同一套按宿主选择的解压逻辑。
. (Join-Path $PSScriptRoot 'dependency-archive.ps1')
$ProjectDir = Split-Path $PSScriptRoot -Parent
Set-Location $ProjectDir

$script:IsWindowsPlatform = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)
$script:IsMacOSPlatform   = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)
$script:IsLinuxPlatform   = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)

$ThirdParty = Join-Path $ProjectDir "third_party"
# RtkPluginDir is the real RTK plugin package owned and distributed by Vulcan Code.
# RtkPluginDir 是由 Vulcan Code 持有并分发的真实 RTK 插件包。
$RtkPluginDir = Join-Path $ProjectDir "plugins\vulcan-rtk"

# RtkBinRoot stores target-specific executables directly inside the RTK plugin package.
# RtkBinRoot 直接在 RTK 插件包内保存按目标平台划分的可执行文件。
$RtkBinRoot = Join-Path $RtkPluginDir "bin"

# RtkRepo identifies the upstream RTK GitHub repository.
$RtkRepo = "rtk-ai/rtk"

# RtkTag pins the exact RTK release used by this dependency bootstrap.
$RtkTag = "v0.42.4"

# RtkLicenseUrl points at the tag-pinned source license because RTK binary assets do not include it.
$RtkLicenseUrl = "https://raw.githubusercontent.com/rtk-ai/rtk/$RtkTag/LICENSE"

# RtkLicenseSha256 binds reused and downloaded license bytes to the exact pinned upstream tag.
# RtkLicenseSha256 将复用与下载的许可证字节绑定到固定的上游标签。
$RtkLicenseSha256 = "4044ade9c21d8b084d3d16a03375cf3b7e166b946a327bb37a3fbbdb53287cfd"

# RtkLicenseFile stores the pinned license as part of the bundled plugin package.
# RtkLicenseFile 把固定版本许可证保存为内建插件包的一部分。
$RtkLicenseFile = Join-Path $RtkPluginDir "LICENSE"

function Ensure-Dir {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -ItemType Directory -Path $Path -Force | Out-Null
    }
}

function Find-LocalArchive {
    param([string]$AssetName)

    $CandidatePaths = @((Join-Path $ThirdParty $AssetName))
    $DirectSubDirs = Get-ChildItem -Path $ThirdParty -Directory -ErrorAction SilentlyContinue
    foreach ($Dir in $DirectSubDirs) {
        $CandidatePaths += Join-Path $Dir.FullName $AssetName
    }

    foreach ($Candidate in $CandidatePaths) {
        if (Test-Path -LiteralPath $Candidate) {
            return $Candidate
        }
    }

    return $null
}

# Test-ExpectedSha256 validates that a downloaded or local archive matches the pinned release checksum.
function Test-ExpectedSha256 {
    param(
        [string]$Path,
        [string]$ExpectedHash,
        [string]$Label
    )

    if ([string]::IsNullOrWhiteSpace($ExpectedHash)) {
        return
    }

    # hashStream opens the exact artifact for a PowerShell-version-independent SHA-256 calculation.
    # hashStream 打开精确制品，以执行不依赖 PowerShell 版本的 SHA-256 计算。
    $hashStream = [System.IO.File]::OpenRead($Path)
    try {
        # sha256 owns the managed hash implementation and is disposed deterministically.
        # sha256 持有托管哈希实现，并会被确定性释放。
        $sha256 = [System.Security.Cryptography.SHA256]::Create()
        try {
            # hashBytes contains the exact digest produced from the downloaded artifact.
            # hashBytes 保存从下载制品计算得到的精确摘要。
            $hashBytes = $sha256.ComputeHash($hashStream)
        }
        finally {
            $sha256.Dispose()
        }
    }
    finally {
        $hashStream.Dispose()
    }
    # actualHash uses the same lowercase hexadecimal representation as the pinned manifest value.
    # actualHash 使用与固定清单值一致的小写十六进制表示。
    $actualHash = [System.BitConverter]::ToString($hashBytes).Replace("-", "").ToLowerInvariant()
    $expectedHashLower = $ExpectedHash.ToLowerInvariant()
    if ($actualHash -ne $expectedHashLower) {
        throw ("Checksum mismatch for {0}. Expected {1}, got {2}" -f $Label, $expectedHashLower, $actualHash)
    }
}

function Get-CurrentArchitectureKey {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString().ToLowerInvariant()
    switch ($arch) {
        "x64" { return "x86_64" }
        "arm64" { return "aarch64" }
        default { throw "Unsupported architecture for dependency bootstrap: $arch" }
    }
}

function Get-ReleaseByTagOrNull {
    param(
        [string]$Repo,
        [string]$TagName
    )

    $apiUrl = "https://api.github.com/repos/$Repo/releases/tags/$TagName"
    try {
        return Invoke-RestMethod -Uri $apiUrl -UseBasicParsing
    } catch {
        $statusCode = $null
        if ($_.Exception.Response -and $_.Exception.Response.StatusCode) {
            try {
                $statusCode = [int]$_.Exception.Response.StatusCode
            } catch {
                $statusCode = $null
            }
        }

        if ($statusCode -eq 404) {
            return $null
        }

        throw
    }
}

# Get-RtkAssetInfo resolves the RTK release asset for the current host platform and architecture.
function Get-RtkAssetInfo {
    $archKey = Get-CurrentArchitectureKey

    if ($script:IsWindowsPlatform) {
        if ($archKey -ne "x86_64") {
            throw "RTK prebuilt host binary currently supports Windows x86_64 only. Current arch: $archKey"
        }
        return @{
            target = "x86_64-pc-windows-msvc"
            plugin_target = "x86_64-pc-windows-msvc"
            archive_ext = ".zip"
            binary_name = "rtk.exe"
            sha256 = "f0ec18963581657173bd6a51f5ba012b093823f844db749fec218581af30a568"
        }
    }

    if ($script:IsLinuxPlatform) {
        if ($archKey -eq "x86_64") {
            return @{
                target = "x86_64-unknown-linux-musl"
                plugin_target = "x86_64-unknown-linux-gnu"
                archive_ext = ".tar.gz"
                binary_name = "rtk"
                sha256 = "34975116da11e09e502501daf758143e0b22ed3a42a10eb67fb693a6270d9e36"
            }
        }

        return @{
            target = "aarch64-unknown-linux-gnu"
            plugin_target = "aarch64-unknown-linux-gnu"
            archive_ext = ".tar.gz"
            binary_name = "rtk"
            sha256 = "cc2b91c064eb670c097c184913c8fbcb1a943d53d7fe505375e96ba0c5b6459f"
        }
    }

    if ($script:IsMacOSPlatform) {
        if ($archKey -eq "x86_64") {
            return @{
                target = "x86_64-apple-darwin"
                plugin_target = "x86_64-apple-darwin"
                archive_ext = ".tar.gz"
                binary_name = "rtk"
                sha256 = "84121316867613e61925c209607f033b2113bb0ce312c267a79d3e3e8f221e49"
            }
        }

        return @{
            target = "aarch64-apple-darwin"
            plugin_target = "aarch64-apple-darwin"
            archive_ext = ".tar.gz"
            binary_name = "rtk"
            sha256 = "f223ca074a0215af002679bc1d34ca92b93e25b3e8ae16aace6e84c06e586802"
        }
    }

    throw "Unsupported platform for RTK bootstrap"
}

# Install-RtkBinary installs the pinned RTK executable into the target-specific plugin bin directory.
# Install-RtkBinary 把固定版本 RTK 可执行文件安装到插件的目标平台专属 bin 目录。
function Install-RtkBinary {
    Ensure-Dir $RtkPluginDir
    Ensure-Dir $RtkBinRoot

    $assetInfo = Get-RtkAssetInfo
    $tagName = $RtkTag
    $assetName = "rtk-$($assetInfo.target)$($assetInfo.archive_ext)"
    # targetBinDir is the exact directory declared for this target in the plugin manifest.
    # targetBinDir 是插件清单为当前目标声明的精确目录。
    $targetBinDir = Join-Path $RtkBinRoot $assetInfo.plugin_target
    Ensure-Dir $targetBinDir
    # markerFile records the pinned artifact version beside its target-specific executable.
    # markerFile 在目标专属可执行文件旁记录固定制品版本。
    $markerFile = Join-Path $targetBinDir ".installed-$tagName"
    $localArchivePath = Find-LocalArchive -AssetName $assetName
    # binaryDest is the real plugin-owned executable path consumed at runtime.
    # binaryDest 是运行时使用的真实插件自有可执行文件路径。
    $binaryDest = Join-Path $targetBinDir $assetInfo.binary_name

    if ((Test-Path -LiteralPath $markerFile) -and (Test-Path -LiteralPath $binaryDest)) {
        Write-Host "==> RTK host binary already installed ($assetName)."
        return
    }

    $asset = $null
    if (-not $localArchivePath) {
        $release = Get-ReleaseByTagOrNull -Repo $RtkRepo -TagName $tagName
        if (-not $release) {
            throw ("RTK tag '{0}' currently has no GitHub Release host binary asset. Please download the artifact '{1}' manually and place it under third_party (or one direct child directory) before rerunning." -f $tagName, $assetName)
        }

        $asset = $release.assets | Where-Object { $_.name -eq $assetName } | Select-Object -First 1
        if (-not $asset) {
            $available = ($release.assets | ForEach-Object { $_.name }) -join ", "
            throw ("RTK asset '{0}' not found in release '{1}'. Available assets: {2}" -f $assetName, $tagName, $available)
        }
    }

    # tempRoot and tempDir use the platform API because Unix does not require a TEMP environment variable.
    # tempRoot 与 tempDir 使用平台接口，因为 Unix 不要求存在 TEMP 环境变量。
    $tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
    $tempDir = Join-Path $tempRoot ('vulcan_rtk_' + [Guid]::NewGuid().ToString('N'))
    Ensure-Dir $tempDir

    try {
        $archivePath = Join-Path $tempDir $assetName
        if ($localArchivePath) {
            Write-Host "==> Using local RTK package: $localArchivePath"
            Copy-Item -LiteralPath $localArchivePath -Destination $archivePath -Force
        } else {
            Write-Host "==> Downloading RTK package: $assetName"
            Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $archivePath -UseBasicParsing
        }

        Test-ExpectedSha256 -Path $archivePath -ExpectedHash $assetInfo.sha256 -Label $assetName

        if ($assetInfo.archive_ext -eq ".zip") {
            Expand-Archive -Path $archivePath -DestinationPath $tempDir -Force
        } else {
            Expand-DependencyTarArchive -ArchivePath $archivePath -DestinationPath $tempDir
        }

        $binarySource = Get-ChildItem -Path $tempDir -Recurse -File -Filter $assetInfo.binary_name -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if (-not $binarySource) {
            throw ("Executable '{0}' not found after extracting {1}" -f $assetInfo.binary_name, $assetName)
        }

        Copy-Item -LiteralPath $binarySource.FullName -Destination $binaryDest -Force

        Get-ChildItem -LiteralPath $targetBinDir -Filter ".installed-*" -File -Force -ErrorAction SilentlyContinue |
            Remove-Item -Force -ErrorAction SilentlyContinue
        New-Item -ItemType File -Path $markerFile -Force | Out-Null
        Write-Host "==> RTK host binary installed successfully."
    } finally {
        # resolvedTempDir must remain an immediate child of the declared temporary root before recursive cleanup.
        # 递归清理前，resolvedTempDir 必须仍为声明临时根目录的直属子目录。
        $resolvedTempDir = [System.IO.Path]::GetFullPath($tempDir)
        if ([System.IO.Path]::GetDirectoryName($resolvedTempDir) -ne $tempRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar)) {
            throw "Refusing temporary cleanup outside the declared temporary root: $resolvedTempDir"
        }
        Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# Install-RtkLicense downloads the pinned RTK license into the real plugin package.
# Install-RtkLicense 把固定版本 RTK 许可证下载到真实插件包中。
function Install-RtkLicense {
    Ensure-Dir $RtkPluginDir
    Ensure-Dir $RtkBinRoot

    # licenseMarker records the exact tag represented by the packaged license.
    # licenseMarker 记录已打包许可证对应的精确标签。
    $licenseMarker = Join-Path $RtkBinRoot ".license-$RtkTag"
    # licenseMatches treats markers as bookkeeping; only the actual bytes establish a reusable installation.
    # licenseMatches 将标记仅视为记账信息；只有实际字节才能证明安装可复用。
    $licenseMatches = (Test-Path -LiteralPath $RtkLicenseFile) -and
        ((Get-FileHash -LiteralPath $RtkLicenseFile -Algorithm SHA256).Hash.ToLowerInvariant() -eq $RtkLicenseSha256)
    if ((Test-Path -LiteralPath $licenseMarker) -and $licenseMatches) {
        Write-Host "==> RTK license already installed ($RtkTag)."
        return
    }

    # A fresh checkout can already contain the exact license; verify it before creating the installation marker.
    # 新检出目录可能已包含精确许可证；先校验内容，再创建安装标记。
    if ($licenseMatches) {
        Get-ChildItem -LiteralPath $RtkBinRoot -Filter ".license-*" -File -Force -ErrorAction SilentlyContinue |
            Remove-Item -Force -ErrorAction SilentlyContinue
        New-Item -ItemType File -Path $licenseMarker -Force | Out-Null
        Write-Host "==> Reusing verified RTK license ($RtkTag)."
        return
    }

    # tempLicenseFile also uses the cross-platform temporary path API.
    # tempLicenseFile 同样使用跨平台临时路径接口。
    $tempLicenseFile = Join-Path ([System.IO.Path]::GetTempPath()) ('rtk_license_' + [Guid]::NewGuid().ToString('N'))
    try {
        Write-Host "==> Downloading RTK license: $RtkTag"
        Invoke-WebRequest -Uri $RtkLicenseUrl -OutFile $tempLicenseFile -UseBasicParsing
        Test-ExpectedSha256 -Path $tempLicenseFile -ExpectedHash $RtkLicenseSha256 -Label "RTK $RtkTag license"
        Copy-Item -LiteralPath $tempLicenseFile -Destination $RtkLicenseFile -Force

        Get-ChildItem -LiteralPath $RtkBinRoot -Filter ".license-*" -File -Force -ErrorAction SilentlyContinue |
            Remove-Item -Force -ErrorAction SilentlyContinue
        New-Item -ItemType File -Path $licenseMarker -Force | Out-Null
        Write-Host "==> RTK license installed successfully."
    } finally {
        Remove-Item -LiteralPath $tempLicenseFile -Force -ErrorAction SilentlyContinue
    }
}

switch ($Kind) {
    "all" {
        Install-RtkBinary
        Install-RtkLicense
    }
    "rtk" {
        Install-RtkBinary
        Install-RtkLicense
    }
}
