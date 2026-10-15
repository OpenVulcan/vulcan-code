//! Verifies bounded CodeKit line previews remain pageable under the shared host budget.
//! 验证有界 CodeKit 长行预览在宿主共享预算下仍可分页。

use vulcan_host_tools::result_format::{
    DEFAULT_INLINE_TOOL_RESULT_BYTES, HostRenderOptions, HostTextRenderInput, HostToolOverflowMode,
    ToolResultBudgetSnapshot, render_tool_result_text,
};

/// Exercises many bounded long-line previews through the real page renderer; returns no value.
/// 将多条有界长行预览送入真实分页渲染器；无参数与返回值。
#[test]
fn codekit_long_line_previews_page_with_default_budget() {
    // CodeKit 0.2.5 bounds individual previews to 4 KiB; repeat them until the whole result must spill.
    // CodeKit 0.2.5 将单条预览限制为 4 KiB；重复至整体结果必须落盘。
    let content = format!("L1: {}\n", "x".repeat(4096)).repeat(64);
    // Own the spill directory for this render call so no shared runtime paths are changed.
    // 为本次渲染独占超限目录，不修改共享运行时路径。
    let spill = tempfile::tempdir().expect("test spill directory must be available");
    // Use actual UTF-8 size and line count, as the LuaSkills result boundary does.
    // 按 LuaSkills 结果边界的方式使用真实 UTF-8 字节数与行数。
    let output = HostTextRenderInput {
        content_bytes: content.len(),
        content_lines: content.lines().count(),
        content,
        overflow_mode: Some(HostToolOverflowMode::Page),
        template_hint: None,
    };
    // Reference the host's single budget definition rather than maintaining a second default.
    // 引用宿主唯一预算定义，不另行维护默认值。
    let budget = ToolResultBudgetSnapshot::uniform_bytes(DEFAULT_INLINE_TOOL_RESULT_BYTES);
    // Rendering must persist readable pages instead of rejecting a preview as an oversized line.
    // 渲染必须持久化可读分页，不能把预览误判为超限单行。
    let rendered = render_tool_result_text(
        &output,
        Some("vulcan-codekit"),
        &budget,
        &HostRenderOptions {
            spill_root: Some(spill.path().to_path_buf()),
            ..HostRenderOptions::default()
        },
    )
    .expect("bounded CodeKit previews must support the default page-read budget");
    assert!(rendered.len() as u64 <= DEFAULT_INLINE_TOOL_RESULT_BYTES);
    // Verify byte preservation rather than a presentation template snapshot.
    // 验证字节保留，不锁定展示模板快照。
    let files = std::fs::read_dir(spill.path())
        .expect("spill files must be enumerable")
        .collect::<Result<Vec<_>, _>>()
        .expect("spill entries must be readable");
    assert_eq!(files.len(), 1);
    assert_eq!(
        std::fs::read_to_string(files.first().expect("one spill file").path())
            .expect("spill must remain readable"),
        output.content
    );
}
