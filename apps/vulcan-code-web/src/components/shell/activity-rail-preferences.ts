// The GUI-local rail layout only chooses which navigation entries are pinned; it never changes service state.
// 此 GUI 本地轨道布局只决定哪些导航入口被固定，不改变任何服务端状态。
const ACTIVITY_RAIL_PINS_KEY = "vulcan.gui.activity-rail-pins.v1";

// DEFAULT_ACTIVITY_RAIL_PINS is the in-memory product default used until the user changes the rail.
// DEFAULT_ACTIVITY_RAIL_PINS 是用户修改轨道前使用的内存产品默认值。
export const DEFAULT_ACTIVITY_RAIL_PINS: readonly string[] = ["skills", "agent-skills", "agents", "memory", "knowledge"];

// readActivityRailPins reads the explicit GUI choice, falling back to the default without writing.
// readActivityRailPins 读取 GUI 的明确选择；未设置或无效时回退默认值，不回写存储。
// Takes no arguments; returns pinned entry ids, possibly including entries that are currently unavailable.
// 无参数；返回已固定的入口标识，其中可能包含当前不可用的入口。
export function readActivityRailPins(): readonly string[] {
  try {
    const stored = window.localStorage.getItem(ACTIVITY_RAIL_PINS_KEY);
    if (stored === null) {
      return DEFAULT_ACTIVITY_RAIL_PINS;
    }
    const parsed: unknown = JSON.parse(stored);
    return Array.isArray(parsed) && parsed.every((id) => typeof id === "string") ? parsed : DEFAULT_ACTIVITY_RAIL_PINS;
  } catch {
    // An unavailable or corrupt local store keeps the in-memory default; saves still report errors.
    // 本地存储不可用或已损坏时使用内存默认值；保存操作仍明确报告错误。
    return DEFAULT_ACTIVITY_RAIL_PINS;
  }
}

// saveActivityRailPins persists an explicit pin choice made by the user.
// saveActivityRailPins 持久化用户做出的明确固定选择。
// pins is the complete pinned id list; returns after storage succeeds, or throws the storage error.
// pins 是完整的固定标识列表；存储成功后返回，失败则抛出存储错误。
export function saveActivityRailPins(pins: readonly string[]): void {
  window.localStorage.setItem(ACTIVITY_RAIL_PINS_KEY, JSON.stringify(pins));
}
