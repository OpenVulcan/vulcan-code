#!/usr/bin/env sh
# Prepare the native libraries used by the GTK tray, windowing dependencies, and the bundled Chromium.
# 准备用于 GTK 托盘、窗口依赖与随包 Chromium 的原生库。
set -eu

if [ "$(uname -s)" != Linux ]; then
    exit 0
fi

# Packages is the Debian/Ubuntu development package set; GTK development packages include GLib, Cairo, Pango, and ATK.
# Packages 是 Debian/Ubuntu 开发软件包集合；GTK 开发包包含 GLib、Cairo、Pango 与 ATK。
Packages='build-essential pkg-config libgtk-3-dev libxdo-dev libayatana-appindicator3-dev'
# ChromiumPackages provides the bundled Chromium's direct shared-library dependencies, which packaging launches.
# Each entry lists alternatives separated by '|', the 64-bit time_t rename first; the first one APT can install is used.
# ChromiumPackages 提供随包 Chromium 的直接共享库依赖，打包时会实际启动该浏览器。
# 每项以 '|' 分隔候选名，64 位 time_t 改名在前；使用 APT 第一个可安装的名称。
ChromiumPackages='libnss3 libnspr4 libasound2t64|libasound2 libatk1.0-0t64|libatk1.0-0 libatk-bridge2.0-0t64|libatk-bridge2.0-0 libatspi2.0-0t64|libatspi2.0-0 libcups2t64|libcups2 libdbus-1-3 libexpat1 libgbm1 libglib2.0-0t64|libglib2.0-0 libpango-1.0-0 libcairo2 libudev1 libx11-6 libxcb1 libxcomposite1 libxdamage1 libxext6 libxfixes3 libxkbcommon0 libxrandr2'
# ChromiumLibraries are the sonames Chromium loads directly beyond glibc; the probe requires each in the linker cache.
# ChromiumLibraries 是 Chromium 在 glibc 之外直接加载的 soname；探针要求每项都在链接器缓存中。
ChromiumLibraries='libnss3.so libnssutil3.so libsmime3.so libnspr4.so libasound.so.2 libatk-1.0.so.0 libatk-bridge-2.0.so.0 libatspi.so.0 libcups.so.2 libdbus-1.so.3 libexpat.so.1 libgbm.so.1 libglib-2.0.so.0 libgobject-2.0.so.0 libgio-2.0.so.0 libpango-1.0.so.0 libcairo.so.2 libudev.so.1 libX11.so.6 libxcb.so.1 libXcomposite.so.1 libXdamage.so.1 libXext.so.6 libXfixes.so.3 libxkbcommon.so.0 libXrandr.so.2'
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
    chromium_libraries_ready
}

# chromium_libraries_ready looks up every Chromium soname in the linker cache; accepts no arguments and returns readiness.
# chromium_libraries_ready 在链接器缓存中查找 Chromium 的每个 soname；无参数，返回是否就绪。
chromium_libraries_ready() {
    # Unprivileged Debian accounts often lack /sbin on PATH, where ldconfig lives.
    # Debian 普通账户的 PATH 常不含 ldconfig 所在的 /sbin。
    LinkerConfig=$(command -v ldconfig || echo /sbin/ldconfig)
    LinkerCache=$("$LinkerConfig" -p) || {
        echo 'The dynamic linker cache could not be read with ldconfig -p.' >>"$ProbeDirectory/probe.log"
        return 1
    }
    MissingLibraries=''
    for Library in $ChromiumLibraries; do
        if ! printf '%s\n' "$LinkerCache" | awk -v name="$Library" '$1 == name { found = 1 } END { exit !found }'; then
            MissingLibraries="$MissingLibraries $Library"
        fi
    done
    if [ -n "$MissingLibraries" ]; then
        printf 'Shared libraries required by the bundled Chromium are missing:%s\n' "$MissingLibraries" >>"$ProbeDirectory/probe.log"
        return 1
    fi
}

# resolve_package prints the first alternative in a '|'-separated entry that APT can install.
# resolve_package 输出以 '|' 分隔的条目中第一个 APT 可安装的候选名。
# Accepts one ChromiumPackages entry and returns failure when no alternative has an installation candidate.
# 接收一个 ChromiumPackages 条目；没有任何候选具备安装候选版本时返回失败。
resolve_package() {
    for Candidate in $(printf '%s' "$1" | tr '|' ' '); do
        # A virtual package reports "Candidate: (none)"; the C locale keeps the field name stable.
        # 虚拟包会报告 "Candidate: (none)"；C 区域设置保证字段名稳定。
        if LC_ALL=C apt-cache policy "$Candidate" 2>/dev/null | grep -q '^ *Candidate: [^(]'; then
            printf '%s\n' "$Candidate"
            return 0
        fi
    done
    printf 'No installable package provides: %s\n' "$1" >&2
    return 1
}

if native_dependencies_ready; then
    echo '==> Native Linux build dependencies are ready.'
    exit 0
fi

echo '==> Missing native Linux development files for GTK/GLib, AppIndicator, or libxdo, or runtime libraries for the bundled Chromium.'
# Report the failed probe before requesting privileges so users can see the actual missing files.
# 请求权限前报告失败探针，使用户能够看到实际缺失的文件。
cat "$ProbeDirectory/probe.log" >&2
if ! command -v apt-get >/dev/null 2>&1; then
    echo 'Install GTK 3, GLib, AppIndicator/Ayatana and libxdo development packages, plus the missing Chromium runtime libraries listed above, with your distribution package manager, then rerun make init-system.' >&2
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
# Alternatives resolve only after the package lists are refreshed; any unresolved entry stops before installation.
# 候选名只能在刷新软件包列表后解析；任一条目无法解析即在安装前停止。
ResolvedPackages=''
for Entry in $ChromiumPackages; do
    ResolvedPackages="$ResolvedPackages $(resolve_package "$Entry")" || exit 1
done
# Intentional word splitting expands only the fixed and resolved package identifiers declared above.
# 此处有意进行单词拆分，仅展开上面声明的固定及已解析的软件包标识。
run_apt install -y --no-install-recommends $Packages $ResolvedPackages
if ! native_dependencies_ready; then
    echo 'Native Linux dependencies are still unavailable after package installation.' >&2
    cat "$ProbeDirectory/probe.log" >&2
    exit 1
fi
echo '==> Native Linux build dependencies installed and verified.'
