Vulcan Code 0.1.28 为外部 MCP 客户端加入独立工作区和调用日志，并能在启动时更新低于最低版本要求的受管 ROOT 技能。

### 外部 MCP

```ui-checklist
ok | 独立项目与会话 | 外部客户端可登记自己的工作目录并创建会话。文件、CodeKit、Shell 和知识库工具通过会话 ID 使用对应的工作区。
ok | 后台命令与终端 | 客户端获得相应授权后，可在 MCP 会话中运行后台命令、查询或停止任务，也可使用独立的终端会话。
ok | 可选的调用日志 | 在客户端设置中开启「记录工具调用」后，可按客户端、项目或会话查看调用记录、参数、结果与状态。
```

### 升级提示

```ui-checklist
ok | 旧版 MCP 独立工具调用需改用会话 ID | 先用 `vc_mcp_project` 登记项目，再用 `vc_mcp_session` 创建会话。后续独立工具调用传入返回的 `sid`，工作目录由该会话绑定。
```

### 技能与开发验证

```ui-checklist
ok | 启动时更新受管 ROOT 技能 | CodeKit、Vulcan Curl 和 Vulcan Work 低于要求版本时会从配置的来源更新。失败时保留可用旧版，下次启动重试。
ok | VMM 策略 HTTP 验收 | 开发者可用 `make test-vmm-http` 运行独立的 HTTP 验收，覆盖 WebUI 策略组件与本地服务的交互。
```
