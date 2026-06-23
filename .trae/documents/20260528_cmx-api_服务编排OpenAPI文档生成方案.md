# 服务编排 OpenAPI 文档生成方案

> 模块：cmx-api / cmx-plugin / cmx-cli
> 日期：2026-05-28

***

## 一、现状分析

### 1.1 当前架构

当前系统中，每个服务编排（`servicedata/*.json`）的接口文档由 `ApiDocGenerator` 在插件安装时自动生成，存入数据库 `cmx_service_define_version.api_doc` 字段。生成的文档是自定义的 `ServiceApiDoc` JSON 结构，**不是 OpenAPI 规范**。

### 1.2 当前存在的问题

| 问题                  | 严重程度 | 说明                                                                                                  |
| ------------------- | ---- | --------------------------------------------------------------------------------------------------- |
| **文档格式非标准**         | 高    | 当前 `ServiceApiDoc` 是自定义 JSON，无法直接导入 Swagger/Postman/Apifox 等工具                                      |
| **api.json 信息不完整**  | 高    | cmx-cli 生成的 api.json 中大量字段 type 为 `unknown`，缺少结构体展开（如 `InsertData`、`RouteInput` 等类型未解析）             |
| **functions 文档未启用** | 中    | `ApiDocGenerator` 已实现 `build_functions_doc` 但第 285 行硬编码为 `vec![]`，丢弃了编排内部函数文档                       |
| **api\_doc 读取丢失**   | 中    | `get_service`、`get_services_by_plugin`、`list_services` 均硬编码 `api_doc: None`，只有 `page_services` 正确返回 |
| **无法按域/应用/模块分组**    | 高    | 没有将所有服务组合成统一 OpenAPI 文档的能力                                                                          |
| **缺少统一的 API 入口**    | 高    | 每个服务 key 的入参不同，但 `/api/service/execute/{service-key}` 是统一入口，无法为每个服务生成独立端点                           |

### 1.3 目标

1. 为每个服务编排生成符合 **OpenAPI 3.0 规范** 的接口文档
2. 将所有服务的 OpenAPI 文档组合成 **按域/应用/模块分组** 的统一文档
3. 可直接导入 Swagger UI / Postman / Apifox 等工具进行调试
4. 解决 api.json 信息不完整的问题

***

## 二、整体方案设计

### 2.1 方案概览

```
┌─────────────────────────────────────────────────────────────┐
│                      改进分三层                              │
├─────────────────────────────────────────────────────────────┤
│ Layer 1: cmx-cli 改进                                       │
│  - 改善注释解析和结构体展开                                    │
│  - 生成更完整的 api.json                                     │
│  - 新增 openapi 子命令直接输出 OpenAPI 格式                    │
├─────────────────────────────────────────────────────────────┤
│ Layer 2: ApiDocGenerator 改进 (cmx-plugin)                   │
│  - 基于 api.json + flow.json 生成 OpenAPI PathItem           │
│  - 替换当前自定义 ServiceApiDoc 为 OpenAPI Schema             │
│  - 启用 functions 文档                                       │
├─────────────────────────────────────────────────────────────┤
│ Layer 3: 新增 OpenAPI 聚合 API (cmx-api)                     │
│  - 新增 GET /api/service/openapi 端点                        │
│  - 按域/应用/模块分组聚合所有服务                               │
│  - 输出完整 OpenAPI 3.0 文档                                 │
│  - 支持按 service_key 查询单个服务文档                         │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 核心思路

**每个服务编排本质上是一个 POST 接口**，统一路径为 `/api/service/execute/{service-key}`，但每个服务的 `input` 和 `output` 不同。我们利用 OpenAPI 的 `paths` + `schemas` 机制：

* 为每个 `service_key` 生成独立的 **Path**：`/api/service/execute/{service-key}`

* 为每个服务生成独立的 **Request Schema** 和 **Response Schema**

* 用 OpenAPI 的 `tags` 实现按域/应用/模块分组

***

## 三、详细设计

### 3.1 Layer 1：cmx-cli 改进

#### 3.1.1 当前不足

查看 `api.json` 中 `route_check` 函数的输出：

```json
{
  "name": "route_check",
  "input": {
    "fields": [
      {
        "name": "input",
        "type": "unknown",        // ← 应该解析出 RouteInput 的实际字段
        "description": "函数输入，包含 `RouteInput` 格式的路由参数"
      }
    ]
  }
}
```

`type: "unknown"` 的原因是：函数签名是 `Msgpack<FunctionInput>`，`FunctionInput.input` 字段类型为 `serde_json::Value`，AST 解析器无法从签名得知实际业务类型。

#### 3.1.2 改进方案

**方案：在** **`/// # Arguments`** **注释中使用结构化表格声明完整参数**

当前部分函数（如 `tx_insert`）已经有良好的注释：

```rust
/// # Arguments
///
/// * `input` - 函数输入，包含 `InsertData` 格式的插入数据
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `input.table` | string | 是 | 表名 |
/// | `input.name` | string | 是 | 名称字段值 |
/// | `input.value` | integer | 是 | 数值字段值 |
```

cmx-cli 的 AST 模式已经能解析这种表格格式（`parse_table` 函数），生成的 api.json 中 `tx_insert` 已经有正确的 properties。

**问题在于**：部分函数的注释不够完整（如 `branch_1_process` 只有 `/// * \`input\` - 函数输入\` 没有详细字段表格）。

**改进措施**：

1. **规范注释模板**：要求所有 `#[plugin_fn]` 函数必须在 `# Arguments` 中用表格声明 `input` 的完整字段结构
2. **改进 AST 解析器**：在 `ast_json_gen.rs` 中，当遇到 `type: "unknown"` 时，尝试从描述中提取反引号包裹的类型名（如 `RouteInput`），然后在 `TypeRegistry` 中查找并展开
3. **新增 openapi 子命令**（可选）：在 cmx-cli 中新增 `doc openapi` 子命令，直接从 Rust 源码生成 OpenAPI 格式文档，方便开发阶段预览

#### 3.1.3 cmx-cli 新增子命令

```bash
# 现有：生成 api.json
cmx-cli doc scan ./src --mode ast --output api/api.json

# 新增：生成 OpenAPI 格式文档（可选）
cmx-cli doc openapi ./src --output openapi.json --plugin-id example_plugin
```

### 3.2 Layer 2：ApiDocGenerator 改进

#### 3.2.1 新增 OpenAPI 生成能力

在 `api_doc_generator.rs` 中，新增 `generate_openapi_doc` 方法，将现有的 `ServiceApiDoc` 转换为 OpenAPI 3.0 的 `PathItem` + `Schema` 结构。

**核心数据结构**：

```rust
/// 单个服务的 OpenAPI 文档片段
pub struct ServiceOpenApiDoc {
    /// 服务标识
    pub service_key: String,
    /// OpenAPI Path 对象（一个 POST 端点）
    pub path: String,
    /// PathItem 定义
    pub path_item: Value,  // serde_json::Value，符合 OpenAPI PathItem 规范
    /// 请求体 Schema
    pub request_schema: Value,
    /// 响应 Schema
    pub response_schema: Value,
    /// Tag（用于分组，格式 "domain/application/module"）
    pub tags: Vec<String>,
}
```

#### 3.2.2 OpenAPI Schema 生成逻辑

**请求体 Schema**：

根据入口节点的 `api.json` 函数定义生成：

```json
{
  "type": "object",
  "properties": {
    "input": {
      "$ref": "#/components/schemas/BmServiceInput"
    },
    "include_steps": {
      "type": "boolean",
      "description": "是否返回步骤数据",
      "default": false
    },
    "debug": {
      "type": "boolean",
      "description": "是否开启调试模式",
      "default": false
    }
  },
  "required": ["input"]
}
```

其中 `BmServiceInput`（以 service\_key 为前缀命名）根据入口函数的参数生成：

```json
{
  "BmServiceInput": {
    "type": "object",
    "description": "服务 bm 的入参（来源: route_check 函数）",
    "properties": {
      "route": {
        "type": "string",
        "description": "路由标识，取值为 1、2、3 或 4",
        "enum": ["1", "2", "3", "4"]
      }
    },
    "required": ["route"]
  }
}
```

**响应 Schema**：

```json
{
  "BmServiceResponse": {
    "type": "object",
    "description": "服务 bm 的出参",
    "properties": {
      "success": { "type": "boolean" },
      "output": { "$ref": "#/components/schemas/BmServiceOutput" },
      "total_elapsed_us": { "type": "integer", "description": "总耗时(微秒)" },
      "error": {
        "type": "object",
        "properties": {
          "message": { "type": "string" }
        }
      }
    }
  }
}
```

#### 3.2.3 Path 生成

为每个 service\_key 生成两条路径：

```
POST /api/service/execute/{service-key}   # 路径参数版本
POST /api/service/execute                  # 请求体版本（service_key 在 body 中）
```

每个 PathItem 的 Operation 包含：

* `operationId`: `execute_{service_key}`

* `summary`: 服务名称

* `description`: 服务描述

* `tags`: `["{domain_name}/{application_name}/{module_name}"]`

* `requestBody`: 引用对应的 Schema

* `responses`: 200 响应引用对应的 Schema

#### 3.2.4 多分支输出处理

当服务编排包含 `skylake-switch` 节点时，输出可能有多个分支。使用 OpenAPI 的 `oneOf` 表达：

```json
{
  "BmServiceOutput": {
    "oneOf": [
      { "$ref": "#/components/schemas/BmBranch1Output" },
      { "$ref": "#/components/schemas/BmBranch2Output" },
      { "$ref": "#/components/schemas/BmBranch3Output" }
    ],
    "description": "输出取决于运行时分支选择"
  }
}
```

#### 3.2.5 数据库存储改进

将 `api_doc` 字段的格式从自定义 `ServiceApiDoc` 改为存储两种格式：

```rust
pub struct ServiceApiDocV2 {
    /// OpenAPI 3.0 格式文档片段
    pub openapi: Value,
    /// 保留旧格式兼容
    pub legacy: Option<ServiceApiDoc>,
}
```

或更简单的方案：**直接替换为 OpenAPI 格式**，在 `api_doc` 字段中存储完整的 OpenAPI `PathItem` JSON。

#### 3.2.6 修复 functions 文档

将第 285 行的 `functions: vec![]` 改为 `functions: functions_doc`，启用内部函数文档。

### 3.3 Layer 3：OpenAPI 聚合 API

#### 3.3.1 新增 API 端点

在 `cmx-api` 中新增以下端点：

| 方法  | 路径                                   | 功能                       |
| --- | ------------------------------------ | ------------------------ |
| GET | `/api/service/openapi`               | 获取所有服务的完整 OpenAPI 3.0 文档 |
| GET | `/api/service/openapi/{service-key}` | 获取单个服务的 OpenAPI 文档       |
| GET | `/api/service/openapi/spec`          | 获取按域/应用/模块分组的 OpenAPI 文档 |

#### 3.3.2 聚合 OpenAPI 文档结构

```json
{
  "openapi": "3.0.3",
  "info": {
    "title": "CMX 服务编排 API",
    "version": "1.0.0",
    "description": "所有服务编排的统一接口文档"
  },
  "servers": [
    { "url": "http://localhost:8080", "description": "本地开发" }
  ],
  "tags": [
    {
      "name": "电子商务/订单系统/处理模块",
      "description": "域:电子商务 > 应用:订单系统 > 模块:处理模块"
    },
    {
      "name": "示例/演示/测试",
      "description": "域:示例 > 应用:演示 > 模块:测试"
    }
  ],
  "paths": {
    "/api/service/execute/order-process": {
      "post": {
        "operationId": "execute_order-process",
        "summary": "订单处理服务",
        "description": "处理订单创建和支付流程",
        "tags": ["电子商务/订单系统/处理模块"],
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": { "$ref": "#/components/schemas/OrderProcessRequest" }
            }
          }
        },
        "responses": {
          "200": {
            "description": "成功",
            "content": {
              "application/json": {
                "schema": { "$ref": "#/components/schemas/OrderProcessResponse" }
              }
            }
          }
        }
      }
    },
    "/api/service/execute/bm": { ... }
  },
  "components": {
    "schemas": {
      "OrderProcessRequest": { ... },
      "OrderProcessResponse": { ... },
      "BmServiceRequest": { ... },
      "BmServiceResponse": { ... }
    }
  }
}
```

#### 3.3.3 聚合逻辑

```
1. 查询所有服务定义 (page_services 全量)
2. 遍历每个服务：
   a. 从 api_doc 字段读取 OpenAPI PathItem
   b. 收集所有 schemas 到 components.schemas
   c. 用 domain_name/application_name/module_name 生成 tag
3. 组装完整 OpenAPI 文档
4. 返回
```

#### 3.3.4 缓存策略

* OpenAPI 聚合文档在首次请求时生成，缓存到内存

* 当插件安装/升级/卸载时，通过事件通知清除缓存

* 支持 `?refresh=true` 参数强制刷新

***

## 四、实施步骤

### 阶段一：完善 cmx-cli 文档生成（优先级最高）

1. **改善 Rust 源码注释**：补全所有 `#[plugin_fn]` 函数的 `# Arguments` 表格注释，特别是 `branch_*_process`、`merge_result`、`final_process` 等函数
2. **改进 AST 解析器**：增强 `extract_type_from_description` 逻辑，当 type 为 unknown 时从描述中提取类型名并展开
3. **验证**：重新运行 `cmx-cli doc scan` 确认生成的 api.json 所有字段类型正确

### 阶段二：改造 ApiDocGenerator

1. **新增 OpenAPI Schema 生成模块**：在 `api_doc_generator.rs` 中新增 `generate_openapi_schema` 方法
2. **实现 ParameterDoc → OpenAPI Schema 转换**：将现有参数文档转换为 JSON Schema 格式
3. **实现多分支 oneOf 处理**
4. **修改数据库存储格式**：api\_doc 字段存储 OpenAPI PathItem JSON
5. **启用 functions 文档**
6. **修复 api\_doc 读取丢失问题**：修复 `get_service`、`get_services_by_plugin` 中 `api_doc: None` 的问题

### 阶段三：新增聚合 API

1. **新增 OpenAPI 相关 Handler**：在 `cmx-api` 中新增 `/api/service/openapi` 端点
2. **实现聚合逻辑**：从数据库读取所有服务的 api\_doc，组装完整 OpenAPI 文档
3. **实现按域/应用/模块分组**：使用 OpenAPI tags 实现
4. **实现缓存机制**：首次请求时生成并缓存，插件变更时清除

***

## 五、OpenAPI Schema 映射关系

### 5.1 api.json 字段 → OpenAPI Schema 映射

| api.json 字段类型 | OpenAPI Schema type                           |
| ------------- | --------------------------------------------- |
| `string`      | `{ "type": "string" }`                        |
| `integer`     | `{ "type": "integer" }`                       |
| `number`      | `{ "type": "number" }`                        |
| `boolean`     | `{ "type": "boolean" }`                       |
| `object`      | `{ "type": "object", "properties": {...} }`   |
| `array`       | `{ "type": "array", "items": {...} }`         |
| `unknown`     | `{ "type": "object", "description": "未知类型" }` |

### 5.2 服务编排 → OpenAPI Path 映射

| 服务编排元素                                     | OpenAPI 元素                                |
| ------------------------------------------ | ----------------------------------------- |
| `service_key` (code)                       | Path `/api/service/execute/{service-key}` |
| `name`                                     | Operation `summary`                       |
| `description`                              | Operation `description`                   |
| 入口函数 input                                 | RequestBody Schema                        |
| 出口函数 output                                | Response Schema                           |
| `domain_code/application_code/module_code` | Tag 名称                                    |
| `skylake-switch` 多分支                       | Response `oneOf`                          |

### 5.3 编排节点函数 → Schema 命名规则

```
Schema 名称 = {ServiceKey} + {用途} + Schema

示例：
- BmServiceRequest        (服务入参)
- BmServiceResponse       (服务出参，包装在 ApiResp 中)
- BmServiceOutput         (服务实际输出)
- BmBranch1Output         (分支1输出)
- BmBranch2Output         (分支2输出)
```

***

## 六、关键设计决策

### 6.1 为什么在运行时生成而非静态文件？

* 服务编排可能跨插件引用函数，只有运行时才能确定完整的函数参数信息

* 插件安装/升级时 api\_doc 自动生成，保证与最新代码同步

* 支持动态服务注册/卸载场景

### 6.2 为什么同时保留 Path 参数版和 Body 版？

* `POST /api/service/execute/{service-key}` 路径参数版更适合 API 网关和工具导入

* `POST /api/service/execute` 请求体版保持向后兼容

* OpenAPI 文档中两者都生成，供不同场景使用

### 6.3 Tag 分组策略

```
Tag 命名: "{domain_name}/{application_name}/{module_name}"
示例: "电子商务/订单系统/处理模块"

当域/应用/模块信息缺失时降级为:
- 无模块: "电子商务/订单系统"
- 无应用: "电子商务"
- 全部缺失: "未分类"
```

***

## 七、已确认的设计决策

1. **api\_doc 字段格式**：**直接替换为 OpenAPI 格式**，不保留旧的自定义 `ServiceApiDoc` 格式
2. **Swagger UI**：Phase 1 **仅提供 OpenAPI JSON 端点**，不嵌入 Swagger UI，后续再集成
3. **认证信息**：OpenAPI 文档中**不声明 security**，由使用方自行处理认证
4. **servers URL**：从配置文件读取，支持动态设置

