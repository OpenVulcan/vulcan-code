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

# TauriSyncScriptPath points at the dedicated Unix-like runtime-resource synchronization script.
# TauriSyncScriptPath 用于指向专用的 Unix-like 运行时资源同步脚本。
TauriSyncScriptPath="$ScriptDir/scripts/sync_tauri_runtime.sh"

# ViteDevServerScriptPath points at the standalone Unix-like WebUI development-server lifecycle script.
# ViteDevServerScriptPath 用于指向独立 Unix-like WebUI 开发服务器生命周期脚本。
ViteDevServerScriptPath="$ScriptDir/scripts/vite_dev_server.sh"

# TauriRunScriptPath points at the Unix-like Tauri/WebUI run script.
# TauriRunScriptPath 用于指向 Unix-like Tauri/WebUI 运行脚本。
TauriRunScriptPath="$ScriptDir/scripts/run_tauri.sh"

# PowerShellScriptPath points at the PowerShell task entry for shared legacy operations.
# PowerShellScriptPath 指向共享旧操作使用的 PowerShell 任务入口。
PowerShellScriptPath="$ScriptDir/make.ps1"

# BundledToolsDepsScriptPath points at the native Unix-like sg and rg dependency installer.
# BundledToolsDepsScriptPath 指向 Unix-like 原生 sg 与 rg 依赖安装器。
BundledToolsDepsScriptPath="$ScriptDir/scripts/install_bundled_tools.sh"

# RustDevToolsDepsScriptPath points at the native Unix-like Cargo developer-tool installer.
# RustDevToolsDepsScriptPath 指向 Unix-like 原生 Cargo 开发工具安装器。
RustDevToolsDepsScriptPath="$ScriptDir/scripts/install_rust_dev_tools.sh"

# CommandMode selects the top-level command such as build, run, release, or deps.
# CommandMode 用于选择顶层命令，例如 build、run、release 或 deps。
CommandMode=${1:-}

# CommandVariant carries a secondary mode such as a Vite action, release after run, or a dependency kind.
# CommandVariant 用于承接二级模式，例如 Vite 操作、run 后面的 release 或依赖类型。
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

# invoke_tauri_sync synchronizes runtime resources without compiling the application.
# invoke_tauri_sync 用于在不编译应用的情况下同步运行时资源。
# The function accepts no arguments and returns the child synchronization status.
# 此函数不接收参数，并返回子同步脚本的状态。
invoke_tauri_sync() {
    if [ ! -f "$TauriSyncScriptPath" ]; then
        echo "Missing Tauri runtime synchronization script: $TauriSyncScriptPath" >&2
        exit 1
    fi
    sh "$TauriSyncScriptPath"
}

# invoke_vite_dev_server delegates one explicit standalone Vite lifecycle action.
# invoke_vite_dev_server 用于委托一个显式的独立 Vite 生命周期操作。
# The function accepts start, stop, or status and returns the child script status.
# 此函数接收 start、stop 或 status，并返回子脚本状态。
invoke_vite_dev_server() {
    # Action selects the exact standalone Vite lifecycle operation.
    # Action 用于选择精确的独立 Vite 生命周期操作。
    Action=$1
    if [ ! -f "$ViteDevServerScriptPath" ]; then
        echo "Missing Vite development-server script: $ViteDevServerScriptPath" >&2
        exit 1
    fi
    sh "$ViteDevServerScriptPath" "$Action"
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

# invoke_bundled_tools_deps installs latest sg and rg payloads without requiring PowerShell.
# invoke_bundled_tools_deps 在不依赖 PowerShell 的情况下安装最新版 sg 与 rg 载荷。
invoke_bundled_tools_deps() {
    if [ ! -f "$BundledToolsDepsScriptPath" ]; then
        echo "Missing bundled-tools dependency script: $BundledToolsDepsScriptPath" >&2
        exit 1
    fi
    sh "$BundledToolsDepsScriptPath"
}

# invoke_rust_dev_tools_deps installs and validates the Cargo tools required by workspace maintenance.
# invoke_rust_dev_tools_deps 安装并验证工作区维护所需的 Cargo 工具。
invoke_rust_dev_tools_deps() {
    if [ ! -f "$RustDevToolsDepsScriptPath" ]; then
        echo "Missing Rust developer-tools dependency script: $RustDevToolsDepsScriptPath" >&2
        exit 1
    fi
    sh "$RustDevToolsDepsScriptPath"
}

# invoke_system_initialization bootstraps every repository dependency in the documented order.
# invoke_system_initialization 按文档约定顺序引导全部仓库依赖。
invoke_system_initialization() {
    invoke_rust_dev_tools_deps
    invoke_powershell_route deps host
    invoke_bundled_tools_deps
    invoke_powershell_route deps lua
    # Prepare pinned Node prerequisites before browser synchronization, without starting Core.
    # 在同步浏览器前准备锁定的 Node 前置依赖，无需启动 Core。
    node "$ScriptDir/scripts/managed-node.mjs" "$ScriptDir/output"
    echo "==> System initialization completed."
}

# show_usage prints the supported command forms so contributors can recover from invalid input.
# show_usage 用于输出支持的命令形式，方便贡献者在输入无效参数后快速恢复。
show_usage() {
    echo "Usage:"
    echo "  ./make.sh sync        # synchronize runtime resources into output"
    echo "  ./make.sh vite start  # start the standalone WebUI Vite dev server"
    echo "  ./make.sh vite stop   # stop the standalone WebUI Vite dev server"
    echo "  ./make.sh vite status # show the standalone WebUI Vite dev server status"
    echo "  ./make.sh             # Core/Electron/WebUI debug build"
    echo "  ./make.sh build       # Core/Electron/WebUI debug build"
    echo "  ./make.sh br          # build Core/WebUI/Electron and run desktop with Vite"
    echo "  ./make.sh br release  # build Core/WebUI/Electron and run desktop"
    echo "  ./make.sh release     # Core/Electron/WebUI release build"
    echo "  ./make.sh licenses    # generate Rust third-party notices for the About page"
    echo "  ./make.sh check-deps  # verify Rust dependency policy without changing manifests or the lockfile"
    echo "  ./make.sh check       # static gates: fmt, clippy -D warnings, lint, typecheck"
    echo "  ./make.sh test        # Rust tests without long real-time and plugin-lifecycle tests"
    echo "  ./make.sh test full   # every Rust test, isolated LuaSkills callbacks, and doctests"
    echo "  ./make.sh run         # run Electron desktop; start Vite when needed"
    echo "  ./make.sh run release # run Core/Electron/WebUI release build"
    echo "  ./make.sh service     # start the tray/background Core only"
    echo "  ./make.sh core-build  # Windows only: compile Debug Core without rebuilding GUI"
    echo "  ./make.sh core-restart # Windows only: compile and restart Debug Core"
    echo "  ./make.sh deps        # install host, bundled tools, and LuaSkills dependencies"
    echo "  ./make.sh deps rust-tools # install cargo-hakari, cargo-nextest, and cargo-deny"
    echo "  ./make.sh init-system # bootstrap Rust tools and all repository dependencies"
    echo "  ./make.sh deps host   # install host native dependencies only"
    echo "  ./make.sh deps tools  # install the latest bundled sg and rg tools"
    echo "  ./make.sh deps lua    # install official LuaSkills runtime dependencies only"
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
    sync)
        if [ "$NormalizedVariant" != "" ]; then
            echo "Unsupported sync command: '$CommandVariant'" >&2
            show_usage
            exit 1
        fi
        invoke_tauri_sync
        ;;
    vite)
        case "$NormalizedVariant" in
            start|stop|status)
                invoke_vite_dev_server "$NormalizedVariant"
                ;;
            *)
                echo "Unsupported vite command: '$CommandVariant'" >&2
                show_usage
                exit 1
                ;;
        esac
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
    check-deps)
        if [ "$NormalizedVariant" != "" ]; then
            echo "Unsupported check-deps command: '$CommandVariant'" >&2
            show_usage
            exit 1
        fi
        invoke_powershell_route check-deps
        ;;
    check)
        if [ "$NormalizedVariant" != "" ]; then
            echo "Unsupported check command: '$CommandVariant'" >&2
            show_usage
            exit 1
        fi
        node "$ScriptDir/scripts/check.mjs"
        ;;
    test)
        case "$NormalizedVariant" in
            "") node "$ScriptDir/scripts/test-rust.mjs" fast ;;
            full) node "$ScriptDir/scripts/test-rust.mjs" full ;;
            *)
                echo "Unsupported test command: '$CommandVariant'" >&2
                show_usage
                exit 1
                ;;
        esac
        ;;
    service)
        # Select one explicit build profile for the background-only entry point.
        # 为纯后台入口选择一个明确的构建模式。
        case "$NormalizedVariant" in
            "") node "$ScriptDir/scripts/desktop-runtime.mjs" service debug ;;
            release) node "$ScriptDir/scripts/desktop-runtime.mjs" service release ;;
            *) echo "Unsupported service profile: $CommandVariant" >&2; exit 1 ;;
        esac
        ;;
    core-build)
        if [ "$NormalizedVariant" != "" ]; then
            echo "Unsupported core-build variant: '$CommandVariant'" >&2
            exit 1
        fi
        invoke_powershell_route core-build
        ;;
    core-restart)
        if [ "$NormalizedVariant" != "" ]; then
            echo "Unsupported core-restart variant: '$CommandVariant'" >&2
            exit 1
        fi
        invoke_powershell_route core-restart
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
    init-system)
        if [ "$NormalizedVariant" != "" ]; then
            echo "Unsupported init-system command: '$CommandVariant'" >&2
            show_usage
            exit 1
        fi
        invoke_system_initialization
        ;;
    deps)
        case "$NormalizedVariant" in
            tools)
                invoke_bundled_tools_deps
                ;;
            rust-tools)
                invoke_rust_dev_tools_deps
                ;;
            ""|all|host|lua)
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
