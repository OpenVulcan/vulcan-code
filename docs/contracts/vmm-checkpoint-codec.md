# VMM 检查点续体编码

## 权威与变更原因

- 外层检查点版本由 `vulcan-memorymesh-core-contracts/src/canonical.rs` 的 `CHECKPOINT_FORMAT_VERSION` 唯一定义。
- 私有续体及执行结果摘要编码由 `vulcan-memorymesh-core-engine/src/checkpoint_codec.rs` 唯一实现。
- 因 `bincode` 停止维护（RUSTSEC-2025-0141），续体改用 Postcard；外层版本同步递增。旧版检查点由现有版本校验明确拒绝，不尝试另一种解码器。
- Postcard 版本以根工作区和锁文件为准；格式契约归属上游 Postcard 1.x 稳定格式。升级时必须复核其 Serde 支持、浮点位模式和长度编码。

## 边界

- 单份私有续体和执行结果摘要共享源码中的字节上限。
- 编码器在追加字节前执行上限检查，不能先构造任意大的完整结果再截断。
- 解码前检查输入大小，解码后必须完全消费输入；拒绝截断、畸形及尾随字节。
- 公共外层检查点、状态机及业务校验继续由原实现负责，编码器不能代替这些校验。
- 执行结果摘要仍使用 SHA-256，但摘要材料是新格式字节，不声称与旧版结果摘要相同。

## 验收

- `vulcan-memorymesh-core` 的公共流程与检查点恢复集成测试。
- `vulcan-memorymesh-core-contracts` 的旧版本拒绝、摘要和规范编码测试。
- `vulcan-memorymesh-core-engine` 的续体恢复测试，以及编码上限、非法输入、Unicode、整数边界和浮点位模式测试。
