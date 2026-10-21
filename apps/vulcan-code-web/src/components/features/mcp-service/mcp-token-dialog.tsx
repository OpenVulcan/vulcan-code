import * as React from "react";
import { Copy, KeyRound } from "lucide-react";
import { copyText } from "@/components/workspace/workbench-layout/types";
import { formatMessage, useI18n } from "@/i18n";
import { Button } from "@/ui-kit/primitives/button";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { toast } from "@/ui-kit/primitives/toast";
import { connectionSnippet, MCP_CODEX_TOKEN_ENV, type McpSnippetKind } from "./mcp-service-model";

// McpTokenDialogProps shows one freshly issued token.
// McpTokenDialogProps 展示一个新签发的令牌。
export interface McpTokenDialogProps {
  clientName: string;
  token: string;
  endpointUrl?: string;
  onClose: () => void;
}

/** Shows a token exactly once, with ready-to-paste setup for common MCP clients.
 * 只展示一次令牌，并附带常见 MCP 客户端可直接粘贴的配置。 */
export function McpTokenDialog(props: McpTokenDialogProps) {
  const { t } = useI18n();
  const [kind, setKind] = React.useState<McpSnippetKind>("claude");
  const copy = async (text: string) => {
    try {
      await copyText(text);
      toast.success(t("mcpService.copied"));
    } catch {
      toast.error(t("mcpService.copyFailed"));
    }
  };
  const snippet = props.endpointUrl ? connectionSnippet(kind, props.endpointUrl, props.token) : null;
  return (
    <Dialog open onOpenChange={(open) => { if (!open) props.onClose(); }}>
      <DialogContent data-mcp-token-dialog="" size="lg">
        <DialogHeader>
          <DialogTitle>{formatMessage(t("mcpService.token.title"), { name: props.clientName })}</DialogTitle>
          <DialogDescription>{t("mcpService.token.detail")}</DialogDescription>
        </DialogHeader>
        <div className="vk-mcp-form">
          <section className="vk-mcp-block">
            <span className="vk-mcp-block-title">{t("mcpService.token.label")}</span>
            <div className="vk-mcp-secret">
              <KeyRound aria-hidden size={14} />
              <code data-mcp-token="">{props.token}</code>
              <Button aria-label={t("mcpService.copy")} onClick={() => void copy(props.token)} size="sm" type="button" variant="outline">
                <Copy size={14} />{t("mcpService.copy")}
              </Button>
            </div>
          </section>
          <section className="vk-mcp-block">
            <span className="vk-mcp-block-title">{t("mcpService.token.connect")}</span>
            {snippet ? <>
              <SegmentedControl aria-label={t("mcpService.token.connect")} onValueChange={(value) => setKind(value as McpSnippetKind)} value={kind}>
                <SegmentedControlItem value="claude">{t("mcpService.snippet.claude")}</SegmentedControlItem>
                <SegmentedControlItem value="codex">{t("mcpService.snippet.codex")}</SegmentedControlItem>
                <SegmentedControlItem value="json">{t("mcpService.snippet.json")}</SegmentedControlItem>
              </SegmentedControl>
              <div className="vk-mcp-snippet">
                <pre data-mcp-snippet={kind}>{snippet}</pre>
                <Button aria-label={t("mcpService.copy")} onClick={() => void copy(snippet)} size="sm" type="button" variant="ghost">
                  <Copy size={14} />
                </Button>
              </div>
              <p className="vk-mcp-note">
                {kind === "codex"
                  ? formatMessage(t("mcpService.snippet.codexHint"), { env: MCP_CODEX_TOKEN_ENV })
                  : t(kind === "claude" ? "mcpService.snippet.claudeHint" : "mcpService.snippet.jsonHint")}
              </p>
            </> : <p className="vk-mcp-note">{t("mcpService.endpoint.unavailable")}</p>}
          </section>
        </div>
        <DialogFooter>
          <Button data-mcp-token-done="" onClick={props.onClose} size="sm" type="button">{t("mcpService.token.done")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
