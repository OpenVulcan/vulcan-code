import React from "react";
import { Check, RefreshCw, Route, Search } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import type { ProviderModelSecondaryRouteSelection, ProviderModelSecondaryRouteSummary } from "@/types";
// parseModelPickerChoiceKey splits only the provider prefix because model ids may contain slashes.
// parseModelPickerChoiceKey 只拆分 Provider 前缀，因为模型标识本身可能包含斜杠。
export function parseModelPickerChoiceKey(value: string): { providerId: string; modelId: string } | null {
  const separatorIndex = value.indexOf("/");
  if (separatorIndex <= 0 || separatorIndex >= value.length - 1) {
    return null;
  }
  return {
    providerId: value.slice(0, separatorIndex),
    modelId: value.slice(separatorIndex + 1),
  };
}

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
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const [open, setOpen] = React.useState(false);
  const [filter, setFilter] = React.useState("");
  const [loading, setLoading] = React.useState(false);
  const [routes, setRoutes] = React.useState<ProviderModelSecondaryRouteSummary[]>([]);
  // optimisticRoute is displayed immediately while the backend persists the authoritative selection.
  // optimisticRoute 在后端持久化权威选择期间立即用于界面显示。
  const [optimisticRoute, setOptimisticRoute] = React.useState<ProviderModelSecondaryRouteSummary | null>(null);
  // optimisticRoutePending distinguishes an optimistic Auto selection from the absence of optimistic state.
  // optimisticRoutePending 区分乐观 Auto 选择与不存在乐观状态。
  const [optimisticRoutePending, setOptimisticRoutePending] = React.useState(false);
  // onListRoutesRef keeps an in-flight endpoint discovery independent from parent callback identity changes.
  // onListRoutesRef 使进行中的端点发现不受父组件回调引用变化影响。
  const onListRoutesRef = React.useRef(props.onListRoutes);
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
  }, [props.onListRoutes]);

  React.useEffect(() => {
    if (
      optimisticRoutePending
      && secondaryRouteIdentity(props.selectedRoute) === secondaryRouteIdentity(optimisticRoute)
    ) {
      setOptimisticRoutePending(false);
    }
  }, [optimisticRoute, optimisticRoutePending, props.selectedRoute]);

  React.useEffect(() => {
    setOptimisticRoute(null);
    setOptimisticRoutePending(false);
  }, [props.modelId, props.providerId]);

  React.useEffect(() => {
    if (!open) {
      setFilter("");
      return;
    }
    let cancelled = false;
    setLoading(true);
    onListRoutesRef.current(props.providerId, props.modelId, false)
      .then((nextRoutes) => {
        if (!cancelled) {
          setRoutes(nextRoutes);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          toast.error(error instanceof Error ? error.message : String(error));
          setRoutes([]);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [open, props.modelId, props.providerId]);

  // refreshRoutes explicitly bypasses the six-hour runtime cache while retaining current rows on failure.
  // refreshRoutes 显式绕过六小时运行时缓存，并在失败时保留当前列表。
  const refreshRoutes = React.useCallback(async () => {
    setLoading(true);
    try {
      const nextRoutes = await onListRoutesRef.current(props.providerId, props.modelId, true);
      setRoutes(nextRoutes);
      toast.success(t("composer.secondaryRoute.refreshed"));
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setLoading(false);
    }
  }, [props.modelId, props.providerId, t]);

  const chooseRoute = React.useCallback(async (route: ProviderModelSecondaryRouteSummary | null) => {
    setOptimisticRoute(route);
    setOptimisticRoutePending(true);
    setOpen(false);
    const ok = await props.onSelectRoute(
      props.providerId,
      props.modelId,
      route ? {
        routeId: route.routeId,
        tag: route.tag,
        providerName: route.providerName,
        endpointName: route.endpointName,
      } : null,
    );
    if (!ok) {
      setOptimisticRoutePending(false);
    }
  }, [props]);

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <Button
          aria-label={t("composer.secondaryRoute.select")}
          className="composer-secondary-route-trigger"
          type="button"
          variant="outline"
        >
          <Route size={14} />
          <span>{selectedLabel}</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="model-picker-content secondary-route-picker-content" sideOffset={8}>
        <div className="model-picker-filter">
          <Search size={14} />
          <Input
            aria-label={t("composer.secondaryRoute.search")}
            autoFocus
            onChange={(event) => setFilter(event.currentTarget.value)}
            placeholder={t("composer.secondaryRoute.search")}
            value={filter}
          />
          <Button
            aria-label={t("composer.secondaryRoute.refresh")}
            disabled={loading}
            onClick={() => void refreshRoutes()}
            size="icon"
            type="button"
            variant="ghost"
          >
            <RefreshCw className={cn(loading && "animate-spin")} size={14} />
          </Button>
        </div>
        <ScrollArea className="model-picker-scroll">
          <div className="model-picker-list">
            <button
              className={cn("model-picker-row", !effectiveSelectedRoute && "selected")}
              onClick={() => void chooseRoute(null)}
              type="button"
            >
              <span className="model-picker-selected-slot">{!effectiveSelectedRoute ? <Check size={15} /> : null}</span>
              <span className="model-picker-row-copy">
                <span className="model-picker-row-title">Auto</span>
                <span className="model-picker-row-meta">{t("composer.secondaryRoute.autoEndpoint")}</span>
              </span>
            </button>
            {loading && routes.length === 0 ? (
              <div className="model-picker-empty">{t("composer.secondaryRoute.loading")}</div>
            ) : filteredRoutes.length === 0 ? (
              <div className="model-picker-empty">{t("composer.secondaryRoute.noMatches")}</div>
            ) : (
              filteredRoutes.map((route) => {
                // selected identifies the route currently visible to the user, including optimistic state.
                // selected 标识当前向用户显示的路由，并包含乐观状态。
                const selected = secondaryRouteIdentity(route) === secondaryRouteIdentity(effectiveSelectedRoute);
                return (
                  <button
                    className={cn("model-picker-row", selected && "selected")}
                    key={route.routeId}
                    onClick={() => void chooseRoute(route)}
                    type="button"
                  >
                    <span className="model-picker-selected-slot">{selected ? <Check size={15} /> : null}</span>
                    <span className="model-picker-row-copy">
                      <span className="model-picker-row-title">
                        {route.label}
                        {route.tag ? <span className="secondary-route-title-tag">（{route.tag}）</span> : null}
                      </span>
                      <span className="model-picker-row-meta">{route.subtitle}</span>
                    </span>
                  </button>
                );
              })
            )}
          </div>
        </ScrollArea>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

