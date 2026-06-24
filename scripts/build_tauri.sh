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

# SourceConfigDir points at the repository-owned packaged configuration templates.
# SourceConfigDir 指向仓库持有的打包配置模板目录。
SourceConfigDir="$ProjectDir/config"

# SourceResourcesDir points at the repository-owned runtime resource catalog.
# SourceResourcesDir 指向仓库持有的运行时资源目录。
SourceResourcesDir="$ProjectDir/crates/vulcan-runtime-assets/resources"

# SourceSystemPromptSkillsDir points at repository-owned system Agent Skills.
# SourceSystemPromptSkillsDir 指向仓库持有的系统 Agent Skills 目录。
SourceSystemPromptSkillsDir="$ProjectDir/crates/vulcan-prompt-skills/src/assets/system"

# IsWindowsPlatform records whether the current shell host is Windows-like.
# IsWindowsPlatform 记录当前 shell 宿主是否类似 Windows。
IsWindowsPlatform=0

case "$(uname -s 2>/dev/null || printf unknown)" in
    MINGW*|MSYS*|CYGWIN*)
        IsWindowsPlatform=1
        ;;
esac

# copy_directory_contents overlays one source directory onto one output directory.
# copy_directory_contents 用于把一个源目录覆盖同步到一个输出目录。
copy_directory_contents() {
    # SourceDirectory is the directory whose immediate children should be copied.
    # SourceDirectory 表示需要复制其直接子项的源目录。
    SourceDirectory=$1
    # DestinationDirectory is the output directory that receives the copied children.
    # DestinationDirectory 表示接收复制子项的输出目录。
    DestinationDirectory=$2
    # Clean removes the existing destination before copying.
    # Clean 表示复制前先删除已有目标目录。
    Clean=${3:-0}

    if [ ! -d "$SourceDirectory" ]; then
        echo "Missing source directory: $SourceDirectory" >&2
        exit 1
    fi

    if [ "$Clean" = "1" ] && [ -e "$DestinationDirectory" ]; then
        case "$DestinationDirectory" in
            "$ProjectDir"/output/*)
                rm -rf "$DestinationDirectory"
                ;;
            *)
                echo "Refusing to clean path outside output: $DestinationDirectory" >&2
                exit 1
                ;;
        esac
    fi

    mkdir -p "$DestinationDirectory"
    cp -R "$SourceDirectory"/. "$DestinationDirectory"/
}

cd "$ProjectDir"

if [ "$Release" = "1" ]; then
    if [ ! -d "$WebAppDir/node_modules" ]; then
        echo "==> Installing WebUI dependencies..."
        pnpm --dir "$WebAppDir" install
    fi

    echo "==> Building WebUI for release..."
    pnpm --dir "$WebAppDir" run build

    echo "==> Building Tauri shell release..."
    if [ "$IsWindowsPlatform" = "1" ]; then
        cargo build -p vulcan-code-tauri -p vulcan-windows-console-helper --release
    else
        cargo build -p vulcan-code-tauri --release
    fi
else
    echo "==> Building Tauri shell debug..."
    if [ "$IsWindowsPlatform" = "1" ]; then
        cargo build -p vulcan-code-tauri -p vulcan-windows-console-helper
    else
        cargo build -p vulcan-code-tauri
    fi
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

# BuiltBinaryPath stores the Cargo-built desktop shell executable.
# BuiltBinaryPath 保存 Cargo 构建出的桌面壳可执行文件。
BuiltBinaryPath="$ProjectDir/target/$ProfileName/$BinaryFileName"

# HelperFileName stores the platform-specific Windows console helper binary name.
# HelperFileName 保存平台相关的 Windows 控制台 helper 二进制名称。
HelperFileName=vulcan-windows-console-helper
if [ "$IsWindowsPlatform" = "1" ]; then
    HelperFileName=vulcan-windows-console-helper.exe
fi

# BuiltHelperPath stores the helper executable built for the same Cargo profile as the shell.
# BuiltHelperPath 保存与桌面壳相同 Cargo profile 构建出的 helper 可执行文件。
BuiltHelperPath="$ProjectDir/target/$ProfileName/$HelperFileName"

# OutputBinDir stores the stable runtime binary output directory used by make run.
# OutputBinDir 保存 make run 使用的稳定运行时二进制输出目录。
OutputBinDir="$ProjectDir/output/bin"

# OutputBinaryPath stores the copied executable path that users and run scripts should launch.
# OutputBinaryPath 保存用户与运行脚本应启动的已复制可执行文件路径。
OutputBinaryPath="$OutputBinDir/$BinaryFileName"

# OutputHelperPath stores the copied Windows console helper beside the shell binary.
# OutputHelperPath 保存复制到桌面壳旁边的 Windows 控制台 helper。
OutputHelperPath="$OutputBinDir/$HelperFileName"

# OutputConfigDir stores the packaged configuration read by output/bin runtime launches.
# OutputConfigDir 保存 output/bin 运行时启动读取的打包配置目录。
OutputConfigDir="$ProjectDir/output/config"

# OutputResourcesDir stores the packaged resource catalog read by output/bin runtime launches.
# OutputResourcesDir 保存 output/bin 运行时启动读取的打包资源目录。
OutputResourcesDir="$ProjectDir/output/resources"

# OutputSystemPromptSkillsDir stores packaged system Agent Skills read directly by the runtime.
# OutputSystemPromptSkillsDir 保存运行时直接读取的打包系统 Agent Skills。
OutputSystemPromptSkillsDir="$OutputResourcesDir/prompt-skills/system"

if [ ! -f "$BuiltBinaryPath" ]; then
    echo "Built Tauri binary was not found: $BuiltBinaryPath" >&2
    exit 1
fi
if [ "$IsWindowsPlatform" = "1" ] && [ ! -f "$BuiltHelperPath" ]; then
    echo "Built Windows console helper was not found: $BuiltHelperPath" >&2
    exit 1
fi

mkdir -p "$OutputBinDir"
cp "$BuiltBinaryPath" "$OutputBinaryPath"
if [ "$IsWindowsPlatform" = "1" ]; then
    cp "$BuiltHelperPath" "$OutputHelperPath"
fi

echo "==> Syncing packaged config..."
copy_directory_contents "$SourceConfigDir" "$OutputConfigDir" 0

echo "==> Syncing packaged resources..."
copy_directory_contents "$SourceResourcesDir" "$OutputResourcesDir" 0

echo "==> Syncing packaged system Agent Skills..."
copy_directory_contents "$SourceSystemPromptSkillsDir" "$OutputSystemPromptSkillsDir" 1

echo "==> Done. Binary: $OutputBinaryPath"
if [ "$IsWindowsPlatform" = "1" ]; then
    echo "==> Done. Windows console helper: $OutputHelperPath"
fi
