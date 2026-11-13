# When invoked directly, validate the prepared native payload before synchronization.
# 直接调用时，在同步前校验已准备的原生载荷。
param([string]$RuntimeRoot, [switch]$Smoke)

# Replace the module suffix expected by LuaSkills while leaving native library aliases intact.
# 改为 LuaSkills 查找的模块后缀，保留共享库的原有别名。
# Modules is the directory supplied by the official archive. Returns no value.
# Modules 是官方归档提供的模块目录。无返回值。
function Rename-MacLuaModules {
    param([string]$Modules)

    foreach ($Binary in Get-ChildItem -LiteralPath $Modules -Recurse -File -Filter '*.so') {
        # Name is the loader-compatible name for this exact native module.
        # Name 是当前原生模块对应的加载器文件名。
        $Name = [IO.Path]::ChangeExtension($Binary.Name, '.dylib')
        if (Test-Path -LiteralPath (Join-Path $Binary.DirectoryName $Name)) {
            throw "Conflicting macOS Lua native module names: $($Binary.FullName)"
        }
        Rename-Item -LiteralPath $Binary.FullName -NewName $Name
    }
}

# Read Mach-O load commands so install IDs are distinct from actual dependencies.
# 读取 Mach-O 加载命令，区分动态库标识和实际依赖。
# Binary is one native file. Returns its ID, dependencies, search paths and minimum macOS versions.
# Binary 是一个原生文件。返回其标识、依赖、搜索路径和最低 macOS 版本。
function Get-MacLuaBinaryLinks {
    param([string]$Binary)

    # Lines are the load commands reported by Apple's otool.
    # Lines 是 Apple otool 报告的加载命令。
    $Lines = & otool -l $Binary
    if ($LASTEXITCODE -ne 0) { throw "Failed to inspect macOS Lua binary: $Binary" }
    # Links keeps the fields that affect loading separate from the binary's own ID.
    # Links 将影响加载的字段与二进制自身标识分开保存。
    $Links = @{
        Id = @()
        Dependencies = @()
        RunPaths = @()
        MinimumVersions = @()
    }
    # Command tracks the record that owns the next name, path or version.
    # Command 记录下一条名称、路径或版本所属的加载命令。
    $Command = ''
    foreach ($Line in $Lines) {
        if ($Line -match '^\s*cmd\s+(LC_\w+)\s*$') {
            $Command = $Matches[1]
        } elseif ($Line -match '^\s*name\s+(.+?)\s+\(offset \d+\)\s*$') {
            if ($Command -eq 'LC_ID_DYLIB') {
                $Links.Id += $Matches[1]
            } elseif ($Command -in @('LC_LOAD_DYLIB', 'LC_LOAD_WEAK_DYLIB', 'LC_REEXPORT_DYLIB', 'LC_LOAD_UPWARD_DYLIB', 'LC_LAZY_LOAD_DYLIB')) {
                $Links.Dependencies += $Matches[1]
            }
        } elseif ($Command -eq 'LC_RPATH' -and $Line -match '^\s*path\s+(.+?)\s+\(offset \d+\)\s*$') {
            $Links.RunPaths += $Matches[1]
        } elseif (($Command -eq 'LC_BUILD_VERSION' -and $Line -match '^\s*minos\s+(\d+(?:\.\d+){0,2})\s*$') -or
            ($Command -eq 'LC_VERSION_MIN_MACOSX' -and $Line -match '^\s*version\s+(\d+(?:\.\d+){0,2})\s*$')) {
            $Links.MinimumVersions += $Matches[1]
        }
    }
    if ($Links.MinimumVersions.Count -eq 0 -or $Links.Id.Count -gt 1) {
        throw "Unexpected Mach-O load commands in $Binary."
    }
    return $Links
}

# Inspect or normalize all native modules and libraries under an extracted macOS runtime.
# 检查或规范化 macOS 解压运行时下的全部原生模块与共享库。
# Root is the payload root. Relocate permits binary edits and signing before publication. Returns no value.
# Root 是载荷根目录。Relocate 允许在发布前编辑二进制并签名。无返回值。
function Confirm-MacLuaRuntime {
    param([string]$Root, [switch]$Relocate)

    # Modules and Libraries are the only two native code directories in the official archive.
    # Modules 和 Libraries 是官方归档中的两个原生代码目录。
    $Modules = Join-Path $Root 'lua_packages/lib/lua'
    $Libraries = Join-Path $Root 'libs'
    if ($Relocate) { Rename-MacLuaModules -Modules $Modules }
    # SystemCurl is the platform ABI supplied on both architectures from macOS 15.
    # SystemCurl 是 macOS 15 起两种架构都提供的平台 ABI。
    $SystemCurl = '/usr/lib/libcurl.4.dylib'
    # SupportedMacOs is the minimum supported macOS version for the distributed payload.
    # SupportedMacOs 是发行载荷支持的最低 macOS 版本。
    $SupportedMacOs = [version]'15.0.0'
    # LuaCurl is the binding that uses Apple's libcurl in place of the broken upstream copies.
    # LuaCurl 是改用 Apple libcurl 的 Lua 绑定。
    $LuaCurl = Join-Path $Modules 'lcurl.dylib'
    foreach ($RequiredModule in @('_openssl.dylib', 'lcurl.dylib', 'lxp.dylib', 'rex_pcre2.dylib', 'ssl.dylib', 'yaml.dylib', 'zip.dylib', 'zlib.dylib')) {
        if (-not (Test-Path -LiteralPath (Join-Path $Modules $RequiredModule) -PathType Leaf)) {
            throw "Missing prepared macOS Lua module $RequiredModule. Run make deps lua."
        }
    }
    if (Get-ChildItem -LiteralPath $Modules -Recurse -File -Filter '*.so') {
        throw 'Found unprepared macOS Lua modules. Run make deps lua.'
    }
    # UpstreamCurl contains the known aliases and versioned copies of the Homebrew-linked libcurl.
    # UpstreamCurl 包含上游链接 Homebrew 的 libcurl 版本文件与别名。
    $UpstreamCurl = Get-ChildItem -LiteralPath $Libraries -File -Filter '*.dylib' |
        Where-Object Name -Match '^libcurl(?:\.\d+)*\.dylib$'
    if ($UpstreamCurl -and -not $Relocate) {
        throw 'Unexpected bundled macOS libcurl. Run make deps lua.'
    }
    # Binaries includes the historical libluajit.so shared library under libs, whose name is part of that package's API.
    # Binaries 包含 libs 下历史命名的 libluajit.so 共享库，其文件名属于该包的 API。
    $Binaries = Get-ChildItem -LiteralPath $Modules, $Libraries -Recurse -File |
        Where-Object { $_.Extension -in @('.dylib', '.so') -and $_.FullName -cnotin $UpstreamCurl.FullName }
    foreach ($Binary in $Binaries) {
        # Links exposes dependency load commands and the binary's own install ID separately.
        # Links 分别提供依赖加载命令与二进制自身的安装标识。
        $Links = Get-MacLuaBinaryLinks -Binary $Binary.FullName
        foreach ($Minimum in $Links.MinimumVersions) {
            if ($Minimum -notmatch '^\d+\.\d+\.\d+$') {
                # Version comparison treats a missing patch as unspecified. Fill it with zero.
                # 版本比较会把缺失的补丁号视为未指定。补齐为零。
                $Minimum = ($Minimum.Split('.') + '0' + '0')[0..2] -join '.'
            }
            if ([version]$Minimum -gt $SupportedMacOs) {
                throw "macOS Lua binary $($Binary.FullName) requires macOS $Minimum, above $SupportedMacOs."
            }
        }
        if ($Links.RunPaths.Count -ne 0) {
            throw "Unexpected Mach-O runpath in $($Binary.FullName)."
        }
        # LibraryPath is relative to each consumer, including nested Lua modules.
        # LibraryPath 相对于各个调用库，包括嵌套的 Lua 模块。
        $LibraryPath = [IO.Path]::GetRelativePath($Binary.DirectoryName, $Libraries).Replace('\', '/')
        # Prefix uses the loader's own directory and never depends on process environment.
        # Prefix 以加载对象自身目录为基准，不依赖进程环境。
        $Prefix = if ($LibraryPath -eq '.') { '@loader_path/' } else { "@loader_path/$LibraryPath/" }
        # Changes is the complete edit for this binary, signed once after all commands are written.
        # Changes 是该二进制的完整改动，全部写入后只签名一次。
        $Changes = @()
        # Expected records the final dependency edges for readback verification.
        # Expected 记录用于回读验证的最终依赖边。
        $Expected = @()
        foreach ($Dependency in $Links.Dependencies) {
            # Name identifies the exact referenced basename without probing other versions.
            # Name 标识精确引用的文件名，不探测其他版本。
            $Name = [IO.Path]::GetFileName($Dependency)
            if ($Binary.FullName -ceq $LuaCurl -and $Name -ceq 'libcurl.4.dylib') {
                if ($Dependency -cnotin @('@rpath/libcurl.4.dylib', $SystemCurl)) {
                    throw "Unexpected Lua cURL dependency: $Dependency"
                }
                $Destination = $SystemCurl
            } elseif ($Dependency -cmatch '^/(?:usr/lib/|System/Library/Frameworks/)' -and $Dependency -cnotmatch '/\.\.(?:/|$)') {
                $Destination = $Dependency
            } else {
                if ($Name -cnotmatch '^[\w.-]+\.(?:dylib|so)$' -or
                    $Name -cmatch '^libcurl(?:\.\d+)*\.dylib$' -or
                    -not (Test-Path -LiteralPath (Join-Path $Libraries $Name) -PathType Leaf)) {
                    throw "Missing macOS Lua dependency $Dependency for $($Binary.FullName)."
                }
                $Destination = $Prefix + $Name
                if ($Dependency -cne $Destination -and
                    $Dependency -cnotmatch '^(?:@rpath/[^/]+$|/Users/runner/work/luaskills-packages/luaskills-packages/luaskills/third_party/deps/)') {
                    throw "Unexpected macOS Lua dependency path $Dependency for $($Binary.FullName)."
                }
            }
            $Expected += $Destination
            if ($Dependency -cne $Destination) {
                $Changes += @('-change', $Dependency, $Destination)
            }
        }
        if ($Binary.FullName -ceq $LuaCurl -and $SystemCurl -cnotin $Expected) {
            throw "Lua cURL has no link to $SystemCurl."
        }
        # Normalize dylib IDs as well. Aliases retain the identity of their canonical file.
        # 同时规范化动态库标识，别名保留其规范文件的标识。
        $ExpectedId = $Links.Id
        if ($Links.Id.Count -eq 1) {
            # Identity is the install name shared by copies of the same upstream library.
            # Identity 是同一上游库各副本共享的安装名。
            $Identity = [IO.Path]::GetFileName($Links.Id[0])
            if ($Links.Id[0] -ceq '/usr/local/lib/libluajit-5.1.2.dylib') {
                $Identity = 'libluajit-5.1.dylib'
            }
            if ($Identity -cnotmatch '^[\w.-]+\.(?:dylib|so)$' -or
                -not (Test-Path -LiteralPath (Join-Path $Libraries $Identity) -PathType Leaf)) {
                throw "Missing canonical macOS Lua library $Identity for $($Binary.FullName)."
            }
            $ExpectedId = @("@rpath/$Identity")
            if ($Links.Id[0] -cne $ExpectedId[0]) {
                $Changes += @('-id', $ExpectedId[0])
            }
        }
        if ($Changes.Count -eq 0) { continue }
        if (-not $Relocate) { throw "Unprepared macOS Lua binary $($Binary.FullName). Run make deps lua." }
        & install_name_tool @Changes $Binary.FullName
        if ($LASTEXITCODE -ne 0) { throw "Failed to relocate macOS Lua binary: $($Binary.FullName)" }
        # Editing Mach-O load commands invalidates the previous signature.
        # 编辑 Mach-O 加载命令会使原签名失效。
        & codesign --force --sign - $Binary.FullName
        if ($LASTEXITCODE -ne 0) { throw "Failed to sign macOS Lua binary: $($Binary.FullName)" }
        # Readback checks the actual binary instead of trusting the tool exit code alone.
        # 回读校验实际二进制，不只依赖工具退出码。
        $Readback = Get-MacLuaBinaryLinks -Binary $Binary.FullName
        if (($Readback.Dependencies -join "`n") -cne ($Expected -join "`n") -or
            ($Readback.Id -join "`n") -cne ($ExpectedId -join "`n")) {
            throw "Unexpected dependencies after relocating $($Binary.FullName)."
        }
    }
    foreach ($Binary in $UpstreamCurl) {
        Remove-Item -LiteralPath $Binary.FullName
    }
}

# Load the normalized modules with the packaged LuaJIT in a fresh native process.
# 在全新原生进程中用随包 LuaJIT 加载规范化后的模块。
# Root is the prepared runtime root. Returns normally after the smoke test succeeds.
# Root 是已准备的运行时根目录。冒烟测试成功后正常返回。
function Test-MacLuaNativeLoading {
    param([string]$Root)

    # Executable is a temporary probe built for the current PowerShell process architecture.
    # Executable 是为当前 PowerShell 进程架构构建的临时探针。
    $Executable = Join-Path ([IO.Path]::GetTempPath()) ('vulcan-lua-smoke-' + [Guid]::NewGuid().ToString('N'))
    # Architecture selects the same slice as the dependency installer, including Rosetta builds.
    # Architecture 选择与依赖安装器相同的架构，包括 Rosetta 构建。
    $Architecture = switch ([Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString()) {
        'Arm64' { 'arm64' }
        'X64' { 'x86_64' }
        default { throw 'Unsupported macOS Lua smoke test architecture.' }
    }
    try {
        & xcrun clang -arch $Architecture -Wall -Wextra -Werror -o $Executable (Join-Path $PSScriptRoot 'macos-lua-runtime-smoke.c')
        if ($LASTEXITCODE -ne 0) { throw 'Failed to build the macOS Lua loading probe.' }
        & $Executable (Join-Path $Root 'libs/libluajit-5.1.dylib') (Join-Path $Root 'lua_packages')
        if ($LASTEXITCODE -ne 0) { throw 'Failed to load or initialize the packaged macOS Lua modules.' }
    } finally {
        if (Test-Path -LiteralPath $Executable) { Remove-Item -LiteralPath $Executable }
    }
}

if ($PSBoundParameters.ContainsKey('RuntimeRoot')) {
    $ErrorActionPreference = 'Stop'
    Confirm-MacLuaRuntime -Root $RuntimeRoot
    if ($Smoke) { Test-MacLuaNativeLoading -Root $RuntimeRoot }
}
