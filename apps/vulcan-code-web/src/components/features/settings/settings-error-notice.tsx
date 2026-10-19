import * as React from "react"
import { CircleAlert, Copy } from "lucide-react"

import { Button } from "@/ui-kit/primitives/button"
import { InlineDisclosure } from "@/ui-kit/primitives/disclosure"
import { useI18n } from "@/i18n"
import { cn } from "@/lib/utils"
import "@/styles/settings-error-notice.css"

// SettingsErrorNoticeProps describes one failed settings action: a plain-language outcome plus the raw error.
// SettingsErrorNoticeProps 描述一次失败的设置操作：通俗的结果说明加原始错误。
export interface SettingsErrorNoticeProps {
  // message tells the user what happened and what is safe, e.g. "保存失败，你的修改还在".
  // message 告诉用户发生了什么以及哪些内容仍然安全，例如「保存失败，你的修改还在」。
  message: React.ReactNode
  // detail is the raw technical error, folded by default and copyable for bug reports.
  // detail 是原始技术错误，默认收起，可复制用于反馈问题。
  detail?: string | null
  // className keeps page-specific hooks used by layouts and tests.
  // className 保留页面布局与测试使用的特定类名。
  className?: string
  // action offers a contextual recovery operation after the diagnostic disclosure.
  // action 在诊断展开区后提供当前情境的恢复操作。
  action?: React.ReactNode
}

// SettingsErrorNotice renders a calm inline error with folded technical details and a copy action.
// SettingsErrorNotice 渲染克制的行内错误提示，技术细节默认收起并提供复制操作。
// Parameters: props provides the readable message, optional raw detail, recovery action and page hook class.
// 参数：props 提供可读说明、可选原始错误、恢复操作及页面钩子类名。
// Returns an alert region so assistive technology announces the failure once.
// 返回 alert 区域，使辅助技术播报一次失败信息。
export function SettingsErrorNotice(props: SettingsErrorNoticeProps) {
  const { t } = useI18n()
  // copied shows a short confirmation after the raw error reaches the clipboard.
  // copied 在原始错误写入剪贴板后显示简短确认。
  const [copied, setCopied] = React.useState<"idle" | "copied" | "failed">("idle")
  const detail = props.detail?.trim()
  return (
    <div className={cn("settings-error-notice", props.className)} role="alert">
      <CircleAlert aria-hidden="true" className="settings-error-notice-icon" size={16} />
      <div className="settings-error-notice-body">
        <p>{props.message}</p>
        {detail ? <InlineDisclosure keepMounted label={t("settings.errors.showErrorDetails")} openLabel={t("settings.errors.hideErrorDetails")}>
          <pre>{detail}</pre>
          <Button size="xs" type="button" variant="ghost" onClick={async () => {
            try { await navigator.clipboard.writeText(detail); setCopied("copied") }
            catch { setCopied("failed") }
          }}>
            <Copy size={12} />{copied === "copied" ? t("settings.errors.copied") : copied === "failed" ? t("settings.errors.copyFailedSelectManually") : t("settings.errors.copyError")}
          </Button>
        </InlineDisclosure> : null}
        {props.action ? <div className="settings-error-notice-action">{props.action}</div> : null}
      </div>
    </div>
  )
}
