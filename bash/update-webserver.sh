#!/bin/bash

set -e  # 遇到任何错误立即退出

# ==================== 配置区 ====================
APP_NAME="web-server"                     # ←←← 可在此修改应用名称
WEB_DIR="/data/apps/webserver"
CMX_DIR="/data/project/cmx/cmx-container"
# ==============================================

# 记录开始时间（秒级）
start_time=$(date +%s)

# 进入 Web 服务目录
cd "$WEB_DIR" || { echo "❌ 无法进入目录: $WEB_DIR"; exit 1; }

# 创建 backup 子目录
BACKUP_DIR="$WEB_DIR/backup"
mkdir -p "$BACKUP_DIR"

# 备份当前二进制文件（如果存在）
if [ -f "$APP_NAME" ]; then
    TIMESTAMP=$(date +"%Y%m%d%H%M%S")
    BACKUP_FILE="$BACKUP_DIR/${APP_NAME}.${TIMESTAMP}"
    cp "$APP_NAME" "$BACKUP_FILE"
    echo "✅ 已备份 $APP_NAME 到 $BACKUP_FILE"
else
    echo "⚠️  警告: $WEB_DIR/$APP_NAME 不存在，跳过备份"
fi

# 拉取并编译新版本
cd "$CMX_DIR" || { echo "❌ 无法进入目录: $CMX_DIR"; exit 1; }

echo "🔄 执行 git pull..."
git pull

echo "⚙️  编译 Rust 应用 (cargo build --release)..."
cargo build --release

# 检查编译产物
TARGET_BIN="$CMX_DIR/target/release/$APP_NAME"
if [ ! -f "$TARGET_BIN" ]; then
    echo "❌ 错误: 编译产物未生成: $TARGET_BIN"
    exit 1
fi

echo "📤 拷贝新版本 $APP_NAME 到 $WEB_DIR ..."
cp "$TARGET_BIN" "$WEB_DIR/$APP_NAME"

# 重启服务
cd "$WEB_DIR" || { echo "❌ 无法返回目录: $WEB_DIR"; exit 1; }

if [ -f "appctl.sh" ]; then
    echo "🔁 执行 ./appctl.sh restart ..."
    ./appctl.sh restart
else
    echo "❌ 错误: $WEB_DIR/appctl.sh 不存在"
    exit 1
fi

# 计算总耗时
end_time=$(date +%s)
elapsed=$((end_time - start_time))
echo "✅ 全部操作完成！总耗时: ${elapsed} 秒"
