"""Populate the legacy Rust evidence schema in the SDK installation tests."""

import json
import sqlite3
import sys
import uuid
from contextlib import closing


def seed(database: sqlite3.Connection) -> None:
    row = database.execute(
        "SELECT instance, expires, payload FROM evidence ORDER BY cursor LIMIT 1",
    ).fetchone()
    instance, expires, payload = row
    record = json.loads(payload)
    attempt_id = str(uuid.uuid4())
    records = [
        {"kind": "field", "attempt_id": attempt_id, "field": "messages",
         "capture": {"coverage": "exact", "value": {"secret": "fixture private message"}}},
        {"kind": "session_event", "attempt_id": attempt_id,
         "event": {"seq": 12345, "timestamp_ms": 123, "kind": "assistant_message"}},
        {"kind": "attempt_started", "attempt_id": attempt_id, "ordinal": 99,
         "purpose": "agent_step", "provider": None, "model": None},
    ]
    for kind in records:
        record["kind"] = kind
        database.execute(
            "INSERT INTO evidence (instance, id, expires, payload) VALUES (?, ?, ?, ?)",
            (instance, str(uuid.uuid4()), expires, json.dumps(record).encode()),
        )
    database.execute(
        "INSERT INTO evidence (instance, id, expires, payload) VALUES (?, ?, ?, ?)",
        (instance, str(uuid.uuid4()), expires, b"broken record"),
    )


if __name__ == "__main__":
    operation, path, *args = sys.argv[1:]
    with closing(sqlite3.connect(path)) as database, database:
        if operation == "seed":
            seed(database)
        elif operation == "delete":
            database.execute("DELETE FROM session_events WHERE session_id = ?", args)
            database.execute("DELETE FROM session_meta WHERE session_id = ?", args)
        elif operation == "break":
            database.execute("DROP TABLE evidence")
        else:
            raise ValueError(operation)
