# 贡献指南

感谢参与！本文说明如何贡献代码与规范。

## 开发环境
- 后端（cmx-container）：Rust（见 `rust-toolchain.toml`）、PostgreSQL
- 前端（presentation）：Node + npm（monorepo，根目录 `npm install`）

## 提交前必过
```bash
# 后端
cargo test          # 单测
cargo clippy        # 无 warning
cargo fmt --check   # 格式

# 前端
npm test            # vitest
npm run lint
```

## 分支与提交
- 从 `main` 切特性分支：`feat/<简述>` / `fix/<简述>`。
- 提交信息用 **Conventional Commits**：`feat(scope): ...` / `fix(scope): ...` / `docs: ...` / `refactor: ...`。
- 一个 PR 聚焦一件事；改动附测试；破坏性变更在 PR 描述里显式标注。

## 代码规范
- 写与周边一致的代码（命名、注释密度、惯用法）。
- 新功能带测试；改 bug 带能复现的回归测试。
- **不提交**：真实凭据、内网地址、大二进制、`node_modules/`、`dist/`、`target/`、本地配置（`dev*.toml` / `.env`）。

## 配置与密钥
- 只提交 `*.example` / `config_template.toml` 模板；真实值走环境变量或本地未跟踪配置。
- 严禁把 API Key / DB 口令 / OAuth secret 写进任何被跟踪文件。

## 测试数据
- 只用脱敏 demo 数据；不得提交真实业务数据 / 客户信息。

## 许可
提交即表示你同意以本仓库 `LICENSE`（Apache-2.0）的条款贡献你的代码。

## 报告问题
- 一般 bug / 需求：GitHub Issue。
- 安全漏洞：**勿开公开 Issue**，见 [SECURITY.md](SECURITY.md)。
