# 插件待处理操作登记表

中断的插件管理操作只有一个列表与继续入口。各类操作仍由原持久所有者保存（激活日志、存活卸载强槽、
策略回执），本契约只规定它们如何被统一列出、由谁声明可用动作、以及如何精确继续。

权威实现：`crates/vulcan-service-kernel/src/skills_services/plugin_pending_operations.rs`；
WebUI 适配：`apps/vulcan-code-web/src/service/plugin-pending-operations-service.ts`；
统一界面：`apps/vulcan-code-web/src/components/features/skills/plugin-recovery-center.tsx`。

## 类别

| `kind` | 原所有者 | `operationId` | `pluginId` | 列入条件 |
| --- | --- | --- | --- | --- |
| `update` | 本地安装／更新激活日志 | 原 `requestId` | 原插件 | 仍有可用动作；已结束且无动作的历史不列出 |
| `uninstall` | 存活卸载强槽 | 原 `requestId` | 原插件 | 强槽存在即列出；`executing` 阶段可见但无动作 |
| `capacityPolicy` | 正式运行时容量策略回执 | 原 `updateId` | 原插件 | 附加正式运行时且保留回执时 |
| `prewarmPolicy` | 自动预热策略回执 | 原 `updateId` | `null` | 附加正式运行时且保留回执时 |

首次安装与更新共用同一激活日志，因此同属 `update`。凭据修订没有待处理记录，不属于本登记表。
已从目录隐藏的插件照常列出；界面不需要也不提供按插件标识逐个探测的入口。

## 可用动作

可用动作只由内核在每类记录旁的单一规则推导，浏览器不推导资格，只渲染声明的动作：

- `update`：`resume` 与 `abort`，按持久阶段和存活所有权决定；`abort` 一律要求确认外部副作用不会被撤销。
- `uninstall`：非 `executing` 阶段提供 `resume`；是否要求确认取自原记录的确认标志。
- 两类策略：可确认的终态回执只提供 `forget`；否则提供 `resume`，仍处于准备阶段时另提供 `cancel`。

声明的动作不是授权；继续时原所有者仍按自身状态与锁重新校验，不一致即明确失败。

## 管理协议

| 方法 | 参数及结果 |
| --- | --- |
| `service_plugin_pending_operations` | 空具名对象；返回 `[{ kind, operationId, pluginId, actions: [{ action, requiresAcknowledgement }], detail }]` |
| `service_continue_plugin_pending_operation` | 按 `kind` 标记的精确继续请求；返回 `{ catalog }`，操作重新发布目录时为目录，否则为 `null` |

继续请求的字段形状：

- `update`：`operationId`、`pluginId`、`action`（`resume`／`abort`）、可选 `acknowledgeExternalEffects`。
- `uninstall`：`operationId`、`pluginId`、`action`（仅 `resume`）、可选 `acknowledgeExternalEffects`。
- `capacityPolicy`／`prewarmPolicy`：`operationId`、`action`（`resume`／`cancel`／`forget`）。

未知字段、数组参数、无效的类别与动作组合在访问服务前即拒绝。读取不启动工作、不创建存储、
不附加运行时；没有正式运行时时策略类别为空。`detail` 是各类原有脱敏投影，字段约定见各自契约。

## 界面约定

- 恢复中心每次扫描只读取一次登记表，再按 `kinds` 与插件范围筛选显示；全量读取及其校验不因筛选而改变。
- 仅更新放弃与卸载继续先经对话框审阅；卸载仅在声明要求时显示确认勾选框，且默认未勾选。
- 响应丢失时保留不确定状态，直到再次看到原操作；成功读取已不再列出它时，只允许用户明确“停止追踪”，
  不据此推断成功。卸载停止追踪与确认成功一样解除卡片锁定。
- 策略回执阻止新的策略开始，因此运行时容量与自动预热页面保留内联回执按钮，但按钮同样来自登记表声明的
  动作（按 `operationId` 与相同阶段匹配），不在浏览器重复规则。
