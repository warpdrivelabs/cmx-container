

use serde::{Deserialize, Serialize};
use crate::model::data::dataset::rds::RowDataSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainEntity {
    pub id: String,
    pub name: String,
    pub dataset: Option<RowDataSet>,
}

impl DomainEntity {
    pub fn new(id: String, name: String, dataset: RowDataSet) -> Self {
        Self {
            id,
            name,
            dataset: Some(dataset),
        }
    }
}
