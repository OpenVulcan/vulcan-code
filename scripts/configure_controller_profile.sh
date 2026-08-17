#!/usr/bin/env sh
# configure_controller_profile.sh writes the build-profile-specific VLDB endpoint into the generated runtime configuration.
# configure_controller_profile.sh 将构建模式对应的 VLDB 端点写入生成的运行时配置。
set -eu

# ScriptDir stores this script directory for stable repository-root resolution.
# ScriptDir 保存当前脚本目录，确保仓库根目录解析稳定。
ScriptDir=$(CDPATH= cd "$(dirname "$0")" && pwd -P)

# ProjectDir stores the repository root for the default generated configuration path.
# ProjectDir 保存仓库根目录，用于解析默认生成配置路径。
ProjectDir=$(CDPATH= cd "$ScriptDir/.." && pwd -P)

# Release stores whether the release VLDB endpoint should be generated.
# Release 保存是否应生成 Release VLDB 端点。
Release=0

# ConfigPath optionally redirects the generated configuration path for focused verification.
# ConfigPath 可选地重定向生成配置路径，供定向验证使用。
ConfigPath=""

while [ "$#" -gt 0 ]; do
    case "$1" in
        --release|release)
            Release=1
            shift
            ;;
        --config-path)
            if [ "$#" -lt 2 ]; then
                echo "Missing value for --config-path." >&2
                exit 1
            fi
            ConfigPath=$2
            shift 2
            ;;
        *)
            echo "Unsupported configure_controller_profile.sh argument: $1" >&2
            exit 1
            ;;
    esac
done

if [ -z "$ConfigPath" ]; then
    ConfigPath="$ProjectDir/output/config/controller.json"
fi

# ControllerEndpoint is selected exclusively from the requested Cargo build profile.
# ControllerEndpoint 仅根据请求的 Cargo 构建模式进行选择。
ControllerEndpoint=http://127.0.0.1:19802
if [ "$Release" = "1" ]; then
    ControllerEndpoint=http://127.0.0.1:19801
fi

if ! command -v node >/dev/null 2>&1; then
    echo "Node.js is required to update the generated Controller profile configuration." >&2
    exit 1
fi

# VULCAN_CONTROLLER_CONFIG_PATH passes the exact destination without shell interpolation in JavaScript.
# VULCAN_CONTROLLER_CONFIG_PATH 向 JavaScript 传递精确目标路径，避免 shell 字符串插值。
# VULCAN_CONTROLLER_ENDPOINT passes the exact profile-selected endpoint to the JSON updater.
# VULCAN_CONTROLLER_ENDPOINT 向 JSON 更新器传递精确的构建模式端点。
VULCAN_CONTROLLER_CONFIG_PATH=$ConfigPath \
VULCAN_CONTROLLER_ENDPOINT=$ControllerEndpoint \
node <<'NODE'
// fs provides the exact filesystem operations used for atomic generated-config replacement.
// fs 提供原子替换生成配置所需的精确文件系统操作。
const fs = require("fs");
// path provides platform-native parent and temporary path construction.
// path 提供平台原生的父目录与临时路径构造。
const path = require("path");

// configPath is the exact generated Controller configuration path supplied by the shell wrapper.
// configPath 是 shell 包装层提供的精确 Controller 生成配置路径。
const configPath = process.env.VULCAN_CONTROLLER_CONFIG_PATH;
// controllerEndpoint is the exact profile-selected endpoint supplied by the shell wrapper.
// controllerEndpoint 是 shell 包装层提供的精确构建模式端点。
const controllerEndpoint = process.env.VULCAN_CONTROLLER_ENDPOINT;

if (!configPath || !controllerEndpoint) {
  throw new Error("Controller profile configuration environment is incomplete.");
}

// configuration preserves every unrelated existing Controller setting.
// configuration 保留全部无关的现有 Controller 设置。
let configuration = {};
if (fs.existsSync(configPath)) {
  configuration = JSON.parse(fs.readFileSync(configPath, "utf8"));
  if (configuration === null || typeof configuration !== "object" || Array.isArray(configuration)) {
    throw new Error(`Controller configuration must contain one JSON object: ${configPath}`);
  }
}

if (configuration.endpoint === controllerEndpoint) {
  process.stdout.write(`==> Controller profile already configured: ${controllerEndpoint}\n`);
  process.exit(0);
}

configuration.endpoint = controllerEndpoint;
fs.mkdirSync(path.dirname(configPath), { recursive: true });

// temporaryConfigPath stays beside the destination so rename remains atomic on one filesystem.
// temporaryConfigPath 与目标文件位于同一目录，确保重命名在同一文件系统中保持原子性。
const temporaryConfigPath = path.join(
  path.dirname(configPath),
  `.controller.${process.pid}.${Date.now()}.tmp`,
);
try {
  fs.writeFileSync(temporaryConfigPath, `${JSON.stringify(configuration, null, 2)}\n`, "utf8");
  fs.renameSync(temporaryConfigPath, configPath);
} finally {
  if (fs.existsSync(temporaryConfigPath)) {
    fs.unlinkSync(temporaryConfigPath);
  }
}

process.stdout.write(`==> Controller profile configured: ${controllerEndpoint}\n`);
NODE
