import asyncio
import httpx
from justapi import JustAPIApp, UploadFile, File, Form

app = JustAPIApp()

@app.post("/upload")
def upload_file(file: UploadFile = File(), username: str = Form()):
    content = file.read()
    file.close()
    return {"filename": file.filename, "content": content.decode("utf-8"), "username": username}

async def main():
    # Start server in background
    import threading
    import time
    
    def run_server():
        app.run("127.0.0.1:8080")
        
    t = threading.Thread(target=run_server, daemon=True)
    t.start()
    time.sleep(1) # wait for server to start
    
    async with httpx.AsyncClient() as client:
        files = {'file': ('test.txt', b'Hello, world!', 'text/plain')}
        data = {'username': 'testuser'}
        response = await client.post('http://localhost:8080/upload', files=files, data=data)
        
        print(response.status_code)
        print(response.json())
        assert response.status_code == 200
        assert response.json() == {"filename": "test.txt", "content": "Hello, world!", "username": "testuser"}
        print("Multipart upload test passed!")

if __name__ == "__main__":
    asyncio.run(main())
