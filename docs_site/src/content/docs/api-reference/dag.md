---
title: Dag and DagNode
description: Execute tasks in a directed acyclic graph (DAG) with dependency resolution in JustAPI.
keywords: [JustAPI, DAG, directed acyclic graph, task orchestration, dependencies]
---

## Basic Usage

Define tasks as nodes and let JustAPI execute them in dependency order:

```python
from justapi import Dag, DagNode

# Define tasks
def fetch_data():
    return {"raw": [1, 2, 3]}

def transform(data):
    return {"transformed": [x * 2 for x in data["raw"]]}

def save(data):
    print(f"Saving: {data}")
    return {"status": "saved"}

# Build DAG
dag = Dag([
    DagNode("fetch", fetch_data),
    DagNode("transform", transform, dependencies=["fetch"]),
    DagNode("save", save, dependencies=["transform"]),
])

# Execute
result = dag.execute({"input": "test"})
# Runs: fetch → transform → save
```

## DagNode

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | `str` | Unique node identifier |
| `handler` | `Callable` | Function to execute |
| `dependencies` | `list[str]` | Names of nodes this depends on |

## Dag.execute

```python
result = dag.execute(inputs={})
# Returns: asyncio.Future with final state dict
```

The `inputs` dict is the initial state. Each node's return value is merged into the state under its `name` key. Dependent nodes receive the accumulated state.

## See Also

- [Background Tasks](/tutorials/background-tasks/) — post-response tasks
- [Resilience Patterns](/advanced/resilience-patterns/) — error handling in DAGs
