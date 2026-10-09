#!/usr/bin/env sh
# Forward to the common locked installer; arguments support --offline and --repository-root.
# 转发到共用锁定安装器；参数支持 --offline 与 --repository-root。
set -eu
# ScriptDir locates the shared implementation without depending on the caller's directory.
# ScriptDir 定位共用实现，不依赖调用方当前目录。
ScriptDir=$(CDPATH= cd "$(dirname "$0")" && pwd -P)
node "$ScriptDir/install-locked-tools.mjs" bundled "$@"
# The bundled judge runtime (ONNX Runtime core plus the WebGPU plugin) ships with the same tools step.
# 随包判定执行库（ONNX Runtime 核心与 WebGPU 插件）随同一工具步骤安装。
exec node "$ScriptDir/install-locked-tools.mjs" judge-runtime "$@"
