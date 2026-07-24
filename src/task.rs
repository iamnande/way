use serde::{Deserialize, Serialize};

/// Legacy fixed pillar set, retained only as the seed list for bootstrapping
/// a fresh store's default "personal" profile. Tasks no longer reference this
/// type directly — see `Task.pillar`, which is a profile-defined name.
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

    pub fn color(self) -> (u8, u8, u8) {
        match self {
            Pillar::Mind => (0x83, 0xC0, 0x92),       // aqua
            Pillar::Body => (0xA7, 0xC0, 0x80),       // green
            Pillar::Relationships => (0xD6, 0x99, 0xB6), // purple
            Pillar::Craft => (0xDB, 0xBC, 0x7F),      // yellow
            Pillar::Stability => (0x7F, 0xBB, 0xB3),  // blue
            Pillar::Purpose => (0xE6, 0x7E, 0x80),    // red
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PillarDef {
    pub name: String,
    pub glyph: String,
    pub color: (u8, u8, u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum IssueSystem {
    GithubDiscussions,
    Linear,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub pillars: Vec<PillarDef>,
    pub default_issue_system: Option<IssueSystem>,
}

impl Profile {
    pub fn find_pillar(&self, name: &str) -> Option<&PillarDef> {
        self.pillars.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// The default "personal" profile, seeded from the legacy fixed pillar set.
    pub fn default_personal() -> Self {
        Self {
            name: "personal".to_string(),
            pillars: Pillar::ALL
                .iter()
                .map(|p| PillarDef { name: p.label().to_string(), glyph: p.glyph().to_string(), color: p.color() })
                .collect(),
            default_issue_system: Some(IssueSystem::GithubDiscussions),
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
    pub pillar: Option<String>,
    #[serde(default)]
    pub parent_key: Option<u32>,
    #[serde(default)]
    pub external_ref: Option<String>,
    #[serde(default)]
    pub session_state: Option<String>,
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
            parent_key: None,
            external_ref: None,
            session_state: None,
        }
    }
}
