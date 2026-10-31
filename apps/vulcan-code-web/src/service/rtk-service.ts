import { serviceClient } from "@/service/webui-service-runtime";
import type { RtkSettings, RtkSettingsSnapshot, RtkStatisticsSnapshot } from "@/types";

// loadRtkSettings reads the saved RTK settings and the offered route catalog.
// loadRtkSettings 读取已保存的 RTK 设置与可选路由目录。
export async function loadRtkSettings(): Promise<RtkSettingsSnapshot> {
  return serviceClient.request<RtkSettingsSnapshot>("service_rtk_settings", {});
}

// saveRtkSettings replaces the RTK settings; the next turn applies them.
// saveRtkSettings 替换 RTK 设置；下一回合生效。
export async function saveRtkSettings(settings: RtkSettings): Promise<RtkSettingsSnapshot> {
  return serviceClient.request<RtkSettingsSnapshot>("service_save_rtk_settings", { request: { settings } });
}

// loadRtkStatistics reads the RTK token-saving statistics; RTK failures come back as unavailable.
// loadRtkStatistics 读取 RTK Token 节省统计；RTK 失败以不可用结果返回。
export async function loadRtkStatistics(): Promise<RtkStatisticsSnapshot> {
  return serviceClient.request<RtkStatisticsSnapshot>("service_rtk_statistics", {});
}

// clearRtkStatistics clears the RTK statistics history and returns the refreshed statistics.
// clearRtkStatistics 清空 RTK 统计历史并返回刷新后的统计。
export async function clearRtkStatistics(): Promise<RtkStatisticsSnapshot> {
  return serviceClient.request<RtkStatisticsSnapshot>("service_clear_rtk_statistics", {});
}
