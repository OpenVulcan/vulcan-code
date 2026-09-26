# Install the reviewed protoc inputs; RepositoryRoot selects an isolated checkout and Offline prohibits downloads.
# 安装已评审的 protoc 输入；RepositoryRoot 选择隔离检出目录，Offline 禁止下载。
param(
    # RepositoryRoot defaults to this checkout and is forwarded without shell interpolation.
    # RepositoryRoot 默认使用本检出目录，转发时不进行 shell 插值。
    [string]$RepositoryRoot = (Split-Path $PSScriptRoot -Parent),
    # Offline requires every archive and license to exist in the verified content cache.
    # Offline 要求所有归档和许可证均存在于已校验的内容缓存中。
    [switch]$Offline
)
$ErrorActionPreference = 'Stop'
# InstallerArguments preserves exact argument boundaries for paths containing spaces.
# InstallerArguments 为含空格路径保留精确参数边界。
$InstallerArguments = @((Join-Path $PSScriptRoot 'install-locked-tools.mjs'), 'protoc', '--repository-root', $RepositoryRoot)
if ($Offline) { $InstallerArguments += '--offline' }
& node @InstallerArguments
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
