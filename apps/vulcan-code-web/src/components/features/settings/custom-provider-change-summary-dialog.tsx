import { LoaderCircle } from "lucide-react"

import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { useI18n } from "@/i18n"

// CustomProviderChangeSummaryItem describes one user-readable persisted change.
// CustomProviderChangeSummaryItem 描述一项面向用户的持久化变更。
export interface CustomProviderChangeSummaryItem {
  // label names the affected setting dimension.
  // label 标识受影响的设置维度。
  label: string
  // value explains the exact value that will be persisted.
  // value 说明将要持久化的精确值。
  value: string
  // warning marks a value that keeps a credential routing-isolated.
  // warning 标记会使凭据继续保持路由隔离的值。
  warning?: boolean
}

// CustomProviderChangeSummaryDialog confirms one bounded settings write before persistence.
// CustomProviderChangeSummaryDialog 在持久化前确认一项有界设置写入。
export function CustomProviderChangeSummaryDialog(props: {
  description: string
  error?: string | null
  items: CustomProviderChangeSummaryItem[]
  onConfirm: () => void
  onOpenChange: (open: boolean) => void
  open: boolean
  saving: boolean
}) {
  const { t } = useI18n()
  return (
    <AlertDialog onOpenChange={props.onOpenChange} open={props.open}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("settings.customModels.changeSummaryTitle")}</AlertDialogTitle>
          <AlertDialogDescription>{props.description}</AlertDialogDescription>
        </AlertDialogHeader>
        <dl className="grid gap-2 rounded-lg border bg-muted/20 p-3 text-sm">
          {props.items.map((item) => (
            <div className="grid gap-1 sm:grid-cols-[9rem_minmax(0,1fr)] sm:items-start" key={`${item.label}:${item.value}`}>
              <dt className="text-muted-foreground">{item.label}</dt>
              <dd className="min-w-0 break-words font-medium">
                {item.warning ? <Badge variant="destructive">{item.value}</Badge> : item.value}
              </dd>
            </div>
          ))}
        </dl>
        {props.error ? (
          <p className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">
            {props.error}
          </p>
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={props.saving}>{t("memory.common.cancel")}</AlertDialogCancel>
          <Button disabled={props.saving} onClick={props.onConfirm} type="button">
            {props.saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}
            {t("settings.customModels.confirmChanges")}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
