# 内置浏览器组件

保留“Vulcan 浏览器”设置入口和插件启停、会话隔离、截图及工具约束。依赖随应用交付，首次启用不执行包管理器，也不下载浏览器。

## 锁定版本

| 组件 | 版本 |
| --- | --- |
| 官方 `@playwright/mcp` | `0.0.82` |
| `playwright` / `playwright-core` | `1.64.0-alpha-1789764292000` |
| Node | `24.18.0` |
| 构建用 pnpm | `11.11.0` |

Playwright 版本取自 MCP 0.0.82 的精确依赖。Chromium 由该 Playwright 包的浏览器清单选择，不按 Electron 的 Chromium 版本猜测兼容性。不使用 `latest`；升级需要单独修改版本组合、锁文件，并重新验证工具协议。

## 构建与分发

1. `make init-system` 准备受管 Node 和 pnpm。
2. `make sync` 使用冻结锁文件准备 `node/node_modules/`，并将对应 Chromium 下载至仓库 `target/browser-runtime/`；随后同步内置插件。
3. `make build` 将已经准备好的依赖打包。缺失、版本不匹配或同步不完整时直接失败。

新检出的仓库不需要预先存在 `output/browser`，也不需要先启动 Core。`init-system` 会从官方精确版本地址下载 Node/pnpm，并分别验证 SHA-256/SHA-512；`sync` 同样会补齐缺失的这两项前置依赖。系统 Node、pnpm 及其余开发工具仍按开发环境文档预先安装。

```powershell
.\make.ps1 init-system
.\make.ps1 sync
.\make.ps1 build
```

`output/browser` 在最后的打包阶段生成。如果仅该目录丢失，构建缓存和同步资源仍完整，重新执行 `make build` 即可；缓存也缺失时先执行 `make sync`。下载失败会停止并显示错误，不会启动 Core 后再补装浏览器。

网络下载只发生在构建准备阶段。生成的 `node_modules`、浏览器缓存及发行目录不提交 Git。

| 发行路径 | 用途 |
| --- | --- |
| `desktop/runtime/` | Electron GUI 及用户临时网页 |
| `browser/runtime/` | Core 浏览器宿主及 Playwright 驱动 |
| `browser/chromium/` | Core 独立 Chromium |
| `dependencies/runtimes/node/` | Core 宿主和 MCP 共用的受管 Node |
| `plugins/vulcan-browser/node/node_modules/` | 内置 MCP 及其依赖 |

Core 根据架构版本 2 的 `runtime-layout.json` 启动 `browserHostExecutable`，不再通过 Electron 的 Node 模式启动。首次使用时插件框架仍会复制到用户目录下、按版本与内容指纹隔离的私有执行代次；此过程完全本地进行。旧下载目录不会覆盖随包 Chromium，也不会被静默删除。

## 故障与验证

AI 浏览器面板保留浏览器、页面两层选择；每层只有一项时隐藏标签栏，唯一浏览器自动连接观察，不自动获取控制权。浏览器名称使用该客户端当前显示页面的标题；切换后名称、地址与画面对应同一页面，不使用会话中历史导航地址。视口区域保持固定布局，只有页面画面流随选择更换。底部 FPS 是最近 2 秒实际接收帧数除以 2，静止或断流后会归零。

只有一个浏览器时，实例标题整行隐藏，菜单位于地址栏。两层标签的图标位置均在悬浮时显示关闭按钮；确认后才获取必要的控制租约并关闭精确目标。单页通过 `browser.page.close` RPC 和宿主 `page.close` 释放真实页面，整浏览器通过 `browser.session.close` 关闭全部页面。取消不发出控制请求，关闭失败保留标签与错误；页面层被隐藏时，菜单仍提供关闭当前页面入口。

页面标题由宿主初始化脚本观察 `document.title`，经 `pageMetadataChanged` 事件写入 Core 页面注册表；普通导航与动态标题修改走同一链路。绑定只接受主框架、当前地址和已登记目标的展示元数据，不修改 AI 的目标页面或控制权。

内置组件缺失时，界面提示修复应用安装；私有插件执行代次损坏时可通过“修复本地环境”从随包文件重新准备。开发者应重新同步并完整构建。

正常回合结束只退休 MCP 连接和自动化授权，浏览器页面应继续存在。宿主异常退出后，异常浏览器保留在侧栏；下一轮清理陈旧连接不会清除浏览器的未知操作阻塞，也不会因此阻断普通会话。

宿主发生未捕获异常时，会在用户数据目录的 `browser-profiles/control/<浏览器标识>/browser-host-crash.json` 写入已脱敏的致命堆栈，保留 Node 默认的失败退出行为。排查时应在重启 Core 前保存该文件，因为启动清理会移除上一进程的临时浏览器目录。退出码本身不能区分脚本异常和外部进程终止；没有报告时不得推测具体异常。

宿主标准错误写入同目录的 `browser-host-stderr.log`，每次启动重新创建，不再继承 Core 控制台句柄。Windows 隐藏进程继承控制台 stderr 会在写入慢截图诊断时触发 `EBADF`；独立文件句柄使日志写入不受控制台开关影响。创建日志失败时直接拒绝启动并报告具体路径及错误。

定向验证入口：

- `node --test scripts/desktop-runtime.test.mjs scripts/browser-runtime.test.mjs scripts/managed-node.test.mjs`
- `cargo test -p vulcan-plugin-runtime runtime::environment::tests -- --test-threads=1`
- `cargo test -p vulcan-code-service runtime_layout::tests -- --test-threads=1`
- `cargo test -p vulcan-service-kernel real_hidden_node_retains_diagnostics_without_console_handles -- --ignored --test-threads=1`：通过生产启动器验证 Windows 隐藏 Node 的同步、异步诊断写入及正常退出。
- `cargo test -p vulcan-service-kernel real_core_host_plugin_mcp_browser_switch_revokes_old_grant -- --ignored --test-threads=1`：验证连续 AI 回合、授权撤销、手动接管点击及重新挂接 AI 后的实际页面状态。
- `node apps/vulcan-code-desktop/tools/bundled-browser-smoke.mjs <发行根绝对路径>`：包含本地页面动态标题上报、页面采集期间的连续 MCP 回合关闭与重新连接；可通过 `VULCAN_BROWSER_SMOKE_URL` 显式指定复现网址、`VULCAN_BROWSER_SMOKE_TURNS` 指定回合数。
- `node apps/vulcan-code-desktop/tools/bundled-browser-core-smoke.mjs <隔离测试发行根绝对路径>`：要求发行物位于仓库 `target/` 且包含 `test-support` 版 Core。测试临时排除默认 CodeKit 技能的网络引导，仅验证浏览器组件离线启用；随后移动宿主脚本验证缺失与恢复诊断。两项变更均在退出时恢复。
- `VULCAN_W08_ASSET_ROOT` 指向发行根后运行 `apps/vulcan-code-desktop/tools/w08-playwright-mcp-compatibility.mjs`，验证官方 MCP 导航、快照、点击、截图及授权撤销。

Windows 上执行真实进程验收；其他平台需要在对应宿主另行验证。

浏览器组件内置不改变其他功能的下载策略：全新 Core 的默认 CodeKit ROOT 技能仍按现有配置自动安装。无网络且该技能未安装时，Lua 初始化失败会阻止所有 Lua 插件启用；这属于独立的默认技能引导流程。
