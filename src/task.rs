use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Pillar {
    Mind,
    Body,
    Relationships,
    Craft,
    Stability,
    Purpose,
}

impl Pillar {
    pub const ALL: [Pillar; 6] = [
        Pillar::Mind,
        Pillar::Body,
        Pillar::Relationships,
        Pillar::Craft,
        Pillar::Stability,
        Pillar::Purpose,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Pillar::Mind => "mind",
            Pillar::Body => "body",
            Pillar::Relationships => "relationships",
            Pillar::Craft => "craft",
            Pillar::Stability => "stability",
            Pillar::Purpose => "purpose",
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Pillar::Mind => "M",
            Pillar::Body => "B",
            Pillar::Relationships => "R",
            Pillar::Craft => "C",
            Pillar::Stability => "S",
            Pillar::Purpose => "P",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    #[serde(default)]
    pub key: u32,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub done: bool,
    pub archived: bool,
    #[serde(default)]
    pub pillar: Option<Pillar>,
}

impl Task {
    pub fn new(id: u64, key: u32, title: String, description: String, tags: Vec<String>) -> Self {
        Self {
            id,
            key,
            title,
            description,
            tags,
            done: false,
            archived: false,
            pillar: None,
        }
    }
}
