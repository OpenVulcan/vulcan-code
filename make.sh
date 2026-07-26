#!/usr/bin/env sh
# make.sh provides the Unix-like task entry for local build and run workflows.
# make.sh 用于提供 Unix-like 本地构建与运行工作流入口。

set -eu

# ScriptDir stores the repository root so child scripts are always resolved from a stable base path.
# ScriptDir 用于保存仓库根目录，确保子脚本始终从稳定的基路径解析。
ScriptDir=$(CDPATH= cd "$(dirname "$0")" && pwd -P)

# TauriBuildScriptPath points at the Unix-like Tauri/WebUI build script.
# TauriBuildScriptPath 用于指向 Unix-like Tauri/WebUI 构建脚本。
TauriBuildScriptPath="$ScriptDir/scripts/build_tauri.sh"

# TauriRunScriptPath points at the Unix-like Tauri/WebUI run script.
# TauriRunScriptPath 用于指向 Unix-like Tauri/WebUI 运行脚本。
TauriRunScriptPath="$ScriptDir/scripts/run_tauri.sh"

# PowerShellScriptPath points at the PowerShell task entry for shared legacy operations.
# PowerShellScriptPath 指向共享旧操作使用的 PowerShell 任务入口。
PowerShellScriptPath="$ScriptDir/make.ps1"

# CommandMode selects the top-level command such as build, run, release, or deps.
# CommandMode 用于选择顶层命令，例如 build、run、release 或 deps。
CommandMode=${1:-}

# CommandVariant carries the optional secondary mode such as release after run or host/lua after deps.
# CommandVariant 用于承接可选的二级模式，例如 run 后面的 release，或 deps 后面的 host/lua。
CommandVariant=${2:-}

# normalize_command converts command text into a trimmed lower-case token.
# normalize_command 用于把命令文本转换成裁剪后的小写标记。
normalize_command() {
    printf '%s' "${1:-}" | tr '[:upper:]' '[:lower:]'
}

# invoke_tauri_build forwards the current mode to scripts/build_tauri.sh.
# invoke_tauri_build 用于把当前模式转发给 scripts/build_tauri.sh。
invoke_tauri_build() {
    # IsRelease controls whether the Tauri shell should target Cargo release output.
    # IsRelease 用于控制 Tauri 桌面壳是否构建 Cargo release 产物。
    IsRelease=$1
    if [ ! -f "$TauriBuildScriptPath" ]; then
        echo "Missing Tauri build script: $TauriBuildScriptPath" >&2
        exit 1
    fi
    if [ "$IsRelease" = "1" ]; then
        sh "$TauriBuildScriptPath" --release
    else
        sh "$TauriBuildScriptPath"
    fi
}

# invoke_tauri_run forwards the current mode to scripts/run_tauri.sh.
# invoke_tauri_run 用于把当前模式转发给 scripts/run_tauri.sh。
invoke_tauri_run() {
    # IsRelease controls whether the runtime should execute a release Tauri shell.
    # IsRelease 用于控制运行时是否执行 release Tauri 桌面壳。
    IsRelease=$1
    if [ ! -f "$TauriRunScriptPath" ]; then
        echo "Missing Tauri run script: $TauriRunScriptPath" >&2
        exit 1
    fi
    if [ "$IsRelease" = "1" ]; then
        sh "$TauriRunScriptPath" --release
    else
        sh "$TauriRunScriptPath"
    fi
}

# invoke_tauri_build_and_run builds the shell first and then starts the matching runtime mode.
# invoke_tauri_build_and_run 用于先构建桌面壳，再启动匹配的运行模式。
invoke_tauri_build_and_run() {
    # IsRelease controls whether both build and run steps should target release output.
    # IsRelease 用于控制构建与运行两个步骤是否都面向 release 产物。
    IsRelease=$1
    invoke_tauri_build "$IsRelease"
    invoke_tauri_run "$IsRelease"
}

# invoke_powershell_route delegates commands that still intentionally share PowerShell implementations.
# invoke_powershell_route 委托仍刻意共享 PowerShell 实现的命令。
invoke_powershell_route() {
    if command -v pwsh >/dev/null 2>&1; then
        pwsh -NoProfile -ExecutionPolicy Bypass -File "$PowerShellScriptPath" "$@"
    elif command -v powershell.exe >/dev/null 2>&1; then
        powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$PowerShellScriptPath" "$@"
    else
        echo "PowerShell is required for this command because the matching dependency/license implementation is PowerShell-owned." >&2
        exit 127
    fi
}

# show_usage prints the supported command forms so contributors can recover from invalid input.
# show_usage 用于输出支持的命令形式，方便贡献者在输入无效参数后快速恢复。
show_usage() {
    echo "Usage:"
    echo "  ./make.sh             # Tauri/WebUI debug build"
    echo "  ./make.sh build       # Tauri/WebUI debug build"
    echo "  ./make.sh br          # build and run Tauri/WebUI debug"
    echo "  ./make.sh br release  # build and run Tauri/WebUI release"
    echo "  ./make.sh release     # Tauri/WebUI release build"
    echo "  ./make.sh licenses    # generate Rust third-party notices for the About page"
    echo "  ./make.sh run         # run Tauri/WebUI debug build"
    echo "  ./make.sh run release # run Tauri/WebUI release build"
    echo "  ./make.sh deps        # install host + official LuaSkills runtime dependencies"
    echo "  ./make.sh deps host   # install host native dependencies only"
    echo "  ./make.sh deps lua    # install official LuaSkills runtime dependencies only"
    echo "  ./make.sh deps cbm    # install the pinned native Codebase Memory runtime only"
}

# NormalizedMode stores the canonical top-level command token used by the dispatcher below.
# NormalizedMode 用于保存下方分发器使用的规范化顶层命令标记。
NormalizedMode=$(normalize_command "$CommandMode")

# NormalizedVariant stores the canonical secondary token used to distinguish release runs from default runs.
# NormalizedVariant 用于保存规范化的二级标记，以区分 release 运行与默认运行。
NormalizedVariant=$(normalize_command "$CommandVariant")

case "$NormalizedMode" in
    "")
        invoke_tauri_build 0
        ;;
    build)
        if [ "$NormalizedVariant" = "" ]; then
            invoke_tauri_build 0
        elif [ "$NormalizedVariant" = "release" ]; then
            invoke_tauri_build 1
        else
            echo "Unsupported build command: '$CommandVariant'" >&2
            show_usage
            exit 1
        fi
        ;;
    release)
        invoke_tauri_build 1
        ;;
    br)
        if [ "$NormalizedVariant" = "" ]; then
            invoke_tauri_build_and_run 0
        elif [ "$NormalizedVariant" = "release" ]; then
            invoke_tauri_build_and_run 1
        else
            echo "Unsupported br command: '$CommandVariant'" >&2
            show_usage
            exit 1
        fi
        ;;
    licenses)
        if [ "$NormalizedVariant" != "" ]; then
            echo "Unsupported licenses command: '$CommandVariant'" >&2
            show_usage
            exit 1
        fi
        invoke_powershell_route licenses
        ;;
    run)
        if [ "$NormalizedVariant" = "" ]; then
            invoke_tauri_run 0
        elif [ "$NormalizedVariant" = "release" ]; then
            invoke_tauri_run 1
        else
            echo "Unsupported run command: '$CommandVariant'" >&2
            show_usage
            exit 1
        fi
        ;;
    deps)
        case "$NormalizedVariant" in
            ""|all|host|lua|cbm)
                invoke_powershell_route deps "$NormalizedVariant"
                ;;
            *)
                echo "Unsupported deps command: '$CommandVariant'" >&2
                show_usage
                exit 1
                ;;
        esac
        ;;
    *)
        echo "Unsupported command: '$CommandMode'" >&2
        show_usage
        exit 1
        ;;
esac
