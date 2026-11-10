# 第三方价格数据来源声明

本插件的默认模型价格目录包含从以下开源项目公开源码中提取并转换的数据：

- 项目：CC Switch
- 官方仓库：`https://github.com/farion1231/cc-switch`
- 冻结发布：`v3.19.2`
- 源码提交：`43eaf07355af145aebfee301801779e824d4c221`
- 源文件：`src-tauri/src/database/schema.rs`
- 源符号：`Database::seed_model_pricing`、`Database::repair_current_model_pricing`
- 原始许可：MIT，完整副本见 `pricing/licenses/CC-SWITCH-MIT.txt`

Vulcan 保留这份上游快照作为许可和数据来源记录。生成随包目录时，用供应商官网报价覆盖已核实的同名条目，并按模型首次发布日移除早于两年截止日的旧参考价。官网链接、报价适用范围和移除依据见 [官网价格数据](upstream/official-model-pricing.v1.json)。

运行时目录使用 Vulcan 自有 Schema、精确模型标识匹配和内容寻址修订号。所有估算均由统计插件本地计算。

价格数据可能过期或与实际 Provider 账单不同。估算结果仅用于插件即时展示，不写入系统统计数据库，也不得替代 Provider 的正式账单。
