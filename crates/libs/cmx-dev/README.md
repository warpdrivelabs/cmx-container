# cmx-dev/

> 开发期模板资产分组：不含任何 crate / Cargo.toml，仅存放 Wasm 插件工程模板等脚手架资产（`templates/`），非 workspace 成员。

## 分组定位

`cmx-dev/` 与其他分组不同：它**不是 crate 分组**——没有 Cargo.toml，
未列入 workspace `members`，不参与编译。它只承载**开发期模板资产**，
供 cmx-cli 等工具在初始化新插件工程时套用（Handlebars 渲染 + 打包分发）。

## 目录内容（实测 `ls` / `find` 确认）

```text
cmx-dev/
└── templates/
    ├── wasm-plugin-template/       # Wasm 插件工程模板（完整脚手架目录）
    ├── wasm-plugin-template.zip    # 上述模板的 zip 打包形态
    └── backup/
        └── wasm-plugin-template1/  # 旧版模板留档备份
```

- `templates/wasm-plugin-template/`：Handlebars 模板化脚手架，核心文件含
  `Cargo.toml.hbs`、`manifest.json.hbs`、`package.sh`、`build.md`、`readme.md`，
  以及 `config/`、`formdata/`、`mcpdata/`、`menudata/`、`metadata/`、
  `permdata/`、`seeddata/`、`servicedata/`、`src/` 等资源目录，覆盖插件
  配置 / 表单 / 菜单 / 权限 / 种子数据 / 服务数据全套资产位。
- `templates/wasm-plugin-template.zip`：模板的打包产物，供下载 / 导入场景使用。
- `templates/backup/wasm-plugin-template1/`：模板历史版本备份留档。

## 特殊状态

- **非 workspace 成员**：根 `Cargo.toml` 的 `members` 中无本目录任何条目。
- 模板内的 `Cargo.toml.hbs` 是渲染模板而非可直接编译的 manifest，
  不应被当作独立 crate 引用。

## 相关背景

- 插件域 HTTP 皮肤：`../cmx-apis/cmx-plugin-api/`（插件运行时 / 市场 /
  迁移包导入导出端点）。
- 另一个非编译资产目录见 `../cmx-meta-store/`（仅含空 `tests/` 占位）。
