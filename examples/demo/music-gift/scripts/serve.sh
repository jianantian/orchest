#!/bin/bash
# music-gift server management script
#
# Usage:
#   ./scripts/serve.sh start    # build + start in background, log to data/server.log
#   ./scripts/serve.sh stop     # stop the server gracefully
#   ./scripts/serve.sh restart  # stop then start
#   ./scripts/serve.sh status   # check if running
#   ./scripts/serve.sh logs     # tail recent logs
#   ./scripts/serve.sh test     # quick health check (GET /api/playlist)
#
# Run from the project root: cd examples/demo/music-gift && ./scripts/serve.sh start

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

WORKSPACE_ROOT="$(cd "$PROJECT_DIR/../../.." && pwd)"
BINARY="$WORKSPACE_ROOT/target/debug/music-gift"
PID_FILE="data/server.pid"
LOG_FILE="data/server.log"

usage() {
    echo "Usage: $0 {start|stop|restart|status|logs|test}"
    exit 1
}

_status() {
    if [ -f "$PID_FILE" ] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
        return 0
    fi
    return 1
}

start() {
    if _status; then
        echo "[serve] already running (pid=$(cat "$PID_FILE"))"
        return 0
    fi

    mkdir -p data

    # Build if binary is missing or stale
    if [ ! -f "$BINARY" ] || [ "$BINARY" -ot "$WORKSPACE_ROOT/crates/orchest-provider-http/src/gen/suno.rs" ]; then
        echo "[serve] building..."
        cargo build -p music-gift-demo --manifest-path "$WORKSPACE_ROOT/Cargo.toml" 2>&1
    fi

    echo "[serve] starting..."
    nohup "$BINARY" >> "$LOG_FILE" 2>&1 &
    local pid=$!
    echo "$pid" > "$PID_FILE"

    for i in $(seq 1 10); do
        sleep 0.3
        if _status && grep -q "music-gift listening" "$LOG_FILE" 2>/dev/null; then
            local addr=$(grep "music-gift listening" "$LOG_FILE" | tail -1 | awk '{print $NF}')
            echo "[serve] ready pid=$pid ($addr)"
            return 0
        fi
    done
    echo "[serve] started pid=$pid (waiting for listen...)"
}

stop() {
    if ! _status; then
        echo "[serve] not running"
        rm -f "$PID_FILE"
        return 0
    fi

    local pid=$(cat "$PID_FILE")
    echo "[serve] stopping pid=$pid..."
    kill "$pid" 2>/dev/null || true

    for i in $(seq 1 10); do
        if ! kill -0 "$pid" 2>/dev/null; then
            echo "[serve] stopped"
            rm -f "$PID_FILE"
            return 0
        fi
        sleep 0.5
    done

    kill -9 "$pid" 2>/dev/null || true
    echo "[serve] killed"
    rm -f "$PID_FILE"
}

restart() {
    stop
    sleep 0.5
    start
}

status() {
    if _status; then
        local pid=$(cat "$PID_FILE")
        local addr=$(grep "music-gift listening" "$LOG_FILE" 2>/dev/null | tail -1 | awk '{print $NF}' || echo "unknown")
        echo "[serve] running pid=$pid ($addr)"
    else
        echo "[serve] not running"
        rm -f "$PID_FILE"
    fi
}

logs() {
    if [ -f "$LOG_FILE" ]; then
        tail -20 "$LOG_FILE"
    else
        echo "[serve] no log file yet"
    fi
}

test_health() {
    local port="${MUSIC_GIFT_PORT:-3000}"
    local base="http://localhost:${port}"
    echo "[serve] health check $base/api/playlist ..."
    if curl -sf "$base/api/playlist" > /dev/null; then
        echo "[serve] OK"
    else
        echo "[serve] FAIL"
        return 1
    fi
}

case "${1:-}" in
    start)    start ;;
    stop)     stop ;;
    restart)  restart ;;
    status)   status ;;
    logs)     logs ;;
    test)     test_health ;;
    *)        usage ;;
esac
