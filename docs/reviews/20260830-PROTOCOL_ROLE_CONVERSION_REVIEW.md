# 协议角色转换审核记录

## 范围

用户要求修复 Chat developer 报错，同时检查其他协议不支持的角色，并确定最稳妥的可恢复方案。
规则记录见 [`协议角色转换与来源恢复规则`](../protocol-role-projection-rules.md)。

## 已发现并处理

1. 直接 Chat 编码和公共 Responses → Chat 转换均原样发送 developer。现在均明确映射为 system。
2. Chat system 合并仅接受字符串，Responses 文本数组会被静默丢弃。现在按顺序保留文本块，
   非文本指令明确报错，不静默删除。
3. 协议注册表包含 Cohere，但公共转换入口没有对应分支，原来会放行 Responses 正文。现在对
   缺少转换器的目标明确返回发送前错误，不探测其他协议。Cohere 公共转换仍未实现，不宣称已支持。

## 可恢复性决策

复用已有持久化原事件角色提示、业务来源及请求投影证明；不增加重复持久化镜像，不把 XML 标签
注入模型正文，不解析助手正文来确定权限角色。出站映射只修改请求副本；切回时重新读取原始事件
并投影。新响应仍按解码事件类型及工具 ID 入库，而不是按请求的原角色反标。

已核对 `MessageRole` 五态、`SystemRoleHint` 三态、`ProjectedItem` 各类来源、
`rebuild_provider_transport_candidate`、`ProviderProjectionProof` 和 `ProviderReplayEnvelope` 的身份边界。
不存在新增逐 Wire 节点反查表；文档已明确区分原事件重建与直接反解析有损请求正文。

## 验证

- 修改实现前，三项新角色测试全部失败，分别复现角色未转换、文本数组丢失、非文本内容静默删除。
- `vulcan-llm-core --lib`：144 项通过。
- `vulcan-provider-adapters protocol_roles_ --lib`：两项通过，分别遍历 9 个协议配置和两类缺失转换器场景。
- `vulcan-provider-adapters --lib`：642 项通过，2 项与上一轮相同的源码资源 Index 校验测试失败。
  两项失败不经过本轮转换逻辑，未修改或放宽资源校验。
- rustfmt 与 `git diff --check` 通过。
- `vulcan-service-kernel provider_transport_rebuild_replaces_pre_gate_conversation --lib`：1 项通过，
  核对 V1 原始请求重建、动态指令保留及来源证明。
- `vulcan-session-core v1::projection::tests --lib`：11 项通过，覆盖事件投影、系统状态恢复和边界。

## 审核

第一轮：核对两处 Chat 入口、文本数组、工具配对、五种角色和所有注册协议到公共转换入口的覆盖；
已修复缺少转换器时错误放行的问题。本轮已实现范围无新增问题。

第二轮：核对原事件身份不被 Wire 映射覆盖、切回 Responses、精确路由重放边界及未知协议发送前
拒绝；规则不依赖模型回传标签，不制造第二份角色真相。本轮已实现范围无新增问题。

未进行真实供应商请求或桌面重启验收。全量媒体、模型参数和未实现的新协议转换不属于已完成验证。
