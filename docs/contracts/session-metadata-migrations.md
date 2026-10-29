# 会话元数据版本

## 权威与入口

本契约归属于 `vulcan-session-core::v1::config::MetadataDocument` 与工作区存储层，修改其中任何持久字段、嵌套字段或读取入口时必须同步本文及回归测试。

- 当前版本只在 `METADATA_SCHEMA_VERSION` 定义；元数据家族仍为 `vulcan.session.v1`。
- 唯一自动读取入口：`session_v1::documents::read_metadata`，实现位于 `session_v1::metadata_access`。
- 普通发布仍经 `write_json_document`；精确 `metadata.json` 写入与显式能力重建共用 `metadata.write.lock`。
- 元数据版本与 `runtime.json`、`compactions.json` 的版本独立，不一起递增。

## 自动升级门槛（`AGENTS.md` 第一节第 12 条）

为发行做准备，自 2026-10-29 起会话协议的每次版本递增都必须带自动升级，V0.1.x 期间同样适用：

- 升级基线是 2026-10-29 时的版本：元数据 9、子执行配置 3。基线及之后的每个版本都必须能被后续版本自动升级，
  不能再出现“更新后旧会话无法打开”。
- 基线之前的版本没有迁移代码，仍按下文返回 `UnsupportedDevelopmentFormat`，并保留删除入口。

当前读取规则：

1. 读取先校验家族与版本。精确等于 `METADATA_SCHEMA_VERSION` 的文件直接按当前强类型解析；位于基线与当前版本
   之间的文件按下文“自动升级”逐版本转换。
2. 基线之前的版本与未来版本一律返回 `UnsupportedDevelopmentFormat`；不写入、不创建备份、不补默认值、不降级。
3. 显式供应商能力重建同样只接受当前版本：在锁内读取原字节、校验版本，仅替换 `config.selection.provider_tools`
   后按完整类型校验；发布由原重建流程负责。
4. 损坏 JSON、缺失必填字段属于损坏，不能被当成空会话。

版本历史（基线 9 之前的版本只作说明，没有迁移代码）：4 引入 `config.answer_mode`，5 引入 `mode.access.permission`
（`read_only` / `workspace_write` / `full_access`，新会话默认 `workspace_write`），6 引入必填可空的
`dedicated`（专职会话配置：工具开放、基础提示词与记忆锁定区间，见[专职会话](dedicated-sessions.md)），7 引入可选的
`mode.access.unattended`（无人值守模式，保存开启时用户确认的 `deny_policy` 快照，缺省表示关闭），8 删除
会话知识库选择及子执行访问中的检索策略字段，模式统一归知识库所有，9 在 `config.selection.provider_tools`
引入 `extension_modes`（模型扩展能力的单/多模型模式及单模型固定目标，缺失即未选模型的单模型模式）与
`changed_mid_session`（首次请求后修改过工具选择，需向模型发送工具配置通知）。子执行配置自身版本同时由 1 递增至
`CHILD_EXECUTION_PROFILE_VERSION`（当前为 3，版本 3 捕获扩展模式），仍保存在 `metadata.json` 的 `child_execution_profile` 中；
基线之前的版本不读取，也不迁移、补值或探测旧字段，这些本地会话无法打开。

基线之前的会话无法加载，因此也无法走“先归档再删除”。唯一清理入口是 `service_delete_unsupported_conversation`：
在会话生命周期守卫内先以 `session_v1_format_is_unsupported` 只读元数据确认其为 `UnsupportedDevelopmentFormat`，
再经 `delete_session_v1` 删除会话目录与索引记录。可读会话返回 `ERR_SESSION_FORMAT_SUPPORTED`，损坏或缺失仍是
错误，绝不当作旧格式处理。WebUI 在归档返回该错误码时弹出确认框，用户确认后才删除。
已归档的旧版本会话走普通的彻底删除：`delete_session_lifecycle_v1` 先以同一只读检查确认格式，旧格式直接经
`delete_session_v1` 删除目录与索引记录，可读会话仍加载完整树并执行协作检查。

发布锁是协作式锁，进程退出会自动释放；锁文件本身永久保留，不能因“看起来为空”而删除。

## 新增或修改字段的固定步骤

1. 修改强类型模型，将 `METADATA_SCHEMA_VERSION`（或对应的嵌套版本，如 `CHILD_EXECUTION_PROFILE_VERSION`）递增
   一位，禁止只增加必填字段却继续写旧版本。
2. 同时新增从上一版本到新版本的自动升级步骤：
   - 明确每个新字段的旧值来源；执行归属、授权、用户观察策略必须根据原始事实转换，不能猜测。无法从原始事实
     得出的字段必须先与用户确认取值规则，不能静默补默认值。
   - 升级在 `read_metadata` 的首次读取中自动完成：在 `metadata.write.lock` 内读取原字节，在同目录保留原字节
     备份，转换内存候选并按完整类型校验，再经原子发布写回。
   - 多个版本的差距按版本逐步升级，不跳步。
3. 补齐测试：旧形状升级、重复读取不再升级、未来版本拒绝、备份冲突、写入故障与并发首次读取。
4. 已升级到当前版本的文件缺字段仍属于损坏，不能添加全局反序列化默认值来掩盖。
5. 在本文“版本历史”追加一句说明。

## 定向验证

```powershell
rtk cargo test -p vulcan-workspace-store metadata_access --lib -- --test-threads=1
rtk cargo test -p vulcan-workspace-store workspace_store::session_v1::tests --lib -- --test-threads=1 --skip performance_
rtk cargo check -p vulcan-workspace-store --all-targets
```

新代码须编译并由新 Core 载入后才会执行新读取规则；仅修改源代码不会让已运行的旧 Core 获得新逻辑。
