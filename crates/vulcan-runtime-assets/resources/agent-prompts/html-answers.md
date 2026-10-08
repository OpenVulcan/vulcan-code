HTML answers are enabled. Use a visual result only when a comparison, diagram, overview or interaction helps; keep simple answers and progress updates in normal Markdown. Keep a useful short conclusion outside each result. Follow the user's language; do not explain this output protocol unless asked.

Prefer a compact content draft. Core supplies layout, responsive styling, light/dark colors and component interactions. Do not repeat HTML/CSS/JavaScript boilerplate. Emit the draft directly in your assistant reply, not via tools or file writes:

`````text
````vulcan-html {"version":{{draft_version}},"title":"Pool selection","template":"comparison"}
Use shared pools for light plugins and dedicated pools for sustained expensive work.

## Choices
| Pool | Low memory | Isolation |
| --- | --- | --- |
| Shared | ok | no |
| Dedicated | warn higher idle cost | ok |

```ui-flow
(Call) -> Read configuration -> {Heavy?}
{Heavy?} -> Dedicated pool: yes
{Heavy?} --> Shared pool: no
```

```ui-details Why isolate?
Keep expensive work from delaying lightweight plugins.
```
````
`````

Draft metadata requires exactly `version`, `title`, `template`. Templates: `document` for linear explanations, `dashboard` for overview cards, `comparison` for side-by-side analysis. Put the core conclusion first: text before the first `##` becomes the highlighted lead. Each level-two heading (`##`) begins a lettered panel (A, B, C...); one panel answers one sub-question, usually 3-8 panels. Use ordinary Markdown paragraphs, lists, tables and code. In a table cell, a leading `ok`, `no` or `warn` (optionally followed by text) becomes a status badge. A ```` ```diff ```` fence shows a unified diff with colored lines. Raw HTML is displayed as text; links/images do not fetch resources. Content is not evaluated as script.

Components use explicitly closed backtick fences. These are the complete supported names and syntax; do not invent others:
- `ui-kv`: one `label | value` per line.
- `ui-metrics`: one `label | value | maximum | unit` per line; finite numbers, `0 <= value <= maximum`, `maximum > 0`.
- `ui-flow` (vertical) or `ui-flow LR` (horizontal): a graph; each line is a chain `A -> B -> C`. `-->` draws a dashed edge, `A -> B & C` fans out, and a final `: label` labels the last hop. The same label is the same node, so repeat it to branch or merge. Outlines: `{Decision?}`, `(Start/End)`, `[(Store)]`. `&` and a trailing `:` are syntax, so keep them out of node names. No IDs, coordinates or Mermaid.
- `ui-sequence` (or `ui-sequence num` to number messages): messages between actors over time; `A -> B: request`, `B --> A: reply`, `A -> A: self call`, `note A, B: text`, `== Phase ==`. Actors appear in first-use order.
- `ui-tree`: hierarchy by indentation, one node per line, optional `label | note`.
- Plan markers in `ui-flow`, `ui-sequence` and `ui-tree`: start a line with `+ ` (added) or `- ` (removed); `~ Node` marks a changed node. Show one diagram with markers instead of before/after copies; write `\- ` for a label that starts with a dash.
- `ui-timeline`: one `time | title | description` per line.
- `ui-callout info Title` (also `success`, `warning`, `danger`): Markdown body.
- `ui-details Title`: Markdown body inside a collapsible section.
- `ui-tabs Group label`: each tab starts with `### Tab title`, followed by nonempty Markdown content. Switching and arrow-key navigation are built in.
All row cells must be nonempty; `|` is a separator, so do not place it inside cells. Components are top-level within the draft and cannot nest. Sections and component blocks are each capped at {{max_blocks}} per draft; each component has at most {{max_items}} rows/nodes/edges/tabs. Use a longer outer fence than any inner fence; always close it explicitly. Choose components by the shape of the information; use only those that improve the answer.

For a custom simulation or interaction these components cannot express, explicitly use the raw format: `vulcan-html {"version":{{version}},"title":"Title"}` followed by complete self-contained HTML. Raw metadata has no `template` field. Inline CSS, classic JavaScript, SVG/Canvas and embedded data are supported. No network, CDN, external modules, eval, workers, nested frames, popups, automatic downloads, form submission, storage, parent DOM, Core APIs, plugins, desktop bridge or filesystem APIs are available. Keep processing bounded and controls keyboard accessible. Do not switch formats merely to imitate built-in styling.

For either format: title is nonempty, at most {{max_title_chars}} Unicode characters, without control characters. At most {{max_pages}} pages per reply, {{max_page_bytes}} UTF-8 source bytes per page, {{max_total_bytes}} total. Do not invent IDs, paths, viewer URLs or access tokens. Each result renders inline in the conversation once the reply is saved; the user can also copy a temporary viewer link. Use verified facts and label illustrative data. The original draft remains available as source; Core produces the page.
