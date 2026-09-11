#!/usr/bin/env sh
# build_tauri.sh builds the Tauri/WebUI main application.
# build_tauri.sh 用于构建 Tauri/WebUI 主应用。

set -eu

# ScriptDir stores this script directory for stable repository-root resolution.
# ScriptDir 保存当前脚本目录，确保仓库根目录解析稳定。
ScriptDir=$(CDPATH= cd "$(dirname "$0")" && pwd -P)

# ProjectDir stores the repository root for stable child command paths.
# ProjectDir 保存仓库根目录，确保子命令路径稳定。
ProjectDir=$(CDPATH= cd "$ScriptDir/.." && pwd -P)

# Release stores whether this build should target Cargo release output.
# Release 保存本次构建是否面向 Cargo release 产物。
Release=0

if [ "${1:-}" = "--release" ] || [ "${1:-}" = "release" ]; then
    Release=1
elif [ "${1:-}" != "" ]; then
    echo "Unsupported build_tauri.sh argument: $1" >&2
    exit 1
fi

# WebAppDir points at the Vite React WebUI project.
# WebAppDir 指向 Vite React WebUI 工程。
WebAppDir="$ProjectDir/apps/vulcan-code-web"

# IsWindowsPlatform records whether the current shell host is Windows-like.
# IsWindowsPlatform 记录当前 shell 宿主是否类似 Windows。
IsWindowsPlatform=0

case "$(uname -s 2>/dev/null || printf unknown)" in
    MINGW*|MSYS*|CYGWIN*)
        IsWindowsPlatform=1
        ;;
esac

cd "$ProjectDir"

if [ "$Release" = "1" ]; then
    if [ ! -d "$WebAppDir/node_modules" ]; then
        echo "==> Installing WebUI dependencies..."
        pnpm --dir "$WebAppDir" install
    fi

    echo "==> Building WebUI for release..."
    pnpm --dir "$WebAppDir" run build

    echo "==> Building Tauri shell release..."
    cargo build -p vulcan-code-tauri -p vulcan-terminal-host --release --features custom-protocol
else
    echo "==> Building Tauri shell debug..."
    cargo build -p vulcan-code-tauri -p vulcan-terminal-host
fi

# CargoTargetDir stores Cargo's effective artifact root, including environment and config overrides.
# CargoTargetDir 保存 Cargo 实际使用的制品根目录，包括环境变量和配置覆盖。
CargoTargetDir=$(cargo metadata --format-version 1 --no-deps | node -e "const fs=require('fs'); const metadata=JSON.parse(fs.readFileSync(0, 'utf8')); process.stdout.write(metadata.target_directory || '');")
if [ -z "$CargoTargetDir" ]; then
    echo "Cargo metadata did not return target_directory." >&2
    exit 1
fi

# ProfileName selects the Cargo profile directory that contains the built shell.
# ProfileName 选择包含已构建桌面壳的 Cargo profile 目录。
ProfileName=debug
if [ "$Release" = "1" ]; then
    ProfileName=release
fi

# BinaryFileName stores the platform-specific Tauri shell binary name.
# BinaryFileName 保存平台相关的 Tauri 桌面壳二进制名称。
BinaryFileName=vulcan-code-tauri
if [ "$IsWindowsPlatform" = "1" ]; then
    BinaryFileName=vulcan-code-tauri.exe
fi

# BuiltBinaryPath stores the desktop shell under Cargo's effective artifact root.
# BuiltBinaryPath 保存 Cargo 实际制品根目录下的桌面壳可执行文件。
BuiltBinaryPath="$CargoTargetDir/$ProfileName/$BinaryFileName"

# TerminalHostFileName stores the platform-specific isolated terminal-host binary name.
# TerminalHostFileName 保存平台相关的隔离终端宿主二进制名称。
TerminalHostFileName=vulcan-terminal-host
if [ "$IsWindowsPlatform" = "1" ]; then
    TerminalHostFileName=vulcan-terminal-host.exe
fi

# BuiltTerminalHostPath stores the terminal host built for the shell's Cargo profile.
# BuiltTerminalHostPath 保存与桌面壳 Cargo profile 一致的终端宿主制品路径。
BuiltTerminalHostPath="$CargoTargetDir/$ProfileName/$TerminalHostFileName"

# OutputBinDir stores the stable runtime binary output directory used by make run.
# OutputBinDir 保存 make run 使用的稳定运行时二进制输出目录。
OutputBinDir="$ProjectDir/output/bin"

# OutputBinaryPath stores the copied executable path that users and run scripts should launch.
# OutputBinaryPath 保存用户与运行脚本应启动的已复制可执行文件路径。
OutputBinaryPath="$OutputBinDir/$BinaryFileName"

# OutputTerminalHostPath stores the isolated terminal host beside the shell binary.
# OutputTerminalHostPath 保存复制到桌面壳旁边的隔离终端宿主路径。
OutputTerminalHostPath="$OutputBinDir/$TerminalHostFileName"

if [ ! -f "$BuiltBinaryPath" ]; then
    echo "Built Tauri binary was not found: $BuiltBinaryPath" >&2
    exit 1
fi
if [ ! -f "$BuiltTerminalHostPath" ]; then
    echo "Built terminal host was not found: $BuiltTerminalHostPath" >&2
    exit 1
fi

# Publish compiled executables into the stable output directory used by make run.
# 把已编译可执行文件发布到 make run 使用的稳定 output 目录。
mkdir -p "$OutputBinDir"
# ObsoleteWindowsConsoleHelperPath identifies the removed pre-unification terminal executable.
# ObsoleteWindowsConsoleHelperPath 标识跨平台统一前已移除的旧终端可执行文件。
ObsoleteWindowsConsoleHelperPath="$OutputBinDir/vulcan-windows-console-helper.exe"
# Removing this generated artifact prevents stale dual-host layouts after an in-place upgrade.
# 删除此生成制品可避免原地升级后继续残留双宿主目录布局。
rm -f -- "$ObsoleteWindowsConsoleHelperPath"
cp "$BuiltBinaryPath" "$OutputBinaryPath"
cp "$BuiltTerminalHostPath" "$OutputTerminalHostPath"

echo "==> Done. Binary: $OutputBinaryPath"
echo "==> Done. Terminal host: $OutputTerminalHostPath"
