use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principle {
    pub id: u64,
    pub created_at: i64,
    pub text: String,
}
