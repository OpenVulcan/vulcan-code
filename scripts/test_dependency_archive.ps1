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
