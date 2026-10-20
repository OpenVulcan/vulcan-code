import React from "react";
import { Check, RefreshCw, Route, Search } from "lucide-react";
import { toast } from "@/ui-kit/primitives/toast";
import { Button } from "@/ui-kit/primitives/button";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/ui-kit/primitives/dropdown-menu";
import { Input } from "@/ui-kit/primitives/input";
import { ScrollArea } from "@/ui-kit/primitives/scroll-area";
import { useI18n } from "@/i18n";
import type { ProviderModelSecondaryRouteSelection, ProviderModelSecondaryRouteSummary } from "@/types";

// secondaryRouteIdentity returns the stable identity used to reconcile optimistic and authoritative selections.
// secondaryRouteIdentity 返回用于协调乐观选择与权威选择的稳定标识。
function secondaryRouteIdentity(route: ProviderModelSecondaryRouteSummary | null): string {
  return route?.routeId.trim() ?? "";
}

// ModelSecondaryRouteControl renders a compact provider-managed route picker beside the model selector.
// ModelSecondaryRouteControl 在模型选择器旁渲染紧凑的 Provider 托管路由选择器。
export function ModelSecondaryRouteControl(props: {
  // Stable provider identifier.
  // 稳定 Provider 标识。
  providerId: string;
  // Stable model identifier.
  // 稳定模型标识。
  modelId: string;
  // Currently selected secondary route.
  // 当前选择的二级路由。
  selectedRoute: ProviderModelSecondaryRouteSummary | null;
  // Loads route choices.
  // 加载路由选项。
  onListRoutes: (
    providerId: string,
    modelId: string,
    forceRefresh: boolean,
  ) => Promise<ProviderModelSecondaryRouteSummary[]>;
  // Persists one route selection.
  // 持久化单个路由选择。
  onSelectRoute: (
    providerId: string,
    modelId: string,
    route: ProviderModelSecondaryRouteSelection | null,
  ) => Promise<boolean>;
}) {
  // t and locale resolve stable labels and bounded diagnostic summaries in the active language.
  // t 与 locale 根据当前语言解析稳定标签和简短诊断摘要。
  const { t, locale } = useI18n();
  const [open, setOpen] = React.useState(false);
  const [filter, setFilter] = React.useState("");
  const [loading, setLoading] = React.useState(false);
  const [routes, setRoutes] = React.useState<ProviderModelSecondaryRouteSummary[]>([]);
  // saving and selectionError expose the bounded lifetime and failure of one selection write.
  // saving 与 selectionError 展示单次选择写入的有界生命周期和失败。
  const [saving, setSaving] = React.useState(false);
  const [selectionError, setSelectionError] = React.useState("");
  // loadError distinguishes failed discovery from a successful empty or filtered result.
  // loadError 区分发现失败与成功返回的空列表或筛选结果。
  const [loadError, setLoadError] = React.useState("");
  // ownerGeneration and listRequestId invalidate responses across model changes, closure, and unmount.
  // ownerGeneration 与 listRequestId 在模型切换、关闭和卸载时使旧响应失效。
  const ownerGeneration = React.useRef(0);
  const listRequestId = React.useRef(0);
  const listPending = React.useRef(false);
  // selectionWrite synchronously rejects duplicate choices without blocking a different model owner.
  // selectionWrite 同步拒绝重复选择，同时不阻塞其他模型归属。
  const selectionWrite = React.useRef<object | null>(null);
  // menuRef connects search, refresh and native menu keyboard navigation inside this exact menu.
  // menuRef 在此精确菜单内关联搜索、刷新及原生菜单键盘导航。
  const menuRef = React.useRef<HTMLDivElement>(null);
  // optimisticRoute is displayed immediately while the backend persists the authoritative selection.
  // optimisticRoute 在后端持久化权威选择期间立即用于界面显示。
  const [optimisticRoute, setOptimisticRoute] = React.useState<ProviderModelSecondaryRouteSummary | null>(null);
  // optimisticRoutePending distinguishes an optimistic Auto selection from the absence of optimistic state.
  // optimisticRoutePending 区分乐观 Auto 选择与不存在乐观状态。
  const [optimisticRoutePending, setOptimisticRoutePending] = React.useState(false);
  // onListRoutesRef keeps an in-flight endpoint discovery independent from parent callback identity changes.
  // onListRoutesRef 使进行中的端点发现不受父组件回调引用变化影响。
  const onListRoutesRef = React.useRef(props.onListRoutes);
  // translateRef keeps active copy current without restarting an in-flight discovery.
  // translateRef 保持当前文案最新，同时不重启进行中的发现请求。
  const translateRef = React.useRef(t);
  // effectiveSelectedRoute prefers the instant optimistic selection until the service snapshot catches up.
  // effectiveSelectedRoute 在服务快照追上前优先使用即时乐观选择。
  const effectiveSelectedRoute = optimisticRoutePending ? optimisticRoute : props.selectedRoute;
  const selectedLabel = effectiveSelectedRoute?.label?.trim() || "Auto";
  const filteredRoutes = React.useMemo(() => {
    const needle = filter.trim().toLowerCase();
    if (!needle) {
      return routes;
    }
    return routes.filter((route) => (
      route.label.toLowerCase().includes(needle)
      || route.subtitle.toLowerCase().includes(needle)
      || route.tag.toLowerCase().includes(needle)
      || route.endpointName.toLowerCase().includes(needle)
    ));
  }, [filter, routes]);

  React.useEffect(() => {
    onListRoutesRef.current = props.onListRoutes;
    translateRef.current = t;
  }, [props.onListRoutes, t]);

  React.useEffect(() => {
    if (
      optimisticRoutePending
      && secondaryRouteIdentity(props.selectedRoute) === secondaryRouteIdentity(optimisticRoute)
    ) {
      setOptimisticRoutePending(false);
    }
  }, [optimisticRoute, optimisticRoutePending, props.selectedRoute]);

  React.useLayoutEffect(() => {
    ownerGeneration.current += 1;
    listRequestId.current += 1;
    listPending.current = false;
    selectionWrite.current = null;
    setRoutes([]);
    setLoading(false);
    setSaving(false);
    setLoadError("");
    setSelectionError("");
    setFilter("");
    setOptimisticRoute(null);
    setOptimisticRoutePending(false);
    return () => {
      ownerGeneration.current += 1;
      listRequestId.current += 1;
      listPending.current = false;
      selectionWrite.current = null;
    };
  }, [props.modelId, props.providerId]);

  // loadRoutes reads the exact current owner and accepts only its latest still-open request.
  // loadRoutes 读取精确当前归属，仅接受其仍打开的最新请求。
  // forceRefresh bypasses the declared service cache; returns after success or visible failure.
  // forceRefresh 绕过已声明服务缓存；在成功或可见失败后返回。
  const loadRoutes = React.useCallback(async (forceRefresh: boolean) => {
    if (forceRefresh && listPending.current) return;
    // generation and requestId bind the eventual result to this exact open owner and request.
    // generation 与 requestId 将最终结果绑定到此精确打开归属和请求。
    const generation = ownerGeneration.current;
    const requestId = ++listRequestId.current;
    listPending.current = true;
    setLoading(true);
    setLoadError("");
    try {
      const nextRoutes = await onListRoutesRef.current(props.providerId, props.modelId, forceRefresh);
      if (generation !== ownerGeneration.current || requestId !== listRequestId.current) return;
      setRoutes(nextRoutes);
      if (forceRefresh) toast.success(translateRef.current("composer.secondaryRoute.refreshed"));
    } catch (error) {
      if (generation !== ownerGeneration.current || requestId !== listRequestId.current) return;
      setLoadError(error instanceof Error ? error.message : String(error));
    } finally {
      if (generation === ownerGeneration.current && requestId === listRequestId.current) {
        listPending.current = false;
        setLoading(false);
      }
    }
  }, [props.modelId, props.providerId]);

  React.useEffect(() => {
    if (open) void loadRoutes(false);
    else { setFilter(""); setLoading(false); }
    return () => {
      listRequestId.current += 1;
      listPending.current = false;
    };
  }, [loadRoutes, open]);

  // chooseRoute serializes one optimistic write and rolls back only its own current model on failure.
  // chooseRoute 串行执行一次乐观写入，失败时仅回滚其自身的当前模型。
  // route is a discovered route or explicit Auto; returns after the boolean/rejection contract settles.
  // route 是已发现路由或显式 Auto；在布尔结果或拒绝契约结束后返回。
  const chooseRoute = React.useCallback(async (route: ProviderModelSecondaryRouteSummary | null) => {
    if (selectionWrite.current || secondaryRouteIdentity(route) === secondaryRouteIdentity(effectiveSelectedRoute)) return;
    // write and generation prevent an old completion from releasing a newer model's write guard.
    // write 与 generation 防止旧请求结束时释放新模型的写入守卫。
    const write = {};
    const generation = ownerGeneration.current;
    selectionWrite.current = write;
    setSaving(true);
    setSelectionError("");
    setOptimisticRoute(route);
    setOptimisticRoutePending(true);
    setOpen(false);
    try {
      const ok = await props.onSelectRoute(props.providerId, props.modelId, route ? {
        routeId: route.routeId, tag: route.tag, providerName: route.providerName, endpointName: route.endpointName,
      } : null);
      if (generation !== ownerGeneration.current || selectionWrite.current !== write) return;
      if (!ok) {
        setOptimisticRoutePending(false);
        setSelectionError(t("composer.secondaryRoute.routeSelectionWasNot"));
      }
    } catch (error) {
      if (generation !== ownerGeneration.current || selectionWrite.current !== write) return;
      setOptimisticRoutePending(false);
      setSelectionError(error instanceof Error ? error.message : String(error));
      toast.error(t("composer.secondaryRoute.routeSelectionWasNot"));
    } finally {
      if (generation === ownerGeneration.current && selectionWrite.current === write) {
        selectionWrite.current = null;
        setSaving(false);
      }
    }
  }, [effectiveSelectedRoute, locale, props.modelId, props.onSelectRoute, props.providerId]);

  return (
    <DropdownMenu open={open} onOpenChange={(nextOpen) => {
      if (nextOpen && selectionWrite.current) return;
      if (!nextOpen) { listRequestId.current += 1; listPending.current = false; }
      setOpen(nextOpen);
    }}>
      <DropdownMenuTrigger asChild>
        <button
          aria-label={t("composer.secondaryRoute.select")}
          aria-busy={saving}
          disabled={saving}
          title={selectionError || selectedLabel}
          className="vk-pick vk-route-pick"
          type="button"
        >
          <Route size={14} />
          <span>{selectedLabel}</span>
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent ref={menuRef} align="start" className="vk-route-pop" sideOffset={8}
        onKeyDown={(event) => {
          if (event.key !== "Tab") return;
          event.preventDefault();
          // Include the native diagnostic disclosure in the explicit menu focus cycle.
          // 将原生诊断详情折叠控件纳入显式菜单焦点循环。
          const controls = [...event.currentTarget.querySelectorAll<HTMLElement>('input:not(:disabled), button:not(:disabled), summary')];
          const index = controls.indexOf(document.activeElement as HTMLElement);
          controls[(index + (event.shiftKey ? controls.length - 1 : 1)) % controls.length]?.focus();
        }}>
        <div className="vk-model-filter">
          <Search size={14} />
          <Input
            aria-label={t("composer.secondaryRoute.search")}
            autoFocus
            onChange={(event) => setFilter(event.currentTarget.value)}
            placeholder={t("composer.secondaryRoute.search")}
            value={filter}
            onKeyDown={(event) => {
              if (event.key === "Escape" || event.key === "Tab") return;
              event.stopPropagation();
              if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                event.preventDefault();
                const items = menuRef.current?.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]:not(:disabled)');
                items?.[event.key === "ArrowDown" ? 0 : items.length - 1]?.focus();
              }
            }}
          />
          <Button
            aria-label={t("composer.secondaryRoute.refresh")}
            disabled={loading}
            onClick={() => void loadRoutes(true)}
            size="icon"
            type="button"
            variant="ghost"
          >
            <RefreshCw className={loading ? "vk-spin" : undefined} size={14} />
          </Button>
        </div>
        <ScrollArea className="vk-model-list">
        {loadError || selectionError ? <details role="alert" className="vk-route-failure">
          <summary>{t("composer.secondaryRoute.routeActionFailedRetry")}</summary>
          <pre>{loadError || selectionError}</pre>
        </details> : null}
        {loading ? <StreamPlaceholder label={t("composer.secondaryRoute.loading")} shape="rows" count={3} /> : null}
          <div className="vk-model-rows">
            <DropdownMenuItem asChild onSelect={() => void chooseRoute(null)} textValue="Auto">
            <button
              className="vk-model-row"
              data-current={!effectiveSelectedRoute || undefined}
              role="menuitemradio" aria-checked={!effectiveSelectedRoute}
              disabled={saving}
              type="button"
            >
              <span className="vk-model-check">{!effectiveSelectedRoute ? <Check size={15} /> : null}</span>
              <span className="vk-model-copy">
                <span className="vk-model-title">Auto</span>
                <span className="vk-model-meta">{t("composer.secondaryRoute.autoEndpoint")}</span>
              </span>
            </button>
            </DropdownMenuItem>
            {filteredRoutes.length === 0 ? (
              !loading && !loadError ? <div role="status" className="vk-model-none">{filter.trim() ? t("composer.secondaryRoute.noMatches") : t("composer.secondaryRoute.noSecondaryProvidersAre")}</div> : null
            ) : (
              filteredRoutes.map((route) => {
                // selected identifies the route currently visible to the user, including optimistic state.
                // selected 标识当前向用户显示的路由，并包含乐观状态。
                const selected = secondaryRouteIdentity(route) === secondaryRouteIdentity(effectiveSelectedRoute);
                return (
                  <DropdownMenuItem asChild key={route.routeId} textValue={route.label} onSelect={() => void chooseRoute(route)}>
                  <button
                    className="vk-model-row"
                    data-current={selected || undefined}
                    role="menuitemradio" aria-checked={selected}
                    disabled={saving}
                    type="button"
                  >
                    <span className="vk-model-check">{selected ? <Check size={15} /> : null}</span>
                    <span className="vk-model-copy">
                      <span className="vk-model-title">
                        {route.label}
                        {route.tag ? <span className="vk-model-tag">（{route.tag}）</span> : null}
                      </span>
                      <span className="vk-model-meta">{route.subtitle}</span>
                    </span>
                  </button>
                  </DropdownMenuItem>
                );
              })
            )}
          </div>
        </ScrollArea>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

