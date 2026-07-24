use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use edtui::{EditorEventHandler, EditorMode, EditorState, Lines};

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
}

pub struct App {
    store: Box<dyn Store>,
    pub tasks: Vec<Task>,
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
    pub pending_spawn: Option<u32>,
}

impl App {
    pub fn new(store: Box<dyn Store>) -> Result<Self> {
        let tasks = store.list()?;
        let active_profile = store.active_profile()?;
        Ok(Self {
            store,
            tasks,
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
        })
    }

    fn refresh(&mut self) -> Result<()> {
        self.tasks = match self.view {
            View::Active => self.store.list()?,
            View::Archived => self.store.list_archived()?,
        };
        if self.selected >= self.tasks.len() && !self.tasks.is_empty() {
            self.selected = self.tasks.len() - 1;
        }
        Ok(())
    }

    pub fn on_key(&mut self, key: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        match self.mode {
            Mode::Normal => self.on_key_normal(key)?,
            Mode::Editing(field) => self.on_key_editing(field, key, modifiers)?,
            Mode::Confirm(kind) => self.on_key_confirm(kind, key)?,
            Mode::PillarPick => self.on_key_pillar_pick(key)?,
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
                if !self.tasks.is_empty() {
                    self.mode = Mode::Confirm(match self.view {
                        View::Active => ConfirmKind::Archive,
                        View::Archived => ConfirmKind::Restore,
                    });
                }
            }
            KeyCode::Char('p') if self.view == View::Active => {
                if !self.tasks.is_empty() {
                    self.mode = Mode::PillarPick;
                }
            }
            KeyCode::Char('s') | KeyCode::Char(' ') | KeyCode::Enter if self.view == View::Active => {
                self.toggle_selected()?;
            }
            KeyCode::Char('c') => {
                if let Some(task) = self.tasks.get(self.selected) {
                    self.pending_spawn = Some(task.key);
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected + 1 < self.tasks.len() {
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
        if let Some(task) = self.tasks.get(self.selected) {
            let id = task.id;
            self.store.toggle(id)?;
            self.refresh()?;
        }
        Ok(())
    }

    fn start_edit(&mut self, field: Field) {
        if let Some(task) = self.tasks.get(self.selected) {
            self.title_editor = text_editor(&task.title);
            self.description_editor = text_editor(&task.description);
            self.draft_tags = task.tags.clone();
            self.tag_input.clear();
            self.editing_id = Some(task.id);
            self.mode = Mode::Editing(field);
        }
    }

    fn on_key_confirm(&mut self, kind: ConfirmKind, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Char('y') | KeyCode::Enter => {
                if let Some(task) = self.tasks.get(self.selected) {
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
            && let Some(task) = self.tasks.get(self.selected)
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
}
