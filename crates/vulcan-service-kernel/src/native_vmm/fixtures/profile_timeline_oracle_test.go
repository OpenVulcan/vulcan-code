package usecase

import (
	"encoding/json"
	"os"
	"testing"
	"time"
	_ "time/tzdata"

	logicdomain "github.com/openvulcan/vmm/internal/logic/domain"
)

// parityProfileNode preserves the explicit cross-runtime input mapping without rendered expectations.
// parityProfileNode 保留明确的跨运行时输入映射，不包含手写渲染预期。
type parityProfileNode struct {
	// Content is the exact active fact supplied to both renderers.
	// Content 是交给两侧渲染器的准确活跃事实。
	Content string `json:"content"`
	// Anchor is the historical UTC instant; empty selects the declared creation fallback.
	// Anchor 是历史 UTC 时刻；空值选择已声明的创建时间备用来源。
	Anchor string `json:"anchor"`
	// Created is the creation instant used as the deterministic ordering tie breaker.
	// Created 是用于确定性平局排序的创建时刻。
	Created string `json:"created"`
	// Priority is the external P0/P1/P2 value, mapped explicitly by the native test.
	// Priority 是外置 P0/P1/P2 值，由原生测试显式映射。
	Priority int `json:"priority"`
	// Level is the shared L0/L1/L2/L3 lifetime label number.
	// Level 是共享 L0/L1/L2/L3 生命周期标签数字。
	Level int `json:"level"`
	// Weight records the exact refresh count.
	// Weight 记录准确刷新次数。
	Weight int `json:"weight"`
}

// parityProfileTimeline contains real production output for one explicit timezone and ordered input.
// parityProfileTimeline 包含一个明确时区及有序输入的真实生产输出。
type parityProfileTimeline struct {
	// Timezone identifies the historical calendar rules used by the external runtime.
	// Timezone 标识外置运行时使用的历史日历规则。
	Timezone string `json:"timezone"`
	// Nodes preserves the synthetic input, including deliberately unsorted facts.
	// Nodes 保留合成输入，包括刻意未排序的事实。
	Nodes []parityProfileNode `json:"nodes"`
	// Rendered is generated exclusively by the external production function.
	// Rendered 仅由外置生产函数生成。
	Rendered string `json:"rendered"`
}

// TestNativeParityProfileTimelineOracle exports exact body text for dates, DST, ordering and empty input.
// TestNativeParityProfileTimelineOracle 导出日期、夏令时、排序及空输入的准确正文。
// The explicit output path receives synthetic evidence; production source and user data are untouched.
// 明确输出路径接收合成证据；生产源码及用户数据不变。
func TestNativeParityProfileTimelineOracle(t *testing.T) {
	// original restores the process-local timezone after this serial-only oracle.
	// original 在仅串行的基准测试结束后恢复进程本地时区。
	original := time.Local
	t.Cleanup(func() { time.Local = original })
	// nodes exercise every priority/lifetime and opposite sides of historical local midnight.
	// nodes 覆盖全部优先级和生命周期，以及历史本地午夜两侧。
	nodes := []parityProfileNode{
		{Content: "ordinary 中文事实", Anchor: "2026-07-15T04:30:00Z", Created: "2026-01-01T00:00:00Z", Priority: 2, Level: 0},
		{Content: "冬季事实", Anchor: "2026-01-15T04:30:00Z", Created: "2026-01-01T00:00:00Z", Priority: 1, Level: 1},
		{Content: "confirmed", Anchor: "2026-07-15T04:30:00Z", Created: "2026-01-01T00:00:00Z", Priority: 1, Level: 2, Weight: 3},
		{Content: "hard later", Anchor: "2026-07-15T04:30:00Z", Created: "2026-01-02T00:00:00Z", Priority: 0, Level: 3, Weight: 1},
		{Content: "hard first", Anchor: "2026-07-15T04:30:00Z", Created: "2026-01-01T00:00:00Z", Priority: 0, Level: 3, Weight: 1},
		{Content: "before midnight", Anchor: "2026-09-06T15:59:59.999999999Z", Created: "2026-01-01T00:00:00Z", Priority: 1, Level: 2},
		{Content: "after midnight", Anchor: "2026-09-06T16:00:00Z", Created: "2026-01-01T00:00:00Z", Priority: 1, Level: 2},
		{Content: "creation anchor", Created: "2026-09-06T16:00:00Z", Priority: 1, Level: 2},
	}
	// fixtures retain empty and nonempty inputs for each historical timezone.
	// fixtures 为每个历史时区保留空与非空输入。
	fixtures := []parityProfileTimeline{}
	for _, timezone := range []string{"UTC", "Asia/Shanghai", "America/New_York"} {
		// location supplies the same named historical timezone used by the native Core query.
		// location 提供与原生 Core 查询相同名称的历史时区。
		location, err := time.LoadLocation(timezone)
		if err != nil {
			t.Fatal(err)
		}
		time.Local = location
		for _, count := range []int{0, len(nodes)} {
			// input is copied so fixture serialization cannot observe later order changes.
			// input 使用副本，避免样本序列化观察到后续顺序变更。
			input := append([]parityProfileNode{}, nodes[:count]...)
			// rows contain real domain fields, with stale display dates to verify timestamp authority.
			// rows 包含真实领域字段和过期显示日期，以验证时间戳权威性。
			rows := []logicdomain.ProfileActiveNodeRecord{}
			for index, node := range input {
				// created is an exact nanosecond instant parsed by the standard library.
				// created 是由标准库解析的准确纳秒时刻。
				created, err := time.Parse(time.RFC3339Nano, node.Created)
				if err != nil {
					t.Fatal(err)
				}
				// anchor is optional in the source domain and remains zero only when explicitly absent.
				// anchor 在来源领域中可选，只有明确缺失时才保留零值。
				var anchor time.Time
				if node.Anchor != "" {
					anchor, err = time.Parse(time.RFC3339Nano, node.Anchor)
					if err != nil {
						t.Fatal(err)
					}
				}
				rows = append(rows, logicdomain.ProfileActiveNodeRecord{ID: uint64(index + 1), Content: node.Content,
					Priority: node.Priority, ProfileLevel: node.Level, RefreshWeight: node.Weight,
					ProfileDate: "1999-01-01", ProfileDateAnchorAt: anchor, CreatedAt: created})
			}
			fixtures = append(fixtures, parityProfileTimeline{Timezone: timezone, Nodes: input, Rendered: renderProfileTimeline(rows, nil, nil)})
		}
	}
	// destination is required so the generator cannot silently overwrite a default location.
	// destination 为必填，避免生成器静默覆盖默认位置。
	destination := os.Getenv("VULCAN_PROFILE_TIMELINE_ORACLE_OUTPUT")
	if destination == "" {
		t.Fatal("VULCAN_PROFILE_TIMELINE_ORACLE_OUTPUT is required")
	}
	// output contains real external rendering results alongside the exact synthetic inputs.
	// output 包含真实外置渲染结果及准确合成输入。
	output, err := json.MarshalIndent(fixtures, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(destination, append(output, '\n'), 0600); err != nil {
		t.Fatal(err)
	}
	t.Logf("generated %d profile timeline cases", len(fixtures))
}
