import pytest
import os
import time
import json
import threading
import requests
from justapi import JustAPIApp
import asyncio

def test_hot_reload():
    config_path = "test_gateway.json"
    
    initial_config = {
        "routes": {
            "/api/proxy": {
                "GET": {
                    "upstream": "http://v1.upstream.com"
                }
            }
        }
    }
    with open(config_path, "w") as f:
        json.dump(initial_config, f)
        
    app = JustAPIApp()
    app.enable_gateway(config_path)

    @app.get("/api/local")
    def local_route():
        return {"msg": "local"}

    def run_server():
        app.run("127.0.0.1:9199")

    t = threading.Thread(target=run_server, daemon=True)
    t.start()
    time.sleep(1) # wait for server to start

    # 1. Test Local
    resp = requests.get("http://127.0.0.1:9199/api/local")
    assert resp.status_code == 200
    assert resp.json() == {"msg": "local"}
    
    # 2. Test initial Gateway Proxy
    resp = requests.get("http://127.0.0.1:9199/api/proxy")
    assert resp.status_code == 200
    assert resp.headers["x-justapi-gateway"] == "hot-reloaded"
    assert resp.content == b"Proxied to: http://v1.upstream.com"
    
    # 3. Modify Gateway config (HOT RELOAD)
    updated_config = {
        "routes": {
            "/api/proxy": {
                "GET": {
                    "upstream": "http://v2.upstream.com" # Updated!
                }
            },
            "/api/new-proxy": {
                "GET": {
                    "upstream": "http://v3.new.upstream.com"
                }
            }
        }
    }
    with open(config_path, "w") as f:
        json.dump(updated_config, f)
        
    time.sleep(1) # wait for file watcher to pick up and debounce
    
    # 4. Test updated Gateway Proxy
    resp = requests.get("http://127.0.0.1:9199/api/proxy")
    assert resp.status_code == 200
    assert resp.headers["x-justapi-gateway"] == "hot-reloaded"
    assert resp.content == b"Proxied to: http://v2.upstream.com"
    
    resp = requests.get("http://127.0.0.1:9199/api/new-proxy")
    assert resp.status_code == 200
    assert resp.content == b"Proxied to: http://v3.new.upstream.com"
    
    print("Test Passed: Hot Reloading Works Successfully!")
    
if __name__ == "__main__":
    test_hot_reload()
