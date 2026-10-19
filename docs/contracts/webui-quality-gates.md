# 前端质量门禁

## 入口与归属

`apps/vulcan-code-web/package.json` 定义实际执行顺序：静态检查、独立类型检查、界面架构检查、生产构建、产物预算。任何一步失败都会使 `pnpm run build` 失败。

- `pnpm run lint`：使用固定版本 ESLint 与 TypeScript 解析器检查异步执行器、不可达条件、异常覆盖、可选链误用等语义问题，不批量引入格式规则。
- `pnpm run typecheck`：保留独立 TypeScript 检查。
- `pnpm run test:lint`：通过生产配置与实际命令行验证故意错误会失败、合法类型源码会通过。
- `pnpm run test:architecture`：验证架构与预算检查器的失败场景。

规则权威是 `eslint.config.mjs`，预算权威是 `scripts/webui-build-budget.json`。更新相关工具版本时重跑上述入口；不能只修改本文档而不验证实现。

## 二〇二六年九月二十六日预算重建依据

此前 CSS 预算为原始四十四万字节、压缩六万六千字节。自最近预算提交 `4f79b99c` 后，主分支样式已有会话分支控制、工具卡、输入区和全局主题等变更，五十二个样式文件累计约二千九百行增加、一千行删除。接入正式门禁时，既有代码实际 CSS 为 487227 字节，压缩为 71280 字节，旧预算已经无法通过。

将 Tailwind 的扫描根限定到实际源码树后，测量为 486949 字节、压缩 71229 字节。CSS 上限重建为原始五十万字节、压缩七万四千字节，约保留百分之三的余量。此调整承认已有功能增长，并非本轮实现了 CSS 体积优化。其余 JavaScript 预算保持不变；后续增长仍需独立评审，不能自动跟随构建结果抬高限制。

工具类扫描规则遵循 [Tailwind 源码检测契约](https://tailwindcss.com/docs/detecting-classes-in-source-files)，升级 Tailwind 时必须重新验证扫描范围与实际生产体积。

## 二〇二六年十月十九日界面重构期间的临时上限

用户同意在 `webui-rebuild` 分支重构期间暂时放宽工作台 CSS 上限：新 kit 样式（`src/ui-kit/styles`，约十六万字节）
与旧样式及 Tailwind 工具类并存，逐区替换完成前体积必然高于原上限。`workbenchCss` 暂定为原始九十万字节、压缩十三万字节，
其余预算不变，每阶段仍运行完整 `pnpm run build`。清理阶段删除 Tailwind 与 `src/styles` 后，按实测体积重建更低的基线，
并在合回主分支前替换本节。
