// The shell_job card: a look at, a wait for, or a stop of a command job. The head names the job by its full id
// followed by its purpose, which is the part cut short on a narrow card.
// shell_job 卡片：查看、等待或停止一个命令任务。卡头以完整任务标识加用途称呼该任务，卡片较窄时截短用途。
import * as React from "react";
import { SquareTerminal } from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";
import { listShellJobs } from "@/service/session-activity-service";
import { ToolCardSessionContext, ToolFrame, type ToolCardProps } from "../tool-card";
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

// jobPurposeLookups holds one job list request per session, shared by every card of that session; a job missing from
// it asks again once, since it may have started after the list was read.
// jobPurposeLookups 按会话保存一次任务列表请求，由该会话的所有卡片共享；列表中缺少的任务会再请求一次，因为它可能在
// 列表读取之后才开始。
const jobPurposeLookups = new Map<string, Promise<Map<string, string>>>();

function lookUpJobPurpose(sessionId: string, jobId: string, retry = true): Promise<string | null> {
  let lookup = jobPurposeLookups.get(sessionId);
  if (!lookup) {
    lookup = listShellJobs(sessionId, true).then((list) => new Map((list?.jobs ?? []).map((job) => [job.jobId, job.purpose])));
    jobPurposeLookups.set(sessionId, lookup);
    void lookup.catch(() => jobPurposeLookups.delete(sessionId));
  }
  return lookup.then((purposes) => {
    const purpose = purposes.get(jobId);
    if (purpose !== undefined || !retry) return purpose ?? null;
    jobPurposeLookups.delete(sessionId);
    return lookUpJobPurpose(sessionId, jobId, false);
  });
}

// useJobPurpose returns the purpose a finished call reported, or looks it up while the call still runs.
// useJobPurpose 返回已结束调用报告的用途；调用仍在运行时按任务标识查询。
function useJobPurpose(reported: string | null, jobId: string | null): string | null {
  const sessionId = React.useContext(ToolCardSessionContext);
  const [looked, setLooked] = React.useState<{ jobId: string; purpose: string | null } | null>(null);
  React.useEffect(() => {
    if (reported || !jobId || !sessionId) return undefined;
    let cancelled = false;
    lookUpJobPurpose(sessionId, jobId)
      .then((purpose) => {
        if (!cancelled) setLooked({ jobId, purpose });
      })
      .catch((error: unknown) => console.warn("Failed to look up the shell job purpose.", error));
    return () => {
      cancelled = true;
    };
  }, [jobId, reported, sessionId]);
  return reported || (looked?.jobId === jobId ? looked.purpose : null);
}

/** Renders one shell_job call.
 * 渲染一次 shell_job 调用。 */
export function ShellJobCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const payload = call.host?.kind === "shell_job" ? call.host.payload : null;
  const action = textOf(call.input.action) ?? "";
  const jobId = textOf(call.input.job_id);
  const purpose = useJobPurpose(textOf(payload?.purpose)?.trim() || null, jobId);
  const pattern = action === "search" ? textOf(call.input.pattern) : null;
  const detail = [pattern ? `/${pattern}/` : null, purpose].filter(Boolean).join(" · ");
  return (
    <ToolFrame
      {...props}
      icon={SquareTerminal}
      state={call.state}
      target={jobId ? <>{jobId}{detail ? <small>{detail}</small> : null}</> : detail}
      targetTitle={[jobId, detail].filter(Boolean).join("\n")}
      verb={t(SHELL_JOB_VERBS[action] ?? "toolCard.shellJob.job")}
    >
      {call.modelOutput ? <TerminalView emptyText={t("toolPresentation.output.empty")} text={call.modelOutput} /> : null}
    </ToolFrame>
  );
}
