//! Diagram components compiled to escaped, layout-ready markup; only the bundled script draws connectors.
//! 编译为已转义且可直接布局标记的图表组件；只有内置脚本绘制连线。

use std::collections::HashMap;

use super::{DRAFT_MAX_ITEMS, escape, invalid};
use crate::v1::html_answers::HtmlAnswerUnavailableReason;

/// Plan-review marker written at the start of a flow, sequence or tree line.
/// 写在流程、时序或树形行首的方案审阅标记。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Change {
    /// The line describes an addition.
    /// 该行描述新增内容。
    Added,
    /// The line describes a removal.
    /// 该行描述移除内容。
    Removed,
    /// The node is modified in place.
    /// 节点在原位修改。
    Changed,
}

impl Change {
    /// Return the fixed CSS class for this marker.
    /// 返回此标记固定的 CSS 类名。
    fn class(self) -> &'static str {
        match self {
            Self::Added => "add",
            Self::Removed => "del",
            Self::Changed => "chg",
        }
    }

    /// Split a leading `+ `, `- ` or `~ ` marker from `line`; `\- ` keeps a literal dash.
    /// 从 `line` 拆出前导 `+ `、`- ` 或 `~ ` 标记；`\- ` 保留字面短横线。
    fn split(line: &str) -> (Option<Self>, &str) {
        if let Some(rest) = line.strip_prefix('\\') {
            return (None, rest);
        }
        for (prefix, change) in [
            ("+ ", Self::Added),
            ("- ", Self::Removed),
            ("~ ", Self::Changed),
        ] {
            if let Some(rest) = line.strip_prefix(prefix) {
                return (Some(change), rest.trim_start());
            }
        }
        (None, line)
    }
}

/// Split `text` at its first ASCII or full-width colon; returns the trimmed head and optional tail.
/// 在第一个 ASCII 或全角冒号处拆分 `text`；返回去除空白的前段和可选后段。
fn split_label(text: &str) -> (&str, Option<&str>) {
    match text.find([':', '：']) {
        Some(index) => {
            let colon = text[index..].chars().next().map_or(1, char::len_utf8);
            (text[..index].trim(), Some(text[index + colon..].trim()))
        }
        None => (text.trim(), None),
    }
}

/// Node outline declared by its surrounding brackets.
/// 由外围括号声明的节点轮廓。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Box,
    Decision,
    Terminal,
    Store,
}

impl Shape {
    /// Strip the outline brackets from `spec`; returns the shape and the inner label.
    /// 去除 `spec` 的轮廓括号；返回形状及内部标签。
    fn parse(spec: &str) -> (Self, &str) {
        let spec = spec.trim();
        if let Some(inner) = spec.strip_prefix("[(").and_then(|s| s.strip_suffix(")]")) {
            (Self::Store, inner.trim())
        } else if let Some(inner) = spec.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            (Self::Decision, inner.trim())
        } else if let Some(inner) = spec.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
            (Self::Terminal, inner.trim())
        } else if let Some(inner) = spec.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            (Self::Box, inner.trim())
        } else {
            (Self::Box, spec)
        }
    }

    /// Return the stable data attribute value consumed by the stylesheet.
    /// 返回样式表使用的稳定数据属性值。
    fn name(self) -> &'static str {
        match self {
            Self::Box => "box",
            Self::Decision => "decision",
            Self::Terminal => "terminal",
            Self::Store => "store",
        }
    }
}

/// One directed connection between node indexes.
/// 节点索引之间的一条有向连接。
struct Edge {
    from: usize,
    to: usize,
    dashed: bool,
    label: Option<String>,
    change: Option<Change>,
}

/// Mutable flow graph keyed by label; repeated labels name the same node.
/// 以标签为键的可变流程图；重复标签表示同一节点。
#[derive(Default)]
struct Graph {
    labels: Vec<String>,
    shapes: Vec<Shape>,
    changes: Vec<Option<Change>>,
    index: HashMap<String, usize>,
    edges: Vec<Edge>,
}

impl Graph {
    /// Resolve or create the node named by `spec`; returns its index or rejects empty/excess nodes.
    /// 解析或创建 `spec` 指定的节点；返回索引，空节点或超额节点被拒绝。
    fn node(&mut self, spec: &str) -> Result<usize, HtmlAnswerUnavailableReason> {
        let (shape, label) = Shape::parse(spec);
        if label.is_empty() {
            return invalid();
        }
        if let Some(&existing) = self.index.get(label) {
            if shape != Shape::Box {
                self.shapes[existing] = shape;
            }
            return Ok(existing);
        }
        if self.labels.len() >= DRAFT_MAX_ITEMS {
            return invalid();
        }
        self.index.insert(label.to_owned(), self.labels.len());
        self.labels.push(label.to_owned());
        self.shapes.push(shape);
        self.changes.push(None);
        Ok(self.labels.len() - 1)
    }

    /// Resolve an `A & B` group into node indexes.
    /// 将 `A & B` 分组解析为节点索引。
    fn group(&mut self, spec: &str) -> Result<Vec<usize>, HtmlAnswerUnavailableReason> {
        spec.split('&').map(|item| self.node(item)).collect()
    }

    /// Assign layers by longest path, ignoring edges that close a cycle; returns per-node ranks.
    /// 按最长路径分层，忽略构成环的边；返回每个节点的层级。
    fn ranks(&self) -> Vec<usize> {
        let count = self.labels.len();
        let mut outgoing = vec![Vec::new(); count];
        for edge in &self.edges {
            outgoing[edge.from].push(edge.to);
        }
        // 0 = unvisited, 1 = on the DFS stack, 2 = finished; stack edges are cycle back-edges.
        // 0 表示未访问，1 表示位于 DFS 栈上，2 表示已完成；指向栈上节点的边是环回边。
        let mut state = vec![0u8; count];
        let mut order = Vec::with_capacity(count);
        let mut forward = vec![Vec::new(); count];
        for root in 0..count {
            if state[root] != 0 {
                continue;
            }
            let mut stack = vec![(root, 0usize)];
            state[root] = 1;
            while let Some((node, next)) = stack.last_mut() {
                if let Some(&target) = outgoing[*node].get(*next) {
                    *next += 1;
                    let from = *node;
                    match state[target] {
                        0 => {
                            forward[from].push(target);
                            state[target] = 1;
                            stack.push((target, 0));
                        }
                        2 => forward[from].push(target),
                        _ => {}
                    }
                } else {
                    state[*node] = 2;
                    order.push(*node);
                    stack.pop();
                }
            }
        }
        // Reverse post-order is topological for the forward edges.
        // 逆后序是前向边的拓扑序。
        let mut rank = vec![0usize; count];
        for &node in order.iter().rev() {
            for &target in &forward[node] {
                rank[target] = rank[target].max(rank[node] + 1);
            }
        }
        rank
    }
}

/// Compile `ui-flow [LR]`: chains `A -> B --> C`, groups `A -> B & C`, a final `: label`,
/// node outlines `{decision}`, `(terminal)`, `[(store)]` and plan markers.
/// 编译 `ui-flow [LR]`：链式 `A -> B --> C`、分组 `A -> B & C`、末尾 `: 标签`、
/// 节点轮廓 `{判断}`、`(端点)`、`[(存储)]` 及方案标记。
/// `id` makes element identifiers unique within the page.
/// `id` 使元素标识在页面内唯一。
pub(super) fn render_flow(
    source: &str,
    args: &str,
    id: usize,
) -> Result<String, HtmlAnswerUnavailableReason> {
    let direction = match args {
        "" => "tb",
        "LR" => "lr",
        _ => return invalid(),
    };
    let mut graph = Graph::default();
    for line in source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let (change, line) = Change::split(line);
        let mut parts = Vec::new();
        let mut dashed = Vec::new();
        let mut rest = line;
        while let Some(index) = rest.find("->") {
            let dash = index > 0 && rest.as_bytes()[index - 1] == b'-';
            parts.push(&rest[..if dash { index - 1 } else { index }]);
            dashed.push(dash);
            rest = &rest[index + 2..];
        }
        if parts.is_empty() {
            // A bare node line only marks an in-place change.
            // 不含箭头的节点行仅用于标记原位修改。
            if change != Some(Change::Changed) {
                return invalid();
            }
            let node = graph.node(line)?;
            graph.changes[node] = Some(Change::Changed);
            continue;
        }
        if change == Some(Change::Changed) {
            return invalid();
        }
        let (last, label) = split_label(rest);
        parts.push(last);
        let groups = parts
            .iter()
            .map(|part| graph.group(part))
            .collect::<Result<Vec<_>, _>>()?;
        for (hop, pair) in groups.windows(2).enumerate() {
            for &from in &pair[0] {
                for &to in &pair[1] {
                    if graph.edges.len() >= DRAFT_MAX_ITEMS {
                        return invalid();
                    }
                    graph.edges.push(Edge {
                        from,
                        to,
                        dashed: dashed[hop],
                        label: (hop + 2 == groups.len())
                            .then(|| label.filter(|text| !text.is_empty()))
                            .flatten()
                            .map(str::to_owned),
                        change,
                    });
                }
            }
        }
    }
    if graph.edges.is_empty() {
        return invalid();
    }
    let rank = graph.ranks();
    let layers = rank.iter().max().map_or(0, |max| max + 1);
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); layers];
    for (node, &layer) in rank.iter().enumerate() {
        members[layer].push(node);
    }
    // One barycentre sweep keeps children under their parents and reduces crossings.
    // 一次重心排序让子节点靠近父节点并减少交叉。
    let mut position = vec![0f64; graph.labels.len()];
    for layer in &mut members {
        let mut keyed: Vec<(f64, usize)> = layer
            .iter()
            .enumerate()
            .map(|(order, &node)| {
                let parents: Vec<f64> = graph
                    .edges
                    .iter()
                    .filter(|edge| edge.to == node && rank[edge.from] < rank[node])
                    .map(|edge| position[edge.from])
                    .collect();
                let key = if parents.is_empty() {
                    order as f64
                } else {
                    parents.iter().sum::<f64>() / parents.len() as f64
                };
                (key, node)
            })
            .collect();
        keyed.sort_by(|left, right| left.0.total_cmp(&right.0));
        for (order, (_, node)) in keyed.iter().enumerate() {
            position[*node] = order as f64;
        }
        *layer = keyed.into_iter().map(|(_, node)| node).collect();
    }
    let mut output =
        format!("<figure class=flow data-dir={direction} id=f{id}><div class=flow-ranks>");
    for layer in &members {
        output.push_str("<div class=flow-rank>");
        for &node in layer {
            output.push_str(&format!(
                "<div class=\"node{}\" data-shape={} id=f{id}-{node} dir=auto>{}</div>",
                graph.changes[node].map_or(String::new(), |change| format!(" {}", change.class())),
                graph.shapes[node].name(),
                escape(&graph.labels[node])
            ));
        }
        output.push_str("</div>");
    }
    output.push_str("</div><ol class=flow-edges>");
    for edge in &graph.edges {
        let mut classes = Vec::new();
        if edge.dashed {
            classes.push("dashed");
        }
        if let Some(change) = edge.change {
            classes.push(change.class());
        }
        output.push_str(&format!(
            "<li class=\"{}\" data-from=f{id}-{} data-to=f{id}-{}><span dir=auto>{}</span> → <span dir=auto>{}</span>{}</li>",
            classes.join(" "),
            edge.from,
            edge.to,
            escape(&graph.labels[edge.from]),
            escape(&graph.labels[edge.to]),
            edge.label
                .as_deref()
                .map_or(String::new(), |label| format!("<em dir=auto>{}</em>", escape(label)))
        ));
    }
    output.push_str("</ol></figure>");
    Ok(output)
}

/// One parsed sequence row.
/// 一条已解析的时序行。
enum Step<'a> {
    Message {
        from: usize,
        to: usize,
        dashed: bool,
        label: &'a str,
        change: Option<Change>,
    },
    Note {
        first: usize,
        last: usize,
        text: &'a str,
    },
    Divider(&'a str),
}

/// Compile `ui-sequence [num]`: `A -> B: label`, dashed `-->` replies, `note A, B: text` and
/// `== phase ==` dividers into a responsive grid with lifelines; `num` numbers messages.
/// 编译 `ui-sequence [num]`：`A -> B: 标签`、虚线 `-->` 回复、`note A, B: 文字` 与 `== 阶段 ==`
/// 分隔，生成带生命线的响应式网格；`num` 为消息编号。
pub(super) fn render_sequence(
    source: &str,
    args: &str,
) -> Result<String, HtmlAnswerUnavailableReason> {
    let numbered = match args {
        "" => false,
        "num" => true,
        _ => return invalid(),
    };
    let mut actors: Vec<String> = Vec::new();
    let mut actor = |name: &str| -> Result<usize, HtmlAnswerUnavailableReason> {
        let name = name.trim();
        if name.is_empty() {
            return invalid();
        }
        Ok(match actors.iter().position(|known| *known == name) {
            Some(index) => index,
            None => {
                actors.push(name.to_owned());
                actors.len() - 1
            }
        })
    };
    let mut steps = Vec::new();
    for line in source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if steps.len() >= DRAFT_MAX_ITEMS {
            return invalid();
        }
        if let Some(inner) = line.strip_prefix("==").and_then(|s| s.strip_suffix("==")) {
            if inner.trim().is_empty() {
                return invalid();
            }
            steps.push(Step::Divider(inner.trim()));
        } else if let Some(rest) = line.strip_prefix("note ") {
            let (over, text) = split_label(rest);
            let text = text.filter(|text| !text.is_empty());
            let Some(text) = text else {
                return invalid();
            };
            let indexes = over
                .split([',', '，'])
                .map(&mut actor)
                .collect::<Result<Vec<_>, _>>()?;
            steps.push(Step::Note {
                first: *indexes.iter().min().expect("split yields one item"),
                last: *indexes.iter().max().expect("split yields one item"),
                text,
            });
        } else {
            let (change, line) = Change::split(line);
            if change == Some(Change::Changed) {
                return invalid();
            }
            let Some(index) = line.find("->") else {
                return invalid();
            };
            let dashed = index > 0 && line.as_bytes()[index - 1] == b'-';
            let from = actor(&line[..if dashed { index - 1 } else { index }])?;
            let (to, label) = split_label(&line[index + 2..]);
            let to = actor(to)?;
            steps.push(Step::Message {
                from,
                to,
                dashed,
                label: label.unwrap_or(""),
                change,
            });
        }
    }
    if !steps
        .iter()
        .any(|step| matches!(step, Step::Message { .. }))
    {
        return invalid();
    }
    let count = actors.len();
    let mut output = format!(
        "<div class=sequence><div class=seq-grid style=\"--n:{count};grid-template-columns:repeat({},minmax(44px,1fr))\">",
        count * 2
    );
    for (index, name) in actors.iter().enumerate() {
        output.push_str(&format!(
            "<div class=actor style=\"grid-column:{}/span 2\" dir=auto>{}</div>",
            index * 2 + 1,
            escape(name)
        ));
    }
    let mut number = 0;
    for step in &steps {
        match step {
            Step::Message {
                from,
                to,
                dashed,
                label,
                change,
            } => {
                number += 1;
                let mut classes = vec!["msg"];
                if *dashed {
                    classes.push("dashed");
                }
                if let Some(change) = change {
                    classes.push(change.class());
                }
                let column = if from == to {
                    classes.push("self");
                    format!("{}/span 2", from * 2 + 2)
                } else {
                    if to < from {
                        classes.push("rev");
                    }
                    format!("{}/{}", from.min(to) * 2 + 2, from.max(to) * 2 + 2)
                };
                let prefix = if numbered {
                    format!("<b>{number}</b> ")
                } else {
                    String::new()
                };
                output.push_str(&format!(
                    "<div class=\"{}\" style=\"grid-column:{column}\"><span dir=auto>{prefix}{}</span></div>",
                    classes.join(" "),
                    escape(label)
                ));
            }
            Step::Note { first, last, text } => output.push_str(&format!(
                "<div class=note style=\"grid-column:{}/{}\" dir=auto>{}</div>",
                first * 2 + 1,
                last * 2 + 3,
                escape(text)
            )),
            Step::Divider(text) => output.push_str(&format!(
                "<div class=divider style=\"grid-column:1/-1\"><span dir=auto>{}</span></div>",
                escape(text)
            )),
        }
    }
    output.push_str("</div></div>");
    Ok(output)
}

/// Compile `ui-tree`: indentation sets depth, `label | note` adds a description, and plan
/// markers flag added, removed or changed nodes.
/// 编译 `ui-tree`：缩进决定层级，`标签 | 说明` 添加描述，方案标记标出新增、移除或修改的节点。
pub(super) fn render_tree(source: &str) -> Result<String, HtmlAnswerUnavailableReason> {
    let mut indents: Vec<usize> = Vec::new();
    let mut output = String::from("<ul class=tree>");
    let mut depth = 0usize;
    let mut items = 0usize;
    for line in source.lines().filter(|line| !line.trim().is_empty()) {
        items += 1;
        if items > DRAFT_MAX_ITEMS {
            return invalid();
        }
        let indent: usize = line
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .map(|ch| if ch == '\t' { 4 } else { 1 })
            .sum();
        while indents.last().is_some_and(|last| *last > indent) {
            indents.pop();
        }
        if indents.last() != Some(&indent) {
            indents.push(indent);
        }
        let level = indents.len() - 1;
        if items == 1 {
            // The first line opens the root list already emitted above.
            // 第一行使用上方已经输出的根列表。
        } else if level > depth {
            output.push_str("<ul>");
        } else {
            output.push_str("</li>");
            for _ in level..depth {
                output.push_str("</ul></li>");
            }
        }
        depth = level;
        let (change, text) = Change::split(line.trim());
        let (label, note) = text
            .split_once('|')
            .map_or((text.trim(), None), |(label, note)| {
                (label.trim(), Some(note.trim()))
            });
        if label.is_empty() || note.is_some_and(str::is_empty) {
            return invalid();
        }
        output.push_str(&format!(
            "<li{}><span class=node dir=auto>{}{}</span>{}",
            change.map_or(String::new(), |change| format!(" class={}", change.class())),
            change.map_or(String::new(), |change| format!(
                "<i class=mark aria-hidden=true>{}</i>",
                match change {
                    Change::Added => '+',
                    Change::Removed => '−',
                    Change::Changed => '~',
                }
            )),
            escape(label),
            note.map_or(String::new(), |note| format!(
                "<small dir=auto>{}</small>",
                escape(note)
            ))
        ));
    }
    output.push_str("</li>");
    for _ in 0..depth {
        output.push_str("</ul></li>");
    }
    output.push_str("</ul>");
    Ok(output)
}
