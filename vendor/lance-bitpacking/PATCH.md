# Lance 位压缩依赖补丁

- 来源：crates.io `lance-bitpacking 11.0.0`。
- 上游提交：`ab6b5bbe46009ed78746b444df8db59a8bc5d842`。
- 上游路径：`rust/compression/bitpacking`。
- 修改范围：仅将清单中的 `paste` 依赖重定向为维护中的 `pastey 0.2.3`；保留原导入名，所有 Rust 源文件与发布包完全一致。
- 原因：`paste` 停止维护，且检查时 `lance-bitpacking 12.0.0` 仍依赖它。局部补丁避免升级整个 Lance 数据格式与数据库接口。
- 此目录是第三方源码，不继承第一方工作区包政策；保留上游版权与许可证。
- 升级 Lance 时必须重新检查此补丁；上游移除 `paste` 后删除补丁及根清单中的覆盖项。
- 验证：运行此 crate 的上游位压缩测试，并检查 Vulcan LanceDB 引擎的全部目标。
