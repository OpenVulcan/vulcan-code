# Exercise native extraction and the real installer functions with isolated offline archives.
# 使用隔离的离线归档验证原生解压和真实安装器函数。
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'dependency-archive.ps1')

# Get-InstallerFunctionDefinitions parses only top-level functions without running downloads or installation.
# Get-InstallerFunctionDefinitions 仅解析顶层函数，不执行下载或安装入口。
# Path identifies the checked-in installer; returns a script block for dot-sourcing in the test scope.
# Path 标识签入的安装器；返回用于在测试作用域加载的脚本块。
function Get-InstallerFunctionDefinitions {
    param([Parameter(Mandatory = $true)][string]$Path)
    # Tokens, Errors, and Ast retain the native PowerShell parser's real syntax result.
    # Tokens、Errors 与 Ast 保存原生 PowerShell 解析器的真实语法结果。
    $Tokens = $null
    $Errors = $null
    $Ast = [Management.Automation.Language.Parser]::ParseFile($Path, [ref]$Tokens, [ref]$Errors)
    if ($Errors.Count -ne 0) { throw ($Errors | Out-String) }
    return [scriptblock]::Create(($Ast.EndBlock.Statements |
        Where-Object { $_ -is [Management.Automation.Language.FunctionDefinitionAst] } |
        ForEach-Object { $_.Extent.Text }) -join "`n")
}

# TemporaryRoot and TestRoot bound all fixture writes and recursive cleanup to one owned directory.
# TemporaryRoot 与 TestRoot 将全部夹具写入和递归清理限制在一个自有目录中。
$TemporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$TestRoot = Join-Path $TemporaryRoot ('vulcan-archive-test-' + [Guid]::NewGuid().ToString('N'))
# WindowsHost selects only the current host's actual archive and executable behavior.
# WindowsHost 只选择当前宿主实际的归档与可执行行为。
$WindowsHost = [Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([Runtime.InteropServices.OSPlatform]::Windows)
# TarPath is the production resolver result, not a mock or a hard-coded operating-system path.
# TarPath 来自生产解析函数，不是模拟或硬编码的操作系统路径。
$TarPath = Get-AvailableTarPath
try {
    New-Item -ItemType Directory -Path (Join-Path $TestRoot 'payload with spaces'), (Join-Path $TestRoot 'extracted') -Force | Out-Null
    # SourceFile and ArchivePath test argument boundaries with real paths containing spaces.
    # SourceFile 与 ArchivePath 使用真实含空格路径验证参数边界。
    $SourceFile = Join-Path $TestRoot 'payload with spaces/payload.txt'
    [IO.File]::WriteAllText($SourceFile, 'offline archive fixture')
    $ArchivePath = Join-Path $TestRoot 'valid archive.tar.gz'
    & $TarPath -czf $ArchivePath -C (Split-Path $SourceFile -Parent) payload.txt
    if ($LASTEXITCODE -ne 0) { throw 'Fixture archive creation failed.' }
    Expand-DependencyTarArchive -ArchivePath $ArchivePath -DestinationPath (Join-Path $TestRoot 'extracted')
    if ([IO.File]::ReadAllText((Join-Path $TestRoot 'extracted/payload.txt')) -ne 'offline archive fixture') {
        throw 'Native tar extraction changed the fixture payload.'
    }
    Write-Host 'PASS: native tar resolution and extraction with spaces.'

    # CorruptArchive contains bytes that satisfy the fixture digest but cannot be extracted as tar.
    # CorruptArchive 的字节满足夹具摘要，但无法作为 tar 解压。
    $CorruptArchive = Join-Path $TestRoot 'corrupt.tar.gz'
    [IO.File]::WriteAllText($CorruptArchive, 'not a tar archive')
    # Rejected records the actual native-exit failure rather than matching diagnostic wording.
    # Rejected 记录真实原生退出失败，不匹配诊断文案。
    $Rejected = $false
    try { Expand-DependencyTarArchive -ArchivePath $CorruptArchive -DestinationPath (Join-Path $TestRoot 'extracted') }
    catch { $Rejected = $true }
    if (-not $Rejected) { throw 'Native tar failure was ignored.' }
    Write-Host 'PASS: nonzero tar exit rejects corrupt archives.'

    & {
        . (Get-InstallerFunctionDefinitions -Path (Join-Path $PSScriptRoot 'install_host_deps.ps1'))
        # Installer fixtures use only this test's plugin root and local archive pool.
        # 安装器夹具仅使用本测试的插件根目录和本地归档池。
        $RtkPluginDir = Join-Path $TestRoot 'host/plugins/vulcan-rtk'
        $RtkBinRoot = Join-Path $RtkPluginDir 'bin'
        $ThirdParty = Join-Path $TestRoot 'host/third_party'
        $RtkTag = 'vfixture'
        $RtkLicenseUrl = 'https://example.invalid/fixture-license'
        $RtkLicenseFile = Join-Path $RtkPluginDir 'LICENSE'
        # RtkLicenseSha256 represents the test-owned license bytes, independent of the production release.
        # RtkLicenseSha256 表示测试拥有的许可证字节，与生产发行版无关。
        $FixtureLicense = Join-Path $TestRoot 'license.fixture.txt'
        [IO.File]::WriteAllText($FixtureLicense, 'fixture license')
        $RtkLicenseSha256 = (Get-FileHash -LiteralPath $FixtureLicense -Algorithm SHA256).Hash.ToLowerInvariant()
        $FixturePayload = Join-Path $TestRoot 'rtk payload'
        New-Item -ItemType Directory -Path $FixturePayload, $ThirdParty -Force | Out-Null
        # BinaryName and FixtureExtension select the actual host branch without emulating another OS.
        # BinaryName 与 FixtureExtension 选择实际宿主分支，不模拟其他操作系统。
        $BinaryName = if ($WindowsHost) { 'rtk.exe' } else { 'rtk' }
        $FixtureExtension = if ($WindowsHost) { '.zip' } else { '.tar.gz' }
        $FixtureBinary = Join-Path $FixturePayload $BinaryName
        [IO.File]::WriteAllText($FixtureBinary, "#!/bin/sh`nprintf '%s\n' 'fixture-rtk'`n")
        if (-not $WindowsHost) {
            & chmod 755 $FixtureBinary
            if ($LASTEXITCODE -ne 0) { throw 'Fixture executable permission setup failed.' }
        }
        $FixtureArchive = Join-Path $ThirdParty "rtk-fixture$FixtureExtension"
        if ($WindowsHost) { Compress-Archive -LiteralPath $FixtureBinary -DestinationPath $FixtureArchive }
        else {
            & $TarPath -czf $FixtureArchive -C $FixturePayload $BinaryName
            if ($LASTEXITCODE -ne 0) { throw 'RTK fixture archive creation failed.' }
        }
        $FixtureAsset = @{ target = 'fixture'; plugin_target = 'fixture'; binary_name = $BinaryName;
            archive_ext = $FixtureExtension; sha256 = (Get-FileHash -LiteralPath $FixtureArchive -Algorithm SHA256).Hash }
        # Get-RtkAssetInfo substitutes only the upstream fixture identity; extraction and publishing remain production functions.
        # Get-RtkAssetInfo 仅替换上游夹具身份；解压与发布仍使用生产函数。
        # Returns the locally generated archive identity and digest with no network access.
        # 返回本地生成归档的身份及摘要，不访问网络。
        function Get-RtkAssetInfo { return $FixtureAsset }
        if ($WindowsHost) {
            # ZIP extraction must work even when no tar application is available to the host installer.
            # 即使宿主安装器无法取得 tar 程序，ZIP 解压仍必须成功。
            # Returns nothing and fails if the ZIP branch incorrectly requests a tar executable.
            # 无返回值；ZIP 分支错误请求 tar 时立即失败。
            function Get-AvailableTarPath { throw 'ZIP installation unexpectedly requested tar.' }
        }
        # Invoke-WebRequest substitutes the license transport only and writes the declared offline fixture.
        # Invoke-WebRequest 仅替换许可证传输，并写入声明的离线夹具。
        # Uri and OutFile identify the fixture request; UseBasicParsing matches the production signature; returns nothing.
        # Uri 与 OutFile 标识夹具请求；UseBasicParsing 匹配生产签名；无返回值。
        function Invoke-WebRequest {
            param([string]$Uri, [string]$OutFile, [switch]$UseBasicParsing)
            if ($Uri -ne $RtkLicenseUrl) { throw 'Unexpected fixture download.' }
            [IO.File]::WriteAllText($OutFile, 'fixture license')
        }
        # OriginalTemp is restored after proving that the installer does not require the Windows TEMP variable.
        # OriginalTemp 在验证安装器不依赖 Windows TEMP 变量后恢复。
        $OriginalTemp = $env:TEMP
        try {
            $env:TEMP = $null
            Install-RtkBinary
            Install-RtkLicense
        } finally { $env:TEMP = $OriginalTemp }
        $InstalledBinary = Join-Path $RtkBinRoot "fixture/$BinaryName"
        if ([IO.File]::ReadAllText($InstalledBinary) -ne [IO.File]::ReadAllText($FixtureBinary)) { throw 'RTK payload changed.' }
        if ([IO.File]::ReadAllText($RtkLicenseFile) -ne 'fixture license') { throw 'License fixture was not installed.' }
        if (-not $WindowsHost) {
            & $InstalledBinary | Out-Null
            if ($LASTEXITCODE -ne 0) { throw 'Installed Unix RTK fixture is not executable.' }
        }
        if ($WindowsHost) { Write-Host 'PASS: offline Windows ZIP and license installation without TEMP or tar.' }
        else { Write-Host 'PASS: offline Unix RTK and license installation without TEMP; executable preserved.' }
        Install-RtkBinary
        # Remove only the test-owned marker to simulate a checkout that already contains the correct license.
        # 仅移除测试拥有的标记，以模拟检出目录已经包含正确许可证的情况。
        Remove-Item -LiteralPath (Join-Path $RtkBinRoot '.license-vfixture') -Force
        # Invoke-WebRequest must not run when exact license bytes already exist locally.
        # 精确许可证字节已在本地存在时，必须不调用 Invoke-WebRequest。
        # Accepts the production transport parameters and throws if unnecessary network access occurs.
        # 接收生产传输参数；发生不必要的联网时抛出异常。
        function Invoke-WebRequest {
            param([string]$Uri, [string]$OutFile, [switch]$UseBasicParsing)
            throw 'Verified local license unexpectedly triggered a download.'
        }
        Install-RtkLicense
        Write-Host 'PASS: repeated RTK installation and verified checkout license avoid downloads.'
        [IO.File]::WriteAllText($RtkLicenseFile, 'corrupt license')
        $Rejected = $false
        try { Install-RtkLicense } catch { $Rejected = $true }
        if (-not $Rejected) { throw 'An installed license marker bypassed content verification.' }
        Write-Host 'PASS: a license marker cannot bypass the required content digest.'
        if (-not $WindowsHost) {
            # The second fixture is checksum-valid but extraction-invalid; no success marker may be published.
            # 第二个夹具摘要有效但无法解压；不得发布成功标记。
            $RtkTag = 'vcorrupt'
            $FixtureAsset.plugin_target = 'corrupt'
            Copy-Item -LiteralPath $CorruptArchive -Destination $FixtureArchive -Force
            $FixtureAsset.sha256 = (Get-FileHash -LiteralPath $FixtureArchive -Algorithm SHA256).Hash
            $Rejected = $false
            try { Install-RtkBinary } catch { $Rejected = $true }
            if (-not $Rejected -or (Test-Path -LiteralPath (Join-Path $RtkBinRoot 'corrupt/.installed-vcorrupt'))) {
                throw 'Failed RTK extraction published success.'
            }
            Write-Host 'PASS: failed RTK extraction does not publish an installed marker.'
        }
    }

    & {
        . (Get-InstallerFunctionDefinitions -Path (Join-Path $PSScriptRoot 'install_lua_deps.ps1'))
        # Runtime roots and manifests are a minimal structural fixture for the real Lua installer.
        # 运行时根目录与清单构成真实 Lua 安装器所需的最小结构夹具。
        $ThirdParty = Join-Path $TestRoot 'lua/third_party'
        $RuntimeInstallRoot = Join-Path $ThirdParty 'luaskills_runtime'
        $PayloadRoot = Join-Path $TestRoot 'lua payload'
        New-Item -ItemType Directory -Path (Join-Path $PayloadRoot 'resources') -Force | Out-Null
        foreach ($Name in @('lua-runtime-manifest.json', 'luaskills-packages-manifest.json')) {
            [IO.File]::WriteAllText((Join-Path $PayloadRoot "resources/$Name"), '{}')
        }
        $LuaArchive = Join-Path $TestRoot 'lua fixture.tar.gz'
        & $TarPath -czf $LuaArchive -C $PayloadRoot resources
        if ($LASTEXITCODE -ne 0) { throw 'Lua fixture archive creation failed.' }
        Install-LuaRuntimePayloads -RuntimeArchivePath $LuaArchive -RuntimeTag 'vfixture' -Platform 'fixture'
        $Marker = Join-Path $RuntimeInstallRoot '.installed-vfixture-fixture'
        if (-not (Test-RuntimeInstallReady -MarkerFile $Marker)) { throw 'Lua layout was not installed.' }
        Write-Host 'PASS: Lua runtime uses the shared native extractor.'
        Install-LuaRuntimePayloads -RuntimeArchivePath $LuaArchive -RuntimeTag 'vfixture' -Platform 'fixture'
        $Rejected = $false
        try { Install-LuaRuntimePayloads -RuntimeArchivePath $CorruptArchive -RuntimeTag 'vcorrupt' -Platform 'fixture' }
        catch { $Rejected = $true }
        if (-not $Rejected -or -not (Test-RuntimeInstallReady -MarkerFile $Marker) -or
            (Test-Path -LiteralPath (Join-Path $RuntimeInstallRoot '.installed-vcorrupt-fixture'))) {
            throw 'Failed Lua extraction replaced the completed runtime or published success.'
        }
        Write-Host 'PASS: failed Lua extraction preserves the previous runtime and does not publish success.'
    }
}
finally {
    # ResolvedTestRoot must be an immediate child of the exact temporary root before recursive cleanup.
    # 递归清理前，ResolvedTestRoot 必须是确切临时根目录的直属子目录。
    $ResolvedTestRoot = [IO.Path]::GetFullPath($TestRoot)
    if ([IO.Path]::GetDirectoryName($ResolvedTestRoot) -ne $TemporaryRoot.TrimEnd([IO.Path]::DirectorySeparatorChar)) {
        throw "Refusing cleanup outside the test temporary root: $ResolvedTestRoot"
    }
    if (Test-Path -LiteralPath $ResolvedTestRoot) { Remove-Item -LiteralPath $ResolvedTestRoot -Recurse -Force }
}
