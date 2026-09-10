# 文件工具 ChangeSet 与执行失败生命周期契约

## 一、适用范围

本文记录原生文件工具、Agent Runtime、Vulcan Core、Provider V1 会话 owner 与 WebUI 之间的稳定边界。适用于 `file`、`file_edit`、`apply_patch` 产生的文件变更结果，以及这些结果在工具恢复边界上无法可靠持久化或投影时的失败收口。

本文只描述当前源码已经实现的契约，不放宽 `vulcan-change-set` 的严格解析规则，也不要求把历史失败会话改写为新格式。

## 二、宿主结果与持久化 ChangeSet

原生工具成功结果包含模型可见正文和宿主专用结果。宿主结果的稳定封套为：

```json
{
  "kind": "change_set",
  "payload": {
    "mode": "applied",
    "files": []
  }
}
```

`payload.files` 中每条记录只能使用一种内容表达方式：

| 变更类型 | 必需表达 | 禁止表达 |
| --- | --- | --- |
| `create` | `content_mode` 与对应生命周期正文 | `hunks` |
| `delete` | `content_mode` 与对应生命周期正文 | `hunks` |
| 纯 `rename` | `old_path`、`new_path` 与生命周期正文 | 空修改块或伪造修改块 |
| `modify` | 一条或多条 `hunks` | 生命周期正文 |
| 带内容修改的 `rename` | `old_path`、`new_path` 与真实 `hunks` | 同时携带生命周期正文 |

生命周期正文有三种模式：

- `full`：包含完整 `content` 与准确的 `total_line_count`。
- `truncated`：包含按字符边界生成的 `content_head`、`content_tail` 与准确的 `total_line_count`。
- `omitted`：用于不能声明文本正文的资源；不得虚构内容。

空文件创建仍属于生命周期创建：正文为空、逻辑行数为零、`hunks` 为空。把已有文件清空属于修改：必须保留修改块，不能伪装成创建。

## 三、文件提交前校验

所有实际写盘入口必须遵循相同顺序：

1. 根据已读取快照构造完整 `PreparedChange`。
2. 用即将返回的真实 `file_payload` 组装宿主 ChangeSet 封套。
3. 调用 `vulcan_change_set::persistent_change_set_from_host_result` 执行规范解析与校验。
4. 重新核对文件版本、目标存在性与父目录等前置条件。
5. 事务性提交全部文件变更；部分失败时按既有备份机制回滚。
6. 仅在提交成功后返回已经通过校验的同一组文件记录。

校验失败必须返回 `invalid_change_set`，且不得创建、删除、覆盖或移动任何目标。批次中任意记录无效时整批不提交。

预览模式同样校验最终公共 ChangeSet，但不得执行文件系统提交。

## 四、工具恢复屏障

工具批次准备、工具开始和工具完成都可以携带执行必需的投影屏障。等待方必须区分以下结果：

- `Projected`：普通投影已完成。
- `ToolBatchPrepared`：批次已可靠持久化，并返回精确的本地 API、工具与事件身份。
- `Rejected(detail)`：宿主拒绝边界，`detail` 保留产生拒绝的原始受控错误。
- 超时：在有界等待窗口内未取得确认。
- 注册表错误：进程级屏障注册表无法一致访问。

执行必需屏障的拒绝、超时、错误回执形状或注册表失败都属于 `ToolDurabilityFailure`。根会话流与分离子任务流必须使用同一带原因拒绝回执，不能让任一路径遗留到通用超时。这类错误不得转换为模型可见的普通工具失败，因为普通工具失败会允许 Core 把失败结果加入下一次 Provider 请求。

普通业务错误仍走原有工具结果路径，例如参数错误、权限错误、匹配失败或工具自身返回的合法失败结果；它们不属于持久化失败。

## 五、并发工具收尾与 Provider 停止

批次准备屏障失败时，任何工具主体都不得启动。

并行工具已经启动后，任一工具发生 `ToolDurabilityFailure` 时：

1. 记录首个观察到的致命持久化错误。
2. 继续汇合已经启动的兄弟 Future。
3. 对成功兄弟保留真实完成事件。
4. 不把致命失败工具转换成普通错误结果。
5. 全部已启动兄弟完成后，把致命错误返回到 Provider 运行循环。
6. 运行循环不得构建或派发下一次 Provider 请求。

服务流记录首个 checkpoint 错误后，只允许已经进入 `Running` 的兄弟工具提交各自的
`ToolFinished` 终态；新的批次准备、工具开始及非工具运行事件全部拒绝。兄弟工具的可靠终态不得
覆盖首个错误，后续确认或投影错误也只能作为诊断输出，最终执行报告仍以首个致命错误为准。

服务流一旦已经保留 V1 checkpoint 错误，之后到达的请求准备边界必须在本地使用同一个错误拒绝，不能再次调用仍处于活动态的唯一 preparation owner。因此二次错误 `A V1 Provider request is already active` 不得覆盖最初的 ChangeSet 或持久化错误。

## 六、V1 失败终态

如果工具边界失败时存在活动物理 API，失败收口必须在一个候选文档中完成：

1. 中断尚未完成的 assistant semantic 与工具事件。
2. 生成唯一 `ApiStats`，状态为 `Failed`。
3. 使用 `Unknown` 结束原因与错误类别，保留稳定 `error_code` 和原始受控错误正文。
4. 把 `continuation_eligibility` 固定为 `Forbidden`。
5. 追加唯一 `SystemError` DisplayNote，正文保留相同错误详情。
6. 关闭当前逻辑 Turn。
7. 完成领域校验后才替换权威文档并进入可靠 flush。

API 中断与 Turn 关闭虽然复用两个既有领域事务，但只在隔离候选中运行；候选发布前必须把两份事件回执合并为唯一的下一个 `memory_revision`。同一回执必须覆盖工具中断、API 统计、错误提示、TurnBoundary 与 TurnStats，保证 `SessionHandle` 的一次条件修改精确推进一个修订。

如果不存在活动 API，则复用同一失败方法追加 DisplayNote 并关闭开放 Turn。完全相同的重复结算必须是幂等操作，不增加修订、不重复追加错误。

如果最终工具结果与成功 `ApiStats` 已经可靠落盘，但紧随其后的实时生命周期投影失败，则该成功
API 终态不等于完整的失败执行报告。流 sink 必须把“终态是否已经包含受控失败”作为强类型事实
一次性交给终态处理；成功终态后出现运行报告错误时，以
`stream_failed_after_v1_api_terminal` 追加独立 `SystemError` 并关闭仍开放的逻辑 Turn。既有工具结果、
成功 `ApiStats`、用量与请求身份保持原样，工具主体不重跑。若已有 API 终态本身已经持久化失败，
则该终态足以完成收口，不重复追加第二条同类失败。

物理存储第一次拒绝该终态时，服务会对同一内存脏修订执行一次受控重试。第二次调用领域方法必须返回未变化，并由持久化层刷写原有修订；不得重复工具、API 统计或错误事件。只有终态可靠落盘后才释放精确的 terminal-persistence owner。持续失败时保留 owner，使 Continue、新输入、Stop 与替代 kernel 回合保持失败关闭。

## 七、界面与恢复语义

现有 V1 事件窗口会把 `SystemError` DisplayNote 投影到会话正文，并显示 `redacted_detail`。因此无需另建与可靠账本竞争的错误存储。

终态执行报告还会先把同一错误应用到进程内运行态；即使随后物理存储失败，当前进程中的工作台仍显示失败状态和原始错误。可靠事件写入成功后，刷新、切换会话或重新加载都从 V1 事件窗口恢复该错误。

前端 Continue 选择器只接受 `continuation_eligibility == "allowed"` 的终态 API。工具可靠性失败写入 `Forbidden`，所以不会显示孤立的“继续”入口。Turn 关闭后允许用户提交新的根输入；新输入不是对不确定工具副作用的自动重放。

历史会话若没有保存原始错误，不能根据文件是否存在伪造 ChangeSet 或改写旧 API 统计。

## 八、验证要求

至少执行以下验证：

- 原生文件工具完整单元测试，覆盖新建、修改、删除、移动、补丁、预览、空文件和生命周期正文阈值。
- 真实公开 `file write` 结果直接送入规范 V1 解析器，不能使用手写期望载荷替代。
- 投影屏障确认、拒绝、取消和超时回归。
- Core 批次准备失败、工具完成失败、兄弟 Future 汇合和 Provider 请求次数回归。
- V1 会话活动 API 失败、ApiStats、DisplayNote、Turn 关闭、幂等重放与新根输入回归。
- 已完成 API 后的宿主投影失败回归，验证成功 ApiStats 不被覆盖、独立错误可重载且工具不重跑。
- 服务层首次存储失败后同一脏修订重试回归。
- `cargo check`、定向测试、格式检查和 `git diff --check`。

测试或编译遇到其他会话尚未同步的源码错误时，必须记录具体文件和诊断，不得越界修改；本任务结论只能覆盖已实际执行的检查。
