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
    /// Ordered phase vocabulary (e.g. grounding, spec, planning, ...). Empty
    /// (the default) means `way` enforces no order at all - today's
    /// free-form behavior. `way` still never interprets what a phase means,
    /// only its position in this list.
    #[serde(default)]
    pub phases: Vec<String>,
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
            phases: Vec::new(),
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
    pub external_refs: Vec<String>,
    /// Not tied to any one workflow's phase vocabulary (senzu's
    /// grounding/spec/planning/... is just one possible set of values) - `way`
    /// never interprets what a phase *means*, but does validate its position
    /// against the active profile's configured `phases` order, when set.
    #[serde(default)]
    pub phase: Option<String>,
    #[serde(default)]
    pub session_decisions: Option<String>,
    #[serde(default)]
    pub session_next: Option<String>,
    #[serde(default)]
    pub session_updated_at: Option<i64>,
    /// The claude CLI session UUID last used for this task, if any. Lets `way`
    /// resume the actual conversation (`claude --resume <id>`) instead of
    /// starting a disconnected new one seeded only with a text summary.
    #[serde(default)]
    pub claude_session_id: Option<String>,
    /// Reason text; presence means the task is blocked waiting on nick. `way`
    /// never interprets the reason, only whether it's set.
    #[serde(default)]
    pub waiting_on: Option<String>,
    /// Unix seconds, stamped whenever `waiting_on` is set; cleared together.
    /// Deliberately separate from `session_updated_at`, which is bumped by
    /// unrelated writes (decisions/next) and would corrupt "how long has
    /// this been blocked" if reused.
    #[serde(default)]
    pub waiting_on_since: Option<i64>,
    /// Local username of whoever created this task, stamped once at creation
    /// and never edited. `way` is still single-player - nothing reads this
    /// today - but capturing it now means existing tasks won't need a
    /// backfill once there's more than one user. Empty on tasks that predate
    /// this field.
    #[serde(default)]
    pub owner: String,
}

impl Task {
    pub fn new(id: u64, key: u32, title: String, description: String, tags: Vec<String>, owner: String) -> Self {
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
            external_refs: Vec::new(),
            phase: None,
            session_decisions: None,
            session_next: None,
            session_updated_at: None,
            claude_session_id: None,
            waiting_on: None,
            waiting_on_since: None,
            owner,
        }
    }
}
