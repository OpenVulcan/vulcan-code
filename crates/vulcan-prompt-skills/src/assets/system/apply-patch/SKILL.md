---
name: apply-patch
description: Examples and verification workflow for composing atomic apply_patch edits across files, including exact hunks, additions, and moves.
---

# Apply Patch

Read the relevant files and confirm the destination of every move before composing a patch. Choose context lines that identify exactly one location in each file. Send the entire set of changes in one call and inspect the result before retrying a rejected patch.

For example, update an existing function and add its documentation in the same call.

```text
*** Begin Patch
*** Update File: src/example.rs
@@ fn greet() {
-    println!("old");
+    println!("new");
*** Add File: docs/example.md
+The example prints a greeting.
*** End Patch
```

For a move with an edit, put the move destination immediately below the update header.

```text
*** Begin Patch
*** Update File: src/old.rs
*** Move to: src/new.rs
@@ fn greet() {
-    println!("old");
+    println!("new");
*** End Patch
```
