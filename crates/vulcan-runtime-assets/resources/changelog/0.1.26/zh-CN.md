Vulcan Code 0.1.26 可以在项目目录失效后保留会话并重新关联目录，同时改进了应用内更新和工具结果的展示。

### 项目与会话

```ui-checklist
ok | 恢复项目目录 | 项目目录被移动或磁盘断开后，可以重新检查原目录，也可以选择新目录并保留会话历史。目录变更会记录在会话时间线上，下次模型请求会使用新目录。
ok | 在选择器中打开挂载盘或输入的路径 | Linux 和 macOS 的目录侧栏会列出实际挂载的卷。输入路径后可以先打开该目录，再确认选择。
ok | 切换会话定位到最新消息 | 切换会话时会打开最新消息，方便继续接着看。
```

### 安装与更新

```ui-checklist
ok | 先准备更新，再选择重启 | 自动下载默认开启。新版下载并校验完成后会提示「更新已就绪」，确认「更新并重启」时才切换版本。也可以关闭自动下载并手动更新。
ok | 独立安装器 0.2.17 | Windows 提供签名图形安装器，Linux 提供 AppImage，macOS 提供 DMG。从 macOS 终端安装后可用 `vcode manager gui` 打开图形管理器，首次安装会为 LuaSkills 选用与安装线路对应的国内或国际 Hub。
```

### 浏览器与文档工具

```ui-checklist
ok | 浏览器操作卡片 | 导航、点击、截图等 Playwright 操作会显示具体动作、目标和结果。内置 Vulcan Browser 技能也加入了操作与验证步骤。
ok | Vulcan Work 文档工具 | Vulcan Code 会从当前 LuaSkills Hub 安装 Vulcan Work，支持 PDF、Word、Excel 和 PowerPoint 操作。专用卡片会按操作显示文件、工作表范围或结果。通过隐式注册调用的 LuaSkills 工具也能在帮助中显示实际参数。
```
