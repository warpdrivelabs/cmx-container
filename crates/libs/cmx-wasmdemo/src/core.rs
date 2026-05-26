use crate::models::*;
use crate::host_traits::HostFunctions;

/// 插件核心实现。
///
/// 包含所有功能函数的业务逻辑实现，通过 `HostFunctions` trait
/// 调用宿主提供的各种能力。
pub struct PluginCore<H: HostFunctions> {
    host: H,
}

impl<H: HostFunctions> PluginCore<H> {
    /// 创建一个新的插件核心实例。
    ///
    /// # Arguments
    ///
    /// * `host` - 宿主功能实现，用于执行日志、缓存、数据库等操作。
    pub fn new(host: H) -> Self {
        Self { host }
    }

    /// 统计输入字符串中的元音字母数量。
    ///
    /// # Arguments
    ///
    /// * `input` - 函数输入，包含待统计的字符串。
    ///
    /// # Returns
    ///
    /// 成功时返回包含统计结果的 `FunctionOutput`。
    /// 失败时返回错误信息字符串。
    pub fn count_vowels(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let input_str = input.input.as_str().unwrap_or_default();
        let vowels = ['a', 'e', 'i', 'o', 'u', 'A', 'E', 'I', 'O', 'U'];
        let count = input_str.chars().filter(|c| vowels.contains(c)).count();
        let result = serde_json::json!({
            "count": count,
            "total": count,
            "input": input_str,
        });
        Ok(FunctionOutput::from_json(result))
    }

    /// 记录不同级别的日志信息。
    ///
    /// 调用宿主的日志函数，记录 info、error、debug、warn 四个级别的日志。
    ///
    /// # Arguments
    ///
    /// * `input` - 函数输入。
    ///
    /// # Returns
    ///
    /// 成功时返回包含日志记录结果的 `FunctionOutput`。
    /// 失败时返回错误信息字符串。
    pub fn demo_log(&self, _input: &FunctionInput) -> Result<FunctionOutput, String> {
        self.host.log_info("Hello from WASM demo!")?;
        self.host.log_error("This is an error from WASM demo!")?;
        self.host.log_debug("This is a debug message!")?;
        self.host.log_warn("This is a warning!")?;
        let response = DemoResponse {
            message: "日志记录完成".to_string(),
            total: 4,
        };
        Ok(FunctionOutput::from_json(serde_json::to_value(&response).map_err(|e| e.to_string())?))
    }

    /// 执行缓存的写入和读取操作。
    ///
    /// 调用宿主的缓存接口，将数据写入缓存后再读取验证。
    ///
    /// # Arguments
    ///
    /// * `input` - 函数输入，包含 `DemoRequest` 格式的缓存键名和计数值。
    ///
    /// # Returns
    ///
    /// 成功时返回包含缓存操作结果的 `FunctionOutput`。
    /// 失败时返回错误信息字符串。
    pub fn demo_cache(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: DemoRequest = serde_json::from_value(input.input.clone())
            .unwrap_or(DemoRequest { name: "default".to_string(), count: 0 });
        let _set_response = self.host.cache_set(
            &request.name,
            serde_json::Value::String(request.count.to_string()),
            Some(3600),
        )?;
        self.host.log_info(&format!("缓存写入结果: name={}", request.name))?;
        let get_response = self.host.cache_get(&request.name)?;
        self.host.log_info(&format!("缓存读取结果: {:?}", get_response))?;
        let response = DemoResponse {
            message: format!("缓存操作成功: {:?}", get_response),
            total: request.count,
        };
        Ok(FunctionOutput::from_json(serde_json::to_value(&response).map_err(|e| e.to_string())?))
    }

    /// 执行数据库查询操作。
    ///
    /// 调用宿主的数据接接口，执行一条 SELECT 查询。
    ///
    /// # Arguments
    ///
    /// * `input` - 函数输入，包含 `DemoRequest` 格式的查询参数。
    ///
    /// # Returns
    ///
    /// 成功时返回包含查询结果的 `FunctionOutput`。
    /// 失败时返回错误信息字符串。
    pub fn demo_database(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: DemoRequest = serde_json::from_value(input.input.clone())
            .unwrap_or(DemoRequest { name: "default".to_string(), count: 0 });
        let query_request = DbRequest {
            sql: format!("SELECT '{}' as name, {} as count", request.name, request.count),
            params: None,
            dataset_id: None,
            db_id: None,
            txn_id: None,
        };
        let db_response = self.host.db_query(query_request)?;
        self.host.log_info(&format!("数据库查询结果: {:?}", db_response))?;
        let response = DemoResponse {
            message: format!("数据库查询成功: {:?}", db_response),
            total: request.count,
        };
        Ok(FunctionOutput::from_json(serde_json::to_value(&response).map_err(|e| e.to_string())?))
    }

    /// 调用指定插件。
    ///
    /// 通过宿主调用另一个指定的插件函数。
    ///
    /// # Arguments
    ///
    /// * `input` - 函数输入，包含 `DemoRequest` 格式的请求参数。
    ///
    /// # Returns
    ///
    /// 成功时返回包含调用结果的 `FunctionOutput`。
    /// 失败时返回包含错误信息的 `FunctionOutput`。
    pub fn demo_call_plugin(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: DemoRequest = serde_json::from_value(input.input.clone())
            .unwrap_or(DemoRequest { name: "default".to_string(), count: 0 });
        let plugin_request = PluginFunRequest {
            plugin_id: "target-plugin".to_string(),
            function_name: "some_function".to_string(),
            input: serde_json::json!({"name": request.name, "count": request.count}),
            initial_input: None,
            debug: Some(false),
        };
        match self.host.call_plugin(plugin_request) {
            Ok(result) => {
                self.host.log_info(&format!("调用指定插件成功: {:?}", result))?;
                let response = DemoResponse {
                    message: format!("调用成功: {:?}", result),
                    total: request.count,
                };
                Ok(FunctionOutput::from_json(serde_json::to_value(&response).map_err(|e| e.to_string())?))
            }
            Err(e) => {
                self.host.log_error(&format!("调用指定插件失败: {}", e))?;
                let response = DemoResponse {
                    message: format!("调用失败: {}", e),
                    total: 0,
                };
                Ok(FunctionOutput::from_json(serde_json::to_value(&response).map_err(|e| e.to_string())?))
            }
        }
    }

    /// 调用服务编排。
    ///
    /// 通过宿主调用服务编排接口。
    ///
    /// # Arguments
    ///
    /// * `input` - 函数输入，包含 `DemoRequest` 格式的请求参数。
    ///
    /// # Returns
    ///
    /// 成功时返回包含调用结果的 `FunctionOutput`。
    /// 失败时返回包含错误信息的 `FunctionOutput`。
    pub fn demo_call_service_by_key(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: DemoRequest = serde_json::from_value(input.input.clone())
            .unwrap_or(DemoRequest { name: "default".to_string(), count: 0 });
        let service_request = CallServiceRequest {
            service_key: "my-domain/my-service".to_string(),
            input: serde_json::json!({"name": request.name, "count": request.count}),
            include_steps: Some(false),
            debug: Some(false),
            debug_node_id: None,
            debug_params: None,
        };
        match self.host.call_service_by_key(service_request) {
            Ok(result) => {
                self.host.log_info(&format!("调用服务编排成功: {:?}", result))?;
                let response = DemoResponse {
                    message: format!("服务执行成功: {:?}", result),
                    total: request.count,
                };
                Ok(FunctionOutput::from_json(serde_json::to_value(&response).map_err(|e| e.to_string())?))
            }
            Err(e) => {
                self.host.log_error(&format!("调用服务编排失败: {}", e))?;
                let response = DemoResponse {
                    message: format!("服务执行失败: {}", e),
                    total: 0,
                };
                Ok(FunctionOutput::from_json(serde_json::to_value(&response).map_err(|e| e.to_string())?))
            }
        }
    }

    /// 执行多项功能测试。
    ///
    /// 依次执行日志、缓存、数据库等功能的测试。
    ///
    /// # Arguments
    ///
    /// * `input` - 函数输入，包含 `DemoRequest` 格式的测试参数。
    ///
    /// # Returns
    ///
    /// 返回包含各项测试结果的 `FunctionOutput`。
    pub fn run_all_demos(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: DemoRequest = serde_json::from_value(input.input.clone())
            .unwrap_or(DemoRequest { name: "default".to_string(), count: 0 });
        let mut results = Vec::new();

        match self.host.log_info("测试日志") {
            Ok(_) => results.push("日志测试: 成功".to_string()),
            Err(e) => results.push(format!("日志测试失败: {}", e)),
        }

        match self.host.cache_set(&request.name, serde_json::Value::String(request.count.to_string()), Some(3600)) {
            Ok(_) => results.push("缓存写入测试: 成功".to_string()),
            Err(e) => results.push(format!("缓存写入测试失败: {}", e)),
        }

        match self.host.cache_get(&request.name) {
            Ok(resp) => results.push(format!("缓存读取测试: {:?}", resp)),
            Err(e) => results.push(format!("缓存读取测试失败: {}", e)),
        }

        let query_request = DbRequest {
            sql: "SELECT * from cmx_meta_table_define_version".to_string(),
            params: None,
            dataset_id: None,
            db_id: None,
            txn_id: None,
        };
        match self.host.db_query(query_request) {
            Ok(resp) => results.push(format!("数据库测试: {:?}", resp)),
            Err(e) => results.push(format!("数据库测试失败: {}", e)),
        }

        Ok(FunctionOutput::from_json(serde_json::to_value(&results).map_err(|e| e.to_string())?))
    }

    /// 路由判断函数。
    ///
    /// 根据输入的 route 字段决定返回哪个分支标识。
    ///
    /// # Arguments
    ///
    /// * `input` - 函数输入，包含 `RouteInput` 格式的路由参数。
    ///
    /// # Returns
    ///
    /// 返回 "1"、"2"、"3" 或 "4"，对应四个分支。
    pub fn route_check(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let route_input: RouteInput = serde_json::from_value(input.input.clone())
            .unwrap_or(RouteInput { route: "1".to_string() });
        let route = route_input.route.trim();
        let result = match route {
            "1" => "1",
            "2" => "2",
            "3" => "3",
            "4" => "4",
            _ => "1",
        };
        self.host.log_info(&format!("路由判断: route={}, 返回分支={}", route, result))?;
        Ok(FunctionOutput::from_json(serde_json::to_value(result).map_err(|e| e.to_string())?))
    }

    /// 分支1处理函数。
    ///
    /// 处理分支1的业务逻辑。
    ///
    /// # Arguments
    ///
    /// * `input` - 函数输入，包含前序步骤的输出和初始入参。
    ///
    /// # Returns
    ///
    /// 返回包含分支标识和处理结果的 `FunctionOutput`。
    pub fn branch_1_process(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        self.host.log_info("执行分支1处理")?;
        let result = serde_json::json!({
            "branch": "1",
            "message": "分支1处理完成",
            "input": input.input,
            "initial_input": input.context.initial_input,
        });
        Ok(FunctionOutput::from_json(result))
    }

    /// 分支2处理函数。
    ///
    /// 处理分支2的业务逻辑。
    ///
    /// # Arguments
    ///
    /// * `input` - 函数输入，包含前序步骤的输出和初始入参。
    ///
    /// # Returns
    ///
    /// 返回包含分支标识和处理结果的 `FunctionOutput`。
    pub fn branch_2_process(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        self.host.log_info("执行分支2处理")?;
        let result = serde_json::json!({
            "branch": "2",
            "message": "分支2处理完成",
            "input": input.input,
            "initial_input": input.context.initial_input,
        });
        Ok(FunctionOutput::from_json(result))
    }

    /// 分支3处理函数。
    ///
    /// 处理分支3的业务逻辑。
    ///
    /// # Arguments
    ///
    /// * `input` - 函数输入，包含前序步骤的输出和初始入参。
    ///
    /// # Returns
    ///
    /// 返回包含分支标识和处理结果的 `FunctionOutput`。
    pub fn branch_3_process(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        self.host.log_info("执行分支3处理")?;
        let result = serde_json::json!({
            "branch": "3",
            "message": "分支3处理完成",
            "input": input.input,
            "initial_input": input.context.initial_input,
        });
        Ok(FunctionOutput::from_json(result))
    }

    /// 合并结果函数。
    ///
    /// 合并各分支的处理结果，从上下文获取各分支的输出并合并。
    ///
    /// # Arguments
    ///
    /// * `input` - 函数输入，包含前序步骤的输出和各步骤的输出缓存。
    ///
    /// # Returns
    ///
    /// 返回包含合并结果的 `FunctionOutput`。
    pub fn merge_result(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        self.host.log_info("执行合并结果处理")?;
        let branch_output = input.context.get_step_output("branch_1_func")
            .or_else(|| input.context.get_step_output("branch_2_func"))
            .or_else(|| input.context.get_step_output("branch_3_func"))
            .cloned()
            .unwrap_or_else(|| input.input.clone());
        let result = serde_json::json!({
            "merged": true,
            "branch_output": branch_output,
            "message": "结果合并完成",
        });
        Ok(FunctionOutput::from_json(result))
    }

    /// 事务插入函数。
    ///
    /// 在事务中执行插入操作，通过上下文获取事务ID确保在同一事务中执行。
    ///
    /// # Arguments
    ///
    /// * `input` - 函数输入，包含 `InsertData` 格式的插入数据。
    ///
    /// # Returns
    ///
    /// 返回包含操作结果的 `FunctionOutput`。
    pub fn tx_insert(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let txn_id = input.context.txn_id.clone();
        self.host.log_info(&format!("执行事务插入, txn_id={:?}", txn_id))?;
        let insert_data: InsertData = serde_json::from_value(input.input.clone())
            .unwrap_or(InsertData { table: "test_table".to_string(), name: "test".to_string(), value: 1 });
        let sql = format!(
            "INSERT INTO {} (name, value) VALUES ('{}', {})",
            insert_data.table, insert_data.name, insert_data.value
        );
        let query_request = DbRequest {
            sql,
            params: None,
            dataset_id: None,
            db_id: None,
            txn_id: txn_id.clone(),
        };
        let db_response = self.host.db_execute(query_request)?;
        let result = serde_json::json!({
            "operation": "insert",
            "txn_id": txn_id,
            "table": insert_data.table,
            "affected_rows": db_response.affected_rows,
            "message": "插入完成",
        });
        Ok(FunctionOutput::from_json(result))
    }

    /// 事务更新函数。
    ///
    /// 在事务中执行更新操作，通过上下文获取事务ID确保在同一事务中执行。
    ///
    /// # Arguments
    ///
    /// * `input` - 函数输入，包含 `UpdateData` 格式的更新数据。
    ///
    /// # Returns
    ///
    /// 返回包含操作结果的 `FunctionOutput`。
    pub fn tx_update(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let txn_id = input.context.txn_id.clone();
        self.host.log_info(&format!("执行事务更新, txn_id={:?}", txn_id))?;
        let update_data: UpdateData = serde_json::from_value(input.input.clone())
            .unwrap_or(UpdateData { table: "test_table".to_string(), name: "test".to_string(), value: 2 });
        let sql = format!(
            "UPDATE {} SET value = {} WHERE name = '{}'",
            update_data.table, update_data.value, update_data.name
        );
        let query_request = DbRequest {
            sql,
            params: None,
            dataset_id: None,
            db_id: None,
            txn_id: txn_id.clone(),
        };
        let db_response = self.host.db_execute(query_request)?;
        let result = serde_json::json!({
            "operation": "update",
            "txn_id": txn_id,
            "table": update_data.table,
            "affected_rows": db_response.affected_rows,
            "message": "更新完成",
        });
        Ok(FunctionOutput::from_json(result))
    }

    /// 事务查询函数。
    ///
    /// 在事务中执行查询操作，通过上下文获取事务ID确保在同一事务中执行。
    ///
    /// # Arguments
    ///
    /// * `input` - 函数输入，包含 `QueryData` 格式的查询条件。
    ///
    /// # Returns
    ///
    /// 返回包含查询结果的 `FunctionOutput`。
    pub fn tx_query(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let txn_id = input.context.txn_id.clone();
        self.host.log_info(&format!("执行事务查询, txn_id={:?}", txn_id))?;
        let query_data: QueryData = serde_json::from_value(input.input.clone())
            .unwrap_or(QueryData { table: "test_table".to_string(), name: "test".to_string() });
        let sql = format!(
            "SELECT * FROM {} WHERE name = '{}'",
            query_data.table, query_data.name
        );
        let query_request = DbRequest {
            sql,
            params: None,
            dataset_id: Some("test_table".to_string()),
            db_id: None,
            txn_id: txn_id.clone(),
        };
        let db_response = self.host.db_query(query_request)?;
        let result = serde_json::json!({
            "operation": "query",
            "txn_id": txn_id,
            "table": query_data.table,
            "success": db_response.success,
            "dataset": db_response.dataset,
            "message": "查询完成",
        });
        Ok(FunctionOutput::from_json(result))
    }

    /// 事务删除函数。
    ///
    /// 在事务中执行删除操作，通过上下文获取事务ID确保在同一事务中执行。
    ///
    /// # Arguments
    ///
    /// * `input` - 函数输入，包含 `DeleteData` 格式的删除条件。
    ///
    /// # Returns
    ///
    /// 返回包含操作结果的 `FunctionOutput`。
    pub fn tx_delete(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let txn_id = input.context.txn_id.clone();
        self.host.log_info(&format!("执行事务删除, txn_id={:?}", txn_id))?;
        let delete_data: DeleteData = serde_json::from_value(input.input.clone())
            .unwrap_or(DeleteData { table: "test_table".to_string(), name: "test1".to_string() });
        let sql = format!(
            "DELETE FROM {} WHERE name = '{}'",
            delete_data.table, delete_data.name
        );
        let query_request = DbRequest {
            sql,
            params: None,
            dataset_id: None,
            db_id: None,
            txn_id: txn_id.clone(),
        };
        let db_response = self.host.db_execute(query_request)?;
        let result = serde_json::json!({
            "operation": "delete",
            "txn_id": txn_id,
            "table": delete_data.table,
            "affected_rows": db_response.affected_rows,
            "message": "删除完成",
        });
        Ok(FunctionOutput::from_json(result))
    }

    /// 最终处理函数。
    ///
    /// 执行最终处理并返回结果，整合各步骤的输出。
    ///
    /// # Arguments
    ///
    /// * `input` - 函数输入，包含前序步骤的输出和各步骤的输出缓存。
    ///
    /// # Returns
    ///
    /// 返回包含最终结果的 `FunctionOutput`。
    pub fn final_process(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        self.host.log_info("执行最终处理")?;
        let merge_output = input.context.get_step_output("merge_func")
            .cloned()
            .unwrap_or_else(|| input.input.clone());
        let tx_insert_output = input.context.get_step_output("tx_insert").cloned();
        let tx_update_output = input.context.get_step_output("tx_update").cloned();
        let tx_query_output = input.context.get_step_output("tx_query").cloned();
        let tx_delete_output = input.context.get_step_output("tx_delete").cloned();
        let result = serde_json::json!({
            "final": true,
            "merge_output": merge_output,
            "tx_insert_output": tx_insert_output,
            "tx_update_output": tx_update_output,
            "tx_query_output": tx_query_output,
            "tx_delete_output": tx_delete_output,
            "txn_id": input.context.txn_id,
            "message": "服务编排执行完成",
        });
        let _set_response = self.host.cache_set(
            "redis_key",
            serde_json::Value::String("测试redis缓存".to_string()),
            Some(3600),
        )?;
        let plugin_fun_request = PluginFunRequest {
            plugin_id: "example_plugin".to_string(),
            function_name: "count_vowels".to_string(),
            input: serde_json::Value::String("aabbccddee".to_string()),
            initial_input: None,
            debug: None,
        };
        match self.host.call_plugin(plugin_fun_request) {
            Ok(call_result) => {
                self.host.log_info(&format!("调用指定插件成功: {:?}", call_result))?;
            }
            Err(e) => {
                self.host.log_error(&format!("调用指定插件失败: {}", e))?;
            }
        };
        Ok(FunctionOutput::from_json(result))
    }
}
