import asyncio
import time
from justapi import JustAPIApp, Depends
import urllib.request
import threading

def common_auth():
    print("Executing common_auth (should only run once if cached)")
    return {"user_id": 123}

async def fetch_db(auth=Depends(common_auth, use_cache=True)):
    print("Executing fetch_db")
    return {"db_conn": True, "user_id": auth["user_id"]}

# App level dependency
app = JustAPIApp(dependencies=[Depends(common_auth)])

# Route level dependency and Handler level dependency
@app.get("/di", dependencies=[Depends(fetch_db, use_cache=True)])
def di_route(request, auth=Depends(common_auth, use_cache=True), db=Depends(fetch_db, use_cache=True)):
    return {"message": "ok", "auth": auth, "db": db}

def start_server():
    app.run("127.0.0.1:8112")

if __name__ == "__main__":
    t = threading.Thread(target=start_server, daemon=True)
    t.start()
    time.sleep(1) # wait for server
    
    req = urllib.request.Request("http://127.0.0.1:8112/di")
    try:
        res = urllib.request.urlopen(req)
        print("Response:", res.read().decode())
    except Exception as e:
        print("Error:", e)
