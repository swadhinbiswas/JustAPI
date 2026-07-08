import requests
import time
import threading
from justapi import JustAPIApp

def test_graphql():
    app = JustAPIApp()
    
    def run_server():
        app.run("127.0.0.1:9200")
        
    t = threading.Thread(target=run_server, daemon=True)
    t.start()
    time.sleep(1) # Wait for startup
    
    # Test 1: GraphiQL UI
    resp = requests.get("http://127.0.0.1:9200/graphql")
    assert resp.status_code == 200
    assert b"GraphiQL" in resp.content
    
    # Test 2: GraphQL Query
    query = """
    query {
        systemStatus
        version
    }
    """
    resp = requests.post("http://127.0.0.1:9200/graphql", json={"query": query})
    assert resp.status_code == 200
    
    data = resp.json()
    assert "data" in data
    assert data["data"]["systemStatus"] == "JustAPI GraphQL Federation Gateway is running"
    
    print("Test Passed: GraphQL & Federation Gateway Works!")

if __name__ == "__main__":
    test_graphql()
