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


def cmd_serve(args: argparse.Namespace) -> None:
    """Run the app in the current directory."""
    if _delegate_or([f"--addr={args.addr}"]):
        return
    app_file = _find_app_file()
    if app_file is None:
        print("error: no app.py or main.py found in the current directory", file=sys.stderr)
        sys.exit(1)
    print(f"justapi {__version__} — serving {app_file} on {args.addr}")
    # Import the app module — its `app.run(...)` (or `if __name__`) starts the server.
    module_name = os.path.splitext(app_file)[0].replace("/", ".")
    spec = importlib.util.spec_from_file_location(module_name, app_file)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.argv = [app_file, args.addr]
    spec.loader.exec_module(module)
    # If the module only defines `app` without calling run(), call it for the user.
    app = getattr(module, "app", None)
    if app is not None and not getattr(module, "_justapi_ran", False):
        app.run(args.addr)


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


def main(argv: list[str] | None = None) -> None:
    argv = argv if argv is not None else sys.argv[1:]
    parser = argparse.ArgumentParser(
        prog="justapi",
        description="JustAPI — Python web framework with a Rust core.",
    )
    parser.add_argument("--version", "-V", action="store_true", help="Show version")
    sub = parser.add_subparsers(dest="command")

    p_serve = sub.add_parser("serve", help="Run the app in the current directory")
    p_serve.add_argument("addr", nargs="?", default="127.0.0.1:8000", help="Address to bind (default 127.0.0.1:8000)")

    p_create = sub.add_parser("create", help="Scaffold a new project (requires Rust CLI)")
    p_create.add_argument("name", help="Project name")
    p_create.add_argument("rest", nargs=argparse.REMAINDER, help="Extra args passed to the Rust CLI")

    p_check = sub.add_parser("check", help="Validate the app without starting the server")
    p_check.add_argument("file", nargs="?", help="App file (default: auto-detect)")

    p_openapi = sub.add_parser("openapi", help="Print the OpenAPI spec")
    p_openapi.add_argument("file", nargs="?", help="App file (default: auto-detect)")

    args = parser.parse_args(argv)

    if args.version:
        cmd_version(args)
        return

    if args.command == "serve":
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
