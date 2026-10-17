#!/usr/bin/env sh
# Prepare the native libraries used by the GTK tray and windowing dependencies.
# 准备用于 GTK 托盘与窗口依赖的原生库。
set -eu

if [ "$(uname -s)" != Linux ]; then
    exit 0
fi

# Packages is the single Debian/Ubuntu package set; GTK development packages include GLib, Cairo, Pango, and ATK.
# Packages 是唯一的 Debian/Ubuntu 软件包集合；GTK 开发包包含 GLib、Cairo、Pango 与 ATK。
Packages='build-essential pkg-config libgtk-3-dev libxdo-dev libayatana-appindicator3-dev'
# ProbeDirectory owns only a compile/link probe and its diagnostics; no test program is executed.
# ProbeDirectory 只拥有编译链接探针及其诊断；不执行测试程序。
ProbeDirectory=$(mktemp -d "${TMPDIR:-/tmp}/vulcan-native-deps.XXXXXX")

# cleanup removes the two exact probe files and their empty owned directory; accepts no arguments and returns cleanup status.
# cleanup 移除两个确切探针文件及其空的自有目录；无参数，返回清理状态。
cleanup() {
    rm -f -- "$ProbeDirectory/probe" "$ProbeDirectory/probe.log"
    rmdir -- "$ProbeDirectory"
}
trap cleanup 0

# native_dependencies_ready probes pkg-config metadata and links libxdo; accepts no arguments and returns readiness.
# native_dependencies_ready 检查 pkg-config 元数据并链接 libxdo；无参数，返回是否就绪。
native_dependencies_ready() {
    : >"$ProbeDirectory/probe.log"
    # RequiredTool names one mandatory native build command; record the first missing prerequisite explicitly.
    # RequiredTool 表示一个必需的原生构建命令；明确记录首个缺失的前置项。
    for RequiredTool in pkg-config cc c++ make; do
        if ! command -v "$RequiredTool" >/dev/null 2>&1; then
            printf 'Required native build command is unavailable: %s\n' "$RequiredTool" >>"$ProbeDirectory/probe.log"
            return 1
        fi
    done
    pkg-config --print-errors --exists glib-2.0 gobject-2.0 gio-2.0 gtk+-3.0 >>"$ProbeDirectory/probe.log" 2>&1 || return 1
    # Upstream explicitly supports either AppIndicator or Ayatana; either development package satisfies this backend.
    # 上游明确支持 AppIndicator 与 Ayatana 两种实现；任意一种开发包均可满足当前后端。
    if ! pkg-config --exists ayatana-appindicator3-0.1 && ! pkg-config --exists appindicator3-0.1; then
        echo 'Neither Ayatana nor AppIndicator development metadata is available.' >>"$ProbeDirectory/probe.log"
        return 1
    fi
    printf '%s\n' '#include <xdo.h>' 'int main(void) { xdo_t *x = xdo_new(0); if (x) xdo_free(x); return 0; }' |
        cc -x c - -o "$ProbeDirectory/probe" -lxdo >>"$ProbeDirectory/probe.log" 2>&1 || return 1
}

if native_dependencies_ready; then
    echo '==> Native Linux build dependencies are ready.'
    exit 0
fi

echo '==> Missing native Linux development files for GTK/GLib, AppIndicator, or libxdo.'
# Report the failed probe before requesting privileges so users can see the actual missing files.
# 请求权限前报告失败探针，使用户能够看到实际缺失的文件。
cat "$ProbeDirectory/probe.log" >&2
if ! command -v apt-get >/dev/null 2>&1; then
    echo 'Install GTK 3, GLib, AppIndicator/Ayatana and libxdo development packages with your distribution package manager, then rerun make init-system.' >&2
    exit 1
fi

# run_apt uses root, cached sudo, or terminal-based sudo authentication without elevating later build stages.
# run_apt 使用 root、已缓存的 sudo 或终端内的 sudo 验证，不提升后续构建阶段的权限。
# Accepts apt-get arguments and returns the package manager or authentication failure status.
# 接收 apt-get 参数，并返回包管理器或身份验证失败的状态码。
run_apt() {
    if [ "$(id -u)" = 0 ]; then
        env DEBIAN_FRONTEND=noninteractive apt-get "$@"
    elif command -v sudo >/dev/null 2>&1; then
        if ! sudo -n true 2>/dev/null; then
            # Ask sudo to authenticate only in a real terminal; automated runs must never wait for a password.
            # 只在真实终端中交由 sudo 验证身份，自动化运行绝不能等待密码输入。
            if [ ! -t 0 ] || [ ! -t 2 ]; then
                echo 'Native Linux packages require administrator privileges. Rerun make init-system in an interactive terminal, or ask an administrator to run: sudo sh scripts/install_linux_system_deps.sh' >&2
                return 1
            fi
            echo '==> Installing native Linux packages requires sudo authorization. Enter your Linux account password if prompted.' >&2
            sudo -v || return $?
        fi
        sudo -n env DEBIAN_FRONTEND=noninteractive apt-get "$@"
    else
        echo 'Native Linux packages require administrator privileges. Ask an administrator to run: sudo sh scripts/install_linux_system_deps.sh' >&2
        return 1
    fi
}

run_apt update
# Intentional word splitting expands only the fixed package identifiers declared above.
# 此处有意进行单词拆分，仅展开上面声明的固定软件包标识。
run_apt install -y --no-install-recommends $Packages
if ! native_dependencies_ready; then
    echo 'Native Linux development files are still unavailable after package installation.' >&2
    cat "$ProbeDirectory/probe.log" >&2
    exit 1
fi
echo '==> Native Linux build dependencies installed and verified.'
