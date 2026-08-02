use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use edtui::{EditorEventHandler, EditorMode, EditorState, Lines};

use crate::agent_session::tab_prefix;
use crate::backlog::BacklogRef;
use crate::craft::Craft;
use crate::journal::JournalEntry;
use crate::multiplexer::{Multiplexer, Zellij};
use crate::person::Person;
use crate::principle::Principle;
use crate::routine::Routine;
use crate::stability::StabilityArea;
use crate::store::Store;
use crate::task::{Profile, Task};

fn text_editor(initial: &str) -> EditorState {
    let mut state = EditorState::new(Lines::from(initial));
    state.mode = EditorMode::Insert;
    state
}

#[derive(Clone, Copy, PartialEq)]
pub enum Field {
    Title,
    Description,
    Tags,
}

impl Field {
    fn next(self) -> Field {
        match self {
            Field::Title => Field::Description,
            Field::Description => Field::Tags,
            Field::Tags => Field::Title,
        }
    }

    fn prev(self) -> Field {
        match self {
            Field::Title => Field::Tags,
            Field::Description => Field::Title,
            Field::Tags => Field::Description,
        }
    }
}

/// WAY-8 thread 1: whether a claude session exists for a task "somewhere."
#[derive(Clone, Copy, PartialEq)]
pub enum SessionStatus {
    /// A live zellij tab for this task is open right now.
    Live,
    /// `claude_session_id` is set but no live tab currently matches it -
    /// previously active, not running now.
    Idle,
    /// Never started.
    None,
}

#[derive(Clone, Copy, PartialEq)]
pub enum View {
    Active,
    Archived,
}

#[derive(Clone, Copy)]
pub enum ConfirmKind {
    Archive,
    Restore,
}

#[derive(Clone, Copy)]
pub enum Mode {
    Normal,
    Editing(Field),
    Confirm(ConfirmKind),
    PillarPick,
    Detail,
}

pub struct App {
    store: Box<dyn Store>,
    pub tasks: Vec<Task>,
    pub journal: Vec<JournalEntry>,
    pub routines: Vec<Routine>,
    pub people: Vec<Person>,
    pub crafts: Vec<Craft>,
    pub stability: Vec<StabilityArea>,
    pub principles: Vec<Principle>,
    /// The unified, flat, selectable sequence rendered in the list pane -
    /// active-view only (archived stays task-only, see `refresh`).
    pub backlog: Vec<BacklogRef>,
    /// Every currently-open tab name in way's zellij session, refreshed
    /// alongside everything else in `refresh()` (one shell-out, not one per
    /// row/frame) - WAY-8's "is there a live session somewhere" indicator.
    /// Empty (not an error) whenever zellij isn't installed or the session
    /// doesn't exist yet - liveness is just unknown/false in that case, not
    /// something worth surfacing as a TUI-blocking error.
    pub live_tabs: Vec<String>,
    pub selected: usize,
    pub mode: Mode,
    pub view: View,
    pub active_profile: Profile,
    pub title_editor: EditorState,
    pub description_editor: EditorState,
    pub draft_tags: Vec<String>,
    pub tag_input: String,
    editor_events: EditorEventHandler,
    editing_id: Option<u64>,
    pub should_quit: bool,
    pub pending_spawn: Option<Task>,
}

impl App {
    pub fn new(store: Box<dyn Store>) -> Result<Self> {
        let active_profile = store.active_profile()?;
        let mut app = Self {
            store,
            tasks: Vec::new(),
            journal: Vec::new(),
            routines: Vec::new(),
            people: Vec::new(),
            crafts: Vec::new(),
            stability: Vec::new(),
            principles: Vec::new(),
            backlog: Vec::new(),
            live_tabs: Vec::new(),
            selected: 0,
            mode: Mode::Normal,
            view: View::Active,
            active_profile,
            title_editor: text_editor(""),
            description_editor: text_editor(""),
            draft_tags: Vec::new(),
            tag_input: String::new(),
            editor_events: EditorEventHandler::emacs_mode(),
            editing_id: None,
            should_quit: false,
            pending_spawn: None,
        };
        app.refresh()?;
        Ok(app)
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.tasks = match self.view {
            View::Active => self.store.list()?,
            View::Archived => self.store.list_archived()?,
        };

        // Best-effort: no zellij, no "way" session yet, or any other query
        // failure all just mean "liveness unknown" - never worth failing an
        // otherwise-successful refresh over.
        self.live_tabs = if Zellij::is_installed() { Zellij.list_open_tabs().unwrap_or_default() } else { Vec::new() };

        self.backlog.clear();
        for i in 0..self.tasks.len() {
            self.backlog.push(BacklogRef::Task(i));
        }

        // Archived view stays task-only, matching the pre-existing archive
        // browse flow exactly - other entity kinds have no archived state
        // exposed here yet (craft/stability use `status`, routine has its
        // own archive that isn't surfaced in this view).
        if self.view == View::Active {
            self.journal = self.store.list_journal_entries()?;
            self.routines = self.store.list_routines()?;
            self.people = self.store.list_people()?;
            self.crafts = self.store.list_crafts(None)?;
            self.stability = self.store.list_stability_areas(None)?;
            self.principles = self.store.list_principles()?;

            for i in 0..self.journal.len() {
                self.backlog.push(BacklogRef::Journal(i));
            }
            for i in 0..self.routines.len() {
                self.backlog.push(BacklogRef::Routine(i));
            }
            for i in 0..self.people.len() {
                self.backlog.push(BacklogRef::Person(i));
            }
            for i in 0..self.crafts.len() {
                self.backlog.push(BacklogRef::Craft(i));
            }
            for i in 0..self.stability.len() {
                self.backlog.push(BacklogRef::Stability(i));
            }
            for i in 0..self.principles.len() {
                self.backlog.push(BacklogRef::Principle(i));
            }
        } else {
            self.journal.clear();
            self.routines.clear();
            self.people.clear();
            self.crafts.clear();
            self.stability.clear();
            self.principles.clear();
        }

        if self.selected >= self.backlog.len() && !self.backlog.is_empty() {
            self.selected = self.backlog.len() - 1;
        }
        Ok(())
    }

    /// `Some` only when the current selection is a `Task` row - every
    /// existing task-mutation keybinding (add aside) is scoped to this,
    /// since the other six entity kinds are CLI-only for now (each
    /// pillar's own PRD decision).
    pub fn selected_task(&self) -> Option<&Task> {
        match self.backlog.get(self.selected) {
            Some(BacklogRef::Task(i)) => self.tasks.get(*i),
            _ => None,
        }
    }

    /// WAY-8, thread 1: is a claude session "somewhere" for this task - a
    /// live zellij tab right now, a previously-active one (session id set,
    /// no live tab), or neither. Purely derived from already-refreshed
    /// state - no I/O here, `refresh` already did the one shell-out.
    pub fn session_status(&self, task: &Task) -> SessionStatus {
        let prefix = tab_prefix(task.key);
        if self.live_tabs.iter().any(|name| name.starts_with(&prefix)) {
            SessionStatus::Live
        } else if task.claude_session_id.is_some() {
            SessionStatus::Idle
        } else {
            SessionStatus::None
        }
    }

    pub fn on_key(&mut self, key: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        match self.mode {
            Mode::Normal => self.on_key_normal(key)?,
            Mode::Editing(field) => self.on_key_editing(field, key, modifiers)?,
            Mode::Confirm(kind) => self.on_key_confirm(kind, key)?,
            Mode::PillarPick => self.on_key_pillar_pick(key)?,
            Mode::Detail => self.on_key_detail(key)?,
        }
        Ok(())
    }

    fn on_key_normal(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('A') => {
                self.view = match self.view {
                    View::Active => View::Archived,
                    View::Archived => View::Active,
                };
                self.selected = 0;
                self.refresh()?;
            }
            KeyCode::Char('a') if self.view == View::Active => {
                self.title_editor = text_editor("");
                self.description_editor = text_editor("");
                self.draft_tags.clear();
                self.tag_input.clear();
                self.editing_id = None;
                self.mode = Mode::Editing(Field::Title);
            }
            KeyCode::Char('e') if self.view == View::Active => self.start_edit(Field::Title),
            KeyCode::Char('t') if self.view == View::Active => self.start_edit(Field::Tags),
            KeyCode::Char('d') => {
                if self.selected_task().is_some() || self.view == View::Archived {
                    self.mode = Mode::Confirm(match self.view {
                        View::Active => ConfirmKind::Archive,
                        View::Archived => ConfirmKind::Restore,
                    });
                }
            }
            KeyCode::Char('p') if self.view == View::Active => {
                if self.selected_task().is_some() {
                    self.mode = Mode::PillarPick;
                }
            }
            KeyCode::Char('s') if self.view == View::Active => {
                self.toggle_selected()?;
            }
            KeyCode::Char('c') => {
                if let Some(task) = self.selected_task() {
                    self.pending_spawn = Some(task.clone());
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if !self.backlog.is_empty() {
                    self.mode = Mode::Detail;
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected + 1 < self.backlog.len() {
                    self.selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
            }
            _ => {}
        }
        Ok(())
    }

    fn on_key_detail(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => self.mode = Mode::Normal,
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected + 1 < self.backlog.len() {
                    self.selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
            }
            _ => {}
        }
        Ok(())
    }

    fn toggle_selected(&mut self) -> Result<()> {
        if let Some(task) = self.selected_task() {
            let id = task.id;
            self.store.toggle(id)?;
            self.refresh()?;
        }
        Ok(())
    }

    fn start_edit(&mut self, field: Field) {
        if let Some(task) = self.selected_task() {
            let (title, description, tags, id) = (task.title.clone(), task.description.clone(), task.tags.clone(), task.id);
            self.title_editor = text_editor(&title);
            self.description_editor = text_editor(&description);
            self.draft_tags = tags;
            self.tag_input.clear();
            self.editing_id = Some(id);
            self.mode = Mode::Editing(field);
        }
    }

    fn on_key_confirm(&mut self, kind: ConfirmKind, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Char('y') | KeyCode::Enter => {
                if let Some(task) = self.selected_task() {
                    let id = task.id;
                    match kind {
                        ConfirmKind::Archive => self.store.archive(id)?,
                        ConfirmKind::Restore => self.store.unarchive(id)?,
                    }
                    self.refresh()?;
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        Ok(())
    }

    fn on_key_pillar_pick(&mut self, key: KeyCode) -> Result<()> {
        let count = self.active_profile.pillars.len() as u32;
        let selection: Option<Option<String>> = match key {
            KeyCode::Char('0') => Some(None),
            KeyCode::Char(c) if c.to_digit(10).is_some_and(|d| (1..=count).contains(&d)) => {
                let idx = c.to_digit(10).unwrap() as usize - 1;
                Some(Some(self.active_profile.pillars[idx].name.clone()))
            }
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                return Ok(());
            }
            _ => return Ok(()),
        };
        if let Some(pillar) = selection
            && let Some(task) = self.selected_task()
        {
            let id = task.id;
            self.store.set_pillar(id, pillar)?;
            self.refresh()?;
        }
        self.mode = Mode::Normal;
        Ok(())
    }

    fn on_key_editing(&mut self, field: Field, key: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        match key {
            KeyCode::Esc => {
                self.editing_id = None;
                self.mode = Mode::Normal;
                return Ok(());
            }
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.editing_id = None;
                self.mode = Mode::Normal;
                return Ok(());
            }
            KeyCode::Char('s') if modifiers.contains(KeyModifiers::CONTROL) => {
                return self.save_edit();
            }
            KeyCode::Tab => {
                self.mode = Mode::Editing(field.next());
                return Ok(());
            }
            KeyCode::BackTab => {
                self.mode = Mode::Editing(field.prev());
                return Ok(());
            }
            _ => {}
        }

        match field {
            Field::Tags => match key {
                KeyCode::Enter | KeyCode::Char(',') if !self.tag_input.trim().is_empty() => {
                    let tag = self.tag_input.trim().to_string();
                    if !self.draft_tags.iter().any(|t| t == &tag) {
                        self.draft_tags.push(tag);
                    }
                    self.tag_input.clear();
                }
                KeyCode::Backspace if self.tag_input.is_empty() => {
                    self.draft_tags.pop();
                }
                KeyCode::Backspace => {
                    self.tag_input.pop();
                }
                KeyCode::Char(c) => {
                    self.tag_input.push(c);
                }
                _ => {}
            },
            Field::Title | Field::Description => {
                let state = match field {
                    Field::Title => &mut self.title_editor,
                    Field::Description => &mut self.description_editor,
                    Field::Tags => unreachable!(),
                };
                self.editor_events.on_key_event(crossterm::event::KeyEvent::new(key, modifiers), state);
            }
        }
        Ok(())
    }

    fn save_edit(&mut self) -> Result<()> {
        let title = self.title_editor.lines.to_string().trim().to_string();
        if title.is_empty() {
            self.mode = Mode::Editing(Field::Title);
            return Ok(());
        }
        let description = self.description_editor.lines.to_string();
        let tags = self.draft_tags.clone();
        match self.editing_id {
            Some(id) => self.store.update_fields(id, title, description, tags)?,
            None => {
                self.store.add(title, description, tags)?;
            }
        }
        self.editing_id = None;
        self.mode = Mode::Normal;
        self.refresh()?;
        Ok(())
    }

    pub fn editing_key(&self) -> Option<u32> {
        self.editing_id.and_then(|id| self.tasks.iter().find(|t| t.id == id)).map(|t| t.key)
    }

    pub fn store(&self) -> &dyn Store {
        self.store.as_ref()
    }
}
