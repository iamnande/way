use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CraftStatus {
    Active,
    Dormant,    // not currently practiced, contingent on something outside nick's control
    Historical, // genuinely past, no realistic path back
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Craft {
    pub name: String, // unique - primary key, same as Routine
    pub status: CraftStatus,
    #[serde(default)]
    pub space: String,
    #[serde(default)]
    pub standing: String,
    #[serde(default)]
    pub trajectory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftSession {
    pub id: u64,
    pub craft_name: String,
    pub logged_on: i64, // unix seconds, may be backdated
    pub note: Option<String>,
}
