import { PageHint } from "@/components/page"
import { UsagePanel, type UsagePanelViewModel } from "@/components/product/usage"

import type { VisualLabDemoProps } from "./visual-lab-shared"

// VisualLabUsage renders the usage product component in every state it must support.
// VisualLabUsage 以用量产品组件必须支持的全部状态进行渲染。
// Parameters: props supplies copy. Returns fixed view-model fixtures; nothing connects to product services.
// 参数：props 提供文案。返回固定视图模型样本，不连接任何产品服务。
export function VisualLabUsage({ text }: VisualLabDemoProps) {
  // samples defines fixed state fixtures; the last one is rendered in a narrow column.
  // samples 定义固定状态样本；最后一个渲染在窄栏中。
  const samples: UsagePanelViewModel[] = [
    {
      title: text("本月用量", "Monthly usage"),
      description: text("供应商上报事实与已知费用。", "Provider-reported facts and known costs."),
      state: "ready",
      metrics: [
        { id: "calls", label: text("真实调用", "Physical attempts"), value: "12,480", description: text("包含重试", "Includes retries") },
        { id: "coverage", label: text("覆盖率", "Coverage"), value: "98.6%" },
        { id: "tokens", label: "Token", value: "8,942,110" },
      ],
      tokenBreakdown: {
        title: text("Token 分解", "Token breakdown"),
        items: [
          { id: "input", label: text("输入", "Input"), value: "5,106,302" },
          { id: "output", label: text("输出", "Output"), value: "3,835,808" },
        ],
      },
      cost: { label: text("已知费用", "Known cost"), amount: "USD 42.7819", source: text("供应商上报", "Provider reported") },
      modelsTitle: text("模型用量", "Model usage"),
      models: [
        { id: "model-a", label: "GPT-5.6 Terra", provider: "OpenAI", tokens: "6,340,110 Token", requests: text("8,920 次调用", "8,920 requests") },
        { id: "model-b", label: "Claude Sonnet 5.5", provider: "Anthropic", tokens: "2,602,000 Token", requests: text("3,560 次调用", "3,560 requests") },
      ],
    },
    { title: text("空状态", "Empty state"), description: text("无事实时不显示虚构的零。", "No invented zero when facts are absent."), state: "empty", stateMessage: text("暂无用量记录", "No usage records") },
    { title: text("加载状态", "Loading state"), state: "loading", stateMessage: text("正在读取原生用量…", "Loading native usage…") },
    { title: text("部分可用", "Partial state"), state: "partial", stateLabel: text("部分", "Partial"), stateMessage: text("费用暂缺，Token 事实仍可查看。", "Cost is unavailable; token facts remain visible."), metrics: [{ id: "partial-token", label: "Token", value: "438,210" }] },
    { title: text("错误状态", "Error state"), state: "error", stateMessage: text("无法读取用量，请检查服务状态。", "Usage could not be loaded. Check service status.") },
    { title: text("长数字与窄宽度", "Long values at narrow width"), state: "ready", metrics: [{ id: "long-token", label: text("精确 Token 合计", "Exact token total"), value: "9,223,372,036,854,775,807" }], cost: { label: text("精确费用", "Exact cost"), amount: "CNY 12345678901234567890.123456789", source: text("本地估算", "Locally estimated") } },
  ]
  return (
    <>
      <PageHint>
        {text(
          "产品组件只接收确定的视图模型；统一验证正常、空、加载、部分、错误、长数字与窄宽度。",
          "Product components consume determined view models only; ready, empty, loading, partial, error, long-value and narrow states are reviewed together.",
        )}
      </PageHint>
      <div className="visual-lab-usage-grid">
        {samples.map((sample, index) => (
          <div className={index === samples.length - 1 ? "visual-lab-usage-narrow" : undefined} key={`${sample.state}-${sample.title}`}>
            <UsagePanel model={sample} />
          </div>
        ))}
      </div>
    </>
  )
}
