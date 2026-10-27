//! SM-2-derived spaced-repetition scheduling over Anki's 4-button grading.
//! Pure and persistence-agnostic: callers own turning `interval_days` into
//! an actual due timestamp and storing it.

const MIN_EASE_FACTOR: f32 = 1.3;
const DEFAULT_EASE_FACTOR: f32 = 2.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade {
    Again,
    Hard,
    Good,
    Easy,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReviewState {
    pub ease_factor: f32,
    pub interval_days: f32,
    pub repetitions: u32,
}

impl Default for ReviewState {
    fn default() -> Self {
        Self {
            ease_factor: DEFAULT_EASE_FACTOR,
            interval_days: 0.0,
            repetitions: 0,
        }
    }
}

impl ReviewState {
    pub fn schedule(self, grade: Grade) -> Self {
        match grade {
            Grade::Again => Self {
                ease_factor: (self.ease_factor - 0.2).max(MIN_EASE_FACTOR),
                interval_days: 0.0,
                repetitions: 0,
            },
            Grade::Hard => Self {
                ease_factor: (self.ease_factor - 0.15).max(MIN_EASE_FACTOR),
                interval_days: (self.interval_days * 1.2).max(1.0),
                repetitions: self.repetitions + 1,
            },
            Grade::Good => {
                let repetitions = self.repetitions + 1;
                let interval_days = match repetitions {
                    1 => 1.0,
                    2 => 6.0,
                    _ => self.interval_days * self.ease_factor,
                };
                Self {
                    ease_factor: self.ease_factor,
                    interval_days,
                    repetitions,
                }
            }
            Grade::Easy => {
                let repetitions = self.repetitions + 1;
                let ease_factor = self.ease_factor + 0.15;
                let interval_days = if repetitions == 1 {
                    4.0
                } else {
                    self.interval_days * ease_factor * 1.3
                };
                Self {
                    ease_factor,
                    interval_days,
                    repetitions,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn again_resets_repetitions_and_interval() {
        let state = ReviewState {
            ease_factor: 2.5,
            interval_days: 10.0,
            repetitions: 3,
        };
        let next = state.schedule(Grade::Again);
        assert_eq!(next.repetitions, 0);
        assert_eq!(next.interval_days, 0.0);
        assert!(next.ease_factor < state.ease_factor);
    }

    #[test]
    fn ease_factor_never_drops_below_minimum() {
        let state = ReviewState {
            ease_factor: 1.3,
            interval_days: 1.0,
            repetitions: 1,
        };
        let next = state.schedule(Grade::Again);
        assert_eq!(next.ease_factor, MIN_EASE_FACTOR);
    }

    #[test]
    fn first_good_review_schedules_one_day_out() {
        let next = ReviewState::default().schedule(Grade::Good);
        assert_eq!(next.repetitions, 1);
        assert_eq!(next.interval_days, 1.0);
    }

    #[test]
    fn second_good_review_schedules_six_days_out() {
        let first = ReviewState::default().schedule(Grade::Good);
        let second = first.schedule(Grade::Good);
        assert_eq!(second.repetitions, 2);
        assert_eq!(second.interval_days, 6.0);
    }

    #[test]
    fn later_good_reviews_grow_by_ease_factor() {
        let state = ReviewState {
            ease_factor: 2.0,
            interval_days: 6.0,
            repetitions: 2,
        };
        let next = state.schedule(Grade::Good);
        assert_eq!(next.repetitions, 3);
        assert_eq!(next.interval_days, 12.0);
    }

    #[test]
    fn easy_grows_interval_faster_than_good() {
        let state = ReviewState {
            ease_factor: 2.5,
            interval_days: 6.0,
            repetitions: 2,
        };
        let good = state.schedule(Grade::Good);
        let easy = state.schedule(Grade::Easy);
        assert!(easy.interval_days > good.interval_days);
    }

    #[test]
    fn hard_grows_interval_slower_than_good_but_keeps_streak() {
        let state = ReviewState {
            ease_factor: 2.5,
            interval_days: 6.0,
            repetitions: 2,
        };
        let hard = state.schedule(Grade::Hard);
        let good = state.schedule(Grade::Good);
        assert_eq!(hard.repetitions, good.repetitions);
        assert!(hard.interval_days < good.interval_days);
    }

    proptest::proptest! {
        #[test]
        fn ease_factor_and_interval_stay_in_bounds(
            initial_ease in 1.3f32..5.0,
            grades in proptest::collection::vec(0u8..4, 0..50),
        ) {
            let mut state = ReviewState { ease_factor: initial_ease, interval_days: 0.0, repetitions: 0 };
            for g in grades {
                let grade = match g {
                    0 => Grade::Again,
                    1 => Grade::Hard,
                    2 => Grade::Good,
                    _ => Grade::Easy,
                };
                state = state.schedule(grade);
                assert!(state.ease_factor >= MIN_EASE_FACTOR);
                assert!(state.interval_days >= 0.0);
            }
        }
    }
}
