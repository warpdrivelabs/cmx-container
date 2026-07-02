//! 服务接口文档生成器
//!
//! 根据服务编排文件（servicedata/*.json）和函数文档（api/api.json），
//! 自动生成服务入参和出参的接口文档。
//!
//! # 核心逻辑
//!
//! 1. 解析编排文件，找到入口/出口可执行节点（skylake-func / skylake-switch）
//! 2. 通过 PluginQuery 查询跨插件版本，加载所有引用插件的 api.json
//! 3. 通过 api.json 获取每个节点的参数信息
//! 4. 生成接口文档 JSON

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmx_core::model::service::{ServiceEdge, ServiceNode, ServiceOrchestration};
use cmx_traits::plugin::PluginQuery;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::PluginResult;

/// 可执行节点类型集合（拥有 nodeMeta 的节点）
const EXECUTABLE_NODE_TYPES: &[&str] = &["skylake-func", "skylake-switch"];

// ==================== 文档结构体 ====================

/// 服务接口文档
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceApiDoc {
    /// 服务基本信息
    pub service: ServiceInfo,
    /// 输入参数文档
    pub input: InputDoc,
    /// 输出参数文档（支持多分支）
    pub output: OutputDoc,
    /// 编排中所有函数的文档
    pub functions: Vec<FunctionDoc>,
}

/// 服务基本信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// 服务唯一标识
    pub key: String,
    /// 服务名称
    pub name: String,
    /// 服务描述
    pub description: String,
    /// 所属插件ID
    pub plugin_id: String,
    /// 版本号
    pub version: String,
}

/// 输入参数文档
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputDoc {
    /// 输入描述
    pub description: String,
    /// 入参来源节点ID
    pub source_node_id: String,
    /// 入参来源节点类型
    pub source_node_type: String,
    /// 参数列表
    pub parameters: Vec<ParameterDoc>,
}

/// 输出参数文档（支持多分支）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputDoc {
    /// 输出描述
    pub description: String,
    /// 输出分支列表（多分支场景下每个分支独立描述）
    pub branches: Vec<OutputBranch>,
}

/// 单个输出分支
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputBranch {
    /// 分支名称（使用出口节点的 data.name）
    pub branch_name: String,
    /// 出口节点ID
    pub source_node_id: String,
    /// 出口节点类型
    pub source_node_type: String,
    /// 参数列表
    pub parameters: Vec<ParameterDoc>,
}

/// 函数文档
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDoc {
    /// 节点ID
    pub node_id: String,
    /// 节点类型
    pub node_type: String,
    /// 节点名称
    pub name: String,
    /// 拓扑顺序序号
    pub step_index: usize,
    /// 所属插件ID
    pub plugin_id: String,
    /// 函数名称
    pub function_name: String,
    /// 函数摘要
    pub summary: String,
    /// 输入参数
    pub input_parameters: Vec<ParameterDoc>,
    /// 输出参数
    pub output_parameters: Vec<ParameterDoc>,
}

/// 参数定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterDoc {
    /// 参数名
    pub name: String,
    /// 参数类型（string, integer, object, array, boolean）
    pub param_type: String,
    /// 是否必填（输出参数可能无此字段）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// 参数描述
    pub description: String,
    /// 对象类型时的嵌套属性
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Vec<ParameterDoc>>,
}

// ==================== api.json 解析结构体 ====================

/// api.json 根结构
#[derive(Debug, Clone, Deserialize)]
pub struct PluginApiDef {
    /// 插件信息
    pub plugin: PluginApiInfo,
    /// 函数列表
    pub functions: Vec<ApiFunction>,
}

/// api.json 插件信息
#[derive(Debug, Clone, Deserialize)]
pub struct PluginApiInfo {
    /// 插件名称
    pub name: String,
    /// 插件版本
    pub version: String,
}

/// api.json 函数定义
#[derive(Debug, Clone, Deserialize)]
pub struct ApiFunction {
    /// 函数名
    pub name: String,
    /// 函数类型（func / branch_fn）
    #[serde(default)]
    pub r#type: String,
    /// 函数摘要
    #[serde(default)]
    pub summary: String,
    /// 函数详细描述
    #[serde(default)]
    pub description: String,
    /// 函数输入
    #[serde(default)]
    pub input: ApiFunctionIO,
    /// 函数输出
    #[serde(default)]
    pub output: ApiFunctionIO,
}

/// api.json 函数输入输出
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ApiFunctionIO {
    /// 字段列表
    #[serde(default)]
    pub fields: Vec<ApiField>,
}

/// api.json 字段定义
#[derive(Debug, Clone, Deserialize)]
pub struct ApiField {
    /// 字段名
    pub name: String,
    /// 字段类型
    #[serde(rename = "type", default)]
    pub field_type: String,
    /// 是否必填
    #[serde(default)]
    pub required: bool,
    /// 字段描述
    #[serde(default)]
    pub description: String,
    /// 嵌套属性（对象类型时）
    #[serde(default)]
    pub properties: Vec<ApiField>,
}

// ==================== 生成器 ====================

/// 接口文档生成器
pub struct ApiDocGenerator {
    /// 插件安装根目录
    plugin_root: PathBuf,
    /// 应用隔离标识
    app_id: String,
    /// 插件查询接口（用于查询跨插件版本）
    plugin_query: Arc<dyn PluginQuery>,
}

impl ApiDocGenerator {
    /// 创建接口文档生成器
    pub fn new(plugin_root: PathBuf, app_id: String, plugin_query: Arc<dyn PluginQuery>) -> Self {
        Self {
            plugin_root,
            app_id,
            plugin_query,
        }
    }

    /// 生成服务接口文档
    pub async fn generate_api_doc(
        &self,
        orchestration: &ServiceOrchestration,
        current_plugin_id: &str,
        current_plugin_version: &str,
        install_path: &Path,
    ) -> PluginResult<Value> {
        let nodes = &orchestration.flow.nodes;
        let edges = &orchestration.flow.edges;

        let current_api = self.load_current_plugin_api(install_path);

        let executable_nodes = Self::collect_executable_nodes(nodes, edges);

        let mut api_cache: HashMap<String, Option<PluginApiDef>> = HashMap::new();
        api_cache.insert(current_plugin_id.to_string(), current_api);

        for (node, _) in &executable_nodes {
            if let Some(data) = &node.data
                && let Some(node_meta) = &data.node_meta
            {
                let pid = &node_meta.plugin_id;
                if !api_cache.contains_key(pid) {
                    let api = self.load_plugin_api(pid).await;
                    api_cache.insert(pid.clone(), api);
                }
            }
        }

        let entry_node = Self::find_entry_node(nodes, edges);
        let exit_nodes = Self::find_exit_nodes(nodes, edges);

        let service_key = &orchestration.code;
        let schema_prefix = to_pascal_case(service_key);

        let input_params = match &entry_node {
            Some(node) => {
                let (plugin_id, function_name) = extract_node_meta(node);
                Self::resolve_parameters(&plugin_id, &function_name, true, &api_cache)
            }
            None => vec![],
        };

        let input_schema = build_request_input_schema(&input_params);
        let output_schema = build_output_schema(&exit_nodes, &api_cache, &schema_prefix);

        let mut schemas = serde_json::Map::new();
        schemas.insert(format!("{schema_prefix}Input"), input_schema);

        let request_schema = json!({
            "type": "object",
            "properties": {
                "input": { "$ref": format!("#/components/schemas/{schema_prefix}Input") },
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
        });

        let response_data_schema = build_response_data_schema(&schema_prefix, &output_schema);
        schemas.insert(format!("{schema_prefix}Output"), output_schema);
        schemas.insert(format!("{schema_prefix}ResponseData"), response_data_schema);

        let response_schema = json!({
            "type": "object",
            "properties": {
                "code": { "type": "integer", "description": "状态码，0 表示成功" },
                "msg": { "type": "string", "description": "状态消息" },
                "data": { "$ref": format!("#/components/schemas/{schema_prefix}ResponseData") }
            }
        });
        schemas.insert(format!("{schema_prefix}Response"), response_schema);

        let path = format!("/api/service/execute/{service_key}");
        let operation_id = format!("execute_{service_key}");

        let path_item = json!({
            "post": {
                "operationId": operation_id,
                "summary": orchestration.name,
                "description": orchestration.description,
                "requestBody": {
                    "required": true,
                    "content": {
                        "application/json": {
                            "schema": { "$ref": format!("#/components/schemas/{schema_prefix}Request") }
                        }
                    }
                },
                "responses": {
                    "200": {
                        "description": "成功",
                        "content": {
                            "application/json": {
                                "schema": { "$ref": format!("#/components/schemas/{schema_prefix}Response") }
                            }
                        }
                    }
                }
            }
        });

        schemas.insert(format!("{schema_prefix}Request"), request_schema);

        let functions_doc = Self::build_functions_doc(&executable_nodes, &api_cache);
        let functions_value: Vec<Value> = functions_doc
            .iter()
            .map(|f| serde_json::to_value(f).unwrap_or_default())
            .collect();

        let result = json!({
            "path": path,
            "path_item": path_item,
            "schemas": schemas,
            "service_info": {
                "key": service_key,
                "name": orchestration.name,
                "description": orchestration.description,
                "plugin_id": current_plugin_id,
                "version": current_plugin_version
            },
            "functions": functions_value
        });

        Ok(result)
    }

    /// 找到入口可执行节点（起点后的第一个 func 或 switch 节点）
    fn find_entry_node<'a>(
        nodes: &'a [ServiceNode],
        edges: &'a [ServiceEdge],
    ) -> Option<&'a ServiceNode> {
        // 找到 start 节点
        let start_node = nodes.iter().find(|n| n.node_type == "skylake-start")?;

        // 从 start 出发，沿边 BFS 找到第一个可执行节点
        let mut queue = vec![start_node.id.clone()];
        let mut visited = std::collections::HashSet::new();
        visited.insert(start_node.id.clone());

        while let Some(current_id) = queue.first().cloned() {
            queue.remove(0);

            for edge in edges {
                if edge.source_node_id == current_id && !visited.contains(&edge.target_node_id) {
                    visited.insert(edge.target_node_id.clone());

                    if let Some(target_node) = nodes.iter().find(|n| n.id == edge.target_node_id) {
                        if EXECUTABLE_NODE_TYPES.contains(&target_node.node_type.as_str()) {
                            return Some(target_node);
                        }
                        // 如果是 transaction 节点，进入事务框内部查找
                        if target_node.node_type == "skylake-transaction"
                            && let Some(inner_entry) =
                                Self::find_entry_in_transaction(target_node, nodes, edges)
                        {
                            return Some(inner_entry);
                        }
                        queue.push(edge.target_node_id.clone());
                    }
                }
            }
        }
        None
    }

    /// 在事务框内找到入口可执行节点
    fn find_entry_in_transaction<'a>(
        transaction_node: &ServiceNode,
        nodes: &'a [ServiceNode],
        edges: &'a [ServiceEdge],
    ) -> Option<&'a ServiceNode> {
        // 找到事务框内指向 transaction 的入边
        for edge in edges {
            if edge.target_node_id == transaction_node.id {
                // 找到入边的源节点
                if let Some(source) = nodes.iter().find(|n| n.id == edge.source_node_id) {
                    // 如果源节点在事务框内且是可执行节点
                    if source.parent.as_deref() == Some(&transaction_node.id)
                        && EXECUTABLE_NODE_TYPES.contains(&source.node_type.as_str())
                    {
                        return Some(source);
                    }
                }
            }
        }

        // 降级：找事务框内第一个可执行节点（按 parent 字段匹配）
        let children: Vec<&ServiceNode> = nodes
            .iter()
            .filter(|n| n.parent.as_deref() == Some(&transaction_node.id))
            .collect();

        // 找到没有入边（从事务框外来的入边除外）的子节点
        for child in &children {
            if EXECUTABLE_NODE_TYPES.contains(&child.node_type.as_str()) {
                // 检查是否有从事务框内其他节点指向此节点的边
                let has_internal_input = edges.iter().any(|e| {
                    e.target_node_id == child.id
                        && nodes.iter().any(|n| {
                            n.id == e.source_node_id
                                && n.parent.as_deref() == Some(&transaction_node.id)
                        })
                });
                if !has_internal_input {
                    return Some(child);
                }
            }
        }

        // 最终降级：返回事务框内第一个可执行节点
        children
            .iter()
            .find(|n| EXECUTABLE_NODE_TYPES.contains(&n.node_type.as_str()))
            .copied()
    }

    /// 找到出口可执行节点（终点前的所有可执行节点）
    fn find_exit_nodes<'a>(
        nodes: &'a [ServiceNode],
        edges: &'a [ServiceEdge],
    ) -> Vec<&'a ServiceNode> {
        let mut exit_nodes = Vec::new();

        // 找到所有 end 节点
        let end_nodes: Vec<&ServiceNode> = nodes
            .iter()
            .filter(|n| n.node_type == "skylake-end")
            .collect();

        for end_node in &end_nodes {
            // 找到所有指向 end 的边
            for edge in edges {
                if edge.target_node_id == end_node.id
                    && let Some(source_node) = nodes.iter().find(|n| n.id == edge.source_node_id)
                {
                    if EXECUTABLE_NODE_TYPES.contains(&source_node.node_type.as_str()) {
                        exit_nodes.push(source_node);
                    } else if source_node.node_type == "skylake-transaction" {
                        // 进入事务框内部找出口
                        if let Some(inner_exit) =
                            Self::find_exit_in_transaction(source_node, nodes, edges)
                        {
                            exit_nodes.push(inner_exit);
                        }
                    }
                }
            }
        }

        // 去重
        let mut seen = std::collections::HashSet::new();
        exit_nodes.retain(|n| seen.insert(n.id.clone()));
        exit_nodes
    }

    /// 在事务框内找到出口可执行节点
    fn find_exit_in_transaction<'a>(
        transaction_node: &ServiceNode,
        nodes: &'a [ServiceNode],
        edges: &[ServiceEdge],
    ) -> Option<&'a ServiceNode> {
        let children: Vec<&ServiceNode> = nodes
            .iter()
            .filter(|n| n.parent.as_deref() == Some(&transaction_node.id))
            .collect();

        // 找到有出边指向事务框外的子节点
        for child in &children {
            if EXECUTABLE_NODE_TYPES.contains(&child.node_type.as_str()) {
                let has_output_to_transaction = edges.iter().any(|e| {
                    e.source_node_id == child.id && e.target_node_id == transaction_node.id
                });
                if has_output_to_transaction {
                    return Some(child);
                }
            }
        }

        // 降级：返回事务框内最后一个可执行节点
        children
            .iter()
            .rfind(|n| EXECUTABLE_NODE_TYPES.contains(&n.node_type.as_str()))
            .copied()
    }

    /// 收集编排中所有可执行节点，按拓扑顺序排列
    fn collect_executable_nodes<'a>(
        nodes: &'a [ServiceNode],
        edges: &'a [ServiceEdge],
    ) -> Vec<(&'a ServiceNode, usize)> {
        let mut result = Vec::new();
        let mut step_index = 0;

        // 找到 start 节点
        let start_node = match nodes.iter().find(|n| n.node_type == "skylake-start") {
            Some(n) => n,
            None => return result,
        };

        // BFS 遍历
        let mut queue = vec![start_node.id.clone()];
        let mut visited = std::collections::HashSet::new();
        visited.insert(start_node.id.clone());

        while let Some(current_id) = queue.first().cloned() {
            queue.remove(0);

            for edge in edges {
                if edge.source_node_id == current_id && !visited.contains(&edge.target_node_id) {
                    visited.insert(edge.target_node_id.clone());

                    if let Some(target_node) = nodes.iter().find(|n| n.id == edge.target_node_id) {
                        if EXECUTABLE_NODE_TYPES.contains(&target_node.node_type.as_str()) {
                            result.push((target_node, step_index));
                            step_index += 1;
                        }
                        // 跳过 end 节点，不加入队列
                        if target_node.node_type != "skylake-end" {
                            queue.push(edge.target_node_id.clone());
                        }
                    }
                }
            }
        }

        result
    }

    /// 加载当前插件的 api.json
    fn load_current_plugin_api(&self, install_path: &Path) -> Option<PluginApiDef> {
        let api_path = install_path.join("api").join("api.json");
        if !api_path.exists() {
            tracing::debug!("当前插件 api.json 不存在: {:?}", api_path);
            return None;
        }
        let content = std::fs::read_to_string(&api_path).ok()?;
        match serde_json::from_str(&content) {
            Ok(api) => Some(api),
            Err(e) => {
                tracing::warn!("解析当前插件 api.json 失败: {:?}", e);
                None
            }
        }
    }

    /// 加载指定插件的 API 定义（跨插件）
    async fn load_plugin_api(&self, plugin_id: &str) -> Option<PluginApiDef> {
        // 1. 查询目标插件当前版本
        let snapshot = match self.plugin_query.get_plugin(plugin_id).await {
            Ok(Some(s)) => s,
            Ok(None) => {
                tracing::warn!("跨插件 {} 未安装，无法加载 api.json", plugin_id);
                return None;
            }
            Err(e) => {
                tracing::warn!("查询跨插件 {} 失败: {:?}", plugin_id, e);
                return None;
            }
        };

        // 2. 拼接 api.json 路径
        let api_path = self
            .plugin_root
            .join(&self.app_id)
            .join(plugin_id)
            .join(&snapshot.version)
            .join("api")
            .join("api.json");

        // 3. 读取并解析
        if !api_path.exists() {
            tracing::warn!("跨插件 api.json 不存在: {:?}", api_path);
            return None;
        }
        let content = tokio::fs::read_to_string(&api_path).await.ok()?;
        match serde_json::from_str(&content) {
            Ok(api) => Some(api),
            Err(e) => {
                tracing::warn!("解析跨插件 api.json 失败: {:?}", e);
                None
            }
        }
    }

    fn build_functions_doc(
        executable_nodes: &[(&ServiceNode, usize)],
        api_cache: &HashMap<String, Option<PluginApiDef>>,
    ) -> Vec<FunctionDoc> {
        let mut functions = Vec::new();
        for (node, step_index) in executable_nodes {
            let (plugin_id, function_name) = extract_node_meta(node);

            let input_parameters =
                Self::resolve_parameters(&plugin_id, &function_name, true, api_cache);
            let output_parameters =
                Self::resolve_parameters(&plugin_id, &function_name, false, api_cache);

            // 从 api.json 获取 summary
            let summary = api_cache
                .get(&plugin_id)
                .and_then(|opt| opt.as_ref())
                .and_then(|api| {
                    api.functions
                        .iter()
                        .find(|f| f.name == function_name)
                        .map(|f| f.summary.clone())
                })
                .unwrap_or_default();

            functions.push(FunctionDoc {
                node_id: node.id.clone(),
                node_type: node.node_type.clone(),
                name: node
                    .data
                    .as_ref()
                    .map(|d| d.name.clone())
                    .unwrap_or_default(),
                step_index: *step_index,
                plugin_id,
                function_name,
                summary,
                input_parameters,
                output_parameters,
            });
        }
        functions
    }

    /// 解析参数信息：从 api.json 获取参数定义
    fn resolve_parameters(
        plugin_id: &str,
        function_name: &str,
        is_input: bool,
        api_cache: &HashMap<String, Option<PluginApiDef>>,
    ) -> Vec<ParameterDoc> {
        let api_fields = api_cache
            .get(plugin_id)
            .and_then(|opt| opt.as_ref())
            .and_then(|api| {
                api.functions
                    .iter()
                    .find(|f| f.name == function_name)
                    .map(|f| {
                        if is_input {
                            &f.input.fields
                        } else {
                            &f.output.fields
                        }
                    })
            });

        match api_fields {
            Some(fields) if !fields.is_empty() => {
                fields.iter().map(Self::api_field_to_param_doc).collect()
            }
            _ => {
                if !is_input {
                    vec![ParameterDoc {
                        name: "output".to_string(),
                        param_type: "unknown".to_string(),
                        required: None,
                        description: "输出结构未在 API 文档中定义".to_string(),
                        properties: None,
                    }]
                } else {
                    vec![]
                }
            }
        }
    }

    /// 将 ApiField 转换为 ParameterDoc
    fn api_field_to_param_doc(field: &ApiField) -> ParameterDoc {
        ParameterDoc {
            name: field.name.clone(),
            param_type: field.field_type.clone(),
            required: Some(field.required),
            description: field.description.clone(),
            properties: if field.properties.is_empty() {
                None
            } else {
                Some(
                    field
                        .properties
                        .iter()
                        .map(Self::api_field_to_param_doc)
                        .collect(),
                )
            },
        }
    }
}

/// 从 ServiceNode 提取 plugin_id 和 function_name
fn extract_node_meta(node: &ServiceNode) -> (String, String) {
    match &node.data {
        Some(data) => match &data.node_meta {
            Some(meta) => (meta.plugin_id.clone(), meta.function_name.clone()),
            None => (String::new(), String::new()),
        },
        None => (String::new(), String::new()),
    }
}

fn to_pascal_case(s: &str) -> String {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_ascii_uppercase().to_string() + &chars.as_str().to_lowercase()
                }
            }
        })
        .collect()
}

fn unwrap_input_params(params: &[ParameterDoc]) -> &[ParameterDoc] {
    if params.len() == 1
        && params[0].name == "input"
        && params[0].param_type == "object"
        && let Some(props) = &params[0].properties
        && !props.is_empty()
    {
        return props;
    }
    params
}

fn is_basic_type(t: &str) -> bool {
    matches!(t, "string" | "integer" | "number" | "boolean")
}

fn unwrap_output_params(params: &[ParameterDoc]) -> &[ParameterDoc] {
    if params.len() == 1
        && params[0].name == "output"
        && params[0].param_type == "object"
        && let Some(props) = &params[0].properties
        && !props.is_empty()
    {
        return props;
    }
    params
}

fn build_request_input_schema(params: &[ParameterDoc]) -> Value {
    let params = unwrap_input_params(params);

    if params.is_empty() {
        return json!({
            "type": "object",
            "description": "此服务无需特定入参"
        });
    }

    if params.len() == 1 && is_basic_type(&params[0].param_type) {
        let schema = param_doc_to_openapi_schema(&params[0]);
        return schema;
    }

    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for param in params {
        let schema = param_doc_to_openapi_schema(param);
        properties.insert(param.name.clone(), schema);
        if param.required.unwrap_or(false) {
            required.push(param.name.clone());
        }
    }

    let mut schema = json!({
        "type": "object",
        "properties": properties
    });
    if !required.is_empty() {
        schema["required"] = json!(required);
    }
    schema
}

fn build_output_schema(
    exit_nodes: &[&ServiceNode],
    api_cache: &HashMap<String, Option<PluginApiDef>>,
    _schema_prefix: &str,
) -> Value {
    if exit_nodes.is_empty() {
        return json!({
            "type": "object",
            "description": "未找到出口节点"
        });
    }

    if exit_nodes.len() == 1 {
        let node = exit_nodes[0];
        let (plugin_id, function_name) = extract_node_meta(node);
        let params =
            ApiDocGenerator::resolve_parameters(&plugin_id, &function_name, false, api_cache);
        return params_to_object_schema(&params);
    }

    let mut branch_schemas = Vec::new();
    for (idx, node) in exit_nodes.iter().enumerate() {
        let (plugin_id, function_name) = extract_node_meta(node);
        let params =
            ApiDocGenerator::resolve_parameters(&plugin_id, &function_name, false, api_cache);

        let branch_name = node
            .data
            .as_ref()
            .map(|d| d.name.clone())
            .unwrap_or_else(|| format!("分支{}", idx + 1));

        let mut branch_schema = params_to_object_schema(&params);
        branch_schema["description"] = json!(branch_name);
        branch_schemas.push(branch_schema);
    }

    json!({
        "oneOf": branch_schemas,
        "description": "输出取决于运行时分支选择"
    })
}

fn build_response_data_schema(_schema_prefix: &str, output_schema: &Value) -> Value {
    json!({
        "type": "object",
        "properties": {
            "success": { "type": "boolean", "description": "是否成功" },
            "output": output_schema.clone(),
            "steps": {
                "type": "array",
                "description": "各步骤执行记录（include_steps=true 时返回）",
                "items": {
                    "type": "object",
                    "properties": {
                        "node_id": { "type": "string" },
                        "node_name": { "type": "string" },
                        "node_type": { "type": "string" },
                        "status": { "type": "string", "enum": ["Success", "Failed", "Skipped", "DebugPaused"] },
                        "output": { "type": "object" },
                        "elapsed_us": { "type": "integer" },
                        "error": { "type": "string" }
                    }
                }
            },
            "total_elapsed_us": { "type": "integer", "description": "总耗时(微秒)" },
            "error": {
                "type": "object",
                "nullable": true,
                "properties": {
                    "message": { "type": "string" }
                }
            },
            "debug_triggered": { "type": "boolean" }
        }
    })
}

fn param_doc_to_openapi_schema(param: &ParameterDoc) -> Value {
    let mut schema = match param.param_type.as_str() {
        "string" => json!({ "type": "string" }),
        "integer" | "i32" | "i64" | "u32" | "u64" => json!({ "type": "integer" }),
        "number" | "f32" | "f64" => json!({ "type": "number" }),
        "boolean" | "bool" => json!({ "type": "boolean" }),
        "array" => json!({ "type": "array", "items": { "type": "object" } }),
        _ => json!({ "type": "object" }),
    };

    if !param.description.is_empty() {
        schema["description"] = json!(param.description);
    }

    if let Some(props) = &param.properties
        && !props.is_empty()
    {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        for p in props {
            properties.insert(p.name.clone(), param_doc_to_openapi_schema(p));
            if p.required.unwrap_or(false) {
                required.push(p.name.clone());
            }
        }
        schema["type"] = json!("object");
        schema["properties"] = json!(properties);
        if !required.is_empty() {
            schema["required"] = json!(required);
        }
    }

    schema
}

fn params_to_object_schema(params: &[ParameterDoc]) -> Value {
    let params = unwrap_output_params(params);

    if params.is_empty() {
        return json!({ "type": "object", "description": "无特定输出结构" });
    }

    let mut properties = serde_json::Map::new();
    for param in params {
        properties.insert(param.name.clone(), param_doc_to_openapi_schema(param));
    }

    json!({
        "type": "object",
        "properties": properties
    })
}
