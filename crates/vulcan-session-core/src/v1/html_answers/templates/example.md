Prefer a shared pool for light plugins; isolate sustained expensive work.

## Overview

```ui-kv
Default | Shared pool
Isolation | Dedicated pool
```

```ui-metrics
Completed checks | 8 | 10 | checks
```

## Comparison

| Pool | Low memory | Isolation |
| --- | --- | --- |
| Shared | ok | no |
| Dedicated | warn higher idle cost | ok |

```ui-flow
(Call) -> Read configuration -> {Heavy?}
{Heavy?} -> Dedicated pool: yes
{Heavy?} --> Shared pool: no
Dedicated pool & Shared pool -> [(VM)] -> (Release)
```

## Request path

```ui-sequence num
Client -> Core: invoke plugin
Core -> Pool: acquire VM
Pool --> Core: VM handle
note Core, Pool: bounded wait
== Execute ==
Core -> Core: run plugin
Core --> Client: result
```

```ui-tree
plugins/
  pool.rs | scheduling
  + dedicated.rs | new pool kind
  ~ config.rs
  - legacy/
```

```diff
@@ -1,2 +1,2 @@
-pool = shared
+pool = dedicated
 timeout = 30
```

```ui-timeline
1 | Prepare | Validate configuration
2 | Execute | Run the plugin
```

```ui-callout info Scope
These values are **illustrative**, not benchmark results.
```

```ui-details More context
A dedicated pool has its own scheduling budget.
```

```ui-tabs Pool selection
### Shared
Use for brief, low-frequency calls.
### Dedicated
Use for sustained, expensive calls.
```
