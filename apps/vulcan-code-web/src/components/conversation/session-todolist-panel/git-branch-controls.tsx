import React from "react";
import "@/styles/session-git-branch.css";
import { Check, GitBranch, GitGraph, LoaderCircle, Plus, RefreshCw, Search, X } from "lucide-react";
import { toast } from "@/ui-kit/primitives/toast";
import { Button } from "@/ui-kit/primitives/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/ui-kit/primitives/field";
import { Input } from "@/ui-kit/primitives/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/ui-kit/primitives/popover";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import type {
  WorkspaceGitHistoryCommit,
  WorkspaceGitHistoryPage,
  WorkspaceGitLocalBranchList,
} from "@/diff/types";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";

// GIT_GRAPH_LANE_WIDTH is the horizontal distance between two graph lanes in pixels.
// GIT_GRAPH_LANE_WIDTH 是两条图谱泳道之间的水平像素距离。
const GIT_GRAPH_LANE_WIDTH = 14;

// GIT_GRAPH_ROW_HEIGHT is the pixel height of one graph row; CSS reads it through a custom property.
// GIT_GRAPH_ROW_HEIGHT 是单行图谱的像素高度；CSS 通过自定义属性读取该值。
const GIT_GRAPH_ROW_HEIGHT = 30;

// GIT_GRAPH_MAX_VISIBLE_LANES caps the graph column so wide histories cannot squeeze the description.
// GIT_GRAPH_MAX_VISIBLE_LANES 限制图谱列宽，避免宽历史挤压描述列。
const GIT_GRAPH_MAX_VISIBLE_LANES = 8;

// GIT_GRAPH_LANE_COLOR_COUNT matches the number of lane color tokens in session-git-branch.css.
// GIT_GRAPH_LANE_COLOR_COUNT 与 session-git-branch.css 中泳道颜色令牌数量一致。
const GIT_GRAPH_LANE_COLOR_COUNT = 6;

// GIT_GRAPH_LOAD_MORE_THRESHOLD is how close to the bottom, in pixels, scrolling starts the next page.
// GIT_GRAPH_LOAD_MORE_THRESHOLD 表示距离底部多少像素时开始加载下一页。
const GIT_GRAPH_LOAD_MORE_THRESHOLD = 48;

// ConversationSessionGitBranchActions binds branch and history IO to the selected workspace.
// ConversationSessionGitBranchActions 将分支与历史读写绑定到当前工作区。
export interface ConversationSessionGitBranchActions {
  // Configured prefix used to recommend new branch names.
  // 用于推荐新分支名的已配置前缀。
  commitBranchPrefix: string;
  // Lists local branches for one repository.
  // 列出单个仓库的本地分支。
  loadBranches: (repositoryKey: string | null) => Promise<WorkspaceGitLocalBranchList>;
  // Switches to a local branch, creating it from HEAD when requested, then refreshes the review summary.
  // 切换到本地分支（按请求基于 HEAD 新建），随后刷新审查摘要。
  switchBranch: (repositoryKey: string | null, branchName: string, create: boolean) => Promise<void>;
  // Loads the backend-sized history page that follows `skip` commits.
  // 加载跳过 `skip` 个提交之后、由后端决定大小的历史页。
  loadHistory: (repositoryKey: string | null, skip: number) => Promise<WorkspaceGitHistoryPage>;
}

// ConversationSessionBranchMenu renders the branch row and its branch switcher, create dialog, and Git graph.
// ConversationSessionBranchMenu 渲染分支行及其分支切换器、新建弹窗与 Git 图谱。
export function ConversationSessionBranchMenu(props: {
  // Branch IO bound to the workspace; null renders a read-only row.
  // 绑定到工作区的分支读写；为 null 时渲染只读行。
  actions: ConversationSessionGitBranchActions | null;
  // Stable repository key, or null for the default repository.
  // 稳定仓库键；为 null 时表示默认仓库。
  repositoryKey: string | null;
  // Current branch name from the review summary.
  // 来自审查摘要的当前分支名称。
  branchName: string;
  // Uncommitted changed file count of the current branch.
  // 当前分支未提交的变更文件数。
  changedFiles: number;
  // Whether branch mutations are blocked by another Git action or refresh.
  // 分支变更是否因其他 Git 动作或刷新而被阻止。
  busy: boolean;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const [menuOpen, setMenuOpen] = React.useState(false);
  const [createOpen, setCreateOpen] = React.useState(false);
  const [graphOpen, setGraphOpen] = React.useState(false);
  const [query, setQuery] = React.useState("");
  const [branchList, setBranchList] = React.useState<WorkspaceGitLocalBranchList | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [switchingBranch, setSwitchingBranch] = React.useState<string | null>(null);
  const displayName = conversationSessionBranchDisplayName(props.branchName);
  const actions = props.actions;
  const repositoryKey = props.repositoryKey;

  React.useEffect(() => {
    if (!menuOpen || !actions) {
      return undefined;
    }
    let cancelled = false;
    setQuery("");
    setLoadError(null);
    actions.loadBranches(repositoryKey).then(
      (list) => { if (!cancelled) setBranchList(list); },
      (error) => { if (!cancelled) setLoadError(error instanceof Error ? error.message : String(error)); },
    );
    return () => { cancelled = true; };
  }, [actions, menuOpen, repositoryKey]);

  if (!actions) {
    return (
      <div className="workspace-tool-panel-menu-row conversation-session-review-row">
        <GitBranch size={15} />
        <span>{displayName}</span>
      </div>
    );
  }

  const currentBranch = branchList?.currentBranch ?? displayName;
  const normalizedQuery = query.trim().toLowerCase();
  const visibleBranches = (branchList?.branches ?? [])
    .filter((name) => !normalizedQuery || name.toLowerCase().includes(normalizedQuery));

  // switchTo checks out an existing local branch and closes the menu on success.
  // switchTo 检出已有本地分支，成功后关闭菜单。
  const switchTo = async (branchName: string) => {
    if (branchName === currentBranch) {
      setMenuOpen(false);
      return;
    }
    setSwitchingBranch(branchName);
    try {
      await actions.switchBranch(repositoryKey, branchName, false);
      setMenuOpen(false);
      toast.success(t("gitBranchMenu.switched").replace("{name}", branchName));
    } catch (error) {
      toast.error(t("gitBranchMenu.switchFailed"), {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setSwitchingBranch(null);
    }
  };

  return (
    <>
      <Popover open={menuOpen} onOpenChange={setMenuOpen}>
        <PopoverTrigger asChild>
          <button
            aria-label={t("gitBranchMenu.label")}
            className={cn(
              "workspace-tool-panel-menu-row conversation-session-review-row conversation-session-branch-trigger",
              menuOpen && "active",
            )}
            title={t("gitBranchMenu.label")}
            type="button"
          >
            <GitBranch size={15} />
            <span>{displayName}</span>
          </button>
        </PopoverTrigger>
        <PopoverContent align="start" className="session-git-branch-menu" side="left" sideOffset={8}>
          <label className="session-git-branch-search">
            <Search size={14} />
            <input
              aria-label={t("gitBranchMenu.search")}
              autoFocus
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t("gitBranchMenu.search")}
              value={query}
            />
          </label>
          <div className="session-git-branch-list" role="listbox" aria-label={t("gitBranchMenu.label")}>
            {loadError ? <p className="session-git-branch-message is-error" role="alert">{t("gitBranchMenu.loadFailed")}：{loadError}</p> : null}
            {!loadError && !branchList ? (
              <StreamPlaceholder className="p-1" label={t("gitBranchMenu.loading")} shape="rows" count={4} />
            ) : null}
            {branchList && visibleBranches.length === 0 ? <p className="session-git-branch-message">{t("gitBranchMenu.empty")}</p> : null}
            {visibleBranches.map((name) => {
              const current = name === currentBranch;
              return (
                <button
                  aria-selected={current}
                  className={cn("session-git-branch-item", current && "is-current")}
                  disabled={Boolean(switchingBranch) || (props.busy && !current)}
                  key={name}
                  onClick={() => void switchTo(name)}
                  role="option"
                  title={name}
                  type="button"
                >
                  <GitBranch className="session-git-branch-item-icon" size={14} />
                  <span className="session-git-branch-item-text">
                    <span className="session-git-branch-item-name">{name}</span>
                    {current && props.changedFiles > 0 ? (
                      <small>{t("gitBranchMenu.uncommitted").replace("{count}", String(props.changedFiles))}</small>
                    ) : null}
                  </span>
                  {switchingBranch === name ? <LoaderCircle className="spin" size={14} /> : null}
                  {current ? <Check className="session-git-branch-item-check" size={14} /> : null}
                </button>
              );
            })}
          </div>
          <div className="session-git-branch-footer">
            <button
              className="session-git-branch-item"
              disabled={props.busy || Boolean(switchingBranch)}
              onClick={() => { setMenuOpen(false); setCreateOpen(true); }}
              type="button"
            >
              <Plus className="session-git-branch-item-icon" size={14} />
              <span className="session-git-branch-item-name">{t("gitBranchMenu.create")}</span>
            </button>
            <button
              className="session-git-branch-item"
              onClick={() => { setMenuOpen(false); setGraphOpen(true); }}
              type="button"
            >
              <GitGraph className="session-git-branch-item-icon" size={14} />
              <span className="session-git-branch-item-name">{t("gitBranchMenu.graph")}</span>
            </button>
          </div>
        </PopoverContent>
      </Popover>
      <ConversationSessionCreateBranchDialog
        actions={actions}
        branchNames={branchList?.branches ?? []}
        currentBranch={currentBranch}
        open={createOpen}
        repositoryKey={repositoryKey}
        onOpenChange={setCreateOpen}
      />
      <ConversationSessionGitGraphDialog
        actions={actions}
        open={graphOpen}
        repositoryKey={repositoryKey}
        onOpenChange={setGraphOpen}
      />
    </>
  );
}

// ConversationSessionCreateBranchDialog creates a branch from HEAD and checks it out.
// ConversationSessionCreateBranchDialog 基于 HEAD 新建分支并检出。
function ConversationSessionCreateBranchDialog(props: {
  // Branch IO bound to the workspace.
  // 绑定到工作区的分支读写。
  actions: ConversationSessionGitBranchActions;
  // Existing local branch names used to avoid recommending a taken name.
  // 用于避免推荐已占用名称的现有本地分支名。
  branchNames: string[];
  // Current branch name used to derive the recommendation.
  // 用于推导推荐名称的当前分支名。
  currentBranch: string;
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean;
  // Stable repository key, or null for the default repository.
  // 稳定仓库键；为 null 时表示默认仓库。
  repositoryKey: string | null;
  // Opens or closes the dialog.
  // 打开或关闭弹窗。
  onOpenChange: (open: boolean) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const [branchName, setBranchName] = React.useState("");
  const [error, setError] = React.useState<string | null>(null);
  const [submitting, setSubmitting] = React.useState(false);
  const inputId = React.useId();
  const { open, currentBranch, branchNames } = props;
  const commitBranchPrefix = props.actions.commitBranchPrefix;

  React.useEffect(() => {
    if (open) {
      setBranchName(conversationSessionDefaultNewBranchName(currentBranch, branchNames, commitBranchPrefix));
      setError(null);
    }
    // The recommendation is captured once per opening so typing is never overwritten.
    // 推荐名称在每次打开时只取一次，避免覆盖用户输入。
  }, [open]);

  // submit creates the branch and closes the dialog, or keeps it open with the Git diagnostic.
  // submit 新建分支并关闭弹窗；失败时保留弹窗并展示 Git 诊断。
  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    const name = branchName.trim();
    if (!name || submitting) {
      return;
    }
    setSubmitting(true);
    setError(null);
    try {
      await props.actions.switchBranch(props.repositoryKey, name, true);
      props.onOpenChange(false);
      toast.success(t("gitBranchCreate.created").replace("{name}", name));
    } catch (submitError) {
      setError(submitError instanceof Error ? submitError.message : String(submitError));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(next) => { if (!submitting) props.onOpenChange(next); }}>
      <DialogContent size="sm">
        <form className="session-git-create-branch-form" onSubmit={(event) => void submit(event)}>
          <DialogHeader>
            <DialogTitle>{t("gitBranchCreate.title")}</DialogTitle>
            <DialogDescription>{t("gitBranchCreate.description")}</DialogDescription>
          </DialogHeader>
          <Field data-invalid={error ? "true" : undefined}>
            <FieldLabel htmlFor={inputId}>{t("gitBranchCreate.name")}</FieldLabel>
            <Input
              aria-invalid={error ? "true" : undefined}
              autoFocus
              id={inputId}
              onChange={(event) => setBranchName(event.target.value)}
              onFocus={(event) => event.currentTarget.select()}
              placeholder={t("gitBranchCreate.placeholder").replace("{prefix}", commitBranchPrefix)}
              spellCheck={false}
              value={branchName}
            />
            {error ? <FieldError>{error}</FieldError> : <FieldDescription>{t("gitBranchCreate.note")}</FieldDescription>}
          </Field>
          <DialogFooter>
            <Button disabled={submitting} onClick={() => props.onOpenChange(false)} type="button" variant="outline">
              {t("gitBranchCreate.cancel")}
            </Button>
            <Button disabled={submitting || !branchName.trim()} type="submit">
              {submitting ? <LoaderCircle className="spin" /> : null}
              {t(submitting ? "gitBranchCreate.submitting" : "gitBranchCreate.submit")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

// ConversationSessionGitGraphDialog shows paged repository history with a toggleable commit detail pane.
// ConversationSessionGitGraphDialog 展示分页仓库历史，并提供可切换的提交详情面板。
function ConversationSessionGitGraphDialog(props: {
  // Branch IO bound to the workspace.
  // 绑定到工作区的分支读写。
  actions: ConversationSessionGitBranchActions;
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean;
  // Stable repository key, or null for the default repository.
  // 稳定仓库键；为 null 时表示默认仓库。
  repositoryKey: string | null;
  // Opens or closes the dialog.
  // 打开或关闭弹窗。
  onOpenChange: (open: boolean) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const [commits, setCommits] = React.useState<WorkspaceGitHistoryCommit[]>([]);
  const [hasMore, setHasMore] = React.useState(false);
  const [loading, setLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [selectedHash, setSelectedHash] = React.useState<string | null>(null);
  const scrollRef = React.useRef<HTMLDivElement | null>(null);
  // generationRef drops responses that belong to a closed dialog or an earlier refresh.
  // generationRef 丢弃属于已关闭弹窗或更早刷新的响应。
  const generationRef = React.useRef(0);
  const loadingRef = React.useRef(false);
  const { actions, open, repositoryKey } = props;

  // loadPage appends the page after `skip` commits, or replaces the list when skip is zero.
  // loadPage 追加跳过 `skip` 个提交之后的页面；skip 为零时替换列表。
  const loadPage = React.useCallback(async (skip: number) => {
    const generation = generationRef.current;
    loadingRef.current = true;
    setLoading(true);
    setError(null);
    try {
      const page = await actions.loadHistory(repositoryKey, skip);
      if (generation !== generationRef.current) {
        return;
      }
      setCommits((current) => (skip === 0 ? page.commits : [...current, ...page.commits]));
      setHasMore(page.hasMore);
    } catch (loadError) {
      if (generation === generationRef.current) {
        setError(loadError instanceof Error ? loadError.message : String(loadError));
      }
    } finally {
      if (generation === generationRef.current) {
        loadingRef.current = false;
        setLoading(false);
      }
    }
  }, [actions, repositoryKey]);

  // reload restarts history from the newest commit.
  // reload 从最新提交重新加载历史。
  const reload = React.useCallback(() => {
    generationRef.current += 1;
    setCommits([]);
    setHasMore(false);
    setSelectedHash(null);
    if (scrollRef.current) scrollRef.current.scrollTop = 0;
    void loadPage(0);
  }, [loadPage]);

  React.useEffect(() => {
    if (open) {
      reload();
      return undefined;
    }
    return () => { generationRef.current += 1; };
  }, [open, reload]);

  // loadMoreWhenNearBottom fetches the next page once the list is scrolled close to its end.
  // loadMoreWhenNearBottom 在列表滚动接近底部时拉取下一页。
  const loadMoreWhenNearBottom = React.useCallback(() => {
    const element = scrollRef.current;
    if (!element || loadingRef.current || !hasMore || error) {
      return;
    }
    if (element.scrollTop + element.clientHeight >= element.scrollHeight - GIT_GRAPH_LOAD_MORE_THRESHOLD) {
      void loadPage(commits.length);
    }
  }, [commits.length, error, hasMore, loadPage]);

  // A short first page cannot scroll, so it keeps filling until the viewport overflows or history ends.
  // 较短的首页无法滚动，因此持续加载直到视口溢出或历史结束。
  React.useEffect(() => {
    if (!loading) {
      loadMoreWhenNearBottom();
    }
  }, [loadMoreWhenNearBottom, loading]);

  const graph = React.useMemo(() => layoutGitGraph(commits), [commits]);
  const visibleLaneCount = Math.min(Math.max(graph.laneCount, 1), GIT_GRAPH_MAX_VISIBLE_LANES);
  const graphWidth = visibleLaneCount * GIT_GRAPH_LANE_WIDTH + GIT_GRAPH_LANE_WIDTH / 2;
  const selectedCommit = selectedHash ? commits.find((commit) => commit.hash === selectedHash) ?? null : null;
  const shortHashByHash = React.useMemo(
    () => new Map(commits.map((commit) => [commit.hash, commit.shortHash])),
    [commits],
  );

  return (
    <Dialog open={open} onOpenChange={props.onOpenChange}>
      <DialogContent
        className="session-git-graph-dialog"
        layout="viewer"
        onOpenAutoFocus={(event) => event.preventDefault()}
        showCloseButton={false}
        size="xl"
      >
        <header className="session-git-graph-header">
          <div>
            <DialogTitle>{t("gitGraph.title")}</DialogTitle>
            <DialogDescription>{t("gitGraph.description")}</DialogDescription>
          </div>
          <div className="session-git-graph-header-actions">
            <Button aria-label={t("gitGraph.refresh")} disabled={loading} onClick={reload} size="icon-sm" title={t("gitGraph.refresh")} variant="ghost">
              <RefreshCw className={cn(loading && commits.length === 0 && "spin")} />
            </Button>
            <Button aria-label={t("gitGraph.close")} onClick={() => props.onOpenChange(false)} size="icon-sm" title={t("gitGraph.close")} variant="ghost">
              <X />
            </Button>
          </div>
        </header>
        <div
          className="session-git-graph-table"
          style={{
            "--session-git-graph-width": `${graphWidth}px`,
            "--session-git-graph-row-height": `${GIT_GRAPH_ROW_HEIGHT}px`,
          } as React.CSSProperties}
        >
          <div className="session-git-graph-row is-head" role="row">
            <span role="columnheader">{t("gitGraph.columnGraph")}</span>
            <span role="columnheader">{t("gitGraph.columnDescription")}</span>
            <span role="columnheader">{t("gitGraph.columnDate")}</span>
            <span role="columnheader">{t("gitGraph.columnAuthor")}</span>
            <span role="columnheader">{t("gitGraph.columnCommit")}</span>
          </div>
          <div className="session-git-graph-body" onScroll={loadMoreWhenNearBottom} ref={scrollRef} role="rowgroup">
            {commits.map((commit, index) => {
              const selected = commit.hash === selectedHash;
              const isHead = commit.refs.some((ref) => ref.head);
              return (
                <button
                  aria-pressed={selected}
                  className={cn("session-git-graph-row", selected && "is-selected")}
                  key={commit.hash}
                  onClick={() => setSelectedHash(selected ? null : commit.hash)}
                  role="row"
                  type="button"
                >
                  <GitGraphCell row={graph.rows[index]} width={graphWidth} isHead={isHead} />
                  <span className="session-git-graph-description">
                    {commit.refs.map((ref) => (
                      <span className={cn("session-git-graph-ref", `is-${ref.kind}`, ref.head && "is-head")} key={`${ref.kind}:${ref.name}`}>
                        {ref.head && ref.kind !== "head" ? <strong>HEAD</strong> : null}
                        {ref.name}
                      </span>
                    ))}
                    <span className="session-git-graph-subject">{commit.subject}</span>
                  </span>
                  <span className="session-git-graph-muted">{formatGitGraphShortDate(commit.authorDate)}</span>
                  <span className="session-git-graph-muted session-git-graph-author" title={commit.authorEmail}>{commit.authorName}</span>
                  <span className="session-git-graph-hash">{commit.shortHash}</span>
                </button>
              );
            })}
            {error ? <p className="session-git-graph-message is-error" role="alert">{t("gitGraph.loadFailed")}：{error}</p> : null}
            {!error && loading ? (
              <p className="session-git-graph-message">
                <LoaderCircle className="spin" size={13} />
                {t(commits.length ? "gitGraph.loadingMore" : "gitGraph.loading")}
              </p>
            ) : null}
            {!error && !loading && commits.length === 0 ? <p className="session-git-graph-message">{t("gitGraph.empty")}</p> : null}
            {!error && !loading && commits.length > 0 && !hasMore ? <p className="session-git-graph-message">{t("gitGraph.end")}</p> : null}
          </div>
        </div>
        {selectedCommit ? (
          <dl className="session-git-graph-detail">
            <dt>{t("gitGraph.subject")}</dt>
            <dd>
              <strong>{selectedCommit.subject}</strong>
              {selectedCommit.body ? <pre>{selectedCommit.body}</pre> : null}
            </dd>
            <dt>{t("gitGraph.columnCommit")}</dt>
            <dd className="session-git-graph-hash">{selectedCommit.hash}</dd>
            <dt>{t("gitGraph.columnAuthor")}</dt>
            <dd>{selectedCommit.authorName}{selectedCommit.authorEmail ? ` <${selectedCommit.authorEmail}>` : ""}</dd>
            <dt>{t("gitGraph.columnDate")}</dt>
            <dd>{formatGitGraphFullDate(selectedCommit.authorDate)}</dd>
            <dt>{t("gitGraph.parents")}</dt>
            <dd className="session-git-graph-parents">
              {selectedCommit.parents.length === 0 ? t("gitGraph.noParents") : selectedCommit.parents.map((parent) => (
                <button key={parent} onClick={() => setSelectedHash(parent)} title={parent} type="button">
                  {shortHashByHash.get(parent) ?? parent.slice(0, 8)}
                </button>
              ))}
            </dd>
          </dl>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

// GitGraphCell draws one row of lanes, edges, and the commit node.
// GitGraphCell 绘制单行的泳道、连线与提交节点。
function GitGraphCell(props: { row: GitGraphRowLayout; width: number; isHead: boolean }) {
  const height = GIT_GRAPH_ROW_HEIGHT;
  const middle = height / 2;
  const x = (lane: number) => lane * GIT_GRAPH_LANE_WIDTH + GIT_GRAPH_LANE_WIDTH / 2;
  const laneClass = (lane: number) => `lane-${lane % GIT_GRAPH_LANE_COLOR_COUNT}`;
  const { row } = props;
  return (
    <svg aria-hidden className="session-git-graph-cell" height={height} width={props.width}>
      {row.passThrough.map((edge) => (
        <path className={laneClass(edge.to)} d={gitGraphEdgePath(x(edge.from), 0, x(edge.to), height)} key={`p${edge.from}`} />
      ))}
      {row.incoming.map((lane) => (
        <path className={laneClass(lane)} d={gitGraphEdgePath(x(lane), 0, x(row.lane), middle)} key={`i${lane}`} />
      ))}
      {row.outgoing.map((lane) => (
        <path className={laneClass(lane)} d={gitGraphEdgePath(x(row.lane), middle, x(lane), height)} key={`o${lane}`} />
      ))}
      <circle
        className={cn(laneClass(row.lane), "node", props.isHead && "is-head")}
        cx={x(row.lane)}
        cy={middle}
        r={props.isHead ? 4.5 : 3.5}
      />
    </svg>
  );
}

// GitGraphRowLayout lists which lanes one row draws.
// GitGraphRowLayout 列出单行需要绘制的泳道。
interface GitGraphRowLayout {
  // Lane holding the commit node.
  // 提交节点所在泳道。
  lane: number;
  // Lanes that enter the node from above.
  // 从上方汇入节点的泳道。
  incoming: number[];
  // Lanes that leave the node downward toward its parents.
  // 从节点向下通往父提交的泳道。
  outgoing: number[];
  // Unrelated lanes crossing the row; `from` differs from `to` when a lane folds into a lower one.
  // 穿过该行的无关泳道；泳道并入更低泳道时 `from` 与 `to` 不同。
  passThrough: { from: number; to: number }[];
}

// layoutGitGraph assigns lanes to commits listed children-first, keeping lanes stable across pages.
// layoutGitGraph 为子提交优先排列的提交分配泳道，使泳道在分页之间保持稳定。
export function layoutGitGraph(commits: WorkspaceGitHistoryCommit[]): { rows: GitGraphRowLayout[]; laneCount: number } {
  // lanes[i] holds the hash lane i is waiting for, or null for a free lane.
  // lanes[i] 保存泳道 i 等待的提交哈希；为 null 表示空闲泳道。
  const lanes: (string | null)[] = [];
  const rows: GitGraphRowLayout[] = [];
  let laneCount = 0;
  const freeLane = (excluded: number) => {
    const index = lanes.findIndex((value, lane) => value === null && lane !== excluded);
    return index >= 0 ? index : lanes.length;
  };
  for (const commit of commits) {
    let lane = lanes.indexOf(commit.hash);
    if (lane < 0) {
      lane = freeLane(-1);
    }
    const incoming: number[] = [];
    lanes.forEach((value, index) => {
      if (value === commit.hash) {
        incoming.push(index);
        lanes[index] = null;
      }
    });
    const crossing = lanes.flatMap((value, index) => (value !== null ? [{ from: index, hash: value }] : []));
    const outgoing: number[] = [];
    commit.parents.forEach((parent, parentIndex) => {
      const existing = lanes.indexOf(parent);
      if (existing >= 0) {
        // A fork point already awaited on a higher lane moves down so the mainline keeps its lane.
        // 已在更高泳道等待的分叉点下移，使主线保持原泳道。
        if (parentIndex === 0 && existing > lane && lanes[lane] == null) {
          lanes[existing] = null;
          lanes[lane] = parent;
          outgoing.push(lane);
        } else {
          outgoing.push(existing);
        }
        return;
      }
      const target = parentIndex === 0 && lanes[lane] == null ? lane : freeLane(lane);
      lanes[target] = parent;
      outgoing.push(target);
    });
    while (lanes.length > 0 && lanes[lanes.length - 1] === null) {
      lanes.pop();
    }
    laneCount = Math.max(laneCount, lane + 1, lanes.length);
    const passThrough = crossing.map((edge) => ({ from: edge.from, to: lanes.indexOf(edge.hash) }));
    rows.push({ lane, incoming, outgoing, passThrough });
  }
  return { rows, laneCount };
}

// gitGraphEdgePath draws a vertical segment or a smooth S-curve between two lane points.
// gitGraphEdgePath 在两个泳道点之间绘制竖线或平滑 S 形曲线。
function gitGraphEdgePath(x1: number, y1: number, x2: number, y2: number): string {
  if (x1 === x2) {
    return `M${x1} ${y1}V${y2}`;
  }
  const middleY = (y1 + y2) / 2;
  return `M${x1} ${y1}C${x1} ${middleY} ${x2} ${middleY} ${x2} ${y2}`;
}

// padGitGraphDatePart left-pads one date component to two digits.
// padGitGraphDatePart 将单个日期分量左补零到两位。
function padGitGraphDatePart(value: number): string {
  return String(value).padStart(2, "0");
}

// formatGitGraphShortDate renders an ISO date as local `MM/DD HH:mm`.
// formatGitGraphShortDate 将 ISO 日期渲染为本地 `MM/DD HH:mm`。
function formatGitGraphShortDate(isoDate: string): string {
  const date = new Date(isoDate);
  if (Number.isNaN(date.getTime())) {
    return isoDate;
  }
  return `${padGitGraphDatePart(date.getMonth() + 1)}/${padGitGraphDatePart(date.getDate())} ${padGitGraphDatePart(date.getHours())}:${padGitGraphDatePart(date.getMinutes())}`;
}

// formatGitGraphFullDate renders an ISO date as local `YYYY-MM-DD HH:mm:ss`.
// formatGitGraphFullDate 将 ISO 日期渲染为本地 `YYYY-MM-DD HH:mm:ss`。
function formatGitGraphFullDate(isoDate: string): string {
  const date = new Date(isoDate);
  if (Number.isNaN(date.getTime())) {
    return isoDate;
  }
  return `${date.getFullYear()}-${padGitGraphDatePart(date.getMonth() + 1)}-${padGitGraphDatePart(date.getDate())} ${padGitGraphDatePart(date.getHours())}:${padGitGraphDatePart(date.getMinutes())}:${padGitGraphDatePart(date.getSeconds())}`;
}

// conversationSessionBranchDisplayName removes noisy Git ref and origin prefixes for compact review display.
// conversationSessionBranchDisplayName 移除噪声 Git ref 与 origin 前缀以紧凑展示审查分支。
export function conversationSessionBranchDisplayName(name: string): string {
  const shortName = name
    .replace(/^refs\/heads\//, "")
    .replace(/^refs\/remotes\//, "");
  return shortName.startsWith("origin/") ? shortName.slice("origin/".length) : shortName;
}

// conversationSessionDefaultNewBranchName builds the first unused recommended branch name from the configured prefix.
// conversationSessionDefaultNewBranchName 基于已配置前缀生成第一个未占用的推荐分支名。
export function conversationSessionDefaultNewBranchName(currentBranchName: string, branchNames: string[], configuredPrefix: string): string {
  const normalizedCurrentBranch = conversationSessionNormalizeBranchName(currentBranchName);
  const existingBranches = new Set(branchNames.map(conversationSessionNormalizeBranchName));
  existingBranches.add(normalizedCurrentBranch);
  const slashIndex = normalizedCurrentBranch.lastIndexOf("/");
  const branchNamespace = slashIndex >= 0 ? normalizedCurrentBranch.slice(0, slashIndex + 1) : configuredPrefix;
  const branchLeafName = slashIndex >= 0 ? normalizedCurrentBranch.slice(slashIndex + 1) : normalizedCurrentBranch;
  const baseBranchName = `${branchNamespace}${branchLeafName}`;
  if (!existingBranches.has(baseBranchName)) {
    return baseBranchName;
  }
  for (let suffix = 2; ; suffix += 1) {
    const candidateBranchName = `${baseBranchName}${suffix}`;
    if (!existingBranches.has(candidateBranchName)) {
      return candidateBranchName;
    }
  }
}

// conversationSessionNormalizeBranchName normalizes Git ref labels before branch-name comparison.
// conversationSessionNormalizeBranchName 在分支名比较前归一化 Git 引用标签。
function conversationSessionNormalizeBranchName(name: string): string {
  return conversationSessionBranchDisplayName(name).trim();
}
