# Apply Patch 来源与同步说明

本 crate 的 Apply Patch 文本 grammar 与 streaming parser 改编自 OpenAI Codex：

- 上游仓库：`openai/codex`
- 固定提交：`343074d4207d572809bd8cea15f4be1d09d98e0b`
- 参考文件：`codex-rs/apply-patch/src/parser.rs`、`codex-rs/apply-patch/src/streaming_parser.rs`
- 上游许可证：Apache-2.0，完整文本见 `LICENSE-APACHE`
- 本地改动：删除权限、Shell 调用识别、绝对路径类型和 Environment ID；路径安全与事务由 Vulcan 宿主负责；输入行尾扩展为 CRLF、LFCR、CR、LF。

同步上游时必须同时核对 `APPLY_PATCH_LARK_GRAMMAR`、完整解析测试、逐字符流式测试以及 `vulcan-host-tools` 的多文件事务测试，不能只复制 grammar。
