use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum JournalEntryKind {
    Freeform,
    CheckIn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub id: u64,
    pub created_at: i64,
    pub kind: JournalEntryKind,
    pub content: String,
}

/// Pure logic, no I/O - `None` cadence means never due; no prior check-in
/// with a cadence configured means due immediately; otherwise due once the
/// elapsed time since the last check-in reaches the configured cadence.
pub fn checkin_due(cadence_days: Option<u32>, now_unix: i64, last_checkin: Option<&JournalEntry>) -> bool {
    let Some(cadence_days) = cadence_days else { return false };
    match last_checkin {
        None => true,
        Some(entry) => {
            let elapsed_days = (now_unix - entry.created_at) / 86_400;
            elapsed_days >= cadence_days as i64
        }
    }
}

/// Renders a completed check-in's prompt/answer pairs into a single
/// `content` string - keeps `JournalEntry.content` uniform (freeform text)
/// across both kinds, so search works identically over either.
pub fn render_checkin(prompts_and_answers: &[(String, String)]) -> String {
    prompts_and_answers.iter().map(|(q, a)| format!("Q: {q}\nA: {a}")).collect::<Vec<_>>().join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_at(created_at: i64) -> JournalEntry {
        JournalEntry { id: 1, created_at, kind: JournalEntryKind::CheckIn, content: String::new() }
    }

    #[test]
    fn no_cadence_never_due() {
        assert!(!checkin_due(None, 1_000_000, None));
        assert!(!checkin_due(None, 1_000_000, Some(&entry_at(0))));
    }

    #[test]
    fn cadence_with_no_prior_checkin_is_due() {
        assert!(checkin_due(Some(7), 1_000_000, None));
    }

    #[test]
    fn cadence_within_window_not_due() {
        let now = 10 * 86_400;
        let last = entry_at(9 * 86_400); // 1 day ago
        assert!(!checkin_due(Some(7), now, Some(&last)));
    }

    #[test]
    fn cadence_past_window_is_due() {
        let now = 10 * 86_400;
        let last = entry_at(0); // 10 days ago
        assert!(checkin_due(Some(7), now, Some(&last)));
    }

    #[test]
    fn render_checkin_formats_qa_pairs() {
        let rendered = render_checkin(&[("Q1".to_string(), "A1".to_string()), ("Q2".to_string(), "A2".to_string())]);
        assert_eq!(rendered, "Q: Q1\nA: A1\n\nQ: Q2\nA: A2");
    }
}
