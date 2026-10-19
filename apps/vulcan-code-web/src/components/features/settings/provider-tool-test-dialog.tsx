import * as React from "react";

import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog";
import { Input } from "@/ui-kit/primitives/input";
import { Label } from "@/ui-kit/primitives/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/ui-kit/primitives/select";
import { Switch } from "@/ui-kit/primitives/switch";
import { Textarea } from "@/ui-kit/primitives/textarea";
import { useI18n } from "@/i18n";
import { testPublicAiProviderTool } from "@/service/settings-actions-service";
import type {
  PublicAiProviderToolTargetSnapshot,
  PublicAiProviderToolTestResult,
} from "@/types";
import { errorMessage } from "@/lib/display-format";

// ProviderToolTestDialogProps defines one exact settings-row diagnostic dialog.
// ProviderToolTestDialogProps 定义一个精确设置行诊断对话框。
interface ProviderToolTestDialogProps {
  // target identifies the exact credential connection and provider tool; null closes the dialog.
  // target 标识精确凭据连接与供应商工具；null 表示关闭对话框。
  target: PublicAiProviderToolTargetSnapshot | null;
  // onClose clears the target owned by the settings page.
  // onClose 清除设置页面持有的目标。
  onClose: () => void;
}

// JsonSchemaProperty contains only the verified schema fields used to shape the diagnostic form.
// JsonSchemaProperty 仅包含用于组织诊断表单的已验证 Schema 字段。
interface JsonSchemaProperty {
  // type is the declared JSON value kind.
  // type 是声明的 JSON 值类型。
  type?: string;
  // default is the provider-target default value.
  // default 是供应商目标默认值。
  default?: unknown;
  // minimum is the inclusive numeric lower bound.
  // minimum 是包含边界的数值下限。
  minimum?: number;
  // maximum is the inclusive numeric upper bound.
  // maximum 是包含边界的数值上限。
  maximum?: number;
  // minItems is the inclusive array length lower bound.
  // minItems 是包含边界的数组长度下限。
  minItems?: number;
  // maxItems is the inclusive array length upper bound.
  // maxItems 是包含边界的数组长度上限。
  maxItems?: number;
  // enum contains the exact declared string vocabulary.
  // enum 包含精确声明的字符串词汇表。
  enum?: unknown[];
}

// ProviderToolTestDialog executes and renders one real Search or Extract diagnostic.
// ProviderToolTestDialog 执行并渲染一次真实 Search 或 Extract 诊断。
// Parameters: props supplies an exact diagnostic target and dialog state; returns schema-backed inputs, explicit execution and result/error output.
// 参数：props 提供精确诊断目标与弹窗状态；返回基于 Schema 的输入、显式执行和结果或错误输出。
export function ProviderToolTestDialog(props: ProviderToolTestDialogProps) {
  // t resolves every operator-visible diagnostic label.
  // t 解析全部面向操作员的诊断标签。
  const { t } = useI18n();
  // query contains the current Search query.
  // query 包含当前 Search 查询。
  const [query, setQuery] = React.useState("");
  // maxResults contains the optional Search result limit.
  // maxResults 包含可选 Search 结果上限。
  const [maxResults, setMaxResults] = React.useState("5");
  // includeDomains contains one Search allowlist domain per line.
  // includeDomains 包含每行一个的 Search 白名单域名。
  const [includeDomains, setIncludeDomains] = React.useState("");
  // excludeDomains contains one Search blocklist domain per line.
  // excludeDomains 包含每行一个的 Search 黑名单域名。
  const [excludeDomains, setExcludeDomains] = React.useState("");
  // urls contains one Extract HTTPS address per line.
  // urls 包含每行一个的 Extract HTTPS 地址。
  const [urls, setUrls] = React.useState("");
  // extractQuery contains the optional relevance query.
  // extractQuery 包含可选的相关性查询。
  const [extractQuery, setExtractQuery] = React.useState("");
  // chunksPerSource contains the optional relevance chunk count.
  // chunksPerSource 包含可选的相关性片段数量。
  const [chunksPerSource, setChunksPerSource] = React.useState("3");
  // extractDepth is the exact provider-declared extraction depth.
  // extractDepth 是供应商精确声明的抓取深度。
  const [extractDepth, setExtractDepth] = React.useState("basic");
  // extractFormat is the exact provider-declared content format.
  // extractFormat 是供应商精确声明的内容格式。
  const [extractFormat, setExtractFormat] = React.useState("markdown");
  // timeoutSeconds contains the optional provider timeout.
  // timeoutSeconds 包含可选的供应商超时时间。
  const [timeoutSeconds, setTimeoutSeconds] = React.useState("");
  // includeImages controls provider-returned image references.
  // includeImages 控制供应商返回的图片引用。
  const [includeImages, setIncludeImages] = React.useState(false);
  // includeFavicon controls provider-returned site icon references.
  // includeFavicon 控制供应商返回的站点图标引用。
  const [includeFavicon, setIncludeFavicon] = React.useState(false);
  // pending reports the single active real provider request.
  // pending 表示唯一正在执行的真实供应商请求。
  const [pending, setPending] = React.useState(false);
  // error contains one visible controlled diagnostic failure.
  // error 包含一项可见的受控诊断失败。
  const [error, setError] = React.useState("");
  // result contains one normalized successful provider response.
  // result 包含一份规范化成功供应商响应。
  const [result, setResult] = React.useState<PublicAiProviderToolTestResult | null>(null);
  // requestSequence binds a paid diagnostic response to one exact target generation.
  // requestSequence 将付费诊断响应绑定到一个精确目标代次。
  const requestSequence = React.useRef(0);
  // requestBusy prevents a second dispatch before React publishes pending state.
  // requestBusy 在 React 发布等待状态之前阻止第二次分派。
  const requestBusy = React.useRef(false);
  // initializedTarget ignores unrelated snapshot object refreshes while preserving user input.
  // initializedTarget 忽略无关快照对象刷新，同时保留用户输入。
  const initializedTarget = React.useRef<string | null>(null);
  React.useEffect(() => () => { requestSequence.current += 1; }, []);

  React.useEffect(() => {
    const identity = props.target ? JSON.stringify([props.target.connectionId, props.target.target.target_id]) : null;
    if (initializedTarget.current === identity) return;
    initializedTarget.current = identity;
    requestSequence.current += 1;
    requestBusy.current = false;
    // searchLimit reads the exact selected target default instead of inventing a provider fallback.
    // searchLimit 读取精确所选目标默认值，而不是构造供应商兜底值。
    const searchLimit = schemaProperty(props.target, "max_results");
    // depthValues preserves the exact provider-declared extraction vocabulary.
    // depthValues 保留精确的供应商声明抓取词汇表。
    const depthValues = schemaEnumValues(schemaProperty(props.target, "extract_depth"));
    // formatValues preserves the exact provider-declared output vocabulary.
    // formatValues 保留精确的供应商声明输出词汇表。
    const formatValues = schemaEnumValues(schemaProperty(props.target, "format"));
    setQuery("");
    setMaxResults(String(schemaDefaultNumber(searchLimit) ?? 5));
    setIncludeDomains("");
    setExcludeDomains("");
    setUrls("");
    setExtractQuery("");
    setChunksPerSource(String(
      schemaDefaultNumber(schemaProperty(props.target, "chunks_per_source")) ?? 3,
    ));
    setExtractDepth(
      schemaDefaultString(schemaProperty(props.target, "extract_depth"))
        ?? depthValues[0]
        ?? "basic",
    );
    setExtractFormat(
      schemaDefaultString(schemaProperty(props.target, "format"))
        ?? formatValues[0]
        ?? "markdown",
    );
    setTimeoutSeconds("");
    setIncludeImages(false);
    setIncludeFavicon(false);
    setPending(false);
    setError("");
    setResult(null);
  }, [props.target]);

  // capability is the exact closed semantic operation carried by the target.
  // capability 是目标承载的精确封闭语义操作。
  const capability = props.target?.target.contract.capability;
  // isSearch selects the Search form and result contract.
  // isSearch 选择 Search 表单与结果契约。
  const isSearch = capability === "web_search";
  // urlValues is the normalized list of operator-entered Extract addresses.
  // urlValues 是规范化后的操作员输入 Extract 地址列表。
  const urlValues = splitNonEmptyLines(urls);
  // urlSchema carries the selected target's exact URL count limits.
  // urlSchema 承载所选目标精确的 URL 数量限制。
  const urlSchema = schemaProperty(props.target, "urls");
  // minimumUrls is the declared minimum URL count.
  // minimumUrls 是声明的最小 URL 数量。
  const minimumUrls = urlSchema?.minItems ?? 1;
  // maximumUrls is the declared maximum URL count.
  // maximumUrls 是声明的最大 URL 数量。
  const maximumUrls = urlSchema?.maxItems ?? 1;
  // searchReady validates the required query before provider dispatch.
  // searchReady 在供应商分派前校验必填查询。
  const searchReady = query.trim() !== "";
  // extractReady validates the declared URL count and HTTPS contract before dispatch.
  // extractReady 在分派前校验声明的 URL 数量与 HTTPS 契约。
  const extractReady = urlValues.length >= minimumUrls
    && urlValues.length <= maximumUrls
    && urlValues.every(isValidHttpsUrl);
  // canSubmit reports whether the active form can start one real request.
  // canSubmit 表示当前表单是否可以启动一次真实请求。
  const canSubmit = Boolean(props.target)
    && (isSearch ? searchReady : extractReady)
    && !pending;

  // submitTest projects only fields declared by the selected target schema.
  // submitTest 仅投影所选目标 Schema 声明的字段。
  // Parameters: event is the submitted target form; returns after validating declared inputs and awaiting the exact guarded diagnostic request.
  // 参数：event 是目标表单提交事件；在校验声明输入并等待精确受保护诊断请求后返回。
  async function submitTest(event: React.FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    if (!props.target || !canSubmit || requestBusy.current || !event.currentTarget.reportValidity()) {
      return;
    }
    requestBusy.current = true;
    // sequence identifies this exact request so stale completions cannot release a newer request.
    // sequence 标识此精确请求，防止旧请求完成时释放新请求。
    const sequence = requestSequence.current + 1;
    requestSequence.current = sequence;
    // arguments is the exact provider-native object accepted by the frozen target.
    // arguments 是冻结目标接受的精确供应商原生对象。
    const argumentsValue = isSearch
      ? buildSearchArguments(
          props.target,
          query,
          maxResults,
          includeDomains,
          excludeDomains,
        )
      : buildExtractArguments(
          props.target,
          urlValues,
          extractQuery,
          chunksPerSource,
          extractDepth,
          extractFormat,
          timeoutSeconds,
          includeImages,
          includeFavicon,
        );
    setPending(true);
    setError("");
    setResult(null);
    try {
      // nextResult is already normalized by the existing provider adapter.
      // nextResult 已由现有供应商适配器规范化。
      const nextResult = await testPublicAiProviderTool({
        capability: props.target.target.contract.capability,
        selection: {
          connection_id: props.target.connectionId,
          target_id: props.target.target.target_id,
        },
        arguments: argumentsValue,
      });
      if (requestSequence.current === sequence) setResult(nextResult);
    } catch (reason) {
      if (requestSequence.current === sequence) setError(errorMessage(reason));
    } finally {
      if (requestSequence.current === sequence) {
        requestBusy.current = false;
        setPending(false);
      }
    }
  }

  return (
    <Dialog
      open={props.target !== null}
      onOpenChange={(open) => {
        if (!open && !requestBusy.current) {
          props.onClose();
        }
      }}
    >
      <DialogContent
        className="flex max-h-[min(85vh,720px)] flex-col overflow-hidden p-0 sm:max-w-3xl"
        showCloseButton={!pending}
      >
        <DialogHeader className="shrink-0 border-b px-5 py-4 pr-12">
          <DialogTitle>
            {isSearch
              ? t("settings.providerTools.testSearchTitle")
              : t("settings.providerTools.testExtractTitle")}
          </DialogTitle>
          <DialogDescription>
            {isSearch
              ? t("settings.providerTools.testSearchDescription")
              : t("settings.providerTools.testExtractDescription")}
          </DialogDescription>
        </DialogHeader>
        {props.target ? (
          <div className="flex shrink-0 flex-wrap gap-2 px-5">
            <Badge variant="outline">{props.target.target.display_name}</Badge>
            <Badge variant="secondary">{props.target.connectionDisplayName}</Badge>
          </div>
        ) : null}
        <form className="flex min-h-0 flex-1 flex-col" onSubmit={(event) => void submitTest(event)}>
          <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4">
          <fieldset disabled={pending} className="min-w-0 space-y-4">
          {isSearch ? (
            <SearchTestFields
              excludeDomains={excludeDomains}
              includeDomains={includeDomains}
              maxResults={maxResults}
              query={query}
              target={props.target}
              onExcludeDomainsChange={setExcludeDomains}
              onIncludeDomainsChange={setIncludeDomains}
              onMaxResultsChange={setMaxResults}
              onQueryChange={setQuery}
            />
          ) : (
            <ExtractTestFields
              chunksPerSource={chunksPerSource}
              extractDepth={extractDepth}
              extractFormat={extractFormat}
              extractQuery={extractQuery}
              includeFavicon={includeFavicon}
              includeImages={includeImages}
              target={props.target}
              timeoutSeconds={timeoutSeconds}
              urls={urls}
              onChunksPerSourceChange={setChunksPerSource}
              onExtractDepthChange={setExtractDepth}
              onExtractFormatChange={setExtractFormat}
              onExtractQueryChange={setExtractQuery}
              onIncludeFaviconChange={setIncludeFavicon}
              onIncludeImagesChange={setIncludeImages}
              onTimeoutSecondsChange={setTimeoutSeconds}
              onUrlsChange={setUrls}
            />
          )}
          <p className="text-xs text-muted-foreground">
            {t("settings.providerTools.testConsumesQuota")}
          </p>
          {error ? <p className="text-sm text-destructive" role="alert">{error}</p> : null}
          </fieldset>
          {result ? <ProviderToolTestResults result={result} /> : null}
          </div>
          <DialogFooter className="shrink-0 px-5 py-3">
            <Button disabled={!canSubmit} type="submit">
              {pending
                ? t("settings.providerTools.testRunning")
                : t("settings.providerTools.test")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

// SearchTestFieldsProps contains the controlled Search form values and setters.
// SearchTestFieldsProps 包含受控 Search 表单值与设置器。
interface SearchTestFieldsProps {
  // target supplies the exact provider-authored Search schema.
  // target 提供精确的供应商编写 Search Schema。
  target: PublicAiProviderToolTargetSnapshot | null;
  // query is the controlled search text.
  // query 是受控搜索文本。
  query: string;
  // maxResults is the controlled result-count text.
  // maxResults 是受控结果数量文本。
  maxResults: string;
  // includeDomains is the newline-delimited allowlist text.
  // includeDomains 是换行分隔的白名单文本。
  includeDomains: string;
  // excludeDomains is the newline-delimited blocklist text.
  // excludeDomains 是换行分隔的黑名单文本。
  excludeDomains: string;
  // onQueryChange replaces the controlled query.
  // onQueryChange 替换受控查询。
  onQueryChange: (value: string) => void;
  // onMaxResultsChange replaces the controlled result count.
  // onMaxResultsChange 替换受控结果数量。
  onMaxResultsChange: (value: string) => void;
  // onIncludeDomainsChange replaces the controlled allowlist.
  // onIncludeDomainsChange 替换受控白名单。
  onIncludeDomainsChange: (value: string) => void;
  // onExcludeDomainsChange replaces the controlled blocklist.
  // onExcludeDomainsChange 替换受控黑名单。
  onExcludeDomainsChange: (value: string) => void;
}

// SearchTestFields renders only inputs declared by the selected Search target.
// SearchTestFields 仅渲染所选 Search 目标声明的输入。
function SearchTestFields(props: SearchTestFieldsProps) {
  // t resolves localized Search field labels.
  // t 解析本地化 Search 字段标签。
  const { t } = useI18n();
  // maximumProperty carries the exact result-count constraints.
  // maximumProperty 承载精确结果数量约束。
  const maximumProperty = schemaProperty(props.target, "max_results");
  // includeProperty indicates whether this target supports an allowlist.
  // includeProperty 表示此目标是否支持白名单。
  const includeProperty = schemaProperty(props.target, "include_domains");
  // excludeProperty indicates whether this target supports a blocklist.
  // excludeProperty 表示此目标是否支持黑名单。
  const excludeProperty = schemaProperty(props.target, "exclude_domains");
  return (
    <>
      <div className="grid gap-2">
        <Label htmlFor="provider-tool-test-query">
          {t("settings.providerTools.testQuery")}
        </Label>
        <Input
          autoFocus
          id="provider-tool-test-query"
          maxLength={8192}
          placeholder={t("settings.providerTools.testQueryPlaceholder")}
          value={props.query}
          onChange={(event) => props.onQueryChange(event.target.value)}
        />
      </div>
      {maximumProperty ? (
        <div className="grid gap-2 sm:max-w-48">
          <Label htmlFor="provider-tool-test-max-results">
            {t("settings.providerTools.testMaxResults")}
          </Label>
          <Input
            id="provider-tool-test-max-results"
            max={maximumProperty.maximum}
            min={maximumProperty.minimum}
            type="number"
            value={props.maxResults}
            onChange={(event) => props.onMaxResultsChange(event.target.value)}
          />
        </div>
      ) : null}
      {includeProperty || excludeProperty ? (
        <div className="grid gap-3 sm:grid-cols-2">
          {includeProperty ? (
            <div className="grid gap-2">
              <Label htmlFor="provider-tool-test-include-domains">
                {t("settings.providerTools.testIncludeDomains")}
              </Label>
              <Textarea
                id="provider-tool-test-include-domains"
                placeholder={t("settings.providerTools.testDomainsPlaceholder")}
                value={props.includeDomains}
                onChange={(event) => props.onIncludeDomainsChange(event.target.value)}
              />
            </div>
          ) : null}
          {excludeProperty ? (
            <div className="grid gap-2">
              <Label htmlFor="provider-tool-test-exclude-domains">
                {t("settings.providerTools.testExcludeDomains")}
              </Label>
              <Textarea
                id="provider-tool-test-exclude-domains"
                placeholder={t("settings.providerTools.testDomainsPlaceholder")}
                value={props.excludeDomains}
                onChange={(event) => props.onExcludeDomainsChange(event.target.value)}
              />
            </div>
          ) : null}
        </div>
      ) : null}
    </>
  );
}

// ExtractTestFieldsProps contains the controlled Extract form values and setters.
// ExtractTestFieldsProps 包含受控 Extract 表单值与设置器。
interface ExtractTestFieldsProps {
  // target supplies the exact provider-authored Extract schema.
  // target 提供精确的供应商编写 Extract Schema。
  target: PublicAiProviderToolTargetSnapshot | null;
  // urls is the newline-delimited controlled URL text.
  // urls 是换行分隔的受控 URL 文本。
  urls: string;
  // extractQuery is the controlled relevance query.
  // extractQuery 是受控相关性查询。
  extractQuery: string;
  // chunksPerSource is the controlled chunk-count text.
  // chunksPerSource 是受控片段数量文本。
  chunksPerSource: string;
  // extractDepth is the selected provider depth value.
  // extractDepth 是选中的供应商深度值。
  extractDepth: string;
  // extractFormat is the selected provider format value.
  // extractFormat 是选中的供应商格式值。
  extractFormat: string;
  // timeoutSeconds is the controlled timeout text.
  // timeoutSeconds 是受控超时文本。
  timeoutSeconds: string;
  // includeImages is the controlled image-reference flag.
  // includeImages 是受控图片引用标志。
  includeImages: boolean;
  // includeFavicon is the controlled favicon-reference flag.
  // includeFavicon 是受控站点图标引用标志。
  includeFavicon: boolean;
  // onUrlsChange replaces the controlled URL text.
  // onUrlsChange 替换受控 URL 文本。
  onUrlsChange: (value: string) => void;
  // onExtractQueryChange replaces the controlled relevance query.
  // onExtractQueryChange 替换受控相关性查询。
  onExtractQueryChange: (value: string) => void;
  // onChunksPerSourceChange replaces the controlled chunk count.
  // onChunksPerSourceChange 替换受控片段数量。
  onChunksPerSourceChange: (value: string) => void;
  // onExtractDepthChange replaces the selected depth.
  // onExtractDepthChange 替换选中的深度。
  onExtractDepthChange: (value: string) => void;
  // onExtractFormatChange replaces the selected format.
  // onExtractFormatChange 替换选中的格式。
  onExtractFormatChange: (value: string) => void;
  // onTimeoutSecondsChange replaces the controlled timeout.
  // onTimeoutSecondsChange 替换受控超时。
  onTimeoutSecondsChange: (value: string) => void;
  // onIncludeImagesChange replaces the image-reference flag.
  // onIncludeImagesChange 替换图片引用标志。
  onIncludeImagesChange: (value: boolean) => void;
  // onIncludeFaviconChange replaces the favicon-reference flag.
  // onIncludeFaviconChange 替换站点图标引用标志。
  onIncludeFaviconChange: (value: boolean) => void;
}

// ExtractTestFields renders only inputs declared by the selected Extract target.
// ExtractTestFields 仅渲染所选 Extract 目标声明的输入。
function ExtractTestFields(props: ExtractTestFieldsProps) {
  // t resolves localized Extract field labels.
  // t 解析本地化 Extract 字段标签。
  const { t } = useI18n();
  // queryProperty indicates whether relevance extraction is supported.
  // queryProperty 表示是否支持相关性抓取。
  const queryProperty = schemaProperty(props.target, "query");
  // chunksProperty carries the relevance chunk limits.
  // chunksProperty 承载相关性片段限制。
  const chunksProperty = schemaProperty(props.target, "chunks_per_source");
  // depthProperty carries the provider extraction-depth vocabulary.
  // depthProperty 承载供应商抓取深度词汇表。
  const depthProperty = schemaProperty(props.target, "extract_depth");
  // formatProperty carries the provider content-format vocabulary.
  // formatProperty 承载供应商内容格式词汇表。
  const formatProperty = schemaProperty(props.target, "format");
  // timeoutProperty carries the provider timeout constraints.
  // timeoutProperty 承载供应商超时约束。
  const timeoutProperty = schemaProperty(props.target, "timeout_seconds");
  // depthValues contains exact selectable depth values.
  // depthValues 包含精确可选深度值。
  const depthValues = schemaEnumValues(depthProperty);
  // formatValues contains exact selectable format values.
  // formatValues 包含精确可选格式值。
  const formatValues = schemaEnumValues(formatProperty);
  return (
    <>
      <div className="grid gap-2">
        <Label htmlFor="provider-tool-test-urls">
          {t("settings.providerTools.testUrls")}
        </Label>
        <Textarea
          autoFocus
          id="provider-tool-test-urls"
          placeholder={t("settings.providerTools.testUrlsPlaceholder")}
          value={props.urls}
          onChange={(event) => props.onUrlsChange(event.target.value)}
        />
      </div>
      {queryProperty ? (
        <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_12rem]">
          <div className="grid gap-2">
            <Label htmlFor="provider-tool-test-extract-query">
              {t("settings.providerTools.testExtractQuery")}
            </Label>
            <Input
              id="provider-tool-test-extract-query"
              value={props.extractQuery}
              onChange={(event) => props.onExtractQueryChange(event.target.value)}
            />
          </div>
          {chunksProperty ? (
            <div className="grid gap-2">
              <Label htmlFor="provider-tool-test-chunks">
                {t("settings.providerTools.testChunksPerSource")}
              </Label>
              <Input
                disabled={props.extractQuery.trim() === ""}
                id="provider-tool-test-chunks"
                max={chunksProperty.maximum}
                min={chunksProperty.minimum}
                type="number"
                value={props.chunksPerSource}
                onChange={(event) => props.onChunksPerSourceChange(event.target.value)}
              />
            </div>
          ) : null}
        </div>
      ) : null}
      <div className="grid gap-3 sm:grid-cols-3">
        {depthProperty ? (
          <div className="grid gap-2">
            <Label htmlFor="provider-tool-test-depth">
              {t("settings.providerTools.testDepth")}
            </Label>
            <Select value={props.extractDepth} onValueChange={props.onExtractDepthChange}>
              <SelectTrigger className="w-full" id="provider-tool-test-depth">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {depthValues.map((value) => (
                  <SelectItem key={value} value={value}>
                    {value === "basic"
                      ? t("settings.providerTools.testDepthBasic")
                      : value === "advanced"
                        ? t("settings.providerTools.testDepthAdvanced")
                        : value}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        ) : null}
        {formatProperty ? (
          <div className="grid gap-2">
            <Label htmlFor="provider-tool-test-format">
              {t("settings.providerTools.testFormat")}
            </Label>
            <Select value={props.extractFormat} onValueChange={props.onExtractFormatChange}>
              <SelectTrigger className="w-full" id="provider-tool-test-format">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {formatValues.map((value) => (
                  <SelectItem key={value} value={value}>
                    {value === "markdown"
                      ? t("settings.providerTools.testFormatMarkdown")
                      : value === "text"
                        ? t("settings.providerTools.testFormatText")
                        : value}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        ) : null}
        {timeoutProperty ? (
          <div className="grid gap-2">
            <Label htmlFor="provider-tool-test-timeout">
              {t("settings.providerTools.testTimeoutSeconds")}
            </Label>
            <Input
              id="provider-tool-test-timeout"
              max={timeoutProperty.maximum}
              min={timeoutProperty.minimum}
              type="number"
              value={props.timeoutSeconds}
              onChange={(event) => props.onTimeoutSecondsChange(event.target.value)}
            />
          </div>
        ) : null}
      </div>
      <div className="flex flex-wrap gap-5">
        {schemaProperty(props.target, "include_images") ? (
          <label className="flex items-center gap-2 text-sm">
            <Switch
              checked={props.includeImages}
              onCheckedChange={props.onIncludeImagesChange}
            />
            {t("settings.providerTools.testIncludeImages")}
          </label>
        ) : null}
        {schemaProperty(props.target, "include_favicon") ? (
          <label className="flex items-center gap-2 text-sm">
            <Switch
              checked={props.includeFavicon}
              onCheckedChange={props.onIncludeFaviconChange}
            />
            {t("settings.providerTools.testIncludeFavicon")}
          </label>
        ) : null}
      </div>
    </>
  );
}

// ProviderToolTestResultsProps contains one normalized diagnostic response.
// ProviderToolTestResultsProps 包含一份规范化诊断响应。
interface ProviderToolTestResultsProps {
  // result is the normalized provider diagnostic response.
  // result 是规范化供应商诊断响应。
  result: PublicAiProviderToolTestResult;
}

// ProviderToolTestResults renders the closed normalized Search or Extract output.
// ProviderToolTestResults 渲染封闭规范化 Search 或 Extract 输出。
function ProviderToolTestResults(props: ProviderToolTestResultsProps) {
  // t resolves result labels.
  // t 解析结果标签。
  const { t } = useI18n();
  // output is the discriminated normalized provider result.
  // output 是带判别字段的规范化供应商结果。
  const { output } = props.result;
  return (
    <section
      aria-label={t("settings.providerTools.testResults")}
      className="grid max-h-[min(45vh,360px)] gap-3 overflow-y-auto border-t pt-4"
    >
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="font-semibold">{t("settings.providerTools.testResults")}</h3>
        {output.kind === "search" ? (
          <Badge variant="outline">{output.value.results.length}</Badge>
        ) : (
          <>
            <Badge variant="outline">
              {output.value.results.length} {t("settings.providerTools.testSucceeded")}
            </Badge>
            {output.value.failures.length ? (
              <Badge variant="destructive">
                {output.value.failures.length} {t("settings.providerTools.testFailed")}
              </Badge>
            ) : null}
          </>
        )}
      </div>
      {output.kind === "search" ? (
        output.value.results.length ? (
          <ol className="grid gap-2">
            {output.value.results.map((item) => (
              <li className="rounded-lg border p-3" key={`${item.rank}:${item.url}`}>
                <div className="flex items-start gap-2">
                  <span className="text-xs tabular-nums text-muted-foreground">{item.rank}</span>
                  <div className="min-w-0">
                    <a
                      className="font-medium underline-offset-4 hover:underline"
                      href={item.url}
                      rel="noreferrer"
                      target="_blank"
                    >
                      {item.title || item.url}
                    </a>
                    {item.source_domain ? (
                      <p className="text-xs text-muted-foreground">{item.source_domain}</p>
                    ) : null}
                    {item.snippet ? (
                      <p className="mt-1 whitespace-pre-wrap text-sm">{item.snippet}</p>
                    ) : null}
                  </div>
                </div>
              </li>
            ))}
          </ol>
        ) : (
          <p className="text-sm text-muted-foreground">
            {t("settings.providerTools.testNoResults")}
          </p>
        )
      ) : (
        <>
          {output.value.results.map((item) => (
            <article className="grid gap-2 rounded-lg border p-3" key={item.url}>
              <a
                className="truncate font-medium underline-offset-4 hover:underline"
                href={item.url}
                rel="noreferrer"
                target="_blank"
              >
                {item.url}
              </a>
              <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-md bg-muted p-3 text-xs">
                {item.content}
              </pre>
              {item.images.length ? (
                <div className="flex flex-wrap gap-2">
                  {item.images.map((imageUrl) => (
                    <a
                      className="text-xs underline"
                      href={imageUrl}
                      key={imageUrl}
                      rel="noreferrer"
                      target="_blank"
                    >
                      {imageUrl}
                    </a>
                  ))}
                </div>
              ) : null}
            </article>
          ))}
          {output.value.failures.map((item) => (
            <article className="rounded-lg border border-destructive/40 p-3 text-sm" key={item.url}>
              <p className="font-medium">{item.url}</p>
              <p className="text-destructive">{item.error}</p>
            </article>
          ))}
          {!output.value.results.length && !output.value.failures.length ? (
            <p className="text-sm text-muted-foreground">
              {t("settings.providerTools.testNoResults")}
            </p>
          ) : null}
        </>
      )}
      {props.result.upstream_response_id ? (
        <p className="text-xs text-muted-foreground">
          {t("settings.providerTools.testProviderRequestId")}:{" "}
          {props.result.upstream_response_id}
        </p>
      ) : null}
    </section>
  );
}

// buildSearchArguments projects only schema-declared Search fields.
// buildSearchArguments 仅投影 Schema 声明的 Search 字段。
function buildSearchArguments(
  target: PublicAiProviderToolTargetSnapshot,
  query: string,
  maxResults: string,
  includeDomains: string,
  excludeDomains: string,
): Record<string, unknown> {
  // argumentsValue begins with the required shared Search query.
  // argumentsValue 以必填共享 Search 查询开始。
  const argumentsValue: Record<string, unknown> = { query: query.trim() };
  if (schemaProperty(target, "max_results") && maxResults !== "") {
    argumentsValue.max_results = Number(maxResults);
  }
  if (schemaProperty(target, "include_domains")) {
    // values is the exact non-empty include-domain list.
    // values 是精确的非空包含域名列表。
    const values = splitNonEmptyLines(includeDomains);
    if (values.length) {
      argumentsValue.include_domains = values;
    }
  }
  if (schemaProperty(target, "exclude_domains")) {
    // values is the exact non-empty exclude-domain list.
    // values 是精确的非空排除域名列表。
    const values = splitNonEmptyLines(excludeDomains);
    if (values.length) {
      argumentsValue.exclude_domains = values;
    }
  }
  return argumentsValue;
}

// buildExtractArguments projects only schema-declared Extract fields.
// buildExtractArguments 仅投影 Schema 声明的 Extract 字段。
function buildExtractArguments(
  target: PublicAiProviderToolTargetSnapshot,
  urls: string[],
  query: string,
  chunksPerSource: string,
  depth: string,
  format: string,
  timeoutSeconds: string,
  includeImages: boolean,
  includeFavicon: boolean,
): Record<string, unknown> {
  // argumentsValue begins with the required exact URL list.
  // argumentsValue 以必填精确 URL 列表开始。
  const argumentsValue: Record<string, unknown> = { urls };
  if (schemaProperty(target, "query") && query.trim()) {
    argumentsValue.query = query.trim();
  }
  if (schemaProperty(target, "chunks_per_source") && query.trim() && chunksPerSource !== "") {
    argumentsValue.chunks_per_source = Number(chunksPerSource);
  }
  if (schemaProperty(target, "extract_depth")) {
    argumentsValue.extract_depth = depth;
  }
  if (schemaProperty(target, "format")) {
    argumentsValue.format = format;
  }
  if (schemaProperty(target, "timeout_seconds") && timeoutSeconds !== "") {
    argumentsValue.timeout_seconds = Number(timeoutSeconds);
  }
  if (schemaProperty(target, "include_images")) {
    argumentsValue.include_images = includeImages;
  }
  if (schemaProperty(target, "include_favicon")) {
    argumentsValue.include_favicon = includeFavicon;
  }
  return argumentsValue;
}

// schemaProperty returns one exact property from the selected target contract.
// schemaProperty 从所选目标契约返回一个精确属性。
function schemaProperty(
  target: PublicAiProviderToolTargetSnapshot | null,
  name: string,
): JsonSchemaProperty | null {
  if (!target) {
    return null;
  }
  // properties is the contract-authored object-property map.
  // properties 是契约编写的对象属性映射。
  const properties = target.target.contract.input_schema.properties;
  if (!properties || typeof properties !== "object" || Array.isArray(properties)) {
    return null;
  }
  // value is the exact named schema node.
  // value 是精确命名的 Schema 节点。
  const value = (properties as Record<string, unknown>)[name];
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as JsonSchemaProperty
    : null;
}

// schemaEnumValues returns only exact string enum members.
// schemaEnumValues 仅返回精确字符串枚举成员。
function schemaEnumValues(property: JsonSchemaProperty | null): string[] {
  return property?.enum?.filter((value): value is string => typeof value === "string") ?? [];
}

// schemaDefaultNumber returns one finite numeric schema default.
// schemaDefaultNumber 返回一个有限数值 Schema 默认值。
function schemaDefaultNumber(property: JsonSchemaProperty | null): number | null {
  return typeof property?.default === "number" && Number.isFinite(property.default)
    ? property.default
    : null;
}

// schemaDefaultString returns one non-empty string schema default.
// schemaDefaultString 返回一个非空字符串 Schema 默认值。
function schemaDefaultString(property: JsonSchemaProperty | null): string | null {
  return typeof property?.default === "string" && property.default !== ""
    ? property.default
    : null;
}

// isValidHttpsUrl rejects malformed addresses before a real quota-consuming diagnostic dispatch.
// isValidHttpsUrl 在实际消耗配额的诊断分派前拒绝格式错误的地址。
// Parameters: value is the raw URL draft; returns true only for a parseable HTTPS URL.
// 参数：value 是原始 URL 草稿；仅对可解析的 HTTPS URL 返回 true。
function isValidHttpsUrl(value: string): boolean {
  try { const url = new URL(value); return url.protocol === "https:" && url.hostname.length > 0; }
  catch { return false; }
}

// splitNonEmptyLines normalizes one operator-entered newline list.
// splitNonEmptyLines 规范化一个操作员输入的换行列表。
function splitNonEmptyLines(value: string): string[] {
  return value
    .split(/\r?\n/)
    .map((item) => item.trim())
    .filter(Boolean);
}
