import * as React from "react";
import { ArrowLeft, ExternalLink, GitPullRequest, Megaphone, MessageSquarePlus, RefreshCw, Trash2 } from "lucide-react";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { OpenVulcanAccountContext } from "@/components/features/account/openvulcan-account-context";
import { signedInAccount } from "@/components/features/account/openvulcan-account-parts";
import { AppUpdateContext } from "@/components/features/app-update/app-update-context";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import { errorMessage, formatDateTime } from "@/lib/display-format";
import { openExternalUrl } from "@/service/project-actions-service";
import {
  deleteFeedback, linkFeedbackPullRequest, listFeedback, listMessages, markAllMessagesRead, markMessagesRead, readFeedback,
  type Announcement, type FeedbackDetail, type FeedbackStatus, type FeedbackSummary, type PersonalMessage,
} from "@/service/feedback-service";
import { Alert, AlertAction, AlertDescription } from "@/ui-kit/primitives/alert";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/ui-kit/primitives/alert-dialog";
import { Badge, type BadgeVariant } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/ui-kit/primitives/empty";
import { Input } from "@/ui-kit/primitives/input";
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/ui-kit/primitives/sheet";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/ui-kit/primitives/tabs";
import { FeedbackCenterContext, type FeedbackCenterController, type MessageCenterTab } from "./feedback-center-context";
import "@/ui-kit/styles/features/feedback.css";

// STATUS_TONES maps each triage state onto a badge tone.
// STATUS_TONES 将每个处理状态映射到徽标色调。
const STATUS_TONES: Record<FeedbackStatus, BadgeVariant> = {
  received: "secondary",
  triaged: "outline",
  accepted: "default",
  duplicate: "secondary",
  wont_fix: "secondary",
  done: "success",
};

// statusKey returns the catalog key of a triage state.
// statusKey 返回处理状态的目录键。
function statusKey(status: FeedbackStatus): TranslationKey {
  return `messageCenter.status.${status}` as TranslationKey;
}

// timeOf formats an RFC 3339 instant in the interface language, keeping the raw text when it does not parse.
// timeOf 以界面语言格式化 RFC 3339 时间点；无法解析时保留原文。
function timeOf(value: string, locale: string): string {
  return formatDateTime(Date.parse(value), locale) ?? value;
}

// MessageCenterSheet shows announcements, personal messages and the user's own reports in one side sheet.
// MessageCenterSheet 在一个侧边面板中显示公告、个人消息与用户本人的报告。
export default function MessageCenterSheet() {
  const center = React.useContext(FeedbackCenterContext);
  const { t } = useI18n();
  // feedbackId opens one report's detail, also from a message that refers to it.
  // feedbackId 打开一份报告的详情，也可由引用它的消息打开。
  const [feedbackId, setFeedbackId] = React.useState<string | null>(null);
  if (!center || !center.messageCenterTab) return null;
  const tab = center.messageCenterTab;
  const unread = center.summary?.unread ?? 0;
  const unseen = center.announcements?.unseen ?? 0;
  return (
    <Sheet open onOpenChange={(open) => { if (!open) center.closeMessageCenter(); }}>
      <SheetContent side="right" size="md" className="vk-inbox">
        <SheetHeader>
          <SheetTitle>{t("messageCenter.title")}</SheetTitle>
          <SheetDescription>{t("messageCenter.description")}</SheetDescription>
        </SheetHeader>
        <Tabs value={tab} onValueChange={(value) => center.openMessageCenter(value as MessageCenterTab)} className="vk-inbox-tabs">
          <TabsList variant="line">
            <TabsTrigger value="announcements">{t("messageCenter.tab.announcements")}{unseen > 0 ? <Badge>{unseen}</Badge> : null}</TabsTrigger>
            <TabsTrigger value="messages">{t("messageCenter.tab.messages")}{unread > 0 ? <Badge>{unread}</Badge> : null}</TabsTrigger>
            <TabsTrigger value="feedback">{t("messageCenter.tab.feedback")}</TabsTrigger>
          </TabsList>
          <TabsContent value="announcements" className="vk-inbox-body">
            <AnnouncementsPanel center={center} onOpenFeedback={(id) => { setFeedbackId(id); center.openMessageCenter("feedback"); }} />
          </TabsContent>
          <TabsContent value="messages" className="vk-inbox-body">
            <MessagesPanel center={center} onOpenFeedback={(id) => { setFeedbackId(id); center.openMessageCenter("feedback"); }} />
          </TabsContent>
          <TabsContent value="feedback" className="vk-inbox-body">
            {feedbackId
              ? <FeedbackDetailPanel feedbackId={feedbackId} onBack={() => setFeedbackId(null)} />
              : <FeedbackListPanel center={center} onOpen={setFeedbackId} />}
          </TabsContent>
        </Tabs>
      </SheetContent>
    </Sheet>
  );
}

// AnnouncementsPanel lists the announcements and records the visible ones as seen.
// AnnouncementsPanel 列出公告，并把可见的公告记为已看。
function AnnouncementsPanel(props: { center: FeedbackCenterController; onOpenFeedback: (id: string) => void }) {
  const { t } = useI18n();
  const view = props.center.announcements;
  const { runtime } = props.center;
  const unseenIds = React.useMemo(() => view?.announcements.filter((announcement) => !announcement.seen).map((announcement) => announcement.id) ?? [], [view]);
  React.useEffect(() => {
    if (unseenIds.length > 0) void runtime.current?.markSeen(unseenIds).catch(() => undefined);
  }, [unseenIds, runtime]);
  if (!view) return <p className="vk-inbox-note">{t("ui.loading")}</p>;
  if (!view.enabled) return <InboxEmpty title={t("messageCenter.announcementsOff")} description={t("messageCenter.announcementsOffHint")} />;
  return (
    <>
      {view.offline ? <Alert variant="warning"><AlertDescription>{t("messageCenter.offline")}</AlertDescription></Alert> : null}
      {view.announcements.length === 0
        ? <InboxEmpty title={t("messageCenter.noAnnouncements")} />
        : view.announcements.map((announcement) => <AnnouncementItem key={announcement.id} announcement={announcement} onOpenFeedback={props.onOpenFeedback} />)}
    </>
  );
}

// AnnouncementItem renders one announcement with its Markdown body and its action.
// AnnouncementItem 渲染一条公告及其 Markdown 正文与操作。
function AnnouncementItem(props: { announcement: Announcement; onOpenFeedback: (id: string) => void }) {
  const { t, locale } = useI18n();
  const appUpdate = React.useContext(AppUpdateContext);
  const center = React.useContext(FeedbackCenterContext);
  const { announcement } = props;
  const link = announcement.link;
  const [error, setError] = React.useState("");
  const act = () => {
    if (!link) return;
    if (link.kind === "app_update") { center?.closeMessageCenter(); appUpdate?.openSettings(); return; }
    if (link.kind === "feedback" && link.feedback_id) { props.onOpenFeedback(link.feedback_id); return; }
    if (link.kind === "url" && link.url) void openExternalUrl(link.url).catch((cause) => setError(errorMessage(cause)));
  };
  return (
    <article className="vk-inbox-item" data-important={announcement.priority === "important" ? "" : undefined}>
      <header>
        <Megaphone aria-hidden="true" />
        <strong>{announcement.title}</strong>
        <Badge variant={announcement.priority === "important" ? "warning" : "secondary"}>{t(`messageCenter.kind.${announcement.kind}` as TranslationKey)}</Badge>
        <time dateTime={announcement.startsAt}>{timeOf(announcement.startsAt, locale)}</time>
      </header>
      <MarkdownMessage content={announcement.body} eager />
      {link ? (
        <Button size="sm" variant="outline" onClick={act}>
          {link.kind === "url" ? <ExternalLink aria-hidden="true" /> : null}
          {t(`messageCenter.link.${link.kind}` as TranslationKey)}
        </Button>
      ) : null}
      {error ? <p className="vk-inbox-error">{error}</p> : null}
    </article>
  );
}

// useSignedIn tells whether an OpenVulcan account is signed in and offers its sign-in entry.
// useSignedIn 判断是否已登录 OpenVulcan 账户，并提供登录入口。
function useSignedIn(): { signedIn: boolean; signIn: (() => void) | null } {
  const account = React.useContext(OpenVulcanAccountContext);
  return { signedIn: Boolean(account && signedInAccount(account)), signIn: account ? () => account.setOpen(true) : null };
}

// SignInPrompt asks a signed-out user to sign in for personal content.
// SignInPrompt 请未登录用户登录以查看个人内容。
function SignInPrompt(props: { signIn: (() => void) | null; description: string }) {
  const { t } = useI18n();
  return (
    <Empty>
      <EmptyHeader>
        <EmptyTitle>{t("messageCenter.signInTitle")}</EmptyTitle>
        <EmptyDescription>{props.description}</EmptyDescription>
      </EmptyHeader>
      {props.signIn ? <Button size="sm" onClick={props.signIn}>{t("openvulcan.account.login")}</Button> : null}
    </Empty>
  );
}

// InboxEmpty renders an empty page.
// InboxEmpty 渲染空页面。
function InboxEmpty(props: { title: string; description?: string }) {
  return (
    <Empty>
      <EmptyHeader>
        <EmptyTitle>{props.title}</EmptyTitle>
        {props.description ? <EmptyDescription>{props.description}</EmptyDescription> : null}
      </EmptyHeader>
    </Empty>
  );
}

// LoadFailure shows a failed read with a retry action.
// LoadFailure 显示读取失败及重试操作。
function LoadFailure(props: { message: string; onRetry: () => void }) {
  const { t } = useI18n();
  return (
    <Alert variant="destructive">
      <AlertDescription>{props.message}</AlertDescription>
      <AlertAction><Button size="sm" variant="outline" onClick={props.onRetry}><RefreshCw aria-hidden="true" />{t("messageCenter.retry")}</Button></AlertAction>
    </Alert>
  );
}

// usePages reads cursor pages of one list; `more` appends the next page.
// usePages 读取一个列表的游标分页；`more` 追加下一页。
function usePages<T>(enabled: boolean, read: (cursor: string | null) => Promise<{ items: T[]; next: string | null }>) {
  const [items, setItems] = React.useState<T[] | null>(null);
  const [next, setNext] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState("");
  const load = React.useCallback(async (cursor: string | null) => {
    setBusy(true);
    setError("");
    try {
      const page = await read(cursor);
      setItems((current) => cursor && current ? [...current, ...page.items] : page.items);
      setNext(page.next);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  }, [read]);
  React.useEffect(() => { if (enabled) void load(null); }, [enabled, load]);
  return { items, setItems, next, busy, error, reload: () => void load(null), more: () => void load(next) };
}

// readMessagePage adapts one message page to the pager.
// readMessagePage 将一页消息适配到分页器。
const readMessagePage = async (cursor: string | null) => {
  const page = await listMessages(cursor);
  return { items: page.messages, next: page.next_cursor };
};

// readFeedbackPage adapts one feedback page to the pager.
// readFeedbackPage 将一页反馈适配到分页器。
const readFeedbackPage = async (cursor: string | null) => {
  const page = await listFeedback(cursor);
  return { items: page.feedback, next: page.next_cursor };
};

// MessagesPanel lists personal messages; opening one marks it read.
// MessagesPanel 列出个人消息；打开一条即标记为已读。
function MessagesPanel(props: { center: FeedbackCenterController; onOpenFeedback: (id: string) => void }) {
  const { t, locale } = useI18n();
  const { signedIn, signIn } = useSignedIn();
  const pages = usePages<PersonalMessage>(signedIn, readMessagePage);
  const [actionError, setActionError] = React.useState("");
  if (!signedIn) return <SignInPrompt signIn={signIn} description={t("messageCenter.messagesSignIn")} />;
  // setUnread applies the unread count a mark-read call returned to the badge.
  // setUnread 把标记已读调用返回的未读数应用到徽标。
  const setUnread = (unread: number) => {
    if (props.center.summary) props.center.setSummary({ ...props.center.summary, unread });
  };
  const markRead = async (message: PersonalMessage) => {
    if (message.read_at) return;
    try {
      const result = await markMessagesRead([message.id]);
      setUnread(result.unread);
      const now = new Date().toISOString();
      pages.setItems((current) => current?.map((item) => item.id === message.id ? { ...item, read_at: now } : item) ?? current);
    } catch (cause) {
      setActionError(errorMessage(cause));
    }
  };
  const markAll = async () => {
    const latest = pages.items?.[0];
    if (!latest) return;
    try {
      const result = await markAllMessagesRead(latest.id);
      setUnread(result.unread);
      const now = new Date().toISOString();
      pages.setItems((current) => current?.map((item) => item.read_at ? item : { ...item, read_at: now }) ?? current);
    } catch (cause) {
      setActionError(errorMessage(cause));
    }
  };
  const anyUnread = pages.items?.some((message) => !message.read_at) ?? false;
  return (
    <>
      {pages.error ? <LoadFailure message={pages.error} onRetry={pages.reload} /> : null}
      {actionError ? <Alert variant="destructive"><AlertDescription>{actionError}</AlertDescription></Alert> : null}
      {anyUnread ? <div className="vk-inbox-toolbar"><Button size="sm" variant="ghost" onClick={() => void markAll()}>{t("messageCenter.markAllRead")}</Button></div> : null}
      {pages.items === null ? (pages.busy ? <p className="vk-inbox-note">{t("ui.loading")}</p> : null)
        : pages.items.length === 0 ? <InboxEmpty title={t("messageCenter.noMessages")} />
          : pages.items.map((message) => (
            <article key={message.id} className="vk-inbox-item" data-unread={message.read_at ? undefined : ""}>
              <header>
                {message.read_at ? null : <span className="vk-dot" data-tone="ember" aria-label={t("messageCenter.unread")} />}
                <strong>{message.title}</strong>
                <time dateTime={message.created_at}>{timeOf(message.created_at, locale)}</time>
              </header>
              <MarkdownMessage content={message.body} eager />
              <div className="vk-inbox-actions">
                {message.ref.feedback_id ? (
                  <Button size="sm" variant="outline" onClick={() => { void markRead(message); props.onOpenFeedback(message.ref.feedback_id as string); }}>{t("messageCenter.openFeedback")}</Button>
                ) : null}
                {message.read_at ? null : <Button size="sm" variant="ghost" onClick={() => void markRead(message)}>{t("messageCenter.markRead")}</Button>}
              </div>
            </article>
          ))}
      {pages.next ? <Button size="sm" variant="ghost" disabled={pages.busy} onClick={pages.more}>{t("messageCenter.loadMore")}</Button> : null}
    </>
  );
}

// FeedbackListPanel lists the user's own reports.
// FeedbackListPanel 列出用户本人的报告。
function FeedbackListPanel(props: { center: FeedbackCenterController; onOpen: (id: string) => void }) {
  const { t, locale } = useI18n();
  const { signedIn, signIn } = useSignedIn();
  const pages = usePages<FeedbackSummary>(signedIn, readFeedbackPage);
  if (!signedIn) return <SignInPrompt signIn={signIn} description={t("messageCenter.feedbackSignIn")} />;
  return (
    <>
      <div className="vk-inbox-toolbar">
        <Button size="sm" variant="outline" onClick={() => { props.center.closeMessageCenter(); props.center.openFeedback(); }}>
          <MessageSquarePlus aria-hidden="true" />{t("messageCenter.newFeedback")}
        </Button>
      </div>
      {pages.error ? <LoadFailure message={pages.error} onRetry={pages.reload} /> : null}
      {pages.items === null ? (pages.busy ? <p className="vk-inbox-note">{t("ui.loading")}</p> : null)
        : pages.items.length === 0 ? <InboxEmpty title={t("messageCenter.noFeedback")} />
          : pages.items.map((item) => (
            <button key={item.id} type="button" className="vk-inbox-item vk-inbox-row" onClick={() => props.onOpen(item.id)}>
              <header>
                <strong>{item.title}</strong>
                <Badge variant={STATUS_TONES[item.status]}>{t(statusKey(item.status))}</Badge>
              </header>
              <span className="vk-inbox-meta">
                {t(item.kind === "bug" ? "feedback.kind.bug" : "feedback.kind.suggestion")}
                {" · "}{timeOf(item.created_at, locale)}
                {item.reply_count > 0 ? <>{" · "}{formatMessage(t("messageCenter.replyCount"), { count: item.reply_count })}</> : null}
              </span>
            </button>
          ))}
      {pages.next ? <Button size="sm" variant="ghost" disabled={pages.busy} onClick={pages.more}>{t("messageCenter.loadMore")}</Button> : null}
    </>
  );
}

// FeedbackDetailPanel shows one report with its replies and links, links a pull request and deletes the report.
// FeedbackDetailPanel 显示一份报告及其回复与链接，可关联拉取请求并删除该报告。
function FeedbackDetailPanel(props: { feedbackId: string; onBack: () => void }) {
  const { t, locale } = useI18n();
  const [detail, setDetail] = React.useState<FeedbackDetail | null>(null);
  const [error, setError] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [prUrl, setPrUrl] = React.useState("");
  const [confirmDelete, setConfirmDelete] = React.useState(false);
  const load = React.useCallback(async () => {
    setError("");
    try {
      setDetail(await readFeedback(props.feedbackId));
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }, [props.feedbackId]);
  React.useEffect(() => { void load(); }, [load]);
  const link = async () => {
    setBusy(true);
    setError("");
    try {
      const result = await linkFeedbackPullRequest(props.feedbackId, prUrl);
      setDetail((current) => current ? { ...current, links: result.links } : current);
      setPrUrl("");
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };
  const remove = async () => {
    setBusy(true);
    setError("");
    try {
      await deleteFeedback(props.feedbackId);
      props.onBack();
    } catch (cause) {
      setError(errorMessage(cause));
      setBusy(false);
    }
  };
  return (
    <div className="vk-inbox-detail">
      <div className="vk-inbox-toolbar">
        <Button size="sm" variant="ghost" onClick={props.onBack}><ArrowLeft aria-hidden="true" />{t("messageCenter.back")}</Button>
      </div>
      {error ? <LoadFailure message={error} onRetry={() => void load()} /> : null}
      {detail ? (
        <>
          <header className="vk-inbox-detail-head">
            <strong>{detail.title}</strong>
            <Badge variant={STATUS_TONES[detail.status]}>{t(statusKey(detail.status))}</Badge>
            <time dateTime={detail.created_at}>{timeOf(detail.created_at, locale)}</time>
          </header>
          <p className="vk-inbox-text">{detail.body}</p>
          {detail.attachments_purged ? <p className="vk-inbox-note">{t("messageCenter.attachmentsPurged")}</p> : null}
          <section aria-label={t("messageCenter.replies")}>
            <h3>{t("messageCenter.replies")}</h3>
            {detail.replies.length === 0 ? <p className="vk-inbox-note">{t("messageCenter.noReplies")}</p>
              : detail.replies.map((reply) => (
                <article key={reply.created_at} className="vk-inbox-item">
                  <header><time dateTime={reply.created_at}>{timeOf(reply.created_at, locale)}</time></header>
                  <MarkdownMessage content={reply.body} eager />
                </article>
              ))}
          </section>
          <section aria-label={t("messageCenter.pullRequests")}>
            <h3>{t("messageCenter.pullRequests")}</h3>
            {detail.links.map((item) => (
              <Button key={item.url} size="sm" variant="link" onClick={() => void openExternalUrl(item.url).catch((cause) => setError(errorMessage(cause)))}>
                <GitPullRequest aria-hidden="true" />{item.url}
              </Button>
            ))}
            <form className="vk-inbox-link-form" onSubmit={(event) => { event.preventDefault(); void link(); }}>
              <Input value={prUrl} disabled={busy} aria-label={t("messageCenter.pullRequestUrl")} placeholder="https://github.com/OpenVulcan/vulcan-code/pull/…"
                onChange={(event) => setPrUrl(event.currentTarget.value)} />
              <Button size="sm" type="submit" variant="outline" disabled={busy || prUrl.trim().length === 0}>{t("messageCenter.linkPullRequest")}</Button>
            </form>
          </section>
          <div className="vk-inbox-toolbar">
            <Button size="sm" variant="danger" disabled={busy} onClick={() => setConfirmDelete(true)}><Trash2 aria-hidden="true" />{t("messageCenter.deleteFeedback")}</Button>
          </div>
          <AlertDialog open={confirmDelete} onOpenChange={setConfirmDelete}>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>{t("messageCenter.deleteTitle")}</AlertDialogTitle>
                <AlertDialogDescription>{t("messageCenter.deleteDescription")}</AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel disabled={busy}>{t("conversation.action.cancel")}</AlertDialogCancel>
                <AlertDialogAction variant="danger" disabled={busy} onClick={() => void remove()}>{t("messageCenter.deleteFeedback")}</AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        </>
      ) : error ? null : <p className="vk-inbox-note">{t("ui.loading")}</p>}
    </div>
  );
}
