import asyncio
import pytest
from justapi import Dag, DagNode

def add_prefix(data: str) -> str:
    return "prefix_" + data

def add_suffix(data: str) -> str:
    return data + "_suffix"

def combine(prefix: str, suffix: str) -> str:
    return prefix + "_" + suffix

@pytest.mark.asyncio
async def test_dag_engine():
    # Define DAG nodes
    node_a = DagNode(name="prefix_task", handler=add_prefix, dependencies=["input_data"])
    node_b = DagNode(name="suffix_task", handler=add_suffix, dependencies=["input_data"])
    node_c = DagNode(name="combine_task", handler=combine, dependencies=["prefix_task", "suffix_task"])

    # Create the DAG
    dag = Dag([node_a, node_b, node_c])

    # Execute it by passing initial inputs
    result_state = await dag.execute({"input_data": "middle"})

    # The result state should contain all intermediate results
    assert result_state["prefix_task"] == "prefix_middle"
    assert result_state["suffix_task"] == "middle_suffix"
    assert result_state["combine_task"] == "prefix_middle_middle_suffix"

if __name__ == "__main__":
    asyncio.run(test_dag_engine())
