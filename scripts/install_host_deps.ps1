# install_host_deps.ps1 - Install host dependencies and bundled plugin artifacts.
# install_host_deps.ps1 - 安装宿主依赖与内建插件制品。
# Developer/build use only. End users do not need to invoke this manually.
# 仅供开发与构建使用，最终用户无需手动调用。
# Usage: powershell -ExecutionPolicy Bypass -File scripts/install_host_deps.ps1

param(
    [ValidateSet("all", "vldb", "rtk")]
    [string]$Kind = "all"
)

$ErrorActionPreference = "Stop"
$ProjectDir = Split-Path $PSScriptRoot -Parent
Set-Location $ProjectDir

$script:IsWindowsPlatform = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)
$script:IsMacOSPlatform   = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)
$script:IsLinuxPlatform   = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)

$ThirdParty = Join-Path $ProjectDir "third_party"
$VldbControllerDir = Join-Path $ThirdParty "vldb_controller"
$VldbControllerBinDir = Join-Path $VldbControllerDir "bin"
# VldbControllerOutputBinDir is the runtime directory preferred by packaged and debug launches.
# VldbControllerOutputBinDir 是打包运行与调试启动优先使用的运行时目录。
$VldbControllerOutputBinDir = Join-Path $ProjectDir "output\bin"
$VldbControllerRepo = "OpenVulcan/vldb-controller"
$VldbControllerTag = "v0.2.4"

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

function Get-AvailableTarPath {
    $systemTar = "$env:SystemRoot\System32\tar.exe"
    if (Test-Path -LiteralPath $systemTar) { return $systemTar }
    $tarCommand = Get-Command "tar.exe" -ErrorAction SilentlyContinue
    if ($tarCommand) { return $tarCommand.Source }
    throw "tar.exe is required to extract host dependency archives"
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

function Get-VldbControllerAssetInfo {
    $archKey = Get-CurrentArchitectureKey

    if ($script:IsWindowsPlatform) {
        if ($archKey -ne "x86_64") {
            throw "vldb-controller prebuilt host binary currently supports Windows x86_64 only. Current arch: $archKey"
        }
        return @{
            target = "x86_64-pc-windows-msvc"
            archive_ext = ".zip"
            binary_name = "vldb-controller.exe"
        }
    }

    if ($script:IsLinuxPlatform) {
        $target = "x86_64-unknown-linux-gnu"
        if ($archKey -eq "aarch64") {
            $target = "aarch64-unknown-linux-gnu"
        }
        return @{
            target = $target
            archive_ext = ".tar.gz"
            binary_name = "vldb-controller"
        }
    }

    if ($script:IsMacOSPlatform) {
        $target = "x86_64-apple-darwin"
        if ($archKey -eq "aarch64") {
            $target = "aarch64-apple-darwin"
        }
        return @{
            target = $target
            archive_ext = ".tar.gz"
            binary_name = "vldb-controller"
        }
    }

    throw "Unsupported platform for vldb-controller bootstrap"
}

# Sync-VldbControllerRuntimeBinary copies the installed controller into the runtime-preferred output directory.
# Sync-VldbControllerRuntimeBinary 将已安装控制器复制到运行时优先使用的 output 目录。
# SourcePath identifies the exact verified executable installed under third_party; BinaryName is its platform filename.
# SourcePath 指向 third_party 下已安装的准确可执行文件；BinaryName 是对应平台文件名。
# This function returns no value and throws when the runtime copy cannot be completed.
# 此函数不返回值，并在运行时副本无法完成同步时抛出错误。
function Sync-VldbControllerRuntimeBinary {
    param(
        [string]$SourcePath,
        [string]$BinaryName
    )

    Ensure-Dir $VldbControllerOutputBinDir
    # runtimeBinaryPath is the exact executable path resolved first by Vulcan Code launches.
    # runtimeBinaryPath 是 Vulcan Code 启动时优先解析的准确可执行文件路径。
    $runtimeBinaryPath = Join-Path $VldbControllerOutputBinDir $BinaryName
    Copy-Item -LiteralPath $SourcePath -Destination $runtimeBinaryPath -Force
    Write-Host "==> vldb-controller runtime binary synchronized: $runtimeBinaryPath"
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

function Install-VldbControllerBinary {
    Ensure-Dir $VldbControllerDir
    Ensure-Dir $VldbControllerBinDir

    $assetInfo = Get-VldbControllerAssetInfo
    $tarPath = Get-AvailableTarPath
    $release = $null
    $tagName = $null
    $assetName = $null
    $markerFile = $null
    $localArchivePath = $null

    $localPattern = "vldb-controller-v*-$($assetInfo.target)$($assetInfo.archive_ext)"
    $localArchive = Get-ChildItem -Path $ThirdParty -File -Filter $localPattern -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending |
        Select-Object -First 1
    if (-not $localArchive) {
        $localArchive = Get-ChildItem -Path $ThirdParty -Directory -ErrorAction SilentlyContinue |
            ForEach-Object { Get-ChildItem -Path $_.FullName -File -Filter $localPattern -ErrorAction SilentlyContinue } |
            Sort-Object Name -Descending |
            Select-Object -First 1
    }

    if ($localArchive) {
        $assetName = $localArchive.Name
        if ($assetName -match '^vldb-controller-(v.+)-[^-]+(?:-[^-]+){2,3}(\.zip|\.tar\.gz)$') {
            $tagName = $Matches[1]
        } else {
            throw "Unable to parse vldb-controller tag from local archive name: $assetName"
        }
        $markerFile = Join-Path $VldbControllerDir ".installed-$tagName-$($assetInfo.target)"
        $localArchivePath = $localArchive.FullName
    } else {
        $tagName = $VldbControllerTag
        $assetName = "vldb-controller-$tagName-$($assetInfo.target)$($assetInfo.archive_ext)"
        $markerFile = Join-Path $VldbControllerDir ".installed-$tagName-$($assetInfo.target)"
        $localArchivePath = Find-LocalArchive -AssetName $assetName
    }

    $binaryDest = Join-Path $VldbControllerBinDir $assetInfo.binary_name

    if ((Test-Path -LiteralPath $markerFile) -and (Test-Path -LiteralPath $binaryDest)) {
        Write-Host "==> vldb-controller host binary already installed ($assetName)."
        Sync-VldbControllerRuntimeBinary -SourcePath $binaryDest -BinaryName $assetInfo.binary_name
        return
    }

    if (-not $localArchivePath) {
        $release = Get-ReleaseByTagOrNull -Repo $VldbControllerRepo -TagName $tagName
        if (-not $release) {
            throw ("vldb-controller tag '{0}' currently has no GitHub Release host binary asset. Please download the artifact '{1}' manually and place it under third_party (or one direct child directory) before rerunning. Existing local binaries are not reused for a different target tag because their version cannot be proven from this install path." -f $tagName, $assetName)
        }

        $asset = $release.assets | Where-Object { $_.name -eq $assetName } | Select-Object -First 1
        if (-not $asset) {
            $available = ($release.assets | ForEach-Object { $_.name }) -join ", "
            throw ("vldb-controller asset '{0}' not found in release '{1}'. Available assets: {2}" -f $assetName, $tagName, $available)
        }
    }

    $tempDir = Join-Path $env:TEMP ('vldb_controller_{0}' -f $PID)
    if (Test-Path -LiteralPath $tempDir) {
        Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    Ensure-Dir $tempDir

    try {
        $archivePath = Join-Path $tempDir $assetName
        if ($localArchivePath) {
            Write-Host "==> Using local vldb-controller package: $localArchivePath"
            Copy-Item -LiteralPath $localArchivePath -Destination $archivePath -Force
        } else {
            Write-Host "==> Downloading vldb-controller package: $assetName"
            Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $archivePath -UseBasicParsing
        }

        if ($assetInfo.archive_ext -eq ".zip") {
            Expand-Archive -Path $archivePath -DestinationPath $tempDir -Force
        } else {
            & $tarPath -xzf $archivePath -C $tempDir
        }

        $binarySource = Get-ChildItem -Path $tempDir -Recurse -File -Filter $assetInfo.binary_name -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if (-not $binarySource) {
            throw ("Executable '{0}' not found after extracting {1}" -f $assetInfo.binary_name, $assetName)
        }

        Copy-Item -LiteralPath $binarySource.FullName -Destination $binaryDest -Force

        Get-ChildItem -Path $VldbControllerDir -Filter ".installed-*" -File -ErrorAction SilentlyContinue |
            Remove-Item -Force -ErrorAction SilentlyContinue
        New-Item -ItemType File -Path $markerFile -Force | Out-Null
        Write-Host "==> vldb-controller host binary installed successfully."
        Sync-VldbControllerRuntimeBinary -SourcePath $binaryDest -BinaryName $assetInfo.binary_name
    } finally {
        Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# Install-RtkBinary installs the pinned RTK executable into the target-specific plugin bin directory.
# Install-RtkBinary 把固定版本 RTK 可执行文件安装到插件的目标平台专属 bin 目录。
function Install-RtkBinary {
    Ensure-Dir $RtkPluginDir
    Ensure-Dir $RtkBinRoot

    $assetInfo = Get-RtkAssetInfo
    $tarPath = Get-AvailableTarPath
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

    $tempDir = Join-Path $env:TEMP ('rtk_{0}' -f $PID)
    if (Test-Path -LiteralPath $tempDir) {
        Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
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
            & $tarPath -xzf $archivePath -C $tempDir
        }

        $binarySource = Get-ChildItem -Path $tempDir -Recurse -File -Filter $assetInfo.binary_name -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if (-not $binarySource) {
            throw ("Executable '{0}' not found after extracting {1}" -f $assetInfo.binary_name, $assetName)
        }

        Copy-Item -LiteralPath $binarySource.FullName -Destination $binaryDest -Force

        Get-ChildItem -Path $targetBinDir -Filter ".installed-*" -File -ErrorAction SilentlyContinue |
            Remove-Item -Force -ErrorAction SilentlyContinue
        New-Item -ItemType File -Path $markerFile -Force | Out-Null
        Write-Host "==> RTK host binary installed successfully."
    } finally {
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
    if ((Test-Path -LiteralPath $licenseMarker) -and (Test-Path -LiteralPath $RtkLicenseFile)) {
        Write-Host "==> RTK license already installed ($RtkTag)."
        return
    }

    $tempLicenseFile = Join-Path $env:TEMP ('rtk_license_{0}' -f $PID)
    try {
        Write-Host "==> Downloading RTK license: $RtkTag"
        Invoke-WebRequest -Uri $RtkLicenseUrl -OutFile $tempLicenseFile -UseBasicParsing
        Copy-Item -LiteralPath $tempLicenseFile -Destination $RtkLicenseFile -Force

        Get-ChildItem -Path $RtkBinRoot -Filter ".license-*" -File -ErrorAction SilentlyContinue |
            Remove-Item -Force -ErrorAction SilentlyContinue
        New-Item -ItemType File -Path $licenseMarker -Force | Out-Null
        Write-Host "==> RTK license installed successfully."
    } finally {
        Remove-Item -LiteralPath $tempLicenseFile -Force -ErrorAction SilentlyContinue
    }
}

switch ($Kind) {
    "all" {
        Install-VldbControllerBinary
        Install-RtkBinary
        Install-RtkLicense
    }
    "vldb" {
        Install-VldbControllerBinary
    }
    "rtk" {
        Install-RtkBinary
        Install-RtkLicense
    }
}
