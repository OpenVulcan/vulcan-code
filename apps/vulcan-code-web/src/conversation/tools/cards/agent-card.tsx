// Agent card for subagent and session-collaboration tools. The head names the agent or the message; the live
// delivery or task state follows the exact question and execution identities; the body shows what was sent, or
// the returned report when the call only looks or waits.
// 子代理与会话协作工具的代理卡。标题标明代理或消息；实时投递或任务状态按精确问题与执行身份跟随；正文展示发出的内容，
// 只查看或等待的调用则展示返回的报告。
import {
  Bot, CircleStop, Hourglass, MessageCircleQuestion, MessageSquareReply, MessagesSquare, Network, Send, SquareX, Users,
  type LucideIcon,
} from "lucide-react";
import { ConversationCollaborationStatus } from "@/components/conversation/conversation-collaboration-card";
import { useI18n, type TranslationKey } from "@/i18n";
import { ToolFrame, type ToolCardProps } from "../tool-card";
import { listOf, numberOf, textOf, type ToolCall } from "../tool-call";
import { ErrorNote, MarkdownResult } from "../views/result-views";

// AgentToolSpec fixes how one agent tool is named, what it targets and which argument it authored.
// AgentToolSpec 固定一个代理工具的命名、目标以及其撰写内容所在的参数。
interface AgentToolSpec {
  // Head icon.
  // 标题图标。
  icon: LucideIcon;
  // Action verb.
  // 动作词。
  verb: TranslationKey;
  // Argument naming the target.
  // 标明目标的参数。
  target?: string;
  // Argument holding the authored text, shown as the body.
  // 承载撰写文本的参数，作为正文展示。
  message?: string;
}

// AGENT_TOOLS is keyed by canonical tool name; only these tools use the agent card.
// AGENT_TOOLS 以规范工具名为键；只有这些工具使用代理卡。
const AGENT_TOOLS: Readonly<Record<string, AgentToolSpec>> = {
  spawn_agent: { icon: Bot, message: "message", target: "task_name", verb: "toolCard.agent.spawn" },
  send_message: { icon: Send, message: "message", target: "target", verb: "toolCard.agent.send" },
  wait_agent: { icon: Hourglass, target: "target", verb: "toolCard.agent.wait" },
  wait_agents_result: { icon: Hourglass, verb: "toolCard.agent.waitResults" },
  list_agents: { icon: Users, target: "path_prefix", verb: "toolCard.agent.list" },
  agent_orchestration_info: { icon: Network, verb: "toolCard.agent.profiles" },
  interrupt_agent: { icon: CircleStop, target: "target", verb: "toolCard.agent.interrupt" },
  close_agent: { icon: SquareX, target: "target", verb: "toolCard.agent.close" },
  ask_question: { icon: MessageCircleQuestion, message: "question", target: "question", verb: "toolCard.agent.ask" },
  ask_parent: { icon: MessageCircleQuestion, message: "question", target: "question", verb: "toolCard.agent.askParent" },
  reply_question: { icon: MessageSquareReply, message: "answer", target: "answer", verb: "toolCard.agent.reply" },
  wait_question: { icon: Hourglass, target: "question_id", verb: "toolCard.agent.waitAnswer" },
  cancel_question: { icon: SquareX, target: "reason", verb: "toolCard.agent.cancelQuestion" },
  list_sessions: { icon: MessagesSquare, verb: "toolCard.agent.listSessions" },
  read_session: { icon: MessagesSquare, target: "session_id", verb: "toolCard.agent.readSession" },
  wait_session: { icon: Hourglass, target: "session_id", verb: "toolCard.agent.waitSession" },
  send_session_message: { icon: Send, message: "message", target: "message", verb: "toolCard.agent.sendSession" },
  send_notification: { icon: Send, message: "message", target: "message", verb: "toolCard.agent.notify" },
};

// agentToolSpec returns the spec of one agent tool, or null for any other tool.
// agentToolSpec 返回代理工具的规格；其他工具返回 null。
export function agentToolSpec(call: ToolCall): AgentToolSpec | null {
  return Object.hasOwn(AGENT_TOOLS, call.name) ? AGENT_TOOLS[call.name] : null;
}

// agentTarget reads the head target: the named argument, then the spawned agent's title.
// agentTarget 读取标题目标：先取指定参数，再取已创建代理的标题。
function agentTarget(call: ToolCall, spec: AgentToolSpec | null): string {
  const named = spec?.target ? textOf(call.input[spec.target])?.trim() : null;
  if (named) return named.split("\n")[0];
  const targets = listOf(call.input.targets).filter((value): value is string => typeof value === "string");
  if (targets.length > 0) return targets.join(", ");
  return textOf(call.host?.payload.display_title) ?? textOf(call.input.agent_id) ?? "";
}

/** Renders one subagent or collaboration call.
 * 渲染一次子代理或协作调用。 */
export function AgentCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const spec = agentToolSpec(call);
  const receipt = call.host?.payload ?? {};
  const target = agentTarget(call, spec);
  const message = spec?.message ? textOf(call.input[spec.message]) ?? "" : "";
  const agentCount = numberOf(receipt.agent_count);
  return (
    <ToolFrame
      {...props}
      icon={spec?.icon ?? Bot}
      meta={(
        <>
          {agentCount !== null ? <span>{t("toolCard.agent.count").replace("{count}", String(agentCount))}</span> : null}
          <ConversationCollaborationStatus
            deliveryId={textOf(receipt.delivery_id)}
            executionId={textOf(receipt.execution_id)}
            notificationId={textOf(receipt.notification_id)}
            questionId={textOf(receipt.question_id) ?? textOf(call.input.question_id)}
          />
        </>
      )}
      state={call.state}
      target={target}
      targetTitle={target}
      verb={spec ? t(spec.verb) : call.event.tool_name}
    >
      {call.isError
        ? <ErrorNote text={call.modelOutput} />
        : message.trim()
          ? <MarkdownResult text={message} />
          : call.modelOutput?.trim() ? <MarkdownResult text={call.modelOutput} /> : null}
    </ToolFrame>
  );
}
