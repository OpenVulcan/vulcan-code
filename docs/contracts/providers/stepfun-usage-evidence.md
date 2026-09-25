# Step Plan 的流式累计用量证据

## 取证范围

2026-09-25 用户提供一次失败请求的线路追踪。请求入口为中国站 `https://api.stepfun.com/step_plan/v1/chat/completions`，模型为 `step-5-preview`，当时使用 StepFun 清单修订 3。HTTP 返回 200 和 `text/event-stream; charset=utf-8`。

本地在收到第一段共 860 字节的正文后停止。捕获没有截断，但响应流没有完成；正文包含两条 SSE 数据帧，均没有 `finish_reason`，没有 `[DONE]`。响应 ID 与模型在两帧保持一致。

| 帧 | 输入 | 输出 | 总量 | 缓存命中 | 推理量 |
| --- | --- | --- | --- | --- | --- |
| 角色初始化 | 22862 | 0 | 22862 | 0 | 0 |
| 首个推理片段 | 22862 | 1 | 22863 | 0 | 0 |

这些字段是该时刻的累计快照，不能逐帧相加，也不能把第一帧当作权威最终用量。推理片段中的计数仍按上游所报的零保留，不能自行将输出量复制到推理量。

## 修复契约

- StepFun 清单修订 4 为 `step-plan` / `step-5-preview` 增加精确模型与入口作用域的用量引用。
- SSE Profile 使用已有 `snapshot` 合并语义，固定提取 `usage.prompt_tokens`、`completion_tokens`、`total_tokens` 及已观察的缓存、推理明细；响应 ID 与模型仍作为来源信息保留。
- 非流式 JSON Profile 沿用标准 Chat 的 `final` 语义和字段映射。
- 其他入口、地区与模型保留原声明，不能将本次证据扩展为所有 StepFun 路由都已验证。
- 不改全局用量归并器；真正的不同最终观测仍应报错。响应何时结束仍由协议流终态判断，而非出现 `usage` 就判定结束。

## 回归与证据边界

`crates/vulcan-provider-adapters/src/runtime/selection/profiles/fixtures/stepfun-step5-cumulative.sse` 保留两帧的协议结构和原始计数，只将实际响应 ID 换成固定测试标识。未保存请求正文、凭据或用户会话身份。测试通过真实清单编译、模型路由选择、用量提取与归并链重放：旧声明复现原错误，修复后输入保持 22862、输出为 1、总量为 22863。

另一个路由测试逐一覆盖四入口与四模型，只有已取证组合得到累计 SSE Profile，并检查非流式字段与标准基线一致。重放测试同时断言计数、快照状态与精确来源路径。

该记录只证明已捕获前缀的协议兼容修复。原请求因本地终止而没有完整上游结束事件，不能声称已重放真实完整回答；应用加载新清单后仍需重新发起在线请求验证最终完成。

## 本次验证与同步

- 修复前：真实前缀重放在第二条统计复现 `Usage update arrived after a different final observation.`。
- 修复后：路由解析五项、用量抽取六项、用量核心十七项、HTTP 执行六项测试通过；测试使用单线程运行。
- `validate-all` 校验十九份清单及随包完整性索引通过；StepFun 修订 4 的 `dry-run-activate` 通过。
- `fixture-check` 的四十二个已有可执行夹具全部通过。
- 通过仓库 CLI 将索引递增至 77，再执行 `make.ps1 sync`。StepFun 源文件及 `output` 副本 SHA-256 均为 `734239c79869bf65241b17fc33d64fe0706137ac0a06ba951b5950f0dcb26517`；索引两份副本均为 `dcd305ea16e64f9057ceaeefccd8bfbd393945af16a1b9cfe3d760ce452cc763`。
- 本次未调用真实模型 API，未重启正在使用的核心；新请求的在线完整回答需在核心重新加载资源后验证。

## 提交前复核

- 2026-09-25 再次运行两项 StepFun 定向回归测试，均通过。
- 发现工作区清单使用 CRLF，而目录内 `.gitattributes` 要求 LF；若直接提交，Git 转换后的字节将与原索引不符。因此按现有仓库规则统一为 LF，再用仓库 CLI 重建索引，保留本次尚未提交的索引修订 77；其他供应商条目没有变化。
- 重建后 `validate-all` 对十九份清单及完整性索引校验通过，并再次执行 `make.ps1 sync`。
- 最终 StepFun 清单为 28817 字节，源文件及同步副本 SHA-256 均为 `7d301cf2264e1064f38f4760cb409b98fdff7ec81ec5f5736f65b62db72d7792`；最终索引两份副本 SHA-256 均为 `d717a97f556e03f0f7e1eeaf032fa9ceffff40e552fd5e30d973ba19a80c9c6e`。上节摘要记录的是换行规范化之前的验证结果。
- 定向测试仍报告两处现有未使用导入警告，位于本次未修改的 `context_engine_embedding_policy.rs` 和 `runtime/selection/tests/accounts.rs`；未将这些无关修改混入此次提交。
