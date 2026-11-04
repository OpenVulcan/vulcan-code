# Prune one Cargo profile down to the newest units of each workspace crate, without building.
# 不经构建，将一个 Cargo 模式目录清理到每个工作区 crate 的最新单元。
# ProfileName is debug or release. NoWait skips the prune at once when other Cargo commands hold the profile,
# for the automatic prune after make test and make check; otherwise it waits like the build sweep does.
# ProfileName 为 debug 或 release。NoWait 在其他 Cargo 命令占用模式目录时立即跳过，用于 make test 与 make check
# 之后的自动清理；否则与构建后清理一样等待。
param(
    [ValidateSet('debug', 'release')][string]$ProfileName = 'debug',
    [switch]$NoWait
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'cargo_artifact_cleanup.ps1')
Push-Location (Split-Path -Parent $PSScriptRoot)
try {
    if ($NoWait) { Invoke-CargoProfilePrune -ProfileName $ProfileName -LockTimeout ([TimeSpan]::Zero) }
    else { Invoke-CargoProfilePrune -ProfileName $ProfileName }
}
finally { Pop-Location }
