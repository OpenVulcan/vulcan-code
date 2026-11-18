import { useI18n } from "@/i18n";
import { Checkbox } from "@/ui-kit/primitives/checkbox";
import { MCP_TOOL_GROUPS, mcpToolDetailKey, type McpToolPage } from "./mcp-service-model";

/** The service-provided candidates and draft selection for one authorization page.
 * 一个授权页面的服务端候选和草稿选择。 */
interface McpToolGrantsProps {
  page: McpToolPage;
  candidates: readonly string[];
  selected: readonly string[];
  disabled: boolean;
  onChange: (tools: readonly string[], checked: boolean) => void;
}

/** Renders the selected page's tool groups with localized tool names, descriptions and bulk toggles.
 * 显示当前页面的工具分组、已本地化的工具名称和说明，以及批量开关。 */
export function McpToolGrants({ page, candidates, selected, disabled, onChange }: McpToolGrantsProps) {
  const { t } = useI18n();
  return MCP_TOOL_GROUPS.filter((group) => group.page === page).map((group) => {
    const tools = group.tools.filter((tool) => candidates.includes(tool));
    if (tools.length === 0) return null;
    const checked = tools.every((tool) => selected.includes(tool));
    const partial = !checked && tools.some((tool) => selected.includes(tool));
    return (
      <section className="vk-mcp-block" data-mcp-tool-group={group.id} key={group.id}>
        <div className="vk-mcp-group-heading">
          <span className="vk-mcp-block-title">{t(group.label)}</span>
          {tools.length > 1 ? (
            <label className="vk-mcp-check">
              <Checkbox aria-label={t(group.label)} checked={partial ? "indeterminate" : checked} data-mcp-group-toggle=""
                disabled={disabled} onCheckedChange={(value) => onChange(tools, value === true)} />
              <span>{t("mcpService.group.selectAll")}</span>
            </label>
          ) : null}
        </div>
        {group.id === "shell" && checked ? <p className="vk-mcp-note">{t("mcpService.field.shellAccess")}</p> : null}
        <div className="vk-mcp-checklist vk-mcp-tool-list">
          {tools.map((tool) => (
            <label className="vk-mcp-check" key={tool} title={tool}>
              <Checkbox aria-label={t(`mcpService.tool.${tool}.name`)} checked={selected.includes(tool)} data-mcp-tool={tool}
                disabled={disabled} onCheckedChange={(value) => onChange([tool], value === true)} />
              <span>{t(`mcpService.tool.${tool}.name`)}<small>{t(mcpToolDetailKey(tool))}</small></span>
            </label>
          ))}
        </div>
      </section>
    );
  });
}
