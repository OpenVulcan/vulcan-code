import * as React from "react";
import { ChevronDown, Save, SlidersHorizontal } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { useI18n } from "@/i18n";
import type { VceProjectOverviewResponse, VceProjectSettingsInput } from "@/types";

// VceRetrievalSettings renders ignore rules, retrieval budgets and optional AI query features.
// VceRetrievalSettings 渲染忽略规则、检索预算与可选 AI 查询功能。
export function VceRetrievalSettings(props: {
  // Current authoritative settings, including host-owned default ignore rules.
  // 当前权威设置，包含宿主持有的默认忽略规则。
  overview: VceProjectOverviewResponse;
  // Whether an overview read or project mutation is active.
  // 是否正在执行概览读取或项目修改。
  busy: boolean;
  // User-editable settings draft.
  // 用户可编辑的设置草稿。
  draft: VceProjectSettingsInput;
  // Replaces the draft without saving.
  // 替换草稿但不保存。
  setDraft: (draft: VceProjectSettingsInput) => void;
  // Saves a complete draft against its frozen revision.
  // 按冻结修订保存完整草稿。
  saveDraft: (next: VceProjectSettingsInput) => Promise<void>;
}) {
  // t localizes retrieval settings.
  // t 本地化检索设置。
  const { t } = useI18n();
  const { draft, setDraft, saveDraft } = props;
  return (
    <details className="vce-settings-disclosure vce-advanced-settings">
      <summary>
        <SlidersHorizontal size={16} aria-hidden="true" />
        <span><strong>{t("codeContext.area.retrievalAndFileFilters")}</strong>
          <small>{t("codeContext.area.ignoreRulesResultLimits")}</small>
        </span>
        <ChevronDown size={16} aria-hidden="true" />
      </summary>
      <div className="vce-settings-disclosure-body">
        <details className="rounded-lg border p-3 text-sm">
          <summary className="cursor-pointer font-medium">
            {t("codeContext.area.defaultFilterRules")} ({props.overview.settings.defaultIgnorePatterns.length})
          </summary>
          <p className="mt-2 text-xs text-muted-foreground">
            {t("codeContext.area.theseRulesAlwaysApply")}
          </p>
          <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap rounded bg-muted p-2 text-xs">
            {props.overview.settings.defaultIgnorePatterns.join("\n")}
          </pre>
        </details>
        <label className="grid gap-1 text-sm">
          {t("codeContext.area.customIgnoreRulesOne")}
          <Textarea
            disabled={props.busy}
            onChange={(event) =>
              setDraft({
                ...draft,
                extraIgnorePatterns: event.target.value.split(/\r?\n/),
              })
            }
            value={draft.extraIgnorePatterns.join("\n")}
          />
        </label>
        <div className="grid grid-cols-2 gap-3">
          <NumericSetting
            disabled={props.busy}
            label={t("codeContext.area.maximumFileBytes")}
            value={draft.maxFileBytes}
            onChange={(value) => setDraft({ ...draft, maxFileBytes: value })}
          />
          <NumericSetting
            disabled={props.busy}
            label={t("codeContext.area.maximumResults")}
            value={draft.maxResults}
            onChange={(value) => setDraft({ ...draft, maxResults: value })}
          />
          <NumericSetting
            disabled={props.busy}
            label={t("codeContext.area.contextCharacterBudget")}
            value={draft.maxContextChars}
            onChange={(value) => setDraft({ ...draft, maxContextChars: value })}
          />
          <NumericSetting
            disabled={props.busy}
            label={t("codeContext.area.embeddingConcurrency")}
            value={draft.embeddingMaxConcurrency}
            onChange={(value) =>
              setDraft({ ...draft, embeddingMaxConcurrency: value })
            }
          />
          <NumericSetting
            disabled={props.busy}
            label={t("codeContext.area.queryConcurrency")}
            value={draft.queryMaxConcurrency}
            onChange={(value) =>
              setDraft({ ...draft, queryMaxConcurrency: value })
            }
          />
        </div>
        <ToggleSetting
          checked={draft.allowQueryRewrite}
          disabled={props.busy}
          label={t("codeContext.area.allowLlmQueryRewriting")}
          onChange={(value) => setDraft({ ...draft, allowQueryRewrite: value })}
        />
        <ToggleSetting
          checked={draft.allowRerank}
          disabled={props.busy}
          label={t("codeContext.area.allowCandidateReranking")}
          onChange={(value) => setDraft({ ...draft, allowRerank: value })}
        />
        <Button
          disabled={props.busy}
          onClick={() => void saveDraft(draft)}
          type="button"
        >
          <Save size={15} /> {t("codeContext.area.saveSettings")}
        </Button>
      </div>
    </details>
  );
}

// NumericSetting renders one explicit integer input used by a VCE budget.
// NumericSetting 渲染一个用于 VCE 预算的明确整数输入。
function NumericSetting(props: {
  // Whether the setting must reject edits during an authoritative host mutation.
  // 设置在权威宿主修改期间是否必须拒绝编辑。
  disabled: boolean;
  // Human-facing setting label.
  // 面向用户的设置标签。
  label: string;
  // Current integer value.
  // 当前整数值。
  value: number;
  // Receives the exact parsed integer.
  // 接收精确解析的整数。
  onChange: (value: number) => void;
}) {
  return (
    <label className="grid gap-1 text-sm">
      {props.label}
      <Input
        disabled={props.disabled}
        min={1}
        onChange={(event) =>
          props.onChange(Number.parseInt(event.target.value, 10) || 0)
        }
        type="number"
        value={props.value}
      />
    </label>
  );
}

// ToggleSetting renders one labeled Boolean VCE setting.
// ToggleSetting 渲染一个带标签的布尔 VCE 设置。
function ToggleSetting(props: {
  // Current Boolean setting value.
  // 当前布尔设置值。
  checked: boolean;
  // Whether the toggle must reject edits during an authoritative host mutation.
  // 开关在权威宿主修改期间是否必须拒绝编辑。
  disabled: boolean;
  // Human-facing setting label.
  // 面向用户的设置标签。
  label: string;
  // Receives the next explicit Boolean value.
  // 接收下一项明确布尔值。
  onChange: (value: boolean) => void;
}) {
  return (
    <label className="flex items-center gap-2 text-sm">
      <Switch
        checked={props.checked}
        disabled={props.disabled}
        onCheckedChange={props.onChange}
      />
      <span>{props.label}</span>
    </label>
  );
}
