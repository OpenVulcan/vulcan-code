# Provisions the vendored protoc that the LanceDB dependency closure requires.
# 取得 LanceDB 依赖闭包所需的随仓库 protoc。
#
# Why this exists / 为何需要它:
#
#   `lance-encoding` (pulled in by `lancedb`) invokes `protoc` from its build script, so the workspace cannot
#   build without one. `.cargo/config.toml` therefore points `PROTOC` and `PROTOC_INCLUDE` at a vendored copy
#   under `.cargo/protoc/`, which keeps a developer machine from needing a system-wide protobuf installation.
#   `lance-encoding`（由 `lancedb` 引入）会在其构建脚本中调用 `protoc`，因此没有它工作区无法构建。
#   `.cargo/config.toml` 因此把 `PROTOC` 与 `PROTOC_INCLUDE` 指向 `.cargo/protoc/` 下的随仓库副本，
#   使开发机无需系统级 protobuf 安装。
#
#   The vendored copy is a build input that is neither a source resource nor a runtime asset, so it is not
#   committed: it is ignored by git and provisioned by `make init-system`, which AGENTS.md names as the single
#   ordered bootstrap contract for a fresh environment.
#   该随仓库副本是构建输入，既非源码资源也非运行时资源，因此不提交：它被 git 忽略，并由 `make init-system`
#   取得——AGENTS.md 将其定为一键有序的新环境初始化契约。
#
# Idempotence and verification / 幂等与验证:
#
#   When a working binary of the pinned version is already present with the pinned digest, this script does
#   nothing. Otherwise it downloads the official release archive, verifies the extracted binary against the
#   pinned digest on Windows (and against the pinned version string everywhere), and fails loudly instead of
#   accepting an unexpected artifact. It never deletes a working installation.
#   当钉定版本且摘要匹配的可用二进制已存在时，本脚本不做任何事。否则它下载官方发布包，在 Windows 上按钉定摘要
#   （并在所有平台按钉定版本字符串）验证解出的二进制，并在遇到意外产物时响亮失败而非接受它。它绝不删除可用的安装。
#
# Usage / 用法:
#   pwsh -File scripts/install_protoc.ps1

param(
    [string] $RepositoryRoot
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = Split-Path -Parent $PSScriptRoot
}

# The version this repository builds with, measured from the vendored binary and not chosen from documentation.
# 本仓库构建所用的版本，取自随仓库二进制的实测值，而非从文档中选取。
$PinnedVersion = '31.1'

# Digest of the vendored Windows binary, also measured rather than assumed. It pins the exact artifact the
# workspace was verified against; other platforms are checked by version only, because only this platform's
# artifact has been observed here.
# 随仓库 Windows 二进制的摘要，同样是实测而非假定。它钉定工作区已验证过的确切产物；其他平台只按版本校验，
# 因为此处只观察到了本平台的产物。
$PinnedWindowsSha256 = 'CBD1CA1FD6AFD1BB6DDD1C09C118ECD4C50F928980857F16FE6FC23704EA17E2'

$TargetRoot = Join-Path $RepositoryRoot '.cargo/protoc'

# Resolves the release asset name and binary file name for the current platform.
# 解析当前平台的发布包名与二进制文件名。
function Resolve-ProtocPlatform {
    $BinaryName = 'protoc'
    if ($IsWindows -or $env:OS -eq 'Windows_NT') {
        return @{ Asset = 'win64'; BinaryName = 'protoc.exe' }
    }
    if ($IsMacOS) {
        return @{ Asset = 'osx-universal_binary'; BinaryName = $BinaryName }
    }
    if ($IsLinux) {
        $Architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
        if ($Architecture -eq [System.Runtime.InteropServices.Architecture]::Arm64) {
            return @{ Asset = 'linux-aarch_64'; BinaryName = $BinaryName }
        }
        return @{ Asset = 'linux-x86_64'; BinaryName = $BinaryName }
    }
    throw "Unsupported platform for protoc provisioning. Install protoc $PinnedVersion manually and point PROTOC at it."
}

$Platform = Resolve-ProtocPlatform
$BinaryPath = Join-Path (Join-Path $TargetRoot 'bin') $Platform.BinaryName
$IncludePath = Join-Path $TargetRoot 'include'

# Reports the version string of one binary, or an empty string when it cannot run.
# 返回某二进制的版本字符串；无法运行时返回空字符串。
function Get-ProtocVersion([string] $Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return '' }
    try {
        $Output = & $Path --version 2>&1
    }
    catch {
        return ''
    }
    return ($Output | Out-String).Trim()
}

$ExistingVersion = Get-ProtocVersion $BinaryPath
if ($ExistingVersion -eq "libprotoc $PinnedVersion") {
    $DigestMatches = $true
    if ($Platform.Asset -eq 'win64') {
        $DigestMatches = (Get-FileHash -LiteralPath $BinaryPath -Algorithm SHA256).Hash -eq $PinnedWindowsSha256
    }
    if ($DigestMatches) {
        Write-Host "==> protoc $PinnedVersion already provisioned at $BinaryPath"
        exit 0
    }
    Write-Host "==> protoc $PinnedVersion is present but its digest does not match the pinned artifact; replacing it."
}

$ArchiveName = "protoc-$PinnedVersion-$($Platform.Asset).zip"
$DownloadUrl = "https://github.com/protocolbuffers/protobuf/releases/download/v$PinnedVersion/$ArchiveName"
$TemporaryDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ("vulcan-protoc-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $TemporaryDirectory -Force | Out-Null

try {
    $ArchivePath = Join-Path $TemporaryDirectory $ArchiveName
    Write-Host "==> downloading $DownloadUrl"
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $ArchivePath -UseBasicParsing

    $ExtractPath = Join-Path $TemporaryDirectory 'extracted'
    Expand-Archive -LiteralPath $ArchivePath -DestinationPath $ExtractPath -Force

    $ExtractedBinary = Join-Path (Join-Path $ExtractPath 'bin') $Platform.BinaryName
    $ExtractedVersion = Get-ProtocVersion $ExtractedBinary
    if ($ExtractedVersion -ne "libprotoc $PinnedVersion") {
        throw "Downloaded protoc reported '$ExtractedVersion' instead of 'libprotoc $PinnedVersion'."
    }
    if ($Platform.Asset -eq 'win64') {
        $ExtractedDigest = (Get-FileHash -LiteralPath $ExtractedBinary -Algorithm SHA256).Hash
        if ($ExtractedDigest -ne $PinnedWindowsSha256) {
            throw "Downloaded protoc digest $ExtractedDigest does not match the pinned $PinnedWindowsSha256."
        }
    }

    $ExtractedInclude = Join-Path $ExtractPath 'include'
    if (-not (Test-Path -LiteralPath $ExtractedInclude)) {
        throw "The release archive did not contain an include directory, which PROTOC_INCLUDE requires."
    }

    New-Item -ItemType Directory -Path (Join-Path $TargetRoot 'bin') -Force | Out-Null
    Copy-Item -LiteralPath $ExtractedBinary -Destination $BinaryPath -Force
    if (Test-Path -LiteralPath $IncludePath) { Remove-Item -LiteralPath $IncludePath -Recurse -Force }
    Copy-Item -LiteralPath $ExtractedInclude -Destination $IncludePath -Recurse -Force

    Write-Host "==> protoc $PinnedVersion provisioned at $BinaryPath"
}
catch {
    Write-Error @"
Failed to provision protoc ${PinnedVersion}: $_
The workspace build needs it because lance-encoding invokes protoc. Install protoc $PinnedVersion manually and
either place it at $BinaryPath with its headers under $IncludePath, or point PROTOC and PROTOC_INCLUDE in
.cargo/config.toml at your own installation.
"@
    exit 1
}
finally {
    if (Test-Path -LiteralPath $TemporaryDirectory) {
        Remove-Item -LiteralPath $TemporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
    }
}
