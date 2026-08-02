use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RelationshipKind {
    Child,
    Partner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    pub id: u64,
    pub name: String, // not unique - id-keyed, not name-keyed
    pub relationship: RelationshipKind,
    #[serde(default)]
    pub birthdate: Option<i64>,
    #[serde(default)]
    pub dreams_aspirations: Vec<String>,
    #[serde(default)]
    pub hobbies: Vec<String>,
    #[serde(default)]
    pub preferences: Vec<(String, String)>,
    #[serde(default)]
    pub attention_areas: Vec<String>,
    #[serde(default)]
    pub notes: String,
}

impl Person {
    pub fn new(id: u64, name: String, relationship: RelationshipKind) -> Self {
        Self {
            id,
            name,
            relationship,
            birthdate: None,
            dreams_aspirations: Vec::new(),
            hobbies: Vec::new(),
            preferences: Vec::new(),
            attention_areas: Vec::new(),
            notes: String::new(),
        }
    }
}
