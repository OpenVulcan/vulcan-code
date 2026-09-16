# build_tauri.ps1 builds the new Tauri/WebUI main application.
# build_tauri.ps1 用于构建新的 Tauri/WebUI 主应用。
# Usage:
# 用法：
#   .\build_tauri.ps1           # debug build
#   .\build_tauri.ps1 -Release  # release build

param(
    # LocalLuaSkillsPath selects an explicit unpublished LuaSkills checkout for integration builds.
    # LocalLuaSkillsPath 为联调构建选择一个明确的未发布 LuaSkills 检出目录。
    [switch]$Release,
    [string]$LocalLuaSkillsPath = ""
)

$ErrorActionPreference = "Stop"

# Relaunch under PowerShell 7 when available so frontend and Rust commands share UTF-8 behavior.
# 若检测到 PowerShell 7，则自动在其下重启，确保前端与 Rust 命令共享稳定的 UTF-8 行为。
if ($PSVersionTable.PSEdition -ne "Core") {
    $CurrentScriptPath = $MyInvocation.MyCommand.Path
    if (-not [string]::IsNullOrWhiteSpace($PSCommandPath)) {
        $CurrentScriptPath = $PSCommandPath
    }
    $PowerShellCore = Get-Command "pwsh" -ErrorAction SilentlyContinue
    if ($PowerShellCore -and -not [string]::IsNullOrWhiteSpace($CurrentScriptPath)) {
        $ForwardedArguments = @()
        if ($Release) {
            $ForwardedArguments += "-Release"
        }
        if (-not [string]::IsNullOrWhiteSpace($LocalLuaSkillsPath)) {
            $ForwardedArguments += @("-LocalLuaSkillsPath", $LocalLuaSkillsPath)
        }
        & $PowerShellCore.Source -NoProfile -ExecutionPolicy Bypass -File $CurrentScriptPath @ForwardedArguments
        exit $LASTEXITCODE
    }
}

# ProjectDir stores the repository root for stable child command paths.
# ProjectDir 保存仓库根目录，确保子命令路径稳定。
$ProjectDir = Split-Path $PSScriptRoot -Parent

# Load the shared successful-build artifact tracker for both Cargo profiles.
# 加载两个 Cargo 模式共用的成功构建制品跟踪器。
. (Join-Path $PSScriptRoot 'cargo_artifact_cleanup.ps1')

# WebAppDir points at the Vite React WebUI project.
# WebAppDir 指向 Vite React WebUI 工程。
$WebAppDir = Join-Path $ProjectDir "apps\vulcan-code-web"

# TauriAppDir points at the Tauri desktop shell crate.
# TauriAppDir 指向 Tauri 桌面壳 crate。
$TauriAppDir = Join-Path $ProjectDir "apps\vulcan-code-tauri"

# CargoConfigArguments carries only the explicit local LuaSkills source override into Cargo.
# CargoConfigArguments 仅把明确指定的本地 LuaSkills 源码覆盖传入 Cargo。
$CargoConfigArguments = @()
# CargoLockPath identifies the workspace lock restored after local integration builds.
# CargoLockPath 标识本地联调构建后需要恢复的工作区锁文件。
$CargoLockPath = Join-Path $ProjectDir "Cargo.lock"
# CargoLockBackupPath is shared by every local-source build entry so only one may own Cargo.lock.
# CargoLockBackupPath 由所有本地源码构建入口共享，确保同一时刻只有一个入口持有 Cargo.lock。
$CargoLockBackupPath = Join-Path $ProjectDir "Cargo.lock.local-luaskills-backup"
# CargoLockMetadataPath records the shared backup owner and baseline digest for safe recovery.
# CargoLockMetadataPath 记录共享备份的所有者与基线摘要，供安全恢复使用。
$CargoLockMetadataPath = Join-Path $ProjectDir "Cargo.lock.local-luaskills-backup.json"
# CargoLockBaselineSha256 retains the authoritative digest used to verify restoration.
# CargoLockBaselineSha256 保留用于验证恢复结果的权威摘要。
$CargoLockBaselineSha256 = ""

if (-not [string]::IsNullOrWhiteSpace($LocalLuaSkillsPath)) {
    # RequestedLuaSkillsPath anchors relative input to the repository instead of the caller's current directory.
    # RequestedLuaSkillsPath 将相对输入锚定到仓库，而不是调用方的当前目录。
    $RequestedLuaSkillsPath = $LocalLuaSkillsPath
    if (-not [System.IO.Path]::IsPathRooted($LocalLuaSkillsPath)) {
        $RequestedLuaSkillsPath = Join-Path $ProjectDir $LocalLuaSkillsPath
    }
    # CanonicalLuaSkillsPath proves the requested checkout exists before Cargo receives it.
    # CanonicalLuaSkillsPath 在 Cargo 接收路径前证明请求的检出目录存在。
    $CanonicalLuaSkillsPath = (Resolve-Path -LiteralPath $RequestedLuaSkillsPath -ErrorAction Stop).Path
    # LuaSkillsManifestPath is the exact package manifest required by the patch source.
    # LuaSkillsManifestPath 是 patch 源必须包含的精确包清单。
    $LuaSkillsManifestPath = Join-Path $CanonicalLuaSkillsPath "Cargo.toml"
    if (-not (Test-Path -LiteralPath $LuaSkillsManifestPath -PathType Leaf)) {
        throw "Local LuaSkills checkout is missing Cargo.toml: $CanonicalLuaSkillsPath"
    }
    # CargoLuaSkillsPath uses forward slashes so the inline TOML string needs no platform-specific escaping.
    # CargoLuaSkillsPath 使用正斜杠，使内联 TOML 字符串不需要平台相关转义。
    $CargoLuaSkillsPath = $CanonicalLuaSkillsPath.Replace('\', '/')
    # CargoPatchConfig is the exact Cargo configuration entry for this one build process.
    # CargoPatchConfig 是仅供本次构建进程使用的精确 Cargo 配置项。
    $CargoPatchConfig = 'patch.crates-io.luaskills.path="{0}"' -f $CargoLuaSkillsPath
    $CargoConfigArguments = @('--config', $CargoPatchConfig)
}

# IsWindowsPlatform records whether the current build target is the Windows host platform.
# IsWindowsPlatform 记录当前构建目标是否为 Windows 宿主平台。
$IsWindowsPlatform = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)

Set-Location $ProjectDir

# Test whether one PowerShell file-operation failure is a Windows sharing or lock violation.
# 判断一次 PowerShell 文件操作失败是否属于 Windows 共享冲突或锁冲突。
# ErrorRecord is the exact caught PowerShell error, including nested native exceptions.
# ErrorRecord 是捕获到的精确 PowerShell 错误，包含嵌套的本机异常。
# This function returns true only for Win32 error 32 or 33 and never masks other failures.
# 此函数仅对 Win32 错误 32 或 33 返回 true，绝不掩盖其他失败。
function Test-IsLockedFileError {
    param(
        [System.Management.Automation.ErrorRecord]$ErrorRecord
    )

    # CurrentException walks every nested exception because Copy-Item may wrap the native IOException.
    # CurrentException 遍历每层嵌套异常，因为 Copy-Item 可能包装本机 IOException。
    $CurrentException = $ErrorRecord.Exception
    while ($null -ne $CurrentException) {
        # NativeErrorCode extracts the low Win32 status word from the HRESULT.
        # NativeErrorCode 从 HRESULT 中提取低位 Win32 状态码。
        $NativeErrorCode = $CurrentException.HResult -band 0xFFFF
        if ($NativeErrorCode -eq 32 -or $NativeErrorCode -eq 33) {
            return $true
        }
        $CurrentException = $CurrentException.InnerException
    }
    return $false
}

# Copy one file while allowing an already-running application to retain a locked destination version.
# 复制单个文件，同时允许运行中的应用继续保留被锁定的目标版本。
# SourceFile is the exact source artifact and DestinationFile is its exact output path.
# SourceFile 是精确源制品，DestinationFile 是其精确输出路径。
# This function returns true when copied, false when skipped for a lock, and throws for every other error.
# 此函数复制成功时返回 true，因占用跳过时返回 false，其他任何错误都会抛出。
function Copy-FileAllowLockedDestination {
    param(
        [string]$SourceFile,
        [string]$DestinationFile
    )

    # DestinationParent is created before the guarded copy so missing-directory failures remain explicit.
    # DestinationParent 在受保护复制前创建，使目录缺失类失败保持显式。
    $DestinationParent = Split-Path $DestinationFile -Parent
    New-Item -ItemType Directory -Path $DestinationParent -Force | Out-Null
    try {
        Copy-Item -LiteralPath $SourceFile -Destination $DestinationFile -Force -ErrorAction Stop
        return $true
    }
    catch {
        if (Test-IsLockedFileError -ErrorRecord $_) {
            Write-Warning "Skipped locked output file; restart the running application before using the new artifact: $DestinationFile"
            return $false
        }
        throw
    }
}

if ($Release) {
    if (-not (Test-Path -LiteralPath (Join-Path $WebAppDir "node_modules"))) {
        Write-Host "==> Installing WebUI dependencies..."
        pnpm --dir $WebAppDir install
    }

    # Release builds package the static WebUI assets into the desktop shell.
    # Release 构建会把静态 WebUI 产物打包进桌面壳。
    Write-Host "==> Building WebUI for release..."
    pnpm --dir $WebAppDir run build
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }

    # CargoBuildArguments selects the release shell, terminal host, and optional local-source patch.
    # CargoBuildArguments 选择 release 桌面壳、终端宿主以及可选的本地源码 patch。
    $CargoBuildArguments = @('build', '-p', 'vulcan-code-tauri', '-p', 'vulcan-terminal-host', '--release', '--features', 'custom-protocol') + $CargoConfigArguments
}
else {
    # CargoBuildArguments selects the debug shell, terminal host, and optional local-source patch.
    # CargoBuildArguments 选择 debug 桌面壳、终端宿主以及可选的本地源码 patch。
    $CargoBuildArguments = @('build', '-p', 'vulcan-code-tauri', '-p', 'vulcan-terminal-host') + $CargoConfigArguments
}

if ($CargoConfigArguments.Count -gt 0) {
    if ((Test-Path -LiteralPath $CargoLockBackupPath) -or (Test-Path -LiteralPath $CargoLockMetadataPath)) {
        throw "Another local LuaSkills source operation owns Cargo.lock. Finish or restore it before retrying: $CargoLockBackupPath"
    }
    # File.Copy with overwrite disabled is the atomic ownership claim shared with the focused build entry.
    # 禁止覆盖的 File.Copy 是与定向构建入口共享的原子所有权声明。
    [System.IO.File]::Copy($CargoLockPath, $CargoLockBackupPath, $false)
    $CargoLockBaselineSha256 = (Get-FileHash -LiteralPath $CargoLockBackupPath -Algorithm SHA256).Hash
    # CargoLockMetadata persists enough identity for the recovery command to reject unsafe overwrites.
    # CargoLockMetadata 持久化恢复命令拒绝不安全覆盖所需的身份信息。
    $CargoLockMetadata = [ordered]@{
        owner = "tauri-build"
        baselineSha256 = $CargoLockBaselineSha256
        localSha256 = $null
    }
    $CargoLockMetadata | ConvertTo-Json | Set-Content -LiteralPath $CargoLockMetadataPath -Encoding utf8NoBOM
    # Resolve the patch before the build and freeze the only non-baseline lock digest
    # this owner may restore over. A later third digest proves concurrent modification.
    # 在构建前解析 patch，并冻结本所有者唯一可恢复覆盖的非基线锁摘要。之后出现第三种摘要
    # 即证明发生了并发修改。
    & cargo metadata --format-version 1 @CargoConfigArguments *> $null
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo failed to prepare the local LuaSkills lock resolution. Recovery evidence remains at $CargoLockBackupPath"
    }
    $CargoLockMetadata.localSha256 = (Get-FileHash -LiteralPath $CargoLockPath -Algorithm SHA256).Hash
    $CargoLockMetadata | ConvertTo-Json | Set-Content -LiteralPath $CargoLockMetadataPath -Encoding utf8NoBOM
    Write-Host "==> Building with local LuaSkills source: $CanonicalLuaSkillsPath"
}
else {
    Write-Host "==> Building with registry LuaSkills dependency."
}

# BuildProfileName names the selected artifact profile in diagnostics.
# BuildProfileName 在诊断中标识所选制品 profile。
$BuildProfileName = if ($Release) { 'release' } else { 'debug' }
Write-Host "==> Building Tauri shell $BuildProfileName..."
try {
    # CargoBuildResult retains the exit status and authoritative artifact paths for cleanup and publishing.
    # CargoBuildResult 保留退出状态及权威制品路径，供清理和发布使用。
    $CargoBuildResult = Invoke-TrackedCargoBuild -BuildArguments $CargoBuildArguments
}
finally {
    if ($CargoConfigArguments.Count -gt 0 -and (Test-Path -LiteralPath $CargoLockBackupPath)) {
        # CurrentLockSha256 decides whether this owner may restore without overwriting
        # another process: only the frozen baseline and prepared local resolution are owned.
        # CurrentLockSha256 判断本所有者能否在不覆盖其他进程的情况下恢复：只有已冻结的基线与
        # 预备本地解析形态属于本所有者。
        $CurrentLockSha256 = (Get-FileHash -LiteralPath $CargoLockPath -Algorithm SHA256).Hash
        if ($CurrentLockSha256 -eq $CargoLockBaselineSha256) {
            Remove-Item -LiteralPath $CargoLockBackupPath -Force
            Remove-Item -LiteralPath $CargoLockMetadataPath -Force -ErrorAction SilentlyContinue
            Write-Host "==> Cargo.lock already matches the registry baseline."
        }
        elseif ($CargoLockMetadata.localSha256 -and $CurrentLockSha256 -eq $CargoLockMetadata.localSha256) {
            Copy-Item -LiteralPath $CargoLockBackupPath -Destination $CargoLockPath -Force
            # RestoredLockSha256 proves the ordinary registry lock returned byte-for-byte.
            # RestoredLockSha256 证明普通注册表锁文件已逐字节恢复。
            $RestoredLockSha256 = (Get-FileHash -LiteralPath $CargoLockPath -Algorithm SHA256).Hash
            if ($RestoredLockSha256 -ne $CargoLockBaselineSha256) {
                throw "Cargo.lock restoration digest mismatch after local LuaSkills build. Backup retained at $CargoLockBackupPath"
            }
            Remove-Item -LiteralPath $CargoLockBackupPath -Force
            Remove-Item -LiteralPath $CargoLockMetadataPath -Force -ErrorAction SilentlyContinue
            Write-Host "==> Restored Cargo.lock after local LuaSkills source resolution."
        }
        else {
            throw "Cargo.lock changed during the local LuaSkills build; refusing to overwrite concurrent work. Recovery evidence remains at $CargoLockBackupPath"
        }
    }
}
if ($CargoBuildResult.ExitCode -ne 0) {
    exit $CargoBuildResult.ExitCode
}

# BinaryFileName stores the platform-specific Tauri shell binary name.
# BinaryFileName 保存平台相关的 Tauri 桌面壳二进制名称。
$BinaryFileName = "vulcan-code-tauri"
if ($IsWindowsPlatform) {
    $BinaryFileName = "vulcan-code-tauri.exe"
}

# BuiltBinaryPath uses Cargo's executable path, including configured target triples.
# BuiltBinaryPath 使用 Cargo 的可执行文件路径，包括配置的目标三元组。
$BuiltBinaryPath = [string]($CargoBuildResult.Artifacts | Where-Object { $_.target.name -eq 'vulcan-code-tauri' -and $_.target.kind -contains 'bin' }).executable

# TerminalHostFileName stores the platform-specific isolated terminal-host binary name.
# TerminalHostFileName 保存平台相关的隔离终端宿主二进制名称。
$TerminalHostFileName = "vulcan-terminal-host"
if ($IsWindowsPlatform) {
    $TerminalHostFileName = "vulcan-terminal-host.exe"
}

# BuiltTerminalHostPath stores the terminal host built for the shell's Cargo profile.
# BuiltTerminalHostPath 保存与桌面壳 Cargo profile 一致的终端宿主制品路径。
$BuiltTerminalHostPath = [string]($CargoBuildResult.Artifacts | Where-Object { $_.target.name -eq 'vulcan-terminal-host' -and $_.target.kind -contains 'bin' }).executable

# OutputBinDir stores the stable runtime binary output directory used by make run.
# OutputBinDir 保存 make run 使用的稳定运行时二进制输出目录。
$OutputBinDir = Join-Path $ProjectDir "output\bin"

# OutputBinaryPath stores the copied executable path that users and run scripts should launch.
# OutputBinaryPath 保存用户与运行脚本应启动的已复制可执行文件路径。
$OutputBinaryPath = Join-Path $OutputBinDir $BinaryFileName

# OutputTerminalHostPath stores the isolated terminal host beside the shell binary.
# OutputTerminalHostPath 保存复制到桌面壳旁边的隔离终端宿主路径。
$OutputTerminalHostPath = Join-Path $OutputBinDir $TerminalHostFileName

if (-not (Test-Path -LiteralPath $BuiltBinaryPath)) {
    throw "Built Tauri binary was not found: $BuiltBinaryPath"
}
if (-not (Test-Path -LiteralPath $BuiltTerminalHostPath)) {
    throw "Built terminal host was not found: $BuiltTerminalHostPath"
}

# Publish compiled executables into the stable output directory used by make run.
# 把已编译可执行文件发布到 make run 使用的稳定 output 目录。
New-Item -ItemType Directory -Path $OutputBinDir -Force | Out-Null
# ObsoleteWindowsConsoleHelperPath identifies the removed pre-unification terminal executable.
# ObsoleteWindowsConsoleHelperPath 标识跨平台统一前已移除的旧终端可执行文件。
$ObsoleteWindowsConsoleHelperPath = Join-Path $OutputBinDir "vulcan-windows-console-helper.exe"
if (Test-Path -LiteralPath $ObsoleteWindowsConsoleHelperPath) {
    # Removing this generated artifact prevents stale dual-host layouts after an in-place upgrade.
    # 删除此生成制品可避免原地升级后继续残留双宿主目录布局。
    Remove-Item -LiteralPath $ObsoleteWindowsConsoleHelperPath -Force
}
# BinaryCopied records whether the running desktop process allowed the executable to be refreshed.
# BinaryCopied 记录运行中的桌面进程是否允许刷新可执行文件。
$BinaryCopied = Copy-FileAllowLockedDestination -SourceFile $BuiltBinaryPath -DestinationFile $OutputBinaryPath
# TerminalHostCopied records whether the isolated terminal host was refreshed.
# TerminalHostCopied 记录隔离终端宿主是否已刷新。
$TerminalHostCopied = Copy-FileAllowLockedDestination -SourceFile $BuiltTerminalHostPath -DestinationFile $OutputTerminalHostPath

if ($BinaryCopied) {
    Write-Host "==> Done. Binary: $OutputBinaryPath"
}
else {
    Write-Host "==> Done. Binary remained unchanged because it is in use: $OutputBinaryPath"
}
if ($TerminalHostCopied) {
    Write-Host "==> Done. Terminal host: $OutputTerminalHostPath"
}
else {
    Write-Host "==> Done. Terminal host remained unchanged because it is in use: $OutputTerminalHostPath"
}
