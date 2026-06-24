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

.PHONY: help build build-release release run run-release br br-release licenses deps deps-all deps-host deps-lua

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
	@echo "  make deps-lua      # install Lua dependencies"

# build routes the default debug build command.
# build 路由默认 debug 构建命令。
build:
	$(call route-command,build)

# build-release routes the explicit release build command.
# build-release 路由显式 release 构建命令。
build-release:
	$(call route-command,build release)

# release routes the short release build command.
# release 路由短形式 release 构建命令。
release:
	$(call route-command,release)

# run routes the debug runtime command.
# run 路由 debug 运行命令。
run:
	$(call route-command,run)

# run-release routes the release runtime command.
# run-release 路由 release 运行命令。
run-release:
	$(call route-command,run release)

# br routes the debug build-and-run command.
# br 路由 debug 构建并运行命令。
br:
	$(call route-command,br)

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
	$(call route-command,deps)

# deps-all routes the explicit all-dependencies installation step.
# deps-all 路由显式全部依赖安装步骤。
deps-all:
	$(call route-command,deps all)

# deps-host routes host dependency installation.
# deps-host 路由宿主依赖安装步骤。
deps-host:
	$(call route-command,deps host)

# deps-lua routes Lua dependency installation.
# deps-lua 路由 Lua 依赖安装步骤。
deps-lua:
	$(call route-command,deps lua)
