# 共用浏览器运行时兼容性验证

## 版本规则

版本组合必须精确锁定，允许经过验证的预发布版本。禁止通过 `latest`、日期自动选择或版本范围自动升级。
升级应作为独立适配任务，同时核对宿主、插件、锁文件和浏览器修订号，再执行下面的实际兼容性验证。

| 组件 | 本次锁定版本 |
| --- | --- |
| Electron | 44.4.1 |
| Electron 内置 Chromium | 152.0.7977.78 |
| Playwright / playwright-core | 1.64.0-alpha-2026-09-14 |
| 官方 Playwright MCP | 0.0.81 |
| Playwright 对应 Chromium | 154.0.8037.0，修订号 1244 |
| MCP 与验收入口使用的 Node.js | 24.18.0 |

`shared-runtime-compatibility.mjs` 中的 `PINNED` 是本组兼容性验收的固定基线。测试会核对桌面和插件声明、
实际安装的依赖版本、Playwright 浏览器描述，以及真实启动后上报的内核版本；版本不符直接失败。
不得为消除版本检查失败而单独修改基线，必须一起完成相应适配和验证。

## 运行方式

1. 使用已有锁文件安装桌面与插件依赖，保留 `--frozen-lockfile`；不得临时下载另一个 MCP 版本。
2. 在桌面目录运行 `rtk pnpm run build`，得到当前源码的宿主协议和中继产物。
3. 使用锁定的 Node.js 执行下列命令，显式指定当前 MCP 包目录和 Chromium 程序；工具不自动安装浏览器。

```powershell
rtk proxy output/dependencies/runtimes/node/node-24.18.0-windows-x64/node.exe apps/vulcan-code-desktop/tools/shared-runtime-compatibility.mjs --evidence-dir=D:/openvulcan/vulcan-code/target/shared-runtime-20260920 --mcp-root=D:/openvulcan/vulcan-code/target/w08-playwright-mcp-compat/node_modules/@playwright/mcp --chromium-executable=D:/openvulcan/vulcan-code/target/browser-runtime-install-20260920/installs/df6ba132-ab20-4edc-ab29-9320ada5872e/chromium-1244/chrome-win64/chrome.exe
```

以上路径是本次实测输入，其他机器应替换为实际安装位置。每次运行创建独立的 `run-*` 目录，保留画像、
宿主日志、MCP 日志和 `results.json`。用例使用本地确定性网页，不访问真实会话、Cookie 或在线服务。
失败仍然输出证据，整体退出码为 1；不把已知兼容性问题豁免为通过。

## 2026-09-20 实测结果

代码基线：`6bfffd4f`，桌面 TypeScript 构建通过。

最终证据：`target/shared-runtime-20260920/run-VxsDnO/results.json`。

| 接入方式 | 通过 | 失败 | 具体结果 |
| --- | ---: | ---: | --- |
| 既有 Electron 宿主，经 CDP 接入 | 8 | 4 | 导航、输入、框架、MCP 连接、快照点击、撤销、原生创建与截图、隔离和独立关闭可用；Playwright/MCP 新标签与隐藏窗口截图失败 |
| Playwright 官方 Electron API | 2 | 1 | 隐藏原生窗口、输入和截图可用；`context.newPage()` 失败 |
| 官方 Electron API 初始化后，再经 CDP 接入 MCP | 4 | 4 | 导航、输入、框架、MCP 连接、快照点击和撤销可用；仍然无法新建标签及截图 |
| 独立 Chromium，经现有 Playwright 中继接入 | 8 | 0 | 相同页面操作、截图、官方 MCP 新标签及撤销全部通过 |

合计 31 项，22 项通过、9 项失败。Electron 新页错误为 `Target.createTarget: Not supported`；
CDP 和 MCP 截图超时为 5000 毫秒。官方 Electron API 截图在 10000 毫秒时限内通过。

补充执行现有 `local-browser-acceptance.mjs`，GUI 原生浏览器 11 组验收全部通过，包括新增标签、
排序、导航、尺寸、独立调试工具、关闭全部页面和工作台重载清理。

宿主退出记录均成功；结束后按本次独立画像目录检查，未发现对应 Electron、Chromium 或 MCP 残留进程。

## 判断与适配边界

- 当前版本下，GUI 与 Core 共用 Electron 程序文件、保持独立进程及会话数据，已验证具有可行基础。
- 现有官方 MCP 不能直接替换到 Electron 上完整使用。新页创建必须适配 Electron 原生接口；截图也需要
  对应的宿主接入适配。原生操作可用不等于官方 MCP 的相同工具已经可用。
- 官方 Electron API 的上下文没有向客户端暴露可直接交给现有中继的 `Browser` 对象；再次通过 CDP
  连接后，截图问题仍然存在，不能据 Electron API 的单项截图通过就宣称 MCP 已兼容。
- 本次没有切换生产浏览器、修改依赖版本或部署运行时。后续共用宿主适配必须保持 Core 独立生命周期、
  页面身份、权限控制及撤销语义；还需覆盖弹窗、下载、对话框、文件选择和异常退出等完整产品验收。
- 旧 `w02-compatibility.mjs` 仍要求握手携带 `cdpEndpoint`，与当前第二版协议不符。本次旧脚本的握手
  超时不计入运行时兼容性结果，新工具直接使用当前协议定义并按独立画像发现 CDP 端点。

## 按 Chromium 分支反查后的补充对照

用户提出按 Electron 的 Chromium 版本寻找更接近的 Playwright。核对发布包后找到：

| Playwright 固定版本 | 发布包声明的 Chromium |
| --- | --- |
| 1.63.0-alpha-2026-08-25 | 152.0.7977.54，修订号 1241 |
| 当前 1.64.0-alpha-2026-09-14 | 154.0.8037.0，修订号 1244 |

候选版本与 Electron 的 `152.0.7977.78` 属于同一分支，但补丁号不完全一致。候选只安装在
`target/shared-runtime-pw-1.63.0-alpha-2026-08-25`，项目清单、锁文件和生产运行时均未更改。

在相同 Electron 44.4.1 程序与隐藏窗口用例上分别运行两个客户端版本：

- 两个版本的 Electron API `context.newPage()` 和 CDP `context.newPage()` 均失败，错误完全相同：
  `Protocol error (Target.createTarget): Not supported`。
- 两个版本的 Electron API 截图、CDP 输入和 CDP 截图均通过本轮简单文档用例。
- 截图结果与上面完整网页用例不同，说明截图问题与场景、页面状态或初始化方式有关；现有证据不能把
  先前截图超时单独归因为版本差异，也不能推断所有隐藏窗口截图都不可用。
- 新建标签问题在同分支客户端上仍可复现，因此仅调整 Playwright 对应的 Chromium 版本不能解决它。
- 此次仅对照 Playwright 客户端，没有将依赖旧版本客户端的候选与现有 MCP 强行混装；该版本的 MCP
  完整接入仍未验证。

补充原始证据：`target/shared-runtime-pw-1.63.0-alpha-2026-08-25/comparison-results.json`。
