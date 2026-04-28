`cargo-workspaces` 是一个专门用于管理 Rust 工作空间的命令行工具，它极大地简化了在多 crate 项目中进行版本控制、依赖更新和发布的流程。

下面为你整理了一份常用的命令速查表，覆盖了从初始化到日常维护的各个环节。

### ⚙️ 安装与核心命令速查

首先，通过 `cargo` 安装工具：

```bash
cargo install cargo-workspaces
```

安装后，你可以通过 `cargo workspaces` 或简写 `cargo ws` 来使用它。所有子命令都支持 `--help` 参数来查看更详细的用法。

以下是几个最常用的命令：

| 命令类别 | 命令 (及示例) | 作用说明 |
| :--- | :--- | :--- |
| **📦 项目管理** | `cargo ws init [PATH]` | 在指定目录初始化一个新的工作空间。 |
| | `cargo ws create <PATH>` | 在工作空间内交互式地创建一个新 crate。官方推荐用它代替 `cargo new`。 |
| | `cargo ws list` | 列出工作空间中的所有 crate。 |
| **🔄 版本管理** | `cargo ws version [patch/minor/major]` | **这是最核心的命令之一**。它会自动检测变更的 crate，升级版本号，更新内部依赖，并执行 `git commit` 和 `tag`。 |
| | `cargo ws version --exact -y` | 使用 `=` 精确符来固定内部依赖的版本，并跳过所有确认提示，适合自动化脚本。 |
| **🚀 发布管理** | `cargo ws publish` | **另一个核心命令**。它会先自动执行 `version` 命令，然后按照依赖关系的正确顺序发布所有 crate。 |
| | `cargo ws publish --publish-as-is` | **跳过版本升级步骤**，直接将当前状态下的 crate 发布出去。 |
| | `cargo ws publish --no-verify` | 发布时跳过代码验证（编译和测试），如果你确定代码没问题，这可以加快速度。 |
| | `cargo ws publish --registry my-reg` | 将 crate 发布到指定的私有或自定义 registry。 |
| **🛠️ 辅助工具** | `cargo ws changed` | 列出自上一个 git tag 以来有文件变更的 crate，用于在发布前进行确认。 |
| | `cargo ws exec <COMMAND>` | 在所有 crate 中批量执行同一个命令，例如 `cargo ws exec cargo fmt` 可以格式化所有 crate 的代码。 |

---

### 💡 核心使用场景与技巧

下面结合一些实际场景来说明这些命令如何使用。

#### 1. 日常发布流程

当你完成了一轮开发，准备发布新版本时，可以使用下面的命令一气呵成：

```bash
# 1. (可选) 检查哪些 crate 将要被发布
cargo ws changed

# 2. 执行预演，查看发布时会执行哪些操作
cargo ws publish --dry-run

# 3. 正式发布！自动完成升级版本、打 tag、按顺序发布
# -y 参数跳过所有交互式确认
cargo ws publish patch --exact -y --registry crates-io
```
这个流程会帮你自动处理好一切，避免人为失误。

#### 2. 独立管理特定 Crate 版本

默认情况下，工作空间中的所有 crate 会共享一个统一的版本号。但如果你希望某些 crate 可以独立地升级版本，可以在其 `Cargo.toml` 文件中添加以下配置：

```toml
[package.metadata.workspaces]
independent = true
```

#### 3. 批量执行命令

`cargo ws exec` 是一个非常实用的功能，可以让你免于编写复杂的 shell 脚本。

```bash
# 在所有 crate 中运行测试
cargo ws exec cargo test

# 在所有 crate 中运行 clippy 检查并自动修复
cargo ws exec cargo clippy --fix --allow-dirty
```

#### 4. 安全控制

为了防止意外发布，`version` 和 `publish` 命令默认只允许在 `master` 分支上执行。你可以通过 `--allow-branch` 参数来修改这个设置：

```bash
# 允许在名为 dev-service 的分支上执行发布操作
cargo ws publish --allow-branch dev-service
```

### 💎 总结

总的来说，`cargo-workspaces` 将一系列繁琐的手动操作（分析依赖、修改版本、更新引用、提交、打 tag、按序发布）整合为了几个简单的命令，极大地提升了多 crate 项目的维护效率和可靠性。

```bash
# 仅发布（不升级版本）
cargo ws publish --publish-as-is --registry nora  --allow-branch dev-service  --allow-dirty --no-verify

# 不创建git tag
cargo ws publish  --registry nora  --allow-branch dev-service  --allow-dirty --no-verify --no-git-tag

```
`--publish-as-is` 和普通的 `cargo ws publish` 最核心的区别在于：**是否会自动升级工作空间中 crate 的版本号**。

普通的 `publish` 是一个“**版本升级+发布**”的组合命令，而 `--publish-as-is` 是一个纯粹的“**发布**”命令。

以下是详细的对比分析：

### 核心区别对比表

| 功能点 | 普通 `cargo ws publish` | `cargo ws publish --publish-as-is` |
| :--- | :--- | :--- |
| **自动修改 `Cargo.toml` 版本号** | ✅ **会**。根据 `patch`/`minor`/`major` 参数升级版本。 | ❌ **不会**。完全保留当前的版本号。 |
| **更新内部依赖** | ✅ **会**。自动将工作空间内互相依赖的 crate 版本更新为新版本。 | ❌ **不会**。依赖关系保持原样。 |
| **Git 操作** | ✅ **会**。自动执行 `git commit` 和 `git tag`。 | ❌ **不会**。不执行任何 Git 操作。 |
| **发布到 Registry** | ✅ **会**。将 crate 打包并上传。 | ✅ **会**。将 crate 打包并上传。 |
| **适用场景** | 标准发布流程（开发完成，准备发新版）。 | 修复已发布版本的 bug、调试、或手动已改好版本。 |

---

### 详细场景解析

#### 1. 普通 `cargo ws publish`（全自动流程）

这是最常用的模式。当你执行 `cargo ws publish patch` 时，工具会假设你**准备发布一个包含新代码的新版本**，并替你完成所有“脏活累活”：

1.  **计算版本**：找出所有有改动的 crate，将版本号从 `1.0.0` 升级到 `1.0.1` (patch)。
2.  **修改文件**：直接修改磁盘上所有 `Cargo.toml` 里的 `version` 字段。
3.  **更新依赖**：如果 crate A 依赖了内部 crate B，且 B 升级到了 `1.0.1`，A 的依赖声明也会自动改成 `1.0.1`。
4.  **Git 提交**：`git commit -m "Bump versions"` 并打上 `v1.0.1` 的 tag。
5.  **发布**：执行 `cargo publish`。

**优点**：一键完成，不会忘记改版本或打 tag。**缺点**：不够灵活，必须升级版本才能发布。

#### 2. `cargo ws publish --publish-as-is`（照原样发布）

这个命令假设**版本号已经正确，代码已经准备就绪**，它只是一个“上传器”。

1.  **检查现状**：读取当前 `Cargo.toml` 里的版本号（比如还是 `1.0.0`）。
2.  **不修改任何文件**：跳过版本升级步骤，也**不会**修改内部依赖关系。
3.  **直接发布**：拿着现有的 `1.0.0` 版本直接执行 `cargo publish`。

**优点**：速度快，灵活（可以重复发布同一版本？不，Registry 不允许）。**缺点**：如果版本号没变，`cargo publish` 本身会报错（因为 Registry 不允许覆盖已存在的版本）。

---

### 为什么需要 `--publish-as-is`？什么时候用它？

既然普通模式那么自动化，为什么还需要这个“照原样”模式？主要是为了解决以下特定场景：

#### 场景 1：修复已发布的版本（yank 后重新发布）
假设你发布了 `v1.0.0`，发现有个小问题。你在 Registry 上 `yank` 了它，然后在本地修复了代码。**你不想把版本升到 `v1.0.1`**，只想重新发布一个修复后的 `v1.0.0`（虽然 crates.io 通常不允许，但私有 Registry 可能允许覆盖，或者你正在调试）。

```bash
# 修改代码后，不想升级版本号，直接重新发布相同的版本
cargo ws publish --publish-as-is --registry my-private-reg
```

#### 场景 2：手动控制版本，仅利用工具进行顺序发布
你可能想精细控制每个 crate 的版本号（比如有的升 minor，有的升 patch），但依然想让 `cargo-workspaces` 帮你**解决依赖顺序**问题。

```bash
# 1. 手动修改各个 crate 的 Cargo.toml 版本号
# 2. 手动 git commit 和 tag
# 3. 利用工具按正确顺序发布
cargo ws publish --publish-as-is
```

#### 场景 3：CI/CD 中的原子发布
在 CI 流水线中，你可能已经在之前的步骤中（比如 `cargo ws version`）完成了版本升级和 Git 提交。那么 `publish` 步骤就只需要“发布”，不需要再次修改版本，此时 `--publish-as-is` 是最佳选择。

```yaml
# 伪代码示例
- name: Bump version
  run: cargo ws version patch --no-git-push -y
- name: Push commit and tag
  run: git push && git push --tags
- name: Publish to crates.io
  run: cargo ws publish --publish-as-is
```

### 💎 总结

- **普通 `publish`** = **版本升级 (`version`)** + **发布 (`publish`)**。适合**标准开发流程**，即开发完成 -> 发布新版本。
- **`--publish-as-is`** = **仅发布 (`publish`)**。适合**特殊维护场景**，如**重发**、**手动控制版本**、**CI 分离步骤**。

简单一句话：**如果你想省事，用普通 `publish`；如果你想把“改版本号”这件事掌握在自己手里，用 `--publish-as-is`。**
