# 逐文件覆盖台账

## 当前限定范围状态

2026-08-31按后续授权完成06插件后，已实施9项全部逐文件闭环（原始15项中的9项）；未实施6项不扩展。限定审核前两轮完成，第三轮发现未修改旧视频夹具失败，当前计数归零并等待修复范围确认，详情见[限定循环记录](GLOBAL_REVIEWS.md)。[此前收尾总结](CLOSEOUT.md)保留历史快照，不覆盖当前状态。

## 统计口径

按仓库忽略规则枚举 Rust 源码，排除构建产物；物理行数包含空行、注释与测试；阈值为严格超过 5000 行。

## 文件清单

初始实时扫描冻结共15个目标。编号保留用于稳定追踪，已执行记录与未实施基线分开保存。

| 编号 | 原始路径 | 基线行数 | 独立记录 | 当前状态 |
| --- | --- | ---: | --- | --- |
| 01 | `crates/vulcan-provider-adapters/src/runtime.rs` | 20951 | [记录](01-vulcan-provider-adapters-src-runtime.md) | 已拆为631行根及完整职责模块，51个Rust产物纳入[终审](01-FINAL-REVIEW.md)；430节点等价、125测试保全、644包级及56消费者回归通过，逐文件闭环 |
| 02 | `crates/vulcan-service-kernel/src/public_ai_settings.rs` | 16244 | [记录](02-vulcan-service-kernel-src-public_ai_settings.md) | 已拆为341行根及17个生产模块，32个Rust产物纳入[终审](02-FINAL-REVIEW.md)；314节点/71类型/47测试/2常量与127原可见入口保全，788包级及16消费者定向复验通过，逐文件闭环；严格Clippy独立旧基线已留档 |
| 03 | `crates/vulcan-provider-adapters/src/execution.rs` | 14353 | [记录](03-vulcan-provider-adapters-src-execution.md) | 已拆为398行根及19个生产子模块，41个Rust产物纳入[终审](03-FINAL-REVIEW.md)；364节点/18常量/103测试保全，644包级及68消费者回归通过，逐文件闭环 |
| 04 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs` | 10521 | [记录](04-vulcan-workspace-store-src-knowledge_base-runtime.md) | 已拆为295行共享根及12生产模块，19个Rust产物纳入[终审](04-FINAL-REVIEW.md)；279节点/21常量/13测试/3夹具与71原可见项保全，183包级通过/5原有忽略、14消费者及显式控制器回归通过；共享根和完整维护生命周期保留理由明确，逐文件闭环 |
| 05 | `crates/vulcan-session-core/src/v1/document.rs` | 10192 | [记录](05-vulcan-session-core-src-v1-document.md) | 已拆分为 1024 行根模块及 11 个职责模块；249 符号等价、247 测试及 44 消费者回归通过，拆分后审核闭环 |
| 06 | `crates/vulcan-plugin-runtime/src/runtime.rs` | 9716 | [记录](06-vulcan-plugin-runtime-src-runtime.md) | 已拆为631行共享根及25子产物，186生产/23测试/9辅助节点保全，55公开项及22常量/1静态保持；3旧测试修复留证，59包级与14服务回归通过，见[完整终审](06-FINAL-REVIEW.md) |
| 07 | `crates/vulcan-provider-adapters/src/provider_manifest/validation.rs` | 9462 | [冻结基线](CLOSEOUT.md#未实施范围与冻结基线) | 未实施；空模板及待办安排已清理 |
| 08 | `crates/vulcan-service-kernel/src/provider_manifest_sync.rs` | 9042 | [冻结基线](CLOSEOUT.md#未实施范围与冻结基线) | 未实施；空模板及待办安排已清理 |
| 09 | `crates/vulcan-session-core/src/v1/document/document_tests.rs` | 7996 | [记录](09-vulcan-session-core-src-v1-document-document_tests.md) | 已拆为 362 行共享夹具入口及 12 个专项模块；122 用例及 17 夹具原样保全，247 测试全通过，拆分后审核闭环 |
| 10 | `crates/vulcan-service-kernel/src/configured_inference.rs` | 7140 | [冻结基线](CLOSEOUT.md#未实施范围与冻结基线) | 未实施；空模板及待办安排已清理 |
| 11 | `crates/vulcan-host-tools/src/file_runtime.rs` | 6537 | [冻结基线](CLOSEOUT.md#未实施范围与冻结基线) | 未实施；空模板及待办安排已清理 |
| 12 | `crates/vulcan-service-kernel/src/session_runtime_registry_v1.rs` | 6521 | [冻结基线](CLOSEOUT.md#未实施范围与冻结基线) | 未实施；空模板及待办安排已清理 |
| 13 | `crates/vulcan-provider-adapters/src/client.rs` | 5695 | [记录](13-vulcan-provider-adapters-src-client.md) | 已拆为 619 行根与 12 生产子模块；41 用例及 6 夹具保全，644 包级回归与消费者编译通过，逐文件审核闭环 |
| 14 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_vulcan.rs` | 5557 | [记录](14-vulcan-agent-runtime-src-agent_runtime-provider_transport_vulcan.md) | 已拆为 394 行根及 7 生产模块，36 有效测试及 27 消费者回归通过；按证据移除 1 项无效旧测试，逐文件审核闭环 |
| 15 | `crates/vulcan-provider-adapters/src/configuration.rs` | 5539 | [冻结基线](CLOSEOUT.md#未实施范围与冻结基线) | 未实施；空模板及待办安排已清理 |

## 完成判定

原逐文件闭环标准：职责已确认、拆分／保留理由已记录、实现已完成或有充分保留理由、测试完整且匹配、拆分后审核完成、发现问题已修复并复验。阶段收尾不改变该标准，未完成项不计入闭环。

## 当前复扫与范围

本次限定的232个Rust产物均已核实可从原根模块到达，无超过5000行的新产物。06已不在超大文件名单。原始名单中剩余以下6个超过5000行文件维持未实施，不将其判定为无需拆分，不建立自动续做队列。

| 原编号 | 当前路径 | 保留行数 |
| --- | --- | ---: |
| 07 | `crates/vulcan-provider-adapters/src/provider_manifest/validation.rs` | 9462 |
| 08 | `crates/vulcan-service-kernel/src/provider_manifest_sync.rs` | 9042 |
| 10 | `crates/vulcan-service-kernel/src/configured_inference.rs` | 7140 |
| 11 | `crates/vulcan-host-tools/src/file_runtime.rs` | 6537 |
| 12 | `crates/vulcan-service-kernel/src/session_runtime_registry_v1.rs` | 6521 |
| 15 | `crates/vulcan-provider-adapters/src/configuration.rs` | 5539 |

逐文件已实施9／9、原始范围9／15；限定循环审核单独记账，不能把阶段测试折算为轮次。完整当前源码清单和摘要见[AUDIT-BASELINE.json](AUDIT-BASELINE.json)。
