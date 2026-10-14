// Default-parameter scalar inference adapted from fsrs-rs v6.6.2.
// Copyright (c) 2023, Open Spaced Repetition
// SPDX-License-Identifier: BSD-3-Clause
// Original source: https://github.com/open-spaced-repetition/fsrs-rs/tree/v6.6.2
// Full license: third_party_licenses/fsrs_bsd_3_clause.txt.
//
// Keep the upstream update equations, f32 arithmetic and clamps. Try supplies
// integer elapsed days, binary Again/Good, and its own selection policy; this
// module does not contain parameter training or the upstream scheduling system.

use serde::{Deserialize, Serialize};

pub(crate) const MODEL_VERSION: &str = "fsrs6_default_v6_6_2";
const W: [f32; 21] = [
    0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001, 1.8722, 0.1666, 0.796, 1.4835,
    0.0614, 0.2629, 1.6483, 0.6014, 1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
];
const S_MIN: f32 = 0.001;
const S_MAX: f32 = 36500.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Memory {
    pub stability: f32,
    pub difficulty: f32,
}

impl Memory {
    pub fn valid(self) -> bool {
        self.stability.is_finite()
            && self.difficulty.is_finite()
            && (S_MIN..=S_MAX).contains(&self.stability)
            && (1.0..=10.0).contains(&self.difficulty)
    }
}

fn init_difficulty(rating: f32) -> f32 {
    W[4] - (W[5] * (rating - 1.0)).exp() + 1.0
}

fn curve(days: f32, stability: f32) -> f32 {
    let decay = -W[20];
    let factor = (0.9_f32.ln() / decay).exp() - 1.0;
    (days / stability * factor + 1.0).powf(decay)
}

pub(crate) fn retrievability(memory: Memory, seconds: i64) -> f64 {
    f64::from(curve(seconds.max(0) as f32 / 86400.0, memory.stability))
}

pub(crate) fn interval_days(memory: Memory) -> f64 {
    // S is the interval at R = 0.9. This is a model input for soft selection,
    // not a promise that Try's fixed reminder budget maintains 90% recall.
    f64::from(memory.stability)
}

pub(crate) fn reviewed(previous: Option<Memory>, elapsed_days: u32, correct: bool) -> Memory {
    let rating = if correct { 3.0 } else { 1.0 };
    let Some(previous) = previous else {
        return Memory {
            stability: W[if correct { 2 } else { 0 }],
            difficulty: init_difficulty(rating).clamp(1.0, 10.0),
        };
    };
    let s = previous.stability.clamp(S_MIN, S_MAX);
    let d = previous.difficulty.clamp(1.0, 10.0);
    let r = curve(elapsed_days as f32, s);
    let new_s = if elapsed_days == 0 {
        let increase = (W[17] * (rating - 3.0 + W[18])).exp() * s.powf(-W[19]);
        s * if correct { increase.max(1.0) } else { increase }
    } else if correct {
        s * (W[8].exp() * (11.0 - d) * s.powf(-W[9]) * (((1.0 - r) * W[10]).exp() - 1.0) + 1.0)
    } else {
        let raw =
            W[11] * d.powf(-W[12]) * ((s + 1.0).powf(W[13]) - 1.0) * ((1.0 - r) * W[14]).exp();
        raw.min(s / (W[17] * W[18]).exp())
    };
    let delta_d = -W[6] * (rating - 3.0);
    let next_d = d + (10.0 - d) * delta_d / 9.0;
    Memory {
        stability: new_s.clamp(S_MIN, S_MAX),
        difficulty: (W[7] * (init_difficulty(4.0) - next_d) + next_d).clamp(1.0, 10.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_upstream_default_sequence() {
        // fsrs-rs v6.6.2 inference.rs assert_memory_state(DEFAULT_PARAMETERS).
        let mut state = None;
        for (days, correct) in [
            (0, false),
            (0, true),
            (1, true),
            (3, true),
            (8, true),
            (21, true),
        ] {
            state = Some(reviewed(state, days, correct));
        }
        let state = state.unwrap();
        assert!((state.stability - 53.62691).abs() < 0.0001, "{state:?}");
        assert!((state.difficulty - 6.3574867).abs() < 0.0001, "{state:?}");
    }

    #[test]
    fn fractions_affect_prediction_but_same_day_success_does_not_invent_growth() {
        let initial = reviewed(None, 0, true);
        let mut repeated = initial;
        for _ in 0..32 {
            repeated = reviewed(Some(repeated), 0, true);
        }
        assert_eq!(repeated.stability, initial.stability);
        assert!(retrievability(initial, 1800) > retrievability(initial, 21600));
        assert!(
            (retrievability(initial, (initial.stability * 86400.0) as i64) - 0.9).abs() < 0.00001
        );
        assert!(reviewed(Some(initial), 1, true).stability > initial.stability);
        assert!(reviewed(Some(initial), 1, false).stability < initial.stability);
    }

    #[test]
    fn long_and_repeated_failures_stay_finite_and_within_upstream_bounds() {
        let mut state = reviewed(None, 0, true);
        for i in 0..400 {
            state = reviewed(Some(state), if i % 2 == 0 { 0 } else { 36500 }, i % 3 == 0);
            assert!(state.valid(), "{state:?}");
        }
    }
}
