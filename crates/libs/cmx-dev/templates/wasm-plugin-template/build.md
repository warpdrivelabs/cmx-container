## 构建说明

### 前置条件

```bash

# 添加 WASM 编译目标
rustup target add wasm32-unknown-unknown
rustup target add wasm32-wasip1
```

### Features 说明

| Feature | 说明 | 启用的依赖 |
|---------|------|-----------|
| *(default)* | 纯逻辑模式，不依赖 Extism | cmx-plugin-sdk (default-features = false) |
| `extism` | Extism 插件模式，启用 `#[plugin_fn]` 导出 | extism-pdk, cmx-plugin-sdk/extism |

### 构建命令

#### wasm32-unknown-unknown 目标

```bash
# Debug 构建（包含调试信息）
cargo build --target wasm32-unknown-unknown

# Release 构建（优化体积和性能）
cargo build --release --target wasm32-unknown-unknown
```

#### wasm32-wasip1 目标（需 extism feature）

```bash
# Debug 构建
cargo build --target wasm32-wasip1 --features extism

# Release 构建
cargo build --release --target wasm32-wasip1 --features extism
```

### 构建输出位置

```
target/wasm32-unknown-unknown/debug/cmx_wasmdemo.wasm       # Debug 版本
target/wasm32-unknown-unknown/release/cmx_wasmdemo.wasm     # Release 版本
target/wasm32-wasip1/debug/cmx_wasmdemo.wasm                # WASI Debug 版本
target/wasm32-wasip1/release/cmx_wasmdemo.wasm              # WASI Release 版本
```

### 生成 API 文档

```bash
# 生成文档（结合注释和结构体定义）
cargo run -p cmx-cli -- doc scan ./ -o ./api/api.json --pretty

# 控制结构体展开深度（默认 5）
cargo run -p cmx-cli -- doc scan ./ --expand-depth 5 -o ./api/api.json --pretty
```