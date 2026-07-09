from justapi import JustAPIApp, APIRouter

app = JustAPIApp()
router = APIRouter(prefix="/api/v1")

@router.get("/users/")
def get_users():
    return [{"username": "alice"}, {"username": "bob"}]

@router.get("/users/{username}")
def get_user(username: str):
    return {"username": username}

# Include the router in the main application
app.include_router(router)

if __name__ == "__main__":
    app.run("127.0.0.1:8000")
