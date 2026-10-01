// Catalog pages: agent definitions and prompt skills, grouped by the layer they come from.
// 目录页面：代理定义与提示技能，按来源层级分组。

use super::*;

/// Layers of a catalog in display order: system, user, plugin.
/// 目录层级的显示顺序：系统、用户、插件。
const LAYERS: [&str; 3] = ["system", "user", "plugin"];

fn layer_label(layer: &str) -> String {
    match layer {
        "system" => tr!("agentSkills.system"),
        "user" => tr!("agentSkills.user"),
        "plugin" => tr!("agentSkills.plugins"),
        other => other.to_string(),
    }
}

pub(super) fn agents(system: &SystemSnapshot) -> Vec<Row> {
    let options = internal_models(system, tr!("agents.followSessionModel"));
    let enabled = system.agents.iter().filter(|a| a.enabled).count();
    let mut rows = vec![
        note(tr!("agents.manageUserLevelAgent"), Tone::Muted),
        info(
            tr!("area.agents"),
            tr!(
                "agents.header.detail",
                total = system.agents.len(),
                enabled = enabled,
                warnings = 0
            ),
            Tone::Normal,
        ),
    ];
    for layer in LAYERS {
        let list: Vec<&AgentProfile> = system.agents.iter().filter(|a| a.scope == layer).collect();
        if list.is_empty() {
            continue;
        }
        rows.push(section(format!("{} · {}", layer_label(layer), list.len())));
        for agent in list {
            rows.push(agent_row(agent, &options));
        }
    }
    if system.agents.is_empty() {
        rows.push(note(tr!("agents.noAgentDefinitions"), Tone::Muted));
    }
    rows.push(spacer());
    rows.push(note(tr!("tui.agents.editNote"), Tone::Muted));
    rows
}

fn agent_row(agent: &AgentProfile, options: &[(String, String)]) -> Row {
    let model = options
        .iter()
        .find(|(id, _)| *id == agent.model)
        .map_or_else(|| agent.model.clone(), |(_, label)| label.clone());
    let tools = match (agent.allowed_tools, agent.denied_tools) {
        (0, 0) => tr!("agents.allTools"),
        (allowed, 0) => tr!("agents.allowTools", count = allowed),
        (0, denied) => tr!("agents.denyTools", count = denied),
        (allowed, denied) => tr!(
            "agents.allowDeny",
            allowedToolCount = allowed,
            disabledToolCount = denied
        ),
    };
    let skills = if agent.skills.is_empty() {
        "—".to_string()
    } else {
        agent.skills.join(", ")
    };
    let mut lines = vec![
        format!("{}  {tools}", tr!("agents.toolPolicy")),
        format!("{}  {skills}", tr!("agents.attachedSkills")),
        format!("{}  {}", tr!("agents.definitionFile"), agent.file),
    ];
    if let Some(plugin) = &agent.plugin {
        lines.push(format!("{}  {plugin}", tr!("agents.owningPlugin")));
    }
    let mut actions = vec![
        switch(
            tr!("agents.enabled"),
            agent.enabled,
            Edit::Enabled(EntityKind::Agent, agent.id.clone()),
        ),
        switch(
            tr!("agents.exposeToModel"),
            agent.expose,
            Edit::Field(EntityKind::Agent, agent.id.clone(), "expose".into()),
        ),
        action(
            tr!("agents.model"),
            UiAction::Pick {
                title: tr!("agents.chooseFixedAgentModel"),
                edit: Edit::Route(EntityKind::Agent, agent.id.clone()),
                options: options.to_vec(),
                current: agent.model.clone(),
            },
            Tone::Normal,
        ),
    ];
    let catalog = |op: CatalogOp| sys_action(SystemCommand::Agent(op));
    if agent.overridden {
        actions.push(action(
            tr!("agents.restoreDefaults"),
            confirm(
                tr!("agents.restoreAgentDefaults"),
                tr!("agents.removeTheUserOverride", value = agent.name),
                tr!("agents.restoreDefaults"),
                catalog(CatalogOp::Restore(agent.id.clone())),
            ),
            Tone::Warn,
        ));
    }
    if agent.scope == "user" {
        actions.push(action(
            tr!("agents.delete"),
            confirm(
                tr!("agents.deleteAgent"),
                tr!("agents.deleteTheUserDefinition", value = agent.name),
                tr!("agents.delete"),
                catalog(CatalogOp::Delete(agent.id.clone())),
            ),
            Tone::Bad,
        ));
    }
    let description = if agent.description.is_empty() {
        tr!("agents.noDescriptionProvided")
    } else {
        agent.description.clone()
    };
    let mut name = agent.name.clone();
    if agent.overridden {
        name = format!("{name}  ✎");
    }
    let (badge, tone) = if !agent.enabled {
        (tr!("agents.disabled"), Tone::Muted)
    } else if agent.model.is_empty() {
        (tr!("agents.followSession"), Tone::Muted)
    } else {
        (model, Tone::Accent)
    };
    item(&agent.id, name, description)
        .badge(badge, tone)
        .lines(lines)
        .actions(actions)
}

pub(super) fn skills(system: &SystemSnapshot) -> Vec<Row> {
    let skills = &system.skills;
    let of = |layer: &str| skills.iter().filter(|s| s.source == layer).count();
    let enabled = skills.iter().filter(|s| s.enabled).count();
    let issues = skills.iter().filter(|s| s.issue.is_some()).count();
    let overrides = skills.iter().filter(|s| s.overridden).count();
    let mut rows = vec![
        note(
            tr!(
                "skills.catalogSummary",
                system = of("system"),
                user = of("user"),
                plugin = of("plugin"),
                effective = enabled,
                issues = issues
            ),
            Tone::Muted,
        ),
        stats(vec![
            (tr!("agentSkills.enabled"), enabled.to_string(), Tone::Good),
            (
                tr!("agentSkills.disabled"),
                (skills.len() - enabled).to_string(),
                Tone::Muted,
            ),
            (
                tr!("agentSkills.issues"),
                issues.to_string(),
                if issues > 0 { Tone::Bad } else { Tone::Normal },
            ),
            (
                tr!("agentSkills.overrides"),
                overrides.to_string(),
                if overrides > 0 {
                    Tone::Warn
                } else {
                    Tone::Normal
                },
            ),
        ]),
    ];
    for layer in LAYERS {
        let list: Vec<&Skill> = skills.iter().filter(|s| s.source == layer).collect();
        if list.is_empty() {
            continue;
        }
        rows.push(section(format!("{} · {}", layer_label(layer), list.len())));
        rows.extend(list.into_iter().map(skill_row));
    }
    if skills.is_empty() {
        rows.push(note(tr!("agentSkills.noSkillsMatchThe"), Tone::Muted));
    }
    rows.push(spacer());
    rows.push(note(tr!("tui.skills.editNote"), Tone::Muted));
    rows
}

fn skill_row(skill: &Skill) -> Row {
    let catalog = |op: CatalogOp| sys_action(SystemCommand::Skill(op));
    let mut actions = vec![switch(
        tr!("agentSkills.enabled"),
        skill.enabled,
        Edit::Enabled(EntityKind::Skill, skill.id.clone()),
    )];
    if skill.overridden {
        actions.push(action(
            tr!("agentSkills.restoreSystemVersion"),
            confirm(
                tr!("agentSkills.restoreSystemVersion"),
                tr!("agentSkills.removeTheUserOverride", value = skill.name),
                tr!("agentSkills.confirm"),
                catalog(CatalogOp::Restore(skill.id.clone())),
            ),
            Tone::Warn,
        ));
    }
    if skill.source == "user" {
        actions.push(action(
            tr!("agentSkills.delete"),
            confirm(
                tr!("agentSkills.deleteSkill"),
                tr!("agentSkills.deleteUserSkillAnd", value = skill.name),
                tr!("agentSkills.delete"),
                catalog(CatalogOp::Delete(skill.id.clone())),
            ),
            Tone::Bad,
        ));
    }
    let mut detail = if skill.description.is_empty() {
        tr!("agentSkills.noDescriptionProvided")
    } else {
        skill.description.clone()
    };
    if let Some(plugin) = &skill.plugin {
        detail = format!("{plugin} · {detail}");
    }
    let mut row = item(&skill.id, skill.name.clone(), detail).actions(actions);
    if let Some(issue) = &skill.issue {
        row = row
            .badge(tr!("agentSkills.issues"), Tone::Bad)
            .lines(vec![issue.clone()]);
    } else if skill.overridden {
        row = row.badge(tr!("agentSkills.userOverride"), Tone::Warn);
    } else if !skill.enabled {
        row = row.badge(tr!("agentSkills.disabled"), Tone::Muted);
    }
    row
}
