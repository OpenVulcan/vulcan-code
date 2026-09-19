# Exercise cleanup with real isolated Cargo builds and explicit file-preservation assertions.
# 使用真实隔离 Cargo 构建和显式文件保留断言验证清理。
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'cargo_artifact_cleanup.ps1')

# ScriptName enumerates the changed executable scripts for syntax validation.
# ScriptName 枚举发生变化的可执行脚本，用于语法校验。
foreach ($ScriptName in @('build_tauri.ps1', 'cargo_artifact_cleanup.ps1', 'test_cargo_artifact_cleanup.ps1')) {
    # ParseTokens and ParseErrors receive the PowerShell parser's structured results.
    # ParseTokens 和 ParseErrors 接收 PowerShell 解析器的结构化结果。
    $ParseTokens = $null
    $ParseErrors = $null
    [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $ScriptName), [ref]$ParseTokens, [ref]$ParseErrors) | Out-Null
    if ($ParseErrors.Count -ne 0) { throw ($ParseErrors | Out-String) }
}

# Assert a required condition; Condition is the result and Message describes failure. Returns nothing.
# 断言必要条件；Condition 是结果，Message 描述失败原因。无返回值。
function Assert-CleanupCondition {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

# FixtureRoot is a uniquely owned temporary project; no production target directory is touched.
# FixtureRoot 是独占的临时工程；不接触生产 target 目录。
$FixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ('vulcan-cleanup-test-' + [guid]::NewGuid().ToString('N'))
# SavedEnvironment restores Cargo overrides after the test, even on failure.
# SavedEnvironment 在测试结束后恢复 Cargo 覆盖，包括失败路径。
$SavedEnvironment = @{}
foreach ($VariableName in @('CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS')) {
    $SavedEnvironment[$VariableName] = [Environment]::GetEnvironmentVariable($VariableName)
    Remove-Item -LiteralPath "Env:$VariableName" -ErrorAction SilentlyContinue
}
New-Item -ItemType Directory -Path (Join-Path $FixtureRoot 'src') -Force | Out-Null
Push-Location $FixtureRoot
try {
    @'
[package]
name = "vulcan-code-service"
version = "0.1.0"
edition = "2021"

[dependencies]
cleanup-dependency = { path = "dependency" }

[workspace]
'@ | Set-Content -LiteralPath 'Cargo.toml'
    New-Item -ItemType Directory -Path 'dependency/src' -Force | Out-Null
    @'
[package]
name = "cleanup-dependency"
version = "0.1.0"
edition = "2021"
'@ | Set-Content -LiteralPath 'dependency/Cargo.toml'
    @'
// Return a dependency fixture value; takes no parameters and returns one.
// 返回依赖测试值；无参数，返回一。
pub fn value() -> u8 { 1 }
'@ | Set-Content -LiteralPath 'dependency/src/lib.rs'
    @'
// Return a stable fixture value without external dependencies.
// 返回稳定的测试值，不使用外部依赖。
pub fn value() -> u8 { cleanup_dependency::value() }
'@ | Set-Content -LiteralPath 'src/lib.rs'
    @'
// Exercise the fixture library; takes no parameters and returns unit.
// 调用测试库；无参数，返回单元类型。
fn main() { println!("{}", vulcan_code_service::value()); }'@ | Set-Content -LiteralPath 'src/main.rs'
    $env:CARGO_TARGET_DIR = Join-Path $FixtureRoot 'custom-target'
    # FirstBuild establishes a real baseline including a hashed rlib.
    # FirstBuild 建立真实基线，包括带哈希的 rlib。
    $FirstBuild = Invoke-TrackedCargoBuild -BuildArguments @('build', '-j', '4')
    Assert-CleanupCondition ($FirstBuild.ExitCode -eq 0) 'Initial debug build failed.'
    # OldLibrary is reported by Cargo rather than inferred from directory contents.
    # OldLibrary 由 Cargo 报告，而非从目录内容推测。
    $OldLibrary = @($FirstBuild.Artifacts.filenames | Where-Object { $_ -match '[/\\]deps[/\\].+\.rlib$' })[0]
    Assert-CleanupCondition (Test-Path -LiteralPath $OldLibrary) 'Initial library missing.'
    # UntrackedFile proves that old files without a baseline are not guessed away.
    # UntrackedFile 验证没有基线的旧文件不会被猜测删除。
    $UntrackedFile = Join-Path $env:CARGO_TARGET_DIR 'debug/deps/untracked-old.rlib'
    Set-Content -LiteralPath $UntrackedFile 'preserve'
    # FreshBuild must keep the original dependency artifact despite its older write time.
    # FreshBuild 必须保留原依赖制品，即使其写入时间较旧。
    $FreshBuild = Invoke-TrackedCargoBuild -BuildArguments @('build', '-j', '4')
    Assert-CleanupCondition ($FreshBuild.ExitCode -eq 0 -and (Test-Path -LiteralPath $OldLibrary)) 'Fresh build removed a live dependency.'
    Assert-CleanupCondition (@($FreshBuild.Artifacts | Where-Object { -not $_.fresh }).Count -eq 0) 'Expected a fully fresh Cargo build.'
    $env:RUSTFLAGS = '--cfg cleanup_fixture_second'
    # SecondBuild changes Cargo's artifact hash and must retire the tracked old library.
    # SecondBuild 改变 Cargo 制品哈希，必须清理已跟踪的旧库。
    $SecondBuild = Invoke-TrackedCargoBuild -BuildArguments @('build', '-j', '4')
    Assert-CleanupCondition ($SecondBuild.ExitCode -eq 0 -and -not (Test-Path -LiteralPath $OldLibrary)) 'Retired debug library was not deleted.'
    Assert-CleanupCondition (Test-Path -LiteralPath $UntrackedFile) 'Untracked file was deleted.'
    # DebugLibrary must survive independent release builds and release cleanup.
    # DebugLibrary 必须在独立 release 构建和清理后保留。
    $DebugLibrary = @($SecondBuild.Artifacts.filenames | Where-Object { $_ -match '[/\\]deps[/\\].+\.rlib$' })[0]
    # ReleaseBuild establishes a separate optimized baseline before changing its hash.
    # ReleaseBuild 在改变哈希之前建立独立的优化构建基线。
    $ReleaseBuild = Invoke-TrackedCargoBuild -BuildArguments @('build', '--release', '-j', '4')
    # OldReleaseLibrary identifies the optimized artifact expected to retire.
    # OldReleaseLibrary 标识预期退役的优化制品。
    $OldReleaseLibrary = @($ReleaseBuild.Artifacts.filenames | Where-Object { $_ -match '[/\\]deps[/\\].+\.rlib$' })[0]
    $env:RUSTFLAGS = '--cfg cleanup_fixture_third'
    # NextReleaseBuild replaces the old optimized dependency without touching debug.
    # NextReleaseBuild 替换旧的优化依赖，不接触 debug。
    $NextReleaseBuild = Invoke-TrackedCargoBuild -BuildArguments @('build', '--release', '-j', '4')
    Assert-CleanupCondition ($NextReleaseBuild.ExitCode -eq 0 -and -not (Test-Path -LiteralPath $OldReleaseLibrary)) 'Retired release library was not deleted.'
    Assert-CleanupCondition (Test-Path -LiteralPath $DebugLibrary) 'Release cleanup removed a debug artifact.'
    # BaselineBefore records persistent state so failed builds cannot silently advance it.
    # BaselineBefore 记录持久化状态，确保失败构建不会悄悄推进基线。
    $BaselineBefore = @(Get-ChildItem (Join-Path $env:CARGO_TARGET_DIR '.vulcan-make-artifacts') -Filter '*.json' | Get-FileHash).Hash -join ','
    Set-Content -LiteralPath 'src/lib.rs' 'compile_error!("intentional cleanup test failure");'
    # FailedBuild must propagate Cargo's failure without authorizing cleanup.
    # FailedBuild 必须传递 Cargo 失败，不能授权清理。
    $FailedBuild = Invoke-TrackedCargoBuild -BuildArguments @('build', '-j', '4')
    # BaselineAfter must match the persisted state captured before the failed build.
    # BaselineAfter 必须匹配失败构建前捕获的持久化状态。
    $BaselineAfter = @(Get-ChildItem (Join-Path $env:CARGO_TARGET_DIR '.vulcan-make-artifacts') -Filter '*.json' | Get-FileHash).Hash -join ','
    Assert-CleanupCondition ($FailedBuild.ExitCode -ne 0 -and $BaselineBefore -eq $BaselineAfter) 'Failed build changed cleanup baselines.'
    Assert-CleanupCondition (Test-Path -LiteralPath $DebugLibrary) 'Failed build deleted the previous good artifact.'
    # GuardRoot isolates synthetic file-identity and path-boundary cases from the real build baselines.
    # GuardRoot 将模拟文件身份和路径边界用例与真实构建基线隔离。
    $GuardRoot = Join-Path $FixtureRoot 'guards'
    $GuardProfile = Join-Path $GuardRoot 'debug'
    $GuardDeps = Join-Path $GuardProfile 'deps'
    New-Item -ItemType Directory -Path $GuardDeps -Force | Out-Null
    # ChangedFile, LockedFile and OutsideFile model externally modified, busy and out-of-scope artifacts.
    # ChangedFile、LockedFile 和 OutsideFile 模拟外部修改、占用及范围外制品。
    $ChangedFile = Join-Path $GuardDeps 'changed.rlib'
    $LockedFile = Join-Path $GuardDeps 'locked.rlib'
    $OutsideFile = Join-Path $FixtureRoot 'outside.rlib'
    foreach ($Filename in @($ChangedFile, $LockedFile, $OutsideFile)) { Set-Content -LiteralPath $Filename 'original' }
    Update-CargoArtifactBaseline -ProfileDirectory $GuardProfile -TargetDirectory $GuardRoot -Artifacts @(@{ filenames = @($ChangedFile, $LockedFile, $OutsideFile) })
    Set-Content -LiteralPath $ChangedFile 'modified by another build, different size'
    # LockedStream prevents removal on Windows and exercises candidate persistence for retry.
    # LockedStream 在 Windows 上阻止删除，验证候选持久化重试。
    $LockedStream = [IO.File]::Open($LockedFile, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        Update-CargoArtifactBaseline -ProfileDirectory $GuardProfile -TargetDirectory $GuardRoot -Artifacts @()
        Assert-CleanupCondition (Test-Path -LiteralPath $ChangedFile) 'Externally modified artifact was deleted.'
        Assert-CleanupCondition (Test-Path -LiteralPath $OutsideFile) 'Out-of-scope artifact was deleted.'
        if ([IO.Path]::DirectorySeparatorChar -eq '\') { Assert-CleanupCondition (Test-Path -LiteralPath $LockedFile) 'Locked file was not preserved.' }
    }
    finally { $LockedStream.Dispose() }
    Update-CargoArtifactBaseline -ProfileDirectory $GuardProfile -TargetDirectory $GuardRoot -Artifacts @()
    Assert-CleanupCondition (-not (Test-Path -LiteralPath $LockedFile)) 'Unlocked retired artifact was not retried.'
    Write-Host 'PASS: syntax, real debug/release retirement, fresh reuse, profile isolation, untracked preservation, failed-build preservation, modified/outside files, locked-file retry.'
}
finally {
    Pop-Location
    foreach ($VariableName in $SavedEnvironment.Keys) {
        if ($null -eq $SavedEnvironment[$VariableName]) { Remove-Item -LiteralPath "Env:$VariableName" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($VariableName, $SavedEnvironment[$VariableName]) }
    }
    # Validate the resolved owned fixture path before recursive removal.
    # 递归删除前验证已解析的独占测试目录路径。
    $ResolvedFixture = [IO.Path]::GetFullPath($FixtureRoot)
    $TemporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if (-not $ResolvedFixture.StartsWith($TemporaryRoot, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($ResolvedFixture) -notlike 'vulcan-cleanup-test-*') { throw 'Unsafe fixture cleanup path.' }
    Remove-Item -LiteralPath $ResolvedFixture -Recurse -Force
}
