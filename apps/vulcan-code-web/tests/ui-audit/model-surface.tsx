import React from "react";
import { I18nProvider } from "../../src/i18n";
import { Button } from "../../src/components/ui/button";
import { TooltipProvider } from "../../src/components/ui/tooltip";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../src/components/ui/select";
import { ModelBuiltinCapabilityIcons } from "../../src/components/shared/model-builtin-capabilities";
import { ProviderToolExtensionModelCard } from "../../src/components/shared/provider-tool-extension-model-card";
import { ModelPickerControl, modelPickerChoiceKey, modelPickerChoicesFromProviders } from "../../src/components/composer/model-picker-control";
import { ModelReasoningPickerControl } from "../../src/components/composer/model-reasoning-picker-control";
import type { ProviderSummary } from "../../src/types";

/** Mounts model controls with exact synthetic route identities and local callbacks.
 * 挂载模型控件，使用精确的合成路由身份与本地回调。
 * No parameters; returns selector, account and model-profile acceptance controls.
 * 无参数；返回选择器、账号与模型档案验收控件。
 */
export function AuditModelSurface() {
  // long declares the continuous name boundary independently from each route identity.
  // long 独立于每个路由身份声明连续名称边界。
  const long = new URLSearchParams(window.location.search).get("content") === "long";
  // providers groups two distinct accounts beneath one provider and two models.
  // providers 将两个不同账号聚合到一个供应商和两个模型下。
  const providers: ProviderSummary[] = ["a", "b"].map((id) => ({
    id, sourceProviderId: "custom:profile-audit", connectionId: id, name: "合成供应商", status: "ready",
    providerDisplayName: long ? "LongProviderName".repeat(12) : "合成供应商", accountName: `账号 ${id}`,
    models: [{ id: "first", name: long ? "LongModelName".repeat(12) : "模型一", active: id === "a", preferredConnection: id === "b", inputModalities: id === "a" ? ["image"] : ["image", "video", "audio"], providerNativeTools: id === "b" ? ["web_search"] : [] },
      { id: "second", name: "模型二", preferredConnection: id === "b" }],
  }));
  // route records only the explicit local selection; missing routes never create a real setting.
  // route 仅记录明确的本地选择；失效路由不会创建真实设置。
  const [route, setRoute] = React.useState(JSON.stringify(["custom:profile-audit", "first", "a"]));
  // catalog and disabled exercise independent availability gates.
  // catalog 与 disabled 验证独立的可用性门控。
  const [catalog, setCatalog] = React.useState(true);
  const [disabled, setDisabled] = React.useState(false);
  // checked and account own only extension-card selection and its independent account control.
  // checked 与 account 仅持有扩展卡片选择及其独立账号控件。
  const [checked, setChecked] = React.useState(false);
  const [account, setAccount] = React.useState("a");
  // reasoning owns only the merged control's local level choice.
  // reasoning 仅持有合并控件的本地档位选择。
  const [reasoning, setReasoning] = React.useState("low");
  // choices derives the exact first-model account set from the same declared directory.
  // choices 从相同声明目录派生精确的首个模型账号集合。
  const choices = modelPickerChoicesFromProviders(providers).filter(choice => choice.modelId === "first");
  return <I18nProvider><TooltipProvider delayDuration={0}><main className="mx-auto grid min-w-0 max-w-3xl gap-4 p-4">
    <h1>模型选择与档案验收</h1><output className="[overflow-wrap:anywhere]">{route} · {String(checked)} · {account}</output>
    <div className="flex flex-wrap gap-2"><Button onClick={() => setRoute("missing")}>设为失效路由</Button><Button onClick={() => setCatalog(value => !value)}>切换空目录</Button><Button onClick={() => setDisabled(value => !value)}>切换禁用</Button></div>
    <ModelPickerControl ariaLabel="验收模型" fallbackLabel="选择模型" unavailableValueLabel="请重新选择" filterPlaceholder="搜索模型或账号" noMatchesLabel="没有匹配模型" providers={catalog ? providers : []} disabled={disabled} value={route} onValueChange={choice => setRoute(modelPickerChoiceKey(choice))} />
    <ModelPickerControl ariaLabel="仅模型设置" fallbackLabel="选择模型" filterPlaceholder="过滤模型设置" noMatchesLabel="没有匹配模型" providers={providers} selectionScope="model" value={JSON.stringify(["custom:profile-audit", "second"])} onValueChange={choice => setRoute(JSON.stringify([choice.providerId, choice.modelId]))} />
    <ModelReasoningPickerControl ariaLabel="验收模型与账号" fallbackLabel="选择模型" filterPlaceholder="搜索模型或账号" noMatchesLabel="没有匹配模型" providers={catalog ? providers : []} disabled={disabled} value={route} onModelChange={choice => setRoute(modelPickerChoiceKey(choice))}
      accountAriaLabel="验收账号" accounts={catalog ? choices : []} accountValue={account} onAccountChange={choice => setAccount(choice.connectionId)}
      reasoningOptions={[{ value: "none", label: "none" }, { value: "low", label: "low" }, { value: "high", label: "high" }]} reasoningEffort={reasoning} defaultReasoningEffort="low" onReasoningEffortChange={setReasoning} />
    <div><span>全部内置能力：</span><ModelBuiltinCapabilityIcons inputResources={["image", "video", "audio"]} providerNativeTools={["web_search"]} /><span>空声明：</span><ModelBuiltinCapabilityIcons /></div>
    <ProviderToolExtensionModelCard title={long ? "LongExtensionTitle".repeat(12) : "扩展模型"} summary="模型行为说明" characteristics="精确路线的模型特性" constraints={long ? "LongConstraint".repeat(20) : "模型约束"} limitations="模型限制" checked={checked} checkboxDisabled={disabled} onCheckedChange={setChecked} providerId="custom:profile-audit" modelId={long ? "LongModelIdentity".repeat(12) : "first"} accountControl={<Select value={account} onValueChange={setAccount}><SelectTrigger aria-label="卡片账号"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="a">账号 a</SelectItem><SelectItem value="b">账号 b</SelectItem></SelectContent></Select>} />
    <Button variant="outline">末尾焦点</Button>
  </main></TooltipProvider></I18nProvider>;
}
