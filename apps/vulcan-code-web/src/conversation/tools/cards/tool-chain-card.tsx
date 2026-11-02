// Tool chain card. The head counts how many steps ran; the body lists every step in order with its outcome and wraps
// the step's own tool card, so a chained shell command or file read looks exactly like a direct one. While the chain
// runs, the planned steps are listed from the request.
// 工具链卡。标题统计已运行的步骤数；正文按顺序列出每个步骤及其结果，并包裹该步骤自身的工具卡，使链中的 shell
// 命令或文件读取与直接调用看起来完全一致。链条运行期间按请求列出计划步骤。
import * as React from "react";
import { Ban, Check, CircleSlash, Minus, Workflow, X, type LucideIcon } from "lucide-react";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import type { ToolPayload } from "@/service/conversation-event-window-store";
import { resolveToolCard } from "../tool-call-card";
import { ToolFrame, type ToolCardProps } from "../tool-card";
import { canonicalToolName, listOf, numberOf, objectOf, textOf, type ToolCall, type ToolCardState } from "../tool-call";
import { ErrorNote } from "../views/result-views";

// ChainOutcome is the final state of one step, plus pending while the chain still runs.
// ChainOutcome 是单个步骤的最终状态；链条仍在运行时为 pending。
export type ChainOutcome = "success" | "failure" | "skipped" | "denied" | "not_run" | "pending";

// ChainStep is one step as the card draws it.
// ChainStep 是卡片绘制的单个步骤。
export interface ChainStep {
  // Zero-based position in the chain.
  // 在链条中的从零开始的位置。
  index: number;
  // Step id the model gave, when any.
  // 模型给出的步骤标识（如有）。
  id: string | null;
  // Tool the step called.
  // 步骤调用的工具。
  tool: string;
  // Final state.
  // 最终状态。
  outcome: ChainOutcome;
  // Why the step was skipped, denied or failed before calling its tool.
  // 步骤被跳过、拒绝或在调用工具前失败的原因。
  detail: string | null;
  // Arguments the tool was called with, or the planned arguments while running.
  // 调用工具时使用的参数；运行期间为计划参数。
  input: Record<string, unknown>;
  // The step's own result as a direct call would have it; null when the tool did not run.
  // 与直接调用相同的步骤自身结果；工具未运行时为 null。
  result: Record<string, unknown> | null;
}

// OUTCOMES lists the outcomes the host reports.
// OUTCOMES 列出宿主报告的结果。
const OUTCOMES = new Set<ChainOutcome>(["success", "failure", "skipped", "denied", "not_run"]);

// chainSteps reads the host report, or the planned steps from the request while the chain runs.
// chainSteps 读取宿主报告；链条运行期间则从请求读取计划步骤。
export function chainSteps(call: ToolCall): ChainStep[] {
  const reported = call.host?.kind === "tool_chain" ? listOf(call.host.payload.steps) : null;
  if (reported) {
    return reported.flatMap((entry, position) => {
      const step = objectOf(entry);
      const tool = textOf(step?.tool);
      const outcome = textOf(step?.outcome) as ChainOutcome | null;
      if (!step || !tool || !outcome || !OUTCOMES.has(outcome)) return [];
      return [{
        detail: textOf(step.detail),
        id: textOf(step.id),
        index: numberOf(step.index) ?? position,
        input: objectOf(step.arguments) ?? {},
        outcome,
        result: objectOf(step.result),
        tool,
      }];
    });
  }
  return listOf(call.input.steps).flatMap((entry, index) => {
    const step = objectOf(entry);
    const tool = textOf(step?.tool);
    if (!step || !tool) return [];
    return [{ detail: null, id: textOf(step.id), index, input: objectOf(step.arguments) ?? {}, outcome: "pending" as const, result: null, tool }];
  });
}

// OUTCOME_STATES maps an outcome onto the nested card's lifecycle.
// OUTCOME_STATES 将结果映射为内嵌卡片的生命周期。
const OUTCOME_STATES: Record<ChainOutcome, ToolCardState> = {
  denied: "failed",
  failure: "failed",
  not_run: "stopped",
  pending: "running",
  skipped: "stopped",
  success: "done",
};

// stepCall builds the call a step's own card reads, as if the step had been called directly.
// stepCall 构建步骤自身卡片读取的调用，如同该步骤被直接调用。
export function stepCall(parent: ToolCall, step: ChainStep): ToolCall {
  const result = step.result;
  const host = objectOf(result?.host_result);
  const isError = result?.is_error === true;
  const modelOutput = textOf(result?.model_output);
  const running = step.outcome === "pending" && parent.state === "running";
  return {
    durationMs: numberOf(result?.duration_ms),
    // The chain's change-set covers every step, so no single step claims it.
    // 链条的变更集覆盖所有步骤，因此不归属任何单个步骤。
    event: {
      ...parent.event,
      changeset: undefined,
      external_refs: undefined,
      generated_resources: undefined,
      // Both come from the host's JSON payload, so they are JSON values.
      // 两者均来自宿主的 JSON 载荷，因此都是 JSON 值。
      input: step.input as ToolPayload,
      output: (result ?? undefined) as ToolPayload | undefined,
      started_at_ms: undefined,
      finished_at_ms: undefined,
      status: running ? "running" : "completed",
      tool_name: step.tool,
    },
    host: host && typeof host.kind === "string" ? { kind: host.kind, payload: objectOf(host.payload) ?? {} } : null,
    input: step.input,
    isError,
    key: `${parent.key}:step:${step.index}`,
    modelOutput,
    name: canonicalToolName(step.tool),
    request: null,
    state: running ? "running" : step.outcome === "pending" ? "stopped" : OUTCOME_STATES[step.outcome],
  };
}

// OUTCOME_ICONS marks each settled outcome in the step list.
// OUTCOME_ICONS 在步骤列表中标记每种已定结果。
const OUTCOME_ICONS: Record<Exclude<ChainOutcome, "pending">, LucideIcon> = {
  denied: Ban,
  failure: X,
  not_run: Minus,
  skipped: CircleSlash,
  success: Check,
};

// OUTCOME_LABEL_KEYS names each outcome.
// OUTCOME_LABEL_KEYS 为每种结果命名。
const OUTCOME_LABEL_KEYS: Record<ChainOutcome, TranslationKey> = {
  denied: "toolCard.chain.denied",
  failure: "toolCard.chain.failure",
  not_run: "toolCard.chain.notRun",
  pending: "toolCard.chain.pending",
  skipped: "toolCard.chain.skipped",
  success: "toolCard.chain.success",
};

// STEP_STATES maps an outcome onto the kit step state.
// STEP_STATES 将结果映射为套件步骤状态。
const STEP_STATES: Record<ChainOutcome, string | undefined> = {
  denied: "failed",
  failure: "failed",
  not_run: undefined,
  pending: undefined,
  skipped: undefined,
  success: "done",
};

/** Renders one step: its order mark and outcome, then its own tool card or why it never ran.
 * 渲染单个步骤：其顺序标记与结果，然后是其自身工具卡或它未运行的原因。 */
function ChainStepRow(props: { parent: ToolCall; step: ChainStep }) {
  const { t } = useI18n();
  const { parent, step } = props;
  const [open, setOpen] = React.useState(false);
  const call = React.useMemo(() => stepCall(parent, step), [parent, step]);
  const { Card, family } = resolveToolCard(call);
  const ran = step.result !== null || step.outcome === "pending";
  const label = t(OUTCOME_LABEL_KEYS[step.outcome]);
  const Icon = step.outcome === "pending" ? null : OUTCOME_ICONS[step.outcome];
  return (
    <li className="vk-step vk-chain-step" data-outcome={step.outcome} data-state={STEP_STATES[step.outcome]}>
      <span aria-label={label} className="vk-step-mark" role="img" title={label}>
        {Icon ? <Icon aria-hidden="true" /> : step.index + 1}
      </span>
      <div className="vk-chain-step-body">
        <div className="vk-chain-step-head">
          <span>{formatMessage(t("toolCard.chain.step"), { index: step.index + 1 })}</span>
          {step.id ? <code>{step.id}</code> : null}
          <span>{label}</span>
        </div>
        {ran ? <Card call={call} family={family} onOpenChange={setOpen} open={open} /> : <code className="vk-chain-step-tool">{step.tool}</code>}
        {/* Only a failed or denied step reads as an error; a skip is the plan working as written. */}
        {/* 只有失败或被拒绝的步骤显示为错误；跳过是计划按写法运行的结果。 */}
        {!step.detail ? null : STEP_STATES[step.outcome] === "failed"
          ? <ErrorNote text={step.detail} />
          : <p className="vk-chain-step-note">{step.detail}</p>}
      </div>
    </li>
  );
}

/** Renders one tool_chain call.
 * 渲染一次 tool_chain 调用。 */
export function ToolChainCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const steps = React.useMemo(() => chainSteps(call), [call]);
  const ran = steps.filter((step) => step.outcome === "success" || step.outcome === "failure").length;
  const target = steps.map((step) => step.tool).join(" → ");
  const reported = call.host?.kind === "tool_chain";
  return (
    <ToolFrame
      {...props}
      icon={Workflow}
      meta={reported && steps.length > 0 ? <span>{formatMessage(t("toolCard.chain.progress"), { ran, total: steps.length })}</span> : null}
      state={call.state}
      target={target}
      targetTitle={target}
      verb={t("toolCard.chain.verb")}
    >
      {call.isError && !reported ? <ErrorNote text={call.modelOutput} /> : steps.length > 0 ? (
        <ol className="vk-steps-list vk-chain-steps">
          {steps.map((step) => <ChainStepRow key={step.index} parent={call} step={step} />)}
        </ol>
      ) : null}
    </ToolFrame>
  );
}
