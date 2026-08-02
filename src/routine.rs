use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exercise {
    pub name: String,
    pub sets: u32,
    pub reps: u32,
    pub intensity: f32, // 0.0-1.0, felt effort - not a physical unit
    pub friction: f32,  // 0.0-1.0, same scale
    pub duration_secs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Routine {
    pub name: String, // unique - primary key
    pub exercises: Vec<Exercise>,
    pub archived: bool,
}

fn avg(values: impl Iterator<Item = f32> + Clone) -> f32 {
    let count = values.clone().count();
    if count == 0 {
        return 0.0;
    }
    values.sum::<f32>() / count as f32
}

impl Routine {
    pub fn duration_secs(&self) -> u32 {
        self.exercises.iter().map(|e| e.duration_secs).sum()
    }

    pub fn intensity(&self) -> f32 {
        avg(self.exercises.iter().map(|e| e.intensity))
    }

    pub fn friction(&self) -> f32 {
        avg(self.exercises.iter().map(|e| e.friction))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineCompletion {
    pub id: u64,
    pub routine_name: String, // loose reference by name, same style as Task.pillar
    pub completed_on: i64,    // unix seconds, the date the routine was done
    pub note: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_routine_has_zero_aggregates() {
        let r = Routine { name: "empty".to_string(), exercises: vec![], archived: false };
        assert_eq!(r.duration_secs(), 0);
        assert_eq!(r.intensity(), 0.0);
        assert_eq!(r.friction(), 0.0);
    }

    #[test]
    fn aggregates_recompute_from_current_exercises() {
        let r = Routine {
            name: "push".to_string(),
            exercises: vec![
                Exercise { name: "bench".to_string(), sets: 4, reps: 8, intensity: 0.8, friction: 0.6, duration_secs: 600 },
                Exercise { name: "ohp".to_string(), sets: 3, reps: 10, intensity: 0.6, friction: 0.4, duration_secs: 540 },
            ],
            archived: false,
        };
        assert_eq!(r.duration_secs(), 1140);
        assert!((r.intensity() - 0.7).abs() < 1e-6);
        assert!((r.friction() - 0.5).abs() < 1e-6);
    }
}
