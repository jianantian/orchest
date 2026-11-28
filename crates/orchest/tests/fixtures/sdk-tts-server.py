"""Local WSS fixture shared by the installed Python and Node SDK tests."""

import base64
import hashlib
import json
from pathlib import Path
import socket
import ssl
import struct
import subprocess
import tempfile
import threading


def read_frame(stream):
    header = stream.read(2)
    if not header:
        return None
    first, second = header
    size = second & 127
    if size == 126:
        size = struct.unpack("!H", stream.read(2))[0]
    elif size == 127:
        size = struct.unpack("!Q", stream.read(8))[0]
    mask = stream.read(4)
    data = stream.read(size)
    if first & 15 == 8:
        return None
    return json.loads(bytes(byte ^ mask[index % 4] for index, byte in enumerate(data)))


def send_event(stream, event, **fields):
    body = json.dumps({"event": event, "base_resp": {"status_code": 0}, **fields}).encode()
    length = bytes([len(body)]) if len(body) < 126 else b"\x7e" + struct.pack("!H", len(body))
    stream.write(b"\x81" + length + body)
    stream.flush()


def serve(stream):
    request_line = stream.readline().decode()
    if not request_line:
        return
    duplex = request_line.startswith("GET /duplex ")
    headers = {}
    while (line := stream.readline().decode()) not in ("", "\r\n"):
        name, value = line.split(":", 1)
        headers[name.lower()] = value.strip()
    assert headers["authorization"] == "Bearer fixture-key"
    accept = base64.b64encode(hashlib.sha1(
        (headers["sec-websocket-key"] + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()
    ).digest())
    stream.write(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n"
                 b"Connection: Upgrade\r\nSec-WebSocket-Accept: " + accept + b"\r\n\r\n")
    stream.flush()
    send_event(stream, "connected_success")
    while message := read_frame(stream):
        action = message["event"]
        if action == "task_start":
            send_event(stream, "task_started")
        elif action == "task_continue":
            if message["text"] == "benchmark":
                for _ in range(128):
                    send_event(stream, "task_continued", data={"audio": "5a" * 4096}, is_final=False)
                continue
            if message["text"] == "error":
                send_event(stream, "task_failed", base_resp={"status_code": 1004, "status_msg": "fixture rejected key"})
                return
            if duplex:
                send_event(stream, "sentence_start")
            send_event(stream, "task_continued", data={"audio": "0061ff"}, is_final=duplex,
                       extra_info={"usage_characters": 6, "audio_length": 125})
            if duplex:
                send_event(stream, "sentence_end")
        elif action == "task_flush":
            send_event(stream, "task_flushed")
        elif action == "task_cancel":
            send_event(stream, "task_canceled")
            send_event(stream, "task_continued", data={"audio": "99"}, is_final=False)
        elif action == "task_finish":
            send_event(stream, "task_finished", extra_info={"usage_characters": 6, "audio_length": 125})
            return
        else:
            raise ValueError(f"unknown fixture action {action}")


def handle(client, tls):
    try:
        with client, tls.wrap_socket(client, server_side=True) as secure:
            with secure.makefile("rwb") as stream:
                serve(stream)
    except (BrokenPipeError, ConnectionResetError, ssl.SSLError):
        pass
    finally:
        print("closed", flush=True)


def main():
    with tempfile.TemporaryDirectory() as directory:
        directory = Path(directory)
        root_key, root = directory / "root-key.pem", directory / "root.pem"
        key, cert = directory / "private.pem", directory / "server.pem"
        csr, config = directory / "server.csr", directory / "tls.cnf"
        config.write_text(
            "[req]\nprompt=no\ndistinguished_name=subject\nx509_extensions=root\n"
            "[subject]\nCN=Orchest SDK fixture CA\n"
            "[root]\nbasicConstraints=critical,CA:TRUE\nkeyUsage=critical,keyCertSign,cRLSign\n"
            "[server]\nsubjectAltName=DNS:localhost,IP:127.0.0.1\n"
            "basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\n"
            "extendedKeyUsage=serverAuth\n"
        )
        commands = [
            ["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-sha256", "-days", "1",
             "-keyout", str(root_key), "-out", str(root), "-config", str(config)],
            ["req", "-new", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=localhost",
             "-keyout", str(key), "-out", str(csr)],
            ["x509", "-req", "-in", str(csr), "-CA", str(root), "-CAkey", str(root_key),
             "-CAcreateserial", "-out", str(cert), "-days", "1", "-sha256",
             "-extfile", str(config), "-extensions", "server"],
        ]
        for command in commands:
            subprocess.run(["openssl", *command], check=True, capture_output=True)
        tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        tls.load_cert_chain(cert, key)
        with socket.create_server(("127.0.0.1", 0)) as listener:
            port = listener.getsockname()[1]
            print(json.dumps({"endpoint": f"wss://127.0.0.1:{port}", "ca": root.read_text()}), flush=True)
            while True:
                client, _ = listener.accept()
                client.settimeout(10)
                threading.Thread(target=handle, args=(client, tls), daemon=True).start()


if __name__ == "__main__":
    main()
