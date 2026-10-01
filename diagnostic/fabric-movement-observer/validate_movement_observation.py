#!/usr/bin/env python3
"""Read-only schema/privacy check for a Movement Observer JSONL capture."""
import json
import sys
from pathlib import Path

FORBIDDEN = {"chat", "sign_text", "raw_nbt", "raw_bytes", "url", "token", "auth"}

def validate(text: str) -> tuple[int, dict]:
    rows = [json.loads(line) for line in text.splitlines() if line.strip()]
    assert len(rows) >= 2 and rows[0].get("type") == "header" and rows[-1].get("type") == "footer"
    h, f = rows[0], rows[-1]
    assert h.get("schema") == 1 and h.get("client_kind") == "fabric" and h.get("wire_protocol") == 776
    events = rows[1:-1]
    prev_seq = prev_offset = 0
    for row in events:
        assert isinstance(row.get("seq"), int) and row["seq"] > prev_seq
        assert isinstance(row.get("offset_us"), int) and row["offset_us"] >= prev_offset
        assert row.get("direction") in {"local", "outbound", "inbound"}
        assert isinstance(row.get("stage"), str) and isinstance(row.get("data"), dict)
        prev_seq, prev_offset = row["seq"], row["offset_us"]
        def keys(value):
            if isinstance(value, dict):
                for key, child in value.items():
                    assert key.lower() not in FORBIDDEN, f"forbidden key: {key}"
                    yield from keys(child)
            elif isinstance(value, list):
                for child in value:
                    yield from keys(child)
        list(keys(row["data"]))
    assert f.get("written") == len(events)
    assert isinstance(f.get("dropped"), int) and isinstance(f.get("oversize_omitted"), int)
    assert f.get("complete") == (f["dropped"] == 0 and f["oversize_omitted"] == 0)
    assert f.get("last_seq", 0) >= prev_seq
    return len(events), f

def self_test():
    rows = [
        {"type":"header", "schema":1, "client_kind":"fabric", "wire_protocol":776},
        {"seq":1,"offset_us":5,"direction":"local","stage":"before_tick","data":{"event":"movement_tick"}},
        {"seq":3,"offset_us":5,"direction":"outbound","stage":"transport_write_attempt","data":{"packet":"Pos","fields":None}},
        {"type":"footer","written":2,"dropped":1,"oversize_omitted":0,"last_seq":3,"complete":False},
    ]
    assert validate("\n".join(map(json.dumps, rows)))[0] == 2
    bad = json.loads(json.dumps(rows)); bad[1]["data"]["raw_bytes"] = "no"
    try: validate("\n".join(map(json.dumps, bad)))
    except AssertionError: pass
    else: raise AssertionError("privacy gate accepted raw_bytes")
    print("self-test: PASS (ordered rows, gap/drop footer, forbidden-field rejection)")

if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        self_test()
    elif len(sys.argv) == 2:
        count, footer = validate(Path(sys.argv[1]).read_text(encoding="utf-8"))
        print(f"valid schema-1 JSONL: {count} event rows; complete={footer['complete']} dropped={footer['dropped']} oversize={footer['oversize_omitted']}")
    else:
        raise SystemExit(f"usage: {Path(sys.argv[0]).name} FILE.jsonl | --self-test")
