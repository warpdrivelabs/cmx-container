//! Menu Service
//!
//! 封装菜单的 CRUD 与列表/分页查询逻辑。
//! create 计算标准分级字段(leaf/depth/parent_code/id_path/code_path)后写入，
//! 并更新父节点 leaf=0。

use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::crud::GenericCrudService;
use cmx_database::DatabaseManager;
use modql::filter::ListOptions;
use serde_json::Value;
use tracing::{debug, instrument};

use crate::error::{BizError, Result};
use crate::menu::{MenuBmc, MenuFilter, MenuForCreate, MenuForUpdate};

/// 创建菜单时计算出的分级字段
struct TreeFields {
    parent_code: Option<String>,
    depth: i32,
    id_path: String,
    code_path: String,
}

/// 菜单服务
pub struct MenuService;

impl MenuService {
    /// 创建菜单：计算标准分级字段(leaf/depth/parent_code/id_path/code_path)后事务内写入，
    /// 并更新父节点 leaf=0。
    ///
    /// # Arguments
    /// * `txn_id` - 外部事务 ID。传 Some 时纳入调用方事务(不再自开自提交);
    ///   传 None 时内部自开事务并提交(向后兼容)
    ///
    /// # Errors
    /// 父菜单不存在、数据库写入失败时返回错误
    #[instrument(skip(mm, data))]
    pub async fn create(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        data: MenuForCreate,
    ) -> Result<DataSet> {
        match txn_id {
            // 外部事务:直接复用,不自开不提交
            Some(t) => Self::create_inner(mm, db_id, t, data).await,
            // 无外部事务:内部自开事务并提交(原行为)
            None => {
                let txn_ctx = mm.get_transaction_context();
                let guard = txn_ctx
                    .begin_with_guard(db_id)
                    .await
                    .map_err(|e| BizError::business(format!("开启事务失败: {e}")))?;
                let txn = guard.txn_id().to_string();
                let dataset = Self::create_inner(mm, db_id, &txn, data).await?;
                guard
                    .commit()
                    .await
                    .map_err(|e| BizError::business(format!("事务提交失败: {e}")))?;
                Ok(dataset)
            }
        }
    }

    /// create 的核心写入逻辑(INSERT 新节点 + 更新父 leaf=0),不管理事务。
    async fn create_inner(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        data: MenuForCreate,
    ) -> Result<DataSet> {
        // 计算分级字段
        let tree = Self::compute_tree_fields(mm, db_id, &data).await?;

        let id = uuid::Uuid::new_v4().to_string();
        let definition_str = data
            .definition
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());
        let sql = "INSERT INTO cmx_menu \
                   (id, code, name, description, path, icon, component, sort_order, visible, \
                    domain_code, application_code, module_code, definition, ext_attributes, status, \
                    leaf, depth, parent_id, parent_code, id_path, code_path, archived) \
                   VALUES ($1, $2, $3, NULL, $4, $5, $6, $7, $8, \
                           $9, $10, $11, $12::jsonb, $13, 1, \
                           1, $14, $15, $16, $17, $18, 0) \
                   RETURNING *";
        let params: Vec<DataValue> = vec![
            DataValue::String(id),
            DataValue::String(data.code.clone()),
            DataValue::String(data.name.clone()),
            data.path.clone().into(),
            data.icon.clone().into(),
            data.component.clone().into(),
            DataValue::Int(data.sort_order as i64),
            DataValue::Int(data.visible as i64),
            DataValue::String(data.domain_code.clone()),
            DataValue::String(data.application_code.clone()),
            DataValue::String(data.module_code.clone()),
            definition_str.into(),
            data.ext_attributes.clone().into(),
            DataValue::Int(tree.depth as i64),
            data.parent_id.clone().into(),
            tree.parent_code.clone().into(),
            DataValue::String(tree.id_path),
            DataValue::String(tree.code_path),
        ];
        let dataset = mm
            .query_sql_with_datavalues(db_id, Some(txn_id), sql, params, "create_menu")
            .await
            .map_err(|e| BizError::business(format!("新增菜单失败: {e}")))?;

        // 父节点 leaf = 0(有子节点后不再是叶子)
        if let Some(pid) = &data.parent_id {
            let upd_sql = "UPDATE cmx_menu SET leaf = 0 WHERE id = $1";
            let _ = mm
                .execute_sql_with_datavalues(
                    db_id,
                    Some(txn_id),
                    upd_sql,
                    vec![DataValue::String(pid.clone())],
                )
                .await;
        }

        Ok(dataset)
    }

    /// 根据父节点计算分级字段
    ///
    /// 根节点: depth=1, id_path=/{id}, code_path=/{code}
    /// 子节点: depth=父+1, id_path=父id_path/{id}, code_path=父code_path/{code}
    async fn compute_tree_fields(
        mm: &DatabaseManager,
        db_id: &str,
        data: &MenuForCreate,
    ) -> Result<TreeFields> {
        let new_id = uuid::Uuid::new_v4().to_string();
        match &data.parent_id {
            Some(pid) => {
                // 查父节点,取 id_path/code_path/depth/code
                let parent = GenericCrudService::<MenuBmc>::get(
                    mm,
                    db_id,
                    None,
                    Value::String(pid.clone()),
                )
                .await?;
                let _row = parent.iter().next().ok_or_else(|| {
                    BizError::business(format!("父菜单不存在: {pid}"))
                })?;
                let p_json = serde_json::to_value(&parent)?;
                let row_json = p_json
                    .get("rows")
                    .and_then(|r| r.as_array())
                    .and_then(|rows| rows.first())
                    .ok_or_else(|| BizError::business("父菜单查询结果为空"))?;

                let p_id_path = row_json
                    .get("id_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let p_code_path = row_json
                    .get("code_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let p_depth = row_json
                    .get("depth")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1) as i32;
                let p_code = row_json
                    .get("code")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                Ok(TreeFields {
                    parent_code: p_code,
                    depth: p_depth + 1,
                    id_path: format!("{p_id_path}/{new_id}"),
                    code_path: format!("{p_code_path}/{}", data.code),
                })
            }
            None => Ok(TreeFields {
                parent_code: None,
                depth: 1,
                id_path: format!("/{new_id}"),
                code_path: format!("/{}", data.code),
            }),
        }
    }

    /// 查询单个菜单
    pub async fn get(mm: &DatabaseManager, db_id: &str, id: &str) -> Result<DataSet> {
        GenericCrudService::<MenuBmc>::get(mm, db_id, None, Value::String(id.to_string()))
            .await
            .map_err(Into::into)
    }

    /// 更新菜单
    pub async fn update(
        mm: &DatabaseManager,
        db_id: &str,
        id: Value,
        data: MenuForUpdate,
    ) -> Result<DataSet> {
        GenericCrudService::<MenuBmc>::update(mm, db_id, None, id, data)
            .await
            .map_err(Into::into)
    }

    /// 删除菜单
    pub async fn delete(mm: &DatabaseManager, db_id: &str, ids: Vec<Value>) -> Result<DataSet> {
        GenericCrudService::<MenuBmc>::delete(mm, db_id, None, ids)
            .await
            .map_err(Into::into)
    }

    /// 按 code 删除菜单(幂等安装用,不存在时静默成功)
    ///
    /// # Arguments
    /// * `txn_id` - 外部事务 ID(传 Some 时纳入调用方事务;传 None 时自动提交)
    ///
    /// # Errors
    /// 数据库执行失败时返回错误
    pub async fn delete_by_code(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        code: &str,
    ) -> Result<()> {
        use cmx_core::model::cell::DataValue;
        mm.execute_sql_with_datavalues(
            db_id,
            txn_id,
            "DELETE FROM cmx_menu WHERE code = $1",
            vec![DataValue::String(code.to_string())],
        )
        .await
        .map_err(|e| crate::error::BizError::business(format!("按 code 删除菜单失败: {e}")))?;
        Ok(())
    }

    /// 按模块编码查询根菜单定义列表(供模块导出复用,返回结构化 MenuDefinition)。
    ///
    /// 只查询根菜单(parent_id IS NULL),其 definition 含完整菜单树。
    /// 封装原 module_export 的内联 SQL,消除导出与导入的不对称。
    ///
    /// # Errors
    /// 数据库查询失败时返回错误
    pub async fn list_by_module(
        mm: &DatabaseManager,
        db_id: &str,
        module_code: &str,
    ) -> Result<Vec<cmx_core::model::module::MenuDefinition>> {
        let sql = "SELECT code, name, definition, domain_code, application_code, module_code \
                   FROM cmx_menu WHERE module_code = $1 AND parent_id IS NULL AND archived = 0";
        let ds = mm
            .query_sql_with_datavalues(
                db_id,
                None,
                sql,
                vec![DataValue::String(module_code.to_string())],
                "menu_list_by_module",
            )
            .await
            .map_err(|e| BizError::internal(format!("按模块查询根菜单失败: {e}")))?;
        let schema = ds.schema.as_ref();
        let mut result = Vec::new();
        for row in ds.iter() {
            let code = row.get_by_name_as::<String>(schema, "code").unwrap_or_default();
            let name = row.get_by_name_as::<String>(schema, "name").unwrap_or_default();
            // definition 是 JSONB,统一归一化
            let definition = row
                .get_by_name_as::<serde_json::Value>(schema, "definition")
                .or_else(|| {
                    row.get_by_name_as::<String>(schema, "definition")
                        .and_then(|s| serde_json::from_str(&s).ok())
                })
                .map(cmx_utils::json::coerce_to_object)
                .unwrap_or_default();
            let domain_code = row
                .get_by_name_as::<String>(schema, "domain_code")
                .unwrap_or_default();
            let application_code = row
                .get_by_name_as::<String>(schema, "application_code")
                .unwrap_or_default();
            let module_code = row
                .get_by_name_as::<String>(schema, "module_code")
                .unwrap_or_default();
            result.push(cmx_core::model::module::MenuDefinition {
                code,
                name,
                definition,
                domain_code,
                application_code,
                module_code,
            });
        }
        Ok(result)
    }

    /// 列表查询
    pub async fn list(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<MenuFilter>>,
        list_options: Option<ListOptions>,
    ) -> Result<DataSet> {
        GenericCrudService::<MenuBmc, MenuFilter>::list(mm, db_id, None, filters, list_options)
            .await
            .map_err(Into::into)
    }

    /// 分页查询
    pub async fn page(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<MenuFilter>>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        GenericCrudService::<MenuBmc, MenuFilter>::page(mm, db_id, None, filters, list_options)
            .await
            .map_err(Into::into)
    }

    /// 查询菜单树(按域/应用/模块过滤,组装为树形结构)
    ///
    /// 参照 DomainService::get_tree / PermissionService::get_permission_tree 模式:
    /// 查全量扁平数据 → 转 MenuTreeNodeData → TreeNode::from_list 组装。
    ///
    /// # Errors
    /// 数据库查询失败时返回错误
    pub async fn get_tree(
        mm: &DatabaseManager,
        db_id: &str,
        domain_code: Option<&str>,
        application_code: Option<&str>,
        module_code: Option<&str>,
    ) -> Result<Vec<cmx_api_types::TreeNode<crate::menu::MenuTreeNodeData>>> {
        debug!("{:<12} - MenuService::get_tree", "SERVICE");

        // 动态构建 WHERE(可选过滤)
        let mut conditions: Vec<String> = vec!["archived = 0".to_string()];
        let mut params: Vec<DataValue> = Vec::new();
        let mut idx = 1;
        if let Some(dc) = domain_code {
            conditions.push(format!("domain_code = ${idx}"));
            params.push(DataValue::String(dc.to_string()));
            idx += 1;
        }
        if let Some(ac) = application_code {
            conditions.push(format!("application_code = ${idx}"));
            params.push(DataValue::String(ac.to_string()));
            idx += 1;
        }
        if let Some(mc) = module_code {
            conditions.push(format!("module_code = ${idx}"));
            params.push(DataValue::String(mc.to_string()));
        }
        let where_clause = conditions.join(" AND ");

        let sql = format!(
            "SELECT id, code, name, description, path, icon, component, sort_order, visible, \
             depth, parent_code, domain_code, application_code, module_code, definition, ext_attributes \
             FROM cmx_menu WHERE {where_clause} ORDER BY sort_order"
        );

        let dataset = mm
            .query_sql_with_datavalues(db_id, None, &sql, params, "menu_tree")
            .await
            .map_err(|e| BizError::internal(format!("查询菜单树形数据失败: {e}")))?;

        let items: Vec<crate::menu::MenuTreeNodeData> = dataset
            .iter()
            .map(|row| Self::row_to_tree_node(row, &dataset.schema))
            .collect::<Result<Vec<_>>>()?;

        Ok(cmx_api_types::TreeNode::from_list(items))
    }

    /// 将 DataSet 一行转换为 MenuTreeNodeData
    fn row_to_tree_node(
        row: &cmx_core::model::data::dataset::Row,
        schema: &cmx_core::model::data::dataset::Schema,
    ) -> Result<crate::menu::MenuTreeNodeData> {
        let get_str = |name: &str| -> Option<String> { row.get_by_name_as(schema, name) };
        let get_i32 = |name: &str| -> i32 { row.get_by_name_as::<i32>(schema, name).unwrap_or(0) };
        let _ = get_str;
        let _ = get_i32;
        Ok(crate::menu::MenuTreeNodeData {
            id: get_str("id").unwrap_or_default(),
            code: get_str("code").unwrap_or_default(),
            name: get_str("name").unwrap_or_default(),
            parent_code: get_str("parent_code"),
            description: get_str("description"),
            path: get_str("path"),
            icon: get_str("icon"),
            component: get_str("component"),
            sort_order: get_i32("sort_order"),
            visible: get_i32("visible"),
            depth: get_i32("depth"),
            domain_code: get_str("domain_code").unwrap_or_default(),
            application_code: get_str("application_code").unwrap_or_default(),
            module_code: get_str("module_code").unwrap_or_default(),
            definition: row
                .get_by_name_as::<serde_json::Value>(schema, "definition")
                .or_else(|| {
                    get_str("definition")
                        .and_then(|s| serde_json::from_str(&s).ok())
                }),
            ext_attributes: get_str("ext_attributes"),
        })
    }
}
