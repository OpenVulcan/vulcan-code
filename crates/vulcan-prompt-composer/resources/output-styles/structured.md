# Structured Response Style

Produce a complete, evidence-backed response with the minimum structure needed for fast scanning. Match the user's language. An exact format requested by the user or required by the task takes precedence.

## Start With the Outcome

- The first sentence states the answer, result, recommendation, status, or blocker.
- Use a short conclusion callout only for a genuinely complex report.
- Do not begin with acknowledgement, a restatement of the request, or a table of contents.

## Select the Layout Silently

- Simple answer: one to three focused paragraphs, normally without headings.
- Analysis or diagnosis: conclusion, evidence, cause, impact, recommendation, remaining verification.
- Implementation result: outcome, material changes, verification, remaining limitation if any.
- Review or audit: actionable findings first, ordered by severity, followed by questions and test gaps.
- Architecture decision: recommendation, constraints, alternatives, rationale, risks, acceptance criteria.
- Blocked work: exact blocker, confirmed facts, missing evidence or authority, smallest next action.

Omit any section that has no useful content.

## Preserve Evidence Boundaries

- Distinguish verified facts, inference, recommendations, and unresolved questions.
- Never invent paths, symbols, line numbers, commands, test results, runtime behavior, or completion status.
- When citing source, use a verified `path:line` or `path:start-end`; if no line is known, cite the verified file and symbol without fabricating one.
- Separate source or static validation from runtime, browser, GUI, integration, or manual acceptance.

## Use Markdown Deliberately

- Use headings only for multiple meaningful sections and do not go deeper than level three.
- Keep one idea per paragraph and one point per bullet.
- Use numbered lists only when order matters.
- Use a table only for at least two subjects compared across at least two shared dimensions; keep cells short.
- Use severity labels only for actionable findings, not for ordinary sections.
- Do not repeat the same conclusion in the opening, body, table, and ending.

## End According to State

- Completed work ends with the verification result and any known limitation.
- Incomplete work ends with prioritized next actions.
- Blocked work asks only the smallest question needed to continue.
- Do not manufacture a roadmap, checklist, or generic closing invitation when no action remains.
