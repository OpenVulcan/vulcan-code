// The shell_job card: a look at, a wait for, or a stop of a command job.
// shell_job 卡片：查看、等待或停止一个命令任务。
import { SquareTerminal } from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";
import { ToolFrame, type ToolCardProps } from "../tool-card";
import { textOf } from "../tool-call";
import { TerminalView } from "../views/terminal-view";

// SHELL_JOB_VERBS names each shell_job action.
// SHELL_JOB_VERBS 命名每个 shell_job 动作。
const SHELL_JOB_VERBS: Record<string, TranslationKey> = {
  list: "toolCard.shellJob.list",
  read: "toolCard.shellJob.read",
  search: "toolCard.shellJob.search",
  status: "toolCard.shellJob.status",
  stop: "toolCard.shellJob.stop",
  wait: "toolCard.shellJob.wait",
};

/** Renders one shell_job call.
 * 渲染一次 shell_job 调用。 */
export function ShellJobCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const action = textOf(call.input.action) ?? "";
  const jobId = textOf(call.input.job_id);
  const pattern = action === "search" ? textOf(call.input.pattern) : null;
  const target = pattern ? `${jobId ?? ""} /${pattern}/`.trim() : jobId ?? "";
  return (
    <ToolFrame
      {...props}
      icon={SquareTerminal}
      state={call.state}
      target={target}
      targetTitle={target}
      verb={t(SHELL_JOB_VERBS[action] ?? "toolCard.shellJob.job")}
    >
      {call.modelOutput ? <TerminalView emptyText={t("toolPresentation.output.empty")} text={call.modelOutput} /> : null}
    </ToolFrame>
  );
}
