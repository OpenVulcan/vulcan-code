import * as React from "react"
import {
  FolderKanban,
  FolderPlus,
  GitBranch,
  RefreshCw,
  Star,
  Trash2,
  UserPlus,
  Users,
} from "lucide-react"

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/ui-kit/primitives/alert-dialog"
import { Badge } from "@/ui-kit/primitives/badge"
import { Button } from "@/ui-kit/primitives/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/ui-kit/primitives/card"
import { Combobox } from "@/ui-kit/primitives/combobox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/ui-kit/primitives/empty"
import { Input } from "@/ui-kit/primitives/input"
import { Label } from "@/ui-kit/primitives/label"
import { useI18n } from "@/i18n"
import type {
  VmmMemoryActionResult,
  VmmMemorySnapshot,
  VmmProjectDisplaySummary,
  VmmUserDisplaySummary,
} from "@/types"

// VmmIdentityProjectPageProps defines catalog data and guarded management actions.
// VmmIdentityProjectPageProps 定义目录数据与受保护的管理操作。
interface VmmIdentityProjectPageProps {
  snapshot: VmmMemorySnapshot
  busy: boolean
  // Persisted global default VMM user id.
  // 已持久化的全局默认 VMM 用户标识。
  defaultUserId: string
  // Persisted global default VMM project id.
  // 已持久化的全局默认 VMM 项目标识。
  defaultProjectId: string
  // Page-owned operation feedback.
  // 当前页面持有的操作反馈。
  notice: string
  result: VmmMemoryActionResult | null
  onRunAction: (
    action: string,
    payload?: Record<string, unknown>,
  ) => Promise<VmmMemoryActionResult | null>
  // Persists one catalog user as the global default VMM identity.
  // 将一项目录用户持久化为全局默认 VMM 身份。
  onSetDefaultUser: (userId: string) => Promise<boolean>
  // Persists one catalog project as the global default VMM identity.
  // 将一项目录项目持久化为全局默认 VMM 身份。
  onSetDefaultProject: (projectId: string) => Promise<boolean>
}

// PendingDeletion identifies the exact visible entity being confirmed.
// PendingDeletion 标识正在确认的准确可见实体。
type PendingDeletion =
  | { kind: "user"; item: VmmUserDisplaySummary }
  | { kind: "project"; item: VmmProjectDisplaySummary }

// VmmIdentityProjectPage renders user and project management without exposing numeric ids.
// VmmIdentityProjectPage 渲染不暴露数字标识的用户与项目管理页面。
function VmmIdentityProjectPage({
  snapshot,
  busy,
  defaultUserId,
  defaultProjectId,
  notice,
  result,
  onRunAction,
  onSetDefaultUser,
  onSetDefaultProject,
}: VmmIdentityProjectPageProps) {
  const { t } = useI18n()
  const [createKind, setCreateKind] = React.useState<"user" | "project" | null>(null)
  // createTeamName and createSpaceName expose the complete VMM project hierarchy while preserving the historical default scope.
  // createTeamName 与 createSpaceName 展示完整的 VMM 项目层级，同时保留历史默认作用域。
  const [createTeamName, setCreateTeamName] = React.useState("default")
  const [createSpaceName, setCreateSpaceName] = React.useState("default")
  const [createName, setCreateName] = React.useState("")
  const [pendingDeletion, setPendingDeletion] = React.useState<PendingDeletion | null>(null)
  const [deletionText, setDeletionText] = React.useState("")
  const [userConfirmationCode, setUserConfirmationCode] = React.useState("")
  const [migrationProject, setMigrationProject] = React.useState<VmmProjectDisplaySummary | null>(null)
  const [migrationTarget, setMigrationTarget] = React.useState("")

  const catalog = snapshot.managementCatalog
  // migrationTargetOptions contains only existing destination projects accepted by VMM.
  // migrationTargetOptions 仅包含 VMM 接受的现有目标项目。
  const migrationTargetOptions = React.useMemo(() => catalog.projects
    .filter((project) => project.actionPath !== migrationProject?.actionPath)
    .map((project) => ({
      value: project.actionPath,
      label: project.displayName,
      description: project.displayPath,
      keywords: [project.displayPath],
    })), [catalog.projects, migrationProject?.actionPath])
  // operationMessage accepts only responses produced by this page so feedback from memory search
  // or profile operations cannot appear as an identity-management result after navigation.
  // operationMessage 只接受本页面产生的响应，避免导航后把记忆搜索或画像操作提示
  // 错误显示为身份管理结果。
  const operationResultMessage = result && [
    "users_projects",
    "resolve_user",
    "ensure_project",
    "delete_user",
    "delete_project",
    "migrate_project",
  ].includes(result.data.kind)
    ? result.summary
    : ""
  const operationMessage = notice || operationResultMessage || snapshot.managementCatalog.error

  // refreshCatalog requests the exact typed users/projects catalog.
  // refreshCatalog 请求准确类型化的用户与项目目录。
  const refreshCatalog = React.useCallback(async () => {
    await onRunAction("load_users_projects")
  }, [onRunAction])

  // submitCreation treats the named dialog action as the protocol's explicit creation confirmation.
  // submitCreation 将具名弹窗中的创建动作作为协议要求的明确创建确认。
  const submitCreation = React.useCallback(async () => {
    const normalizedName = createName.trim()
    if (!createKind || !normalizedName) {
      return
    }
    // creationResult proves the confirmed create call completed before the dialog is cleared.
    // creationResult 证明已确认的创建调用完成后才会清空弹窗。
    let creationResult: VmmMemoryActionResult | null
    if (createKind === "user") {
      creationResult = await onRunAction("resolve_user", {
        userRef: normalizedName,
        confirmCreate: true,
      })
    } else {
      const projectPath = canonicalProjectPath(createTeamName, createSpaceName, normalizedName)
      creationResult = await onRunAction("ensure_project", {
        projectPath,
        confirmCreate: true,
      })
    }
    if (!creationResult) {
      return
    }
    setCreateKind(null)
    setCreateTeamName("default")
    setCreateSpaceName("default")
    setCreateName("")
    await refreshCatalog()
  }, [createKind, createName, createSpaceName, createTeamName, onRunAction, refreshCatalog])

  // confirmDeletion performs the required hidden preflight before the destructive call.
  // confirmDeletion 在破坏性调用前执行所需的隐藏预检。
  const confirmDeletion = React.useCallback(async () => {
    if (!pendingDeletion || deletionText !== pendingDeletion.item.displayName) {
      return
    }
    if (pendingDeletion.kind === "project") {
      const preview = await onRunAction("delete_project", {
        projectPath: pendingDeletion.item.actionPath,
        confirmDelete: false,
      })
      if (!preview) {
        return
      }
      const deleted = await onRunAction("delete_project", {
        projectPath: pendingDeletion.item.actionPath,
        confirmDelete: true,
      })
      if (!deleted) {
        return
      }
    } else {
      const preview = await onRunAction("delete_user", {
        userRef: pendingDeletion.item.displayName,
        confirmationCode: "",
      })
      const confirmationCode = vmmDeleteUserConfirmationCode(preview)
      if (!confirmationCode) {
        return
      }
      setUserConfirmationCode(confirmationCode)
      const deleted = await onRunAction("delete_user", {
        userRef: pendingDeletion.item.displayName,
        confirmationCode,
      })
      if (!deleted) {
        return
      }
    }
    setPendingDeletion(null)
    setDeletionText("")
    setUserConfirmationCode("")
    await refreshCatalog()
  }, [deletionText, onRunAction, pendingDeletion, refreshCatalog])

  // submitMigration preflights and confirms one migration between existing canonical projects.
  // submitMigration 在两个现有规范项目之间预检并确认一次迁移。
  const submitMigration = React.useCallback(async () => {
    if (!migrationProject || !migrationTarget) {
      return
    }
    const preview = await onRunAction("migrate_project", {
      sourceProjectPath: migrationProject.actionPath,
      targetProjectPath: migrationTarget,
      confirmMigrate: false,
    })
    if (!preview) {
      return
    }
    const migrated = await onRunAction("migrate_project", {
      sourceProjectPath: migrationProject.actionPath,
      targetProjectPath: migrationTarget,
      confirmMigrate: true,
    })
    if (!migrated) {
      return
    }
    setMigrationProject(null)
    setMigrationTarget("")
    await refreshCatalog()
  }, [migrationProject, migrationTarget, onRunAction, refreshCatalog])

  return (
    <div className="flex w-full min-w-0 flex-col gap-4">
      <div className="vmm-page-toolbar">
        <Button disabled={busy} onClick={() => void refreshCatalog()} type="button" variant="outline">
          <RefreshCw className="size-4" aria-hidden="true" />
          {t("memory.identity.refresh")}
        </Button>
      </div>

      {operationMessage ? (
        <p className="rounded-lg border bg-muted/30 px-3 py-2 text-sm text-muted-foreground">
          {operationMessage}
        </p>
      ) : null}

      <div className="grid gap-4 lg:grid-cols-2">
        <CatalogCard
          actionIcon={<UserPlus className="size-4" aria-hidden="true" />}
          actionLabel={t("memory.identity.addUser")}
          emptyLabel={t("memory.identity.emptyUsers")}
          icon={<Users className="size-4" aria-hidden="true" />}
          onAdd={() => {
            setCreateName("")
            setCreateKind("user")
          }}
          title={t("memory.identity.users")}
        >
          {catalog.users.map((user) => (
            <CatalogRow
              busy={busy}
              defaultItem={user.stableKey === defaultUserId}
              key={user.stableKey}
              protectedItem={user.protected}
              title={user.displayName}
              onDelete={user.protected || user.stableKey === defaultUserId ? undefined : () => {
                setDeletionText("")
                setPendingDeletion({ kind: "user", item: user })
              }}
              onSetDefault={user.stableKey === defaultUserId
                ? undefined
                : () => void onSetDefaultUser(user.stableKey)}
            />
          ))}
        </CatalogCard>

        <CatalogCard
          actionIcon={<FolderPlus className="size-4" aria-hidden="true" />}
          actionLabel={t("memory.identity.addProject")}
          emptyLabel={t("memory.identity.emptyProjects")}
          icon={<FolderKanban className="size-4" aria-hidden="true" />}
          onAdd={() => {
            setCreateTeamName("default")
            setCreateSpaceName("default")
            setCreateName("")
            setCreateKind("project")
          }}
          title={t("memory.identity.projects")}
        >
          {catalog.projects.map((project) => (
            <CatalogRow
              busy={busy}
              defaultItem={project.stableKey === defaultProjectId}
              detail={project.displayPath}
              key={project.stableKey}
              protectedItem={project.protected}
              title={project.displayName}
              onDelete={project.protected || project.stableKey === defaultProjectId ? undefined : () => {
                setDeletionText("")
                setPendingDeletion({ kind: "project", item: project })
              }}
              onMigrate={project.protected ? undefined : () => {
                setMigrationTarget("")
                setMigrationProject(project)
              }}
              onSetDefault={project.stableKey === defaultProjectId
                ? undefined
                : () => void onSetDefaultProject(project.stableKey)}
            />
          ))}
        </CatalogCard>
      </div>

      <Dialog
        open={createKind !== null}
        onOpenChange={(open) => {
          if (!open && !busy) {
            setCreateKind(null)
            setCreateTeamName("default")
            setCreateSpaceName("default")
            setCreateName("")
          }
        }}
      >
        <DialogContent className={createKind === "project" ? "sm:max-w-xl" : undefined} showCloseButton={!busy}
          onEscapeKeyDown={(event) => { if (busy) event.preventDefault() }}
          onInteractOutside={(event) => { if (busy) event.preventDefault() }}>
          <DialogHeader>
            <DialogTitle>
              {createKind === "user" ? t("memory.identity.addUser") : t("memory.identity.addProject")}
            </DialogTitle>
            <DialogDescription>{t("memory.identity.description")}</DialogDescription>
          </DialogHeader>
          {createKind === "project" ? (
            <div className="space-y-4">
              <p className="text-sm text-muted-foreground">
                {t("memory.identity.projectPathStructure")}
              </p>
              <div className="grid gap-3 sm:grid-cols-3">
                <div className="space-y-2">
                  <Label htmlFor="vmm-create-team">{t("memory.identity.teamName")}</Label>
                  <Input
                    autoFocus
                    disabled={busy}
                    id="vmm-create-team"
                    onChange={(event) => setCreateTeamName(event.target.value)}
                    value={createTeamName}
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="vmm-create-space">{t("memory.identity.spaceName")}</Label>
                  <Input
                    disabled={busy}
                    id="vmm-create-space"
                    onChange={(event) => setCreateSpaceName(event.target.value)}
                    value={createSpaceName}
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="vmm-create-project">{t("memory.identity.projectName")}</Label>
                  <Input
                    disabled={busy}
                    id="vmm-create-project"
                    onChange={(event) => setCreateName(event.target.value)}
                    placeholder={t("memory.identity.projectPlaceholder")}
                    value={createName}
                  />
                </div>
              </div>
              <div className="rounded-lg border bg-muted/30 px-3 py-2 text-sm">
                <span className="text-muted-foreground">{t("memory.identity.projectPathPreview")}</span>
                <code className="ml-2 break-all">
                  {canonicalProjectPath(
                    createTeamName.trim() || "Team",
                    createSpaceName.trim() || "Space",
                    createName.trim() || "Project",
                  )}
                </code>
              </div>
            </div>
          ) : (
            <div className="space-y-2">
              <Label htmlFor="vmm-create-name">{t("memory.identity.userName")}</Label>
              <Input
                autoFocus
                disabled={busy}
                id="vmm-create-name"
                onChange={(event) => setCreateName(event.target.value)}
                value={createName}
              />
            </div>
          )}
          {operationMessage ? <p role="status" className="text-sm text-muted-foreground">{operationMessage}</p> : null}
          <DialogFooter>
            <Button disabled={busy} onClick={() => setCreateKind(null)} type="button" variant="outline">
              {t("memory.common.cancel")}
            </Button>
            <Button
              disabled={busy
                || !createName.trim()
                || (createKind === "project" && (!createTeamName.trim() || !createSpaceName.trim()))}
              onClick={() => void submitCreation()}
              type="button"
            >
              {busy ? t("memory.common.loading") : t("memory.identity.create")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <AlertDialog
        open={pendingDeletion !== null}
        onOpenChange={(open) => {
          if (!open && !busy) {
            setPendingDeletion(null)
            setDeletionText("")
            setUserConfirmationCode("")
          }
        }}
      >
        <AlertDialogContent onEscapeKeyDown={(event) => { if (busy) event.preventDefault() }}>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("memory.identity.confirmDeleteTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("memory.identity.confirmDeleteDescription")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <div className="space-y-2">
            <Label htmlFor="vmm-delete-confirm">
              {t("memory.identity.typeName").replace(
                "{name}",
                pendingDeletion?.item.displayName ?? "",
              )}
            </Label>
            <Input
              autoFocus
              disabled={busy}
              id="vmm-delete-confirm"
              onChange={(event) => setDeletionText(event.target.value)}
              value={deletionText}
            />
            {userConfirmationCode ? (
              <p className="text-xs text-muted-foreground">{t("memory.identity.checking")}</p>
            ) : null}
          </div>
          {operationMessage ? <p role="status" className="text-sm text-muted-foreground">{operationMessage}</p> : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>{t("memory.common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              disabled={
                busy
                || !pendingDeletion
                || deletionText !== pendingDeletion.item.displayName
              }
              onClick={(event) => {
                event.preventDefault()
                void confirmDeletion()
              }}
              variant="destructive"
            >
              <Trash2 className="size-4" aria-hidden="true" />
              {busy ? t("memory.identity.checking") : t("memory.identity.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <Dialog
        open={migrationProject !== null}
        onOpenChange={(open) => {
          if (!open && !busy) {
            setMigrationProject(null)
            setMigrationTarget("")
          }
        }}
      >
        <DialogContent showCloseButton={!busy}
          onEscapeKeyDown={(event) => { if (busy) event.preventDefault() }}
          onInteractOutside={(event) => { if (busy) event.preventDefault() }}>
          <DialogHeader>
            <DialogTitle>{t("memory.identity.migrateTitle")}</DialogTitle>
            <DialogDescription>{t("memory.identity.migrateDescription")}</DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label>{t("memory.identity.targetProjectName")}</Label>
            <Combobox
              ariaLabel={t("memory.identity.targetProjectName")}
              disabled={busy}
              emptyText={t("memory.rules.noProject")}
              onValueChange={setMigrationTarget}
              options={migrationTargetOptions}
              placeholder={t("memory.identity.targetProjectName")}
              searchPlaceholder={t("memory.rules.searchProject")}
              value={migrationTarget}
            />
          </div>
          {operationMessage ? <p role="status" className="text-sm text-muted-foreground">{operationMessage}</p> : null}
          <DialogFooter>
            <Button disabled={busy} onClick={() => setMigrationProject(null)} type="button" variant="outline">
              {t("memory.common.cancel")}
            </Button>
            <Button disabled={busy || !migrationTarget} onClick={() => void submitMigration()} type="button">
              <GitBranch className="size-4" aria-hidden="true" />
              {busy ? t("memory.common.loading") : t("memory.identity.migrate")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

// CatalogCard renders one catalog group with a stable empty state and add action.
// CatalogCard 渲染带稳定空状态与添加操作的单个目录分组。
function CatalogCard({
  actionIcon,
  actionLabel,
  children,
  emptyLabel,
  icon,
  onAdd,
  title,
}: {
  actionIcon: React.ReactNode
  actionLabel: string
  children: React.ReactNode
  emptyLabel: string
  icon: React.ReactNode
  onAdd: () => void
  title: string
}) {
  const rows = React.Children.toArray(children)
  return (
    <Card>
      <CardHeader className="flex-row items-start justify-between gap-3">
        <div className="space-y-1">
          <CardTitle className="flex items-center gap-2">
            {icon}
            {title}
          </CardTitle>
          <CardDescription>{rows.length}</CardDescription>
        </div>
        <Button onClick={onAdd} size="sm" type="button" variant="outline">
          {actionIcon}
          {actionLabel}
        </Button>
      </CardHeader>
      <CardContent>
        {rows.length === 0 ? (
          <Empty className="min-h-40" variant="outlined">
            <EmptyHeader>
              <EmptyMedia variant="icon">{icon}</EmptyMedia>
              <EmptyTitle>{title}</EmptyTitle>
              <EmptyDescription>{emptyLabel}</EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <div className="divide-y rounded-lg border">{rows}</div>
        )}
      </CardContent>
    </Card>
  )
}

// CatalogRow renders one human-readable user or project entry.
// CatalogRow 渲染单个人类可读的用户或项目条目。
function CatalogRow({
  busy,
  defaultItem,
  detail,
  onDelete,
  onMigrate,
  onSetDefault,
  protectedItem,
  title,
}: {
  busy: boolean
  defaultItem: boolean
  detail?: string
  onDelete?: () => void
  onMigrate?: () => void
  onSetDefault?: () => void
  protectedItem: boolean
  title: string
}) {
  const { t } = useI18n()
  return (
    <div className="flex min-w-0 items-center gap-3 px-3 py-2.5">
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <strong className="truncate text-sm">{title}</strong>
          {defaultItem ? <Badge>{t("memory.identity.default")}</Badge> : null}
          {protectedItem ? <Badge variant="secondary">{t("memory.identity.protected")}</Badge> : null}
        </div>
        {detail ? <p className="truncate text-xs text-muted-foreground">{detail}</p> : null}
      </div>
      {onSetDefault ? (
        <Button
          aria-label={t("memory.identity.setDefault")}
          disabled={busy}
          onClick={onSetDefault}
          size="icon-sm"
          title={t("memory.identity.setDefault")}
          type="button"
          variant="ghost"
        >
          <Star className="size-4" />
        </Button>
      ) : null}
      {onMigrate ? (
        <Button aria-label={t("memory.identity.migrate")} disabled={busy} onClick={onMigrate} size="icon-sm" type="button" variant="ghost">
          <GitBranch className="size-4" />
        </Button>
      ) : null}
      {onDelete ? (
        <Button aria-label={t("memory.identity.delete")} disabled={busy} onClick={onDelete} size="icon-sm" type="button" variant="ghost">
          <Trash2 className="size-4 text-destructive" />
        </Button>
      ) : null}
    </div>
  )
}

// canonicalProjectPath joins the explicit Team, Space, and Project segments required by VMM.
// canonicalProjectPath 拼接 VMM 要求的显式 Team、Space 与 Project 路径段。
// Parameters are the visible Team, Space, and Project names; the return value is one canonical three-segment path.
// 参数是可见的 Team、Space 与 Project 名称；返回值是规范的三级路径。
export function canonicalProjectPath(teamName: string, spaceName: string, projectName: string): string {
  // normalizeSegment prevents one field from silently injecting extra hierarchy levels.
  // normalizeSegment 防止单个输入字段静默注入额外层级。
  const normalizeSegment = (value: string) => value.trim().replace(/[\\/]+/g, "-")
  return [teamName, spaceName, projectName].map(normalizeSegment).join("/")
}

// vmmDeleteUserConfirmationCode reads the exact DeleteUserResponse field.
// vmmDeleteUserConfirmationCode 读取准确的 DeleteUserResponse 字段。
function vmmDeleteUserConfirmationCode(result: VmmMemoryActionResult | null): string {
  if (result?.data.kind !== "delete_user") {
    return ""
  }
  return result.data.value.confirmation_code
}

export { VmmIdentityProjectPage }
export type { VmmIdentityProjectPageProps }
