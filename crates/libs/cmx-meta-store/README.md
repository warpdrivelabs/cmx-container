# cmx-meta-store/

> 遗留占位分组：目录下当前只有一个**空的** `tests/` 目录——无 Cargo.toml、无任何 crate，不是 workspace 成员，不参与编译。

## 分组定位

`cmx-meta-store/` 在目录树上形似一个分组目录，但**不含任何子 crate**：
没有 Cargo.toml，根 `Cargo.toml` 的 `members` 中也无本目录条目。
目前仅保留一个空的 `tests/` 目录作为历史占位。

## 目录内容（实测 `ls` / `find` 确认）

```text
cmx-meta-store/
└── tests/     # 空目录：无任何文件（含隐藏文件）
```

- `tests/`：空目录。常被误记在此的模板 / 备份资产
  （`wasm-plugin-template.zip`、`backup/`）实测位于 `../cmx-dev/templates/`，
  本目录并无这些内容。

## 特殊状态

- **非 workspace 成员**：不参与编译，无任何构建产物。
- 若后续要在此恢复测试资产或新建 crate，需同步登记根 `Cargo.toml`
  的 `members` 列表；若确认废弃，也可整体删除。

## 相关背景

- Wasm 插件模板（模板目录 / zip / 备份）真源：`../cmx-dev/templates/`。
- 各域持久化 crate（store-pg）分组：`../cmx-dct/`、`../cmx-doc/`、
  `../cmx-mdm/`、`../cmx-job/`。
- 定义中心元数据建模（设计期）见 `../cmx-model/cmx-model-meta/`。
