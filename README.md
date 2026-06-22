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
./make run
```

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
