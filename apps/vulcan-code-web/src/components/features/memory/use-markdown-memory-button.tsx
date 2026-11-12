import * as React from "react";
import { useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { Button } from "@/ui-kit/primitives/button";
import { toast } from "@/ui-kit/primitives/toast";
import { applyMemoryMode, useMemorySettings } from "./markdown/memory-mode-store";

// Offers the same explicit Markdown switch from first-use setup and missing-model diagnostics.
// 在首次配置与模型缺失诊断中提供相同的 Markdown 显式切换操作。
export function UseMarkdownMemoryButton({ disabled, onApplied }: { disabled?: boolean; onApplied?: () => void }) {
  const { t } = useI18n();
  // Read the current mode so the shortcut is unavailable when Markdown is already active.
  // 读取当前模式，使 Markdown 已启用时无法重复切换。
  const settings = useMemorySettings();
  // Keep the action locked until the service confirms the mode change.
  // 在服务确认模式变更前锁定操作。
  const [saving, setSaving] = React.useState(false);

  // Applies Markdown and invokes the caller's completion action after publication.
  // 应用 Markdown，并在发布成功后调用调用方的完成操作。
  const apply = async () => {
    setSaving(true);
    try {
      await applyMemoryMode("markdown");
      toast.success(t("memory.markdown.saved"));
      onApplied?.();
    } catch (error) {
      toast.error(t("memory.markdown.saveFailed"), { description: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  };

  return <Button data-memory-use-markdown="" disabled={disabled || saving || settings?.mode === "markdown"} onClick={() => void apply()} type="button" variant="outline">
    {t("memory.setup.useMarkdown")}
  </Button>;
}
