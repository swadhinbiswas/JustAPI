import os
import sys
import subprocess

def compile_protobuf(proto_path: str, out_dir: str):
    """
    Compiles a .proto file into Python stubs using grpcio-tools.
    """
    try:
        import grpc_tools.protoc
    except ImportError:
        print("Error: grpcio-tools is required. Install it using 'pip install grpcio-tools'")
        sys.exit(1)

    if not os.path.exists(out_dir):
        os.makedirs(out_dir, exist_ok=True)

    protoc_args = [
        "grpc_tools.protoc",
        f"--proto_path={os.path.dirname(proto_path) or '.'}",
        f"--python_out={out_dir}",
        f"--grpc_python_out={out_dir}",
        f"--pyi_out={out_dir}",
        proto_path,
    ]

    print(f"Compiling {proto_path}...")
    result = grpc_tools.protoc.main(protoc_args)
    if result != 0:
        print(f"Failed to compile {proto_path}")
        sys.exit(result)
    print("Protobuf compilation successful.")

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: python compile_proto.py <proto_path> <out_dir>")
        sys.exit(1)
    compile_protobuf(sys.argv[1], sys.argv[2])
