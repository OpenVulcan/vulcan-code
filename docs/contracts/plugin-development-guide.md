# 插件开发指南

## 入口与权威

新插件使用当前第二版清单。从[清单编写与校验](plugin-manifest-authoring.md)进入，完整字段以[生成 Schema](plugin-manifest.schema.json)及 `vulcan-plugin-runtime/src/manifest.rs` 为准。
本指南说明开发顺序、性能选择、多语言与验证，不替代 Rust 的跨字段校验和真实运行验收。

上游版本以根 `Cargo.toml`、`Cargo.lock` 为准；运行时、清单类型、UI SDK 或语言注册表变化时须同步本指南。
`docs/vulcan-plugin-runtime-architecture.md` 的第一版清单描述属于历史设计，不应据此给新插件选择第一版。

| 要解决的问题 | 当前入口 |
| --- | --- |
| 清单、资源、依赖及显式迁移 | [清单编写](plugin-manifest-authoring.md) |
| 工具复用、固定会话与关闭 | [执行引擎](plugin-execution-engine.md) |
| 五个内置插件的性能适配 | [性能指南](plugin-performance-adaptation.md) |
| 宿主配置的容量修订 | [容量事务](plugin-capacity-policy-transactions.md) |
| 自动预热与保留回执 | [预热策略](plugin-automatic-prewarm-policy.md) |
| 普通 LuaSkills 技能与宿主注入 | [技能配置说明](luaskills-skill-config-guide.md) |

## 从真实业务选择入口

先明确插件处理什么业务、调用方是谁、状态保存在哪里，再选择清单能力。

| 入口 | 用途 | 状态与性能约束 |
| --- | --- | --- |
| 普通 Lua 工具 | 模型发起的独立操作 | 默认单次生命周期；需要 VM 复用时显式声明并实现清理 |
| 固定会话工具 | 调试、协议连接等连续操作 | 会话拥有 VM 和受管资源；保持所有者、工作区及代次亲和 |
| Hook | 宿主固定扩展点的注册、转换或调用治理 | 使用规定的输入输出及顺序，不能发明 Hook 阶段或取消安全校验 |
| UI Action | 设置、统计和面板交互 | 当前宿主有持久会话路径；按区域加载、保留请求身份、拒绝迟到结果 |
| MCP | 独立工具服务及其进程协议 | 遵守清单范围和宿主依赖管理；其进程/连接寿命与 Lua VM 分开考虑 |

内置示例在根 `plugins/`：Context7 提供普通工具和设置，调试插件展示固定会话与 sidecar，浏览器展示 MCP 与 Hook，RTK 展示受管进程 Hook，使用统计展示查询型 UI Action。
示例当前行为需要按源码理解，不能因为它已经能运行就认为它适合直接复制到不同业务。

## 包结构与能力注册

`plugin.yaml` 绑定包身份、版本、权限、运行时要求、Lua 导出、UI 挂载点、语言资源和依赖声明。
Lua 业务、配置 Schema、工具输入输出 Schema、UI 模块和依赖锁文件都应在包中有明确归属；路径来自清单，不依赖开发者机器上的绝对地址。

`runtime_requirements` 描述核心能力、宿主回调与容量要求；它是需求声明，不是授权。
宿主回调还必须在正式注册表中存在，版本、执行方式、作用域及权限均匹配。插件不能靠猜函数名、全局对象或兼容字段获得未声明能力。

需要新增宿主功能时，应同时设计并实现：

1. 稳定的能力名称、版本、请求与结果类型、错误语义、取消和资源归属。
2. 正式宿主注册、调用时权限校验、代次与修订冻结。
3. Lua 接口、清单需求、生成 Schema 和适用 SDK 的适配。
4. 模型工具或 UI Action 的完整暴露链、撤权及更新回归。

不要仅向全局 Lua 表注入一个函数就称为完整插件能力；也不要向插件 UI 暴露仅供宿主管理的容量和发布控制接口。

## 配置、状态与关闭

配置通过既有宿主接口取得，默认值遵循配置 Schema 与宿主策略，不在启动时偷偷写入用户配置。
调用参数、鉴权材料和响应缓冲属于请求；有意跨请求保留的模块缓存必须写明归属及失效条件。
固定会话的子进程、连接与文件句柄必须有明确关闭入口，并覆盖取消、超时、目标退出、禁用、卸载和更新。

只有声明可复用的普通工具才能使用逐调用清理契约。详细回调签名、失败语义及容量选择见[性能指南](plugin-performance-adaptation.md#正确声明复用)。
独立容量不等于独立进程；当前进程内后端不提供对恶意插件的进程隔离。

发布更新保留不可变代次和旧资源排空边界。旧工具不能自动指向新版本，旧 UI 的动作也不能静默迁移。
写入结果未知时先查询和核对原请求，不能自动生成新身份重放。插件私有业务数据迁移需另行设计备份、迁移与恢复，不由 VM 更新自动完成。

## 多语言资源与界面

### 语言集合只有一处权威

主界面语言来自 [SUPPORTED_LOCALES](../../apps/vulcan-code-web/src/i18n-locales.ts)。本次核对为英语、简体中文、繁体中文（台湾、香港）、日语、韩语、西班牙语、法语、德语、巴西葡萄牙语及俄语，共十一种。
内置插件的完整性测试直接读取这个注册表；不要在另一个实现中维护同义的硬编码语言列表。

所有内置插件显式声明每种语言。保留 `en` 作为既有兼容回退，并令 `en-US` 指向同一份 `locales/en.yaml`，不复制第二份英文资源。
以下仅展示映射方式，完整清单须覆盖注册表的全部语言：

```yaml
locales:
  en: locales/en.yaml
  en-US: locales/en.yaml
  zh-CN: locales/zh-CN.yaml
  zh-TW: locales/zh-TW.yaml
  zh-HK: locales/zh-HK.yaml
```

每份语言资源包括 `name`、`capability` 和可选的平面 `messages` 映射。没有 UI 消息的插件仍须本地化名称和能力说明。
消息键保持一致，值为非空字符串；品牌、协议标识和真实数据可以保留原文，不能把整份英文复制为其他语言充数。
YAML 中具有特殊字符的值须正确引用；禁止重复键。

### 宿主与插件的职责

Rust 目录加载声明的语言文件，并投影到插件描述及 UI 模块元数据。
宿主 `DynamicPluginMountPanel` 给插件提供当前 `sdk.locale` 与 `sdk.t(key, fallback)`；语言变化时重新创建挂载组件。
消息解析顺序为精确语言、同语言族、英文和调用点回退。内置插件应命中精确语言，不能用回退成功掩盖缺翻译。

具体权威路径：

- [目录加载](../../crates/vulcan-plugin-runtime/src/catalog.rs) 与 [语言类型](../../crates/vulcan-plugin-runtime/src/manifest.rs)。
- [UI 模块元数据](../../apps/vulcan-code-web/src/service/plugin-ui-module-runtime.ts)。
- [SDK 挂载与语言切换](../../apps/vulcan-code-web/src/components/plugins/dynamic-plugin-mount-panel.tsx)。
- [插件标签与能力说明](../../apps/vulcan-code-web/src/components/features/skills/skill-area-types.ts)。

`sdk.t` 取得消息文本；插件现有的文本辅助函数负责 `{{name}}` 形式插值。新增消息时保留各语言占位符名称及次数，不把翻译后的名称当作参数键。
不得将翻译内容作为 HTML 直接注入；普通 UI 文本由组件安全呈现。

### 不仅翻译按钮

- 设置说明、页签、空状态、确认提示、可访问标签、状态标签、图表说明和单位应使用消息键。
- 日期、数量、百分比等展示使用当前 `sdk.locale`。只有日期的业务值应按其真实日历语义处理，避免时区换算导致日期前移或后移。
- 比率、金额及精确十进制字符串保持数值语义，格式化不改变存储值、请求字段或协议的小数点约定；大整数不得先转浮点再格式化。
- 枚举标签可以本地化，提交的枚举值保持协议原值。
- 面向 AI 的工具名、描述、Schema、参数说明及工具错误保持英文。后端或外部进程原始诊断保留原始内容；本地化的是周围的用户提示，不任意翻译错误载荷。
- 长德语、法语和俄语标签与中文、日文的布局需求不同；检查换行、窄窗口和按钮可见性，不能只验证文件存在。

### 必要验证

在仓库根运行实际插件 UI 测试：

```text
rtk proxy node --test plugins/context7/ui/index.test.mjs plugins/vulcan-browser/ui/settings.test.mjs plugins/vulcan-rtk/ui/index.test.mjs plugins/vulcan-usage-statistics/ui/index.test.mjs
```

在 `apps/vulcan-code-web` 运行语言覆盖及宿主挂载测试：

```text
rtk pnpm exec vitest run src/components/plugins/bundled-plugin-locales.test.mjs
rtk pnpm run typecheck
```

[统一测试](../../apps/vulcan-code-web/src/components/plugins/bundled-plugin-locales.test.mjs) 从根 `plugins/` 发现包，检查十一种语言的映射、消息键、非空值及占位符，并使用实际语言资源验证宿主 SDK 逐语言切换和 UI 挂载标签。
它使用模拟服务和测试组件来隔离宿主边界；实际插件 UI 由上述 Node 测试覆盖。这些测试不证明真实服务端业务、桌面安装包或人工语言质量全部通过。

手工验收时启动使用同步资源的宿主，依次切换主界面语言，检查插件目录名称与能力说明、各设置页、浏览器面板、RTK 统计和使用统计。
确认语言切换后正常重新加载，选择项仍提交原枚举值，日期/百分比/金额展示正确。使用含变量的消息检查插值，使用空数据和出错状态检查提示。
新语言上线前应做母语审校；自动测试只能验证结构和已断言的展示行为。

## 从源码到本地运行

仓库内置插件的权威文件位于 `plugins/`。不要直接编辑生成的 `output/plugins/`。
修改语言、清单或运行时资源后在仓库根显式同步：

```text
rtk proxy pwsh -NoProfile -File ./make.ps1 sync
```

同步不会替代编译，也不会强制替换已经运行的插件代次或使所有客户端缓存失效。需要按宿主正常刷新、重载或重启流程观察新资源；不要通过删除用户插件配置来强行刷新。
`build`、`br`、`release` 不替代显式 `sync`。

清单和资源使用[包校验工具](plugin-manifest-authoring.md#命令与构建)验证。静态包校验不执行 Lua，不证明外部进程或网络已经可用。
业务或宿主协议变化后按具体入口增加调用、取消、撤权、关闭与更新测试；只改翻译不需要重跑所有跨平台发行或长期性能测试。

## 提交与发布

提交包含权威资源、对应测试和契约说明；排除编译缓存、临时数据库、依赖安装目录与验证日志。
先本地验证，再按项目既有版本和发布流程分发；不为了插件文案变化自动提升 LuaSkills 或 SDK 版本。
涉及新核心能力或 SDK 契约时，另行冻结兼容范围并执行核心 GitHub 发布、注册表和 SDK 发布顺序，不能把未发布的本地依赖路径写入正式清单。
