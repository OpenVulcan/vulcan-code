# 会话载入与执行许可

## 归属

本契约约束服务启动恢复、历史会话读取、普通用户任务和显式继续之间的边界。修改启动恢复或提示准备入口时，应同步验证本文列出的回归。

## 规则

- 历史开放回合、持久运行标记和已经发布的业务钩子只证明历史归属，不能单独授权新的模型请求。
- 服务启动和打开普通历史会话不执行旧任务，包括此前失败、中断以及输入已保存但首个请求尚未发出的任务。
- 冷加载可将失去进程所有者的请求投影为中断；该投影必须先持久化再发布会话句柄，不能附带供应商调用。
- 用户点击继续，通过唯一继续动作标识、派发互斥及原请求的继续策略取得许可；禁止继续的错误不能被后台恢复绕过。
- 用户发送新任务，在确认旧执行者不存在后，可原子收口遗留回合并接纳新输入；不能仅因缺少旧回合结束标记而永久拒绝。
- 当前进程内正常的用户队列、方向引导、目标续跑及协作消息，使用各自的明确来源和准入入口。不得用“扫描所有开放回合”补偿任何特定来源的唤醒遗漏。
- 协作启动恢复可以依据具体消息、问题或子执行的持久凭据，按既有契约恢复该项工作或补齐结果投递；这不授予任意普通历史主任务新的执行许可。仅修复子任务回执不需要创建主业务钩子或派发主任务。
- 目标模式的启动暂停与用户显式恢复沿用现有契约。

## 当前实现与验证入口

| 职责 | 入口 |
| --- | --- |
| 只扫描明确协作待办 | `crates/vulcan-service-kernel/src/agent_collaboration/startup_recovery.rs` |
| 冷加载与中断持久化 | `crates/vulcan-service-kernel/src/session_v1_lifecycle.rs` 中的 `get_or_load_session_v1` |
| 显式继续动作 | `crates/vulcan-service-kernel/src/conversation_submission.rs` 中的 `service_continue_conversation_with_submission` |
| 已失去执行者后的新输入 | `crates/vulcan-session-core/src/v1/document/recovery.rs` 中的 `submit_root_input_after_execution_ended` |
| 六种普通历史状态及重复扫描 | `crates/vulcan-service-kernel/src/agent_collaboration/session_wait/tests/task_messages/recovery/main_business.rs` |
| 子树结果恢复不派发主任务 | `crates/vulcan-service-kernel/src/agent_collaboration/session_wait/tests/task_messages/recovery/embedded_tree.rs` |

## 持久化边界

本次修复没有改变持久字段或元数据版本。仅删除自动启动专用类型、方法与临时扫描字段；元数据中的开放回合指针仍用于历史重建和显式继续。不得为修复执行许可问题删除用户历史或改写原供应商错误。
