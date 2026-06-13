"""Tiny Anthropic-compatible SSE provider for local demos.

The server implements just enough of POST /v1/messages for the examples:
first response asks for a tool call, second response returns final text after
tool results are present.
"""

from __future__ import annotations

import json
import os
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any


def _sse(event: str, data: dict[str, Any]) -> bytes:
    return f"event: {event}\ndata: {json.dumps(data)}\n\n".encode()


def _tool_args(tool_name: str) -> dict[str, Any]:
    if tool_name == "get_weather":
        return {"city": "Tokyo"}
    if tool_name == "get_time":
        return {"timezone": "Asia/Tokyo"}
    if tool_name == "generate_video":
        return {"prompt": "a cat playing piano", "duration_seconds": 5}
    return {}


class MockAnthropicHandler(BaseHTTPRequestHandler):
    def log_message(self, _format: str, *_args: Any) -> None:
        return

    def do_POST(self) -> None:
        if self.path != "/v1/messages":
            self.send_error(404)
            return

        content_len = int(self.headers.get("content-length", "0"))
        body = json.loads(self.rfile.read(content_len) or b"{}")
        messages = body.get("messages", [])
        has_tool_result = any(
            block.get("type") == "tool_result"
            for message in messages
            for block in message.get("content", [])
            if isinstance(block, dict)
        )

        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.end_headers()

        self.wfile.write(
            _sse("message_start", {"message": {"usage": {"input_tokens": 10}}})
        )

        if has_tool_result:
            text = "Demo completed using tool results."
            self.wfile.write(
                _sse("content_block_start", {"content_block": {"type": "text"}})
            )
            self.wfile.write(
                _sse(
                    "content_block_delta",
                    {"delta": {"type": "text_delta", "text": text}},
                )
            )
            self.wfile.write(_sse("content_block_stop", {}))
            self.wfile.write(
                _sse(
                    "message_delta",
                    {
                        "delta": {"stop_reason": "end_turn"},
                        "usage": {"output_tokens": 12},
                    },
                )
            )
            self.wfile.write(_sse("message_stop", {}))
            return

        tools = body.get("tools", [])
        tool_names = [tool["name"] for tool in tools if "name" in tool]
        selected = [name for name in ("get_weather", "get_time", "generate_video") if name in tool_names]
        if not selected and tool_names:
            selected = [tool_names[0]]

        for idx, tool_name in enumerate(selected):
            self.wfile.write(
                _sse(
                    "content_block_start",
                    {
                        "content_block": {
                            "type": "tool_use",
                            "id": f"call_{idx + 1}",
                            "name": tool_name,
                        }
                    },
                )
            )
            self.wfile.write(
                _sse(
                    "content_block_delta",
                    {
                        "delta": {
                            "type": "input_json_delta",
                            "partial_json": json.dumps(_tool_args(tool_name)),
                        }
                    },
                )
            )
            self.wfile.write(_sse("content_block_stop", {}))

        self.wfile.write(
            _sse(
                "message_delta",
                {"delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 8}},
            )
        )
        self.wfile.write(_sse("message_stop", {}))


def serve(port: int) -> ThreadingHTTPServer:
    server = ThreadingHTTPServer(("127.0.0.1", port), MockAnthropicHandler)
    return server


if __name__ == "__main__":
    port = int(sys.argv[1] if len(sys.argv) > 1 else os.environ.get("PORT", "8787"))
    server = serve(port)
    print(f"mock Anthropic provider listening on http://127.0.0.1:{port}/v1/messages")
    server.serve_forever()
