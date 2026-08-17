# VMM 托管运行说明

## 配置权威

VMM 的部署模式位于主页面“记忆”区域。Vulcan Code 持久化用户意图，运行时清单只是每次启动重新生成的派生产物，不作为下次启动的配置回退。

托管模式下：

- Vulcan Code 持有并监督 `vmm-local` 子进程。
- gRPC 地址和随机 Token 只存在于当前进程内；设置页只展示非敏感实际地址。
- VMM 使用 Vulcan Code 已启动的共享 VLDB Controller，但绑定独立 `vmm-memory` Space。
- VMM 的全部 LLM、Embedding、Rerank 请求使用“内部推理 · VMM 配置档”。
- 设置页不允许编辑运行期 gRPC 地址或 Token。

外部模式下：

- 用户显式配置外部 VMM 的 gRPC 地址和可选 API Key。
- Vulcan Code 不启动、重启或关闭外部进程。

## 启动前置条件

托管 VMM 只会在以下条件全部满足后启动：

1. 共享 VLDB Controller 已发布。
2. 受控推理服务发现文件属于当前 Vulcan Code 进程，且协议版本匹配。
3. `vmm` 配置档包含 Embedding 路由，并且该模型配置了有效向量维度。
4. `precheck_l1`、`precheck_l2`、`postaction_l1`、`postaction_l2` 四条基础 LLM 路由均精确绑定到可用账号，并且模型支持关闭思考。
5. VMM Sidecar、正式维护程序与 Prompt、Noise、PII 资产存在。

首次配置始终保持 VMM 禁用，先完成向量和四条基础 LLM 路由配置与校验，再启动连接、建立用户/项目身份，最后才写入启用状态。`profile_instruction` 与 Rerank 属于可选增强能力；缺失时只禁用对应功能，不阻止托管进程启动。必需条件不满足时记忆能力降级为 `failed`，不会阻止 Vulcan Code 工作台启动。完整错误会显示在记忆运行时卡片和服务日志中。

当当前项目的最终 VMM 配置未启用时，会话输入区不会显示 VMM 选项，Agent Runtime 也不会注入 VMM 上下文或注册 VMM AI 工具。

## 进程与重启

- 首选端口为当前 WebUI Bridge 端口加 `10`；release 默认 WebUI 端口为 `17891`，debug 默认 WebUI 端口为 `17921`，因此对应首选 VMM 端口分别为 `17901` 与 `17931`；冲突时 VMM 自行回退到动态回环端口。
- 实际端口始终来自 VMM 状态握手，不能使用推算值替代。
- 意外退出后按 `5、10、20……` 秒指数退避，单次等待最长 `1800` 秒。
- 用户关闭托管模式或替换运行时代次后，旧监督任务立即失效，不会重新拉起旧实例。
- 正常关闭先请求 VMM 优雅释放，等待 10 秒后才强制终止。

## 向量空间迁移闸门

- 托管数据根使用 `embedding-space-v1.json` 记录供应商、服务入口、账号、二级路由、模型、动作绑定、能力修订、任务、维度、输出类型、编码、归一化和距离度量。
- 首次启动只允许在空数据根中建立该标记。
- 已有数据但缺少标记时拒绝启动，避免把来源不明的历史向量静默解释为当前空间。
- 任一指纹字段变化都会阻止托管 VMM 启动，并在记忆运行时卡片显示需要迁移或重建的完整原因。
- 发现阻塞状态时，“高级设置”提供明确的“稍后处理”或“备份并重建”选择。
- 用户确认后，宿主先把完整数据复制到唯一备份目录，再停服调用 `vmm-migrate -vulcan-managed-config ... -vector-rebuild -confirm-vector-rebuild`。
- 用户可在备份或重建阶段取消；原始数据不会因取消或失败被删除。
- 只有正式重建成功后才原子更新向量空间标记。

## 依赖准备

VMM 是 Vulcan Code 的运行时依赖，不属于常规 `build`、`br` 或 Tauri 构建流程。首次准备环境或 VMM 源码更新后，显式执行：

```powershell
make deps-vmm
```

也可以使用 `make deps vmm`。上述入口调用 `scripts/package_vmm_runtime.ps1`，默认从同级 `VulcanMemoryMesh` 仓库构建并复制运行时；可通过 `VULCAN_VMM_SOURCE_DIR` 指定其他源码检出目录。

依赖产物：

- `output/bin/vmm-local(.exe)`
- `output/bin/vmm-migrate(.exe)`
- `output/configs/base.yaml`
- `output/configs/config.yaml`
- `output/configs/noise_rules`
- `output/configs/pii_rules`
- `output/configs/prompts`

常规 Vulcan Code 构建只使用已经准备好的依赖产物，不会重新编译 VMM，也不会打包或启动 VMM 自带的第二个 Controller。
