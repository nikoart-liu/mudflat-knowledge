//! 间隔重复调度（SM-2 简化版）。纯函数，可单测。
//!
//! rating 四档：Again(1) Hard(2) Good(3) Easy(4)。
//! Again 分钟级重置（due = now + 10min）；成功分支 interval 以天计。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rating {
    Again,
    Hard,
    Good,
    Easy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SrsState {
    pub due_at: i64,
    pub interval_days: f64,
    pub ease: f64,
    pub reps: i64,
    pub lapses: i64,
}

impl Default for SrsState {
    fn default() -> Self {
        Self { due_at: 0, interval_days: 0.0, ease: 2.5, reps: 0, lapses: 0 }
    }
}

const DAY: f64 = 86400.0;
const MIN_INTERVAL_DAYS: f64 = 1.0;
const EASE_MIN: f64 = 1.3;
const AGAIN_DELAY_SECONDS: i64 = 600; // 10 分钟

pub fn schedule(state: &SrsState, rating: Rating, now: i64) -> SrsState {
    let mut s = *state;
    match rating {
        Rating::Again => {
            s.interval_days = 0.0;
            s.ease = (state.ease - 0.2).max(EASE_MIN);
            s.lapses += 1;
            s.reps = 0;
            s.due_at = now + AGAIN_DELAY_SECONDS;
        }
        Rating::Hard => {
            s.interval_days = (state.interval_days * 1.2).max(MIN_INTERVAL_DAYS);
            s.reps += 1;
            s.due_at = now + (s.interval_days * DAY).round() as i64;
        }
        Rating::Good => {
            s.interval_days = if state.reps == 0 { 1.0 } else { (state.interval_days * state.ease).max(MIN_INTERVAL_DAYS) };
            s.reps += 1;
            s.due_at = now + (s.interval_days * DAY).round() as i64;
        }
        Rating::Easy => {
            s.interval_days =
                if state.reps == 0 { 3.0 } else { (state.interval_days * state.ease * 1.3).max(MIN_INTERVAL_DAYS) };
            s.reps += 1;
            s.due_at = now + (s.interval_days * DAY).round() as i64;
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000;

    fn new_state() -> SrsState {
        SrsState::default()
    }

    #[test]
    fn again_on_new_card_resets_to_ten_minutes() {
        let s = schedule(&new_state(), Rating::Again, NOW);
        assert_eq!(s.due_at, NOW + 600);
        assert_eq!(s.interval_days, 0.0);
        assert_eq!((s.ease * 100.0).round() / 100.0, 2.30);
        assert_eq!(s.lapses, 1);
        assert_eq!(s.reps, 0);
    }

    #[test]
    fn good_new_card_interval_one_day() {
        let s = schedule(&new_state(), Rating::Good, NOW);
        assert_eq!(s.interval_days, 1.0);
        assert_eq!(s.due_at, NOW + 86_400);
        assert_eq!(s.reps, 1);
        assert_eq!(s.ease, 2.5);
    }

    #[test]
    fn easy_new_card_interval_three_days() {
        let s = schedule(&new_state(), Rating::Easy, NOW);
        assert_eq!(s.interval_days, 3.0);
        assert_eq!(s.due_at, NOW + 3 * 86_400);
    }

    #[test]
    fn hard_new_card_clamps_to_one_day() {
        let s = schedule(&new_state(), Rating::Hard, NOW);
        assert_eq!(s.interval_days, 1.0); // 0*1.2=0 -> clamp 到 1
    }

    #[test]
    fn ease_floor_at_1_3_after_repeated_again() {
        let mut s = new_state();
        for _ in 0..10 {
            s = schedule(&s, Rating::Again, NOW);
        }
        assert!((s.ease - EASE_MIN).abs() < 1e-9);
        assert_eq!(s.reps, 0);
        assert_eq!(s.lapses, 10);
        assert_eq!(s.due_at, NOW + 600); // 永远停在 10 分钟后
    }

    #[test]
    fn again_resets_interval_and_reps() {
        let mut s = new_state();
        s = schedule(&s, Rating::Good, NOW);
        s = schedule(&s, Rating::Good, NOW);
        assert_eq!(s.reps, 2);
        assert!(s.interval_days > 1.0);
        s = schedule(&s, Rating::Again, NOW);
        assert_eq!(s.interval_days, 0.0);
        assert_eq!(s.reps, 0);
        // 又一次成功后从 1 天重新起步
        let g = schedule(&s, Rating::Good, NOW);
        assert_eq!(g.interval_days, 1.0);
    }

    #[test]
    fn hard_grows_slowly_from_known_interval() {
        let base = SrsState { due_at: NOW, interval_days: 10.0, ease: 2.5, reps: 5, lapses: 0 };
        let s = schedule(&base, Rating::Hard, NOW);
        assert!((s.interval_days - 12.0).abs() < 1e-9);
        assert_eq!(s.due_at, NOW + (12.0 * DAY).round() as i64);
    }

    #[test]
    fn good_multiplies_by_ease() {
        let base = SrsState { due_at: NOW, interval_days: 10.0, ease: 2.5, reps: 5, lapses: 0 };
        let s = schedule(&base, Rating::Good, NOW);
        assert!((s.interval_days - 25.0).abs() < 1e-9);
    }

    #[test]
    fn easy_multiplies_by_ease_times_1_3() {
        let base = SrsState { due_at: NOW, interval_days: 10.0, ease: 2.5, reps: 5, lapses: 0 };
        let s = schedule(&base, Rating::Easy, NOW);
        assert!((s.interval_days - 32.5).abs() < 1e-9);
    }

    #[test]
    fn low_ease_still_never_shrinks_below_one_day_on_success() {
        let base = SrsState { due_at: NOW, interval_days: 10.0, ease: 1.3, reps: 8, lapses: 3 };
        let h = schedule(&base, Rating::Hard, NOW);
        assert!(h.interval_days >= 1.0);
        let g = schedule(&base, Rating::Good, NOW);
        assert!(g.interval_days >= 1.0);
    }

    #[test]
    fn success_branches_increment_reps_only() {
        let mut s = new_state();
        for r in [Rating::Hard, Rating::Good, Rating::Easy] {
            s = schedule(&s, r, NOW);
            assert_eq!(s.lapses, 0);
        }
        assert_eq!(s.reps, 3);
    }

    #[test]
    fn due_always_in_future_for_success() {
        for r in [Rating::Hard, Rating::Good, Rating::Easy] {
            let s = schedule(&new_state(), r, NOW);
            assert!(s.due_at > NOW);
        }
    }

    #[test]
    fn serialize_rating_roundtrip() {
        let j = serde_json::to_string(&Rating::Again).unwrap();
        assert_eq!(j, "\"again\"");
        let back: Rating = serde_json::from_str("\"easy\"").unwrap();
        assert_eq!(back, Rating::Easy);
    }
}
