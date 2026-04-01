pub mod schema;
pub mod migration;
pub mod table_metadata;

pub mod plugin;
pub mod version_history;
pub mod deployment;

pub mod repository {
    pub use super::plugin::*;
}
