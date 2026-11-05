import * as React from "react";
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
import { toast } from "@/ui-kit/primitives/toast";
import {
  emptySessionGrant,
  MCP_CAPABILITY_LABELS,
  MCP_SESSION_PERMISSIONS,
  sameTarget,
  withPermission,
  withRead,
} from "./mcp-service-model";

// McpClientDialogProps edits one client, or creates one when `client` is absent.
// McpClientDialogProps 编辑一个客户端；`client` 缺失时新建一个。
export interface McpClientDialogProps {
  client?: McpServiceClientSummary;
  projects: ProjectSummary[];
  candidates: McpMediaTargetCandidate[];
  limits: McpServiceLimits;
  onSaved: (snapshot: McpServiceSnapshot) => void;
  onCreated: (issued: McpServiceIssuedToken, name: string) => void;
  onClose: () => void;
}

/** Client editor: a name, an on/off switch, session permissions with a project scope, and media models.
 * Every session write permission implies read access, so the toggles keep that rule visible.
 * 客户端编辑器：名称、开关、带项目范围的会话权限，以及媒体模型。每项会话写权限都隐含读取权限，开关会体现这一规则。 */
export function McpClientDialog(props: McpClientDialogProps) {
  const { t } = useI18n();
  const [name, setName] = React.useState(props.client?.name ?? "");
  const [enabled, setEnabled] = React.useState(props.client?.enabled ?? true);
  const [sessions, setSessions] = React.useState<McpSessionGrantEntry>(props.client?.sessions ?? emptySessionGrant());
  const [mediaTargets, setMediaTargets] = React.useState<McpMediaTargetEntry[]>(props.client?.mediaTargets ?? []);
  const [saving, setSaving] = React.useState(false);
  const trimmedName = name.trim();
  const nameFits = [...trimmedName].length <= props.limits.maxNameChars;
  const selectedProjects = sessions.projects.mode === "selected" ? sessions.projects.projectIds : [];
  const projectsValid = !sessions.read || sessions.projects.mode === "all" || selectedProjects.length > 0;
  const valid = trimmedName.length > 0 && nameFits && projectsValid
    && selectedProjects.length <= props.limits.maxProjects && mediaTargets.length <= props.limits.maxMediaTargets;
  // staleTargets are grants whose model is no longer available; they stay until the user removes them.
  // staleTargets 是其模型已不可用的授权；在用户移除前保留。
  const staleTargets = mediaTargets.filter((target) => !props.candidates.some((candidate) => sameTarget(candidate, target)));
  const toggleProject = (projectId: string, checked: boolean) => {
    const next = checked ? [...selectedProjects, projectId] : selectedProjects.filter((id) => id !== projectId);
    setSessions({ ...sessions, projects: { mode: "selected", projectIds: next } });
  };
  const toggleTarget = (target: McpMediaTargetEntry, checked: boolean) => {
    setMediaTargets(checked
      ? [...mediaTargets, { connectionId: target.connectionId, targetId: target.targetId }]
      : mediaTargets.filter((existing) => !sameTarget(existing, target)));
  };
  const save = async () => {
    setSaving(true);
    const request = { clientId: props.client?.id, name: trimmedName, enabled, sessions, mediaTargets };
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
          <section className="vk-mcp-block" data-mcp-session-grants="">
            <span className="vk-mcp-block-title">{t("mcpService.field.sessions")}</span>
            <label className="vk-mcp-check">
              <Checkbox checked={sessions.read} disabled={saving} onCheckedChange={(checked) => setSessions(withRead(sessions, checked === true))} />
              <span>{t("mcpService.grant.read")}</span>
            </label>
            {MCP_SESSION_PERMISSIONS.map((permission) => (
              <label className="vk-mcp-check" data-depends="" key={permission.key}>
                <Checkbox checked={sessions[permission.key]} disabled={saving}
                  onCheckedChange={(checked) => setSessions(withPermission(sessions, permission.key, checked === true))} />
                <span>{t(permission.label)}</span>
              </label>
            ))}
            <p className="vk-mcp-note">{t("mcpService.grant.readRequired")}</p>
          </section>
          {sessions.read ? (
            <section className="vk-mcp-block" data-mcp-project-scope="">
              <span className="vk-mcp-block-title">{t("mcpService.field.projects")}</span>
              <SegmentedControl aria-label={t("mcpService.field.projects")} disabled={saving} onValueChange={(value) => {
                setSessions({ ...sessions, projects: value === "all" ? { mode: "all" } : { mode: "selected", projectIds: selectedProjects } });
              }} value={sessions.projects.mode}>
                <SegmentedControlItem value="all">{t("mcpService.field.scopeAll")}</SegmentedControlItem>
                <SegmentedControlItem value="selected">{t("mcpService.field.scopeSelected")}</SegmentedControlItem>
              </SegmentedControl>
              {sessions.projects.mode === "selected" ? (
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
            </section>
          ) : null}
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
