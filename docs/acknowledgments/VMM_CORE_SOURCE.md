# VulcanMemoryMesh Core 来源与内置维护边界

## 1. 来源基线

- 来源仓库：`D:\openvulcan\vulcan-memory-mesh-core`
- 来源提交：`e9c3d45ca16ad5d62ba8b84745448130929e35cc`
- 提交时间：`2026-09-02T12:43:46+08:00`
- 提交说明：`支持按 Retention 计划族独立禁用`
- 纳入方式：一次性复制领域核心到 VulcanCode workspace；后续由 VulcanCode 独立维护，不与来源仓库同步发布，也不维持 FFI/Go SDK 兼容。

来源仓库的许可元数据存在已核实的不一致：顶层 `LICENSE` 自初始提交起为 MIT，workspace
`Cargo.toml` 自 `73333a2f0cadbb0abdf90d58b1108220f3ebdd0f` 起声明 `Apache-2.0`。两项事实均保留在本记录中，
不把其中任一项伪装成已由本次集成修正。VulcanCode 顶层 `LICENSE` 与 workspace package 当前均为 MIT，
本次纳入的 crate 继承 VulcanCode workspace 许可字段。原始代码和目标仓库的版权主体均为 OpenVulcan。

## 2. 纳入映射

| 来源 crate | VulcanCode crate | 保留范围 |
| --- | --- | --- |
| `vmm-core-contracts` | `vulcan-memorymesh-core-contracts` | 命令、领域实体、Effect、错误、指纹、检索、替代、保留和全部领域契约测试 |
| `vmm-core-config` | `vulcan-memorymesh-core-config` | 配置快照、PII、噪声、提示词和资源清单及全部配置测试 |
| `vmm-core-storage` | `vulcan-memorymesh-core-storage` | 内存参考存储、存储 Port、快照及存储契约测试 |
| `vmm-core-engine` | `vulcan-memorymesh-core-engine` | 命令引擎、检索、RRF、MMR、Weibull、PreCheck/PostAction 领域逻辑和源码内测试 |
| `vulcan-memory-mesh-core` | `vulcan-memorymesh-core` | Rust 原生门面、命令生命周期、checkpoint、HostDriver 及除导入导出外的公共调用链测试 |

资源文件随 `vulcan-memorymesh-core-config` 放入 crate 自有 `resources/`；Weibull 数值夹具与
`THIRD_PARTY_NOTICES.md` 随 `vulcan-memorymesh-core-engine` 保留。

## 3. 明确排除

以下来源内容没有复制，也不属于 VulcanCode 内置 Core 的兼容目标：

- `vmm-core-ffi`、C 头文件、ABI Proto、动态库加载和原生导出符号。
- Go SDK、Go module consumer、CGO bridge 和 Go 二进制兼容测试。
- `vmm-core-provider-adapter` 及其 Provider JSON 中间协议；AI 调用改由 VulcanCode `HostAiPort` 执行。
- `vmm-core-import-export`、导入导出容器、回滚元数据及仅验证该模块的测试。
- 独立 release packaging、跨仓库冻结校验、独立仓库 CI/性能脚本和 Rust SDK demo。
- gRPC 服务、`vmm-local`、外部接口和多语言兼容层。

外置 VMM 仍由 `vulcan-vmm` 的 Tonic/管理 HTTP 路径独立支持；它不是上述原生 Core 的导出接口。

## 4. VulcanCode 本地调整

- crate 名称改为 `vulcan-memorymesh-core-*`，依赖统一走 VulcanCode workspace。
- facade 移除 Provider Adapter 与 Import/Export re-export，只保留进程内 Rust 原生领域入口。
- `StorageSnapshot` 由存储 crate 直接导出，供宿主持久化与项目维护使用。
- `vulcan-service-kernel::native_vmm` 实现 HostEffect 到统一 `HostAiPort`、SQLite 与 LanceDB 的映射。
- 原生项目 ID、绑定、迁移、清空和删除由 VulcanCode 的 VMM 数据平面维护，不扩展来源仓库的 FFI 或服务协议。
- 上游集成测试只做 crate 名称和资源路径的机械调整；facade 测试移除了两处只覆盖被排除 Import/Export API 的断言。

## 5. 验证入口

在 VulcanCode 仓库根目录执行：

```powershell
rtk cargo test -p vulcan-memorymesh-core-contracts -p vulcan-memorymesh-core-config -p vulcan-memorymesh-core-storage -p vulcan-memorymesh-core-engine -p vulcan-memorymesh-core
```

当前恢复后的领域测试基线为 28 个测试套件、221 项测试。宿主适配、项目数据平面和 VMM 双后端
回归另由 `vulcan-service-kernel`、`vulcan-workspace-store`、`vulcan-vmm` 与 WebUI 测试覆盖。
