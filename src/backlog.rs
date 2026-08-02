/// A lightweight reference into one of `App`'s per-entity vecs, letting the
/// unified backlog stay a single flat, selectable sequence (matching the
/// validated TUI-prototype direction - one list across all entity kinds,
/// not siloed per-pillar tabs) without duplicating each entity's data.
#[derive(Clone, Copy)]
pub enum BacklogRef {
    Task(usize),
    Journal(usize),
    Routine(usize),
    Person(usize),
    Craft(usize),
    Stability(usize),
    Principle(usize),
}
