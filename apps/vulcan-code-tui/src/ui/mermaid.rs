// Text drawings of Mermaid diagrams for the transcript.
// 会话记录中 Mermaid 图表的文本绘制。
//
// Fenced `mermaid` blocks are drawn with box characters instead of being shown as source.
// Flowcharts (`graph` / `flowchart`) and state diagrams go through a small layered layout:
// longest-path layers, bend points on long edges, barycenter ordering, and one routing lane per
// source between layers. Sequence diagrams draw participants, lifelines, messages, notes and block
// frames; pie charts become bars. Another diagram type, a statement the parser does not
// understand, or a drawing wider than the transcript returns `None`, and the caller shows the
// source instead, so a diagram is never drawn wrong or clipped.
// 围栏 `mermaid` 代码块以方框字符绘制，而不是显示源码。流程图（`graph` / `flowchart`）与状态图经过
// 一个小型分层布局：最长路径分层、长边插入折点、重心排序、层间每个来源一条走线通道。时序图绘制参与者、
// 生命线、消息、注释与区块框；饼图转为条形。其他图表类型、解析器无法理解的语句或宽于会话记录的图形
// 返回 `None`，由调用方改为显示源码，因此图表不会被画错或截断。

use std::collections::HashMap;

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use super::text::{truncate, width_of, wrap};
use super::theme::Theme;

/// Widest a node label grows before it wraps, and an edge label before it is cut.
/// 节点标签换行前、边标签截断前的最大宽度。
const LABEL_MAX: usize = 20;
/// Widest a sequence message or note grows before it is cut or wrapped.
/// 时序消息截断前、注释换行前的最大宽度。
const MESSAGE_MAX: usize = 40;
/// Most nodes a flowchart may have before the source is shown instead.
/// 流程图节点数上限，超出时改为显示源码。
const NODE_LIMIT: usize = 150;

/// Draws `source` at most `width` columns wide, or `None` when it cannot be drawn faithfully.
/// 以不超过 `width` 列的宽度绘制 `source`；无法如实绘制时返回 `None`。
pub fn render(source: &str, width: usize, theme: &Theme) -> Option<Vec<Line<'static>>> {
    let mut statements: Vec<&str> = source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("%%"))
        .collect();
    // Front matter (`---` ... `---`) carries configuration only.
    // 前置元数据（`---` ... `---`）只承载配置。
    if statements.first() == Some(&"---") {
        let close = statements[1..].iter().position(|line| *line == "---")?;
        statements.drain(..close + 2);
    }
    let (header, body) = statements.split_first()?;
    let canvas = match header.split([' ', ';']).next()? {
        "graph" | "flowchart" | "flowchart-elk" => {
            flowchart(&parse_flowchart(header, body)?, width)?
        }
        "stateDiagram" | "stateDiagram-v2" => flowchart(&parse_state(body)?, width)?,
        "sequenceDiagram" => sequence(&parse_sequence(body)?, width)?,
        "pie" => return pie(header, body, width, theme),
        _ => return None,
    };
    Some(canvas.into_lines(theme))
}

// ---- Canvas ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ink {
    Blank,
    Border,
    Text,
    Edge,
    Arrow,
    Label,
    Note,
    Frame,
    Lifeline,
}

const UP: u8 = 1;
const DOWN: u8 = 2;
const LEFT: u8 = 4;
const RIGHT: u8 = 8;
/// Marks the second cell of a wide character.
/// 标记宽字符的第二个单元格。
const TAIL: char = '\0';

/// One cell: a character, or connection bits that become a box character when drawn.
/// 一个单元格：字符，或绘制时转为方框字符的连接位。
#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    ink: Ink,
    bits: u8,
}

const BLANK: Cell = Cell {
    ch: ' ',
    ink: Ink::Blank,
    bits: 0,
};

struct Canvas {
    width: usize,
    height: usize,
    cells: Vec<Cell>,
}

impl Canvas {
    fn new(width: usize, height: usize) -> Self {
        Canvas {
            width,
            height,
            cells: vec![BLANK; width * height],
        }
    }

    fn cell(&mut self, x: usize, y: usize) -> Option<&mut Cell> {
        (x < self.width && y < self.height).then(|| &mut self.cells[y * self.width + x])
    }

    fn is_free(&self, x: usize, y: usize) -> bool {
        x < self.width && y < self.height && {
            let cell = self.cells[y * self.width + x];
            cell.ink == Ink::Blank && cell.bits == 0
        }
    }

    /// Writes one character and returns the columns it takes.
    /// 写入一个字符并返回其占用的列数。
    fn put(&mut self, x: usize, y: usize, ch: char, ink: Ink) -> usize {
        let width = ch.width().unwrap_or(0).max(1);
        if let Some(cell) = self.cell(x, y) {
            *cell = Cell { ch, ink, bits: 0 };
        }
        if width == 2
            && let Some(cell) = self.cell(x + 1, y)
        {
            *cell = Cell {
                ch: TAIL,
                ink,
                bits: 0,
            };
        }
        width
    }

    fn text(&mut self, x: usize, y: usize, text: &str, ink: Ink) {
        let mut x = x;
        for ch in text.chars() {
            x += self.put(x, y, ch, ink);
        }
    }

    /// Draws a straight line between two cells on one row or column. Lines join where they meet
    /// and never cover characters.
    /// 在同一行或同一列的两个单元格之间画直线。线条相遇处自动连接，且不会覆盖字符。
    fn link(&mut self, (x0, y0): (usize, usize), (x1, y1): (usize, usize)) {
        if x0 == x1 {
            let (low, high) = (y0.min(y1), y0.max(y1));
            for y in low..=high {
                let bits = if y > low { UP } else { 0 } | if y < high { DOWN } else { 0 };
                self.mark(x0, y, bits);
            }
        } else if y0 == y1 {
            let (low, high) = (x0.min(x1), x0.max(x1));
            for x in low..=high {
                let bits = if x > low { LEFT } else { 0 } | if x < high { RIGHT } else { 0 };
                self.mark(x, y0, bits);
            }
        }
    }

    fn mark(&mut self, x: usize, y: usize, bits: u8) {
        if let Some(cell) = self.cell(x, y)
            && cell.ink == Ink::Blank
        {
            cell.bits |= bits;
        }
    }

    /// Columns up to the last drawn cell of the widest row.
    /// 最宽一行中直到最后一个已绘制单元格的列数。
    fn used_width(&self) -> usize {
        self.cells
            .chunks(self.width.max(1))
            .filter_map(|row| row.iter().rposition(|c| c.ink != Ink::Blank || c.bits != 0))
            .map(|last| last + 1)
            .max()
            .unwrap_or(0)
    }

    fn into_lines(self, theme: &Theme) -> Vec<Line<'static>> {
        let style = |ink: Ink| match ink {
            Ink::Blank => Style::new(),
            Ink::Border | Ink::Arrow => Style::new().fg(theme.primary),
            Ink::Text => theme.base(),
            Ink::Edge => Style::new().fg(theme.muted_fg),
            Ink::Label | Ink::Note => Style::new().fg(theme.warning),
            Ink::Frame => Style::new().fg(theme.violet),
            Ink::Lifeline => Style::new().fg(theme.border),
        };
        let mut lines: Vec<Line<'static>> = Vec::new();
        for row in self.cells.chunks(self.width.max(1)) {
            let end = row
                .iter()
                .rposition(|c| c.ink != Ink::Blank || c.bits != 0)
                .map_or(0, |last| last + 1);
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut text = String::new();
            let mut current = None;
            for cell in &row[..end] {
                if cell.ch == TAIL {
                    continue;
                }
                let (ch, ink) = if cell.ink == Ink::Blank && cell.bits != 0 {
                    (box_char(cell.bits), Ink::Edge)
                } else {
                    (cell.ch, cell.ink)
                };
                let cell_style = style(ink);
                if current != Some(cell_style) {
                    if let Some(previous) = current
                        && !text.is_empty()
                    {
                        spans.push(Span::styled(std::mem::take(&mut text), previous));
                    }
                    current = Some(cell_style);
                }
                text.push(ch);
            }
            if let Some(previous) = current
                && !text.is_empty()
            {
                spans.push(Span::styled(text, previous));
            }
            lines.push(Line::from(spans));
        }
        while lines.last().is_some_and(|line| line.spans.is_empty()) {
            lines.pop();
        }
        lines
    }
}

fn box_char(bits: u8) -> char {
    match bits {
        b if b == UP | DOWN || b == UP || b == DOWN => '│',
        b if b == LEFT | RIGHT || b == LEFT || b == RIGHT => '─',
        b if b == DOWN | RIGHT => '┌',
        b if b == DOWN | LEFT => '┐',
        b if b == UP | RIGHT => '└',
        b if b == UP | LEFT => '┘',
        b if b == UP | DOWN | RIGHT => '├',
        b if b == UP | DOWN | LEFT => '┤',
        b if b == LEFT | RIGHT | DOWN => '┬',
        b if b == LEFT | RIGHT | UP => '┴',
        _ => '┼',
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Rect,
    Round,
    Decision,
}

/// Draws a box with its label lines centered inside.
/// 绘制方框并将标签各行居中放在其中。
fn draw_box(
    canvas: &mut Canvas,
    (x, y): (usize, usize),
    (width, height): (usize, usize),
    shape: Shape,
    lines: &[String],
    border: Ink,
) {
    let (top_left, top_right, bottom_left, bottom_right) = match shape {
        Shape::Rect => ('┌', '┐', '└', '┘'),
        Shape::Round => ('╭', '╮', '╰', '╯'),
        Shape::Decision => ('╱', '╲', '╲', '╱'),
    };
    let (right, bottom) = (x + width - 1, y + height - 1);
    canvas.put(x, y, top_left, border);
    canvas.put(right, y, top_right, border);
    canvas.put(x, bottom, bottom_left, border);
    canvas.put(right, bottom, bottom_right, border);
    for column in x + 1..right {
        canvas.put(column, y, '─', border);
        canvas.put(column, bottom, '─', border);
    }
    for row in y + 1..bottom {
        canvas.put(x, row, '│', border);
        canvas.put(right, row, '│', border);
        for column in x + 1..right {
            canvas.put(column, row, ' ', Ink::Text);
        }
    }
    for (index, line) in lines.iter().enumerate() {
        let offset = (width - 2).saturating_sub(width_of(line)) / 2;
        canvas.text(x + 1 + offset, y + 1 + index, line, Ink::Text);
    }
}

/// Splits a label into lines no wider than `max`, breaking at `<br>` and between words.
/// 将标签拆分为不宽于 `max` 的行，在 `<br>` 处及词间断行。
fn label_lines(label: &str, max: usize) -> Vec<String> {
    label
        .split('\n')
        .flat_map(|part| wrap(&[Span::raw(part.trim().to_string())], max, &[], &[]))
        .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect()
}

/// Strips quoting, line-break tags and simple formatting from a label.
/// 去掉标签中的引号、换行标签与简单格式。
fn clean_label(raw: &str) -> String {
    let mut text = raw.trim().trim_matches('"').trim_matches('`').to_string();
    for tag in ["<br/>", "<br />", "<br>", "\\n"] {
        text = text.replace(tag, "\n");
    }
    for tag in [
        "<b>",
        "</b>",
        "<i>",
        "</i>",
        "<strong>",
        "</strong>",
        "<em>",
        "</em>",
        "<u>",
        "</u>",
        "**",
    ] {
        text = text.replace(tag, "");
    }
    text.replace("#quot;", "\"")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

// ---- Flowcharts and state diagrams ----

struct Node {
    label: String,
    shape: Shape,
}

struct Edge {
    from: usize,
    to: usize,
    label: String,
    /// Arrowheads at `to` and at `from`.
    /// `to` 端与 `from` 端的箭头。
    head: bool,
    tail: bool,
    /// Invisible links (`~~~`) only shape the layout.
    /// 不可见连线（`~~~`）只影响布局。
    hidden: bool,
}

struct Graph {
    /// Top-down rather than left-to-right.
    /// 自上而下而非自左向右。
    down: bool,
    shape: Shape,
    nodes: Vec<Node>,
    ids: HashMap<String, usize>,
    edges: Vec<Edge>,
}

impl Graph {
    fn new(down: bool, shape: Shape) -> Self {
        Graph {
            down,
            shape,
            nodes: Vec::new(),
            ids: HashMap::new(),
            edges: Vec::new(),
        }
    }

    /// Finds or adds a node; a label or shape given later replaces the earlier one.
    /// 查找或添加节点；之后给出的标签或形状会替换先前的。
    fn node(&mut self, id: &str, label: Option<String>, shape: Option<Shape>) -> usize {
        let index = match self.ids.get(id) {
            Some(index) => *index,
            None => {
                self.nodes.push(Node {
                    label: id.to_string(),
                    shape: self.shape,
                });
                self.ids.insert(id.to_string(), self.nodes.len() - 1);
                self.nodes.len() - 1
            }
        };
        if let Some(label) = label {
            self.nodes[index].label = label;
        }
        if let Some(shape) = shape {
            self.nodes[index].shape = shape;
        }
        index
    }

    fn checked(self) -> Option<Self> {
        (!self.nodes.is_empty() && self.nodes.len() <= NODE_LIMIT).then_some(self)
    }
}

fn is_down(direction: Option<&str>) -> bool {
    !matches!(direction, Some("LR" | "RL"))
}

fn parse_flowchart(header: &str, body: &[&str]) -> Option<Graph> {
    let mut parts = header.split(';');
    let direction = parts.next()?.split_whitespace().nth(1);
    let mut graph = Graph::new(is_down(direction), Shape::Rect);
    let statements = parts.chain(body.iter().flat_map(|line| line.split(';')));
    for statement in statements {
        let statement = statement.trim();
        let keyword = statement.split_whitespace().next().unwrap_or("");
        let skipped = matches!(
            keyword,
            "" | "subgraph"
                | "end"
                | "classDef"
                | "class"
                | "style"
                | "linkStyle"
                | "click"
                | "direction"
        ) || keyword.starts_with("accTitle")
            || keyword.starts_with("accDescr");
        if !skipped {
            chain(&mut graph, statement)?;
        }
    }
    graph.checked()
}

/// Parses `a --> b & c -- text --> d` into nodes and edges.
/// 将 `a --> b & c -- text --> d` 解析为节点与边。
fn chain(graph: &mut Graph, statement: &str) -> Option<()> {
    let (mut previous, mut rest) = group(graph, statement)?;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return Some(());
        }
        let (link, after) = parse_link(rest)?;
        let (next, after) = group(graph, after.trim_start())?;
        for &from in &previous {
            for &to in &next {
                graph.edges.push(Edge {
                    from,
                    to,
                    label: link.label.clone(),
                    head: link.head,
                    tail: link.tail,
                    hidden: link.hidden,
                });
            }
        }
        previous = next;
        rest = after;
    }
}

fn group<'a>(graph: &mut Graph, text: &'a str) -> Option<(Vec<usize>, &'a str)> {
    let mut nodes = Vec::new();
    let mut rest = text;
    loop {
        let (id, label, shape, after) = node(rest.trim_start())?;
        nodes.push(graph.node(id, label, shape));
        rest = after.trim_start();
        match rest.strip_prefix('&') {
            Some(after) => rest = after,
            None => return Some((nodes, rest)),
        }
    }
}

/// Node shape delimiters, longest opener first.
/// 节点形状定界符，较长的开头在前。
const SHAPES: [(&str, &str, Shape); 14] = [
    ("(((", ")))", Shape::Round),
    ("((", "))", Shape::Round),
    ("([", "])", Shape::Round),
    ("[(", ")]", Shape::Round),
    ("[[", "]]", Shape::Rect),
    ("[/", "/]", Shape::Rect),
    ("[/", "\\]", Shape::Rect),
    ("[\\", "\\]", Shape::Rect),
    ("[\\", "/]", Shape::Rect),
    ("{{", "}}", Shape::Decision),
    ("[", "]", Shape::Rect),
    ("(", ")", Shape::Round),
    ("{", "}", Shape::Decision),
    (">", "]", Shape::Rect),
];

type ParsedNode<'a> = (&'a str, Option<String>, Option<Shape>, &'a str);

fn node(text: &str) -> Option<ParsedNode<'_>> {
    let end = text
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(text.len());
    if end == 0 {
        return None;
    }
    let (id, mut rest) = text.split_at(end);
    let mut label = None;
    let mut shape = None;
    for (open, close, kind) in SHAPES {
        let Some(inner) = rest.strip_prefix(open) else {
            continue;
        };
        // A quoted label may hold the closing delimiter.
        // 带引号的标签可能包含结束定界符。
        let close_at = match inner.strip_prefix('"') {
            Some(quoted) => quoted
                .find('"')
                .and_then(|quote| inner[quote + 2..].find(close).map(|at| quote + 2 + at)),
            None => inner.find(close),
        };
        let Some(close_at) = close_at else {
            continue;
        };
        label = Some(clean_label(&inner[..close_at]));
        shape = Some(kind);
        rest = &inner[close_at + close.len()..];
        break;
    }
    if let Some(after) = rest.strip_prefix(":::") {
        let end = after
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
            .unwrap_or(after.len());
        rest = &after[end..];
    }
    Some((id, label, shape, rest))
}

struct Link {
    label: String,
    head: bool,
    tail: bool,
    hidden: bool,
}

/// Parses one link (`-->`, `---`, `-.->`, `==>`, `~~~`, `--o`, `<-->`, `-- text -->`,
/// `-->|text|`) at the start of `text`.
/// 解析 `text` 开头的一条连线（`-->`、`---`、`-.->`、`==>`、`~~~`、`--o`、`<-->`、`-- text -->`、
/// `-->|text|`）。
fn parse_link(text: &str) -> Option<(Link, &str)> {
    let bytes = text.as_bytes();
    let tail = bytes.first() == Some(&b'<');
    let start = usize::from(tail);
    let run = bytes[start..]
        .iter()
        .take_while(|b| matches!(b, b'-' | b'.' | b'=' | b'~'))
        .count();
    if run < 2 {
        return None;
    }
    let mut end = start + run;
    let mut head = match bytes.get(end) {
        Some(b'>') => true,
        Some(b'x' | b'o') => bytes.get(end + 1).is_none_or(u8::is_ascii_whitespace),
        _ => false,
    };
    if head {
        end += 1;
    }
    let body = &text[start..start + run];
    let mut rest = &text[end..];
    let mut label = String::new();
    if !head && matches!(body, "--" | "==" | "-.") && rest.starts_with(char::is_whitespace) {
        let (inner, after, closed_head) = closing_link(rest)?;
        label = clean_label(inner);
        rest = after;
        head = closed_head;
    }
    if let Some(after) = rest.trim_start().strip_prefix('|') {
        let close = after.find('|')?;
        label = clean_label(&after[..close]);
        rest = &after[close + 1..];
    }
    let link = Link {
        label: label.replace('\n', " "),
        head,
        tail: tail && head,
        hidden: body.starts_with('~'),
    };
    Some((link, rest))
}

/// Finds the end of a `-- text -->` link: the label, what follows and whether it has a head.
/// 查找 `-- text -->` 连线的结尾：标签、其后内容以及是否带箭头。
fn closing_link(text: &str) -> Option<(&str, &str, bool)> {
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        let opens = matches!(bytes[at], b'-' | b'.' | b'=')
            && (at == 0 || bytes[at - 1].is_ascii_whitespace());
        if !opens {
            at += 1;
            continue;
        }
        let run = bytes[at..]
            .iter()
            .take_while(|b| matches!(b, b'-' | b'.' | b'='))
            .count();
        let after = at + run;
        if run >= 2 && bytes.get(after) == Some(&b'>') {
            return Some((&text[..at], &text[after + 1..], true));
        }
        if run >= 3 {
            return Some((&text[..at], &text[after..], false));
        }
        at = after;
    }
    None
}

fn parse_state(body: &[&str]) -> Option<Graph> {
    let mut graph = Graph::new(true, Shape::Round);
    let mut in_note = false;
    for line in body {
        if in_note {
            in_note = !line.eq_ignore_ascii_case("end note");
            continue;
        }
        let keyword = line.split_whitespace().next().unwrap_or("");
        match keyword {
            "direction" => {
                graph.down = is_down(line.split_whitespace().nth(1));
                continue;
            }
            "note" => {
                in_note = !line.contains(':');
                continue;
            }
            "}" | "--" | "classDef" | "class" | "style" | "hide" | "scale" => continue,
            _ if keyword.starts_with("accTitle") || keyword.starts_with("accDescr") => continue,
            _ => {}
        }
        if keyword == "state" {
            let rest = line["state".len()..].trim().trim_end_matches('{').trim();
            if let Some(quoted) = rest.strip_prefix('"') {
                let close = quoted.find('"')?;
                let id = quoted[close + 1..].trim().strip_prefix("as")?.trim();
                graph.node(id, Some(clean_label(&quoted[..close])), None);
            } else {
                let mut words = rest.split_whitespace();
                let id = words.next()?;
                let shape = (words.next() == Some("<<choice>>")).then_some(Shape::Decision);
                graph.node(id, None, shape);
            }
            continue;
        }
        if let Some((left, right)) = line.split_once("-->") {
            let (right, label) = match right.split_once(':') {
                Some((right, label)) => (right, clean_label(label)),
                None => (right, String::new()),
            };
            let from = state_node(&mut graph, left.trim(), true)?;
            let to = state_node(&mut graph, right.trim(), false)?;
            graph.edges.push(Edge {
                from,
                to,
                label: label.replace('\n', " "),
                head: true,
                tail: false,
                hidden: false,
            });
            continue;
        }
        if let Some((id, description)) = line.split_once(':') {
            state_node(&mut graph, id.trim(), true)?;
            graph.node(id.trim(), Some(clean_label(description)), None);
            continue;
        }
        state_node(&mut graph, line, true)?;
    }
    graph.checked()
}

/// A state reference; `[*]` is the start state as a source and the end state as a target.
/// 状态引用；`[*]` 作为来源时是起始状态，作为目标时是结束状态。
fn state_node(graph: &mut Graph, id: &str, source: bool) -> Option<usize> {
    if id.is_empty() || id.contains(char::is_whitespace) {
        return None;
    }
    Some(match (id, source) {
        ("[*]", true) => graph.node("[*]start", Some("●".into()), None),
        ("[*]", false) => graph.node("[*]end", Some("◉".into()), None),
        _ => graph.node(id, None, None),
    })
}

/// Draws the graph in its own direction, or turned when that is too wide.
/// 按图自身方向绘制；过宽时改用另一方向。
fn flowchart(graph: &Graph, width: usize) -> Option<Canvas> {
    [graph.down, !graph.down]
        .into_iter()
        .filter_map(|down| layered(graph, down))
        .find(|canvas| canvas.used_width() <= width)
}

/// A node of the layered drawing: a graph node, or a bend point carrying a long edge through a
/// layer.
/// 分层绘图中的一个节点：图节点，或让长边穿过某一层的折点。
struct Placed {
    real: bool,
    layer: usize,
    lines: Vec<String>,
    shape: Shape,
    /// Extent along the layer axis and across it.
    /// 沿分层方向与垂直方向的尺寸。
    main: usize,
    cross: usize,
}

/// An edge drawn through consecutive layers, upper end first.
/// 穿过相邻各层绘制的一条边，上端在前。
struct Path {
    stops: Vec<usize>,
    head_lower: bool,
    head_upper: bool,
    /// Drawn against its direction to keep the layers acyclic.
    /// 为保持分层无环而逆向绘制。
    back: bool,
    label: String,
}

fn layered(graph: &Graph, down: bool) -> Option<Canvas> {
    let n = graph.nodes.len();
    let back = back_edges(graph);
    let oriented: Vec<Option<(usize, usize)>> = graph
        .edges
        .iter()
        .zip(&back)
        .map(|(edge, back)| {
            (edge.from != edge.to).then_some(if *back {
                (edge.to, edge.from)
            } else {
                (edge.from, edge.to)
            })
        })
        .collect();
    let layer = longest_path_layers(n, oriented.iter().flatten().copied());

    // Nodes, with a loop mark on nodes that link to themselves.
    // 节点；链接到自身的节点带循环标记。
    let mut placed: Vec<Placed> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let looped = graph.edges.iter().any(|e| e.from == index && e.to == index);
            let label = if looped {
                format!("{} ↺", node.label)
            } else {
                node.label.clone()
            };
            let lines = label_lines(&label, LABEL_MAX);
            let width = lines.iter().map(|l| width_of(l)).max().unwrap_or(0) + 4;
            let height = lines.len() + 2;
            let (main, cross) = if down {
                (height, width)
            } else {
                (width, height)
            };
            Placed {
                real: true,
                layer: layer[index],
                lines,
                shape: node.shape,
                main,
                cross,
            }
        })
        .collect();
    let mut paths = Vec::new();
    for ((edge, oriented), back) in graph.edges.iter().zip(&oriented).zip(&back) {
        let Some((upper, lower)) = *oriented else {
            continue;
        };
        if edge.hidden {
            continue;
        }
        let mut stops = vec![upper];
        for between in layer[upper] + 1..layer[lower] {
            placed.push(Placed {
                real: false,
                layer: between,
                lines: Vec::new(),
                shape: Shape::Rect,
                main: 0,
                cross: 1,
            });
            stops.push(placed.len() - 1);
        }
        stops.push(lower);
        let (head_lower, head_upper) = if *back {
            (edge.tail, edge.head)
        } else {
            (edge.head, edge.tail)
        };
        paths.push(Path {
            stops,
            head_lower,
            head_upper,
            back: *back,
            label: truncate(&edge.label, LABEL_MAX),
        });
    }

    // Order each layer to reduce crossings.
    // 为减少交叉而排序各层。
    let depth = layer.iter().max().map_or(1, |d| d + 1);
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); depth];
    for (index, node) in placed.iter().enumerate() {
        layers[node.layer].push(index);
    }
    let mut uppers: Vec<Vec<usize>> = vec![Vec::new(); placed.len()];
    let mut lowers: Vec<Vec<usize>> = vec![Vec::new(); placed.len()];
    for path in &paths {
        for pair in path.stops.windows(2) {
            lowers[pair[0]].push(pair[1]);
            uppers[pair[1]].push(pair[0]);
        }
    }
    for _ in 0..4 {
        for index in 1..depth {
            reorder(&mut layers, index, index - 1, &uppers);
        }
        for index in (0..depth.saturating_sub(1)).rev() {
            reorder(&mut layers, index, index + 1, &lowers);
        }
    }

    // Positions across the layers: line each node up with its neighbours where space allows.
    // 垂直于分层方向的位置：在空间允许时使各节点与其相邻节点对齐。
    let gap = if down { 2 } else { 1 };
    let mut across = vec![0isize; placed.len()];
    for layer in &layers {
        let mut next = 0;
        for &index in layer {
            across[index] = next;
            next += (placed[index].cross + gap) as isize;
        }
    }
    let sizes: Vec<usize> = placed.iter().map(|p| p.cross).collect();
    for (from, neighbors) in [(1, &uppers), (0, &lowers), (1, &uppers)] {
        let order: Vec<usize> = if from == 1 {
            (1..depth).collect()
        } else {
            (0..depth.saturating_sub(1)).rev().collect()
        };
        for index in order {
            align(&layers[index], neighbors, &sizes, gap, &mut across);
        }
    }
    let low = across.iter().copied().min().unwrap_or(0);
    let across: Vec<usize> = across.iter().map(|x| (x - low) as usize).collect();
    let center = |index: usize| across[index] + placed[index].cross / 2;
    let attach = |index: usize, back: bool| {
        let node = &placed[index];
        let offset = match (back && node.real, down) {
            (true, true) if node.cross >= 7 => 2,
            (true, false) if node.cross >= 5 => 1,
            _ => 0,
        };
        center(index) + offset
    };

    // Layer thickness and the gaps between layers: a stub, one routing lane per source, a label
    // run, and the arrow.
    // 层厚度与层间间隙：引出段、每个来源一条走线通道、标签段，以及箭头。
    let thickness: Vec<usize> = layers
        .iter()
        .map(|layer| {
            layer
                .iter()
                .filter(|i| placed[**i].real)
                .map(|i| placed[*i].main)
                .max()
                .unwrap_or(1)
        })
        .collect();
    let mut lanes: HashMap<(usize, usize), usize> = HashMap::new();
    let mut lane_count = vec![0; depth];
    let mut label_run = vec![0; depth];
    for path in &paths {
        let last = path.stops.len() - 2;
        for (step, pair) in path.stops.windows(2).enumerate() {
            let upper_layer = placed[pair[0]].layer;
            let (from, to) = (attach(pair[0], path.back), attach(pair[1], path.back));
            if from != to && !lanes.contains_key(&(pair[0], from)) {
                lanes.insert((pair[0], from), lane_count[upper_layer]);
                lane_count[upper_layer] += 1;
            }
            if step == last && !path.label.is_empty() {
                let run = if down { 1 } else { width_of(&path.label) + 4 };
                label_run[upper_layer] = label_run[upper_layer].max(run);
            }
        }
    }
    let mut start = vec![0; depth];
    for index in 1..depth {
        start[index] = start[index - 1]
            + thickness[index - 1]
            + 2
            + lane_count[index - 1]
            + label_run[index - 1];
    }
    let length = start[depth - 1] + thickness[depth - 1];
    let breadth = (0..placed.len())
        .map(|i| across[i] + placed[i].cross)
        .max()
        .unwrap_or(0);
    let at = |main: usize, cross: usize| if down { (cross, main) } else { (main, cross) };
    let (width, height) = if down {
        (breadth + LABEL_MAX + 4, length)
    } else {
        (length, breadth)
    };
    let mut canvas = Canvas::new(width, height);

    for (index, node) in placed.iter().enumerate() {
        let main = start[node.layer];
        if node.real {
            let size = if down {
                (node.cross, node.main)
            } else {
                (node.main, node.cross)
            };
            draw_box(
                &mut canvas,
                at(main, across[index]),
                size,
                node.shape,
                &node.lines,
                Ink::Border,
            );
        } else {
            let end = main + thickness[node.layer] - 1;
            canvas.link(at(main, across[index]), at(end, across[index]));
        }
    }
    let (out_mark, in_mark, down_arrow, up_arrow) = if down {
        ('┬', '┴', '▼', '▲')
    } else {
        ('├', '┤', '▶', '◀')
    };
    let mut marks: Vec<((usize, usize), char, Ink)> = Vec::new();
    let mut labels: Vec<(usize, usize, &str)> = Vec::new();
    for path in &paths {
        let last = path.stops.len() - 2;
        for (step, pair) in path.stops.windows(2).enumerate() {
            let (upper, lower) = (&placed[pair[0]], &placed[pair[1]]);
            let layer = upper.layer;
            let (from, to) = (attach(pair[0], path.back), attach(pair[1], path.back));
            let gap_start = start[layer] + thickness[layer];
            let leave = if upper.real {
                start[layer] + upper.main
            } else {
                gap_start
            };
            let arrow = start[layer + 1] - 1;
            let reach = if lower.real { arrow } else { start[layer + 1] };
            if from == to {
                canvas.link(at(leave, from), at(reach, to));
            } else {
                let lane = gap_start + 1 + lanes[&(pair[0], from)];
                canvas.link(at(leave, from), at(lane, from));
                canvas.link(at(lane, from), at(lane, to));
                canvas.link(at(lane, to), at(reach, to));
            }
            if upper.real {
                if step == 0 && path.head_upper {
                    marks.push((at(leave, from), up_arrow, Ink::Arrow));
                } else {
                    marks.push((at(leave - 1, from), out_mark, Ink::Border));
                }
            }
            if lower.real {
                if step == last && path.head_lower {
                    marks.push((at(arrow, to), down_arrow, Ink::Arrow));
                } else {
                    marks.push((at(start[layer + 1], to), in_mark, Ink::Border));
                }
                if step == last && !path.label.is_empty() {
                    let run = gap_start + 1 + lane_count[layer];
                    labels.push((run, to, &path.label));
                }
            }
        }
    }
    for ((x, y), ch, ink) in marks {
        canvas.put(x, y, ch, ink);
    }
    for (run, cross, label) in labels {
        let label_width = width_of(label);
        if down {
            // Beside the line into the target on whichever side is clear, else at the nearest
            // clear place to its right, so labels sharing a row never overwrite each other.
            // 位于进入目标的线旁空闲的一侧，否则放在其右侧最近的空闲处，使同一行的标签互不覆盖。
            let clear = |canvas: &Canvas, x: usize| {
                (x.saturating_sub(1)..=x + label_width).all(|x| canvas.is_free(x, run))
            };
            let left = cross.checked_sub(label_width + 1);
            let x = if clear(&canvas, cross + 2) {
                cross + 2
            } else if let Some(left) = left.filter(|left| clear(&canvas, *left)) {
                left
            } else {
                (cross + 3..canvas.width)
                    .find(|x| clear(&canvas, *x))
                    .unwrap_or(cross + 2)
            };
            canvas.text(x, run, label, Ink::Label);
        } else {
            canvas.text(run + 1, cross, &format!(" {label} "), Ink::Label);
        }
    }
    Some(canvas)
}

/// Marks edges that close a cycle in a depth-first walk; drawing them reversed keeps layers acyclic.
/// 标记深度优先遍历中闭合环路的边；将其逆向绘制可使分层无环。
fn back_edges(graph: &Graph) -> Vec<bool> {
    let n = graph.nodes.len();
    let mut outgoing: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    for (index, edge) in graph.edges.iter().enumerate() {
        if edge.from != edge.to {
            outgoing[edge.from].push((edge.to, index));
        }
    }
    // 0: unvisited, 1: on the walk, 2: finished.
    // 0：未访问，1：在当前路径上，2：已完成。
    let mut state = vec![0u8; n];
    let mut back = vec![false; graph.edges.len()];
    for root in 0..n {
        if state[root] != 0 {
            continue;
        }
        state[root] = 1;
        let mut stack = vec![(root, 0)];
        while let Some(&(node, cursor)) = stack.last() {
            match outgoing[node].get(cursor) {
                Some(&(to, edge)) => {
                    if let Some(top) = stack.last_mut() {
                        top.1 += 1;
                    }
                    match state[to] {
                        0 => {
                            state[to] = 1;
                            stack.push((to, 0));
                        }
                        1 => back[edge] = true,
                        _ => {}
                    }
                }
                None => {
                    state[node] = 2;
                    stack.pop();
                }
            }
        }
    }
    back
}

/// Puts every node one layer below its deepest predecessor.
/// 将每个节点放在其最深前驱的下一层。
fn longest_path_layers(n: usize, edges: impl Iterator<Item = (usize, usize)>) -> Vec<usize> {
    let mut successors: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut indegree = vec![0; n];
    for (from, to) in edges {
        successors[from].push(to);
        indegree[to] += 1;
    }
    let mut queue: Vec<usize> = (0..n).filter(|i| indegree[*i] == 0).collect();
    let mut layer = vec![0; n];
    let mut next = 0;
    while next < queue.len() {
        let node = queue[next];
        next += 1;
        for &to in &successors[node] {
            layer[to] = layer[to].max(layer[node] + 1);
            indegree[to] -= 1;
            if indegree[to] == 0 {
                queue.push(to);
            }
        }
    }
    layer
}

/// Sorts one layer by the mean position of each node's neighbours in the reference layer.
/// 按各节点在参照层中相邻节点的平均位置对一层排序。
fn reorder(layers: &mut [Vec<usize>], layer: usize, reference: usize, neighbors: &[Vec<usize>]) {
    let position: HashMap<usize, usize> = layers[reference]
        .iter()
        .enumerate()
        .map(|(index, node)| (*node, index))
        .collect();
    let mut keyed: Vec<(f64, usize)> = layers[layer]
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let found: Vec<usize> = neighbors[*node]
                .iter()
                .filter_map(|n| position.get(n).copied())
                .collect();
            let key = if found.is_empty() {
                index as f64
            } else {
                found.iter().sum::<usize>() as f64 / found.len() as f64
            };
            (key, *node)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
    layers[layer] = keyed.into_iter().map(|(_, node)| node).collect();
}

/// Moves each node of a layer toward the mean center of its neighbours, keeping the layer order and
/// `gap` between nodes.
/// 将一层中的各节点移向其相邻节点的平均中心，同时保持层内顺序与节点间距 `gap`。
fn align(layer: &[usize], neighbors: &[Vec<usize>], sizes: &[usize], gap: usize, at: &mut [isize]) {
    let mut next: Option<isize> = None;
    for &node in layer {
        let found = &neighbors[node];
        let desired = if found.is_empty() {
            at[node]
        } else {
            let sum: isize = found
                .iter()
                .map(|n| at[*n] + (sizes[*n] / 2) as isize)
                .sum();
            sum / found.len() as isize - (sizes[node] / 2) as isize
        };
        let position = next.map_or(desired, |next| desired.max(next));
        at[node] = position;
        next = Some(position + (sizes[node] + gap) as isize);
    }
}

// ---- Sequence diagrams ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Head {
    Filled,
    Open,
    Cross,
    Async,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Over,
    Left,
    Right,
}

enum Step {
    Message {
        from: usize,
        to: usize,
        text: String,
        dashed: bool,
        head: Head,
    },
    Note {
        first: usize,
        last: usize,
        side: Side,
        lines: Vec<String>,
    },
    Frame(String),
    End,
}

#[derive(Default)]
struct Sequence {
    actors: Vec<String>,
    ids: HashMap<String, usize>,
    steps: Vec<Step>,
}

impl Sequence {
    fn actor(&mut self, id: &str, label: Option<String>) -> Option<usize> {
        if id.is_empty() || id.contains(char::is_whitespace) {
            return None;
        }
        let index = match self.ids.get(id) {
            Some(index) => *index,
            None => {
                self.actors.push(id.to_string());
                self.ids.insert(id.to_string(), self.actors.len() - 1);
                self.actors.len() - 1
            }
        };
        if let Some(label) = label {
            self.actors[index] = label.replace('\n', " ");
        }
        Some(index)
    }

    fn declare(&mut self, text: &str) -> Option<()> {
        let (id, label) = match text.split_once(" as ") {
            Some((id, label)) => (id.trim(), Some(clean_label(label))),
            None => (text.trim(), None),
        };
        self.actor(id, label).map(|_| ())
    }
}

/// Arrow tokens, each with its head and whether the line is dashed.
/// 箭头记号，以及各自的箭头样式与是否为虚线。
const ARROWS: [(&str, Head, bool); 8] = [
    ("-->>", Head::Filled, true),
    ("->>", Head::Filled, false),
    ("--x", Head::Cross, true),
    ("-x", Head::Cross, false),
    ("--)", Head::Async, true),
    ("-)", Head::Async, false),
    ("-->", Head::Open, true),
    ("->", Head::Open, false),
];

fn parse_sequence(body: &[&str]) -> Option<Sequence> {
    let mut sequence = Sequence::default();
    let mut number: Option<usize> = None;
    // Whether each open block draws frame lines (`box` groups participants and does not).
    // 每个未闭合区块是否绘制框线（`box` 只为参与者分组，不绘制）。
    let mut blocks: Vec<bool> = Vec::new();
    for line in body {
        let (keyword, rest) = line
            .split_once(char::is_whitespace)
            .map_or((*line, ""), |(keyword, rest)| (keyword, rest.trim()));
        match keyword.trim_end_matches(':') {
            "participant" | "actor" => {
                sequence.declare(rest)?;
                continue;
            }
            "create" => {
                sequence.declare(rest.split_once(char::is_whitespace)?.1)?;
                continue;
            }
            "autonumber" => {
                number = Some(1);
                continue;
            }
            "activate" | "deactivate" | "destroy" | "title" | "accTitle" | "accDescr" | "link"
            | "links" => continue,
            "loop" | "alt" | "opt" | "par" | "critical" | "break" | "rect" => {
                blocks.push(true);
                let label = if keyword == "rect" { keyword } else { line };
                sequence.steps.push(Step::Frame(label.to_string()));
                continue;
            }
            "else" | "and" | "option" => {
                sequence.steps.push(Step::Frame(line.to_string()));
                continue;
            }
            "box" => {
                blocks.push(false);
                continue;
            }
            "end" => {
                if blocks.pop()? {
                    sequence.steps.push(Step::End);
                }
                continue;
            }
            _ => {}
        }
        if keyword.eq_ignore_ascii_case("note") {
            let (place, text) = rest.split_once(':')?;
            let place = place.trim();
            let (side, actors) = if let Some(actors) = place.strip_prefix("right of") {
                (Side::Right, actors)
            } else if let Some(actors) = place.strip_prefix("left of") {
                (Side::Left, actors)
            } else {
                (Side::Over, place.strip_prefix("over")?)
            };
            let ids = actors
                .split(',')
                .map(|actor| sequence.actor(actor.trim(), None))
                .collect::<Option<Vec<usize>>>()?;
            let (first, last) = (*ids.iter().min()?, *ids.iter().max()?);
            sequence.steps.push(Step::Note {
                first,
                last,
                side,
                lines: label_lines(&clean_label(text), MESSAGE_MAX),
            });
            continue;
        }
        let (at, (token, head, dashed)) = ARROWS
            .iter()
            .filter_map(|arrow| line.find(arrow.0).map(|at| (at, arrow)))
            .min_by_key(|(at, arrow)| (*at, std::cmp::Reverse(arrow.0.len())))?;
        let from = sequence.actor(line[..at].trim(), None)?;
        let rest = line[at + token.len()..]
            .trim_start()
            .trim_start_matches(['+', '-']);
        let (to, text) = rest.split_once(':').unwrap_or((rest, ""));
        let to = sequence.actor(to.trim(), None)?;
        let mut text = clean_label(text).replace('\n', " ");
        if let Some(n) = &mut number {
            text = format!("{n}. {text}");
            *n += 1;
        }
        sequence.steps.push(Step::Message {
            from,
            to,
            text: truncate(text.trim(), MESSAGE_MAX),
            dashed: *dashed,
            head: *head,
        });
    }
    (!sequence.actors.is_empty()).then_some(sequence)
}

fn head_char(head: Head, rightward: bool) -> Option<char> {
    match (head, rightward) {
        (Head::Filled, true) => Some('▶'),
        (Head::Filled, false) => Some('◀'),
        (Head::Async, true) => Some('▷'),
        (Head::Async, false) => Some('◁'),
        (Head::Cross, _) => Some('×'),
        (Head::Open, _) => None,
    }
}

fn sequence(sequence: &Sequence, width: usize) -> Option<Canvas> {
    let n = sequence.actors.len();
    let names: Vec<String> = sequence
        .actors
        .iter()
        .map(|a| truncate(a, LABEL_MAX))
        .collect();
    let boxes: Vec<usize> = names.iter().map(|name| width_of(name) + 4).collect();
    let note_width = |lines: &[String]| lines.iter().map(|l| width_of(l)).max().unwrap_or(0) + 4;

    // Distances between neighbouring lifelines, widened so every label fits.
    // 相邻生命线之间的距离，按需加宽以容纳所有标签。
    let mut gaps: Vec<usize> = (1..n)
        .map(|i| boxes[i - 1] - boxes[i - 1] / 2 + boxes[i] / 2 + 2)
        .collect();
    let mut needs: Vec<(usize, usize, usize)> = Vec::new();
    let mut lead = 0;
    let mut trail = 0;
    let mut height = 4;
    for step in &sequence.steps {
        match step {
            Step::Message { from, to, text, .. } if from != to => {
                needs.push((*from.min(to), *from.max(to), width_of(text) + 6));
                height += 1 + usize::from(!text.is_empty());
            }
            Step::Message { from, text, .. } => {
                let room = width_of(text) + 7;
                if from + 1 < n {
                    needs.push((*from, from + 1, room));
                } else {
                    trail = trail.max(room);
                }
                height += 2;
            }
            Step::Note {
                first,
                last,
                side,
                lines,
            } => {
                let note = note_width(lines);
                match side {
                    Side::Over if first == last => {
                        if *first > 0 {
                            needs.push((first - 1, *first, note / 2 + 2));
                        } else {
                            lead = lead.max((note / 2).saturating_sub(boxes[0] / 2));
                        }
                        if first + 1 < n {
                            needs.push((*first, first + 1, note - note / 2 + 2));
                        } else {
                            trail = trail.max(note - note / 2);
                        }
                    }
                    Side::Over => needs.push((*first, *last, note.saturating_sub(4))),
                    Side::Left if *first > 0 => needs.push((first - 1, *first, note + 2)),
                    Side::Left => lead = lead.max((note + 1).saturating_sub(boxes[0] / 2)),
                    Side::Right if first + 1 < n => needs.push((*first, first + 1, note + 3)),
                    Side::Right => trail = trail.max(note + 2),
                }
                height += lines.len() + 2;
            }
            Step::Frame(_) | Step::End => height += 1,
        }
    }
    needs.sort_by_key(|need| need.1);
    for (from, to, distance) in needs {
        let current: usize = gaps[from..to].iter().sum();
        if current < distance {
            gaps[to - 1] += distance - current;
        }
    }
    let mut centers = vec![lead + boxes[0] / 2];
    for gap in &gaps {
        centers.push(centers[centers.len() - 1] + gap);
    }
    let last = centers[n - 1];
    let total = last + boxes[n - 1] - boxes[n - 1] / 2 + trail + 1;
    if total > width {
        return None;
    }
    let mut canvas = Canvas::new(total, height);
    for (index, name) in names.iter().enumerate() {
        let x = centers[index] - boxes[index] / 2;
        draw_box(
            &mut canvas,
            (x, 0),
            (boxes[index], 3),
            Shape::Rect,
            std::slice::from_ref(name),
            Ink::Border,
        );
        canvas.put(centers[index], 2, '┬', Ink::Border);
        for y in 3..height {
            canvas.put(centers[index], y, '│', Ink::Lifeline);
        }
    }
    let mut y = 3;
    for step in &sequence.steps {
        match step {
            Step::Message {
                from,
                to,
                text,
                dashed,
                head,
            } if from != to => {
                let (a, b) = (centers[*from], centers[*to]);
                let (low, high) = (a.min(b), a.max(b));
                if !text.is_empty() {
                    let offset = (high - low - 1).saturating_sub(width_of(text)) / 2;
                    canvas.text(low + 1 + offset, y, text, Ink::Text);
                    y += 1;
                }
                let fill = if *dashed { '╌' } else { '─' };
                for x in low + 1..high {
                    canvas.put(x, y, fill, Ink::Edge);
                }
                let rightward = b > a;
                canvas.put(a, y, if rightward { '├' } else { '┤' }, Ink::Edge);
                if let Some(ch) = head_char(*head, rightward) {
                    canvas.put(if rightward { b - 1 } else { b + 1 }, y, ch, Ink::Arrow);
                }
                y += 1;
            }
            Step::Message {
                from,
                text,
                dashed,
                head,
                ..
            } => {
                let x = centers[*from];
                let fill = if *dashed { '╌' } else { '─' };
                canvas.put(x, y, '├', Ink::Edge);
                canvas.put(x + 1, y, fill, Ink::Edge);
                canvas.put(x + 2, y, fill, Ink::Edge);
                canvas.put(x + 3, y, '┐', Ink::Edge);
                canvas.text(x + 5, y, text, Ink::Text);
                let back = head_char(*head, false).unwrap_or(fill);
                canvas.put(x + 1, y + 1, back, Ink::Arrow);
                canvas.put(x + 2, y + 1, fill, Ink::Edge);
                canvas.put(x + 3, y + 1, '┘', Ink::Edge);
                y += 2;
            }
            Step::Note {
                first,
                last,
                side,
                lines,
            } => {
                let note = note_width(lines);
                let (x, w) = match side {
                    Side::Over if first == last => (centers[*first] - note / 2, note),
                    Side::Over => {
                        let x = centers[*first] - 2;
                        (x, note.max(centers[*last] + 3 - x))
                    }
                    Side::Left => (centers[*first].saturating_sub(note + 1), note),
                    Side::Right => (centers[*first] + 2, note),
                };
                draw_box(
                    &mut canvas,
                    (x, y),
                    (w, lines.len() + 2),
                    Shape::Rect,
                    lines,
                    Ink::Note,
                );
                y += lines.len() + 2;
            }
            Step::Frame(label) => {
                for x in 0..total {
                    canvas.put(x, y, '┄', Ink::Frame);
                }
                canvas.text(
                    1,
                    y,
                    &format!(" {} ", truncate(label, total.saturating_sub(4))),
                    Ink::Frame,
                );
                y += 1;
            }
            Step::End => {
                for x in 0..total {
                    canvas.put(x, y, '┄', Ink::Frame);
                }
                y += 1;
            }
        }
    }
    Some(canvas)
}

// ---- Pie charts ----

/// Draws a pie chart as one bar per slice with its share.
/// 将饼图绘制为每个扇区一条带占比的条形。
fn pie(header: &str, body: &[&str], width: usize, theme: &Theme) -> Option<Vec<Line<'static>>> {
    let mut title = header
        .split_once("title")
        .map(|(_, title)| title.trim().to_string());
    let mut slices: Vec<(String, f64)> = Vec::new();
    for line in body {
        if let Some(text) = line.strip_prefix("title") {
            title = Some(text.trim().to_string());
            continue;
        }
        if line.starts_with("showData")
            || line.starts_with("accTitle")
            || line.starts_with("accDescr")
        {
            continue;
        }
        let (label, value) = line.rsplit_once(':')?;
        let value: f64 = value.trim().parse().ok()?;
        if !value.is_finite() || value < 0.0 {
            return None;
        }
        slices.push((truncate(&clean_label(label), LABEL_MAX), value));
    }
    let total: f64 = slices.iter().map(|(_, value)| value).sum();
    if slices.is_empty() || total <= 0.0 {
        return None;
    }
    let label_width = slices.iter().map(|(label, _)| width_of(label)).max()?;
    // Label, a space, the bar, a space and the share (`100.0%`).
    // 标签、空格、条形、空格与占比（`100.0%`）。
    let room = width.checked_sub(label_width + 8)?;
    if room < 4 {
        return None;
    }
    let palette = [
        theme.primary,
        theme.teal,
        theme.violet,
        theme.warning,
        theme.info,
        theme.success,
        theme.danger,
    ];
    let mut lines = Vec::new();
    if let Some(title) = title.filter(|title| !title.is_empty()) {
        lines.push(Line::from(Span::styled(
            truncate(&title, width),
            theme.strong(),
        )));
    }
    for (index, (label, value)) in slices.iter().enumerate() {
        let share = value / total;
        let bar = ((share * room as f64).round() as usize).max(usize::from(*value > 0.0));
        let pad = " ".repeat(label_width - width_of(label));
        lines.push(Line::from(vec![
            Span::styled(format!("{label}{pad} "), theme.base()),
            Span::styled(
                "█".repeat(bar),
                Style::new().fg(palette[index % palette.len()]),
            ),
            Span::styled(
                format!(" {:.1}%", share * 100.0),
                theme.muted_text().add_modifier(Modifier::DIM),
            ),
        ]));
    }
    Some(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    /// Row and display column of the first line containing `needle`.
    /// 首个包含 `needle` 的行及其显示列。
    fn find(texts: &[String], needle: &str) -> (usize, usize) {
        texts
            .iter()
            .enumerate()
            .find_map(|(row, text)| text.find(needle).map(|at| (row, width_of(&text[..at]))))
            .unwrap_or_else(|| panic!("{needle:?} not drawn in {texts:#?}"))
    }

    fn assert_fits(lines: &[String], width: usize) {
        for line in lines {
            assert!(width_of(line) <= width, "{line:?} is wider than {width}");
        }
    }

    #[test]
    fn top_down_flowcharts_put_targets_below_their_sources() {
        let theme = Theme::dark();
        let source = "flowchart TD\n  A[Start] --> B{Ready?}\n  B -->|Yes| C(Ship)\n  B -->|No| D[Fix]\n  D --> B";
        let lines = texts(&render(source, 60, &theme).expect("drawn"));
        assert_fits(&lines, 60);
        let start = find(&lines, "Start");
        let ready = find(&lines, "Ready?");
        let ship = find(&lines, "Ship");
        let fix = find(&lines, "Fix");
        assert!(start.0 < ready.0 && ready.0 < ship.0 && ready.0 < fix.0);
        assert_eq!(ship.0, fix.0, "both branches sit in one layer");
        // Every label is drawn once, edge labels included, and arrows point both ways because
        // `D --> B` loops back up.
        // 每个标签只绘制一次（含边标签）；由于 `D --> B` 回指上方，箭头双向都有。
        for label in ["Start", "Ready?", "Ship", "Fix", "Yes", "No"] {
            let count: usize = lines.iter().map(|l| l.matches(label).count()).sum();
            assert_eq!(count, 1, "{label} drawn {count} times");
        }
        assert!(lines.iter().any(|l| l.contains('▼')));
        assert!(lines.iter().any(|l| l.contains('▲')));
        assert!(lines.iter().all(|l| !l.contains("-->")));
    }

    #[test]
    fn left_to_right_flowcharts_put_targets_beside_their_sources() {
        let theme = Theme::dark();
        let lines =
            texts(&render("graph LR; A[One] --> B[Two] --> C[Three]", 80, &theme).expect("drawn"));
        let (one, two, three) = (
            find(&lines, "One"),
            find(&lines, "Two"),
            find(&lines, "Three"),
        );
        assert!(
            one.0 == two.0 && two.0 == three.0,
            "a chain stays on one row"
        );
        assert!(one.1 < two.1 && two.1 < three.1);
        assert_eq!(
            lines.iter().map(|l| l.matches('▶').count()).sum::<usize>(),
            2
        );
    }

    #[test]
    fn wide_flowcharts_turn_or_fall_back() {
        let theme = Theme::dark();
        let chain = "graph LR\nA[Alpha node] --> B[Beta node] --> C[Gamma node] --> D[Delta node]";
        // Too wide left to right, so it is drawn top-down instead.
        // 自左向右过宽，因此改为自上而下绘制。
        let lines = texts(&render(chain, 30, &theme).expect("turned"));
        assert_fits(&lines, 30);
        assert!(find(&lines, "Alpha").0 < find(&lines, "Delta").0);
        assert!(render(chain, 8, &theme).is_none(), "no direction fits");
    }

    #[test]
    fn unknown_diagrams_and_statements_are_not_drawn() {
        let theme = Theme::dark();
        assert!(render("classDiagram\n  A <|-- B", 80, &theme).is_none());
        assert!(render("graph TD\n  A --> \n", 80, &theme).is_none());
        assert!(render("", 80, &theme).is_none());
    }

    #[test]
    fn state_diagrams_draw_start_and_end_states() {
        let theme = Theme::dark();
        let source = "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy : start\n  Busy --> Idle : done\n  Busy --> [*]";
        let lines = texts(&render(source, 60, &theme).expect("drawn"));
        let (start, idle, busy, end) = (
            find(&lines, "●"),
            find(&lines, "Idle"),
            find(&lines, "Busy"),
            find(&lines, "◉"),
        );
        assert!(start.0 < idle.0 && idle.0 < busy.0 && busy.0 < end.0);
        // Labels that share a row stay whole.
        // 共用一行的标签保持完整。
        find(&lines, "start");
        find(&lines, "done");
    }

    #[test]
    fn sequence_diagrams_draw_lifelines_messages_and_notes() {
        let theme = Theme::dark();
        let source = "sequenceDiagram\n  participant A as Alice\n  participant B as Bob\n  A->>B: Hello Bob\n  B-->>A: Hi\n  A->>A: Think\n  loop Every minute\n    Note over A,B: sync\n  end";
        let lines = texts(&render(source, 60, &theme).expect("drawn"));
        assert_fits(&lines, 60);
        let (alice, bob) = (find(&lines, "Alice"), find(&lines, "Bob"));
        assert_eq!(alice.0, bob.0, "participants share the header row");
        assert!(alice.1 < bob.1);
        let hello = find(&lines, "Hello Bob");
        let hi = find(&lines, "Hi");
        assert!(hello.0 < hi.0, "messages keep their order");
        assert!(
            hello.1 > alice.1 && hello.1 < bob.1 + 3,
            "a label sits between its lifelines"
        );
        // The request points right, the dashed reply left.
        // 请求向右，虚线回复向左。
        assert!(lines[hello.0 + 1].contains('▶'));
        assert!(lines[hi.0 + 1].contains('◀') && lines[hi.0 + 1].contains('╌'));
        find(&lines, "Think");
        find(&lines, "loop Every minute");
        find(&lines, "sync");
        assert!(render(source, 12, &theme).is_none(), "too narrow to draw");
    }

    #[test]
    fn pie_charts_become_proportional_bars() {
        let theme = Theme::dark();
        let lines = render(
            "pie title Pets\n  \"Dogs\" : 30\n  \"Cats\" : 10",
            40,
            &theme,
        )
        .expect("drawn");
        let texts = texts(&lines);
        assert_eq!(texts[0], "Pets");
        let bar = |row: usize| texts[row].matches('█').count();
        assert!(bar(1) == 3 * bar(2) || bar(1).abs_diff(3 * bar(2)) <= 3);
        assert!(texts[1].ends_with("75.0%") && texts[2].ends_with("25.0%"));
        assert_fits(&texts, 40);
    }
}
