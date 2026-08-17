# Rust 依赖治理规范

## 一、权威边界

1. 所有工作区直接使用的注册表依赖都必须在根 `[workspace.dependencies]` 声明版本。
2. 成员 `Cargo.toml` 只能使用 `workspace = true` 或 `dependency.workspace = true` 继承版本，并按模块追加必要 Feature。
3. 内部路径依赖同样由根工作区统一声明，禁止把内部包改写为注册表依赖。
4. `workspace-hack/Cargo.toml` 是 cargo-hakari 生成文件，不手工修改生成依赖区段。
5. 精确解析结果由已提交的 `Cargo.lock` 持有；不要求把所有依赖改成 `=` 固定。

## 二、工具链与 Resolver

1. 仓库根 `rust-toolchain.toml` 固定 Rust `1.97.1`，使用 `minimal` Profile，并包含 `clippy` 与 `rustfmt`。
2. 根工作区采用 Cargo Resolver 3，并以 `[workspace.package].rust-version = "1.97.1"` 持有唯一支持基线。
3. 41 个工作区成员必须显式声明 `rust-version.workspace = true`；`workspace-hack` 包头同样继承，但 Hakari 管理的依赖区段不手工修改。
4. 当前政策只支持仓库固定工具链，不维护更低 MSRV；禁止使用 `--ignore-rust-version` 把不受支持工具链伪装成通过。
5. `check-deps` 会关闭失败地验证 Resolver、固定频道、最小 Profile、必要组件、成员继承与 Cargo 元数据解析结果。
6. Resolver 3 首次落地时 `cargo update` 选择 0 个新包，`Cargo.lock` SHA-256 保持不变，说明当前锁定依赖均与 Rust 1.97.1 兼容。

## 三、升级流程

先只读检查直接依赖候选：

```bash
rtk cargo upgrade --dry-run --incompatible allow --pinned allow
rtk cargo update --dry-run
```

处理规则：

1. 先确认候选属于业务成员直接依赖，还是 `workspace-hack`、传递依赖或平台依赖。
2. 兼容升级需要执行受影响 crate 的 `cargo check` 与定向测试。
3. 不兼容升级必须读取当前调用点与上游迁移契约，禁止只根据版本号机械替换。
4. 网络、TLS、代理、密钥环、Tauri、MCP、LuaSkills 与数据库依赖不能只以编译通过作为验收。
5. `cargo update --dry-run` 中仅由上游约束造成的传递依赖滞后需要记录，但不强行使用 `[patch]` 或私有分叉收敛。

## 四、统一检查入口

Windows 与类 Unix 入口分别为：

```bash
./make.ps1 check-deps
./make.sh check-deps
```

检查入口是只读的，依次验证：

1. 锁定的 Cargo 元数据可解析，Resolver 3 与固定 Rust 版本政策一致。
2. 全部成员显式继承工作区 `rust-version`，解析结果与固定工具链一致。
3. 非生成成员的所有依赖声明均继承根工作区权威。
4. Hakari 生成内容、成员接入与校验状态一致。
5. cargo-deny 的公告、许可证、来源与重复版本政策通过。
6. `cargo-about` 重新生成的公告及索引与仓库权威源码制品逐字节一致。
7. Rust 格式检查通过。
8. 工作区所有目标在当前宿主下完成静态检查。

该入口不会执行 `cargo upgrade`、`cargo update`、Hakari 写入生成或用户配置修改。

## 五、cargo-deny 政策

1. 已知安全漏洞、未授权许可证、未知注册表与未知 Git 来源阻断检查。
2. RustSec“停止维护”公告只有在上游没有安全升级路径时才可临时忽略，每个条目必须包含依赖来源与复查理由。
3. 传递依赖多版本默认告警，不直接阻断。直接依赖版本分散由工作区继承检查单独阻断。
4. Cargo 会把工作区继承和本地路径依赖表达为通配要求，因此 cargo-deny 的 `wildcards` 不承担直接版本门禁；仓库检查脚本负责验证每个成员依赖声明。
5. 新增许可证或 Git 来源前必须先审查 `deny.toml`，不得为了通过检查扩大到宽泛来源或无理由白名单。

## 六、第三方公告制品

Rust 第三方公告的权威源码制品位于 `crates/vulcan-runtime-assets/resources/licenses/`。依赖或许可证政策变化后执行：

```bash
./make.ps1 licenses
./make.sh licenses
```

生成命令只更新权威源码制品，不直接写入 `output/`。公告依赖图与 Hakari、cargo-deny 使用同一五平台集合，包含 Linux ARM64 专属依赖。审阅生成差异并通过 `check-deps` 后，再显式执行 `make sync` 同步运行产物。`check-deps` 只在 `target/dependency-policy/` 生成临时对照文件，并以 SHA-256 校验 Markdown 与 JSON 索引，发现源码制品滞后时失败。

## 七、Hakari 维护

依赖或 Feature 发生变化后执行：

```bash
rtk cargo hakari generate --diff
rtk cargo hakari manage-deps --dry-run
rtk cargo hakari verify
```

确认需要更新生成内容后，再显式执行：

```bash
rtk cargo hakari generate
rtk cargo hakari manage-deps
```

Hakari 的平台集合覆盖 Windows x86_64、Linux x86_64、Linux ARM64、macOS x86_64 和 macOS ARM64。它只统一同版本依赖的 Feature，不解决不同主版本或上游不兼容约束。

## 八、Feature 变更要求

1. 修改 `default-features` 前必须先追踪源码调用点、依赖 Feature 定义和反向特性图。
2. 使用 `rtk cargo tree -e features -i <crate>@<version>` 确认 Feature 的真实启用来源。
3. 系统代理、证书根、TLS Provider、HTTP/2、流式传输、上传、压缩与平台密钥环属于用户可见行为，不能无证据删除。
4. 根工作区保存全局最小能力，成员只追加模块真实需要的 Feature。
5. 目标平台后端只有在依赖公开契约允许独立选择且对应宿主已验证时，才拆分为目标条件依赖。

## 九、测试与资源限制

1. Cargo 编译和常规测试保持仓库默认 4 线程上限。
2. Nextest 普通测试最多并行 4 个进程。
3. `shared-runtime-resources` 组中的 Provider Manifest、服务注册表与端口测试强制单线程。
4. 修改测试名称或新增共享资源测试后，使用以下命令核对真实匹配：

```bash
rtk cargo nextest show-config test-groups --workspace --groups shared-runtime-resources --no-pager
```

5. Nextest 不覆盖 Doctest，文档测试必须单独执行 `rtk cargo test --workspace --doc`。

## 十、跨平台验收

依赖图与 Hakari 统一覆盖以下目标：

- `x86_64-pc-windows-msvc`
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

本机 `cargo check` 不能替代目标宿主运行验收。涉及 Tauri、Keyring、TLS、系统代理或原生依赖时，必须在对应 Windows、Linux x86_64、Linux ARM64、macOS x86_64 与 macOS ARM64 宿主或真实 CI 中验证。本轮已完成 Windows x86_64 验收；其余四个平台按用户确认边界后续独立测试，不作为本轮仓库治理归档的阻塞条件。

## 十一、性能变更门槛

1. 先使用 `rtk cargo build --workspace --timings --locked` 采集计时，再改缓存、Profile 或链接器。
2. `sccache` 至少需要 3 次等价构建，中位耗时改善达到 15%，且没有明显失败率或维护成本增加，才可进入默认工具链。
3. Thin LTO、代码生成单元、符号裁剪和替换链接器必须同时比较构建时间、制品大小、启动性能与崩溃诊断能力。
4. `panic = "abort"` 是运行时行为变化，需要独立确认。
