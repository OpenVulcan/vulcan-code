# 前端质量门禁

## 入口与归属

`apps/vulcan-code-web/package.json` 定义实际执行顺序：静态检查、独立类型检查、界面架构检查、生产构建、产物预算。任何一步失败都会使 `pnpm run build` 失败。

- `pnpm run lint`：使用固定版本 ESLint 与 TypeScript 解析器检查异步执行器、不可达条件、异常覆盖、可选链误用等语义问题，不批量引入格式规则。
- `pnpm run typecheck`：保留独立 TypeScript 检查。
- `pnpm run test:lint`：通过生产配置与实际命令行验证故意错误会失败、合法类型源码会通过。
- `pnpm run test:architecture`：验证架构与预算检查器的失败场景。

规则权威是 `eslint.config.mjs`，预算权威是 `scripts/webui-build-budget.json`。更新相关工具版本时重跑上述入口；不能只修改本文档而不验证实现。

## 二〇二六年十月二十日界面重构后的基线

界面重构删除了 Tailwind、shadcn 与旧 `src/styles`：工作台样式全部来自 `src/ui-kit/styles`（令牌、元素重置、
组件与 `app/` 页面样式，懒加载页面样式在 `features/`），构建不再扫描工具类。实测工作台 CSS 为 359139 字节、
压缩 55635 字节；工作台入口 JavaScript 为 1354819 字节、压缩 389134 字节；首屏工作台 JavaScript 合计 2012816 字节、
压缩 571801 字节。上限按实测保留约百分之五至八的余量重建：`workbenchCss` 为原始三十九万字节、压缩六万字节，
`workbench` 为一百四十五万字节、压缩四十一万五千字节，`initialWorkbenchJavaScript` 为二百一十二万字节、压缩六十万字节；
其余预算不变。后续增长仍需独立评审，不能自动跟随构建结果抬高限制；新页面样式优先放入懒加载的 `features/` 样式表。

