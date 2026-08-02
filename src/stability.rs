use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum StabilityStatus {
    Active,
    Dormant,
    Historical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StabilityArea {
    pub id: u64,
    pub name: String, // unique - primary key
    pub status: StabilityStatus,
    #[serde(default)]
    pub standing: String,
    #[serde(default)]
    pub trajectory: String,
}
