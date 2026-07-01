#!/bin/bash
#
# One-click deploy: build both migrated frontends + cmx-container web-server, start, health-check.
#
# Usage:
#   bash/deploy.sh                # build frontends + backend(release), then start
#   bash/deploy.sh --no-build     # skip build, just (re)start existing artifacts
#   bash/deploy.sh --frontend-only / --backend-only
#
# Prereqs:
#   - Node+npm (frontends), Rust toolchain (backend)
#   - PostgreSQL / Redis ready (per dev-local.toml)
#   - .env has CONFIG_FILE=./dev-local.toml and DEEPSEEK_API_KEY (AI/agent)
#   - dev-local.toml [portal] web_portal_dist / web_html_dist / web_shared_dist point at the dist dirs
#     (web_shared_dist -> packages/cmx-ui5-runtime/dist; serves /shared/assets/*, the UI5 runtime that
#      both frontends dynamic-import. Missing it => /shared 404 => white screen.)
#
set -euo pipefail

#=== paths ===
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CMX_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"          # cmx-container
ROOT_DIR="$(cd "$CMX_DIR/.." && pwd)"            # presentation (parent of both frontends)
PORTAL_DIR="$ROOT_DIR/CMXPortalManager"
DESIGNER_DIR="$ROOT_DIR/CMXHTMLDesigner"
SHARED_DIST="$ROOT_DIR/packages/cmx-ui5-runtime/dist"   # UI5 runtime, served at /shared

BIN="$CMX_DIR/target/release/web-server"
LOG_FILE="$CMX_DIR/logs/deploy-server.log"
PID_FILE="$CMX_DIR/.deploy-server.pid"
PORT="${CMX_PORT:-8080}"

DO_FRONTEND=1
DO_BACKEND=1
for arg in "$@"; do
  case "$arg" in
    --no-build) DO_FRONTEND=0; DO_BACKEND=0 ;;
    --frontend-only) DO_BACKEND=0 ;;
    --backend-only) DO_FRONTEND=0 ;;
    *) echo "unknown arg: $arg"; exit 2 ;;
  esac
done

log() { echo "[deploy] $*"; }
err() { echo "[deploy] $*" >&2; }

#=== 1) build frontends (base /portal/ and /html/, output to each dist) ===
# Note: each frontend's `npm run build` first builds cmx-ui5-runtime (npm run build -w
# cmx-ui5-runtime), which produces packages/cmx-ui5-runtime/dist served at /shared. We verify
# that shared dist explicitly because a missing /shared/assets/install-*.js => white screen.
build_frontend() {
  log "building CMXPortalManager frontend (base=/portal/, incl. cmx-ui5-runtime) ..."
  ( cd "$PORTAL_DIR" && npm run build )
  log "building CMXHTMLDesigner frontend (base=/html/, incl. cmx-ui5-runtime) ..."
  ( cd "$DESIGNER_DIR" && npm run build )
  [ -f "$PORTAL_DIR/dist/index.html" ] || { err "Portal dist build failed"; exit 1; }
  [ -f "$DESIGNER_DIR/dist/index.html" ] || { err "Designer dist build failed"; exit 1; }
  # /shared runtime: at least one install-*.js chunk must exist, else /portal & /html white-screen.
  if ! ls "$SHARED_DIST"/assets/install-*.js >/dev/null 2>&1; then
    err "shared UI5 runtime dist missing ($SHARED_DIST/assets/install-*.js)."
    err "  -> /shared would 404 and both frontends would white-screen. Build cmx-ui5-runtime:"
    err "     ( cd $ROOT_DIR && npm run build -w cmx-ui5-runtime )"
    exit 1
  fi
  log "frontend build done: $PORTAL_DIR/dist + $DESIGNER_DIR/dist + $SHARED_DIST (/shared)"
}

#=== 2) build backend (release) ===
build_backend() {
  log "building web-server (cargo build --release) ..."
  ( cd "$CMX_DIR" && cargo build --release -p web-server )
  [ -x "$BIN" ] || { err "web-server binary missing: $BIN"; exit 1; }
  log "backend build done: $BIN"
}

#=== 3) stop old instance ===
stop_old() {
  if [ -f "$PID_FILE" ] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
    log "stopping old instance PID $(cat "$PID_FILE") ..."
    kill "$(cat "$PID_FILE")" 2>/dev/null || true
    sleep 2
  fi
  rm -f "$PID_FILE"
}

#=== 4) start + health check ===
start_and_check() {
  mkdir -p "$CMX_DIR/logs"
  stop_old
  log "starting web-server (cwd=$CMX_DIR, reads .env / dev-local.toml) ..."
  # cwd must be CMX_DIR: data_root=./data, dist relative paths, and .env are all relative to it.
  (
    cd "$CMX_DIR"
    set -a
    [ -f .env ] && . ./.env
    set +a
    nohup "$BIN" > "$LOG_FILE" 2>&1 &
    echo $! > "$PID_FILE"
  )
  local pid
  pid="$(cat "$PID_FILE")"
  log "started PID $pid, waiting for health ..."

  # Health probe: any HTTP status (incl. 401/422) means the server accepted the
  # connection and is up. Connection refused yields code 000.
  local ok=0 code
  for _ in $(seq 1 30); do
    code="$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$PORT/api/auth/login" -X POST -H 'Content-Type: application/json' -d '{}' 2>/dev/null || echo 000)"
    if [ "$code" != "000" ]; then ok=1; break; fi
    kill -0 "$pid" 2>/dev/null || { err "web-server exited early; see log: $LOG_FILE"; tail -20 "$LOG_FILE"; exit 1; }
    sleep 1
  done
  [ "$ok" = 1 ] || { err "health check timed out (30s). log tail:"; tail -20 "$LOG_FILE"; exit 1; }

  log "OK. service ready:"
  echo "    backend API : http://127.0.0.1:$PORT/api"
  echo "    portal      : http://127.0.0.1:$PORT/portal/"
  echo "    designer    : http://127.0.0.1:$PORT/html/"
  echo "    shared rt   : http://127.0.0.1:$PORT/shared/  (UI5 runtime; backs /portal & /html)"
  echo "    swagger     : http://127.0.0.1:$PORT/swagger-ui"
  echo "    log         : $LOG_FILE   (stop: kill \$(cat $PID_FILE))"
}

#=== main ===
log "deploy start (CMX_DIR=$CMX_DIR)"
if [ "$DO_FRONTEND" = 1 ]; then build_frontend; else log "skip frontend build"; fi
if [ "$DO_BACKEND" = 1 ]; then build_backend; else log "skip backend build"; fi
start_and_check
