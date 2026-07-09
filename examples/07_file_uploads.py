import os
from justapi import JustAPIApp, UploadFile, File

app = JustAPIApp()

@app.post("/upload/")
async def upload_file(file: UploadFile = File(...)):
    """
    Demonstrates receiving a streamed multipart file.
    The file is saved to a temporary location to avoid OOM issues.
    """
    content_type = file.content_type
    filename = file.filename
    temp_path = file.file
    
    file_size = os.path.getsize(temp_path)
    
    return {
        "filename": filename,
        "content_type": content_type,
        "size_bytes": file_size,
        "message": f"Successfully uploaded {filename}"
    }

if __name__ == "__main__":
    app.run("127.0.0.1:8000")
