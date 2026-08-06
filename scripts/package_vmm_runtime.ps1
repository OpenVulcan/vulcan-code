param(
    [string]$ProjectDir = "",
    [string]$VmmSourceDir = $env:VULCAN_VMM_SOURCE_DIR
)

# Build and transactionally package the sibling VulcanMemoryMesh managed runtime.
# 构建并以事务方式打包同级 VulcanMemoryMesh 托管运行时。

$ErrorActionPreference = "Stop"

# Resolve the owning Vulcan Code repository when callers omit it.
# 调用方省略时解析所属 Vulcan Code 仓库。
if ([string]::IsNullOrWhiteSpace($ProjectDir)) {
    $ProjectDir = Split-Path -Parent $PSScriptRoot
}
$ProjectDir = [System.IO.Path]::GetFullPath($ProjectDir)

# Resolve the default sibling source checkout when no explicit source path is configured.
# 未配置显式源码路径时解析默认的同级源码检出目录。
if ([string]::IsNullOrWhiteSpace($VmmSourceDir)) {
    $VmmSourceDir = Join-Path (Split-Path -Parent $ProjectDir) "VulcanMemoryMesh"
}
$VmmSourceDir = [System.IO.Path]::GetFullPath($VmmSourceDir)

# Assert-PathWithinRoot protects every recursive replacement and cleanup target.
# Assert-PathWithinRoot 保护所有递归替换与清理目标均位于预期根目录内。
function Assert-PathWithinRoot {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Root
    )
    $resolvedPath = [System.IO.Path]::GetFullPath($Path)
    $resolvedRoot = [System.IO.Path]::GetFullPath($Root).TrimEnd([char[]]@('\', '/'))
    $prefix = $resolvedRoot + [System.IO.Path]::DirectorySeparatorChar
    if ($resolvedPath -ne $resolvedRoot -and -not $resolvedPath.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to access a path outside the intended root: $resolvedPath"
    }
    return $resolvedPath
}

# Get-Sha256 returns one lowercase SHA-256 for an exact file.
# Get-Sha256 为精确文件返回小写 SHA-256。
function Get-Sha256 {
    param([Parameter(Mandatory = $true)][string]$Path)
    # Open the exact file through .NET so packaging does not depend on the host PowerShell cmdlet set.
    # 通过 .NET 打开精确文件，避免打包依赖宿主 PowerShell 的 cmdlet 集合。
    $stream = [System.IO.File]::OpenRead($Path)
    # Create one SHA-256 implementation for the exact byte stream.
    # 为精确字节流创建一个 SHA-256 实现。
    $hash = [System.Security.Cryptography.SHA256]::Create()
    try {
        # Convert the digest to the lowercase form shared by the Rust validator and manifest.
        # 将摘要转换为 Rust 校验器与清单共同使用的小写格式。
        return [System.BitConverter]::ToString($hash.ComputeHash($stream)).Replace("-", "").ToLowerInvariant()
    }
    finally {
        $hash.Dispose()
        $stream.Dispose()
    }
}

# Get-ConfigAssetsDigest mirrors the Rust startup validator's path-and-bytes digest algorithm.
# Get-ConfigAssetsDigest 与 Rust 启动校验器使用相同的路径与字节摘要算法。
function Get-ConfigAssetsDigest {
    param([Parameter(Mandatory = $true)][string]$Root)
    $rootPath = [System.IO.Path]::GetFullPath($Root)
    $relativeFiles = [System.Collections.Generic.List[string]]::new()
    $filePaths = @{}
    foreach ($file in @(Get-ChildItem -LiteralPath $rootPath -Recurse -File | Where-Object { $_.Name -ne "vmm-runtime-manifest.json" })) {
        # Derive the relative path from the validated root for .NET Framework compatibility.
        # 从已校验根目录计算相对路径，以兼容缺少 Path.GetRelativePath 的 .NET Framework。
        $relative = $file.FullName.Substring($rootPath.Length).TrimStart([char[]]@('\', '/')).Replace('\', '/')
        $relativeFiles.Add($relative)
        $filePaths[$relative] = $file.FullName
    }
    $relativeFiles.Sort([System.StringComparer]::Ordinal)
    $buffer = [System.IO.MemoryStream]::new()
    try {
        foreach ($relative in $relativeFiles) {
            $relativeBytes = [System.Text.Encoding]::UTF8.GetBytes($relative)
            $buffer.Write($relativeBytes, 0, $relativeBytes.Length)
            $buffer.WriteByte(0)
            $fileBytes = [System.IO.File]::ReadAllBytes($filePaths[$relative])
            $buffer.Write($fileBytes, 0, $fileBytes.Length)
            $buffer.WriteByte(0)
        }
        $hash = [System.Security.Cryptography.SHA256]::Create()
        try {
            # Use the legacy-compatible formatter shared with the binary digest helper.
            # 使用与二进制摘要辅助函数相同的旧运行时兼容格式化方式。
            return [System.BitConverter]::ToString($hash.ComputeHash($buffer.ToArray())).Replace("-", "").ToLowerInvariant()
        }
        finally {
            $hash.Dispose()
        }
    }
    finally {
        $buffer.Dispose()
    }
}

# Read-VersionJSON invokes a binary-only metadata path that must not initialize runtime services.
# Read-VersionJSON 调用只读取二进制元数据且不得初始化运行时服务的路径。
function Read-VersionJSON {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Name
    )
    $text = (& $Path -version-json | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($text)) {
        throw "$Name -version-json failed."
    }
    try {
        return $text | ConvertFrom-Json
    }
    catch {
        throw "$Name -version-json did not return valid JSON: $($_.Exception.Message)"
    }
}

# Assert-TargetUnlocked rejects a running Windows executable before any package target is changed.
# Assert-TargetUnlocked 在修改任何打包目标前拒绝仍被运行进程锁定的 Windows 可执行文件。
function Assert-TargetUnlocked {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return
    }
    try {
        $stream = [System.IO.FileStream]::new($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::None)
        $stream.Dispose()
    }
    catch {
        throw "Package target is locked; stop the owning process before packaging: $Path"
    }
}

# Copy-ManagedConfigAssets copies only the audited managed-runtime asset set into staging.
# Copy-ManagedConfigAssets 仅把已审计的托管运行时资产集合复制到 staging。
function Copy-ManagedConfigAssets {
    param(
        [Parameter(Mandatory = $true)][string]$SourceRoot,
        [Parameter(Mandatory = $true)][string]$DestinationRoot
    )
    New-Item -ItemType Directory -Path $DestinationRoot -Force | Out-Null
    foreach ($fileName in @("base.yaml", "config.yaml")) {
        $source = Join-Path $SourceRoot $fileName
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "VulcanMemoryMesh packaged config file was not found: $source"
        }
        Copy-Item -LiteralPath $source -Destination (Join-Path $DestinationRoot $fileName) -Force
    }
    foreach ($directoryName in @("noise_rules", "pii_rules", "prompts")) {
        $source = Join-Path $SourceRoot $directoryName
        $destination = Assert-PathWithinRoot (Join-Path $DestinationRoot $directoryName) $DestinationRoot
        if (-not (Test-Path -LiteralPath $source -PathType Container)) {
            throw "VulcanMemoryMesh managed asset directory was not found: $source"
        }
        Copy-Item -LiteralPath $source -Destination $destination -Recurse -Force
    }
}

# Restore-PackageBackup restores the previous complete managed package after a replacement failure.
# Restore-PackageBackup 在替换失败后恢复之前的完整托管包。
function Restore-PackageBackup {
    param(
        [Parameter(Mandatory = $true)][string]$OutputBinDir,
        [Parameter(Mandatory = $true)][string]$OutputConfigDir,
        [Parameter(Mandatory = $true)][string]$BackupRoot,
        [Parameter(Mandatory = $true)][string[]]$BinaryNames
    )
    $outputRoot = Split-Path -Parent $OutputBinDir
    Assert-PathWithinRoot $OutputBinDir $outputRoot | Out-Null
    Assert-PathWithinRoot $OutputConfigDir $outputRoot | Out-Null
    if (Test-Path -LiteralPath $OutputConfigDir) {
        Remove-Item -LiteralPath $OutputConfigDir -Recurse -Force
    }
    $backupConfig = Join-Path $BackupRoot "configs"
    if (Test-Path -LiteralPath $backupConfig) {
        Move-Item -LiteralPath $backupConfig -Destination $OutputConfigDir
    }
    foreach ($binaryName in $BinaryNames) {
        $target = Join-Path $OutputBinDir $binaryName
        $backup = Join-Path (Join-Path $BackupRoot "bin") $binaryName
        if (Test-Path -LiteralPath $target) {
            Remove-Item -LiteralPath $target -Force
        }
        if (Test-Path -LiteralPath $backup) {
            Move-Item -LiteralPath $backup -Destination $target
        }
    }
}

# Validate the exact build outputs and assemble a source/build/config manifest in staging.
# 在 staging 中校验精确构建产物并组装源码/构建/配置清单。
function New-StagedManifest {
    param(
        [Parameter(Mandatory = $true)][string]$StageBinDir,
        [Parameter(Mandatory = $true)][string]$StageConfigDir,
        [Parameter(Mandatory = $true)][string]$LocalName,
        [Parameter(Mandatory = $true)][string]$MigrateName
    )
    $localPath = Join-Path $StageBinDir $LocalName
    $migratePath = Join-Path $StageBinDir $MigrateName
    $localVersion = Read-VersionJSON $localPath "vmm-local"
    $migrateVersion = Read-VersionJSON $migratePath "vmm-migrate"
    foreach ($version in @($localVersion, $migrateVersion)) {
        if ($version.build_id -ne "vmm-runtime-v1" -or $version.managed_contract_version -ne 1 -or $version.inference_protocol_version -ne 1) {
            throw "VMM version identity is not contract v1/build vmm-runtime-v1."
        }
        if ([string]::IsNullOrWhiteSpace($version.source_revision) -or $version.source_revision -eq "unknown" -or [string]::IsNullOrWhiteSpace($version.source_state_digest) -or $version.source_state_digest -eq "unknown") {
            throw "VMM version identity is missing injected source revision or source-state digest."
        }
    }
    if ($localVersion.build_id -ne $migrateVersion.build_id -or $localVersion.source_state_digest -ne $migrateVersion.source_state_digest) {
        throw "vmm-local and vmm-migrate do not share one build identity."
    }
    $manifest = [ordered]@{
        manifest_version = 1
        platform = "{0}-{1}" -f $localVersion.goos, $localVersion.goarch
        managed_contract_version = [int]$localVersion.managed_contract_version
        inference_protocol_version = [int]$localVersion.inference_protocol_version
        source_revision = [string]$localVersion.source_revision
        source_state_digest = [string]$localVersion.source_state_digest
        build_id = [string]$localVersion.build_id
        binaries = [ordered]@{
            $LocalName = [ordered]@{ sha256 = Get-Sha256 $localPath }
            $MigrateName = [ordered]@{ sha256 = Get-Sha256 $migratePath }
        }
        config_assets_sha256 = Get-ConfigAssetsDigest $StageConfigDir
    }
    $manifestPath = Join-Path $StageConfigDir "vmm-runtime-manifest.json"
    $json = $manifest | ConvertTo-Json -Depth 10
    [System.IO.File]::WriteAllText($manifestPath, $json + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
    return $manifest
}

# Resolve the exact build entrypoint before invoking another repository.
# 调用另一个仓库前解析精确构建入口。
$VmmBuildScript = Join-Path $VmmSourceDir "make.bat"
if (-not (Test-Path -LiteralPath $VmmBuildScript -PathType Leaf)) {
    throw "VulcanMemoryMesh build entrypoint was not found: $VmmBuildScript"
}

$RunningOnWindows = if (Get-Variable -Name IsWindows -ErrorAction SilentlyContinue) { $IsWindows } else { [Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT }
$LocalName = if ($RunningOnWindows) { "vmm-local.exe" } else { "vmm-local" }
$MigrateName = if ($RunningOnWindows) { "vmm-migrate.exe" } else { "vmm-migrate" }
$OutputBinDir = Assert-PathWithinRoot (Join-Path $ProjectDir "output\bin") $ProjectDir
$OutputConfigDir = Assert-PathWithinRoot (Join-Path $ProjectDir "output\configs") $ProjectDir
$PackageTempRoot = Assert-PathWithinRoot (Join-Path $ProjectDir ".vulcan\temp") $ProjectDir
$PackageId = [Guid]::NewGuid().ToString("N")
$StageRoot = Assert-PathWithinRoot (Join-Path $PackageTempRoot ("vmm-package-stage-" + $PackageId)) $PackageTempRoot
$BackupRoot = Assert-PathWithinRoot (Join-Path $PackageTempRoot ("vmm-package-backup-" + $PackageId)) $PackageTempRoot
$StageBinDir = Join-Path $StageRoot "bin"
$StageConfigDir = Join-Path $StageRoot "configs"
$Committed = $false
$ReplacementStarted = $false

try {
    New-Item -ItemType Directory -Path $StageBinDir -Force | Out-Null
    New-Item -ItemType Directory -Path $StageConfigDir -Force | Out-Null
    New-Item -ItemType Directory -Path $BackupRoot -Force | Out-Null

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

    $VmmSourceBinDir = Join-Path $VmmSourceDir "output\bin"
    foreach ($binaryName in @($LocalName, $MigrateName)) {
        $source = Join-Path $VmmSourceBinDir $binaryName
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "Built VulcanMemoryMesh binary was not found: $source"
        }
        Copy-Item -LiteralPath $source -Destination (Join-Path $StageBinDir $binaryName) -Force
    }
    Copy-ManagedConfigAssets (Join-Path $VmmSourceDir "output\configs") $StageConfigDir
    $manifest = New-StagedManifest $StageBinDir $StageConfigDir $LocalName $MigrateName

    foreach ($binaryName in @($LocalName, $MigrateName)) {
        Assert-TargetUnlocked (Join-Path $OutputBinDir $binaryName)
    }
    if (Test-Path -LiteralPath $OutputConfigDir) {
        Assert-PathWithinRoot $OutputConfigDir (Split-Path -Parent $OutputBinDir) | Out-Null
    }

    New-Item -ItemType Directory -Path (Join-Path $BackupRoot "bin") -Force | Out-Null
    $ReplacementStarted = $true
    if (Test-Path -LiteralPath $OutputConfigDir) {
        Move-Item -LiteralPath $OutputConfigDir -Destination (Join-Path $BackupRoot "configs")
    }
    foreach ($binaryName in @($LocalName, $MigrateName)) {
        $target = Join-Path $OutputBinDir $binaryName
        if (Test-Path -LiteralPath $target) {
            Move-Item -LiteralPath $target -Destination (Join-Path $BackupRoot "bin")
        }
    }
    New-Item -ItemType Directory -Path (Split-Path -Parent $OutputConfigDir) -Force | Out-Null
    Move-Item -LiteralPath $StageConfigDir -Destination $OutputConfigDir
    foreach ($binaryName in @($LocalName, $MigrateName)) {
        Move-Item -LiteralPath (Join-Path $StageBinDir $binaryName) -Destination (Join-Path $OutputBinDir $binaryName)
    }
    $Committed = $true
    Write-Host "==> Managed runtime packaged with build ID $($manifest.build_id) and config digest $($manifest.config_assets_sha256)."
}
catch {
    if (-not $Committed -and $ReplacementStarted -and (Test-Path -LiteralPath $BackupRoot)) {
        try {
            Restore-PackageBackup $OutputBinDir $OutputConfigDir $BackupRoot @($LocalName, $MigrateName)
        }
        catch {
            throw "VMM package replacement failed and rollback also failed: $($_.Exception.Message)"
        }
    }
    throw
}
finally {
    if (Test-Path -LiteralPath $StageRoot) {
        Assert-PathWithinRoot $StageRoot $PackageTempRoot | Out-Null
        Remove-Item -LiteralPath $StageRoot -Recurse -Force
    }
    if (Test-Path -LiteralPath $BackupRoot) {
        Assert-PathWithinRoot $BackupRoot $PackageTempRoot | Out-Null
        Remove-Item -LiteralPath $BackupRoot -Recurse -Force
    }
}
