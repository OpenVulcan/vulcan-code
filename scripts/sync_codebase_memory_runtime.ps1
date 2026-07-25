#requires -Version 7.0

# SourceRoot contains the verified dependency installed by make deps cbm.
# SourceRoot 包含由 make deps cbm 安装并校验的依赖。
# DestinationRoot is the runtime distribution consumed by Vulcan Code.
# DestinationRoot 是 Vulcan Code 消费的运行时分发目录。
param(
    [string]$SourceRoot = (Join-Path $PSScriptRoot "..\third_party\codebase_memory_runtime"),
    [string]$DestinationRoot = (Join-Path $PSScriptRoot "..\output\dependencies\runtimes\codebase-memory")
)

$ErrorActionPreference = "Stop"

# Get-FileSha256 returns the lowercase SHA-256 digest for one exact file.
# Get-FileSha256 返回一个精确文件的小写 SHA-256 摘要。
#
# Path is the file whose complete bytes are hashed.
# Path 是需要计算完整字节摘要的文件。
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

# Get-NativePlatformKey returns the platform key consumed by the Rust host manifest.
# Get-NativePlatformKey 返回 Rust 宿主清单消费的平台键。
#
# Returns one of the five supported native distribution keys.
# 返回五个受支持原生分发键之一。
function Get-NativePlatformKey {
    # OsKey matches Rust std::env::consts::OS.
    # OsKey 与 Rust std::env::consts::OS 一致。
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

    # ArchKey matches Rust std::env::consts::ARCH.
    # ArchKey 与 Rust std::env::consts::ARCH 一致。
    $ArchKey = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
        "X64" { "x86_64" }
        "Arm64" { "aarch64" }
        default { throw "Unsupported Codebase Memory runtime architecture." }
    }

    return "$OsKey-$ArchKey"
}

# Read-ValidatedRuntime verifies one source or destination runtime pair and returns its manifest.
# Read-ValidatedRuntime 校验一组源或目标运行时文件并返回其清单。
#
# Root is the exact distribution directory, Label identifies validation errors, and AllowDevelopmentVersion accepts only the legacy dev marker.
# Root 是精确分发目录，Label 用于标识校验错误，AllowDevelopmentVersion 仅允许旧版 dev 标记。
#
# Returns the parsed manifest only after contract, platform, executable, digest, and version validation.
# 仅在契约、平台、可执行文件、摘要和版本校验通过后返回解析后的清单。
function Read-ValidatedRuntime {
    param(
        [string]$Root,
        [string]$Label,
        [switch]$AllowDevelopmentVersion
    )

    # ManifestPath is the exact host-validation manifest.
    # ManifestPath 是精确的宿主校验清单。
    $ManifestPath = Join-Path $Root "manifest.json"
    if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
        throw "$Label manifest is missing: $ManifestPath"
    }

    # Manifest is parsed from the exact file that will be consumed by the host.
    # Manifest 从宿主将要消费的精确文件解析。
    $Manifest = Get-Content -LiteralPath $ManifestPath -Raw -Encoding utf8 | ConvertFrom-Json

    # PropertyNames prevents undeclared fields from drifting away from the strict Rust schema.
    # PropertyNames 防止未声明字段偏离严格 Rust Schema。
    $PropertyNames = @($Manifest.PSObject.Properties.Name | Sort-Object)

    # ExpectedPropertyNames is the byte-stable manifest field contract.
    # ExpectedPropertyNames 是字节稳定的清单字段契约。
    $ExpectedPropertyNames = @("executable", "platform", "protocol", "sha256", "version")
    if (($PropertyNames -join "`n") -cne ($ExpectedPropertyNames -join "`n")) {
        throw "$Label manifest fields do not match the managed runtime contract."
    }
    if ($Manifest.protocol -cne "vulcan.codebase-memory/1") {
        throw "$Label manifest protocol is invalid."
    }
    if ($Manifest.platform -cne (Get-NativePlatformKey)) {
        throw "$Label manifest platform '$($Manifest.platform)' does not match the native host."
    }

    # ExpectedExecutable is the only product filename accepted by the Rust host.
    # ExpectedExecutable 是 Rust 宿主接受的唯一产品文件名。
    $ExpectedExecutable = if ($Manifest.platform.StartsWith("windows-")) {
        "vulcan-codebase-memory-mcp.exe"
    }
    else {
        "vulcan-codebase-memory-mcp"
    }
    if ($Manifest.executable -cne $ExpectedExecutable) {
        throw "$Label manifest executable '$($Manifest.executable)' is invalid."
    }
    # VersionIsValid accepts SemVer for dependencies and optionally the legacy local dev marker for a locked destination.
    # VersionIsValid 对依赖接受 SemVer，并可为被锁定目标额外接受旧版本地 dev 标记。
    $VersionIsValid = $Manifest.version -match '^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$' -or
        ($AllowDevelopmentVersion -and $Manifest.version -ceq "dev")
    if (-not $VersionIsValid) {
        throw "$Label manifest version '$($Manifest.version)' is invalid."
    }
    if ($Manifest.sha256 -cnotmatch '^[0-9a-f]{64}$') {
        throw "$Label manifest SHA-256 is invalid."
    }

    # BinaryPath is resolved only from the validated fixed executable name.
    # BinaryPath 仅从已校验的固定可执行文件名解析。
    $BinaryPath = Join-Path $Root $Manifest.executable
    if (-not (Test-Path -LiteralPath $BinaryPath -PathType Leaf)) {
        throw "$Label runtime executable is missing: $BinaryPath"
    }
    if ((Get-FileSha256 -Path $BinaryPath) -cne $Manifest.sha256) {
        throw "$Label runtime checksum does not match its manifest."
    }

    # VersionOutput proves the copied binary itself matches the manifest version.
    # VersionOutput 证明复制的二进制自身与清单版本一致。
    $VersionOutput = (& $BinaryPath --version 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or
        $VersionOutput -cne "vulcan-codebase-memory-mcp $($Manifest.version)") {
        throw "$Label runtime version output does not match its manifest."
    }

    return $Manifest
}

# Test-FileLocked reports whether an existing destination executable is held without write sharing.
# Test-FileLocked 报告现有目标可执行文件是否在无写共享状态下被占用。
#
# Path is the exact destination executable.
# Path 是精确的目标可执行文件。
#
# Returns true only for an existing file that cannot be opened with exclusive read-write access.
# 仅当现有文件无法以独占读写方式打开时返回 true。
function Test-FileLocked {
    param(
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $false
    }
    try {
        # ProbeStream is closed immediately after the exclusive-access check.
        # ProbeStream 在独占访问检查后立即关闭。
        $ProbeStream = [System.IO.File]::Open(
            $Path,
            [System.IO.FileMode]::Open,
            [System.IO.FileAccess]::ReadWrite,
            [System.IO.FileShare]::None
        )
        $ProbeStream.Dispose()
        return $false
    }
    catch [System.IO.IOException] {
        return $true
    }
}

# ResolvedSourceRoot is the normalized verified dependency directory.
# ResolvedSourceRoot 是规范化后的已校验依赖目录。
$ResolvedSourceRoot = [System.IO.Path]::GetFullPath($SourceRoot)

# ResolvedDestinationRoot is the normalized runtime distribution directory.
# ResolvedDestinationRoot 是规范化后的运行时分发目录。
$ResolvedDestinationRoot = [System.IO.Path]::GetFullPath($DestinationRoot)

# SourceManifest is trusted only after complete local validation.
# SourceManifest 仅在完整本地校验后才被信任。
$SourceManifest = Read-ValidatedRuntime -Root $ResolvedSourceRoot -Label "Source Codebase Memory"

# SourceBinaryPath is derived from the validated fixed executable name.
# SourceBinaryPath 从已校验的固定可执行文件名派生。
$SourceBinaryPath = Join-Path $ResolvedSourceRoot $SourceManifest.executable

# DestinationBinaryPath is the runtime executable path consumed by Vulcan Code.
# DestinationBinaryPath 是 Vulcan Code 消费的运行时可执行文件路径。
$DestinationBinaryPath = Join-Path $ResolvedDestinationRoot $SourceManifest.executable

# DestinationManifestPath is the runtime manifest path consumed before process startup.
# DestinationManifestPath 是进程启动前消费的运行时清单路径。
$DestinationManifestPath = Join-Path $ResolvedDestinationRoot "manifest.json"

if (Test-FileLocked -Path $DestinationBinaryPath) {
    # ExistingManifest proves a locked destination remains internally consistent before it is retained.
    # ExistingManifest 证明被锁定目标在保留前仍保持内部一致。
    $ExistingManifest = Read-ValidatedRuntime `
        -Root $ResolvedDestinationRoot `
        -Label "Locked destination Codebase Memory" `
        -AllowDevelopmentVersion
    Write-Warning (
        "Skipped Codebase Memory runtime synchronization because the destination is in use; " +
        "the existing consistent version '$($ExistingManifest.version)' remains active."
    )
    return
}

New-Item -ItemType Directory -Path $ResolvedDestinationRoot -Force | Out-Null

# TransactionId makes every staging and backup path unique to this synchronization attempt.
# TransactionId 使每个暂存与备份路径在本次同步中保持唯一。
$TransactionId = "{0}-{1}" -f $PID, [System.Guid]::NewGuid().ToString("N")

# StagedBinaryPath receives verified bytes before the destination pair is changed.
# StagedBinaryPath 在目标文件对变更前接收已校验字节。
$StagedBinaryPath = Join-Path $ResolvedDestinationRoot ".$($SourceManifest.executable).$TransactionId.tmp"

# StagedManifestPath receives the verified manifest before the destination pair is changed.
# StagedManifestPath 在目标文件对变更前接收已校验清单。
$StagedManifestPath = Join-Path $ResolvedDestinationRoot ".manifest.json.$TransactionId.tmp"

# BackupBinaryPath temporarily owns the previous executable for rollback.
# BackupBinaryPath 临时持有旧可执行文件以供回滚。
$BackupBinaryPath = Join-Path $ResolvedDestinationRoot ".$($SourceManifest.executable).$TransactionId.previous"

# BackupManifestPath temporarily owns the previous manifest for rollback.
# BackupManifestPath 临时持有旧清单以供回滚。
$BackupManifestPath = Join-Path $ResolvedDestinationRoot ".manifest.json.$TransactionId.previous"

# PreviousBinaryMoved records exact rollback ownership.
# PreviousBinaryMoved 记录精确的回滚所有权。
$PreviousBinaryMoved = $false

# PreviousManifestMoved records exact rollback ownership.
# PreviousManifestMoved 记录精确的回滚所有权。
$PreviousManifestMoved = $false

# NewBinaryCommitted records whether the staged executable reached the live path.
# NewBinaryCommitted 记录暂存可执行文件是否已进入生效路径。
$NewBinaryCommitted = $false

# NewManifestCommitted records whether the staged manifest reached the live path.
# NewManifestCommitted 记录暂存清单是否已进入生效路径。
$NewManifestCommitted = $false

try {
    Copy-Item -LiteralPath $SourceBinaryPath -Destination $StagedBinaryPath -Force
    Copy-Item -LiteralPath (Join-Path $ResolvedSourceRoot "manifest.json") -Destination $StagedManifestPath -Force
    if ((Get-FileSha256 -Path $StagedBinaryPath) -cne $SourceManifest.sha256) {
        throw "Staged Codebase Memory runtime checksum changed during synchronization."
    }

    if (Test-Path -LiteralPath $DestinationBinaryPath -PathType Leaf) {
        Move-Item -LiteralPath $DestinationBinaryPath -Destination $BackupBinaryPath
        $PreviousBinaryMoved = $true
    }
    if (Test-Path -LiteralPath $DestinationManifestPath -PathType Leaf) {
        Move-Item -LiteralPath $DestinationManifestPath -Destination $BackupManifestPath
        $PreviousManifestMoved = $true
    }

    Move-Item -LiteralPath $StagedBinaryPath -Destination $DestinationBinaryPath
    $NewBinaryCommitted = $true
    Move-Item -LiteralPath $StagedManifestPath -Destination $DestinationManifestPath
    $NewManifestCommitted = $true

    # DestinationManifest proves the committed pair is valid before backups are discarded.
    # DestinationManifest 证明已提交文件对有效后才丢弃备份。
    $DestinationManifest = Read-ValidatedRuntime -Root $ResolvedDestinationRoot -Label "Destination Codebase Memory"

    if ($PreviousBinaryMoved) {
        Remove-Item -LiteralPath $BackupBinaryPath -Force
    }
    if ($PreviousManifestMoved) {
        Remove-Item -LiteralPath $BackupManifestPath -Force
    }

    Write-Host (
        "==> Codebase Memory runtime synchronized: version {0}, platform {1}, sha256 {2}" -f
        $DestinationManifest.version,
        $DestinationManifest.platform,
        $DestinationManifest.sha256
    )
}
catch {
    # SynchronizationError is retained while rollback restores the previous consistent pair.
    # SynchronizationError 在回滚恢复旧的一致文件对期间保留原始错误。
    $SynchronizationError = $_
    if ($NewManifestCommitted -and (Test-Path -LiteralPath $DestinationManifestPath)) {
        Remove-Item -LiteralPath $DestinationManifestPath -Force -ErrorAction SilentlyContinue
    }
    if ($NewBinaryCommitted -and (Test-Path -LiteralPath $DestinationBinaryPath)) {
        Remove-Item -LiteralPath $DestinationBinaryPath -Force -ErrorAction SilentlyContinue
    }
    if ($PreviousBinaryMoved -and (Test-Path -LiteralPath $BackupBinaryPath)) {
        Move-Item -LiteralPath $BackupBinaryPath -Destination $DestinationBinaryPath -Force
    }
    if ($PreviousManifestMoved -and (Test-Path -LiteralPath $BackupManifestPath)) {
        Move-Item -LiteralPath $BackupManifestPath -Destination $DestinationManifestPath -Force
    }
    throw $SynchronizationError
}
finally {
    # TemporaryPath iterates only the four exact transaction-owned paths declared above.
    # TemporaryPath 仅遍历上方声明的四个精确事务所有路径。
    foreach ($TemporaryPath in @(
            $StagedBinaryPath,
            $StagedManifestPath,
            $BackupBinaryPath,
            $BackupManifestPath
        )) {
        if (Test-Path -LiteralPath $TemporaryPath) {
            Remove-Item -LiteralPath $TemporaryPath -Force -ErrorAction SilentlyContinue
        }
    }
}
