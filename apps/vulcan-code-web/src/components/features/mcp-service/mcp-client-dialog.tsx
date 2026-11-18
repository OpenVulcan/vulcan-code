import * as React from "react";
import { PageGuide } from "@/components/page";
import { errorMessage } from "@/lib/display-format";
import { formatMessage, useI18n } from "@/i18n";
import { createMcpClient, updateMcpClient } from "@/service/mcp-service-service";
import type {
  McpMediaTargetCandidate,
  McpMediaTargetEntry,
  McpServiceClientSummary,
  McpServiceIssuedToken,
  McpServiceLimits,
  McpServiceSnapshot,
  McpSessionGrantEntry,
  ProjectSummary,
} from "@/types";
import { Button } from "@/ui-kit/primitives/button";
import { Checkbox } from "@/ui-kit/primitives/checkbox";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { Input } from "@/ui-kit/primitives/input";
import { Switch } from "@/ui-kit/primitives/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/ui-kit/primitives/tabs";
import { toast } from "@/ui-kit/primitives/toast";
import {
  hasMcpSessionTools,
  MCP_CAPABILITY_LABELS,
  sameTarget,
  updateMcpToolGrants,
} from "./mcp-service-model";
import { McpToolGrants } from "./mcp-tool-grants";

// McpClientDialogProps edits one client, or creates one when `client` is absent.
// McpClientDialogProps 编辑一个客户端；`client` 缺失时新建一个。
export interface McpClientDialogProps {
  client?: McpServiceClientSummary;
  projects: ProjectSummary[];
  candidates: McpMediaTargetCandidate[];
  // Grantable native names and knowledge library labels.
  // 可授权的原生名称和知识库标签。
  toolCandidates: string[];
  knowledgeCandidates: Record<string, string>;
  limits: McpServiceLimits;
  onSaved: (snapshot: McpServiceSnapshot) => void;
  onCreated: (issued: McpServiceIssuedToken, name: string) => void;
  onClose: () => void;
}

/** Edits the client and each granted tool, project, knowledge library, and media target.
 * 编辑客户端及逐项授权的工具、项目、知识库和媒体目标。 */
export function McpClientDialog(props: McpClientDialogProps) {
  const { t } = useI18n();
  const [name, setName] = React.useState(props.client?.name ?? "");
  const [enabled, setEnabled] = React.useState(props.client?.enabled ?? true);
  const [sessionProjects, setSessionProjects] = React.useState<McpSessionGrantEntry["projects"]>(props.client?.sessions.projects ?? { mode: "all" });
  const [mediaTargets, setMediaTargets] = React.useState<McpMediaTargetEntry[]>(props.client?.mediaTargets ?? []);
  // New clients start without individually granted tools.
  // 新客户端初始没有独立勾选的工具。
  const [toolIds, setToolIds] = React.useState(props.client?.toolIds ?? []);
  const [toolProjectIds, setToolProjectIds] = React.useState(props.client?.toolProjectIds ?? []);
  // Keeps the draft selection when temporarily switching the scope in the editor.
  // 在编辑器中临时切换范围时保留指定项目草稿。
  const [restrictToolProjects, setRestrictToolProjects] = React.useState(toolProjectIds.length > 0);
  const [knowledgeLibraries, setKnowledgeLibraries] = React.useState(props.client?.knowledgeLibraries ?? {});
  const knowledgeLabels = { ...Object.fromEntries(Object.keys(knowledgeLibraries).map((id) => [id, id])), ...props.knowledgeCandidates };
  const [saving, setSaving] = React.useState(false);
  const trimmedName = name.trim();
  const nameFits = [...trimmedName].length <= props.limits.maxNameChars;
  // The draft tool selection controls whether the session project scope needs validation.
  // 由草稿中选中的会话工具决定是否需要校验会话项目范围。
  const hasSessionTools = hasMcpSessionTools(toolIds);
  const selectedProjects = sessionProjects.mode === "selected" ? sessionProjects.projectIds : [];
  const projectsValid = !hasSessionTools || sessionProjects.mode === "all" || selectedProjects.length > 0;
  const toolProjectsValid = !restrictToolProjects || (toolProjectIds.length > 0 && toolProjectIds.length <= props.limits.maxProjects);
  const valid = trimmedName.length > 0 && nameFits && projectsValid && toolProjectsValid
    && selectedProjects.length <= props.limits.maxProjects && mediaTargets.length <= props.limits.maxMediaTargets;
  // staleTargets are grants whose model is no longer available; they stay until the user removes them.
  // staleTargets 是其模型已不可用的授权；在用户移除前保留。
  const staleTargets = mediaTargets.filter((target) => !props.candidates.some((candidate) => sameTarget(candidate, target)));
  const toggleProject = (projectId: string, checked: boolean) => {
    const next = checked ? [...selectedProjects, projectId] : selectedProjects.filter((id) => id !== projectId);
    setSessionProjects({ mode: "selected", projectIds: next });
  };
  // Updates a single tool or a whole group without affecting the other authorization pages.
  // 更新单项工具或整组工具，保留其他授权页面的选择。
  const changeTools = (tools: readonly string[], checked: boolean) => {
    setToolIds((ids) => updateMcpToolGrants(ids, tools, checked));
  };
  const toggleTarget = (target: McpMediaTargetEntry, checked: boolean) => {
    setMediaTargets(checked
      ? [...mediaTargets, { connectionId: target.connectionId, targetId: target.targetId }]
      : mediaTargets.filter((existing) => !sameTarget(existing, target)));
  };
  const save = async () => {
    setSaving(true);
    // Compatibility fields reflect the draft without implicitly selecting any additional tools.
    // 兼容字段反映草稿状态，不会自动勾选额外工具。
    const sessions = {
      projects: sessionProjects,
      read: hasSessionTools,
      send: toolIds.includes("send_message"),
      create: toolIds.includes("create_session"),
      stop: toolIds.includes("stop_session"),
      answerQuestions: toolIds.includes("answer_question"),
    };
    const request = {
      clientId: props.client?.id, name: trimmedName, enabled, sessions, mediaTargets, toolIds,
      toolProjectIds: restrictToolProjects ? toolProjectIds : [], knowledgeLibraries,
    };
    try {
      if (props.client) {
        props.onSaved(await updateMcpClient(request));
        toast.success(t("mcpService.saved"));
      } else {
        props.onCreated(await createMcpClient(request), trimmedName);
      }
    } catch (error) {
      toast.error(t("mcpService.saveFailed"), { description: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  };
  const capabilityGroups = groupByCapability(props.candidates);
  return (
    <Dialog open onOpenChange={(open) => { if (!open && !saving) props.onClose(); }}>
      <DialogContent data-mcp-client-dialog="" scrollMode="fixed-footer" size="lg">
        <DialogHeader>
          <DialogTitle>{props.client ? t("mcpService.dialog.editTitle") : t("mcpService.dialog.createTitle")}</DialogTitle>
          <DialogDescription>{t("mcpService.dialog.detail")}</DialogDescription>
        </DialogHeader>
        <div className="vk-mcp-form vk-dialog-body">
          <section className="vk-mcp-block">
            <label className="vk-mcp-block-title" htmlFor="mcp-client-name">{t("mcpService.field.name")}</label>
            <Input aria-invalid={!nameFits} disabled={saving} id="mcp-client-name" maxLength={props.limits.maxNameChars * 2}
              onChange={(event) => setName(event.target.value)} placeholder={t("mcpService.field.namePlaceholder")} value={name} />
            {!nameFits ? <p className="vk-field-error" role="alert">{formatMessage(t("mcpService.field.nameTooLong"), { max: props.limits.maxNameChars })}</p> : null}
            <div className="vk-mcp-switch-row">
              <span>
                <strong>{t("mcpService.field.enabled")}</strong>
                <small>{t("mcpService.field.enabledDetail")}</small>
              </span>
              <Switch aria-label={t("mcpService.field.enabled")} checked={enabled} disabled={saving} onCheckedChange={setEnabled} />
            </div>
          </section>
          <Tabs className="vk-mcp-tabs" defaultValue="tools">
            <TabsList aria-label={t("mcpService.tabs.label")} variant="line">
              <TabsTrigger value="sessions">{t("mcpService.field.sessions")}</TabsTrigger>
              <TabsTrigger value="tools">{t("mcpService.field.tools")}</TabsTrigger>
              <TabsTrigger value="media">{t("mcpService.tabs.media")}</TabsTrigger>
            </TabsList>
            <TabsContent className="vk-mcp-tab" value="sessions">
              <PageGuide items={[
                { term: t("mcpService.guide.what"), detail: t("mcpService.guide.whatDetail") },
                { term: t("mcpService.guide.approvals"), detail: t("mcpService.guide.approvalsDetail") },
                { term: t("mcpService.guide.view"), detail: t("mcpService.guide.viewDetail") },
              ]} />
              <McpToolGrants candidates={props.toolCandidates} disabled={saving} onChange={changeTools} page="sessions" selected={toolIds} />
              {hasSessionTools ? <section className="vk-mcp-block" data-mcp-project-scope="">
                <span className="vk-mcp-block-title">{t("mcpService.field.sessionProjects")}</span>
                <SegmentedControl aria-label={t("mcpService.field.sessionProjects")} disabled={saving} onValueChange={(value) => {
                  setSessionProjects(value === "all" ? { mode: "all" } : { mode: "selected", projectIds: selectedProjects });
                }} value={sessionProjects.mode}>
                  <SegmentedControlItem value="all">{t("mcpService.field.scopeAll")}</SegmentedControlItem>
                  <SegmentedControlItem value="selected">{t("mcpService.field.scopeSelected")}</SegmentedControlItem>
                </SegmentedControl>
                {sessionProjects.mode === "selected" ? (
                  props.projects.length === 0 ? <p className="vk-mcp-note">{t("mcpService.field.noProjects")}</p> : (
                    <div className="vk-mcp-checklist">
                      {props.projects.map((project) => (
                        <label className="vk-mcp-check" key={project.id}>
                          <Checkbox checked={selectedProjects.includes(project.id)} disabled={saving}
                            onCheckedChange={(checked) => toggleProject(project.id, checked === true)} />
                          <span>{project.name}<small>{project.path}</small></span>
                        </label>
                      ))}
                    </div>
                  )
                ) : null}
                {!projectsValid ? <p className="vk-field-error" role="alert">{t("mcpService.field.projectsRequired")}</p> : null}
              </section> : null}
            </TabsContent>
            <TabsContent className="vk-mcp-tab" value="tools" data-mcp-tool-grants="">
              <section className="vk-mcp-block" data-mcp-workspace-grants="">
                <span className="vk-mcp-block-title">{t("mcpService.field.toolProjects")}</span>
                <SegmentedControl aria-label={t("mcpService.field.toolProjects")} disabled={saving}
                  onValueChange={(value) => setRestrictToolProjects(value === "selected")} value={restrictToolProjects ? "selected" : "unrestricted"}>
                  <SegmentedControlItem value="unrestricted">{t("mcpService.field.scopeUnrestricted")}</SegmentedControlItem>
                  <SegmentedControlItem value="selected">{t("mcpService.field.scopeSelected")}</SegmentedControlItem>
                </SegmentedControl>
                {restrictToolProjects ? (
                  props.projects.length === 0 ? <p className="vk-mcp-note">{t("mcpService.field.noProjects")}</p> : (
                    <div className="vk-mcp-checklist">
                      {props.projects.map((project) => (
                        <label className="vk-mcp-check" key={project.id}>
                          <Checkbox checked={toolProjectIds.includes(project.id)} disabled={saving}
                            onCheckedChange={(checked) => setToolProjectIds((ids) => checked === true ? [...ids, project.id] : ids.filter((id) => id !== project.id))} />
                          <span>{project.name}<small>{project.path}</small></span>
                        </label>
                      ))}
                    </div>
                  )
                ) : null}
                {restrictToolProjects && toolProjectIds.length === 0 ? <p className="vk-field-error" role="alert">{t("mcpService.field.projectsRequired")}</p> : null}
              </section>
              <McpToolGrants candidates={props.toolCandidates} disabled={saving} onChange={changeTools} page="tools" selected={toolIds} />
              <section className="vk-mcp-block" data-mcp-knowledge-grants="">
                <span className="vk-mcp-block-title">{t("mcpService.field.knowledgeLibraries")}</span>
                {Object.entries(knowledgeLabels).map(([id, label]) => (
                  <div className="vk-mcp-checklist" key={id}>
                    <span>{label}</span>
                    <label className="vk-mcp-check">
                      <Checkbox checked={Object.hasOwn(knowledgeLibraries, id)} disabled={saving}
                        onCheckedChange={(checked) => setKnowledgeLibraries((entries) => {
                          if (checked === true) return { ...entries, [id]: false };
                          const next = { ...entries };
                          delete next[id];
                          return next;
                        })} />
                      <span>{t("mcpService.field.knowledgeRead")}</span>
                    </label>
                    <label className="vk-mcp-check">
                      <Checkbox checked={knowledgeLibraries[id] === true} disabled={saving || !Object.hasOwn(knowledgeLibraries, id)}
                        onCheckedChange={(checked) => setKnowledgeLibraries((entries) => ({ ...entries, [id]: checked === true }))} />
                      <span>{t("mcpService.field.knowledgeWrite")}</span>
                    </label>
                  </div>
                ))}
              </section>
            </TabsContent>
            <TabsContent className="vk-mcp-tab" value="media">
              <McpToolGrants candidates={props.toolCandidates} disabled={saving} onChange={changeTools} page="media" selected={toolIds} />
              <section className="vk-mcp-block" data-mcp-media-grants="">
                <span className="vk-mcp-block-title">{t("mcpService.field.media")}</span>
                <p className="vk-mcp-note">{t("mcpService.field.mediaDetail")}</p>
                {props.candidates.length === 0 && staleTargets.length === 0 ? <p className="vk-mcp-note">{t("mcpService.field.mediaEmpty")}</p> : null}
                {capabilityGroups.map(([capability, candidates]) => (
                  <div className="vk-mcp-checklist" key={capability}>
                    <span className="vk-label">{MCP_CAPABILITY_LABELS[capability] ? t(MCP_CAPABILITY_LABELS[capability]) : capability}</span>
                    {candidates.map((candidate) => (
                      <label className="vk-mcp-check" key={`${candidate.connectionId}/${candidate.targetId}`}>
                        <Checkbox checked={mediaTargets.some((target) => sameTarget(target, candidate))} disabled={saving}
                          onCheckedChange={(checked) => toggleTarget(candidate, checked === true)} />
                        <span>{candidate.displayName}<small>{candidate.model} · {candidate.accountName}</small></span>
                      </label>
                    ))}
                  </div>
                ))}
                {staleTargets.length > 0 ? (
                  <div className="vk-mcp-checklist" data-mcp-stale-targets="">
                    <span className="vk-label">{t("mcpService.field.mediaStale")}</span>
                    {staleTargets.map((target) => (
                      <label className="vk-mcp-check" key={`${target.connectionId}/${target.targetId}`}>
                        <Checkbox checked disabled={saving} onCheckedChange={() => toggleTarget(target, false)} />
                        <span>{target.targetId}<small>{target.connectionId}</small></span>
                      </label>
                    ))}
                  </div>
                ) : null}
              </section>
            </TabsContent>
          </Tabs>
        </div>
        <DialogFooter>
          <Button disabled={saving} onClick={props.onClose} size="sm" type="button" variant="outline">{t("mcpService.cancel")}</Button>
          <Button data-mcp-client-save="" disabled={!valid || saving} onClick={() => void save()} size="sm" type="button">
            {props.client ? t("mcpService.save") : t("mcpService.createAndShowToken")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// groupByCapability keeps the candidates' order while grouping them under their capability.
// groupByCapability 在保持候选顺序的同时按能力分组。
function groupByCapability(candidates: McpMediaTargetCandidate[]): Array<[string, McpMediaTargetCandidate[]]> {
  const groups = new Map<string, McpMediaTargetCandidate[]>();
  for (const capability of Object.keys(MCP_CAPABILITY_LABELS)) groups.set(capability, []);
  for (const candidate of candidates) {
    const group = groups.get(candidate.capability) ?? [];
    group.push(candidate);
    groups.set(candidate.capability, group);
  }
  return [...groups.entries()].filter(([, group]) => group.length > 0);
}
