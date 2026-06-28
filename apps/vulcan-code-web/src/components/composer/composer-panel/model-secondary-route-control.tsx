import React from "react";
import { Check, Route, Search } from "lucide-react";
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
  onListRoutes: (providerId: string, modelId: string) => Promise<ProviderModelSecondaryRouteSummary[]>;
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
  const selectedLabel = props.selectedRoute?.label?.trim() || "Auto";
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
    if (!open) {
      setFilter("");
      return;
    }
    let cancelled = false;
    setLoading(true);
    props.onListRoutes(props.providerId, props.modelId)
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
  }, [open, props.modelId, props.onListRoutes, props.providerId]);

  const chooseRoute = React.useCallback(async (route: ProviderModelSecondaryRouteSummary | null) => {
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
      toast.error(t("composer.secondaryRoute.switchFailed"));
      return;
    }
    setOpen(false);
  }, [props, t]);

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
        </div>
        <ScrollArea className="model-picker-scroll">
          <div className="model-picker-list">
            <button
              className={cn("model-picker-row", !props.selectedRoute && "selected")}
              onClick={() => void chooseRoute(null)}
              type="button"
            >
              <span className="model-picker-selected-slot">{!props.selectedRoute ? <Check size={15} /> : null}</span>
              <span className="model-picker-row-copy">
                <span className="model-picker-row-title">Auto</span>
                <span className="model-picker-row-meta">{t("composer.secondaryRoute.autoEndpoint")}</span>
              </span>
            </button>
            {loading ? (
              <div className="model-picker-empty">{t("composer.secondaryRoute.loading")}</div>
            ) : filteredRoutes.length === 0 ? (
              <div className="model-picker-empty">{t("composer.secondaryRoute.noMatches")}</div>
            ) : (
              filteredRoutes.map((route) => (
                <button
                  className={cn("model-picker-row", route.active && "selected")}
                  key={route.routeId}
                  onClick={() => void chooseRoute(route)}
                  type="button"
                >
                  <span className="model-picker-selected-slot">{route.active ? <Check size={15} /> : null}</span>
                  <span className="model-picker-row-copy">
                    <span className="model-picker-row-title">
                      {route.label}
                      {route.tag ? <span className="secondary-route-title-tag">（{route.tag}）</span> : null}
                    </span>
                    <span className="model-picker-row-meta">{route.subtitle}</span>
                  </span>
                </button>
              ))
            )}
          </div>
        </ScrollArea>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

