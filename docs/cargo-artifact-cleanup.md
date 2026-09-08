# make 构建后的旧制品清理

`make build`、`make build-release`、`make release` 及对应的 `br` 构建入口，在 Cargo
成功结束后自动执行清理。Debug 和 Release 使用同一实现，按实际可执行文件所在目录分别保存基线。
不需要安装额外工具，也不需要改变日常命令。

## 清理依据

- 使用 Cargo 的 `compiler-artifact` 消息记录制品，包括 `fresh: true` 的复用依赖。
- 清单保存在实际 Cargo target 目录的 `.vulcan-make-artifacts/` 中，支持 `CARGO_TARGET_DIR`。
- 仅删除上一成功基线记录、本次已不再报告、其他 make 模式也未引用的直接 `deps` 文件。
- 删除前检查文件大小、写入时间及路径；已被其他构建修改的文件和经过符号链接、联接目录的路径不删除。
- 输出本次实际删除的文件数量与字节数。文件被占用时发出警告，并保留候选供下次重试。
- Cargo 失败不会触发清理或更新基线。清理失败会显示警告，不把已经成功的编译报告为失败。
- 同一 target 下的跟踪构建通过独占锁互斥；并发启动另一个 make 构建会因锁占用失败。

## 明确边界

第一次成功构建只建立基线，不清理接入前无法确认归属的历史文件。后续构建才开始回收已跟踪的退役文件。
不扫描删除 `incremental`、`build`、`.fingerprint`，也不推测 Cargo 未报告的同名哈希文件、调试符号或链接副本。
因此这是有明确证据的制品回收，不是整个 target 的完整垃圾回收。

当前入口只有固定的 Debug 和 Release 两组构建参数，同一路径保存该模式最近一次成功构建的清单。
直接执行的 `cargo build/test/check` 不参与此跟踪，也不受 make 锁保护；不要与 make 清理并发执行。
这些命令曾复用的旧依赖若被 make 清理，下一次执行时可能重新编译。以后增加其他 features 构建入口时，
需要同步扩展基线配置维度，不能直接共享现有模式基线。

## 验证

在仓库根目录执行：

```powershell
rtk proxy pwsh -NoProfile -File scripts/test_cargo_artifact_cleanup.ps1
```

测试在独占临时目录中创建无外部依赖的 Rust 工程，真实验证 Debug 和 Release 的退役制品删除、
全量缓存命中保留、模式隔离、未跟踪文件保留，以及编译失败后基线与上一成功制品保留。
测试不会构建主应用或清理主工程 target。
