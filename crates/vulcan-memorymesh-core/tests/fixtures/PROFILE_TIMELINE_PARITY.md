# 画像时间线同数据对照

来源基线：外置 `D:/openvulcan/VulcanMemoryMesh`，提交 `82f05fa5e3c7657e770fefd84e220fba5654ce07`。

生成器位于 `crates/vulcan-service-kernel/src/native_vmm/fixtures/profile_timeline_oracle_test.go`。
通过 Go 只读覆盖层编译进外置 `internal/app/usecase`，直接调用生产 `renderProfileTimeline`；没有复制排序和渲染算法。
生成器仅在独立串行测试进程中切换 `time.Local`，结束后恢复；不修改外置源码或用户数据。

`profile_timeline.json` 记录三个时区的空和非空输入及实际输出：UTC、Asia/Shanghai、America/New_York。
八条合成事实覆盖 P0/P1/P2、L0/L1/L2/L3、刷新权重、创建时间排序、历史冬夏时差、上海午夜前后一纳秒和缺省生效锚点。
原生对照通过公共 Core 命令和真实渲染链执行，另将输入反转验证存储顺序不影响输出。

映射严格限定为：外置优先级 0/1/2 对应原生 100/50/20；生命周期 0/1/2/3 对应同名强类型枚举；RFC3339 时刻精确转为纳秒。
正文、日期、排序和 P/L/W 标记均逐字节比较，不规范化这些差异。样本显式时间戳为权威，外置过期显示日期仅用于验证锚点优先规则。

外置运行命令：设置 `VULCAN_PROFILE_TIMELINE_ORACLE_OUTPUT` 为明确输出路径，然后执行：

```powershell
rtk proxy go test -overlay C:/Users/lingeasy/AppData/Local/Temp/vmm-profile-timeline-parity-20260907-overlay.json -p 4 -parallel 1 -count 1 ./internal/app/usecase -run '^TestNativeParityProfileTimelineOracle$' -v
```

本证据覆盖画像正文渲染，不代表真实宿主的时区选择、完整画像包文案、Full/Split 全链或模型质量验收完成。
当前发现原生宿主 `NativeProfileBundleFormat::view` 固定传入 UTC，而外置根据运行时本地日历渲染；该宿主差异须独立修复和验收。
