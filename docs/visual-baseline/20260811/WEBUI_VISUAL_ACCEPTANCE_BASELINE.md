# WebUI 视觉验收基线

## 一、基线状态

- 基线日期：2026-08-11。
- 对应计划：`docs/plan/20260811-01-WEBUI_UNIFIED_VISUAL_SYSTEM_REFACTOR.md`。
- 对应步骤：步骤 0——建立视觉基线与验收清单。
- 自动检查状态：已完成。
- 浏览器截图状态：未采集；转入首个可见视觉调整步骤后按修改范围采集，不作为步骤 0阻塞项。
- 本步骤运行时代码变化：无。
- 当前是否允许进入步骤 1：是；步骤 0属于无视觉变化的工程基线，自动检查通过后直接完成。

## 二、源码基线

### 2.1 Git基线

- 分支：`main`。
- HEAD：`fcc73532f003`。
- 工作区状态：存在未提交修改，本基线生产构建包含这些当前工作区内容。
- 本步骤没有覆盖、回退、暂存或提交任何已有修改。

当前 WebUI相关工作区修改快照：

```text
M apps/vulcan-code-web/src/app/WorkbenchApp.tsx
M apps/vulcan-code-web/src/components/composer/composer-capability-controls.test.ts
M apps/vulcan-code-web/src/components/conversation/conversation-system-error-card.test.tsx
M apps/vulcan-code-web/src/components/conversation/conversation-system-error-card.tsx
M apps/vulcan-code-web/src/components/features/settings/custom-provider-model-defaults-card.tsx
M apps/vulcan-code-web/src/components/features/settings/custom-provider-model-drafts.test.ts
M apps/vulcan-code-web/src/components/features/settings/custom-provider-models-dialog.tsx
M apps/vulcan-code-web/src/components/features/settings/custom-provider-models-state.test.ts
M apps/vulcan-code-web/src/components/features/settings/direct-provider-model-settings.test.ts
M apps/vulcan-code-web/src/components/features/settings/direct-provider-model-settings.ts
M apps/vulcan-code-web/src/components/features/settings/provider-model-access-dialog.tsx
M apps/vulcan-code-web/src/components/features/settings/provider-model-modalities.test.ts
M apps/vulcan-code-web/src/components/features/settings/provider-model-routes.test.ts
M apps/vulcan-code-web/src/components/features/settings/public-ai-model-capability-dialog.test.ts
M apps/vulcan-code-web/src/components/features/settings/public-ai-model-capability-dialog.tsx
M apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx
M apps/vulcan-code-web/src/components/workspace/workbench-layout/conversation-actions.ts
M apps/vulcan-code-web/src/components/workspace/workbench-layout/conversation-hydration.ts
M apps/vulcan-code-web/src/i18n-messages.ts
M apps/vulcan-code-web/src/service/browser-fallback-workbench.ts
M apps/vulcan-code-web/src/service/browserTransport.test.ts
M apps/vulcan-code-web/src/service/browserTransport.ts
M apps/vulcan-code-web/src/styles/composer-controls.css
M apps/vulcan-code-web/src/styles/composer-layout.css
M apps/vulcan-code-web/src/types/public-ai.ts
```

后续步骤进行包体或视觉对比时，必须先确认这些文件是否仍处于相同状态。若工作区内容已经变化，应记录新的对比基线，不得把其他功能变更造成的差异归因给视觉体系调整。

### 2.2 组件与样式静态基线

| 指标 | 当前值 | 统计口径 |
|---|---:|---|
| TSX文件 | 195 | `src` 下全部 `.tsx` |
| `components/ui`组件文件 | 37 | 目录下 `.tsx` 文件 |
| 标准UI JSX标签使用 | 2005 | 所有导入自 `@/components/ui/*` 的JSX标签 |
| 原生 `<button>` | 215 | 排除 `components/ui` 自身 |
| 原生 `<input>` | 77 | 排除 `components/ui` 自身 |
| 原生 `<select>` | 21 | 排除 `components/ui` 自身 |
| 原生 `<textarea>` | 15 | 排除 `components/ui` 自身 |
| 原生 `<table>` | 1 | 排除 `components/ui` 自身，Markdown特殊表格 |
| `src/styles` CSS文件 | 28 | 目录内 `.css` 文件 |
| `src/styles` CSS源码体积 | 300956字节 | 未压缩源码 |
| 顶层语义类选择器 | 2305 | 以类选择器开头的静态近似计数 |

使用量最高的标准组件标签：

| 组件 | 静态使用次数 |
|---|---:|
| Button | 274 |
| Badge | 124 |
| Input | 82 |
| SelectItem | 79 |
| Dialog | 56 |
| DialogContent | 56 |
| DialogTitle | 56 |
| DropdownMenuItem | 52 |
| DialogHeader | 50 |
| Label | 49 |
| DialogDescription | 46 |
| Select | 41 |

以上统计只用于后续趋势对比，不代表运行时同时渲染的组件数量。

## 三、生产构建基线

### 3.1 自动检查结果

| 检查 | 命令 | 结果 |
|---|---|---|
| TypeScript类型检查 | `npm run lint` | 通过，约6.6秒 |
| WebUI生产构建 | `npm run build` | 通过，Vite约9.35秒，总命令约16.4秒 |
| 转换模块数量 | Vite构建输出 | 4567 |
| 构建警告 | Vite构建输出 | 存在大于500 KB的chunk警告 |

构建还出现 Node `module.register()`弃用警告。该警告不是步骤 0 引入，本步骤不处理。

### 3.2 主要构建产物

| 产物 | Raw字节 | Gzip字节 | 说明 |
|---|---:|---:|---|
| `WorkbenchApp-C3isqiAI.js` | 5330401 | 1480941 | 主工作台功能块 |
| `cynefin-VYW2F7L2-DpL8wBg8.js` | 687939 | 153518 | Mermaid延迟图表块 |
| `cpp-CaeG3uDQ.js` | 637587 | 46258 | Shiki延迟语言块 |
| `mermaid.core-KEblgbM_.js` | 622190 | 148347 | Mermaid核心延迟块 |
| `cytoscape.esm-CNmotkZ9.js` | 443024 | 141610 | 图形布局延迟块 |
| `WorkbenchApp-QeUFN8AN.css` | 428373 | 62832 | 工作台主样式 |
| `katex-HP8lGamR.js` | 261313 | 77166 | KaTeX块 |
| `index-D36hqiSf.js` | 246608 | 78244 | 启动入口块 |
| `diff-highlight-worker-DJPaSS0n.js` | 192089 | 59957 | Diff高亮Worker |

说明：文件哈希会随源码和构建器变化，后续主要比较功能角色、raw大小和gzip大小，不要求文件名哈希相同。

### 3.3 已知重型静态数据

| 数据源 | 当前源码大小 |
|---|---:|
| `@iconify-json/material-icon-theme/icons.json` | 891701字节 |
| `@iconify-json/skill-icons/icons.json` | 1673784字节 |
| 合计 | 2565485字节 |

本步骤只记录，不进行图标提取或代码拆包。

## 四、人工截图统一规则

### 4.1 截图保存位置

人工截图统一保存到：

```text
docs/visual-baseline/20260811/screenshots/
```

当前没有已接管的 WebUI浏览器页面，因此截图尚未生成。禁止用旧截图或其他分支截图冒充当前工作区基线。

### 4.2 截图环境记录

第一次截图时必须在本节填写：

- 桌面应用构建或启动方式：待填写。
- Windows显示缩放：待填写。
- 应用窗口像素尺寸：待填写。
- 应用语言：待填写。
- 当前项目：待填写。
- 当前会话：待填写。
- 截图采集人：待填写。
- 截图采集时间：待填写。

后续对比必须尽量复用相同窗口尺寸、显示缩放、语言、项目和会话。动态数据内容允许变化，但组件结构、间距、颜色层级和交互状态必须可比较。

### 4.3 截图命名

| 页面 | 浅色截图 | 深色截图 |
|---|---|---|
| 空会话主页与Composer | `01-conversation-home-light.png` | `01-conversation-home-dark.png` |
| 已有会话与底部Composer | `02-conversation-active-light.png` | `02-conversation-active-dark.png` |
| 设置主区域 | `03-settings-light.png` | `03-settings-dark.png` |
| Skills区域 | `04-skills-light.png` | `04-skills-dark.png` |
| Agents区域 | `05-agents-light.png` | `05-agents-dark.png` |
| Knowledge区域 | `06-knowledge-light.png` | `06-knowledge-dark.png` |
| VMM/Memory区域 | `07-vmm-light.png` | `07-vmm-dark.png` |
| 项目设置弹窗 | `08-project-settings-light.png` | `08-project-settings-dark.png` |
| Provider用量区域 | `09-provider-usage-light.png` | `09-provider-usage-dark.png` |

## 五、后续可见调整复用的人工验收清单

本章不是步骤 0验收闸门。后续某一步产生实际可见变化时，只执行与该步骤修改范围对应的条目，不要求用户为无视觉变化的准备工作验收。

### 5.1 公共外壳

- [ ] 应用标题栏高度、按钮位置和窗口拖动区域正常。
- [ ] 左侧侧栏宽度、折叠状态、项目行和会话行正常。
- [ ] 主内容与侧栏边界没有新增色差、间距或阴影。
- [ ] 浅色和深色主题切换正常。

### 5.2 空会话主页与 Composer

操作路径：选择项目后新建或打开空会话。

- [ ] 输入区尺寸与圆角保持当前状态。
- [ ] 输入内容区与底部工具区的现有分隔线和背景保持当前状态。
- [ ] 模型、能力、附件、知识库、目标和发送按钮位置正常。
- [ ] 调整窗口宽度后控件没有异常换行、遮挡或溢出。
- [ ] 分别保存浅色和深色截图。

### 5.3 已有会话

操作路径：打开一条同时包含普通文本、Markdown、代码块和工具调用的已有会话。

- [ ] 用户消息与助手消息宽度和排版正常。
- [ ] Markdown标题、列表、表格和代码块正常。
- [ ] 工具调用、过程卡和折叠状态正常。
- [ ] 底部Composer位置正常。
- [ ] 滚动条位置没有因为本步骤发生变化。
- [ ] 分别保存浅色和深色截图。

### 5.4 设置

操作路径：进入设置主区域，并打开至少一个Provider相关设置页面。

- [ ] 设置侧栏、分区标题和表单密度正常。
- [ ] Input、Select、Switch、Button保持当前视觉。
- [ ] 打开一个普通Dialog，检查宽度、Header、Footer和关闭按钮。
- [ ] 分别保存浅色和深色截图。

### 5.5 Skills

操作路径：进入Skills区域。

- [ ] Skill卡片、状态标签和操作按钮正常。
- [ ] 安装、更新、启停按钮当前视觉没有变化。
- [ ] 打开Skill详情或配置弹窗，检查弹窗与表单。
- [ ] 分别保存浅色和深色截图。

### 5.6 Agents

操作路径：进入Agents区域。

- [ ] Agent列表、图标按钮、启停状态和筛选器正常。
- [ ] 打开Agent编辑弹窗，检查表单和操作区。
- [ ] 分别保存浅色和深色截图。

### 5.7 Knowledge

操作路径：进入Knowledge区域。

- [ ] Library卡片、空状态、导入进度和操作按钮正常。
- [ ] 打开Library Viewer或导入弹窗，检查大型弹窗布局。
- [ ] 分别保存浅色和深色截图。

### 5.8 VMM/Memory

操作路径：进入VMM或Memory区域，并切换至少两个子页面。

- [ ] Overview、Content、Data、Profile等普通卡片和表单正常。
- [ ] Tabs、RadioGroup、Combobox和维护弹窗正常。
- [ ] 分别保存浅色和深色截图。

### 5.9 项目设置

操作路径：从项目菜单打开项目设置弹窗。

- [ ] 弹窗尺寸、左侧导航、内容滚动和Footer正常。
- [ ] 输入框、选择器和按钮保持当前视觉。
- [ ] 分别保存浅色和深色截图。

### 5.10 Provider用量

操作路径：进入Provider用量或凭据用量页面。

- [ ] 用量摘要、Token数据、刷新状态和错误状态正常。
- [ ] 长数字、无数据和部分数据不会破坏布局。
- [ ] 分别保存浅色和深色截图。

## 六、后续可见步骤验收记录模板

### 6.1 用户结论

- [ ] 通过：允许进入下一个产生可见变化的步骤。
- [ ] 不通过：冻结后续步骤，只修正当前可见调整。

### 6.2 发现的问题

等待用户填写或反馈。

### 6.3 截图完整性

- [ ] 18张基线截图已经保存到约定目录。
- [ ] 截图环境记录已经填写完整。
- [ ] 浅色和深色主题均已覆盖。

## 七、步骤 0 当前结论

- 自动基线已经完成。
- 类型检查和生产构建通过。
- 组件、样式和构建体积数据已经记录。
- 没有修改运行时代码或样式。
- 当前没有可接管的WebUI页面，因此没有采集截图；截图将在首个实际可见调整步骤中按对应页面补充。
- 步骤 0已经完成，不需要用户验收无变化的界面。
