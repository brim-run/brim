use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct BrewPackageResult {
    pub name: String,
    pub status: String,
}
