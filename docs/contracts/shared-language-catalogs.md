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
5. **迁移状态**：目前只有 Git 提交动作（`git/commit_action.rs`）的请求冲突使用该机制；其余服务错误仍为
   固定文本，按区域逐步迁移。Git、存储等底层诊断保持原文，不进入目录。

## 验证

```bash
cargo nextest run -p vulcan-i18n
cargo nextest run -p vulcan-service-api localized_message
```

WebUI 侧执行 `pnpm run typecheck` 与 `vitest run src/i18n-locales.test.ts`。
