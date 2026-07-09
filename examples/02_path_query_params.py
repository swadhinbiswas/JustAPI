from justapi import JustAPIApp
from typing import Optional

app = JustAPIApp()

@app.get("/users/{user_id}")
def get_user(user_id: int, q: Optional[str] = None, skip: int = 0, limit: int = 10):
    """
    Fetch a user by ID, with optional query parameters.
    Try accessing: /users/42?q=search&limit=5
    """
    return {
        "user_id": user_id,
        "q": q,
        "skip": skip,
        "limit": limit
    }

if __name__ == "__main__":
    app.run("127.0.0.1:8000")
