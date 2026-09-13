"use client"

import { Toaster as Sonner, type ToasterProps } from "sonner"
import { cn } from "@/lib/utils"
import { useUiText } from "@/components/ui/ui-text"
import "@/styles/toasts.css"

// Toaster renders the shared toast viewport used by WebUI actions.
// Toaster 渲染 WebUI 操作共用的 toast 视口。
// Parameters: props customize native toast options. Returns: the localized, theme-token notification viewport.
// 参数：props 自定义原生通知选项。返回值：本地化并使用主题令牌的通知视口。
function Toaster({ className, toastOptions, ...props }: ToasterProps) {
  // text is supplied by the same localization authority as dialogs and loading indicators.
  // text 由与弹窗及加载指示器相同的本地化权威提供。
  const text = useUiText()
  return (
    <Sonner
      className={cn("toaster group", className)}
      position="bottom-right"
      containerAriaLabel={text.notifications}
      toastOptions={{
        closeButtonAriaLabel: text.close,
        ...toastOptions,
        classNames: {
          toast:
            "group toast group-[.toaster]:bg-popover group-[.toaster]:text-popover-foreground group-[.toaster]:border-border group-[.toaster]:shadow-lg",
          description: "group-[.toast]:text-muted-foreground",
          actionButton: "group-[.toast]:bg-primary group-[.toast]:text-primary-foreground",
          cancelButton: "group-[.toast]:bg-muted group-[.toast]:text-muted-foreground",
          ...toastOptions?.classNames,
        },
      }}
      {...props}
    />
  )
}

export { Toaster }
