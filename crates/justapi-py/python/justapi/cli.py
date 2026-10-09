"""JustAPI CLI — Python package entry point.

Provides the core commands standalone (no Rust binary required) and
transparently delegates to the high-performance Rust CLI (`justapi-cli`)
when it is installed via `cargo install justapi-cli`.

Commands:
  justapi --version       Show version
  justapi serve [addr]    Run the app in the current directory (main.py / app.py)
  justapi create NAME     Scaffold a new project
  justapi check [file]    Validate routes without starting the server
  justapi openapi [file]  Print the OpenAPI spec
"""

import argparse
import importlib.util
import os
import subprocess
import sys
import shutil

__version__ = "2.0.10"

RUST_BINARY = "justapi-cli"  # cargo install justapi-cli → binary name


def _rust_cli() -> str | None:
    """Return the path to the Rust CLI if installed, else None."""
    return shutil.which(RUST_BINARY)


def _delegate_or(args: list[str]) -> bool:
    """If the Rust CLI is installed, exec it with args. Returns True if delegated."""
    rust = _rust_cli()
    if rust:
        sys.exit(subprocess.call([rust] + args))
    return False


def _find_app_file() -> str | None:
    """Locate the user's app module: app.py, main.py, or src/app.py."""
    for name in ("app.py", "main.py", "src/app.py"):
        if os.path.isfile(name):
            return name
    return None


def cmd_version(args: argparse.Namespace) -> None:
    rust = _rust_cli()
    print(f"justapi {__version__} (Python package)")
    if rust:
        print(f"Rust CLI detected: {rust}")
    print(f"Python {sys.version.split()[0]} · PyPI package")


def _load_and_run(app_file: str, addr: str, reuse_port: bool = False) -> None:
    """Import the app module and start the server (blocking).

    ``reuse_port`` binds with SO_REUSEPORT so several worker processes can
    share one address (`justapi serve --workers N`).
    """
    # Import the app module — its `app.run(...)` (or `if __name__`) starts the server.
    module_name = os.path.splitext(app_file)[0].replace("/", ".")
    spec = importlib.util.spec_from_file_location(module_name, app_file)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.argv = [app_file, addr]
    spec.loader.exec_module(module)
    # If the module only defines `app` without calling run(), call it for the user.
    app = getattr(module, "app", None)
    if app is not None and not getattr(module, "_justapi_ran", False):
        app.run(addr, reuse_port=reuse_port)


def cmd_serve(args: argparse.Namespace) -> None:
    """Run the app in the current directory.

    Deliberately does NOT delegate to the Rust CLI: the Rust binary cannot
    load a Python application (it only serves its built-in routes / static
    files), so delegation would silently serve 404s instead of the user's
    app. Use `justapi-cli serve` directly for static-only serving.

    Supports ``--workers N`` (prefork, SO_REUSEPORT) and ``--reload``
    (watch-and-restart supervisor) for Python apps.
    """
    app_file = _find_app_file()
    if app_file is None:
        print("error: no app.py or main.py found in the current directory", file=sys.stderr)
        sys.exit(1)

    workers = max(int(args.workers or 1), 1)
    if args.reload:
        _serve_with_reload(args, app_file, workers)
        return
    if workers > 1:
        _serve_prefork(args, app_file, workers)
        return

    print(f"justapi {__version__} — serving {app_file} on {args.addr}")
    _load_and_run(app_file, args.addr)


def _serve_prefork(args: argparse.Namespace, app_file: str, workers: int) -> None:
    """Run N worker processes sharing one address via SO_REUSEPORT.

    Workers that die unexpectedly are respawned (matching the Rust CLI
    supervisor). Unix-only: relies on fork(2).
    """
    import multiprocessing
    import signal
    import time

    try:
        ctx = multiprocessing.get_context("fork")
    except ValueError:
        print(
            "error: --workers requires a fork-capable platform (Unix)",
            file=sys.stderr,
        )
        sys.exit(1)

    print(f"justapi {__version__} — serving {app_file} on {args.addr} with {workers} workers")
    children: list = []
    stop = {"flag": False}

    def _spawn():
        proc = ctx.Process(target=_load_and_run, args=(app_file, args.addr, True))
        proc.start()
        return proc

    def _shutdown(_signum, _frame):
        stop["flag"] = True
        for proc in children:
            if proc.is_alive():
                proc.terminate()

    signal.signal(signal.SIGTERM, _shutdown)
    signal.signal(signal.SIGINT, _shutdown)

    children.extend(_spawn() for _ in range(workers))
    try:
        while not stop["flag"]:
            time.sleep(0.2)
            for idx, proc in enumerate(children):
                if not proc.is_alive() and not stop["flag"]:
                    children[idx] = _spawn()
    finally:
        _shutdown(None, None)
        for proc in children:
            proc.join(timeout=10)


def _snapshot_tree(root: str) -> dict:
    """`{path: (mtime_ns, size)}` for *.py files under ``root`` (skip noise)."""
    skip = {".git", ".venv", "venv", "__pycache__", "node_modules", ".venv-ft", ".venv-p53"}
    out: dict = {}
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in skip and not d.startswith(".")]
        for name in filenames:
            if name.endswith(".py"):
                full = os.path.join(dirpath, name)
                try:
                    st = os.stat(full)
                except OSError:
                    continue
                out[full] = (st.st_mtime_ns, st.st_size)
    return out


def _serve_with_reload(args: argparse.Namespace, app_file: str, workers: int) -> None:
    """Supervise a child ``justapi serve`` and restart it when *.py files change."""
    import signal
    import subprocess
    import time

    cmd = [sys.executable, "-m", "justapi", "serve", args.addr]
    if workers > 1:
        cmd += ["--workers", str(workers)]
    print(f"justapi {__version__} — serving {app_file} on {args.addr} (reload enabled)")

    stop = {"flag": False}

    def _shutdown(_signum, _frame):
        stop["flag"] = True

    signal.signal(signal.SIGTERM, _shutdown)
    signal.signal(signal.SIGINT, _shutdown)

    child = subprocess.Popen(cmd)
    state = _snapshot_tree(os.getcwd())
    try:
        while not stop["flag"]:
            time.sleep(0.4)
            if child.poll() is not None:
                sys.exit(child.returncode)
            new_state = _snapshot_tree(os.getcwd())
            if new_state != state:
                print("justapi — change detected, restarting...")
                child.terminate()
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
                child = subprocess.Popen(cmd)
                state = new_state
    finally:
        # Terminate the child on any exit path (SIGTERM/SIGINT included), so
        # the port is released and no orphan keeps serving.
        if child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()


def cmd_create(args: argparse.Namespace) -> None:
    """Scaffold a new project. Delegates to Rust CLI when available."""
    if _delegate_or(["create", args.name] + args.rest):
        return
    print("error: `justapi create` requires the Rust CLI:", file=sys.stderr)
    print("  cargo install justapi-cli", file=sys.stderr)
    sys.exit(1)


def cmd_check(args: argparse.Namespace) -> None:
    """Validate the app's routes without starting the server."""
    if _delegate_or(["check"] + ([args.file] if args.file else [])):
        return
    app_file = args.file or _find_app_file()
    if app_file is None:
        print("error: no app.py or main.py found", file=sys.stderr)
        sys.exit(1)
    spec = importlib.util.spec_from_file_location("app", app_file)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    app = getattr(module, "app", None)
    if app is None:
        print(f"error: {app_file} does not define `app`", file=sys.stderr)
        sys.exit(1)
    print(f"✓ {app_file}: app loaded, routes registered")


def cmd_openapi(args: argparse.Namespace) -> None:
    """Print the OpenAPI spec for the app."""
    if _delegate_or(["openapi"] + ([args.file] if args.file else [])):
        return
    app_file = args.file or _find_app_file()
    if app_file is None:
        print("error: no app.py or main.py found", file=sys.stderr)
        sys.exit(1)
    spec = importlib.util.spec_from_file_location("app", app_file)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    app = getattr(module, "app", None)
    if app is None:
        print(f"error: {app_file} does not define `app`", file=sys.stderr)
        sys.exit(1)
    import json
    spec_dict = app.openapi_spec() if hasattr(app, "openapi_spec") else {}
    print(json.dumps(spec_dict, indent=2))


def build_parser() -> argparse.ArgumentParser:
    """Construct the CLI parser (exposed for tests)."""
    parser = argparse.ArgumentParser(
        prog="justapi",
        description="JustAPI — Python web framework with a Rust core.",
    )
    parser.add_argument("--version", "-V", action="store_true", help="Show version")
    sub = parser.add_subparsers(dest="command")

    p_serve = sub.add_parser("serve", help="Run the app in the current directory")
    p_serve.add_argument("addr", nargs="?", default="127.0.0.1:8000", help="Address to bind (default 127.0.0.1:8000)")
    p_serve.add_argument("--addr", dest="addr_flag", default=None, help="Address to bind (flag form, overrides positional)")
    p_serve.add_argument("--workers", type=int, default=1, help="Number of worker processes (prefork, SO_REUSEPORT)")
    p_serve.add_argument("--reload", action="store_true", help="Watch *.py files and restart on change")

    p_create = sub.add_parser("create", help="Scaffold a new project (requires Rust CLI)")
    p_create.add_argument("name", help="Project name")
    p_create.add_argument("rest", nargs=argparse.REMAINDER, help="Extra args passed to the Rust CLI")

    p_check = sub.add_parser("check", help="Validate the app without starting the server")
    p_check.add_argument("file", nargs="?", help="App file (default: auto-detect)")

    p_openapi = sub.add_parser("openapi", help="Print the OpenAPI spec")
    p_openapi.add_argument("file", nargs="?", help="App file (default: auto-detect)")

    return parser


def main(argv: list[str] | None = None) -> None:
    argv = argv if argv is not None else sys.argv[1:]
    parser = build_parser()
    args = parser.parse_args(argv)

    if args.version:
        cmd_version(args)
        return

    if args.command == "serve":
        if args.addr_flag:
            args.addr = args.addr_flag
        cmd_serve(args)
    elif args.command == "create":
        cmd_create(args)
    elif args.command == "check":
        cmd_check(args)
    elif args.command == "openapi":
        cmd_openapi(args)
    else:
        # No command → show help / quick start
        if _delegate_or([]):
            return
        parser.print_help()


if __name__ == "__main__":
    main()
