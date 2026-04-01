# 域-应用-模块 树形接口实现计划

## 目标

根据 `tree.sql` 中已有的 SQL 查询，在 domain 模块中实现一个树形数据查询接口，返回按 **域→应用→模块** 层级组织的数据，同级数据按 `sort_order` 排序。

## 核心设计：泛型 TreeNode

用户要求 TreeNode 使用泛型封装，支持任意数据格式的树形转换。设计如下：

### 泛型 TreeNode 结构体（放在 `rest/tree.rs`）

```rust
use serde::{Deserialize, Serialize};

/// 泛型树节点，支持任意数据格式的树形结构
///
/// 只要数据实现了 `TreeNodeData` trait（提供 id、parent_id、sort_key），
/// 就可以通过 `TreeNode::from_list()` 将扁平列表转为树形结构。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TreeNode<T> {
    /// 节点关联的业务数据
    pub data: T,
    /// 子节点列表
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreeNode<T>>,
}

/// 树节点数据 trait，用于泛型树构建
///
/// 任何需要构建树形结构的数据类型都需要实现此 trait，
/// 提供 id、parent_id 和排序字段。
pub trait TreeNodeData {
    /// 获取节点 ID（唯一标识）
    fn node_id(&self) -> &str;
    /// 获取父节点 ID（根节点返回 None）
    fn parent_id(&self) -> Option<&str>;
    /// 获取排序值（用于同级排序）
    fn sort_key(&self) -> i32;
}
```

`TreeNode<T>` 提供以下方法：
- `from_list(items: Vec<T>, sort_field: Option<&str>) -> Vec<TreeNode<T>>` — 从扁平列表构建树
  - 遍历列表，以 `node_id()` 为 key 建立 HashMap
  - 按 `parent_id()` 将子节点挂载到父节点
  - 收集 `parent_id() == None` 的节点作为根节点
  - 每层递归按 `sort_key()` 排序

### 为什么用 trait 而不是字段名参数

使用 trait 方式更符合 Rust 的类型安全理念，且 `sort_key()` 方法让排序逻辑类型化。
如果用户需要不同的排序字段，只需在具体类型的 `sort_key()` 实现中映射到对应字段即可。

---

## 实现步骤

### 步骤 1: 新建 `rest/tree.rs` — 泛型 TreeNode + TreeNodeData trait

- 定义 `TreeNodeData` trait（`node_id`、`parent_id`、`sort_key` 三个方法）
- 定义 `TreeNode<T>` 泛型结构体（`data: T` + `children: Vec<TreeNode<T>>`）
- 实现 `TreeNode::from_list()` 方法：扁平列表 → 树形结构
- 派生 `Serialize`、`Deserialize`、`ToSchema`（仅 `TreeNode`，trait 不能派生）

### 步骤 2: 修改 `rest/mod.rs` — 导出新模块

添加 `pub mod tree;` 和 `pub use tree::{TreeNode, TreeNodeData};`

### 步骤 3: 在 `domain/entity.rs` 中添加树节点数据 DTO

定义一个 `DomainTreeNodeData` 结构体，用于接收 SQL 查询结果，并实现 `TreeNodeData` trait：

```rust
/// 域-应用-模块 树形节点数据
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, FromRow)]
pub struct DomainTreeNodeData {
    pub parent_id: Option<String>,
    pub code: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    pub node_type: String,
    pub level: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
    // ... 其他字段
}

impl TreeNodeData for DomainTreeNodeData {
    fn node_id(&self) -> &str { &self.code }
    fn parent_id(&self) -> Option<&str> { self.parent_id.as_deref() }
    fn sort_key(&self) -> i32 { self.sort_order.unwrap_or(0) }
}
```

### 步骤 4: 在 `domain/service.rs` 中添加 `get_tree` 方法

- 使用 `include_str!("tree.sql")` 嵌入 SQL
- 调用 `mm.query_sql(db_id, None, &sql, "domain_tree")` 执行查询获取 `DataSet`
- 将 `DataSet` 的每行转换为 `DomainTreeNodeData`
- 调用 `TreeNode::from_list(items)` 构建树形结构
- 返回 `Vec<TreeNode<DomainTreeNodeData>>`

### 步骤 5: 在 `domain/handler.rs` 中添加 `get_tree` Handler

- HTTP 方法：`POST`
- 路径：`/api/domains/tree`
- 无请求参数
- 响应类型：`ApiResp<Vec<TreeNode<DomainTreeNodeData>>>`
- 遵循 handler 规范

### 步骤 6: 在 `domain/mod.rs` 中导出 `DomainTreeNodeData`

### 步骤 7: 在 `routes.rs` 中注册路由

### 步骤 8: 在 `openapi.rs` 中注册 OpenAPI path 和 Schema

---

## 涉及文件

| 文件 | 操作 | 说明 |
|------|------|------|
| `src/rest/tree.rs` | **新建** | 泛型 TreeNode + TreeNodeData trait |
| `src/rest/mod.rs` | 修改 | 导出 tree 模块 |
| `src/handlers/domain/entity.rs` | 修改 | 添加 DomainTreeNodeData DTO + TreeNodeData impl |
| `src/handlers/domain/service.rs` | 修改 | 添加 get_tree 方法 |
| `src/handlers/domain/handler.rs` | 修改 | 添加 get_tree handler |
| `src/handlers/domain/mod.rs` | 修改 | 导出 DomainTreeNodeData |
| `src/routes/routes.rs` | 修改 | 注册 /domains/tree 路由 |
| `src/openapi.rs` | 修改 | 注册 OpenAPI path 和 schema |
