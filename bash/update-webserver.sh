#!/bin/bash

set -e  # 遇到任何命令失败立即退出

# ⚠️ 已迁移（P3）：门户后端 bin（web-server）已迁到独立 workspace presentation/cmx-portalservice，
#    改名为 cmx-portal-server。cmx-container 现为零可执行服务。生产更新请改用 cmx-portalservice 的
#    bash/appctl.sh（通用进程管理，APP_NAME=cmx-portal-server），构建走
#    `cd presentation/cmx-portalservice && cargo build --release -p cmx-portal-server`。
#    下方 APP_NAME/CMX_DIR 已按新 bin 更新，但请复核 WEB_DIR/CMX_DIR 部署路径是否随之调整。

# ==================== 配置区 ====================
APP_NAME="cmx-portal-server"              # ←←← 已随 P3 从 web-server 改名
WEB_DIR="/data/apps/webserver"
CMX_DIR="/data/project/cmx/cmx-portalservice"   # ←←← 已随 P3 从 cmx-container 改为 cmx-portalservice
# ==============================================

start_time=$(date +%s)

echo "🚀 开始更新应用: $APP_NAME"

# --- 1. 进入 Web 目录并备份 ---
cd "$WEB_DIR" || { echo "❌ 无法进入目录: $WEB_DIR"; exit 1; }

BACKUP_DIR="$WEB_DIR/backup"
mkdir -p "$BACKUP_DIR"

if [ -f "$APP_NAME" ]; then
    TIMESTAMP=$(date +"%Y%m%d%H%M%S")
    BACKUP_FILE="$BACKUP_DIR/${APP_NAME}.${TIMESTAMP}"
    cp "$APP_NAME" "$BACKUP_FILE"
    echo "✅ 已备份 $APP_NAME 到 $BACKUP_FILE"
else
    echo "⚠️  警告: $WEB_DIR/$APP_NAME 不存在，跳过备份"
fi

# --- 2. 拉取并编译新版本 ---
cd "$CMX_DIR" || { echo "❌ 无法进入目录: $CMX_DIR"; exit 1; }

echo "🔄 执行 git pull..."
git pull

echo "⚙️  编译 Rust 应用 (cargo build --release)..."
cargo build --release

TARGET_BIN="$CMX_DIR/target/release/$APP_NAME"
if [ ! -f "$TARGET_BIN" ]; then
    echo "❌ 错误: 编译产物未生成: $TARGET_BIN"
    exit 1
fi

# --- 3. 停止服务（关键！避免“文本文件忙”）---
cd "$WEB_DIR" || { echo "❌ 无法返回目录: $WEB_DIR"; exit 1; }

if [ ! -f "appctl.sh" ]; then
    echo "❌ 错误: 控制脚本 appctl.sh 不存在"
    exit 1
fi

echo "⏹️  停止当前服务..."
# 使用 set +e 临时关闭严格模式，避免 stop 失败导致脚本退出
(
    set +e
    sudo ./appctl.sh stop
    exit_code=$?
    if [ $exit_code -eq 0 ]; then
        echo "✅ 服务已成功停止"
    else
        echo "ℹ️  停止命令返回非零状态码 ($exit_code)，可能服务未运行，继续执行..."
    fi
)

# 可选：确保进程已退出（等待最多5秒）
sleep 1
if pgrep -f "$APP_NAME" > /dev/null; then
    echo "⏳ 检测到残留进程，等待其退出..."
    timeout 5 bash -c "while pgrep -f '$APP_NAME' > /dev/null; do sleep 0.5; done" || {
        echo "⚠️  进程未在5秒内退出，尝试强制终止..."
        pkill -f "$APP_NAME" || true
        sleep 1
    }
fi

# --- 4. 拷贝新二进制 ---
echo "📤 拷贝新版本 $APP_NAME 到 $WEB_DIR ..."
cp "$TARGET_BIN" "$WEB_DIR/$APP_NAME"

# --- 5. 启动服务 ---
echo "▶️  启动服务..."
sudo ./appctl.sh start

# --- 6. 完成统计 ---
end_time=$(date +%s)
elapsed=$((end_time - start_time))
echo "✅ 应用 $APP_NAME 更新完成！总耗时: ${elapsed} 秒"
