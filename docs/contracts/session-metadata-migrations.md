# 会话元数据版本

## 权威与入口

本契约归属于 `vulcan-session-core::v1::config::MetadataDocument` 与工作区存储层，修改其中任何持久字段、嵌套字段或读取入口时必须同步本文及回归测试。

- 当前版本只在 `METADATA_SCHEMA_VERSION` 定义；元数据家族仍为 `vulcan.session.v1`。
- 唯一自动读取入口：`session_v1::documents::read_metadata`，实现位于 `session_v1::metadata_access`。
- 普通发布仍经 `write_json_document`；精确 `metadata.json` 写入与显式能力重建共用 `metadata.write.lock`。
- 元数据版本与 `runtime.json`、`compactions.json` 的版本独立，不一起递增。

## 发布阶段门槛（`AGENTS.md` 第一节第 12 条）

V0.1.x 是发布前版本，不保留任何元数据迁移：

1. 读取先校验家族与版本，只接受精确等于 `METADATA_SCHEMA_VERSION` 的文件，再按当前强类型解析。
2. 旧版本与未来版本一律返回 `UnsupportedDevelopmentFormat`；不写入、不创建备份、不补默认值、不降级。
3. 显式供应商能力重建同样只接受当前版本：在锁内读取原字节、校验版本，仅替换 `config.selection.provider_tools`
   后按完整类型校验；发布由原重建流程负责。
4. 损坏 JSON、缺失必填字段属于损坏，不能被当成空会话。

版本历史（仅说明，不再提供迁移代码）：4 引入 `config.answer_mode`，5 引入 `mode.access.permission`
（`read_only` / `workspace_write` / `full_access`，新会话默认 `workspace_write`）。更新到新版本后，
旧版本的本地会话将无法打开。

发布锁是协作式锁，进程退出会自动释放；锁文件本身永久保留，不能因“看起来为空”而删除。

## 新增或修改字段的固定步骤

V0.1.x 期间：

1. 修改强类型模型，将 `METADATA_SCHEMA_VERSION` 递增一位，禁止只增加必填字段却继续写旧版本。
2. 不新增迁移步骤、不写备份；已升级版本缺字段属于损坏，不能添加全局反序列化默认值来掩盖。
3. 在本文“版本历史”追加一句说明。

自 V0.2 正式发布起，持久格式变化必须重新建立逐版本迁移：明确每个新字段的旧值来源（执行归属、授权、
用户观察策略必须根据原始事实转换，不能猜测），升级前在同目录保留原字节备份，在锁内转换内存候选并经原子
发布写回，并补齐旧形状、重复读取、未来版本拒绝、备份冲突、写入故障与并发首次读取测试。

## 定向验证

```powershell
rtk cargo test -p vulcan-workspace-store metadata_access --lib -- --test-threads=1
rtk cargo test -p vulcan-workspace-store workspace_store::session_v1::tests --lib -- --test-threads=1 --skip performance_
rtk cargo check -p vulcan-workspace-store --all-targets
```

新代码须编译并由新 Core 载入后才会执行新读取规则；仅修改源代码不会让已运行的旧 Core 获得新逻辑。
