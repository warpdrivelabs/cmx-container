//! 数据源初始化模块
//!
//! 负责数据源配置的持久化、查询和动态加载。

use cmx_api::handlers::sys_datasource::{
    SysDatasourceBmc, SysDatasourceFilter, SysDatasourceForCreate, SysDatasourceForUpdate,
};
use cmx_database::crud::GenericCrudService;
use cmx_database::{DatabaseManager, DbConfig, get_default_db_manager};
use cmx_utils::ConfigManager;
use modql::filter::{OpValInt64, OpValString, OpValsInt64, OpValsString};
use tracing::{info, warn};

use crate::Error;

/// 初始化数据源（主入口）
///
/// 执行以下流程：
/// 1. 从配置加载数据源配置
/// 2. 注册配置文件中的数据源连接
/// 3. 执行数据库迁移
/// 4. 持久化配置文件中的数据源到数据库
/// 5. 从数据库加载有效数据源
/// 6. 注册到内存
///
/// # Errors
///
/// * `Error::DatasourceInit` - 数据源初始化失败
pub async fn init_datasources() -> crate::Result<()> {
    info!("开始初始化数据源...");

    let config = ConfigManager::global();
    let mut configs: Vec<DbConfig> = config.get_as("databases")
        .map_err(|e| Error::DatasourceInit(format!("无法从配置管理器获取 databases 配置: {}", e)))?;

    info!("成功解析到 {} 个数据源配置", configs.len());

    // 读取本实例应用标识，为配置文件数据源统一注入域应用模块归属。
    let app_identity = crate::config::load_app_identity();
    for c in &mut configs {
        if c.domain_code.is_none() {
            c.domain_code = Some(app_identity.domain_code.clone());
        }
        if c.application_code.is_none() {
            c.application_code = Some(app_identity.application_code.clone());
        }
        if c.module_code.is_none() {
            c.module_code = Some(app_identity.module_code.clone());
        }
        if c.source_type.is_none() {
            c.source_type = Some(if c.default { "default".to_string() } else { "other".to_string() });
        }
    }

    let db_manager = get_default_db_manager();

    if let Err(e) = register_datasources(db_manager, configs.clone()).await {
        return Err(Error::DatasourceInit(format!(
            "注册配置文件数据源失败: {}", e
        )));
    }

    let default_db_id = db_manager.get_default_db_id().await;

    crate::config::init_database_migrations().await?;

    if let Err(e) = persist_datasource_configs(db_manager, &default_db_id, &app_identity, configs).await {
        return Err(Error::DatasourceInit(format!(
            "持久化数据源配置失败: {}", e
        )));
    }

    let active_datasources = load_active_datasources(db_manager, &default_db_id, &app_identity).await?;

    // 过滤掉配置文件中的数据源（只保留数据库中的数据源）
    let mut filtered_datasources = Vec::new();
    for config in active_datasources {
        if db_manager.get_db(config.db_id.as_str()).await.is_err() {
            filtered_datasources.push(config);
        }
    }

    info!("从数据库加载到 {} 个有效数据源", filtered_datasources.len());

    if let Err(e) = register_datasources(db_manager, filtered_datasources).await {
        warn!("注册数据库中的数据源失败: {}", e);
    }

    info!("数据源初始化完成");
    Ok(())
}

/// 持久化数据源配置到数据库
///
/// 检查每个 db_id 是否已存在于数据库中：
/// - 存在则用配置文件信息更新数据库记录
/// - 不存在则插入新记录
///
/// 使用 UPSERT 语义保证幂等性。
///
/// # Arguments
///
/// * `mm` - 数据库管理器
/// * `db_id` - 默认数据库ID
/// * `configs` - 配置文件中的数据源配置列表
async fn persist_datasource_configs(
    mm: &DatabaseManager,
    db_id: &str,
    app_identity: &crate::config::AppIdentity,
    configs: Vec<DbConfig>,
) -> crate::Result<()> {
    info!("开始持久化数据源配置...");

    for config in configs {
        // 按 db_id + 域应用模块联合查重（db_id 在不同域下可重复）
        let filter = SysDatasourceFilter {
            db_id: Some(OpValsString(vec![OpValString::Eq(config.db_id.clone())])),
            domain_code: Some(OpValsString(vec![OpValString::Eq(app_identity.domain_code.clone())])),
            application_code: Some(OpValsString(vec![OpValString::Eq(app_identity.application_code.clone())])),
            module_code: Some(OpValsString(vec![OpValString::Eq(app_identity.module_code.clone())])),
            ..Default::default()
        };

        let existing = GenericCrudService::<SysDatasourceBmc, SysDatasourceFilter>::list(
            mm,
            db_id,
            None,
            Some(vec![filter]),
            None,
        )
            .await
            .map_err(|e| Error::DatasourceInit(format!("查询数据源失败: {}", e)))?;

        let entity_for_update = dbconfig_to_entity_for_update(&config);

        if existing.iter().count() > 0 {
            if let Some(data_row) = existing.iter().next() {
                let id = data_row.get_by_name(&existing.schema, "id");
                if let Some(cmx_core::model::cell::DataValue::String(id_str)) = id {
                    GenericCrudService::<SysDatasourceBmc>::update(
                        mm,
                        db_id,
                        None,
                        serde_json::Value::String(id_str.clone()),
                        entity_for_update,
                    )
                        .await
                        .map_err(|e| Error::DatasourceInit(format!(
                            "更新数据源 {} 失败: {}", config.db_id, e
                        )))?;
                    info!("成功更新数据源: {}", config.db_id);
                }
            }
            continue;
        }

        let entity = dbconfig_to_entity(&config);

        GenericCrudService::<SysDatasourceBmc>::create(mm, db_id, None, entity)
            .await
            .map_err(|e| Error::DatasourceInit(format!(
                "持久化数据源 {} 失败: {}", config.db_id, e
            )))?;
        info!("成功持久化数据源: {}", config.db_id);
    }

    info!("数据源配置持久化完成");
    Ok(())
}

/// 从数据库加载启用的数据源
///
/// 查询条件：status=1（启用）且 archived=0（未归档）。
///
/// # Arguments
///
/// * `mm` - 数据库管理器
/// * `db_id` - 默认数据库ID
///
/// # Returns
///
/// 满足条件的数据源配置列表。
async fn load_active_datasources(
    mm: &DatabaseManager,
    db_id: &str,
    app_identity: &crate::config::AppIdentity,
) -> crate::Result<Vec<DbConfig>> {
    info!("开始加载有效数据源（域: {}/{}/{}）...",
        app_identity.domain_code, app_identity.application_code, app_identity.module_code);

    let filter = SysDatasourceFilter {
        domain_code: Some(OpValsString(vec![OpValString::Eq(app_identity.domain_code.clone())])),
        application_code: Some(OpValsString(vec![OpValString::Eq(app_identity.application_code.clone())])),
        module_code: Some(OpValsString(vec![OpValString::Eq(app_identity.module_code.clone())])),
        status: Some(OpValsInt64(vec![OpValInt64::Eq(1)])),
        archived: Some(OpValsInt64(vec![OpValInt64::Eq(0)])),
        ..Default::default()
    };

    let dataset = GenericCrudService::<SysDatasourceBmc, SysDatasourceFilter>::list(
        mm,
        db_id,
        None,
        Some(vec![filter]),
        None,
    )
        .await
        .map_err(|e| Error::DatasourceInit(format!("查询数据源失败: {}", e)))?;

    let mut datasources = Vec::new();
    let schema = &dataset.schema;

    for row in dataset.iter() {
        if let Some(config) = build_dbconfig_from_row(row, schema) {
            datasources.push(config);
        } else {
            warn!("解析数据源配置失败，跳过");
        }
    }

    info!("成功加载 {} 个有效数据源", datasources.len());
    Ok(datasources)
}

/// 注册数据源到内存
///
/// 遍历配置列表，调用 `db_manager.register_data_source` 注册每个数据源。
///
/// # Arguments
///
/// * `mm` - 数据库管理器
/// * `configs` - 待注册的数据源配置列表
async fn register_datasources(mm: &DatabaseManager, configs: Vec<DbConfig>) -> crate::Result<()> {
    info!("开始注册数据源到内存...");

    let mut success_count = 0;
    let mut fail_count = 0;

    for config in configs {
        match mm.register_data_source(config.clone()).await {
            Ok(_) => {
                info!(
                    "成功注册数据源: {} (类型: {:?})",
                    config.db_id, config.db_type
                );
                success_count += 1;
            }
            Err(e) => {
                warn!("注册数据源 {} 失败: {}", config.db_id, e);
                fail_count += 1;
            }
        }
    }

    info!(
        "数据源注册完成: 成功 {} 个，失败 {} 个",
        success_count, fail_count
    );

    if fail_count > 0 {
        Err(Error::DatasourceInit(format!("有 {} 个数据源注册失败", fail_count)))
    } else {
        Ok(())
    }
}

/// 将 DbConfig 转换为 SysDatasourceForCreate
///
/// 用于创建新数据源记录时的数据转换。
///
/// # Arguments
///
/// * `config` - 数据源配置
fn dbconfig_to_entity(config: &DbConfig) -> SysDatasourceForCreate {
    SysDatasourceForCreate {
        db_id: config.db_id.clone(),
        description: None,
        db_type: format!("{:?}", config.db_type).to_lowercase(),
        db_url: config.db_url.clone(),
        db_schema: config.db_schema.clone(),
        default_flag: if config.default { Some(1) } else { Some(0) },
        domain_code: config.domain_code.clone(),
        application_code: config.application_code.clone(),
        module_code: config.module_code.clone(),
        source_type: config.source_type.clone(),
        max_connections: Some(config.pool_config.max_connections as i32),
        min_connections: Some(config.pool_config.min_connections as i32),
        connect_timeout: Some(config.pool_config.connect_timeout as i64),
        idle_timeout: Some(config.pool_config.idle_timeout as i64),
        max_lifetime: Some(config.pool_config.max_lifetime as i64),
        health_check_interval: Some(config.health_check_interval as i64),
        health_check_timeout: Some(config.health_check_timeout as i64),
        source: Some("config".to_string()),
        status: 1,
    }
}

/// 将 DbConfig 转换为 SysDatasourceForUpdate
///
/// 用于更新数据源记录时的数据转换。
///
/// # Arguments
///
/// * `config` - 数据源配置
fn dbconfig_to_entity_for_update(config: &DbConfig) -> SysDatasourceForUpdate {
    SysDatasourceForUpdate {
        db_id: Some(config.db_id.clone()),
        description: None,
        db_type: Some(format!("{:?}", config.db_type).to_lowercase()),
        db_url: Some(config.db_url.clone()),
        db_schema: config.db_schema.clone(),
        default_flag: Some(if config.default { 1 } else { 0 }),
        domain_code: config.domain_code.clone(),
        application_code: config.application_code.clone(),
        module_code: config.module_code.clone(),
        source_type: config.source_type.clone(),
        max_connections: Some(config.pool_config.max_connections as i32),
        min_connections: Some(config.pool_config.min_connections as i32),
        connect_timeout: Some(config.pool_config.connect_timeout as i64),
        idle_timeout: Some(config.pool_config.idle_timeout as i64),
        max_lifetime: Some(config.pool_config.max_lifetime as i64),
        health_check_interval: Some(config.health_check_interval as i64),
        health_check_timeout: Some(config.health_check_timeout as i64),
        source: Some("config".to_string()),
        status: 1,
        archived: Some(0),
    }
}

/// 从数据集行构建 DbConfig
///
/// GenericCrudService 已自动解密 db_url，此处为防御性处理，
/// 以防未来直接 SQL 查询的场景。
///
/// # Arguments
///
/// * `row` - 数据行
/// * `schema` - 数据集模式
fn build_dbconfig_from_row(
    row: &cmx_core::model::data::dataset::Row,
    schema: &cmx_core::model::data::dataset::Schema,
) -> Option<DbConfig> {
    use std::str::FromStr;

    let db_id = get_string_field(row, schema, "db_id")?;
    let db_type_str = get_string_field(row, schema, "db_type")?;
    let db_url = get_string_field(row, schema, "db_url")?;

    // 防御性解密：GenericCrudService 已自动解密 db_url
    let decrypted = cmx_utils::crypto::CryptoService::global()
        .ok()
        .and_then(|c| c.decrypt(&db_url).ok());
    let db_url = decrypted.unwrap_or(db_url);

    let db_type = cmx_database::config::DbType::from_str(&db_type_str).ok()?;

    let db_schema = get_string_field(row, schema, "db_schema");
    let default_flag = get_int_field(row, schema, "default_flag").unwrap_or(0);

    let domain_code = get_string_field(row, schema, "domain_code");
    let application_code = get_string_field(row, schema, "application_code");
    let module_code = get_string_field(row, schema, "module_code");
    let source_type = get_string_field(row, schema, "source_type");

    let max_connections = get_int_field(row, schema, "max_connections").unwrap_or(10) as usize;
    let min_connections = get_int_field(row, schema, "min_connections").unwrap_or(2) as usize;
    let connect_timeout = get_int_field(row, schema, "connect_timeout").unwrap_or(30) as u64;
    let idle_timeout = get_int_field(row, schema, "idle_timeout").unwrap_or(600) as u64;
    let max_lifetime = get_int_field(row, schema, "max_lifetime").unwrap_or(1800) as u64;
    let health_check_interval =
        get_int_field(row, schema, "health_check_interval").unwrap_or(60) as u64;
    let health_check_timeout =
        get_int_field(row, schema, "health_check_timeout").unwrap_or(5) as u64;

    Some(DbConfig {
        db_type,
        db_url,
        db_id,
        db_schema,
        default: default_flag == 1,
        pool_config: cmx_database::PoolConfig {
            max_connections,
            min_connections,
            connect_timeout,
            idle_timeout,
            max_lifetime,
            ..Default::default()
        },
        health_check_interval,
        health_check_timeout,
        domain_code,
        application_code,
        module_code,
        source_type,
    })
}

/// 从数据行获取字符串字段
///
/// # Arguments
///
/// * `row` - 数据行
/// * `schema` - 数据集模式
/// * `field_name` - 字段名称
fn get_string_field(
    row: &cmx_core::model::data::dataset::Row,
    schema: &cmx_core::model::data::dataset::Schema,
    field_name: &str,
) -> Option<String> {
    use std::convert::TryFrom;
    row.get_by_name(schema, field_name)
        .and_then(|v| String::try_from(v.clone()).ok())
}

/// 从数据行获取整数字段
///
/// # Arguments
///
/// * `row` - 数据行
/// * `schema` - 数据集模式
/// * `field_name` - 字段名称
fn get_int_field(
    row: &cmx_core::model::data::dataset::Row,
    schema: &cmx_core::model::data::dataset::Schema,
    field_name: &str,
) -> Option<i64> {
    use std::convert::TryFrom;
    row.get_by_name(schema, field_name)
        .and_then(|v| i64::try_from(v.clone()).ok())
}
