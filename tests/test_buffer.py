import sys
sys.path.insert(0, "/home/swadhin/RastAPI/crates/justapi-py")
import justapi

print("Testing buffer...")
try:
    buf = justapi._justapi._test_zero_copy(b"Hello from Rust Zero-Copy Buffer!")
    mv = memoryview(buf)
    print("Memoryview created successfully:", mv.tobytes())
except Exception as e:
    print("Failed:", e)
