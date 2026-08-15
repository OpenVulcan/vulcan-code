# 第三方价格数据来源声明

本插件的默认模型价格目录包含从以下开源项目公开源码中提取并转换的数据：

- 项目：CC Switch
- 官方仓库：`https://github.com/farion1231/cc-switch`
- 冻结发布：`v3.19.2`
- 源码提交：`43eaf07355af145aebfee301801779e824d4c221`
- 源文件：`src-tauri/src/database/schema.rs`
- 源符号：`Database::seed_model_pricing`、`Database::repair_current_model_pricing`
- 原始许可：MIT，完整副本见 `pricing/licenses/CC-SWITCH-MIT.txt`

这些数据仅作为 Vulcan 统计插件的版本化参考输入。转换后的运行时目录采用 Vulcan 自有 Schema、精确模型标识匹配与内容寻址 revision；Vulcan 不链接、不加载、不调用 CC Switch，也不依赖其本地配置、数据库或更新服务。

价格数据可能过期或与实际 Provider 账单不同。估算结果仅用于插件即时展示，不写入系统统计数据库，也不得替代 Provider 的正式账单。
