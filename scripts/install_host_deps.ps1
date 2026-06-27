# install_host_deps.ps1 - Install host-side third-party tool binaries into third_party.
# Developer/build use only. End users do not need to invoke this manually.
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
$VldbControllerRepo = "OpenVulcan/vldb-controller"
$VldbControllerTag = "v0.2.1"

# RtkDir stores the RTK host tool payload and metadata downloaded from its release tag.
$RtkDir = Join-Path $ThirdParty "rtk"

# RtkBinDir stores the extracted RTK executable only.
$RtkBinDir = Join-Path $RtkDir "bin"

# RtkRepo identifies the upstream RTK GitHub repository.
$RtkRepo = "rtk-ai/rtk"

# RtkTag pins the exact RTK release used by this dependency bootstrap.
$RtkTag = "v0.42.4"

# RtkLicenseUrl points at the tag-pinned source license because RTK binary assets do not include it.
$RtkLicenseUrl = "https://raw.githubusercontent.com/rtk-ai/rtk/$RtkTag/LICENSE"

# RtkLicenseFile stores the license for future application-side license aggregation, not packaging.
$RtkLicenseFile = Join-Path $RtkDir "LICENSE"

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

    $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
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

# Get-RtkAssetInfo resolves the RTK release asset for the current host platform and architecture.
function Get-RtkAssetInfo {
    $archKey = Get-CurrentArchitectureKey

    if ($script:IsWindowsPlatform) {
        if ($archKey -ne "x86_64") {
            throw "RTK prebuilt host binary currently supports Windows x86_64 only. Current arch: $archKey"
        }
        return @{
            target = "x86_64-pc-windows-msvc"
            archive_ext = ".zip"
            binary_name = "rtk.exe"
            sha256 = "f0ec18963581657173bd6a51f5ba012b093823f844db749fec218581af30a568"
        }
    }

    if ($script:IsLinuxPlatform) {
        if ($archKey -eq "x86_64") {
            return @{
                target = "x86_64-unknown-linux-musl"
                archive_ext = ".tar.gz"
                binary_name = "rtk"
                sha256 = "34975116da11e09e502501daf758143e0b22ed3a42a10eb67fb693a6270d9e36"
            }
        }

        return @{
            target = "aarch64-unknown-linux-gnu"
            archive_ext = ".tar.gz"
            binary_name = "rtk"
            sha256 = "cc2b91c064eb670c097c184913c8fbcb1a943d53d7fe505375e96ba0c5b6459f"
        }
    }

    if ($script:IsMacOSPlatform) {
        if ($archKey -eq "x86_64") {
            return @{
                target = "x86_64-apple-darwin"
                archive_ext = ".tar.gz"
                binary_name = "rtk"
                sha256 = "84121316867613e61925c209607f033b2113bb0ce312c267a79d3e3e8f221e49"
            }
        }

        return @{
            target = "aarch64-apple-darwin"
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
    } finally {
        Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# Install-RtkBinary installs the pinned RTK executable into third_party/rtk/bin.
function Install-RtkBinary {
    Ensure-Dir $RtkDir
    Ensure-Dir $RtkBinDir

    $assetInfo = Get-RtkAssetInfo
    $tarPath = Get-AvailableTarPath
    $tagName = $RtkTag
    $assetName = "rtk-$($assetInfo.target)$($assetInfo.archive_ext)"
    $markerFile = Join-Path $RtkDir ".installed-$tagName-$($assetInfo.target)"
    $localArchivePath = Find-LocalArchive -AssetName $assetName
    $binaryDest = Join-Path $RtkBinDir $assetInfo.binary_name

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

        Get-ChildItem -Path $RtkDir -Filter ".installed-*" -File -ErrorAction SilentlyContinue |
            Remove-Item -Force -ErrorAction SilentlyContinue
        New-Item -ItemType File -Path $markerFile -Force | Out-Null
        Write-Host "==> RTK host binary installed successfully."
    } finally {
        Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# Install-RtkLicense downloads the RTK license beside the third_party payload for future license aggregation.
function Install-RtkLicense {
    Ensure-Dir $RtkDir

    $licenseMarker = Join-Path $RtkDir ".license-$RtkTag"
    if ((Test-Path -LiteralPath $licenseMarker) -and (Test-Path -LiteralPath $RtkLicenseFile)) {
        Write-Host "==> RTK license already installed ($RtkTag)."
        return
    }

    $tempLicenseFile = Join-Path $env:TEMP ('rtk_license_{0}' -f $PID)
    try {
        Write-Host "==> Downloading RTK license: $RtkTag"
        Invoke-WebRequest -Uri $RtkLicenseUrl -OutFile $tempLicenseFile -UseBasicParsing
        Copy-Item -LiteralPath $tempLicenseFile -Destination $RtkLicenseFile -Force

        Get-ChildItem -Path $RtkDir -Filter ".license-*" -File -ErrorAction SilentlyContinue |
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
