# 插件清单校验、生成契约与显式迁移

## 契约归属

完整清单结构的权威是 `manifest.rs`、`manifest/requirements.rs`、`manifest/reuse.rs`，
Hook 字段直接引用 `vulcan-hooks` 的生产枚举，运行时枚举直接引用 LuaSkills。
`manifest_authoring.rs` 从这些类型生成完整 Schema 与两份片段，并加入清单版本及复用类别约束。
生成器固定使用工作区的 Schemars 版本及 JSON Schema 2020-12，选择反序列化契约；
行为依据见 [Schemars 生成设置](https://docs.rs/schemars/1.2.2/schemars/generate/struct.SchemaSettings.html)。
修改类型、Serde 属性、版本规则或生成器版本时，必须重新生成并运行同步检查。

生成产物：

- [完整插件清单](plugin-manifest.schema.json)；
- [运行时必需条件](plugin-runtime-requirements.schema.json)；
- [无状态工具复用声明](plugin-tool-reuse.schema.json)。

这些文件描述全部字段、枚举、字段存在性、类型及版本组合，不是插件激活许可。
跨字段数值关系、唯一身份、支持的后端、资源文件、授权、宿主预算及实时回调仍由生产 Rust 校验。
编辑器通过 Schema 不等于插件通过包检查，也不等于真实 Lua、外部进程或网络服务已经成功运行。

## 命令与构建

编写工具通过可选 `manifest-schema` 功能构建，普通产品运行时不启用该生成依赖。
以下为正式依赖可用后的标准仓库命令；开发阶段的隔离联调仍须显式使用候选核心配置，
不能将本地路径写入正式依赖或发布锁文件。

```text
cargo build --locked -p vulcan-plugin-runtime --features manifest-schema --bin vulcan-plugin-manifest
cargo run --locked -p vulcan-plugin-runtime --features manifest-schema --bin vulcan-plugin-manifest -- schema-check docs/contracts
cargo run --locked -p vulcan-plugin-runtime --features manifest-schema --bin vulcan-plugin-manifest -- schema docs/contracts
cargo run --locked -p vulcan-plugin-runtime --features manifest-schema --bin vulcan-plugin-manifest -- validate plugins/example
```

`schema-check` 逐字节比较三份产物且不写入；`schema` 是作者明确请求的生成操作，
三份产物的 Git 属性固定使用 LF 换行，避免 Windows 自动转换使检查失败。
每份文件通过同步的同级临时文件发布。三文件生成不是跨文件事务；中断后重新生成并运行检查即可。
持续集成包含生成结果检查及迁移专项测试。

`validate` 同时检查无歧义清单和生产包资源，包括原有路径、依赖声明及工具 Schema 检查。
成功仅表示该本地包通过静态检查，不创建运行时、不申请权限、不安装依赖、不执行 Lua。
校验期间清单字节变化会明确报错；本工具不声称锁定并发编辑中的整个插件目录。

## 从第一版生成第二版

用户插件的启动、目录刷新和普通调用不改写清单。迁移仅由作者显式执行：

```text
vulcan-plugin-manifest migrate <源清单路径> <不存在的输出路径> [运行时要求文件]
```

源清单必须为第一版，身份必须匹配其所在包目录名。Lua 插件必须提供完整的运行时要求文件；
非 Lua 插件不得提供该参数。未知版本、重复字段或映射键、非字符串映射键、YAML 标签和不完整要求直接拒绝。
合法的普通 YAML 标量、数组和字符串键映射保持其值，不通过候选字段或默认值猜测作者意图。

供作者明确选用的最小要求示例：

```yaml
protocol_version: 1
backend: in_process
core_capabilities: []
host_capabilities: []
capacities: []
```

空列表表示作者没有增加相应的能力或资源下限，不能视为无限制权限、自动选择专用池或豁免预算。
若插件依赖特定宿主回调或必须获得独立容量，应完整填写对应要求，再评审迁移结果。

迁移只提升清单版本，并为 Lua 插件加入明确提供的要求对象；既有权限、依赖、会话和无状态入口、
超时与包版本保持原值。不会自动添加热复用、并行声明或宿主配置。输出使用规范化 YAML，
不保留注释、原有排版或键顺序；原文件字节始终保留。

输出只允许新文件。已有目标、源文件自身、指向原件的硬链接或其他已有链接均拒绝覆盖。
内容先写入并同步同级临时文件，再以不覆盖方式发布。
临时文件库不承诺所有平台和文件系统上的完整原子性；异常退出可能留下临时链接，
但不会因此覆盖已有目标，详情见 [临时文件发布契约](https://docs.rs/tempfile/latest/tempfile/struct.NamedTempFile.html#method.persist_noclobber)。

作者应先审阅输出，在版本控制中明确替换开发副本清单，然后执行 `validate`，
运行真实调用与生命周期回归，再通过正常的插件更新流程发布。
迁移命令不会将候选文件自动安装进正在运行的插件，也不代替不可变代次更新及回滚事务。
内置插件的逐个清单升级和运行验收是独立交付步骤。

## 验证边界

测试使用实际随附插件进行完整结构对照；额外覆盖版本一／二差异、明确空值、复用类别、
传输变体未知字段、重复动态键及保留原声明。
实际文件测试验证硬链接保护、并发输出竞争、产物漂移检查，以及迁移包再次通过生产静态检查。
本地 Windows 通过不能替代 Linux、macOS、长期稳定性或 GitHub 发布产物验收。
