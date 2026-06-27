# Makefile is a thin platform router for the repository build entrypoints.
# Makefile 是仓库构建入口的轻量平台路由层。

# The default target mirrors the existing make.ps1 default debug build behavior.
# 默认目标保持与现有 make.ps1 默认 debug 构建行为一致。
.DEFAULT_GOAL := build

# POWERSHELL_SCRIPT points at the Windows-oriented authoritative script.
# POWERSHELL_SCRIPT 指向面向 Windows 的权威脚本入口。
POWERSHELL_SCRIPT ?= ./make.ps1

# POWERSHELL_COMMAND selects the PowerShell executable used on Windows.
# POWERSHELL_COMMAND 选择 Windows 上使用的 PowerShell 可执行文件。
POWERSHELL_COMMAND ?= powershell.exe

# SHELL_SCRIPT points at the optional Unix-oriented authoritative script.
# SHELL_SCRIPT 指向可选的面向 Unix 的权威脚本入口。
SHELL_SCRIPT ?= ./make.sh

# SH_COMMAND selects the Unix shell executable used for shell-script routing.
# SH_COMMAND 选择 shell 脚本路由使用的 Unix shell 可执行文件。
SH_COMMAND ?= sh

ifeq ($(OS),Windows_NT)
# route-command forwards one normalized command to make.ps1 on Windows.
# route-command 在 Windows 上把规范化命令转发给 make.ps1。
define route-command
	@$(POWERSHELL_COMMAND) -NoProfile -ExecutionPolicy Bypass -File $(POWERSHELL_SCRIPT) $(1)
endef
else
# route-command forwards one normalized command to make.sh when available, then falls back to pwsh.
# route-command 优先把规范化命令转发给可用的 make.sh，随后回退到 pwsh。
define route-command
	@if [ -f "$(SHELL_SCRIPT)" ]; then \
		$(SH_COMMAND) "$(SHELL_SCRIPT)" $(1); \
	elif command -v pwsh >/dev/null 2>&1; then \
		pwsh -NoProfile -ExecutionPolicy Bypass -File "$(POWERSHELL_SCRIPT)" $(1); \
	else \
		echo "No supported build route found: $(SHELL_SCRIPT) is missing and pwsh is unavailable." >&2; \
		exit 127; \
	fi
endef
endif

.PHONY: help build build-release release run run-release br br-release licenses deps deps-all deps-host deps-vldb deps-rtk deps-lua host vldb rtk lua all

# DEPS_COMPANION_GOALS captures optional secondary dependency goals such as `make deps rtk`.
# DEPS_COMPANION_GOALS 捕获 `make deps rtk` 这类可选的依赖二级目标。
DEPS_COMPANION_GOALS := $(filter-out deps,$(MAKECMDGOALS))

# DEPS_ROUTE_COMMAND selects the command forwarded by the deps router.
# DEPS_ROUTE_COMMAND 选择 deps 路由需要转发的命令。
DEPS_ROUTE_COMMAND := $(if $(DEPS_COMPANION_GOALS),deps $(DEPS_COMPANION_GOALS),deps)

# help prints the supported Makefile router targets.
# help 输出 Makefile 路由层支持的目标。
help:
	@echo "Targets:"
	@echo "  make build         # Tauri/WebUI debug build"
	@echo "  make build-release # Tauri/WebUI release build"
	@echo "  make release       # Tauri/WebUI release build"
	@echo "  make run           # run Tauri/WebUI debug build"
	@echo "  make run-release   # run Tauri/WebUI release build"
	@echo "  make br            # build and run Tauri/WebUI debug"
	@echo "  make br-release    # build and run Tauri/WebUI release"
	@echo "  make licenses      # generate Rust third-party notices"
	@echo "  make deps          # install host + Lua dependencies"
	@echo "  make deps-host     # install host dependencies"
	@echo "  make deps-vldb     # install vldb-controller host dependency"
	@echo "  make deps-rtk      # install RTK host dependency"
	@echo "  make deps-lua      # install Lua dependencies"

# build routes debug by default, or release when the release companion goal is present.
# build 默认路由 debug，存在 release 伴随目标时路由 release。
build:
	$(call route-command,$(if $(filter release,$(MAKECMDGOALS)),build release,build))

# build-release routes the explicit release build command.
# build-release 路由显式 release 构建命令。
build-release:
	$(call route-command,build release)

# release routes the short release build command or becomes a companion no-op for paired goals.
# release 路由短形式 release 构建命令，或在组合目标中作为伴随空操作。
release:
ifneq ($(filter build build-release run run-release br br-release,$(MAKECMDGOALS)),)
	@:
else
	$(call route-command,release)
endif

# run routes debug by default, or release when the release companion goal is present.
# run 默认路由 debug，存在 release 伴随目标时路由 release。
run:
	$(call route-command,$(if $(filter release,$(MAKECMDGOALS)),run release,run))

# run-release routes the release runtime command.
# run-release 路由 release 运行命令。
run-release:
	$(call route-command,run release)

# br routes debug by default, or release when the release companion goal is present.
# br 默认路由 debug，存在 release 伴随目标时路由 release。
br:
	$(call route-command,$(if $(filter release,$(MAKECMDGOALS)),br release,br))

# br-release routes the release build-and-run command.
# br-release 路由 release 构建并运行命令。
br-release:
	$(call route-command,br release)

# licenses routes Rust third-party notice generation.
# licenses 路由 Rust 第三方声明生成命令。
licenses:
	$(call route-command,licenses)

# deps routes all dependency installation steps.
# deps 路由全部依赖安装步骤。
deps:
	$(call route-command,$(DEPS_ROUTE_COMMAND))

# deps-all routes the explicit all-dependencies installation step.
# deps-all 路由显式全部依赖安装步骤。
deps-all:
	$(call route-command,deps all)

# deps-host routes host dependency installation.
# deps-host 路由宿主依赖安装步骤。
deps-host:
	$(call route-command,deps host)

# deps-vldb routes vldb-controller host dependency installation.
# deps-vldb 路由 vldb-controller 宿主依赖安装步骤。
deps-vldb:
	$(call route-command,deps vldb)

# deps-rtk routes RTK host dependency installation.
# deps-rtk 路由 RTK 宿主依赖安装步骤。
deps-rtk:
	$(call route-command,deps rtk)

# deps-lua routes Lua dependency installation.
# deps-lua 路由 Lua 依赖安装步骤。
deps-lua:
	$(call route-command,deps lua)

# all is a deps companion no-op when used as `make deps all`.
# all 在 `make deps all` 形式中作为 deps 的伴随空目标。
all:
ifneq ($(filter deps,$(MAKECMDGOALS)),)
	@:
else
	$(call route-command,build)
endif

# host is a deps companion no-op when used as `make deps host`.
# host 在 `make deps host` 形式中作为 deps 的伴随空目标。
host:
ifneq ($(filter deps,$(MAKECMDGOALS)),)
	@:
else
	$(call route-command,deps host)
endif

# vldb is a deps companion no-op when used as `make deps vldb`.
# vldb 在 `make deps vldb` 形式中作为 deps 的伴随空目标。
vldb:
ifneq ($(filter deps,$(MAKECMDGOALS)),)
	@:
else
	$(call route-command,deps vldb)
endif

# rtk is a deps companion no-op when used as `make deps rtk`.
# rtk 在 `make deps rtk` 形式中作为 deps 的伴随空目标。
rtk:
ifneq ($(filter deps,$(MAKECMDGOALS)),)
	@:
else
	$(call route-command,deps rtk)
endif

# lua is a deps companion no-op when used as `make deps lua`.
# lua 在 `make deps lua` 形式中作为 deps 的伴随空目标。
lua:
ifneq ($(filter deps,$(MAKECMDGOALS)),)
	@:
else
	$(call route-command,deps lua)
endif
