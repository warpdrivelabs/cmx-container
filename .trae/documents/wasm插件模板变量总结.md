# wasm-plugin-template 模板变量总结

## 结论

wasm-plugin-template 中共有 **9 个模板变量**，分布在 **2 个文件**中。其余 7 个 `.hbs` 文件不含任何模板变量。

---

## 含模板变量的文件

### 1. `Cargo.toml.hbs`（1 个变量）

| 变量 | 行号 | 上下文 |
|------|------|--------|
| `{{project_name}}` | 2 | `name = "{{project_name}}"` |

### 2. `manifest.json.hbs`（10 处引用，8 个唯一变量）

| 变量 | 行号 | 上下文 |
|------|------|--------|
| `{{plugin_id}}` | 5 | `"id": "{{plugin_id}}"` |
| `{{plugin_name}}` | 6 | `"name": "{{plugin_name}}"` |
| `{{description}}` | 8 | `"description": "{{description}}"` |
| `{{plugin_id}}` | 9 | `"url": ".../{{plugin_id}}"` (重复) |
| `{{project_path}}` | 10 | `"source_path": "{{project_path}}"` |
| `{{plugin_id}}` | 12 | `"main_file": "{{plugin_id}}.wasm"` (重复) |
| `{{datasource_id}}` | 13 | `"datasource_id": "{{datasource_id}}"` |
| `{{domain_code}}` | 27 | `"domain_code": "{{domain_code}}"` |
| `{{application_code}}` | 28 | `"application_code": "{{application_code}}"` |
| `{{module_code}}` | 29 | `"module_code": "{{module_code}}"` |

---

## 唯一变量汇总（共 9 个）

| 变量 | 来源字段 | 所在文件 |
|------|----------|----------|
| `{{project_name}}` | `req.id` | Cargo.toml.hbs |
| `{{plugin_id}}` | `req.id` | manifest.json.hbs |
| `{{plugin_name}}` | `req.name` | manifest.json.hbs |
| `{{description}}` | `req.description` | manifest.json.hbs |
| `{{project_path}}` | 目标目录路径 | manifest.json.hbs |
| `{{datasource_id}}` | `req.datasource_id` | manifest.json.hbs |
| `{{domain_code}}` | `req.domain_code` | manifest.json.hbs |
| `{{application_code}}` | `req.application_code` | manifest.json.hbs |
| `{{module_code}}` | `req.module_code` | manifest.json.hbs |

---

## 不含模板变量的 .hbs 文件（7 个）

这些文件虽然使用了 `.hbs` 后缀，但内容中没有任何 `{{变量}}`，渲染效果等同于直接复制：

- `src/lib.rs.hbs`
- `src/core.rs.hbs`
- `src/models.rs.hbs`
- `src/host_traits.rs.hbs`
- `src/extism_layer.rs.hbs`
- `src/tests.rs.hbs`
- `.vscode/launch.json.hbs`

> **建议**：这 7 个文件可以去掉 `.hbs` 后缀改为直接复制，减少不必要的模板处理开销。但保留 `.hbs` 也无害，只是多了一步"去掉后缀+原样写入"的操作。
