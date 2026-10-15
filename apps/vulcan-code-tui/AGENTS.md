# Vulcan Code TUI Agent Notes

本文件只保存修改 `apps/vulcan-code-tui` 的会话必须知道的长期约定，仓库根 `AGENTS.md` 的约定同样适用。
结构说明见 [`tui-design.md`](../../docs/contracts/tui-design.md)，页面与 RPC 对照见
[`tui-service-adaptation.md`](../../docs/contracts/tui-service-adaptation.md)。

## 一、边界

1. **工作区成员。** TUI 是根工作区的 crate，沿用工作区依赖、lint 与 `workspace-hack`；协议、类型与 i18n 的
   权威来源是核心服务与 WebUI，TUI 跟随它们，不在本地另造同功能、不同名的接口或文案。
2. **两个后端，同一边界。** `src/backend/service/` 连接真实服务（默认），`src/backend/mock/` 按脚本模拟；
   UI 与 `app/` 只通过 `backend::Command` / `BackendEvent` 交互，不直接访问后端内部。系统页面使用
   `service_api::ServiceCall` 一对一镜像 WebUI 函数、方法与参数，未有服务契约的动作不创建入口。
3. **插件 UI 只渲染声明式设置版面。** 系统插件（记忆、知识库、代理、技能）以声明式页面
   实现；LuaSkills 与 Vulcan 插件做管理（启用、工具模式、安装、卸载）。插件详情挂载中 `kind: settings`
   的版面由 `app/pages/plugin_settings.rs` 按 `plugin-ui-authoring.md` 渲染，所有读写都经插件声明的
   Action（`invoke_plugin_action`），只读取 `tui` 提示；`kind: module` 的 ESM 模块只列出、不执行。
4. **不保存机密。** 凭据只在临时表单中遮蔽输入并单次发送；界面展示公开连接元数据，原值不回显。服务发现文件
   （`~/.vulcan*/code/webui-service.json`）只由 `service/discovery.rs` 以程序读取；令牌不得出现在日志、提示、
   错误、`Debug` 输出或终端命令输出中，调试时也不得打印该文件或 `/health` 响应。

## 二、代码约定

1. 注释先英文一行、再中文一行；面向 AI 的文本使用英文，面向用户的文案走 i18n。
2. 所有副作用经 `app::action::perform(app, UiAction)` 单一路径；键盘与鼠标最终调用同一函数。
3. 鼠标：视图绘制时用 `app.hits.add(rect, Target)` / `add_scroll` 登记，最新登记优先；每帧开头
   `hits.reset()`。新增可交互元素必须同时登记命中目标。
4. 浮层清屏使用 `ui::text::clear_area`，不要直接 `Clear.render`（否则宽字符会遮住浮层左边框）。
5. 同一语义的边界值只允许一处定义。
6. 系统页面按区域放在 `src/app/pages/<area>.rs`，只产出 `Row`；渲染与输入由 `ui/pages.rs` 与
   `app/input/pages.rs` 通用处理，新增行类型时两处同时补齐。
7. 原始 WebUI DTO 放在 `app.slots`。`Load` 替换准确槽位，`Mutate` 用最近登记的加载器重新读取，
   `Preview` 先展示影响再确认；完整设置 DTO 保存必须保留同级字段，不恢复旧系统快照、虚构设置键或设置草稿。
8. 两步调用冻结服务签发的版本、指纹、目标集合与幂等身份。VMM 项目确认复用预检命令标识；
   知识套件导入冻结原生归档路径及完整文件指纹，只可编辑新库名称且不覆盖既有库。
9. 异步读取同时检查连接代际与槽位序号；选择改变后拒绝旧项目、知识库或画像目标的结果。
   变更后权威刷新失败时清空旧 DTO，完成刷新后才解除完整文档写入门禁。
10. 机密清理按真实字段语义执行，不能清掉文件键、仓库标识、安装密钥名称或 MCP 环境变量名称引用。
    MCP 请求头、环境值及安装初始密钥均清空；后端错误中的已知请求密钥及发现令牌必须脱敏。
11. 知识库正文与目录只读，模式草稿在明确保存前仅存于进程内；不提供来源扫描或整理语料。
    目录游标、历史序号、镜像偏移和正文续读值取自服务。已提交变更不得因刷新失败重放，自动刷新不得重发付费搜索；
    内容头或设置变化撤销旧搜索。任务读取失败或未知时不授权任务变更，详见根知识库契约。

## 三、多语言

1. 只用 `tr!("key", name = value)` 字面量键；占位符写作 `{name}`。
2. 文案与语言清单来自共享目录 `crates/vulcan-i18n/catalogs/`（见
   [`shared-language-catalogs.md`](../../docs/contracts/shared-language-catalogs.md)）：先查 `webui` 分面，
   再查 `tui` 分面；不在 TUI 内另建语言清单或文案副本。
3. TUI 专属文案使用 `tui.*` 键，写入 `catalogs/tui/<locale>.json`；其他语言缺失时按键回退到 en-US。
4. `src/i18n/` 的测试要求每个 `tr!` 键都能解析，且 `tui` 分面没有无人使用的键。

## 四、验证

1. 根 `.cargo/config.toml` 限制 `build.jobs = 16` 与 `RUST_TEST_THREADS = 4`，不得绕过。
2. 常规验证：`cargo clippy -p vulcan-code-tui --all-targets -- -D warnings`、
   `cargo nextest run -p vulcan-code-tui`、`cargo fmt --check`。
3. `src/render_smoke_tests.rs` 以无头后端在宽、窄尺寸下渲染全部区域与浮层（快照另含超宽尺寸），并用 `hits.find` 驱动鼠标点击；
   需要目视检查布局时先创建快照目录，再设置 `VULCAN_TUI_SNAPSHOT_DIR=<目录>` 运行该测试。
4. 默认使用模拟数据或本地合成 HTTP/WebSocket 服务器验证；不得对真实服务执行有副作用或付费的操作。
   模拟后端未知写入的成功回执不能证明真实协议已适配。
