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

# Return the unit entry names in one profile area; Profile is debug or release, Area a subdirectory.
# 返回某模式目录区域内的单元条目名；Profile 为 debug 或 release，Area 为子目录。
function Get-UnitEntries {
    param([string]$Profile, [string]$Area)
    $Root = Join-Path $env:CARGO_TARGET_DIR "$Profile/$Area"
    if (-not (Test-Path -LiteralPath $Root)) { return @() }
    return @(Get-ChildItem -LiteralPath $Root -Force | ForEach-Object Name | Sort-Object)
}

# Return the unit entries of a profile as one comparable string; Profile is debug or release, Areas the subdirectories.
# 将模式目录的单元条目返回为可比较的字符串；Profile 为 debug 或 release，Areas 为子目录。
function Get-ProfileSnapshot {
    param([string]$Profile, [string[]]$Areas = @('deps', '.fingerprint', 'build', 'incremental'))
    return ($Areas | ForEach-Object { "$_=" + ((Get-UnitEntries $Profile $_) -join ',') }) -join ';'
}

# FixtureRoot is a uniquely owned temporary project; no production target directory is touched.
# FixtureRoot 是独占的临时工程；不接触生产 target 目录。
$FixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ('vulcan-cleanup-test-' + [guid]::NewGuid().ToString('N'))
# SavedEnvironment restores Cargo overrides after the test, even on failure.
# SavedEnvironment 在测试结束后恢复 Cargo 覆盖，包括失败路径。
$SavedEnvironment = @{}
foreach ($VariableName in @('CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_INCREMENTAL')) {
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
// Write a generated constant so the fixture owns a build-script output directory.
// 写入生成的常量，使测试工程拥有构建脚本输出目录。
fn main() {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(out.join("value.rs"), "pub const VALUE: u8 = 1;").unwrap();
}
'@ | Set-Content -LiteralPath 'dependency/build.rs'
    @'
// Return a dependency fixture value; takes no parameters and returns one.
// 返回依赖测试值；无参数，返回一。
include!(concat!(env!("OUT_DIR"), "/value.rs"));
pub fn value() -> u8 { VALUE }
'@ | Set-Content -LiteralPath 'dependency/src/lib.rs'
    @'
// Return a stable fixture value without external dependencies.
// 返回稳定的测试值，不使用外部依赖。
pub fn value() -> u8 { cleanup_dependency::value() }

#[test]
fn value_is_one() { assert_eq!(value(), 1); }
'@ | Set-Content -LiteralPath 'src/lib.rs'
    @'
// Exercise the fixture library; takes no parameters and returns unit.
// 调用测试库；无参数，返回单元类型。
fn main() { println!("{}", vulcan_code_service::value()); }
'@ | Set-Content -LiteralPath 'src/main.rs'
    $env:CARGO_TARGET_DIR = Join-Path $FixtureRoot 'custom-target'
    $Build = @('build', '-j', '4')

    # FirstBuild produces every unit kind: libraries, a binary, a build script and its run.
    # FirstBuild 产生全部单元类型：库、二进制、构建脚本及其运行。
    $FirstBuild = Invoke-TrackedCargoBuild -BuildArguments $Build
    Assert-CleanupCondition ($FirstBuild.ExitCode -eq 0) 'Initial debug build failed.'
    $OldLibrary = @($FirstBuild.Artifacts.filenames | Where-Object { $_ -match '[/\\]deps[/\\].+\.rlib$' })[0]
    $OldFingerprints = Get-UnitEntries 'debug' '.fingerprint'
    $OldBuildDirectories = Get-UnitEntries 'debug' 'build'
    Assert-CleanupCondition ($OldBuildDirectories.Count -ge 2) 'Fixture produced no build-script directories.'

    # UnhashedFile carries no unit hash and must never be swept.
    # UnhashedFile 不含单元哈希，绝不能被清理。
    $UnhashedFile = Join-Path $env:CARGO_TARGET_DIR 'debug/deps/unhashed-note.txt'
    Set-Content -LiteralPath $UnhashedFile 'preserve'

    # FreshBuild must keep every live unit, including the binary behind the uplifted executable.
    # FreshBuild 必须保留全部在用单元，包括提升后可执行文件对应的二进制。
    $Before = Get-ProfileSnapshot 'debug'
    $FreshBuild = Invoke-TrackedCargoBuild -BuildArguments $Build
    Assert-CleanupCondition (@($FreshBuild.Artifacts | Where-Object { -not $_.fresh }).Count -eq 0) 'Expected a fully fresh Cargo build.'
    Assert-CleanupCondition ((Get-ProfileSnapshot 'debug') -eq $Before) 'Fresh build removed a live unit.'

    # TestBuild leaves units the tracked build never uses; they stay within the retention period and a tracked
    # build sweeps them once they are older. Incremental caches of a crate the tracked build reused stay until
    # it recompiles that crate.
    # TestBuild 留下受跟踪构建不使用的单元；保留期内保留，超过后由受跟踪构建清理。受跟踪构建复用的 crate 的
    # 增量缓存保留到该 crate 重新编译为止。
    $UnitAreas = @('deps', '.fingerprint', 'build')
    $Before = Get-ProfileSnapshot 'debug' $UnitAreas
    & cargo test --no-run -j 4 --quiet
    Assert-CleanupCondition ($LASTEXITCODE -eq 0) 'Fixture test build failed.'
    $WithTests = Get-ProfileSnapshot 'debug' $UnitAreas
    Assert-CleanupCondition ($WithTests -ne $Before) 'Test build added no units.'
    $KeptBuild = Invoke-TrackedCargoBuild -BuildArguments $Build
    Assert-CleanupCondition ($KeptBuild.ExitCode -eq 0 -and (Get-ProfileSnapshot 'debug' $UnitAreas) -eq $WithTests) 'Recent test-only units were swept within the retention period.'
    # A zero retention period ages every unit out the moment a build starts; the remaining steps rely on it.
    # 保留期为零时，每个未使用单元在构建开始时即已超期；后续步骤依赖此设置。
    $script:UnusedUnitRetention = [TimeSpan]::Zero
    $SweptBuild = Invoke-TrackedCargoBuild -BuildArguments $Build
    Assert-CleanupCondition ($SweptBuild.ExitCode -eq 0 -and (Get-ProfileSnapshot 'debug' $UnitAreas) -eq $Before) 'Test-only units were not swept.'
    Assert-CleanupCondition (Test-Path -LiteralPath $UnhashedFile) 'Unhashed file was deleted.'
    # A build after the sweep must stay fully fresh, proving no live fingerprint or output was removed.
    # 清理后的构建必须保持完全复用，证明未删除在用指纹或输出。
    $AfterSweep = Invoke-TrackedCargoBuild -BuildArguments $Build
    Assert-CleanupCondition (@($AfterSweep.Artifacts | Where-Object { -not $_.fresh }).Count -eq 0) 'Sweep removed state a later build needed.'

    # HashBuild changes every unit hash; the previous units retire from every area.
    # HashBuild 改变全部单元哈希；旧单元从各区域退役。
    $env:RUSTFLAGS = '--cfg cleanup_fixture_second'
    $HashStartedUtc = [DateTime]::UtcNow
    $HashBuild = Invoke-TrackedCargoBuild -BuildArguments $Build
    Assert-CleanupCondition ($HashBuild.ExitCode -eq 0 -and -not (Test-Path -LiteralPath $OldLibrary)) 'Retired library was not deleted.'
    # Units whose hash ignores the flags stay live, so each area must keep exactly one entry per unit.
    # 哈希不受该标志影响的单元仍在使用，因此各区域必须保持每个单元恰好一个条目。
    Assert-CleanupCondition ((Get-UnitEntries 'debug' '.fingerprint').Count -eq $OldFingerprints.Count) 'Retired fingerprints were not deleted.'
    Assert-CleanupCondition ((Get-UnitEntries 'debug' 'build').Count -eq $OldBuildDirectories.Count) 'Retired build-script directories were not deleted.'
    $Retired = @(Get-UnitEntries 'debug' '.fingerprint' | Where-Object { $OldFingerprints -notcontains $_ }).Count
    Assert-CleanupCondition ($Retired -gt 0) 'The flag change replaced no unit.'
    foreach ($Incremental in Get-ChildItem -LiteralPath (Join-Path $env:CARGO_TARGET_DIR 'debug/incremental') -Directory) {
        $Newest = (@($Incremental) + @($Incremental.GetFileSystemInfos()) | ForEach-Object LastWriteTimeUtc | Measure-Object -Maximum).Maximum
        Assert-CleanupCondition ($Newest -ge $HashStartedUtc.AddSeconds(-2)) "Retired incremental cache $($Incremental.Name) was kept."
    }

    # ReleaseBuild sweeps only its own profile.
    # ReleaseBuild 只清理自身模式目录。
    $DebugBefore = Get-ProfileSnapshot 'debug'
    $ReleaseBuild = Invoke-TrackedCargoBuild -BuildArguments @('build', '--release', '-j', '4')
    Assert-CleanupCondition ($ReleaseBuild.ExitCode -eq 0 -and (Get-ProfileSnapshot 'debug') -eq $DebugBefore) 'Release cleanup touched debug units.'

    # StaleUnit stands in for an unused unit; a failed build must not sweep it, the next success must.
    # StaleUnit 代表未使用的单元；失败构建不得清理它，下一次成功构建必须清理。
    $StaleUnit = Join-Path $env:CARGO_TARGET_DIR 'debug/deps/libstale-0123456789abcdef.rlib'
    Set-Content -LiteralPath $StaleUnit 'stale'
    $Source = Get-Content -LiteralPath 'src/lib.rs' -Raw
    Set-Content -LiteralPath 'src/lib.rs' 'compile_error!("intentional cleanup test failure");'
    $FailedBuild = Invoke-TrackedCargoBuild -BuildArguments $Build
    Assert-CleanupCondition ($FailedBuild.ExitCode -ne 0 -and (Test-Path -LiteralPath $StaleUnit)) 'Failed build swept artifacts.'
    Set-Content -LiteralPath 'src/lib.rs' $Source -NoNewline

    # HeldLock stands in for a running Cargo command; cleanup must wait for it rather than sweep.
    # HeldLock 代表运行中的 Cargo 命令；清理必须等待其释放而非直接清理。
    $script:CargoLockTimeout = [TimeSpan]::FromSeconds(2)
    $HeldLock = [IO.File]::Open((Join-Path $env:CARGO_TARGET_DIR 'debug/.cargo-lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete)
    try {
        $HeldLock.Lock(0, [long]::MaxValue)
        $Waiting = $false
        try { Lock-CargoProfile -ProfileRoot (Join-Path $env:CARGO_TARGET_DIR 'debug') | ForEach-Object { $_.Dispose() } }
        catch { $Waiting = $_.Exception.Message -like 'Timed out*' }
        Assert-CleanupCondition $Waiting 'Cleanup did not wait for a held Cargo lock.'
    }
    finally { $HeldLock.Dispose() }
    $RecoveredBuild = Invoke-TrackedCargoBuild -BuildArguments $Build
    Assert-CleanupCondition ($RecoveredBuild.ExitCode -eq 0 -and -not (Test-Path -LiteralPath $StaleUnit)) 'Stale unit survived a successful build.'
    Write-Host 'PASS: syntax, fresh reuse, test-unit retention and sweep, deps/fingerprint/build/incremental retirement, unhashed preservation, profile isolation, failed-build preservation, Cargo lock wait.'
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
