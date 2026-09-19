#!/usr/bin/env sh
# run_tauri.sh opens the Electron workbench and prepares Vite in development mode.
# run_tauri.sh 打开 Electron 工作台，并在开发模式准备 Vite。

set -eu

# ScriptDir stores this script directory for stable repository-root resolution.
# ScriptDir 保存当前脚本目录，确保仓库根目录解析稳定。
ScriptDir=$(CDPATH= cd "$(dirname "$0")" && pwd -P)

# ProjectDir stores the repository root for stable output paths.
# ProjectDir 保存仓库根目录，确保输出路径稳定。
ProjectDir=$(CDPATH= cd "$ScriptDir/.." && pwd -P)

# Release stores whether this run should launch the release output.
# Release 保存本次运行是否启动 release 产物。
Release=0

if [ "${1:-}" = "--release" ] || [ "${1:-}" = "release" ]; then
    Release=1
elif [ "${1:-}" != "" ]; then
    echo "Unsupported run_tauri.sh argument: $1" >&2
    exit 1
fi

# IsWindowsPlatform records whether the launched artifact requires the Windows executable suffix.
# IsWindowsPlatform 记录启动制品是否需要 Windows 可执行文件后缀。
IsWindowsPlatform=0
case "$(uname -s 2>/dev/null || printf unknown)" in
    MINGW*|MSYS*|CYGWIN*) IsWindowsPlatform=1 ;;
esac

# BinaryFileName stores the platform-specific Tauri shell binary name.
# BinaryFileName 保存平台相关的 Tauri 桌面壳二进制名称。
BinaryFileName=vulcan-code-service
if [ "$IsWindowsPlatform" = "1" ]; then
    BinaryFileName=vulcan-code-service.exe
fi

# OutputBinDir stores the stable runtime binary directory produced by make build.
# OutputBinDir 保存 make build 产出的稳定运行时二进制目录。
OutputBinDir="$ProjectDir/output/bin"

# OutputBinaryPath stores the exact executable launched by this script.
# OutputBinaryPath 保存本脚本启动的精确可执行文件路径。
OutputBinaryPath="$OutputBinDir/$BinaryFileName"

if [ ! -f "$OutputBinaryPath" ]; then
    # BuildCommand tells the user which build mode should produce the missing executable.
    # BuildCommand 告知用户应使用哪种构建模式产出缺失的可执行文件。
    BuildCommand="./make.sh build"
    if [ "$Release" = "1" ]; then
        BuildCommand="./make.sh build release"
    fi
    echo "Missing built Tauri binary: $OutputBinaryPath. Run $BuildCommand first." >&2
    exit 1
fi

# ProfileName describes the selected runtime profile in command output.
# ProfileName 在命令输出中描述选定的运行时 profile。
ProfileName=debug
if [ "$Release" = "1" ]; then
    ProfileName=release
fi

echo "==> Running Vulcan Code desktop $ProfileName..."
if [ "$Release" = "0" ]; then
    bash "$ProjectDir/scripts/vite_dev_server.sh" start
fi
node "$ProjectDir/scripts/desktop-runtime.mjs" run "$ProfileName"
