import * as React from "react"
import "@/ui-kit/styles/features/provider-models.css"
import "@/ui-kit/styles/features/settings-forms.css"
import { LoaderCircle } from "lucide-react"

import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/ui-kit/primitives/alert-dialog"
import { Badge } from "@/ui-kit/primitives/badge"
import { Button } from "@/ui-kit/primitives/button"
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
        <dl className="vk-facts vk-form-summary">
          {props.items.map((item) => (
            <React.Fragment key={`${item.label}:${item.value}`}>
              <dt>{item.label}</dt>
              <dd>
                {item.warning ? <Badge variant="destructive">{item.value}</Badge> : item.value}
              </dd>
            </React.Fragment>
          ))}
        </dl>
        {props.error ? (
          <p className="vk-form-fault" role="alert">
            {props.error}
          </p>
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={props.saving}>{t("memory.common.cancel")}</AlertDialogCancel>
          <Button disabled={props.saving} onClick={props.onConfirm} type="button">
            {props.saving ? <LoaderCircle aria-hidden="true" className="vk-spin" /> : null}
            {t("settings.customModels.confirmChanges")}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
