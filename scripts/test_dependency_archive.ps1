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
        . (Join-Path $PSScriptRoot 'macos-lua-runtime.ps1')
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

        # MacModuleRoot mirrors the official archive layout, including a nested Lua C module.
        # MacModuleRoot 模拟官方归档布局，并包含一个嵌套的 Lua C 模块。
        $MacModuleRoot = Join-Path $PayloadRoot 'lua_packages/lib/lua'
        New-Item -ItemType Directory -Path (Join-Path $MacModuleRoot 'socket'), (Join-Path $PayloadRoot 'libs') -Force | Out-Null
        [IO.File]::WriteAllText((Join-Path $MacModuleRoot 'socket/core.so'), 'nested module fixture')
        [IO.File]::WriteAllText((Join-Path $MacModuleRoot 'lcurl.so'), '@rpath/libcurl.4.dylib')
        [IO.File]::WriteAllText((Join-Path $MacModuleRoot 'lxp.so'), '@rpath/libexpat.1.dylib')
        foreach ($Module in @('rex_pcre2', 'ssl', 'yaml', 'zip', 'zlib')) {
            [IO.File]::WriteAllText((Join-Path $MacModuleRoot "$Module.so"), 'module fixture')
        }
        [IO.File]::WriteAllText((Join-Path $PayloadRoot 'libs/libcrypto.3.dylib'), 'crypto fixture')
        [IO.File]::WriteAllText((Join-Path $PayloadRoot 'libs/libexpat.1.dylib'), 'expat fixture')
        [IO.File]::WriteAllText((Join-Path $PayloadRoot 'libs/libcurl.4.dylib'), 'broken upstream curl fixture')
        # MacArchive covers the real install and cache paths without requiring a Mac or network access.
        # MacArchive 覆盖真实安装与缓存路径，无需 Mac 或网络。
        $MacArchive = Join-Path $TestRoot 'lua mac fixture.tar.gz'

        # NativeCalls captures the real installer's arguments to platform tools unavailable on Windows.
        # NativeCalls 捕获真实安装器传给 Windows 上不可用的原生工具的参数。
        $NativeCalls = [Collections.Generic.List[string]]::new()
        # Read fixture load commands in the format printed by otool for one binary.
        # 以 otool 的输出格式读取一个夹具二进制的加载命令。
        function otool {
            if ($args[0] -cne '-l') { throw 'Unexpected otool operation.' }
            # FixturePath persists simulated edits across staging and the installed tree.
            # FixturePath 使模拟编辑结果在暂存目录与安装目录之间保留。
            $FixturePath = $args[1]
            # FixtureRecords contains link commands and an optional minimum system version.
            # FixtureRecords 包含链接命令和可选的最低系统版本。
            $FixtureRecords = [IO.File]::ReadAllLines($FixturePath)
            if (-not ($FixtureRecords -match '^MIN ')) {
                '  cmd LC_BUILD_VERSION'
                'minos 15.0'
            }
            foreach ($Line in $FixtureRecords) {
                if ($Line.StartsWith('ID ')) {
                    '  cmd LC_ID_DYLIB'
                    " name $($Line.Substring(3)) (offset 24)"
                } elseif ($Line.StartsWith('MIN ')) {
                    '  cmd LC_BUILD_VERSION'
                    "minos $($Line.Substring(4))"
                } elseif ($Line.StartsWith('@') -or $Line.StartsWith('/')) {
                    '  cmd LC_LOAD_DYLIB'
                    " name $Line (offset 24)"
                }
            }
            $global:LASTEXITCODE = 0
        }
        # Apply install-name edits to the fixture so the production readback checks its result.
        # 在夹具中应用安装名编辑，使生产回读校验能检查其结果。
        function install_name_tool {
            # FixturePath names the native file whose test load commands are being edited.
            # FixturePath 指定正在编辑测试加载命令的原生文件。
            $FixturePath = $args[-1]
            # FixtureLinks holds the current file content while simulating the tool's update.
            # FixtureLinks 在模拟工具更新时保存当前文件内容。
            $FixtureLinks = [IO.File]::ReadAllText($FixturePath)
            for ($Index = 0; $Index -lt ($args.Length - 1); $Index++) {
                switch -CaseSensitive ($args[$Index]) {
                    '-change' {
                        $FixtureLinks = $FixtureLinks.Replace([string]$args[$Index + 1], [string]$args[$Index + 2])
                        $Index += 2
                    }
                    '-id' {
                        $FixtureLinks = $FixtureLinks -creplace '(?m)^ID [^\r\n]+', "ID $($args[$Index + 1])"
                        $Index += 1
                    }
                    default { throw 'Unexpected install_name_tool operation.' }
                }
            }
            [IO.File]::WriteAllText($FixturePath, $FixtureLinks)
            $NativeCalls.Add(($args[0..($args.Length - 2)] -join ' '))
            $global:LASTEXITCODE = 0
        }
        # Record the signature refresh for the supplied binary and return success.
        # 记录对指定二进制的签名刷新并返回成功。
        function codesign {
            $NativeCalls.Add(($args[0..2] -join ' ') + ' ' + [IO.Path]::GetFileName($args[3]))
            $global:LASTEXITCODE = 0
        }

        # BuildPath is the upstream archive's observed build-root prefix for bundled libraries.
        # BuildPath 是已核对的上游归档随包库构建根路径前缀。
        $BuildPath = '/Users/runner/work/luaskills-packages/luaskills-packages/luaskills/third_party/deps/openssl/lib'
        [IO.File]::WriteAllLines((Join-Path $MacModuleRoot '_openssl.so'), @("$BuildPath/libssl.3.dylib", "$BuildPath/libcrypto.3.dylib"))
        [IO.File]::WriteAllLines((Join-Path $PayloadRoot 'libs/libssl.3.dylib'), @("ID $BuildPath/libssl.3.dylib", "$BuildPath/libcrypto.3.dylib"))
        & $TarPath -czf $MacArchive -C $PayloadRoot resources lua_packages libs
        if ($LASTEXITCODE -ne 0) { throw 'macOS Lua fixture archive creation failed.' }

        foreach ($Platform in @('macos-arm64', 'macos-x64')) {
            $NativeCalls.Clear()
            # Simulate a previous release's cache marker to force a rebuild from the verified archive.
            # 模拟旧发行版的缓存标记，确认从经校验的归档重建。
            New-Item -ItemType File -Path (Join-Path $RuntimeInstallRoot ".installed-vmac-$Platform-loader-layout-v1") -Force | Out-Null
            Install-LuaRuntimePayloads -RuntimeArchivePath $MacArchive -RuntimeTag 'vmac' -Platform $Platform
            # InstalledModules is the isolated install's native module search directory.
            # InstalledModules 是隔离安装中的原生模块搜索目录。
            $InstalledModules = Join-Path $RuntimeInstallRoot 'lua_packages/lib/lua'
            if ([IO.File]::ReadAllText((Join-Path $InstalledModules 'socket/core.dylib')) -ne 'nested module fixture' -or
                (Test-Path -LiteralPath (Join-Path $RuntimeInstallRoot 'libs/libcurl.4.dylib')) -or
                (Test-Path -LiteralPath (Join-Path $InstalledModules '_openssl.so'))) {
                throw "macOS Lua module installation did not normalize the verified payload for $Platform."
            }
            if ([IO.File]::ReadAllText((Join-Path $InstalledModules 'lcurl.dylib')) -ne '/usr/lib/libcurl.4.dylib' -or
                [IO.File]::ReadAllText((Join-Path $InstalledModules 'lxp.dylib')) -ne '@loader_path/../../../libs/libexpat.1.dylib' -or
                [IO.File]::ReadAllText((Join-Path $RuntimeInstallRoot 'libs/libssl.3.dylib')) -notmatch '(?m)^@loader_path/libcrypto\.3\.dylib' -or
                -not $NativeCalls.Contains('--force --sign - lcurl.dylib')) {
                throw "macOS Lua libraries were not relocated and signed for $Platform."
            }
            $NativeCalls.Clear()
            Install-LuaRuntimePayloads -RuntimeArchivePath $MacArchive -RuntimeTag 'vmac' -Platform $Platform
            if ($NativeCalls.Count -ne 0) { throw "The updated macOS runtime cache was not reused for $Platform." }
        }
        # A missing dependency must fail before the prepared runtime is replaced.
        # 缺失依赖必须在替换已准备的运行时之前失败。
        [IO.File]::WriteAllText((Join-Path $MacModuleRoot 'lxp.so'), '@rpath/missing.dylib')
        & $TarPath -czf $MacArchive -C $PayloadRoot resources lua_packages libs
        if ($LASTEXITCODE -ne 0) { throw 'Broken macOS Lua fixture archive creation failed.' }
        $Rejected = $false
        try { Install-LuaRuntimePayloads -RuntimeArchivePath $MacArchive -RuntimeTag 'vmissing' -Platform 'macos-x64' }
        catch { $Rejected = $_.Exception.Message -like '*missing.dylib*' }
        if (-not $Rejected -or -not (Test-Path -LiteralPath (Join-Path $InstalledModules 'lxp.dylib'))) {
            throw 'Missing macOS Lua dependency did not preserve the previous runtime.'
        }
        # Synchronization must reject a prepared-looking library that raises the minimum system version.
        # 同步必须拒绝看似已准备但提高了最低系统版本的原生库。
        [IO.File]::AppendAllText((Join-Path $InstalledModules 'lxp.dylib'), [Environment]::NewLine + 'MIN 15.1')
        $Rejected = $false
        try { Confirm-MacLuaRuntime -Root $RuntimeInstallRoot }
        catch { $Rejected = $_.Exception.Message -like '*macOS 15.1*' }
        if (-not $Rejected) { throw 'macOS Lua validation accepted a library that requires macOS 15.1.' }
        Install-LuaRuntimePayloads -RuntimeArchivePath $MacArchive -RuntimeTag 'vlinux' -Platform 'linux-x64'
        if (-not (Test-Path -LiteralPath (Join-Path $InstalledModules '_openssl.so')) -or
            (Test-Path -LiteralPath (Join-Path $InstalledModules '_openssl.dylib'))) {
            throw 'The macOS module naming rule changed the Linux layout.'
        }
        Write-Host 'PASS: macOS Lua modules link to system cURL and bundled libraries; missing dependencies preserve the prior installation.'
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
