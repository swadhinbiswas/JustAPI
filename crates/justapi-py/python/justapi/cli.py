import sys
import subprocess
import shutil

def main():
    """JustAPI CLI entrypoint for python package."""
    binary = shutil.which("justapi-cli")
    if binary:
        sys.exit(subprocess.call([binary] + sys.argv[1:]))

    if len(sys.argv) > 1 and sys.argv[1] in ("--version", "-V"):
        print("justapi 2.0.0")
        return

    print("JustAPI Runtime v2.0.0 (Python package)")
    print("Commands:")
    print("  justapi --version    Display version")
    print("  python -m app.main   Run application directly")
    print("\nFor high-performance Rust CLI features (justapi create, serve --reload, profile),")
    print("install justapi-cli via cargo: cargo install justapi-cli")

if __name__ == "__main__":
    main()

