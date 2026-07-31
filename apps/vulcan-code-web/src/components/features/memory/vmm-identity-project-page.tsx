import * as React from "react"
import {
  FolderKanban,
  FolderPlus,
  GitBranch,
  RefreshCw,
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
} from "@/components/ui/alert-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Combobox } from "@/components/ui/combobox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
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
  // Page-owned operation feedback.
  // 当前页面持有的操作反馈。
  notice: string
  result: VmmMemoryActionResult | null
  onRunAction: (
    action: string,
    payload?: Record<string, unknown>,
  ) => Promise<VmmMemoryActionResult | null>
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
  notice,
  result,
  onRunAction,
}: VmmIdentityProjectPageProps) {
  const { t } = useI18n()
  const [createKind, setCreateKind] = React.useState<"user" | "project" | null>(null)
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
      const projectPath = canonicalProjectPath(normalizedName)
      creationResult = await onRunAction("ensure_project", {
        projectPath,
        confirmCreate: true,
      })
    }
    if (!creationResult) {
      return
    }
    setCreateKind(null)
    setCreateName("")
    await refreshCatalog()
  }, [createKind, createName, onRunAction, refreshCatalog])

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
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{t("memory.identity.title")}</h2>
          <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
            {t("memory.identity.description")}
          </p>
        </div>
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
          onAdd={() => setCreateKind("user")}
          title={t("memory.identity.users")}
        >
          {catalog.users.map((user) => (
            <CatalogRow
              key={user.stableKey}
              protectedItem={user.protected}
              title={user.displayName}
              onDelete={user.protected ? undefined : () => {
                setDeletionText("")
                setPendingDeletion({ kind: "user", item: user })
              }}
            />
          ))}
        </CatalogCard>

        <CatalogCard
          actionIcon={<FolderPlus className="size-4" aria-hidden="true" />}
          actionLabel={t("memory.identity.addProject")}
          emptyLabel={t("memory.identity.emptyProjects")}
          icon={<FolderKanban className="size-4" aria-hidden="true" />}
          onAdd={() => setCreateKind("project")}
          title={t("memory.identity.projects")}
        >
          {catalog.projects.map((project) => (
            <CatalogRow
              detail={project.protected ? undefined : project.displayPath}
              key={project.stableKey}
              protectedItem={project.protected}
              title={project.displayName}
              onDelete={project.protected ? undefined : () => {
                setDeletionText("")
                setPendingDeletion({ kind: "project", item: project })
              }}
              onMigrate={project.protected ? undefined : () => {
                setMigrationTarget("")
                setMigrationProject(project)
              }}
            />
          ))}
        </CatalogCard>
      </div>

      <Dialog
        open={createKind !== null}
        onOpenChange={(open) => {
          if (!open) {
            setCreateKind(null)
            setCreateName("")
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {createKind === "user" ? t("memory.identity.addUser") : t("memory.identity.addProject")}
            </DialogTitle>
            <DialogDescription>{t("memory.identity.description")}</DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor="vmm-create-name">
              {createKind === "user" ? t("memory.identity.userName") : t("memory.identity.projectName")}
            </Label>
            <Input
              autoFocus
              disabled={busy}
              id="vmm-create-name"
              onChange={(event) => setCreateName(event.target.value)}
              value={createName}
            />
          </div>
          <DialogFooter>
            <Button onClick={() => setCreateKind(null)} type="button" variant="outline">
              {t("memory.common.cancel")}
            </Button>
            <Button disabled={busy || !createName.trim()} onClick={() => void submitCreation()} type="button">
              {busy ? t("memory.common.loading") : t("memory.identity.create")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <AlertDialog
        open={pendingDeletion !== null}
        onOpenChange={(open) => {
          if (!open) {
            setPendingDeletion(null)
            setDeletionText("")
            setUserConfirmationCode("")
          }
        }}
      >
        <AlertDialogContent>
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
          <AlertDialogFooter>
            <AlertDialogCancel>{t("memory.common.cancel")}</AlertDialogCancel>
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
          if (!open) {
            setMigrationProject(null)
            setMigrationTarget("")
          }
        }}
      >
        <DialogContent>
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
          <DialogFooter>
            <Button onClick={() => setMigrationProject(null)} type="button" variant="outline">
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
          <Empty className="min-h-40 border">
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
  detail,
  onDelete,
  onMigrate,
  protectedItem,
  title,
}: {
  detail?: string
  onDelete?: () => void
  onMigrate?: () => void
  protectedItem: boolean
  title: string
}) {
  const { t } = useI18n()
  return (
    <div className="flex min-w-0 items-center gap-3 px-3 py-2.5">
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <strong className="truncate text-sm">{title}</strong>
          {protectedItem ? <Badge variant="secondary">{t("memory.identity.protected")}</Badge> : null}
        </div>
        {detail ? <p className="truncate text-xs text-muted-foreground">{detail}</p> : null}
      </div>
      {onMigrate ? (
        <Button aria-label={t("memory.identity.migrate")} onClick={onMigrate} size="icon-sm" type="button" variant="ghost">
          <GitBranch className="size-4" />
        </Button>
      ) : null}
      {onDelete ? (
        <Button aria-label={t("memory.identity.delete")} onClick={onDelete} size="icon-sm" type="button" variant="ghost">
          <Trash2 className="size-4 text-destructive" />
        </Button>
      ) : null}
    </div>
  )
}

// canonicalProjectPath converts one visible project name into the exact VMM hierarchy.
// canonicalProjectPath 将一个可见项目名称转换为准确的 VMM 层级路径。
function canonicalProjectPath(projectName: string): string {
  const normalizedName = projectName.trim().replace(/[\\/]+/g, "-")
  return `default/default/${normalizedName}`
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
