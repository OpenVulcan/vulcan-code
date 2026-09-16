#Requires -Version 7.0
<#
.SYNOPSIS
    Local LuaSkills source-binding build entry for integration testing.
    本地 LuaSkills 源码绑定构建入口，用于联调。

.DESCRIPTION
    Runs one Cargo command with an explicit command-line
    `patch.crates-io.luaskills.path` override so the workspace resolves
    `luaskills` from the local checkout instead of crates.io.

    用命令行的 `patch.crates-io.luaskills.path` 覆盖执行一次 Cargo 命令，
    使工作区从本地检出解析 `luaskills`，而不是 crates.io。

    The personal absolute path is NEVER written into the publishable
    `Cargo.toml`; only `Cargo.lock` source resolution changes, and the changed
    lock is backed up so it can be restored exactly.

    个人绝对路径绝不写入可发布的 `Cargo.toml`；只有 `Cargo.lock` 的来源解析会变化，
    且变化的锁文件会备份，以便精确恢复。

.PARAMETER Cargo
    Cargo command and arguments as one whitespace-separated string, for example
    `check -p vulcan-agent-runtime`. A single string avoids PowerShell parameter
    binding stealing cargo flags such as `-p`.
    以单个空白分隔字符串给出 Cargo 命令与参数，例如 `check -p vulcan-agent-runtime`。
    使用单个字符串可避免 PowerShell 参数绑定抢走 `-p` 这类 cargo 标志。

.PARAMETER UpstreamRepo
    Local upstream checkout used for the git identity report.
    用于 git 身份报告的本地上游检出。

.PARAMETER UpstreamPath
    Path value handed to Cargo; forward slashes keep TOML escaping trivial.
    传给 Cargo 的路径值；使用正斜杠以避免 TOML 转义问题。

.PARAMETER Restore
    Restore `Cargo.lock` from the backup and exit without running Cargo.
    从备份恢复 `Cargo.lock` 并退出，不运行 Cargo。
#>
[CmdletBinding()]
param(
    [string]$Cargo = 'check',

    [string]$UpstreamRepo = 'D:\projects\vulcan-luaskills',
    [string]$UpstreamPath = 'D:/projects/vulcan-luaskills',
    [switch]$Restore
)

$ErrorActionPreference = 'Stop'

$VulcanRoot = Split-Path -Parent $PSScriptRoot
$LockPath = Join-Path $VulcanRoot 'Cargo.lock'
$LockBackup = Join-Path $VulcanRoot 'Cargo.lock.local-luaskills-backup'
$LockMetadata = Join-Path $VulcanRoot 'Cargo.lock.local-luaskills-backup.json'

# Reports the exact Git identity of one local repository.
# 报告一个本地仓库的精确 Git 身份。
function Get-RepoIdentity {
    param([string]$Repo, [string]$Label)

    if (-not (Test-Path $Repo)) {
        Write-Host "  [$Label] path not found: $Repo" -ForegroundColor Red
        return
    }
    $head = (git -C $Repo rev-parse HEAD 2>$null)
    $statusEntries = @(git -C $Repo status --porcelain --untracked-files=all 2>$null)
    $dirty = if ($statusEntries.Count -gt 0) { 'dirty' } else { 'clean' }
    Write-Host "  [$Label] HEAD=$head  worktree=$dirty"
    Write-Host "  [$Label] path=$Repo"
    if ($dirty -eq 'dirty') {
        # trackedDiffPath receives Git's exact binary patch so line-ending conversion cannot alter the digest.
        # trackedDiffPath 接收 Git 的精确二进制补丁，避免行尾转换改变摘要。
        $trackedDiffPath = Join-Path ([System.IO.Path]::GetTempPath()) (
            'vulcan-luaskills-diff-{0}.patch' -f [System.Guid]::NewGuid().ToString('N')
        )
        try {
            & git -C $Repo --no-pager diff --binary --no-ext-diff --output=$trackedDiffPath HEAD -- 2>$null
            if ($LASTEXITCODE -ne 0) {
                throw "failed to fingerprint tracked changes in $Repo"
            }
            $trackedDiffSha256 = Get-FileSha256 -Path $trackedDiffPath
            Write-Host "  [$Label] tracked_diff_sha256=$trackedDiffSha256"
        }
        finally {
            Remove-Item -LiteralPath $trackedDiffPath -Force -ErrorAction SilentlyContinue
        }

        # Untracked files are outside `git diff HEAD`; report each path and digest explicitly.
        # 未跟踪文件不属于 `git diff HEAD`，因此逐个报告路径与摘要。
        $untrackedFiles = @(git -C $Repo ls-files --others --exclude-standard 2>$null)
        Write-Host "  [$Label] untracked_files=$($untrackedFiles.Count)"
        foreach ($relativePath in $untrackedFiles) {
            $untrackedPath = Join-Path $Repo $relativePath
            $untrackedSha256 = Get-FileSha256 -Path $untrackedPath
            Write-Host "  [$Label] untracked=$relativePath sha256=$untrackedSha256"
        }
    }
}

# Returns the uppercase SHA-256 digest of one exact file.
# 返回一个精确文件的大写 SHA-256 摘要。
function Get-FileSha256 {
    param([string]$Path)

    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
}

if ($Restore) {
    if (Test-Path $LockBackup) {
        if (-not (Test-Path -LiteralPath $LockMetadata)) {
            Write-Host 'Cargo.lock backup metadata is missing; refusing an unsafe automatic restore.' -ForegroundColor Red
            Write-Host "  Compare manually: $LockBackup" -ForegroundColor Yellow
            exit 3
        }
        $metadata = Get-Content -LiteralPath $LockMetadata -Raw -Encoding utf8 | ConvertFrom-Json
        $currentHash = Get-FileSha256 -Path $LockPath
        if ($currentHash -eq $metadata.baselineSha256) {
            Remove-Item -LiteralPath $LockBackup -Force
            Remove-Item -LiteralPath $LockMetadata -Force
            Write-Host 'Cargo.lock already matches the recorded baseline; stale backup removed.' -ForegroundColor Green
        }
        elseif ($metadata.localSha256 -and $currentHash -eq $metadata.localSha256) {
            Copy-Item -LiteralPath $LockBackup -Destination $LockPath -Force
            $restoredHash = Get-FileSha256 -Path $LockPath
            if ($restoredHash -ne $metadata.baselineSha256) {
                Write-Host 'Cargo.lock restore did not reproduce the recorded baseline; backup retained.' -ForegroundColor Red
                exit 3
            }
            Remove-Item -LiteralPath $LockBackup -Force
            Remove-Item -LiteralPath $LockMetadata -Force
            Write-Host 'Cargo.lock restored from the verified backup.' -ForegroundColor Green
        }
        else {
            Write-Host 'Cargo.lock changed after the local-source command; refusing to overwrite concurrent work.' -ForegroundColor Red
            Write-Host "  Current SHA-256:  $currentHash" -ForegroundColor Yellow
            Write-Host "  Baseline SHA-256: $($metadata.baselineSha256)" -ForegroundColor Yellow
            Write-Host "  Local SHA-256:    $($metadata.localSha256)" -ForegroundColor Yellow
            exit 3
        }
    }
    elseif (Test-Path -LiteralPath $LockMetadata) {
        Write-Host 'Cargo.lock backup metadata exists without its backup; refusing to discard recovery evidence.' -ForegroundColor Red
        Write-Host "  Inspect manually: $LockMetadata" -ForegroundColor Yellow
        exit 3
    }
    else {
        Write-Host 'No backup present; nothing to restore.' -ForegroundColor Yellow
    }
    exit 0
}

# Both paths name the same checkout and are supplied separately, so each is validated
# before anything is written. UpstreamPath is the one that reaches Cargo.lock, where a
# typo would otherwise surface as a confusing Cargo failure only after the lock had
# already been backed up and the build had started.
# 两个路径指向同一次检出却分别传入，因此在写入任何东西之前逐一校验。UpstreamPath 才是写进
# Cargo.lock 的那个；拼错时否则要等到锁已被备份、构建已开始后，才以一段令人困惑的 Cargo
# 失败浮现。
foreach ($candidate in @(
        @{ Label = 'UpstreamRepo'; Path = $UpstreamRepo },
        @{ Label = 'UpstreamPath'; Path = $UpstreamPath }
    )) {
    if (-not (Test-Path -LiteralPath $candidate.Path)) {
        Write-Host "  [$($candidate.Label)] path not found: $($candidate.Path)" -ForegroundColor Red
        Write-Host '  Refusing to bind LuaSkills to a missing local source.' -ForegroundColor Red
        exit 2
    }
    if (-not (Test-Path -LiteralPath (Join-Path $candidate.Path 'Cargo.toml'))) {
        Write-Host "  [$($candidate.Label)] no Cargo.toml under: $($candidate.Path)" -ForegroundColor Red
        Write-Host '  Refusing to bind LuaSkills to a directory that is not a Rust crate.' -ForegroundColor Red
        exit 2
    }
}

$canonicalUpstreamRepo = (Resolve-Path -LiteralPath $UpstreamRepo).Path.TrimEnd('\', '/')
$canonicalUpstreamPath = (Resolve-Path -LiteralPath $UpstreamPath).Path.TrimEnd('\', '/')
if (-not [string]::Equals(
        $canonicalUpstreamRepo,
        $canonicalUpstreamPath,
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
    Write-Host '  UpstreamRepo and UpstreamPath resolve to different directories.' -ForegroundColor Red
    Write-Host "  UpstreamRepo: $canonicalUpstreamRepo" -ForegroundColor Yellow
    Write-Host "  UpstreamPath: $canonicalUpstreamPath" -ForegroundColor Yellow
    exit 2
}

Write-Host '=== local LuaSkills source binding ===' -ForegroundColor Cyan
Get-RepoIdentity -Repo $VulcanRoot -Label 'vulcan-code'
Get-RepoIdentity -Repo $UpstreamRepo -Label 'luaskills'

$configArg = 'patch.crates-io.luaskills.path="{0}"' -f $UpstreamPath
if ((Test-Path -LiteralPath $LockBackup) -or (Test-Path -LiteralPath $LockMetadata)) {
    Write-Host '  Another local-source operation already owns Cargo.lock; finish or restore it before another run.' -ForegroundColor Red
    exit 3
}
# File.Copy with overwrite disabled atomically claims the same backup used by full Tauri builds.
# 禁止覆盖的 File.Copy 会原子占用完整 Tauri 构建同样使用的备份文件。
[System.IO.File]::Copy($LockPath, $LockBackup, $false)
$lockState = [ordered]@{
    owner = 'focused-cargo'
    baselineSha256 = Get-FileSha256 -Path $LockBackup
    localSha256 = $null
}
$lockState | ConvertTo-Json | Set-Content -LiteralPath $LockMetadata -Encoding utf8NoBOM
Write-Host '  Cargo.lock backed up with a verified baseline digest.' -ForegroundColor DarkGray

# Resolve the local patch before the requested command and freeze its exact lock digest.
# 在请求命令前解析本地 patch，并冻结其精确锁文件摘要。
& cargo metadata --format-version 1 --config $configArg *> $null
$metadataExit = $LASTEXITCODE
$lockState.localSha256 = Get-FileSha256 -Path $LockPath
$lockState | ConvertTo-Json | Set-Content -LiteralPath $LockMetadata -Encoding utf8NoBOM
if ($metadataExit -ne 0) {
    Write-Host '  Cargo failed to prepare the local LuaSkills lock resolution; recovery evidence was retained.' -ForegroundColor Red
    Write-Host '  Run this script with -Restore after inspecting Cargo.lock.' -ForegroundColor Yellow
    exit $metadataExit
}

$cargoArgList = $Cargo -split '\s+' | Where-Object { $_ -ne '' }
Write-Host "  cargo $($cargoArgList -join ' ')" -ForegroundColor DarkGray
Write-Host "  --config $configArg" -ForegroundColor DarkGray
Write-Host ''

# Cargo owns `--config`, so it must precede the first `--` that transfers all
# remaining arguments to a test binary or another Cargo-launched program.
# `--config` 属于 Cargo，因此必须位于首个 `--` 之前；该分隔符会把其余参数交给测试
# 二进制或其它由 Cargo 启动的程序。
$passthroughIndex = [Array]::IndexOf($cargoArgList, '--')
if ($passthroughIndex -ge 0) {
    # Cargo arguments before the passthrough delimiter.
    # 透传分隔符之前的 Cargo 参数。
    $cargoCommandArguments = @($cargoArgList | Select-Object -First $passthroughIndex)
    # Program arguments after the passthrough delimiter.
    # 透传分隔符之后的程序参数。
    $cargoPassthroughArguments = @($cargoArgList | Select-Object -Skip ($passthroughIndex + 1))
    & cargo @cargoCommandArguments --config $configArg -- @cargoPassthroughArguments
}
else {
    & cargo @cargoArgList --config $configArg
}
$cargoExit = $LASTEXITCODE

Write-Host ''
Write-Host '=== Cargo.lock resolution delta (registry -> local path) ===' -ForegroundColor Cyan
$delta = git -C $VulcanRoot diff -- $LockPath
if ($delta) {
    $delta | Select-String -Pattern '^[+-](?![+-])' | Select-Object -First 12 | ForEach-Object { '  ' + $_.Line }
    Write-Host '  (run with -Restore to return to pristine registry resolution)' -ForegroundColor DarkGray
}
else {
    Write-Host '  no lock delta' -ForegroundColor DarkGray
}

Write-Host ''
Write-Host "cargo exit code: $cargoExit"
exit $cargoExit
