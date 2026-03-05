use std::sync::Arc;
use crate::{model::data::dataset::{rds::RowDataSet, Schema}, model::meta::dme::DMEMeta};
use super::entity::DomainEntity;

fn empty_schema() -> Arc<Schema> {
    Arc::new(Schema::new("default", vec![]))
}

#[derive(Debug)]
pub struct DomainEntityManager;

impl DomainEntityManager {
    // 根据元数据创建 DataSet
    pub fn create_dataset(meta: &DMEMeta) -> Result<RowDataSet, String> {
        // 检查是否存在 fct_list 定义
        if let Some(_fct_list) = &meta.fct_list {
            // 使用 fct_list 的 schema 创建新的 DataSet（占位：暂用空 schema）
            Ok(RowDataSet::empty("test", empty_schema()))
        } else if let Some(fct_meta_map) = &meta.fct_meta_map {
            // 如果有 FCTMeta，使用第一个 FCTMeta 的 schema
            if let Some(_fct_meta) = fct_meta_map.values().next() {
                Ok(RowDataSet::empty("test", empty_schema()))
            } else {
                Err("No FCTMeta found in meta definition".to_string())
            }
        } else {
            Err("No schema definition found in meta".to_string())
        }
    }

    // 创建实体（包含数据集创建）
    pub fn create_entity(meta: &DMEMeta) -> Result<DomainEntity, String> {
        let dataset = Self::create_dataset(meta)?;
        Ok(DomainEntity::new("test".to_string(), "test".to_string(), dataset))
    }

    // 验证实体数据是否符合元数据定义
    pub fn validate_entity(_entity: &DomainEntity, _meta: &DMEMeta) -> Result<(), String> {
        // 验证数据集结构
        // if let Some(fct_list) = &meta.fct_list {
        //     if entity.dataset.schema() != fct_list.schema() {
        //         return Err("Dataset schema does not match meta definition".to_string());
        //     }
        // }
        
        // TODO: 添加更多验证规则
        // 1. 检查必填字段
        // 2. 验证数据类型
        // 3. 验证外键关系
        // 4. 验证业务规则
        
        Ok(())
    }

    // 序列化实体
    pub fn serialize_entity(entity: &DomainEntity) -> Result<String, serde_json::Error> {
        serde_json::to_string(entity)
    }

    // 反序列化实体
    pub fn deserialize_entity(json: &str) -> Result<DomainEntity, serde_json::Error> {
        serde_json::from_str(json)
    }
} 