"""Smoke test: Step C Rust-native CRUD through the real Python server."""
import sys, json, time, urllib.request, subprocess, socket

from justapi import JustAPIApp, Database

app = JustAPIApp()
app.set_database(
    Database('sqlite://:memory:', max_connections=1),
    init_sql="CREATE TABLE items (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, qty INTEGER NOT NULL)",
)
app.post("/items", crud_table="items", crud_columns=["name", "qty"])
app.get("/items/{id}", crud_table="items", crud_columns=["name", "qty"])
app.put("/items/{id}", crud_table="items", crud_columns=["name", "qty"])
app.delete("/items/{id}", crud_table="items", crud_columns=["name", "qty"])


def free_port():
    s = socket.socket()
    s.bind(("", 0))
    p = s.getsockname()[1]
    s.close()
    return p


port = free_port()

# Start the server in a detached process.
proc = subprocess.Popen(
    [sys.executable, "-c",
     "import sys; sys.path.insert(0,'benchmarks');"
     "from justapi import JustAPIApp, Database;"
     "app=JustAPIApp();"
     "app.set_database(Database('sqlite://:memory:', max_connections=1), init_sql='CREATE TABLE items (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, qty INTEGER NOT NULL)');"
     "app.post('/items', crud_table='items', crud_columns=['name','qty']);"
     "app.get('/items/{id}', crud_table='items', crud_columns=['name','qty']);"
     "app.put('/items/{id}', crud_table='items', crud_columns=['name','qty']);"
     "app.delete('/items/{id}', crud_table='items', crud_columns=['name','qty']);"
     "app.run('127.0.0.1:%d')" % port],
    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
)
time.sleep(2.0)

base = "http://127.0.0.1:%d" % port


def req(method, path, body=None):
    data = json.dumps(body).encode() if body is not None else None
    r = urllib.request.Request(base + path, data=data, method=method,
                               headers={"content-type": "application/json"})
    try:
        with urllib.request.urlopen(r) as resp:
            return resp.status, resp.read()
    except urllib.error.HTTPError as e:
        return e.code, e.read()


try:
    # INSERT
    st, b = req("POST", "/items", {"name": "widget", "qty": 3})
    assert st == 200, (st, b)
    rid = json.loads(b)["id"]
    print("INSERT ok id=", rid)

    # SELECT
    st, b = req("GET", "/items/%d" % rid)
    assert st == 200, (st, b)
    assert json.loads(b)[0]["name"] == "widget"
    print("SELECT ok")

    # UPDATE
    st, b = req("PUT", "/items/%d" % rid, {"name": "gadget", "qty": 7})
    assert st == 200, (st, b)
    assert json.loads(b)["qty"] == 7
    print("UPDATE ok")

    # DELETE
    st, b = req("DELETE", "/items/%d" % rid)
    assert st == 200, (st, b)
    print("DELETE ok")

    # SELECT after delete -> empty array
    st, b = req("GET", "/items/%d" % rid)
    assert st == 200 and json.loads(b) == [], (st, b)
    print("SELECT-after-delete ok (empty)")

    print("SMOKE PASS")
finally:
    proc.terminate()
