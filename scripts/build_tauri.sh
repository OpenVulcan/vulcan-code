#!/usr/bin/env sh
# build_tauri.sh builds the renamed service host and its WebUI assets.
# build_tauri.sh 用于构建重命名后的服务宿主及其 WebUI 产物.
# This legacy entry name is retained as a thin forwarder; the service no longer
# embeds a workbench window.

set -eu

# ScriptDir stores this script directory for stable repository-root resolution.
# ScriptDir 保存当前脚本目录，确保仓库根目录解析稳定。
ScriptDir=$(CDPATH= cd "$(dirname "$0")" && pwd -P)

# ProjectDir stores the repository root for stable child command paths.
# ProjectDir 保存仓库根目录，确保子命令路径稳定。
ProjectDir=$(CDPATH= cd "$ScriptDir/.." && pwd -P)

# OutputRootDir is the same validated distribution root used by sync and packaging.
# OutputRootDir 是同步与打包共同使用的已验证发行根目录。
OutputRootDir=$(node "$ProjectDir/scripts/desktop-runtime.mjs" root)

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

# Validate release dependency locks even when node_modules already exists.
# 即使 node_modules 已存在，也要验证发布依赖锁。
if [ "$Release" = "1" ] || [ ! -d "$WebAppDir/node_modules" ]; then
    pnpm --dir "$WebAppDir" install --frozen-lockfile
fi
if [ "$Release" = "1" ] || [ ! -d "$ProjectDir/apps/vulcan-code-desktop/node_modules" ]; then
    pnpm --dir "$ProjectDir/apps/vulcan-code-desktop" install --frozen-lockfile
fi

if [ "$Release" = "1" ]; then

    echo "==> Building WebUI for release..."
    pnpm --dir "$WebAppDir" run build

    echo "==> Building Vulcan Core release..."
    cargo build --locked -p vulcan-code-service -p vulcan-terminal-host --release
else
    echo "==> Building Vulcan Core debug..."
    cargo build --locked -p vulcan-code-service -p vulcan-terminal-host
fi

# CargoTargetDir stores Cargo's effective artifact root, including environment and config overrides.
# CargoTargetDir 保存 Cargo 实际使用的制品根目录，包括环境变量和配置覆盖。
CargoTargetDir=$(cargo metadata --locked --format-version 1 --no-deps | node -e "const fs=require('fs'); const metadata=JSON.parse(fs.readFileSync(0, 'utf8')); process.stdout.write(metadata.target_directory || '');")
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
BinaryFileName=vulcan-code-service
if [ "$IsWindowsPlatform" = "1" ]; then
    BinaryFileName=vulcan-code-service.exe
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

# OutputBinDir stores runtime executables beneath the selected distribution root.
# OutputBinDir 保存选定发行根目录下的运行时可执行文件。
OutputBinDir="$OutputRootDir/bin"

# OutputBinaryPath stores the copied executable path that users and run scripts should launch.
# OutputBinaryPath 保存用户与运行脚本应启动的已复制可执行文件路径。
OutputBinaryPath="$OutputBinDir/$BinaryFileName"

# OutputTerminalHostPath stores the isolated terminal host beside the shell binary.
# OutputTerminalHostPath 保存复制到桌面壳旁边的隔离终端宿主路径。
OutputTerminalHostPath="$OutputBinDir/$TerminalHostFileName"

if [ ! -f "$BuiltBinaryPath" ]; then
    echo "Built Core binary was not found: $BuiltBinaryPath" >&2
    exit 1
fi
if [ ! -f "$BuiltTerminalHostPath" ]; then
    echo "Built terminal host was not found: $BuiltTerminalHostPath" >&2
    exit 1
fi

# Debug distributions also serve a complete static WebUI through Core.
# 调试发行物同样通过 Core 提供完整静态网页。
if [ "$Release" = "0" ]; then
    pnpm --dir "$ProjectDir/apps/vulcan-code-web" run build
fi
# Stage and validate a complete generation before switching the published directory.
# 切换正式目录之前，暂存并验证完整一代。
node "$ProjectDir/scripts/publish-distribution.mjs" "$ProfileName" "$BuiltBinaryPath" "$BuiltTerminalHostPath"
