#!/usr/bin/env sh

# install_rust_dev_tools.sh installs the Rust workspace maintenance tools used by this repository.
# install_rust_dev_tools.sh 用于安装本仓库使用的 Rust 工作区维护工具。
# The installer uses a small Cargo job count so first-time setup remains responsive on developer machines.
# 安装器使用较小的 Cargo 并发度，确保首次初始化不会让开发机失去响应。

set -eu

# extract_semantic_version parses the first three-part semantic version from tool output.
# extract_semantic_version 从工具输出中解析第一个三段语义版本。
# VersionText is the exact version output to inspect.
# VersionText 是需要检查的原始版本输出。
# Prints the parsed semantic version or an empty string when the output is unrecognized.
# 输出解析后的语义版本；无法识别输出时返回空字符串。
extract_semantic_version() {
    # VersionText stores the caller-provided version output.
    # VersionText 保存调用方传入的版本输出。
    VersionText=$1
    printf '%s\n' "$VersionText" | sed -n 's/^[^0-9]*\([0-9]\{1,\}\.[0-9]\{1,\}\.[0-9]\{1,\}\).*$/\1/p'
}

# semantic_version_at_least compares two three-part semantic versions numerically.
# semantic_version_at_least 以数值方式比较两个三段语义版本。
# CurrentVersion is the installed version and MinimumVersion is the required compatibility floor.
# CurrentVersion 是已安装版本，MinimumVersion 是所需兼容版本下限。
# Returns success when CurrentVersion is greater than or equal to MinimumVersion.
# 当 CurrentVersion 大于或等于 MinimumVersion 时返回成功。
semantic_version_at_least() {
    # CurrentVersion stores the installed semantic version.
    # CurrentVersion 保存已安装的语义版本。
    CurrentVersion=$1
    # MinimumVersion stores the repository compatibility floor.
    # MinimumVersion 保存仓库兼容版本下限。
    MinimumVersion=$2
    # The comparison evaluates major, minor, and patch components in order.
    # 比较逻辑依次计算主版本、次版本与补丁版本。
    awk -v current="$CurrentVersion" -v minimum="$MinimumVersion" 'BEGIN {
        split(current, current_parts, ".")
        split(minimum, minimum_parts, ".")
        for (component_index = 1; component_index <= 3; component_index++) {
            if ((current_parts[component_index] + 0) > (minimum_parts[component_index] + 0)) exit 0
            if ((current_parts[component_index] + 0) < (minimum_parts[component_index] + 0)) exit 1
        }
        exit 0
    }'
}

# install_cargo_dev_tool installs or upgrades one Cargo subcommand and validates its resulting version.
# install_cargo_dev_tool 安装或升级一个 Cargo 子命令，并验证最终版本。
# PackageName and BinaryName identify the package and executable; MinimumVersion and VersionRequirement define an optional compatibility floor.
# PackageName 与 BinaryName 标识包和可执行文件；MinimumVersion 与 VersionRequirement 定义可选的兼容版本下限。
# Returns after the executable exists, runs successfully, and satisfies the declared minimum version.
# 在可执行文件存在、运行成功并满足声明的最低版本后返回。
install_cargo_dev_tool() {
    # PackageName stores the crates.io package name.
    # PackageName 保存 crates.io 包名。
    PackageName=$1
    # BinaryName stores the executable name resolved from PATH.
    # BinaryName 保存从 PATH 解析的可执行文件名。
    BinaryName=$2
    # MinimumVersion stores the optional repository compatibility floor.
    # MinimumVersion 保存可选的仓库兼容版本下限。
    MinimumVersion=${3:-}
    # VersionRequirement stores the optional Cargo installation requirement.
    # VersionRequirement 保存可选的 Cargo 安装版本要求。
    VersionRequirement=${4:-}

    if command -v "$BinaryName" >/dev/null 2>&1; then
        # ExistingVersionText stores the exact output used for diagnostics and parsing.
        # ExistingVersionText 保存用于诊断与解析的原始输出。
        ExistingVersionText=$("$BinaryName" --version)
        echo "$ExistingVersionText"
        if [ -z "$MinimumVersion" ]; then
            echo "==> Rust developer tool already satisfies repository requirements: $BinaryName"
            return 0
        fi
        # ExistingVersion stores the parsed installed semantic version.
        # ExistingVersion 保存解析后的已安装语义版本。
        ExistingVersion=$(extract_semantic_version "$ExistingVersionText")
        if [ -z "$ExistingVersion" ]; then
            echo "Rust developer tool returned an unrecognized version: $BinaryName: $ExistingVersionText" >&2
            exit 1
        fi
        if semantic_version_at_least "$ExistingVersion" "$MinimumVersion"; then
            echo "==> Rust developer tool already satisfies repository requirements: $BinaryName"
            return 0
        fi
        echo "==> Rust developer tool requires an update: $BinaryName $ExistingVersion < $MinimumVersion"
    fi

    if ! command -v cargo >/dev/null 2>&1; then
        echo "cargo is required to install Rust developer tools. Install Rustup and the stable toolchain first." >&2
        exit 127
    fi

    echo "==> Installing Rust developer tool: $PackageName (Cargo jobs: 2)"
    if [ -n "$VersionRequirement" ]; then
        cargo install --locked --jobs 2 --version "$VersionRequirement" "$PackageName"
    else
        cargo install --locked --jobs 2 "$PackageName"
    fi

    if ! command -v "$BinaryName" >/dev/null 2>&1; then
        echo "Rust developer tool installation completed but executable is not available on PATH: $BinaryName" >&2
        exit 1
    fi

    # InstalledVersionText stores the exact post-install output used for validation.
    # InstalledVersionText 保存安装后用于验证的原始输出。
    InstalledVersionText=$("$BinaryName" --version)
    echo "$InstalledVersionText"
    if [ -n "$MinimumVersion" ]; then
        # InstalledVersion stores the parsed post-install semantic version.
        # InstalledVersion 保存解析后的安装后语义版本。
        InstalledVersion=$(extract_semantic_version "$InstalledVersionText")
        if [ -z "$InstalledVersion" ]; then
            echo "Rust developer tool returned an unrecognized version after installation: $BinaryName: $InstalledVersionText" >&2
            exit 1
        fi
        if ! semantic_version_at_least "$InstalledVersion" "$MinimumVersion"; then
            echo "Rust developer tool does not satisfy the repository minimum after installation: $BinaryName $InstalledVersion < $MinimumVersion" >&2
            exit 1
        fi
    fi
}

install_cargo_dev_tool "cargo-hakari" "cargo-hakari" "0.9.22" ">=0.9.22"
install_cargo_dev_tool "cargo-nextest" "cargo-nextest"

echo "==> Rust developer tools are ready."
