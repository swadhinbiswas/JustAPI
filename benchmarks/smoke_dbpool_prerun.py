import time, threading, urllib.request
from justapi import JustAPIApp, Database

# TODO: set JUSTAPI_PG_URL or pass via env var — never commit live credentials
URL = None  # overridden by environment

app = JustAPIApp()
app.set_database(
    URL,
    init_sql="CREATE TABLE IF NOT EXISTS dbpool_demo (id SERIAL PRIMARY KEY, name TEXT, qty INT)",
)

# FOOTGUN FIX: app.db must work BEFORE run() (no server started yet).
assert app.db is not None, "app.db was None before run()"
pre = app.db.query("SELECT 1 AS ok")
assert pre == [{"ok": 1}], pre
print("PRE-RUN app.db OK:", pre)

app.db.execute("DELETE FROM dbpool_demo")
app.db.execute("INSERT INTO dbpool_demo (name, qty) VALUES (?, ?)", ["alpha", 10])
seed = app.db.query("SELECT * FROM dbpool_demo ORDER BY id")
print("PRE-RUN SEED:", seed)

@app.get("/pre")
def pre(req):
    return {"rows": app.db.query("SELECT count(*) AS c FROM dbpool_demo")}

srv = threading.Thread(target=lambda: app.run("127.0.0.1:8124"), daemon=True)
srv.start()
time.sleep(2.0)

with urllib.request.urlopen("http://127.0.0.1:8124/pre", timeout=10) as r:
    print("RUNTIME:", r.read().decode())

print("FOOTGUN FIX VERIFIED")
