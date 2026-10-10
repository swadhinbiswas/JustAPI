"""CLI helpers — `--workers` / `--reload` plumbing (unit level).

The end-to-end prefork/reload behaviour is verified by scripts against a real
server; these tests cover the pure-Python helpers that don't need a socket.
"""
import os

from justapi import cli


def test_snapshot_tree_finds_python_and_skips_noise(tmp_path):
    (tmp_path / "app.py").write_text("x = 1\n")
    (tmp_path / "pkg").mkdir()
    (tmp_path / "pkg" / "mod.py").write_text("y = 2\n")
    (tmp_path / "notes.txt").write_text("not python\n")
    (tmp_path / "__pycache__").mkdir()
    (tmp_path / "__pycache__" / "cached.py").write_text("# noise\n")
    (tmp_path / ".venv").mkdir()
    (tmp_path / ".venv" / "lib.py").write_text("# noise\n")

    snap = cli._snapshot_tree(str(tmp_path))
    names = sorted(os.path.basename(p) for p in snap)
    assert names == ["app.py", "mod.py"]


def test_snapshot_tree_detects_changes(tmp_path):
    f = tmp_path / "app.py"
    f.write_text("v = 1\n")
    before = cli._snapshot_tree(str(tmp_path))
    f.write_text("v = 2\n")
    after = cli._snapshot_tree(str(tmp_path))
    assert before != after
    # Same content → same snapshot
    assert cli._snapshot_tree(str(tmp_path)) == after


def test_serve_parser_accepts_workers_and_reload():
    parser = cli.build_parser()

    args = parser.parse_args(["serve", "--addr", "0.0.0.0:9000", "--workers", "3", "--reload"])
    assert args.addr_flag == "0.0.0.0:9000"
    assert args.workers == 3
    assert args.reload is True

    args = parser.parse_args(["serve"])
    assert args.workers == 1
    assert args.reload is False
    assert args.addr == "127.0.0.1:8000"


def test_load_app_module_registers_in_sys_modules(tmp_path, monkeypatch):
    """`justapi serve` must register the app module in sys.modules.

    Pydantic resolves a model's own annotations through
    `sys.modules[cls.__module__]`; without registration, models using
    `from __future__ import annotations` stay "not fully defined" and every
    validated request returns 422 (found by serving examples/expense_analytics
    with `justapi serve --workers 4`).
    """
    import sys

    (tmp_path / "app.py").write_text(
        "from __future__ import annotations\n"
        "\n"
        "from datetime import date\n"
        "\n"
        "from pydantic import BaseModel\n"
        "\n"
        "from justapi import JustAPIApp\n"
        "\n"
        "\n"
        "class Item(BaseModel):\n"
        "    spent_on: date\n"
        "\n"
        "\n"
        "app = JustAPIApp()\n"
        "\n"
        "\n"
        "@app.post('/items', status_code=201)\n"
        "def create(item: Item):\n"
        "    return {'day': str(item.spent_on)}\n"
    )
    monkeypatch.chdir(tmp_path)
    try:
        module = cli._load_app_module("app.py")
        assert sys.modules.get("app") is module
        # The model resolves its `date` annotation (this failed before the fix).
        item = module.Item.model_validate({"spent_on": "2026-10-10"})
        assert str(item.spent_on) == "2026-10-10"
    finally:
        sys.modules.pop("app", None)
