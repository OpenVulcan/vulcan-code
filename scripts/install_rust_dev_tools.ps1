#requires -Version 7.0

# install_rust_dev_tools.ps1 installs the Rust workspace maintenance tools used by this repository.
# install_rust_dev_tools.ps1 用于安装本仓库使用的 Rust 工作区维护工具。
# The installer uses a small Cargo job count so first-time setup remains responsive on developer machines.
# 安装器使用较小的 Cargo 并发度，确保首次初始化不会让开发机失去响应。

[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

# Get-CargoDevToolSemanticVersion reads and parses one Cargo developer tool semantic version.
# Get-CargoDevToolSemanticVersion 读取并解析一个 Cargo 开发工具的语义版本。
# Command is the resolved executable command to inspect.
# Command 是需要检查的已解析可执行命令。
# Returns the parsed three-part semantic version and throws when the command cannot report one.
# 返回解析后的三段语义版本；命令无法报告版本时抛出异常。
function Get-CargoDevToolSemanticVersion {
    param(
        [Parameter(Mandatory = $true)]
        [System.Management.Automation.CommandInfo]$Command
    )

    # VersionText stores the exact version output used for diagnostics and parsing.
    # VersionText 保存用于诊断与解析的原始版本输出。
    $VersionText = (& $Command.Source --version 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) {
        throw "Rust developer tool failed to report its version: $($Command.Name)"
    }
    Write-Host $VersionText

    # VersionMatch captures the first three-part semantic version from the tool output.
    # VersionMatch 捕获工具输出中的第一个三段语义版本。
    $VersionMatch = [regex]::Match($VersionText, '(?<major>\d+)\.(?<minor>\d+)\.(?<patch>\d+)')
    if (-not $VersionMatch.Success) {
        throw ("Rust developer tool returned an unrecognized version: {0}: {1}" -f $Command.Name, $VersionText)
    }

    # ParsedVersion stores the normalized version object returned to the caller.
    # ParsedVersion 保存返回给调用方的规范化版本对象。
    $ParsedVersion = [System.Version]("{0}.{1}.{2}" -f
        $VersionMatch.Groups['major'].Value,
        $VersionMatch.Groups['minor'].Value,
        $VersionMatch.Groups['patch'].Value)
    return $ParsedVersion
}

# Install-CargoDevTool installs or upgrades one Cargo subcommand and validates its resulting version.
# Install-CargoDevTool 安装或升级一个 Cargo 子命令，并验证最终版本。
# PackageName and BinaryName identify the crates.io package and executable; MinimumVersion and VersionRequirement define an optional compatibility floor.
# PackageName 与 BinaryName 标识 crates.io 包和可执行文件；MinimumVersion 与 VersionRequirement 定义可选的兼容版本下限。
# Returns after the executable exists, runs successfully, and satisfies the declared minimum version.
# 在可执行文件存在、运行成功并满足声明的最低版本后返回。
function Install-CargoDevTool {
    param(
        [Parameter(Mandatory = $true)]
        [string]$PackageName,
        [Parameter(Mandatory = $true)]
        [string]$BinaryName,
        [System.Version]$MinimumVersion,
        [string]$VersionRequirement
    )

    # ExistingCommand resolves a previously installed executable from PATH.
    # ExistingCommand 从 PATH 解析已经安装的可执行文件。
    $ExistingCommand = Get-Command $BinaryName -ErrorAction SilentlyContinue
    if ($ExistingCommand) {
        # ExistingVersion stores the verified semantic version of the installed executable.
        # ExistingVersion 保存已安装可执行文件经过验证的语义版本。
        $ExistingVersion = Get-CargoDevToolSemanticVersion -Command $ExistingCommand
        if (-not $MinimumVersion -or $ExistingVersion -ge $MinimumVersion) {
            Write-Host ("==> Rust developer tool already satisfies repository requirements: {0}" -f $BinaryName)
            return
        }
        Write-Host ("==> Rust developer tool requires an update: {0} {1} < {2}" -f $BinaryName, $ExistingVersion, $MinimumVersion)
    }

    # CargoCommand resolves the platform-appropriate Cargo executable from PATH.
    # CargoCommand 从 PATH 解析当前平台适用的 Cargo 可执行文件。
    $CargoCommand = Get-Command "cargo" -ErrorAction SilentlyContinue
    if (-not $CargoCommand) {
        throw "cargo is required to install Rust developer tools. Install Rustup and the stable toolchain first."
    }

    Write-Host ("==> Installing Rust developer tool: {0} (Cargo jobs: 2)" -f $PackageName)
    if ([string]::IsNullOrWhiteSpace($VersionRequirement)) {
        & $CargoCommand.Source install --locked --jobs 2 $PackageName
    } else {
        & $CargoCommand.Source install --locked --jobs 2 --version $VersionRequirement $PackageName
    }
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    # InstalledCommand resolves the executable again after Cargo completes installation.
    # InstalledCommand 在 Cargo 完成安装后再次解析可执行文件。
    $InstalledCommand = Get-Command $BinaryName -ErrorAction SilentlyContinue
    if (-not $InstalledCommand) {
        throw "Rust developer tool installation completed but executable is not available on PATH: $BinaryName"
    }

    # InstalledVersion stores the post-install semantic version used for the compatibility assertion.
    # InstalledVersion 保存安装后用于兼容性断言的语义版本。
    $InstalledVersion = Get-CargoDevToolSemanticVersion -Command $InstalledCommand
    if ($MinimumVersion -and $InstalledVersion -lt $MinimumVersion) {
        throw "Rust developer tool does not satisfy the repository minimum after installation: $BinaryName $InstalledVersion < $MinimumVersion"
    }
}

Install-CargoDevTool -PackageName "cargo-hakari" -BinaryName "cargo-hakari" -MinimumVersion ([System.Version]"0.9.22") -VersionRequirement ">=0.9.22"
Install-CargoDevTool -PackageName "cargo-nextest" -BinaryName "cargo-nextest"

Write-Host "==> Rust developer tools are ready."
