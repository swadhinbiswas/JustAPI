import pytest
import time
from justapi import JustAPIApp, JustAPITestClient, BackgroundTasks

def test_background_tasks():
    app = JustAPIApp()
    
    result = []
    def bg_task(msg):
        time.sleep(0.1)
        result.append(msg)
        
    @app.get("/task")
    def add_task(background_tasks: BackgroundTasks):
        background_tasks.add_task(bg_task, "hello from bg")
        return {"status": "ok"}
        
    client = JustAPITestClient(app)
    response = client.get("/task")
    assert response["status"] == 200
    assert len(result) == 0 # Hasn't finished yet or ran in background
    
    # wait for background task
    time.sleep(0.3)
    assert len(result) == 1
    assert result[0] == "hello from bg"

