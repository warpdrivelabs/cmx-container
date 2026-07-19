//! 测试基础设施冒烟测试
mod common;

use common::{TEST_DB_KEY, setup_db_manager};

#[tokio::test]
async fn test_db_manager_setup() {
    let manager = setup_db_manager().await;
    let data_sources = manager.list_data_sources().await;
    assert!(
        data_sources.contains(&TEST_DB_KEY.to_string()),
        "数据源应已注册"
    );
    manager.shutdown().await.expect("关闭应成功");
}
