from __future__ import annotations

import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

import orchest
import pytest


QUESTIONS = {
    "urgent": {"type": "boolean", "instructions": {"question": "Urgent?"}},
    "team": {
        "type": "choice",
        "instructions": "Which team?",
        "criteria": {"billing": None, "tech": {"about": "bugs"}},
    },
    "severity": {
        "type": "score",
        "instructions": ["How severe?"],
        "criteria": ["low", {"label": "medium"}, "high"],
    },
}


class DecisionHandler(BaseHTTPRequestHandler):
    request: dict[str, Any] | None = None
    request_ready = threading.Event()
    allow_response = threading.Event()
    status = 200
    headers: dict[str, str] = {}
    response: dict[str, Any] = {}

    def do_POST(self) -> None:
        length = int(self.headers["Content-Length"])
        type(self).request = {
            "path": self.path,
            "authorization": self.headers.get("Authorization"),
            "body": json.loads(self.rfile.read(length)),
        }
        type(self).request_ready.set()
        if not type(self).allow_response.wait(2):
            self.send_error(500, "test coordination timed out")
            return
        self.send_response(type(self).status)
        for name, value in type(self).headers.items():
            self.send_header(name, value)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(type(self).response).encode())

    def log_message(self, format: str, *args: object) -> None:
        pass


@pytest.fixture
def decision_server() -> tuple[str, type[DecisionHandler]]:
    DecisionHandler.request = None
    DecisionHandler.request_ready = threading.Event()
    DecisionHandler.allow_response = threading.Event()
    DecisionHandler.status = 200
    DecisionHandler.headers = {}
    DecisionHandler.response = {}
    server = ThreadingHTTPServer(("127.0.0.1", 0), DecisionHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    yield f"http://127.0.0.1:{server.server_port}/api/alpha/decisions", DecisionHandler
    server.shutdown()
    thread.join(timeout=2)


def test_decide_real_extension_maps_wire_and_releases_gil(decision_server: tuple[str, type[DecisionHandler]]) -> None:
    url, handler = decision_server
    handler.response = {
        "id": "decision-1",
        "model": "~typesafe/jev-latest",
        "provider": "openrouter",
        "answers": {
            "urgent": {"type": "noul", "noul": 0.95},
            "team": {"type": "choice", "choice": "billing"},
            "severity": {"type": "score", "score": 1.05},
        },
        "usage": {"input_tokens": 12, "output_tokens": 5, "cost": 0.0003},
    }
    outcome: list[object] = []

    def invoke() -> None:
        try:
            outcome.append(
                orchest.decide(
                    model="openrouter/~typesafe/jev-latest",
                    state={"message": "help", "nested": [1, {"ok": True}]},
                    questions=QUESTIONS,
                    api_key="secret",
                    api_url=url,
                    timeout_ms=2_000,
                )
            )
        except BaseException as error:
            outcome.append(error)

    caller = threading.Thread(target=invoke)
    caller.start()
    assert handler.request_ready.wait(1), "server thread could not run while decide waited (GIL held)"
    handler.allow_response.set()
    caller.join(timeout=2)
    assert not caller.is_alive()
    assert not isinstance(outcome[0], BaseException)

    request = handler.request
    assert request is not None
    assert request["path"] == "/api/alpha/decisions"
    assert request["authorization"] == "Bearer secret"
    assert request["body"]["questions"]["urgent"]["type"] == "noul"
    assert request["body"]["state"]["nested"][1] == {"ok": True}
    response = outcome[0]
    assert isinstance(response, dict)
    assert response["answers"]["urgent"] == {"type": "boolean", "probability": 0.95}
    assert response["answers"]["severity"]["score"] == 1.05
    assert "confidence" not in response["answers"]["team"]
    assert response["usage"]["cost_usd"] == 0.0003


def test_decide_exposes_structured_http_error(decision_server: tuple[str, type[DecisionHandler]]) -> None:
    url, handler = decision_server
    handler.status = 429
    handler.headers = {"Retry-After": "7"}
    handler.response = {"error": {"message": "slow down"}}
    handler.allow_response.set()
    with pytest.raises(orchest.ModelError) as caught:
        orchest.decide(
            model="openrouter/~typesafe/jev-latest",
            state="case",
            questions={"q": {"type": "boolean", "instructions": "Urgent?"}},
            api_key="secret",
            api_url=url,
        )
    assert caught.value.status == 429
    assert caught.value.retry_after_secs == 7
    assert caught.value.provider == "openrouter"


def test_decide_rejects_invalid_request_before_network() -> None:
    with pytest.raises(orchest.ModelError) as caught:
        orchest.decide(
            model="openrouter/~typesafe/jev-latest",
            state={"message": "help"},
            questions={},
            api_key="secret",
        )
    assert caught.value.code == "invalid_request"

    with pytest.raises(orchest.ModelError) as malformed:
        orchest.decide(
            model="openrouter/~typesafe/jev-latest",
            state={"message": "help"},
            questions={"q": {"type": "unknown", "instructions": "What?"}},
            api_key="secret",
        )
    assert malformed.value.code == "invalid_request"
