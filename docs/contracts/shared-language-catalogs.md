# 共享语言目录

所有面向用户的界面（WebUI、TUI、服务、后续的托盘与桌面壳）共用 `crates/vulcan-i18n/catalogs/` 中的同一份
语言清单与文案目录。实现以该 crate 的 `build.rs`、`src/lib.rs` 与测试为准。

## 文件

| 文件 | 内容 |
| --- | --- |
| `catalogs/locales.json` | 唯一的语言注册表：`default`、`locales`（`code`、`nativeName`、`mark`，按展示顺序）与 `surfaces` 分面声明 |
| `catalogs/<surface>/<code>.json` | 一个分面在一种语言下的扁平键值目录，值全部为字符串，占位符写作 `{name}` |
| `catalogs/locale-matching.json` | 设备语言标签匹配、偏好列表检测与已保存选择校验的行为向量，Rust 与 TypeScript 测试共用 |

当前分面：

- `webui`（完整）：WebUI 全部文案；TUI 也复用这些键。
- `tui`（不完整）：TUI 专属 `tui.*` 键；缺少的语言按键回退到默认语言。

## 规则

1. **只有一份语言清单。** 服务（保存 `uiLocale` 时的校验）、TUI、WebUI 都从 `locales.json` 读取；不得在消费方
   另建语言列表或匹配规则。WebUI 的 `catalogLoaders` 因打包器需要字面路径而重复代码，由测试保证与注册表一致。
2. **默认语言补全一切。** 每个分面必须有默认语言文件；完整分面要求每种语言都覆盖默认语言的全部键（WebUI 在编译期
   通过 `satisfies` 检查，crate 测试再统一检查）；非默认语言不得出现默认语言之外的键，占位符集合必须一致。
3. **新增分面**：在 `locales.json` 的 `surfaces` 中声明，创建 `catalogs/<surface>/<default>.json`，并在 crate 测试的
   `SURFACES` 中登记；`build.rs` 为其生成同名大写静态表（例如 `TUI`），未使用的分面由链接器剔除。
4. **新增语言**：在 `locales.json` 追加条目，为每个完整分面补齐目录，在 WebUI `catalogLoaders` 增加一行；匹配规则有
   变化时同步更新 `locale-matching.json`。
5. **查找语义**：当前语言逐层查找，再默认语言逐层查找；无取值的 `{name}` 占位符保持原样。语言检测结果不得写回
   用户配置。

## 验证

```bash
cargo nextest run -p vulcan-i18n
```

WebUI 侧执行 `pnpm run typecheck` 与 `vitest run src/i18n-locales.test.ts`。
