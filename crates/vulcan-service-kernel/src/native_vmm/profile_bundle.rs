//! Projects native profile views into the mutually exclusive public bundle modes.
//! 将原生画像视图投影为互斥的公开画像包模式。

use vulcan_memorymesh_core::{
    ProfileBundle, ProfileView, ProfileViewMode, ProfileViewSection, SectionRole,
};
use vulcan_vmm::{GetProfileBundleResponse, ProfileBundleMode};

/// Freezes the normalized bundle mode and explanation policy for one query and response.
/// 为一次查询及响应冻结归一化的画像包模式和说明策略。
pub(super) struct NativeProfileBundleFormat {
    /// Selects the actual Core rendering mode; unspecified requests resolve to Full.
    /// 选择实际 Core 渲染模式；未指定的请求归一为 Full。
    mode: ProfileViewMode,
    /// Enables explanations only in Full mode, matching the external contract.
    /// 仅在 Full 模式启用说明，与外置契约一致。
    include_explanations: bool,
}

impl NativeProfileBundleFormat {
    /// Normalizes the wire mode and optional explanation flag into one authoritative format.
    /// 将协议模式和可选说明开关归一为唯一权威格式。
    pub(super) fn new(mode: ProfileBundleMode, include_explanations: Option<bool>) -> Self {
        // split identifies the only wire mode that suppresses combined rendering.
        // split 标识协议中唯一禁止组合渲染的模式。
        let split = mode == ProfileBundleMode::ProfileBundleModeSplit;
        Self {
            mode: if split {
                ProfileViewMode::Split
            } else {
                ProfileViewMode::Full
            },
            include_explanations: !split && include_explanations.unwrap_or(false),
        }
    }

    /// Builds the Core view from authorized ordered sections using the frozen format.
    /// 使用冻结格式及已授权的有序分区构建 Core 视图。
    pub(super) fn view(&self, sections: Vec<ProfileViewSection>) -> ProfileView {
        ProfileView {
            mode: self.mode,
            sections: sections
                .into_iter()
                .map(|mut section| {
                    // Full rendering embeds the label itself; Split adds identity during projection.
                    // Full 渲染直接嵌入标签；Split 在投影阶段补充身份。
                    if self.mode == ProfileViewMode::Full
                        && section.role == SectionRole::Environment
                    {
                        section.display_label =
                            format!("{} ({})", section.display_label, section.scope_id.as_str());
                    }
                    section
                })
                .collect(),
            include_explanations: self.include_explanations,
            display_timezone: String::from("UTC"),
        }
    }

    /// Returns one public representation of the queried bundle and the caller's trace ID.
    /// 返回查询画像包的唯一公开表示以及调用方的追踪 ID。
    pub(super) fn response(
        &self,
        bundle: ProfileBundle,
        trace_id: String,
    ) -> GetProfileBundleResponse {
        // response deliberately leaves unsupported hierarchy and helper metadata empty.
        // response 有意将不支持的层级及辅助元数据留空。
        let mut response = GetProfileBundleResponse {
            mode: match self.mode {
                ProfileViewMode::Full => ProfileBundleMode::ProfileBundleModeFull,
                ProfileViewMode::Split => ProfileBundleMode::ProfileBundleModeSplit,
            },
            include_explanation: self.include_explanations,
            trace_id,
            ..GetProfileBundleResponse::default()
        };
        if self.mode == ProfileViewMode::Full {
            response.combined_text = bundle.rendered;
            return response;
        }
        for section in bundle.sections {
            if section.rendered.is_empty() {
                continue;
            }
            match section.role {
                SectionRole::Environment => {
                    if !response.project_profile.is_empty() {
                        response.project_profile.push_str("\n\n");
                    }
                    // Stable scope identity distinguishes linked projects even after renaming.
                    // 稳定作用域身份确保连带项目即使改名也可区分。
                    response.project_profile.push_str(&format!(
                        "[{} ({})]\n{}",
                        section.display_label,
                        section.scope_id.as_str(),
                        section.rendered
                    ));
                }
                SectionRole::Subject => response.user_profile = section.rendered,
            }
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_memorymesh_core::{ProfileSectionBundle, ScopeId, SubjectId};

    /// Verifies Full embeds stable project identities without changing authorized scope order.
    /// 验证 Full 嵌入稳定项目身份且不改变已授权作用域顺序。
    #[test]
    fn full_view_keeps_project_identity_and_scope_order() {
        // sections uses the same names deliberately so identity must come from the scopes.
        // sections 有意使用相同名称，确保身份来自作用域。
        let sections = ["project-1", "project-2"]
            .into_iter()
            .map(|id| ProfileViewSection {
                scope_id: ScopeId::new(id).expect("scope"),
                role: SectionRole::Environment,
                display_label: String::from("Project"),
            })
            .collect();
        // view is the exact structure sent by the client to Core.
        // view 是客户端发送给 Core 的准确结构。
        let view = NativeProfileBundleFormat::new(ProfileBundleMode::ProfileBundleModeFull, None)
            .view(sections);
        assert_eq!(view.sections[0].scope_id.as_str(), "project-1");
        assert_eq!(view.sections[1].scope_id.as_str(), "project-2");
        assert_eq!(view.sections[0].display_label, "Project (project-1)");
        assert_eq!(view.sections[1].display_label, "Project (project-2)");
    }

    /// Creates an ordered bundle with two projects and one fixed user for projection tests.
    /// 创建含两个有序项目及一个固定用户的画像包用于投影测试。
    fn bundle(project: bool, user: bool) -> ProfileBundle {
        // sections retains explicit identities rather than relying on project display names.
        // sections 保留显式身份，而非依赖项目显示名称。
        let sections = [
            ("project-1", SectionRole::Environment, "Project", project),
            (
                "project-2",
                SectionRole::Environment,
                "Linked Project",
                project,
            ),
            ("user-1", SectionRole::Subject, "User", user),
        ]
        .into_iter()
        .map(|(id, role, label, present)| ProfileSectionBundle {
            scope_id: ScopeId::new(id).expect("scope"),
            role,
            display_label: label.to_owned(),
            nodes: Vec::new(),
            rendered: if present {
                format!("content-{id}")
            } else {
                String::new()
            },
        })
        .collect::<Vec<_>>();
        ProfileBundle {
            subject_id: SubjectId::new("user-1").expect("subject"),
            visible_scopes: sections
                .iter()
                .map(|section| section.scope_id.clone())
                .collect(),
            nodes: Vec::new(),
            rendered: if project || user {
                String::from("combined-only")
            } else {
                String::new()
            },
            sections,
        }
    }

    /// Verifies Full and unspecified modes expose only the combined text for all content shapes.
    /// 验证所有内容组合下 Full 及未指定模式仅暴露组合文本。
    #[test]
    fn full_bundle_suppresses_split_sections() {
        for mode in [
            ProfileBundleMode::ProfileBundleModeFull,
            ProfileBundleMode::ProfileBundleModeUnspecified,
        ] {
            for explanation in [None, Some(false), Some(true)] {
                for (project, user) in [(false, false), (false, true), (true, false), (true, true)]
                {
                    // format controls both the Core request and its public projection.
                    // format 同时控制 Core 请求及其公开投影。
                    let format = NativeProfileBundleFormat::new(mode, explanation);
                    assert_eq!(format.view(Vec::new()).mode, ProfileViewMode::Full);
                    assert_eq!(
                        format.view(Vec::new()).include_explanations,
                        explanation.unwrap_or(false)
                    );
                    // response must never invite consumers to concatenate content twice.
                    // response 绝不能诱导消费方将内容拼接两次。
                    let response = format.response(bundle(project, user), String::from("trace"));
                    assert_eq!(response.mode, ProfileBundleMode::ProfileBundleModeFull);
                    assert_eq!(response.include_explanation, explanation.unwrap_or(false));
                    assert_eq!(
                        response.combined_text,
                        if project || user { "combined-only" } else { "" }
                    );
                    assert!(response.project_profile.is_empty());
                    assert!(response.user_profile.is_empty());
                    assert!(response.team_profile.is_empty());
                    assert!(response.space_profile.is_empty());
                    assert!(response.explanation_text.is_empty());
                    assert!(response.environment_priority_text.is_empty());
                }
            }
        }
    }

    /// Verifies Split suppresses explanations and merged text while preserving project identity.
    /// 验证 Split 禁用说明及组合文本，同时保留项目身份。
    #[test]
    fn split_bundle_preserves_partition_identity_without_combined_text() {
        for explanation in [None, Some(false), Some(true)] {
            for (project, user) in [(false, false), (false, true), (true, false), (true, true)] {
                // format normalizes the explanation flag before dispatch, not only after rendering.
                // format 在派发前归一说明开关，而非仅在渲染后处理。
                let format = NativeProfileBundleFormat::new(
                    ProfileBundleMode::ProfileBundleModeSplit,
                    explanation,
                );
                assert_eq!(format.view(Vec::new()).mode, ProfileViewMode::Split);
                assert!(!format.view(Vec::new()).include_explanations);
                // response intentionally receives a merged fixture to verify output suppression.
                // response 有意接收带组合文本的样本，以验证输出抑制。
                let response = format.response(bundle(project, user), String::from("trace"));
                assert_eq!(response.mode, ProfileBundleMode::ProfileBundleModeSplit);
                assert!(!response.include_explanation);
                assert!(response.combined_text.is_empty());
                assert_eq!(
                    response.project_profile,
                    if project {
                        "[Project (project-1)]\ncontent-project-1\n\n[Linked Project (project-2)]\ncontent-project-2"
                    } else {
                        ""
                    }
                );
                assert_eq!(
                    response.user_profile,
                    if user { "content-user-1" } else { "" }
                );
                assert!(response.team_profile.is_empty());
                assert!(response.space_profile.is_empty());
                assert!(response.explanation_text.is_empty());
                assert!(response.environment_priority_text.is_empty());
                assert_eq!(response.trace_id, "trace");
            }
        }
    }
}
