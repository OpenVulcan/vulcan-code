# 本地 Debug 应用数据库维护

## 适用范围与权威来源

首次正式发布前（当前 V0.1.x），应用 SQLite 数据库的 `APP_SCHEMA_VERSION`、
`PRAGMA user_version` 和 `app_metadata.schema_version` 保持 v1。结构变化直接更新当前 DDL，
再使用独立维护命令同步本地既有 Debug 数据库。应用启动不执行历史结构迁移，也不通过重标版本
掩盖结构差异。会话文档、会话元数据、系统统计、VCE/VMM 或知识库的独立存储格式不因此重置版本。

结构权威是 `crates/vulcan-workspace-store/src/data_plane.rs` 中的
`build_application_schema_script()`。维护程序在内存 SQLite 中执行该生成器，再读取
`sqlite_schema` 与 `pragma_table_xinfo` 获取实际定义，不复制第二份 DDL，也不解析 Rust 源码文本。
修改该生成器或数据库路径派生时，必须同时验证维护命令和本机 Debug 库。

数据库路径由应用与维护命令共用的 `application_sqlite_path()` 派生。Debug 的位置为
`~/.vulcan-debug/code/sqlite/app-workspace.sqlite3`；命令不接受任意路径、不创建缺失数据库，
也不跟随符号链接或 Windows 目录联接。Release 构建直接拒绝运行维护逻辑。

## 独立执行入口

在仓库根目录执行，先结束当前 Core 的工作并退出服务：

```powershell
rtk cargo run -p vulcan-workspace-store --features debug-db-maintenance --bin vulcan-debug-db -- --check
rtk cargo run -p vulcan-workspace-store --features debug-db-maintenance --bin vulcan-debug-db -- --apply
```

`--check` 只读检查，存在结构差异时返回非零状态并列出待更新的表和索引。`--apply` 是显式修改入口；
同步完成后再执行 `--check`，应报告当前结构且无修改。已同步的数据库再次执行 `--apply` 不创建备份
或改写数据。不要使用 `--all-features`：测试隔离特性会将用户路径重定向到测试专用目录。

这不是构建、资源同步、服务启动或发布的隐式步骤。`make sync` 同步运行资源，不维护用户数据库。
不存在的 Debug 库由应用首次启动创建；维护命令不会凭空创建空库替代既有数据。

## 同步与验收规则

1. 校验两个版本标记与当前应用常量一致，并检查原库完整性和外键。
2. 获取 `BEGIN IMMEDIATE` 写入保留锁，在同一事务内完成规划、快照、结构重建与验证。
   锁等待有明确上限；遇到正在写入的进程时失败，不强制结束该进程。
3. 通过独立只读连接执行 `VACUUM INTO`，在原 SQLite 目录内创建唯一的
   `app-workspace-before-sync-*.sqlite3` 完整逻辑快照，检查备份完整性。快照包括已提交的 WAL 数据。
   备份在后续失败时仍保留，可能包含凭据，沿用应用私有目录权限，不进入仓库或日志。
4. 只重建权威 DDL 中有差异的表和索引，从备份按精确列名复制值。事务期间关闭当前连接的外键动作，
   避免重建父表触发级联删除；提交前显式执行全库外键检查。
5. 对每张既有表比较所有保留列的值、SQLite 类型和重复次数，包括不属于基础 DDL 的会话索引及引擎表。
   再检查目标 DDL 一致性、版本标记、`integrity_check` 和 `foreign_key_check`，全部通过才提交。
   任意一步失败回滚源库；不得输出行内容、凭据或触发器生成的错误正文。

维护范围包括约束更新、新表和索引。新增缺失列、类型变更、未知多余列、生成列，以及重建表时存在的视图、
触发器或未映射关联索引，
必须在查证来源后补充明确映射和回归测试；命令直接拒绝，不自行推测字段用途、填充默认值或删除数据。

当前明确支持移除两个已无源码读取者的旧字段：`projects.codebase_memory_enabled` 与
`project_vce_settings.migrated_from_codebase_memory`。仅在其所有值均为 0 时允许移除，原值仍留在备份；
非零或空值必须停止并确定迁移语义。旧版可空 VCE 更新策略只有本身已符合新约束才能复制，禁止猜测默认值。

知识库内容改由 AI 管理后，另有两项明确移除映射：

- 允许移除 `knowledge_libraries.last_completed_import_at_ms`，包含非空值；原值完整保留在备份中，
  不推测转换为内容变更时间。当前变更时间来自每库不可变变更日志。
- 允许移除已废弃的 `knowledge_import_jobs` 及其既有库索引，但必须核对已查证的列形状，且不存在
  `queued` 或 `running` 行。原表保留在备份中，不将其计数转换成 `knowledge_operations`。
  未映射列、额外索引、视图、触发器及其他表指向该表的外键均明确拒绝，包含空的依赖表。

上述映射仅属于显式离线维护命令；应用启动、每库内容、套件与会话元数据都不新增旧格式兼容。
同步仍保持应用数据库 v1，并验证项目授权、既有通用操作及其他保留数据的值、类型和重复次数。

恢复时先退出 Core，保留失败库及其 WAL/SHM 文件，再使用 SQLite 工具从已验证快照恢复。
不能在运行中的服务旁覆盖单个主文件，也不能删掉库或备份来绕过结构错误。

SQLite 行为依据：[一致快照与 VACUUM INTO](https://www.sqlite.org/lang_vacuum.html)、
[事务重建与外键检查](https://www.sqlite.org/lang_altertable.html#otheralter)。备份是完整逻辑快照，
不承诺原始页布局或未声明为主键的内部 rowid 不变。

## 回归验证

```powershell
rtk cargo test -p vulcan-workspace-store --features debug-db-maintenance --bin vulcan-debug-db -- --test-threads=1
rtk cargo test -p vulcan-workspace-store --lib data_plane::tests -- --test-threads=1
```

夹具仅使用临时 SQLite 数据库。覆盖双密钥约束更新、保留值及类型、父表重建不级联丢失子行、可读备份、
幂等、只读检查不写文件、未知映射和视图/触发器依赖拒绝、复制失败整体回滚、WAL 备份、写锁争用及未知版本拒绝。
知识库回归另覆盖非零旧导入时间的备份保留、整张旧任务表移除、排队及运行任务拒绝，以及未知列、
额外索引和外键依赖拒绝；不对真实服务发起操作。
