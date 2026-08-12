# vulcan-code

基于 Rust Tauri 后端与 React WebUI 的 AI Coding 桌面端仓库。

## 当前结构

- `Cargo.toml`：根工作区配置，统一管理 Tauri 桌面壳、Service 后端与共享运行时 crate。
- `apps/vulcan-code-tauri`：Tauri 桌面壳，负责启动本地 WebUI 服务、桌面事件与宿主能力。
- `apps/vulcan-code-web`：React/Vite WebUI，承载工作台、设置、会话、Diff、技能与 VMM 管理界面。
- `crates/vulcan-service-*`：WebUI 后端服务 API、宿主桥、核心服务内核与运行时适配。
- `crates/vulcan-agent-runtime`：共享代理运行时，负责 Provider、工具、LuaSkills、MCP、VMM 与流式执行。
- `docs/plan`：任务执行计划文件目录。

## 运行方式

```bash
./make vite start
./make vite status
./make run
./make vite stop
```

debug 模式下的 `run` 与 `br` 不再自动启动或停止 Vite；开发服务器由 `make vite start|status|stop` 独立管理。release 模式继续直接使用已经构建的 WebUI 资源，不依赖 Vite。

## 资源同步

```bash
./make sync
```

`./make sync` 只负责把配置、运行时资源、系统技能和插件同步到 `output`；资源未变化时，后续 `build`、`release` 与 `br` 无需重复同步。

## 构建方式

```bash
./make build
./make release
```

## 前端验证

```bash
pnpm --dir apps/vulcan-code-web run lint
pnpm --dir apps/vulcan-code-web run build
```
