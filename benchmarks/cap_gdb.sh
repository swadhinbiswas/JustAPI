#!/bin/bash
cd /home/swadhin/RastAPI/benchmarks
PY=/home/swadhin/RastAPI/.venv/bin/python
PORT=8268
OUT=/home/swadhin/RastAPI/benchmarks/gdb_out.txt
rm -f $OUT
gdb -batch \
  -ex "set pagination off" \
  -ex "set debuginfod enabled off" \
  -ex "set non-stop off" \
  -ex "run" \
  -ex "thread apply all bt 6" \
  --args $PY workloads_bt4.py $PORT > $OUT 2>&1 &
GDBPID=$!
for i in $(seq 1 60); do
  if curl -s -o /dev/null http://127.0.0.1:$PORT/body_json -X POST -d '{"x":1}' 2>/dev/null; then break; fi
  sleep 1
done
~/.cargo/bin/oha -c 100 -z 20s http://127.0.0.1:$PORT/body_json -m POST -d '{"name":"alice","email":"a@b.com","age":30}' >/dev/null 2>&1 &
OHAPID=$!
sleep 8
kill -INT $GDBPID
sleep 2
kill $OHAPID 2>/dev/null
wait $GDBPID 2>/dev/null
echo "captured to $OUT"
