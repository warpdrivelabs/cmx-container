# cmx-container 项目开发规范

> AI 在本项目中开发时遵循的规范。

---

## 一、Error 处理规则

### 1.1 必须使用 thiserror 库

所有自定义 Error 使用 `#[derive(thiserror::Error)]`：

```rust
#[derive(Error, Debug)]
pub enum MyError {
    #[error("操作失败: {0}")]
    OperationFailed(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
```

### 1.2 禁止手写 Error 实现

- 禁止 `impl std::error::Error` 或 `impl Display`
- 禁止 `derive_more::From`（与 thiserror 冲突）
- 使用 `#[error(...)]` 属性定义错误消息

### 1.3 Error 定义位置

- 每个 crate 有独立 error 模块 (`src/error.rs`)
- 使用 `pub type Result<T> = core::result::Result<T, Error>;`

### 1.4 禁止裸用 unwrap()

在编写 Rust 代码时，禁止在生产环境中直接使用 `unwrap()` 方法。

**正确做法**：

1. **首选方案**：使用 `?` 操作符将错误优雅地向上抛出（`Result`）。
2. **兜底方案**：如果当前逻辑确实无法处理该错误且必须中断，请使用 `expect("...")`，并在括号内提供具有业务指导意义的错误提示（例如：`expect("Redis 连接池初始化失败，请检查配置")`）。

**例外情况**：

仅在 `#[test]` 单元测试函数内部，或者 100% 确信该值不可能为 `None` 的极特殊场景下，才允许使用 `unwrap()`。

---

## 二、日志处理规则

### 2.1 必须使用 tracing 库

```rust
// ✅ 正确
use tracing::{info, warn, error};
info!("操作成功");

// ❌ 错误
use log::info;
```

### 2.2 日志格式

使用结构化日志：`info!("message", key = value)`

---

## 三、依赖管理规则

### 3.1 Workspace 依赖集中管理

所有第三方依赖在 workspace `Cargo.toml` 统一定义，子 crate 禁止直接指定版本，`cmx-wasmdemo`除外。

### 3.2 Crate 引用方式

使用 `workspace = true` 引用，允许添加额外 features：

```toml
# ✅ 正确
uuid = { workspace = true, features = ["v4"] }

# ❌ 错误
uuid = "1.21"
```

### 3.3 依赖注释要求

每个依赖上方必须添加单独注释，禁止分组注释：

```toml
# ✅ 正确
# 序列化框架
serde = { workspace = true }

# ❌ 错误 - 禁止分组注释
# ============================================
serde = { workspace = true }
```

注释格式：`# <用途>` 或 `# <分类> - <用途>`，内部依赖：`# 内部依赖 - <模块>`

### 3.4 禁止使用 log crate

```toml
# ❌ 错误
log = "0.4"
```

### 3.5 新增依赖流程

1. 检查 workspace 是否已定义
2. 未定义则先在 workspace 添加并注释
3. 用 `workspace = true` 引用

### 3.6 未使用依赖

注释保留而非删除：

```toml
# [dependencies]
# unused_crate = "1.0"
```

---

## 四、Git 提交规则

### 4.1 禁止提交根目录 .env 文件

Git 提交代码时，必须忽略根目录的 `.env` 文件，**即使该文件已经被 `git add` 加入暂存区，也不得提交**。

**正确做法**：
1. 提交时不通过 `git add .` 或 `git add -A` 批量添加，逐个指定文件

**错误示例**：

```bash
# ❌ 错误 - 可能误提交 .env
git add .
git commit -m "update config"

# ❌ 错误 - 强制提交 .env
git add -f .env
```

### 4.2 禁止自动提交代码

AI 助手在完成任务后，**禁止主动执行 `git commit` 等提交操作**，必须由用户主动确认并提出提交请求后才能提交。

**正确做法**：

1. AI 完成代码修改后，仅向用户汇报改动内容，等待用户明确指令（如「提交代码」「commit」「提交一下」等）
2. 收到用户明确指令后，再按规范执行 `git status` → `git diff` → 暂存指定文件 → `git commit` 流程
3. 提交信息需遵循 Conventional Commits 规范（feat / fix / refactor / docs / chore 等）

**错误示例**：

```bash
# ❌ 错误 - 未经用户允许直接提交
git add .
git commit -m "update"

# ❌ 错误 - 完成任务后自动 push
git push origin main
```

**例外情况**：

仅在用户明确表示「帮我提交」「请提交这次改动」等明确指令时，AI 才可以执行提交操作。

---

## 五、SQL 与配置维护规则

### 5.1 新建表结构

新建 PostgreSQL 表时，**推荐**使用 `pg-table-generator` 技能生成 DDL，确保表结构符合项目规范（标准审计字段、命名规则、注释规则等）。

### 5.2 SQL 文件维护

涉及 SQL 变更（新建表、新增/修改/删除字段、新增索引等）时，**必须**使用 `sql-guide` 技能同步维护：

- `docs/sql/migrations/` 目录下创建增量迁移文件（`YYYYMMDD_XXX.up.sql` + `.down.sql`）
- `docs/sql/init/init_ddl.sql` 同步更新为最新完整状态

### 5.3 配置文档维护

新增或修改 TOML 配置项、环境变量时，**必须**使用 `config-sync` 技能同步维护：

- TOML 配置 → `config/config_template.toml` + `config/CONFIG_MANUAL.md`
- 环境变量 → `config/.env.template` + `config/ENV_MANUAL.md`
