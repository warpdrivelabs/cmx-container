//! agent 工具 schema（与 Node `agentTools.js` 一致）。

use serde_json::{json, Value};

/// 8 个工具的公开 schema（capabilities 返回）。
pub fn public_tool_schemas() -> Value {
    json!([
        {
            "name": "search_files", "description": "搜索项目文件内容", "requiresApproval": false,
            "inputSchema": { "type": "object", "properties": {
                "query": { "type": "string", "description": "搜索关键词或文件名" },
                "limit": { "type": "number", "description": "最多返回条数" }
            }, "required": ["query"] }
        },
        {
            "name": "read_file", "description": "读取项目内指定文件", "requiresApproval": false,
            "inputSchema": { "type": "object", "properties": {
                "path": { "type": "string", "description": "相对 CMXPortalManager 根目录的文件路径" }
            }, "required": ["path"] }
        },
        {
            "name": "list_definitions", "description": "列出定义中心元数据文件摘要", "requiresApproval": false,
            "inputSchema": { "type": "object", "properties": {
                "kind": { "type": "string", "enum": ["DCT", "DOC", "BASE"] },
                "domain": { "type": "string" }, "module": { "type": "string" }, "limit": { "type": "number" }
            } }
        },
        {
            "name": "list_html_pages", "description": "列出自定义 HTML 页面/设计系统页面摘要", "requiresApproval": false,
            "inputSchema": { "type": "object", "properties": {
                "domain": { "type": "string" }, "app": { "type": "string" }, "module": { "type": "string" },
                "page": { "type": "number", "description": "页码，默认 1" },
                "pageSize": { "type": "number", "description": "每页数量，默认 20，最大 50" }
            } }
        },
        {
            "name": "read_html_page", "description": "读取指定自定义 HTML 页面源码", "requiresApproval": false,
            "inputSchema": { "type": "object", "properties": {
                "id": { "type": "string", "description": "HTML 页面 ID，如 fi.cmxfico.gl.demo-page 或 welcome" }
            }, "required": ["id"] }
        },
        {
            "name": "validate_metadata", "description": "检查元数据 JSON 是否可解析", "requiresApproval": false,
            "inputSchema": { "type": "object", "properties": {
                "path": { "type": "string", "description": "可选，指定要检查的文件或目录" }
            } }
        },
        {
            "name": "run_command", "description": "审批后执行预置安全命令", "requiresApproval": true,
            "inputSchema": { "type": "object", "properties": {
                "command": { "type": "string", "enum": ["npm"] },
                "args": { "type": "array", "items": { "type": "string" }, "description": "当前白名单支持 lint/build" },
                "timeoutMs": { "type": "number" }
            }, "required": ["command", "args"] }
        },
        {
            "name": "apply_json_patch", "description": "审批后应用 JSON Pointer 补丁", "requiresApproval": true,
            "inputSchema": { "type": "object", "properties": {
                "path": { "type": "string" },
                "pointer": { "type": "string", "description": "JSON Pointer，如 /baseMeta/status" },
                "value": { "description": "要写入的 JSON 值" }
            }, "required": ["path", "pointer", "value"] }
        },
        {
            "name": "apply_text_replace", "description": "审批后应用文本替换补丁", "requiresApproval": true,
            "inputSchema": { "type": "object", "properties": {
                "path": { "type": "string" }, "oldText": { "type": "string" }, "newText": { "type": "string" },
                "occurrence": { "type": "string", "enum": ["first", "all"] }
            }, "required": ["path", "oldText", "newText"] }
        }
    ])
}
