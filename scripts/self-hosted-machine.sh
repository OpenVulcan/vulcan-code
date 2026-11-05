#!/bin/sh
# Prepare a fresh Ubuntu 22.04 machine, as root, for the runner of one self-hosted pool
# (docs/contracts/self-hosted-runners.md). `node scripts/ci-runners.mjs prepare` sends this script and
# scripts/install_linux_system_deps.sh to the machine and runs it; the versions come from the repository.
# 以 root 将一台新的 Ubuntu 22.04 机器准备为一个自托管池的运行器机器（docs/contracts/self-hosted-runners.md）。
# `node scripts/ci-runners.mjs prepare` 将本脚本与 scripts/install_linux_system_deps.sh 发送到机器并运行；
# 版本取自仓库。
#
#   self-hosted-machine.sh --node <version> --rust <toolchain> [--user <name>] [--data-disk <device>] [--data-dir <dir>]
#
# --data-disk formats the device when it holds no filesystem (never otherwise) and mounts it at --data-dir
# (default /data) through /etc/fstab. Everything that is not a system package then lives there: Node, and the
# runner user's home with its Rust toolchain, Cargo cache, PowerShell and the runner with its work directory,
# so a machine whose system disk is replaced keeps them. Running it again changes nothing that is in place.
# --data-disk 在设备上没有文件系统时将其格式化（否则绝不格式化），并通过 /etc/fstab 挂载到 --data-dir
# （默认 /data）。系统软件包以外的一切都放在其中：Node，以及运行器用户的家目录（含 Rust 工具链、Cargo 缓存、
# PowerShell 与运行器及其工作目录），因此更换系统盘的机器仍保留它们。重复运行不会改变已就绪的内容。
set -eu

fail() {
    printf 'self-hosted-machine: %s\n' "$*" >&2
    exit 1
}

[ "$(id -u)" = 0 ] || fail "run this as root"
[ "$(uname -s)" = Linux ] || fail "this prepares Linux machines only"
Here=$(CDPATH= cd "$(dirname "$0")" && pwd -P)

User=runner Node= Rust= DataDisk= DataDir=/data
while [ $# -gt 0 ]; do
    case "$1" in
        --user) User=$2; shift 2 ;;
        --node) Node=$2; shift 2 ;;
        --rust) Rust=$2; shift 2 ;;
        --data-disk) DataDisk=$2; shift 2 ;;
        --data-dir) DataDir=$2; shift 2 ;;
        *) fail "unknown argument $1" ;;
    esac
done
[ -n "$Node" ] && [ -n "$Rust" ] || fail "--node and --rust are required"

case "$(uname -m)" in
    x86_64) NodeArch=linux-x64 ;;
    aarch64) NodeArch=linux-arm64 ;;
    *) fail "unsupported machine $(uname -m)" ;;
esac

# The data disk: formatted only while blank, mounted by UUID with nofail so a missing disk never stops the boot.
# 数据盘：仅在空白时格式化，按 UUID 以 nofail 挂载，使缺失的数据盘不会阻止开机。
if [ -n "$DataDisk" ]; then
    [ -b "$DataDisk" ] || fail "$DataDisk is not a block device"
    if ! blkid "$DataDisk" > /dev/null 2>&1; then
        echo "Formatting the blank $DataDisk"
        mkfs.ext4 -q -L vulcan-data "$DataDisk"
    fi
    Uuid=$(blkid -s UUID -o value "$DataDisk")
    [ -n "$Uuid" ] || fail "$DataDisk has a filesystem without a UUID"
    mkdir -p "$DataDir"
    grep -q "UUID=$Uuid " /etc/fstab || echo "UUID=$Uuid $DataDir ext4 defaults,nofail 0 2" >> /etc/fstab
    mountpoint -q "$DataDir" || mount "$DataDir"
    Opt="$DataDir/opt" Home="$DataDir/$User"
else
    Opt=/opt/vulcan Home="/home/$User"
fi

# System packages: the tools the jobs call directly, then the native build and Chromium libraries.
# 系统软件包：作业直接调用的工具，以及原生构建与 Chromium 所需的库。
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq --no-install-recommends git curl unzip xz-utils zstd ca-certificates sudo > /dev/null
sh "$Here/install_linux_system_deps.sh"

# Node, checked against the official SHA-256 list, on the data disk with links on the system PATH.
# Node 按官方 SHA-256 清单校验，放在数据盘上，并在系统 PATH 中建立链接。
if [ "$(/usr/local/bin/node --version 2>/dev/null)" != "v$Node" ]; then
    Archive="node-v$Node-$NodeArch.tar.xz"
    Download=$(mktemp -d)
    curl -fsSL -o "$Download/$Archive" "https://nodejs.org/dist/v$Node/$Archive"
    curl -fsSL -o "$Download/SHASUMS256.txt" "https://nodejs.org/dist/v$Node/SHASUMS256.txt"
    (cd "$Download" && grep " $Archive\$" SHASUMS256.txt | sha256sum -c -)
    mkdir -p "$Opt/nodejs"
    rm -rf "$Opt/nodejs/node-v$Node-$NodeArch"
    tar -xJf "$Download/$Archive" -C "$Opt/nodejs"
    rm -rf "$Download"
    for Tool in node npm npx corepack; do
        ln -sfn "$Opt/nodejs/node-v$Node-$NodeArch/bin/$Tool" "/usr/local/bin/$Tool"
    done
fi
echo "Node $(/usr/local/bin/node --version)"

# The runner user. It logs in with root's SSH keys, so the maintainer can reach it for `register`.
# 运行器用户。它使用 root 的 SSH 密钥登录，使维护者可以为 `register` 访问它。
if ! id "$User" > /dev/null 2>&1; then
    useradd -m -d "$Home" -s /bin/bash "$User"
fi
Home=$(getent passwd "$User" | cut -d: -f6)
if [ -s /root/.ssh/authorized_keys ] && [ ! -s "$Home/.ssh/authorized_keys" ]; then
    install -d -m 700 -o "$User" -g "$User" "$Home/.ssh"
    install -m 600 -o "$User" -g "$User" /root/.ssh/authorized_keys "$Home/.ssh/authorized_keys"
fi

# The runner's service script is the only command the user may run as root, so `register` installs the service.
# 运行器的服务脚本是该用户唯一可以 root 身份运行的命令，使 `register` 能够安装服务。
Sudoers=/etc/sudoers.d/vulcan-runner
printf '%s ALL=(root) NOPASSWD: %s/actions-runner/vulcan-code/svc.sh\n' "$User" "$Home" > "$Sudoers.tmp"
chmod 440 "$Sudoers.tmp"
visudo -cq -f "$Sudoers.tmp" || { rm -f "$Sudoers.tmp"; fail "the sudoers rule does not parse"; }
mv "$Sudoers.tmp" "$Sudoers"

# Rust for the runner user, with the repository toolchain as the default: the build-script tests call cargo
# outside the repository, where rust-toolchain.toml does not apply.
# 为运行器用户安装 Rust，并以仓库工具链为默认值：构建脚本测试在仓库之外调用 cargo，那里不适用 rust-toolchain.toml。
su - "$User" -c "set -eu
    if [ ! -x \"\$HOME/.cargo/bin/rustup\" ]; then
        curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y -q --profile minimal --default-toolchain none
    fi
    \"\$HOME/.cargo/bin/rustup\" toolchain install '$Rust' --profile minimal -c clippy -c rustfmt
    \"\$HOME/.cargo/bin/rustup\" default '$Rust'
    \"\$HOME/.cargo/bin/cargo\" --version"

echo "The machine is ready; register the runner as $User."
