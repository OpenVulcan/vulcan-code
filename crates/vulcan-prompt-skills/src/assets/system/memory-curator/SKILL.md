---
name: memory-curator
description: Use when asked to tidy, clean up, reorganize or review long-term memory, when a memory_write receipt warns that a base section is nearly full or over budget, or when memory_outline shows duplicates, vague titles or misplaced nodes. Requires the memory_outline, memory_search and memory_write tools (Markdown memory mode).
---

# Memory Curator

Long-term memory is three Markdown files of heading nodes: `user` (who the user is), `space` (rules and environment for every project) and `project` (this project only). The `base` sections of space and project are injected in full into every conversation; every other section is injected as titles only. Curation keeps memory accurate, small and easy to scan.

## Workflow

1. Run `memory_outline` for all scopes. Note the budget lines from the last write receipt if you have one.
2. Read candidate nodes with `memory_search` (by `ids`, or by patterns for suspected duplicates) before changing anything. Every change needs the node's current `expectedRevision`.
3. Plan the changes, then apply them with as few `memory_write` calls as practical; one call applies up to 16 operations all-or-nothing.
4. Report what changed: merged, moved, renamed, deleted, and anything you left for the user to decide.

## What to fix

- **Duplicates.** Merge nodes that state the same fact: keep the clearer node, `append` or `replace` the missing details into it, then `delete` the other. Check `space` against `project` too.
- **Stale facts.** For project nodes that name files, functions, flags or commands, verify them with the workspace tools. Correct or delete what no longer holds. When unsure, keep the node and say so in the report.
- **Wrong scope.** Ask: would it still hold in another project? No: `project`. Yes: about the person: `user`; about how to work: `space`. `move` keeps the node id.
- **Heavy base.** Base sections should hold only a few critical, stable rules whose absence causes mistakes. Move the rest to another section (`move` with `section`), most specific first. Finished work in `in-progress` should be deleted or turned into a lasting fact elsewhere.
- **Vague titles.** Rename topic titles ("Testing") to statements of the fact ("Verify with targeted tests only") so catalog lines carry meaning. Titles stay single-line plain text.
- **Oversized nodes.** Split a node that mixes several facts into one node per fact, using child nodes for details (down to H4).

## Limits

- Never delete a node only because it is old; age alone does not make a fact wrong.
- Never rewrite text the user wrote by hand (`source: user`) beyond moving it, unless the user asks; suggest changes instead.
- Do not invent facts while merging. Keep the user's wording where possible.
- The `base` section itself cannot be moved or deleted; change the nodes inside it.
