# 插件系统

本模块提供**插件管理**：从 JSON 或 ZIP 装配清单加载、注册插件定义，验签与表配置/表定义解析；**不执行、不实例化** WASM，执行由上层或其它模块负责。插件通过 JSON 定义 WASM 入口路径与建表配置文件（表定义已在配置文件的 `files` 中）；ZIP 分发时使用**装配清单**描述包内布局。

---

## 1. 插件定义 JSON（单文件）

插件可由**独立 JSON 文件**描述，与下文 ZIP 内 `manifest.json` 中的 `plugin` 字段同构。

### 字段说明

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | string | 是 | 插件唯一标识 |
| `name` | string | 是 | 显示名称 |
| `version` | string | 否 | 语义化版本，如 `"1.0.0"` |
| `wasm_file` | string | 是 | **WASM 入口文件路径**（相对本 JSON 所在目录或约定的插件根目录） |
| `table_config_files` | string[] | 否 | **建表配置 JSON 文件列表**（即 `TableDefinesConfig` 格式），配置内已定义具体表定义文件（`files`），路径相对插件根 |
| `supported_databases` | string[] | 否 | **本插件声明支持的数据库类型**（小写规范名），如 `mysql`、`postgres`、`sqlite`、`oracle` 等，供宿主匹配运行时数据库 |
| `domain_code` | string | 否 | **所属域编码**（对应 `cmx_domain.code`，如 FIN 财务域），便于宿主与域-应用-模块表关联 |
| `application_code` | string | 否 | **所属应用编码**（对应 `cmx_application.code`，如 GL_ACCT 会计核算） |
| `module_code` | string | 否 | **所属模块编码**（对应 `cmx_module.code`，如 GL 总账、AR_AP 应收应付） |
| `vendor_name` | string | 否 | **开发商/供应商名称** |
| `vendor_url` | string | 否 | **开发商网址** |
| `vendor_contact` | string | 否 | **开发商联系方式**（如邮箱、电话） |
| `development_languages` | string[] | 否 | **开发语言列表**（小写规范名），如 `rust`、`javascript`、`c++`、`go` 等（Rust 中见 `def::supported_lang`） |
| `description` | string | 否 | 说明 |

- **wasm_file**：唯一必须的“代码”入口，指向一个 `.wasm` 文件。
- **table_config_files**：本插件用到的**建表配置**（`TableDefinesConfig`），每个配置的 `files` 已列出表定义 JSON，无需在插件 JSON 中再列 `table_define_files`。
- **supported_databases**：声明本插件可在哪些数据库上使用；推荐使用小写规范名：`mysql`、`postgres`、`sqlite`、`oracle`（Rust 中见 `def::supported_db`）。
- **domain_code / application_code / module_code**：与宿主「域-应用-模块」表通过编码关联，便于归类、权限与菜单组织。
- **vendor_***：开发商信息，便于追溯与合规；**development_languages**：开发语言（rust、javascript、c、c++、go、python 等），便于宿主识别与策略配置。

示例见：`plugin_definition_example.json`。

---

## 2. 插件 ZIP 压缩包与装配清单

插件可打成一个 **ZIP 包** 分发。ZIP 内**必须**在根目录放置 **`manifest.json`**，作为**装配清单**。

### 装配清单格式（manifest.json）

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `manifest_version` | string | 否 | 清单格式版本，便于后续兼容 |
| `plugin` | object | 是 | 与单文件插件定义同构；其中 **所有路径均相对 ZIP 根** |
| `entries` | string[] | 否 | **ZIP 内应包含的条目相对路径列表**，用于装配校验与完整性检查 |
| `signature_algorithm` | string | 否 | 签名算法，如 `"Ed25519"`（与 `signature` 成对出现时参与验签） |
| `signature` | string | 否 | 签名字节的 **Base64 编码**（对「不含签名字段的清单」规范 JSON 字节签名） |
| `signer_key_id` | string | 否 | 可选；签名者公钥或密钥标识，供宿主从本地信任库查找公钥 |

- **plugin**：与上面“插件定义 JSON”结构相同，`wasm_file`、`table_config_files` 中的路径均为 **ZIP 内相对路径**。
- **entries**：列出 ZIP 内期望存在的文件/目录相对路径，便于安装或校验时检查包是否完整、无缺项。
- **签名与验签**：为防篡改，可对装配清单签名。签名的**有效载荷**为：仅包含 `manifest_version`、`plugin`、`entries` 的 JSON，按该字段顺序序列化（UTF-8）得到的字节；不包含 `signature_algorithm`、`signature`、`signer_key_id`。当前支持的算法为 **Ed25519**；宿主加载 ZIP 时可传入公钥进行验签，验签通过后才注册插件。

示例见：`manifest_example.json`。

### ZIP 包建议目录结构示例

```
plugin.zip
├── manifest.json          # 装配清单（必选）
├── bin/
│   └── plugin.wasm        # WASM 入口
└── meta/
    ├── sys_tables_config.json
    ├── oracle_tables_config.json
    └── （配置中 files 所列的表定义 JSON，如 oracle_tables_01.json 等）
```

打包时保证 `manifest.json` 中 `plugin.wasm_file`、`plugin.table_config_files` 及 `entries` 与 ZIP 内实际路径一致。

### 装配清单签名与验签（防篡改）

- **签名字段**：`signature_algorithm`（如 `"Ed25519"`）、`signature`（Base64 编码的 64 字节签名）、可选 `signer_key_id`。
- **有效载荷**：用于签名的字节 = 仅包含 `manifest_version`、`plugin`、`entries` 的 JSON 序列化（UTF-8），**不包含** `signature_algorithm`、`signature`、`signer_key_id`。字段顺序固定（与 `PluginManifestSigningPayload` 一致），以保证同一清单多次序列化得到相同字节。
- **签名流程**：1）构造 `manifest.json` 内容但先不写签名字段；2）按上述规则序列化得到 payload 字节；3）用 Ed25519 私钥对 payload 签名；4）将签名 Base64 编码后与 `signature_algorithm`（及可选 `signer_key_id`）一并写入 `manifest.json`。
- **验签流程**：加载 ZIP 后解析 `manifest.json`；若清单带签名且调用方提供了 `VerifySignatureConfig`（含 Ed25519 公钥），则用同一规则构造 payload 字节，用公钥验证 `signature`；验签失败或 `require_signature == true` 但清单未带签名时，加载失败并返回 `PluginError::SignatureVerification`。

---

## 3. 注册表用法（Rust）

```rust
use cmx_core::plugin::{PluginDefinition, PluginManifest, PluginRegistry};

// 从独立 JSON 加载
let mut registry = PluginRegistry::new();
registry.load_definition_from_path(
    Path::new("path/to/plugin_definition.json"),
    Path::new("path/to/plugin_root"),
)?;

// 从 ZIP 加载（自动解压并解析 manifest.json）；第三参数为可选验签配置
let def = registry.load_from_zip_path(Path::new("path/to/plugin.zip"), None, None)?;

// 若需验签：使用 Ed25519 公钥（Base64 或 32 字节），清单带签名时自动验签
use cmx_core::plugin::VerifySignatureConfig;
let verify = VerifySignatureConfig::from_public_key_base64("YOUR_PUBLIC_KEY_BASE64")?
    .require_signature(true);  // 若要求清单必须带签名
let def = registry.load_from_zip_path(Path::new("path/to/plugin.zip"), None, Some(&verify))?;

// 按 table_config_files 加载该插件需要的全部表定义（配置内已定义表定义文件）
let tables = registry.load_tables_for_plugin(&def.id)?;

// 获取插件根路径与定义，供上层自行加载/执行 WASM（本模块不实例化）
let base = registry.base_path_for_plugin(&def.id).unwrap();
let wasm_path = base.join(&def.wasm_file);
```

---

## 4. 小结

- **插件定义**（单文件或 ZIP 内 `manifest.plugin`）：写清 **wasm 文件路径**、**建表配置 JSON 文件**、**表定义文件**（由配置的 `files` 指定）。
- **装配清单**（ZIP 内 `manifest.json`）：在 ZIP 根目录，描述 `plugin` 与可选 `entries`，所有路径相对 ZIP 根；用于安装与校验。
- **PluginRegistry**（插件注册表）：仅负责从定义或 ZIP 装配清单**加载、注册**插件元数据，验签与表配置/表定义解析；**不执行、不实例化** WASM，执行由上层按 `base_path_for_plugin` + `def.wasm_file` 自行处理。
