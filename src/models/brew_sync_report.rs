use super::BrewPackage;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct BrewSyncReport {
    pub to_install: Vec<BrewPackage>,
    pub to_remove: Vec<BrewPackage>,
    pub in_sync: Vec<BrewPackage>,
}
