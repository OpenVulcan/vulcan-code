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

| Pool | Strength | Use |
| --- | --- | --- |
| Shared | Lower memory | Light plugins |
| Dedicated | Scheduling isolation | Heavy plugins |

```ui-flow
Call -> Read configuration -> Acquire VM -> Execute -> Release
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
