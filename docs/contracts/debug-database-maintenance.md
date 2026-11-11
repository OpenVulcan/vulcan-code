# 应用数据库升级与本地 Debug 维护

## 运行时逐版本升级

应用 SQLite 数据库的版本由 `crates/vulcan-workspace-store/src/app_schema_migrations.rs` 中的升级链
`APP_SCHEMA_MIGRATIONS` 唯一决定：`APP_SCHEMA_VERSION = APP_SCHEMA_BASELINE_VERSION + 链长度`，
不另写字面量。基线为 v1；`PRAGMA user_version` 与 `app_metadata.schema_version` 必须同时等于当前版本。

应用启动在执行当前 DDL 之前调用 `upgrade_application_schema`：

1. 新库（版本 0）与当前版本库不做任何事，也不创建备份。
2. 基线到当前版本之间的库先做 `integrity_check`，再用 `VACUUM INTO` 在同目录写出
   `app-workspace-before-v{目标}-from-v{原}-*.sqlite3` 完整快照并只读校验；失败时原库不变。
3. 在 `BEGIN IMMEDIATE` 事务内按顺序执行从原版本到当前版本的每一步，写入两个版本标记，复查完整性后提交；
   任一步失败整体回滚，备份保留。
4. 低于基线或高于当前版本的库直接报错，不改写、不降级。

升级步骤只追加不修改：已发布的步骤冻结自身 SQL，不引用会随当前 DDL 演进的常量。新增结构变化时，
同时修改当前 DDL 并追加一步，使升级后的结构与新建库一致（`upgraded_schema_equals_a_new_database` 校验）。
步骤只做已查证的明确映射，不猜测字段用途或补默认值。会话文档、会话元数据、系统统计、VMM 或知识库的
独立存储格式不受此链约束。

v1→v2：新增 `project_session_preferences`（项目会话排序方式，缺行即默认 `last_request`），并在会话定位
索引存在时新增 `session_list_state_v1`（最近请求时间与手动位置），以 `updated_at_ms` 回填最近请求时间。

v4→v5：凭据表接受 `tokenrhythm_auth` 材料（托管密钥存 `secret_value`、登录会话存 `provider_device_id`、
客户端注册存 `oauth_client_id`，令牌沿用通用令牌列）。SQLite 不能原地修改 CHECK，因此按“改名旧表、以原名
建新表、按列名逐列复制、删除旧表”重建；不增删或转换任何列，该表没有索引、触发器或外部引用。

v5→v6：新增 `project_attached_folders`，按项目保存本机文件夹的规范路径。现有项目初始没有附属目录，项目记录删除时级联移除关联。

## 本地 Debug 离线维护的适用范围与权威来源

运行时升级处理版本递增。离线维护命令只处理同一版本内的结构漂移（开发期直接修改当前 DDL 而未递增版本），
要求库已处于当前 `APP_SCHEMA_VERSION`，不进入 Release 启动及发布流程。

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

OpenCode Console 授权增加 `opencode_console_oauth` 材料类型及必要计费工作区约束，不新增列。
既有 `account_subject` 保存自然人主体，`account_id` 保存供应商计费工作区；同步旧表约束时仍按原列
逐值保留，不从既有 Key 猜测组织或自动转换凭据模式。

当前明确支持移除已无源码读取者的旧字段 `projects.codebase_memory_enabled`，仅在其所有值均为 0 时
允许移除；非零或空值必须停止并确定迁移语义。

代码导航退役还明确允许删除整张 `project_vce_settings` 表，原始记录只保留在完整备份中。
该映射逐列核实已退役设置的列名与类型，包含历史 `migrated_from_codebase_memory` 标记；不再单独迁移
该标记或推测更新策略默认值。未知列、生成列、额外索引、关联外键以及相关视图或触发器都会使维护停止。
表身份与外键引用按 SQLite 的 ASCII 大小写不敏感规则核对，不能由空子表或不同大小写绕过依赖检查。
此映射的权威是维护工具与退役回归测试；再次变更历史映射时须同步本文。磁盘上的旧代码索引目录不自动删除。

知识库内容改由 AI 管理后，另有两项明确移除映射：

- 允许移除 `knowledge_libraries.last_completed_import_at_ms`，包含非空值；原值完整保留在备份中，
  不推测转换为内容变更时间。当前变更时间来自每库不可变变更日志。
- 允许移除已废弃的 `knowledge_import_jobs` 及其既有库索引，但必须核对已查证的列形状，且不存在
  `queued` 或 `running` 行。原表保留在备份中，不将其计数转换成 `knowledge_operations`。
  未映射列、额外索引、视图、触发器及其他表指向该表的外键均明确拒绝，包含空的依赖表。
  表与索引身份同样按 ASCII 大小写不敏感规则核对。

上述映射仅属于显式离线维护命令；每库内容、套件与会话元数据都不新增旧格式兼容。
同步不改变应用数据库版本，并验证项目授权、既有通用操作及其他保留数据的值、类型和重复次数。

恢复时先退出 Core，保留失败库及其 WAL/SHM 文件，再使用 SQLite 工具从已验证快照恢复。
不能在运行中的服务旁覆盖单个主文件，也不能删掉库或备份来绕过结构错误。

SQLite 行为依据：[一致快照与 VACUUM INTO](https://www.sqlite.org/lang_vacuum.html)、
[事务重建与外键检查](https://www.sqlite.org/lang_altertable.html#otheralter)。备份是完整逻辑快照，
不承诺原始页布局或未声明为主键的内部 rowid 不变。

## 回归验证

```powershell
rtk cargo test -p vulcan-workspace-store --features debug-db-maintenance --bin vulcan-debug-db -- --test-threads=1
rtk cargo test -p vulcan-workspace-store --lib data_plane::tests -- --test-threads=1
rtk cargo test -p vulcan-workspace-store --lib app_schema_migrations -- --test-threads=1
```

夹具仅使用临时 SQLite 数据库。覆盖双密钥约束更新、保留值及类型、父表重建不级联丢失子行、可读备份、
幂等、只读检查不写文件、未知映射和视图/触发器依赖拒绝、复制失败整体回滚、WAL 备份、写锁争用及未知版本拒绝。
知识库回归另覆盖非零旧导入时间的备份保留、整张旧任务表移除、排队及运行任务拒绝，以及未知列、
额外索引和外键依赖拒绝；不对真实服务发起操作。
