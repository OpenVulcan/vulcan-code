# 共享语言目录

所有面向用户的界面（WebUI、TUI、服务、托盘与桌面壳）共用 `crates/vulcan-i18n/catalogs/` 中的同一份
语言清单与文案目录。实现以该 crate 的 `build.rs`、`src/lib.rs` 与测试为准。

## 文件

| 文件 | 内容 |
| --- | --- |
| `catalogs/locales.json` | 唯一的语言注册表：`default`、`locales`（`code`、`nativeName`、`mark`，按展示顺序）与 `surfaces` 分面声明 |
| `catalogs/<surface>/<code>.json` | 一个分面在一种语言下的扁平键值目录，值全部为字符串，占位符写作 `{name}` |
| `catalogs/locale-matching.json` | 设备语言标签匹配、偏好列表检测与已保存选择校验的行为向量，Rust 与 TypeScript 测试共用 |

当前分面：

- `webui`（完整）：WebUI 全部文案；TUI 也复用这些键。
- `boot`（完整）：WebUI 启动屏文案，在完整目录下载前即可显示，因此单独成面并静态打包。
- `desktop`（完整）：Electron 桌面壳文案。构建（`pnpm run build`）与打包（`scripts/desktop-runtime.mjs`）通过
  `apps/vulcan-code-desktop/tools/copy-i18n-catalogs.mjs` 把 `locales.json` 与该分面复制为 `gui/` 旁的 `i18n/`；
  启动时缺失或默认语言缺字段直接失败。
- `tray`（完整）：服务托盘菜单、状态行与失败通知。托盘以已保存的 `uiLocale` 为准，未保存时使用启动时的系统语言；
  每次状态刷新读取一次，任一客户端切换语言后菜单文字随即更新。
- `tui`（不完整）：TUI 专属 `tui.*` 键；缺少的语言按键回退到默认语言。
- `oauth`（完整）：宿主返回的本地授权结果单页，编译进服务，不进入工作台首屏语言分块。

## 规则

1. **只有一份语言清单。** 服务（保存 `uiLocale` 时的校验）、TUI、WebUI、桌面壳都从 `locales.json` 读取；不得在消费方
   另建语言列表。匹配规则在 Rust、WebUI 与桌面壳各有一份实现，行为以 `locale-matching.json` 为准，三者测试都运行这组
   共享向量。WebUI 的 `catalogLoaders` 因打包器需要字面路径而重复代码，由测试保证与注册表一致。
2. **默认语言补全一切。** 每个分面必须有默认语言文件；完整分面要求每种语言都覆盖默认语言的全部键（WebUI 在编译期
   通过 `satisfies` 检查，crate 测试再统一检查）；非默认语言不得出现默认语言之外的键，占位符集合必须一致。
3. **新增分面**：在 `locales.json` 的 `surfaces` 中声明，创建 `catalogs/<surface>/<default>.json`，并在 crate 测试的
   `SURFACES` 中登记；`build.rs` 为其生成同名大写静态表（例如 `TUI`），未使用的分面由链接器剔除。
4. **新增语言**：在 `locales.json` 追加条目，为每个完整分面补齐目录，在 WebUI `catalogLoaders` 增加一行；匹配规则有
   变化时同步更新 `locale-matching.json`。
5. **查找语义**：当前语言逐层查找，再默认语言逐层查找；无取值的 `{name}` 占位符保持原样。语言检测结果不得写回
   用户配置。

## 更新记录

“关于”分组中的更新记录同样属于该 crate，按同一份语言注册表分语言维护，但它是逐版本的正文而非键值文案，
因此不放在 `catalogs/`，也不是分面：

| 文件 | 内容 |
| --- | --- |
| `changelog/releases.json` | 发布清单：`releases` 按从新到旧排列，每项 `version`（严格递减的 `x.y.z`）与 `date`（`YYYY-MM-DD`；只有最新一项在发布前可为 `null`，界面显示“未发布”） |
| `changelog/<version>/<code>.md` | 一个发布在一种语言下的说明：普通 Markdown，可使用 `ui-*` 回答组件（语法见 [回答模式与回答组件](answer-mode-and-components.md)） |

规则：

1. **每个发布覆盖全部语言。** `build.rs` 要求每个已列出的发布恰好拥有每种注册语言一个非空文件，缺失、多余的
   文件或未列出的版本目录都使构建失败；没有语言回退。
2. **版本号变化必须先写说明。** crate 测试要求 `releases[0].version` 等于工作区版本
   （根 `Cargo.toml` `[workspace.package].version`）；提升版本时在清单顶部加入新发布并为全部语言写好说明，
   发布当天把 `null` 换成实际日期。
3. **组件必须有效且不使用 `ui-code`。** WebUI（`src/service/changelog.test.ts`）与 TUI
   （`ui/answer_components.rs` 测试）分别用自身解析器检查全部说明的每个 `ui-*` 围栏；`ui-code` 只在回答封口时
   抓取文件行，更新记录没有这一步，因此禁止使用。说明页没有输入区，`ui-decision` 只读显示。
4. **消费方。** WebUI 经 `@changelog` 别名按发布与语言分块懒加载（`src/service/changelog.ts`），设置 → 关于 →
   更新记录页每页一个发布并可翻页；TUI 读取编译期内嵌的 `vulcan_i18n::RELEASES`，在同名页面镜像该行为。
   两者都不经服务调用。

## 服务错误消息

服务向用户报告可自行修正的拒绝原因时，不再返回固定语言文本，而是返回 `webui` 目录键及占位符取值：

1. **构造**：服务端以 `vulcan_service_api::WebuiLocalizedMessage::new("<key>").with("<name>", value)` 构造消息，
   通过 `WebuiServiceRpcError::Localized` 返回；错误类别码仍为 `service_rpc_error`。键必须是字符串字面量。
2. **线协议**：RPC 失败信封的 `error` 在 `code` 与 `message` 之外携带可选 `localized: { key, params? }`；
   `message` 是默认语言渲染结果，供日志和无法翻译的客户端使用。
3. **客户端**：WebUI（`browserTransport.ts`）与 TUI（`backend/service/rpc.rs`）认识该键时按当前语言渲染，
   不认识时显示 `message`；客户端不得依据错误文本做流程判断。
4. **校验**：`vulcan-service-api` 的 `localized_messages_match_the_webui_catalog` 扫描 `crates/` 与 `apps/`
   中的构造调用，要求键存在于默认语言目录且填充的占位符与目录声明一致。
5. **分级**：界面上的标题、标签、说明与操作回馈等展示文案走目录（由前端按稳定标识或元数据渲染，或由后端
   以目录键返回）；错误消息与 JSON 诊断只返回英文文本，不进入目录。后端不写中文文本，
   `scripts/backend-cjk-literals.mjs` 按文件棘轮约束 Rust 生产代码中的 CJK 字面量；基线中剩余的条目是
   尚待迁移的旧文本，或必须匹配 CJK 输入的数据（查询关键词、分词与隐私规则等）。

## 验证

```bash
cargo nextest run -p vulcan-i18n
cargo nextest run -p vulcan-service-api localized_message
```

WebUI 侧执行 `pnpm run typecheck` 与 `vitest run src/i18n-locales.test.ts`。

## 开发热更新与桌面故障恢复

- WebUI 的语言上下文与基础组件文案上下文在开发模式中通过 Vite 的 `hot.data` 保留对象标识，
  防止热更新后的消费者与已挂载的提供者使用不同上下文；不向用户配置写入任何值。
- 已加载目录与订阅关系同样跨热更新保留；新目录加载成功后统一通知消费者，相同语言代码也会刷新。
  刷新期间保留上一次成功的目录，旧请求不能覆盖新请求的结果。
- 用户切换语言时先加载目录，成功后才保存选择；加载失败保留原语言并显示现有失败提示。
- 桌面渲染进程退出后的恢复提示由 Electron 主进程使用 `desktop` 分面显示，不依赖 React、
  WebUI IPC 或已经失效的页面按钮。恢复只重新加载工作台，不重新发送用户消息、不重启 Core。
- 原生退出原因和退出码记录到 Core 声明的数据目录中的 `logs/desktop-renderer.jsonl`，
  日志不包含服务发现令牌、页面地址及会话正文。
