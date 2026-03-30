#!/bin/bash

# ================== 配置区（请按需修改）==================
APP_NAME="myapp"
APP_BIN="/opt/myapp/myapp"          # 你的 Rust 二进制路径
APP_DIR="/opt/myapp"                # 工作目录
LOG_FILE="/var/log/myapp.log"       # 日志文件
PID_FILE="/var/run/myapp.pid"       # PID 文件路径
# =======================================================

# 创建日志和 PID 目录（如不存在）
mkdir -p "$(dirname "$LOG_FILE")"
mkdir -p "$(dirname "$PID_FILE")"

# 设置权限（可选）
chown -R myapp:myapp "$(dirname "$LOG_FILE")" 2>/dev/null || true
chown -R myapp:myapp "$(dirname "$PID_FILE")" 2>/dev/null || true

start() {
    if [ -f "$PID_FILE" ]; then
        PID=$(cat "$PID_FILE")
        if kill -0 "$PID" 2>/dev/null; then
            echo "[$APP_NAME] 已在运行 (PID: $PID)"
            exit 1
        else
            echo "[$APP_NAME] PID 文件存在但进程已退出，清理中..."
            rm -f "$PID_FILE"
        fi
    fi

    echo "[$APP_NAME] 正在启动..."
    cd "$APP_DIR" || exit 1

    # 启动程序并记录 PID
    nohup "$APP_BIN" > "$LOG_FILE" 2>&1 &
    PID=$!
    echo $PID > "$PID_FILE"

    # 等待 1 秒确保启动
    sleep 1

    if kill -0 "$PID" 2>/dev/null; then
        echo "[$APP_NAME] 启动成功 (PID: $PID)"
    else
        echo "[$APP_NAME] 启动失败，请查看日志: $LOG_FILE"
        rm -f "$PID_FILE"
        exit 1
    fi
}

stop() {
    if [ ! -f "$PID_FILE" ]; then
        echo "[$APP_NAME] 未运行（PID 文件不存在）"
        exit 1
    fi

    PID=$(cat "$PID_FILE")
    if kill -0 "$PID" 2>/dev/null; then
        echo "[$APP_NAME] 正在停止 (PID: $PID)..."
        kill "$PID"
        # 等待最多 10 秒优雅退出
        for i in {1..10}; do
            if ! kill -0 "$PID" 2>/dev/null; then
                break
            fi
            sleep 1
        done
        if kill -0 "$PID" 2>/dev/null; then
            echo "[$APP_NAME] 强制终止 (超时)"
            kill -9 "$PID"
        fi
        rm -f "$PID_FILE"
        echo "[$APP_NAME] 已停止"
    else
        echo "[$APP_NAME] 进程已退出，清理 PID 文件"
        rm -f "$PID_FILE"
    fi
}

status() {
    if [ -f "$PID_FILE" ]; then
        PID=$(cat "$PID_FILE")
        if kill -0 "$PID" 2>/dev/null; then
            echo "[$APP_NAME] 运行中 (PID: $PID)"
            exit 0
        else
            echo "[$APP_NAME] PID 文件存在但进程已退出"
            exit 1
        fi
    else
        echo "[$APP_NAME] 未运行"
        exit 1
    fi
}

case "$1" in
    start)
        start
        ;;
    stop)
        stop
        ;;
    restart)
        stop
        sleep 2
        start
        ;;
    status)
        status
        ;;
    *)
        echo "用法: $0 {start|stop|restart|status}"
        exit 1
        ;;
esac
