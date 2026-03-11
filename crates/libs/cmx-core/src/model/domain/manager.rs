use std::sync::Arc;
use crate::{model::data::dataset::{rds::RowDataSet, Schema}};
use super::entity::DomainEntity;

fn empty_schema() -> Arc<Schema> {
    Arc::new(Schema::new("default", vec![]))
}

#[derive(Debug)]
pub struct DomainEntityManager;

impl DomainEntityManager {


    // 序列化实体
    pub fn serialize_entity(entity: &DomainEntity) -> Result<String, serde_json::Error> {
        serde_json::to_string(entity)
    }

    // 反序列化实体
    pub fn deserialize_entity(json: &str) -> Result<DomainEntity, serde_json::Error> {
        serde_json::from_str(json)
    }
}
