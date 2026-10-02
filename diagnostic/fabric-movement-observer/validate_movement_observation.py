#!/usr/bin/env python3
"""Read-only schema/privacy check for a Movement Observer JSONL capture."""
import base64
import json
import sys
from pathlib import Path

FORBIDDEN = {"chat", "sign_text", "raw_nbt", "raw_bytes", "url", "token", "auth"}
TRAVEL_KEYS = {
    "used_friction_f32", "friction_source_pos", "used_ground_drag_f32", "on_ground_at_land_start",
    "used_block_speed_factor_f32", "used_block_jump_factor_f32", "jump_power_f32", "used_step_height",
    "pose_at_move", "bbox_before", "bbox_after", "support_before", "support_after",
    "requested_delta", "clipped_delta", "original_requested_y_negative", "final_y_clipped",
    "ground_decision", "entity_shapes", "entity_shapes_max", "entity_shapes_truncated",
    "entity_shapes_omitted", "context", "frame_nanos", "frame_elapsed_sec", "frame_id",
    "frame_player_tick_count", "native_partial_ticks_f32", "native_physics", "actual_gravity_f64",
    "creative_vertical_drag_f64",
}

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
        data = row["data"]
        if row["stage"] == "after_tick" and data.get("event") == "movement_tick":
            travel = data.get("travel_observation")
            assert isinstance(travel, dict), "after_tick must contain travel_observation"
            assert TRAVEL_KEYS <= travel.keys(), f"travel_observation missing keys: {TRAVEL_KEYS - travel.keys()}"
            reasons = travel.get("unavailable_reasons")
            assert isinstance(reasons, dict), "travel_observation must distinguish unavailable/null values with unavailable_reasons"
            for key in TRAVEL_KEYS:
                if travel[key] is None:
                    reason = reasons.get(key)
                    assert isinstance(reason, str) and reason.strip(), f"null travel field lacks a reason: {key}"
                    if key == "entity_shapes" or key.startswith("entity_shapes_"):
                        assert "requery" in reason.lower() or "resolver" in reason.lower(), f"shape null needs query-vs-resolver semantics: {key}"
                    if key == "actual_gravity_f64":
                        assert "gravity" in reason.lower() and "branch" in reason.lower(), "gravity null needs a branch reason"
                    if key == "native_physics":
                        assert "physics" in reason.lower() and ("branch" in reason.lower() or "call site" in reason.lower()), "native physics null needs a call-site reason"
                    if key in {"frame_nanos", "frame_elapsed_sec"}:
                        assert "frame_observation" in reason, f"{key} null must identify the separate frame observation"
        if row["stage"] == "packet_raw":
            assert data.get("protocol_state") in {"play", "configuration"}
            assert data.get("payload_layout") == "id_plus_payload" and data.get("capture_point") == "wire_plaintext"
            payload = base64.b64decode(data["payload_base64"], validate=True)
            assert len(payload) == data.get("raw_length") and len(payload) > 0
            assert isinstance(data.get("connection_epoch"), int) and isinstance(data.get("packet_trace_id"), str)
        if "packet" in data:
            assert isinstance(data["packet"], str) and isinstance(data.get("fields"), dict), "packet fields must be explicit typed JSON"
            assert "native_id" in data, "numeric native ID must be explicit (null when unavailable)"
            assert "fields_capture" not in data, "class-name-only packet records are incomplete"
            if data["packet"] == "section_blocks_update":
                fields = data["fields"]
                assert len(fields.get("updates", [])) <= 4096
                assert fields.get("truncated") == (fields.get("count", 0) > 4096)
        if row["stage"] == "transport_write_failure":
            assert isinstance(data.get("error_class"), str) and data["error_class"]
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
    assert f.get("complete") == (f["dropped"] == 0 and f["oversize_omitted"] == 0 and f.get("reason") != "size_limit")
    if "raw_omitted_bytes" in f:
        assert isinstance(f["raw_omitted_bytes"], int) and f["raw_omitted_bytes"] >= 0
    assert f.get("last_seq", 0) >= prev_seq
    return len(events), f

def self_test():
    null_travel = {key: None for key in TRAVEL_KEYS}
    null_travel["unavailable_reasons"] = {
        key: ("bounded shape requery is not actual resolver inputs" if key.startswith("entity_shapes_") or key == "entity_shapes"
              else "separate frame_observation" if key.startswith("frame_") or key == "native_partial_ticks_f32"
              else "gravity call site not invoked in this branch" if key == "actual_gravity_f64"
              else "no hooked physics branch in this tick" if key == "native_physics"
              else "actual hook not observed in this branch")
        for key in TRAVEL_KEYS
    }
    rows = [
        {"type":"header", "schema":1, "client_kind":"fabric", "wire_protocol":776},
        {"seq":1,"offset_us":5,"direction":"local","stage":"before_tick","data":{"event":"movement_tick"}},
        {"seq":2,"offset_us":5,"direction":"local","stage":"after_tick","data":{"event":"movement_tick","travel_observation":null_travel}},
        {"seq":3,"offset_us":5,"direction":"outbound","stage":"transport_write_attempt","data":{"packet":"move_player_rot","native_id":None,"fields":{"position":None,"yaw_pitch":[90.0,10.0],"on_ground":True,"horizontal_collision":False}}},
        {"seq":4,"offset_us":5,"direction":"inbound","stage":"apply_before","data":{"packet":"block_ack","native_id":None,"fields":{"sequence":17},"applied_state":{"prediction_state":None}}},
        {"seq":5,"offset_us":5,"direction":"outbound","stage":"transport_write_failure","data":{"packet":"use_item","native_id":None,"fields":{"hand":0,"sequence":2,"yaw_pitch":[0.0,0.0]},"error_class":"java.io.IOException"}},
        {"seq":6,"offset_us":5,"direction":"inbound","stage":"packet_raw","data":{"protocol_state":"play","wire_protocol":776,"connection_epoch":1,"packet_trace_id":"fixture-1","native_id":4,"packet_type":"minecraft:cookie_request","raw_length":8,"payload_base64":base64.b64encode(b"cookie!!").decode(),"capture_point":"wire_plaintext","payload_layout":"id_plus_payload"}},
        {"seq":7,"offset_us":5,"direction":"local","stage":"input_event","data":{"event":"input_event","kind":"key","action":1,"code":87,"code_name":"W","scancode":17,"modifiers":0,"frame_id":2}},
        {"type":"footer","written":7,"dropped":1,"oversize_omitted":0,"last_seq":7,"complete":False},
    ]
    assert validate("\n".join(map(json.dumps, rows)))[0] == 7
    bad_reasons = json.loads(json.dumps(rows)); del bad_reasons[2]["data"]["travel_observation"]["unavailable_reasons"]["actual_gravity_f64"]
    try: validate("\n".join(map(json.dumps, bad_reasons)))
    except AssertionError: pass
    else: raise AssertionError("validator accepted a null field without an unavailable reason")
    bad = json.loads(json.dumps(rows)); bad[1]["data"]["raw_bytes"] = "no"
    try: validate("\n".join(map(json.dumps, bad)))
    except AssertionError: pass
    else: raise AssertionError("privacy gate accepted raw_bytes")
    incomplete = json.loads(json.dumps(rows)); incomplete[3]["data"] = {"packet":"Pos","fields":None,"fields_capture":"class only"}
    try: validate("\n".join(map(json.dumps, incomplete)))
    except AssertionError: pass
    else: raise AssertionError("validator accepted class-name-only payload")
    print("self-test: PASS (ordered rows, raw cookie bytes, key input, drop gaps, typed packets, failure, privacy rejection)")

if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        self_test()
    elif len(sys.argv) == 2:
        count, footer = validate(Path(sys.argv[1]).read_text(encoding="utf-8"))
        print(f"valid schema-1 JSONL: {count} event rows; complete={footer['complete']} dropped={footer['dropped']} oversize={footer['oversize_omitted']}")
    else:
        raise SystemExit(f"usage: {Path(sys.argv[0]).name} FILE.jsonl | --self-test")
