import { Brain, GitBranch, GitCompare, History, Library } from "lucide-react";
import { formatMessage, useI18n } from "@/i18n";
import type { ConversationSessionReviewSummary } from "@/components/conversation/session-todolist-panel";
import { formatConversationRelativeTime } from "@/components/project/project-sidebar-time";
import type { ConversationSummary } from "@/types";

// START_RECENT_LIMIT caps the recent-session list under the start composer.
// START_RECENT_LIMIT 限制起始输入区下方最近会话列表的条数。
const START_RECENT_LIMIT = 4;

// ConversationStartHero opens a blank conversation: the project and branch it will work in, the question and
// one line on what happens next. It sits in the composer column, so it can never overlap the draft controls.
// ConversationStartHero 作为空会话的开场：将要工作的项目与分支、提问以及接下来会发生什么的一句说明。
// 它位于输入区所在的列中，因此不会与草稿控件重叠。
export function ConversationStartHero(props: {
  // Project the conversation belongs to.
  // 会话所属的项目。
  projectName: string;
  // Current branch of the project's primary repository, when known.
  // 项目主仓库的当前分支（已知时）。
  branchName: string | null;
  // Whether the lead line is shown; a persisted empty session keeps only the question.
  // 是否显示说明行；已持久化的空会话只保留提问。
  showLead: boolean;
}) {
  const { t } = useI18n();
  // The project name is user data, so the localized question is split around its placeholder rather than
  // formatted into one string, letting the name carry its own emphasis.
  // 项目名称属于用户数据，因此本地化提问按占位符拆分而不是格式化为一个字符串，使名称带有自身的强调。
  const [before, after = ""] = t("console.emptyPrompt").split("{projectName}");
  return (
    <header className="vk-start-hero">
      <span className="vk-label">
        <span>{props.projectName}</span>
        {props.branchName ? <><GitBranch aria-hidden="true" /><span>{props.branchName}</span></> : null}
      </span>
      <h1>{before}<em>{props.projectName}</em>{after}</h1>
      {props.showLead ? <p>{t("start.lead")}</p> : null}
    </header>
  );
}

// ConversationStartFooter offers the ways to begin that exist right now: the last session, the uncommitted
// changes, and the project's knowledge and memory, followed by the other recent sessions of the project.
// Entries without real state behind them are left out instead of shown empty.
// ConversationStartFooter 提供当下真实存在的开始方式：上次的会话、未提交变更、项目的知识与记忆，随后是项目的其他
// 最近会话。没有真实状态支撑的入口直接省略，不显示为空。
export function ConversationStartFooter(props: {
  // Project whose sessions are listed.
  // 列出其会话的项目。
  projectId: string;
  // Every known conversation; the footer picks this project's ordinary sessions.
  // 所有已知会话；底部只挑选该项目的普通会话。
  conversations: readonly ConversationSummary[];
  // Live Git summary of the project, when monitored.
  // 项目的实时 Git 摘要（启用监听时）。
  reviewSummary: ConversationSessionReviewSummary | null;
  onSelectConversation: (conversationId: string) => void;
  onOpenReviewChanges?: () => void;
  onOpenKnowledge: () => void;
  onOpenMemory?: () => void;
}) {
  const { t, locale } = useI18n();
  const nowMs = Date.now();
  // lastUpdatedAt carries epoch milliseconds, so recency compares numerically.
  // lastUpdatedAt 为毫秒时间戳，因此按数值比较新近程度。
  const updatedAt = (conversation: ConversationSummary) => Number.parseInt(conversation.lastUpdatedAt ?? "", 10) || 0;
  const sessions = props.conversations
    .filter((conversation) => conversation.projectId === props.projectId && !conversation.assistantName)
    .sort((left, right) => updatedAt(right) - updatedAt(left));
  const [latest, ...older] = sessions;
  const review = props.reviewSummary && props.reviewSummary.changedFiles > 0 ? props.reviewSummary : null;
  const recent = older.slice(0, START_RECENT_LIMIT);
  return (
    <div className="vk-start-more">
      <div className="vk-start-grid">
        {latest ? (
          <button className="vk-start-card" onClick={() => props.onSelectConversation(latest.id)} type="button">
            <History aria-hidden="true" />
            <strong>{t("start.continue")}</strong>
            <small>{latest.title}</small>
          </button>
        ) : null}
        {review && props.onOpenReviewChanges ? (
          <button className="vk-start-card" onClick={props.onOpenReviewChanges} type="button">
            <GitCompare aria-hidden="true" />
            <strong>{t("start.review")}</strong>
            <small>{formatMessage(t("start.reviewDetail"), {
              files: review.changedFiles,
              additions: review.additions,
              deletions: review.deletions,
            })}</small>
          </button>
        ) : null}
        <button className="vk-start-card" onClick={props.onOpenKnowledge} type="button">
          <Library aria-hidden="true" />
          <strong>{t("start.knowledge")}</strong>
          <small>{t("start.knowledgeDetail")}</small>
        </button>
        {props.onOpenMemory ? (
          <button className="vk-start-card" onClick={props.onOpenMemory} type="button">
            <Brain aria-hidden="true" />
            <strong>{t("start.memory")}</strong>
            <small>{t("start.memoryDetail")}</small>
          </button>
        ) : null}
      </div>
      {recent.length > 0 ? (
        <section className="vk-start-recent" aria-label={t("start.recent")}>
          <h3 className="vk-label">{t("start.recent")}</h3>
          {recent.map((conversation) => (
            <button
              className="vk-start-session"
              key={conversation.id}
              onClick={() => props.onSelectConversation(conversation.id)}
              type="button"
            >
              <span>{conversation.title}</span>
              <small>{formatConversationRelativeTime(conversation, nowMs, locale)}</small>
            </button>
          ))}
        </section>
      ) : null}
    </div>
  );
}
