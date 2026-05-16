# 表名重命名计划：file\_detail → cmx\_file\_detail，file\_part\_detail → cmx\_file\_part\_detail

## 变更范围

涉及 4 个文件，共 86 处匹配：

| # | 文件                                                 | 匹配数  | 说明                                                                |
|---|----------------------------------------------------|------|-------------------------------------------------------------------|
| 1 | `crates/libs/cmx-infra/cmx-storage/src/bmc.rs`     | \~14 | 表名常量 + 注释                                                         |
| 2 | `crates/libs/cmx-infra/cmx-storage/src/types.rs`   | 2    | 文档注释                                                              |
| 3 | `crates/libs/cmx-infra/cmx-storage/src/service.rs` | \~14 | 函数名 `dataset_to_file_detail` / `find_file_detail`（函数名不动，只改表名相关注释） |
| 4 | `example/sqlexample/oss_pg.sql`                    | \~56 | SQL DDL（CREATE TABLE、INDEX、COMMENT）                               |
| 5 | `example/sqlexample/oss.sql`                       | 2    | MySQL SQL DDL                                                     |

## 不需要修改的项

以下内容中的 `file_detail` 是 **Rust 函数名/变量名**，不是表名，**保持不变**：

* `service.rs` 中的 `dataset_to_file_detail()` 函数名

* `service.rs` 中的 `find_file_detail()` 函数名

* `types.rs` 中的 `FileDetail` / `FilePartDetail` 类型名（Rust 结构体名，非数据库表名）

## 实施步骤

### 步骤 1：修改 bmc.rs（核心表名常量）

将两个 `DbBmc` 实现中的 `TABLE` 常量和相关注释中的表名替换：

* `"file_detail"` → `"cmx_file_detail"`

* `"file_part_detail"` → `"cmx_file_part_detail"`

* 注释中 `file_detail` → `cmx_file_detail`

* 注释中 `file_part_detail` → `cmx_file_part_detail`

### 步骤 2：修改 types.rs（文档注释）

* 第 357 行：`对应 file_detail 表` → `对应 cmx_file_detail 表`

* 第 466 行：`对应 file_part_detail 表` → `对应 cmx_file_part_detail 表`

### 步骤 3：修改 oss\_pg.sql（PostgreSQL DDL）

全局替换：

* `file_detail` → `cmx_file_detail`（涵盖 CREATE TABLE、CONSTRAINT、INDEX、COMMENT ON）

* `file_part_detail` → `cmx_file_part_detail`

### 步骤 4：修改 oss.sql（MySQL DDL）

全局替换：

* `file_detail` → `cmx_file_detail`

* `file_part_detail` → `cmx_file_part_detail`

### 步骤 5：编译验证

执行 `cargo check -p cmx-storage` 确认编译通过。

## 验证方法

* `grep -rn "file_detail\|file_part_detail" crates/libs/cmx-infra/cmx-storage/src/` 应只在函数名/类型名中出现，不再有表名字符串

* `grep -rn "cmx_file_detail\|cmx_file_part_detail"` 应覆盖所有需要的位置

