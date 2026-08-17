# build_tauri.ps1 builds the new Tauri/WebUI main application.
# build_tauri.ps1 用于构建新的 Tauri/WebUI 主应用。
# Usage:
# 用法：
#   .\build_tauri.ps1           # debug build
#   .\build_tauri.ps1 -Release  # release build

param(
    [switch]$Release
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
        & $PowerShellCore.Source -NoProfile -ExecutionPolicy Bypass -File $CurrentScriptPath @ForwardedArguments
        exit $LASTEXITCODE
    }
}

# ProjectDir stores the repository root for stable child command paths.
# ProjectDir 保存仓库根目录，确保子命令路径稳定。
$ProjectDir = Split-Path $PSScriptRoot -Parent

# ControllerProfileScriptPath points at the build-profile-aware generated Controller configuration updater.
# ControllerProfileScriptPath 指向按构建模式更新 Controller 生成配置的脚本。
$ControllerProfileScriptPath = Join-Path $PSScriptRoot "configure_controller_profile.ps1"

# WebAppDir points at the Vite React WebUI project.
# WebAppDir 指向 Vite React WebUI 工程。
$WebAppDir = Join-Path $ProjectDir "apps\vulcan-code-web"

# TauriAppDir points at the Tauri desktop shell crate.
# TauriAppDir 指向 Tauri 桌面壳 crate。
$TauriAppDir = Join-Path $ProjectDir "apps\vulcan-code-tauri"

# IsWindowsPlatform records whether the current build target is the Windows host platform.
# IsWindowsPlatform 记录当前构建目标是否为 Windows 宿主平台。
$IsWindowsPlatform = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)

Set-Location $ProjectDir

if (-not (Test-Path -LiteralPath $ControllerProfileScriptPath -PathType Leaf)) {
    throw "Missing Controller profile configuration script: $ControllerProfileScriptPath"
}
if ($Release) {
    & $ControllerProfileScriptPath -Release
}
else {
    & $ControllerProfileScriptPath
}
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

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

    Write-Host "==> Building Tauri shell release..."
    cargo build -p vulcan-code-tauri -p vulcan-terminal-host --release --features custom-protocol
}
else {
    Write-Host "==> Building Tauri shell debug..."
    cargo build -p vulcan-code-tauri -p vulcan-terminal-host
}
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

# ProfileName selects the Cargo profile directory that contains the built shell.
# ProfileName 选择包含已构建桌面壳的 Cargo profile 目录。
$ProfileName = if ($Release) { "release" } else { "debug" }

# CargoMetadataJson captures Cargo's authoritative workspace paths after applying environment and configuration overrides.
# CargoMetadataJson 捕获 Cargo 在应用环境与配置覆盖后的权威工作区路径。
$CargoMetadataJson = cargo metadata --format-version 1 --no-deps
if ($LASTEXITCODE -ne 0) {
    throw "Cargo metadata failed while resolving the build artifact directory."
}
# CargoMetadata parses the authoritative target directory instead of assuming a repository-local target folder.
# CargoMetadata 解析权威 target 目录，不再假定制品位于仓库内 target 目录。
$CargoMetadata = $CargoMetadataJson | ConvertFrom-Json
# CargoTargetDir stores the effective Cargo artifact root, including CARGO_TARGET_DIR overrides.
# CargoTargetDir 保存实际 Cargo 制品根目录，包括 CARGO_TARGET_DIR 覆盖。
$CargoTargetDir = [string]$CargoMetadata.target_directory
if ([string]::IsNullOrWhiteSpace($CargoTargetDir)) {
    throw "Cargo metadata did not return target_directory."
}

# BinaryFileName stores the platform-specific Tauri shell binary name.
# BinaryFileName 保存平台相关的 Tauri 桌面壳二进制名称。
$BinaryFileName = "vulcan-code-tauri"
if ($IsWindowsPlatform) {
    $BinaryFileName = "vulcan-code-tauri.exe"
}

# BuiltBinaryPath points at the shell artifact under Cargo's effective target directory.
# BuiltBinaryPath 指向 Cargo 实际 target 目录下的桌面壳制品。
$BuiltBinaryPath = Join-Path $CargoTargetDir "$ProfileName\$BinaryFileName"

# TerminalHostFileName stores the platform-specific isolated terminal-host binary name.
# TerminalHostFileName 保存平台相关的隔离终端宿主二进制名称。
$TerminalHostFileName = "vulcan-terminal-host"
if ($IsWindowsPlatform) {
    $TerminalHostFileName = "vulcan-terminal-host.exe"
}

# BuiltTerminalHostPath stores the terminal host built for the shell's Cargo profile.
# BuiltTerminalHostPath 保存与桌面壳 Cargo profile 一致的终端宿主制品路径。
$BuiltTerminalHostPath = Join-Path $CargoTargetDir "$ProfileName\$TerminalHostFileName"

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
