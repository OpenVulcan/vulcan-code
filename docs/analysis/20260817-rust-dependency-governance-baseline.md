# Rust 依赖治理与构建基线报告

## 一、报告范围

- 采集日期：2026 年 8 月 17 日
- 工作区：`D:\openvulcan\vulcan-code`
- 宿主：`x86_64-pc-windows-msvc`
- Rust/Cargo：`1.97.1`
- 工作区成员：41 个，其中包含 1 个 Hakari 生成包 `workspace-hack`
- 注册表直接依赖：65 个

本报告记录本轮依赖治理的实时事实、治理后的依赖边界与本机性能样本。传递依赖的多版本只作为上游兼容性证据，不作为直接依赖治理失败条件。

## 二、执行前事实

1. 根工作区已经使用 Rust 2024 Edition，但仍为 Cargo Resolver 2，且没有 `rust-toolchain.toml` 与 `rust-version`。
2. 31 个注册表直接依赖尚未进入根 `[workspace.dependencies]`，成员清单同时承担版本权威。
3. `cargo update --dry-run` 没有可应用的锁文件更新；12 个包因上游约束保留在旧版本，命令未改写 `Cargo.lock`。
4. `cargo upgrade --dry-run --incompatible allow --pinned allow` 没有发现业务成员清单中的直接依赖升级项；输出中的候选均来自 Hakari 生成依赖表达。
5. `cargo tree --workspace -d` 仍包含 `base64`、`reqwest`、`rand`、`windows`、`toml` 等不同版本，来源为 Tauri、LuaSkills、平台库等传递依赖的不同约束。
6. 初始工作区 `cargo check --workspace --all-targets` 通过，并存在 4 条测试代码中的既有未使用项警告；本轮没有把这些无关警告并入修改范围。

## 三、治理结果

### 3.1 版本权威

1. 65 个注册表直接依赖均由根 `[workspace.dependencies]` 持有版本、默认特性与全局基础特性。
2. 全部非生成工作区成员使用 `workspace = true` 或点号继承形式，不再重复书写第三方版本。
3. 内部路径依赖同样由根工作区统一声明；`workspace-hack` 保持由 cargo-hakari 生成。
4. `jsonschema = 0.49.9` 保留精确固定，因为当前根版本本身已经显式固定且调用链只处理本地 Draft 2020-12 Schema。
5. `luaskills` 与 Tauri 相关包继续保留仓库已有的精确固定策略；当前 dry-run 未发现可用更新。

### 3.2 Feature 事实

1. `jsonschema` 已关闭默认的 HTTP、文件引用解析与额外 TLS 能力。源码调用链明确拒绝外部 Schema 引用，验证后特性图不再显示默认解析特性。
2. 根 `reqwest` 已关闭默认特性，并保留 Provider 所需的 `form`、`json` 与 Rustls。MCP 成员显式追加 `charset`、`default-tls`、`http2`、`stream`、`system-proxy`，从而保持升级前的系统代理与 HTTP 行为。
3. `keyring 4.1.6` 已关闭隐式默认特性并显式启用 `v1` facade。当前 Keyring 4 的 facade 契约要求 `v1` 或 `cli`，因此不能在不迁移到 `keyring-core` 和平台 Store crate 的情况下按目标拆成完全独立后端。
4. 当前修改保持 Windows Credential Manager、macOS Keychain 与 Linux Zbus Secret Service 的 Keyring 4 既有 facade 行为，同时移除了未被 facade 使用的额外 DBus Store 特性。

### 3.3 Hakari 与目标平台

Hakari 现在以以下 5 个目标生成统一特性图：

- `x86_64-pc-windows-msvc`
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

`cargo hakari generate --diff`、`cargo hakari manage-deps --dry-run` 与 `cargo hakari verify` 均通过。Hakari 只统一同版本依赖的特性集合，不负责强行合并不同主版本。

### 3.4 安全、许可证与来源

1. `cargo-deny 0.20.2` 已纳入仓库开发工具入口。
2. `deny.toml` 对已知安全公告、许可证与未知来源采用失败策略；仅 crates.io 注册表被允许，Git 来源必须显式审查。
3. 传递依赖多版本保留为警告。当前 cargo-deny 报告 51 组重复版本告警，不阻断本轮直接依赖治理。
4. 16 个 RustSec 条目均为无安全升级路径的“停止维护”公告，来自 Tauri 2 的 GTK3/urlpattern 依赖与现有过程宏依赖；每个例外均在 `deny.toml` 中记录明确原因。
5. 当前 `cargo deny check` 结果为 `advisories ok, bans ok, licenses ok, sources ok`。
6. `cargo-about 0.9.1` 已以 `cli` Feature 固定纳入开发工具入口；公告依赖图覆盖 Windows x86_64、Linux x86_64、Linux ARM64 与两种 macOS 架构。
7. 权威公告与索引位于 `crates/vulcan-runtime-assets/resources/licenses/`，共形成 11 个许可证分段和 654 个包引用；检查入口使用锁定依赖图重新生成并执行 SHA-256 差异校验。

### 3.5 测试资源

Nextest 全局测试进程仍限制为 4。共享 Provider Manifest、服务注册表、回环端口、WebSocket 与相关进程级资源测试进入 `shared-runtime-resources` 测试组，并以 `max-threads = 1` 执行；`cargo nextest show-config test-groups` 已验证实际匹配结果。

## 四、构建计时样本

样本均使用仓库默认 `build.jobs = 4`，未清理用户现有 `target`，因此代表本轮变更后的首次重编译与等价增量构建，不代表全新机器的绝对冷构建时间。

| 构建 | 样本 1 | 样本 2 | 样本 3 | Cargo 单元 |
| --- | ---: | ---: | ---: | ---: |
| Debug 工作区 | 108.7 秒 | 0.0 秒 | 0.0 秒 | 653 |
| Release 工作区 | 560.8 秒 | 28.6 秒 | 0.0 秒 | 838 |

Windows Release 主程序 `vulcan-code-tauri.exe` 大小为 134,369,280 字节。测试机没有安装 `sccache`，仓库也没有已确认的 CI 缓存环境，无法形成与现有 Rust 增量缓存的等价命中率对比。

## 五、性能决策

1. 当前等价增量构建已能收敛到 0 秒级 Cargo 编译时间，未取得 `sccache` 达到至少 15% 中位耗时改善的证据，因此本轮不把 `sccache` 写入强制默认配置。
2. Release 首次重编译耗时较长，但没有 Thin LTO、代码生成单元、链接器或符号策略的对照样本；本轮不修改默认 Release Profile。
3. 不启用 Fat LTO、`codegen-units = 1`、`panic = "abort"` 或替换链接器，避免在没有运行时与诊断收益证据时扩大风险。
4. 后续若接入真实 CI，可在固定工具链和同一缓存后端下重新采集 3 次冷/热样本，再决定是否只对 CI 启用 `sccache`。

## 六、尚未闭环的外部条件

1. 用户已确认选择 A：仓库固定 Rust `1.97.1`、采用 Resolver 3，并以当前工具链作为唯一支持基线，不维护更低 MSRV。
2. 本机 Rustup 仅安装 `x86_64-pc-windows-msvc`；用户已确认 Linux x86_64、Linux ARM64 与两种 macOS 目标后续在对应宿主独立测试，不阻塞本轮仓库治理归档。
3. 在后续宿主验收完成前，不能宣称 Keyring、Tauri、TLS 与系统代理已经完成五平台运行验证；本轮仅声明五平台依赖图覆盖和 Windows x86_64 运行/构建验证。

## 七、最终只读复验

1. `make.ps1 check-deps` 在最终工作树通过，覆盖工作区版本权威、Hakari、cargo-deny、第三方公告差异、格式和当前宿主全目标静态编译。
2. 最终 `cargo upgrade --dry-run --incompatible allow --pinned allow` 显示的 `bit-set`、旧 `getrandom`、`miniz_oxide`、`phf_shared`、旧 `schemars`、旧 `syn`、旧 `tower-http`、旧 `windows`、旧 `windows-sys` 与旧 `digest` 候选均只存在于 Hakari 生成的 `workspace-hack/Cargo.toml`；业务成员和根工作区没有这些旧版本直接声明。
3. 根工作区持有的 `tower-http 0.7.0` 与 `windows-sys 0.61.2` 已是 dry-run 报告中的最新版本；其他业务直接依赖没有剩余升级候选。
4. 最终 `cargo update --dry-run` 可应用更新为 0 个；16 个未到最新版本的包均受上游兼容约束，不属于业务直接依赖升级遗漏。
5. Dry-run 前后 `Cargo.lock` SHA-256 均为 `110085DEA6919FB36E714331B861BDBB43214964489885185B5804A9AEC420D4`，证明只读检查没有改写锁文件。
6. Resolver 3 落地后执行一次实际 `cargo update`，结果为 0 个包发生选择变化，锁文件 SHA-256 仍保持上述值；41 个成员的 Cargo 元数据均解析为 `rust-version = 1.97.1`。
7. 固定工具链下 Debug 与 Release 工作区构建通过；全量 Nextest 结果为 3342 个通过、4 个跳过，Doctest 结果为 38 个套件通过且当前无文档测试用例。
