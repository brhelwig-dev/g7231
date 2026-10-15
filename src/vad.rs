//! Voice activity detector (Annex A.2).

use crate::lsp::Lpc;
use crate::tables::VAD_THRESHOLD_SCALE;
use crate::{FRAME_LEN, LPC_ORDER};

const FIRST_SAMPLE: usize = 60;

#[derive(Clone, Debug)]
pub(crate) struct Vad {
    hangover: i32,
    burst: i32,
    previous_energy: f64,
    noise_level: f64,
    adaptation: i32,
    /// Open-loop lags of the two frames before the current one, oldest first.
    lags: [usize; 4],
    /// Inverse filter of the background noise.
    pub noise_filter: Lpc,
}

impl Default for Vad {
    fn default() -> Self {
        Self {
            hangover: 3,
            burst: 0,
            previous_energy: 1024.0,
            noise_level: 1024.0,
            adaptation: 0,
            lags: [1, 1, 60, 60],
            noise_filter: [0.0; LPC_ORDER],
        }
    }
}

/// Splits a positive normal number into a fraction in [0.5, 1) and a power of two.
fn frexp(x: f64) -> (f64, i32) {
    let bits = x.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1022;
    let fraction = f64::from_bits((bits & !(0x7ff << 52)) | (1022 << 52));
    (fraction, exponent)
}

impl Vad {
    /// Whether the adaptation counter is at rest, which lets comfort noise update the
    /// noise filter.
    pub(crate) fn adapting(&self) -> bool {
        self.adaptation == 0
    }

    /// Decides whether the high-pass filtered frame `s` holds speech.
    pub(crate) fn detect(&mut self, s: &[f64; FRAME_LEN], sine_detected: bool) -> bool {
        let shortest = self.lags.iter().copied().fold(145, usize::min);
        let mut matches = 0;
        for &lag in &self.lags {
            for j in 1..=8 {
                if (j * shortest).abs_diff(lag) <= 3 {
                    matches += 1;
                }
            }
        }
        if matches == 4 || sine_detected {
            self.adaptation += 2;
        } else {
            self.adaptation -= 1;
        }
        self.adaptation = self.adaptation.clamp(0, 6);

        let mut sum = 0.0;
        for n in FIRST_SAMPLE..FRAME_LEN {
            let mut e = s[n];
            for j in 0..LPC_ORDER {
                e -= self.noise_filter[j] * s[n - 1 - j];
            }
            sum += e * e;
        }
        let energy = 0.5 * (sum / 180.0);

        if self.noise_level > self.previous_energy {
            self.noise_level = 0.25 * self.noise_level + 0.75 * self.previous_energy;
        }
        if self.adaptation == 0 {
            self.noise_level *= 33.0 / 32.0;
        } else {
            self.noise_level *= 2047.0 / 2048.0;
        }
        self.previous_energy = energy;
        self.noise_level = self.noise_level.clamp(128.0, 131071.0);

        let (fraction, exponent) = frexp(self.noise_level);
        let t = (fraction * 128.0).trunc() / 64.0 - 1.0;
        let upper = VAD_THRESHOLD_SCALE[(18 - exponent) as usize];
        let lower = VAD_THRESHOLD_SCALE[(17 - exponent) as usize];
        let scale = (1.0 - t) * upper + t * lower;
        let threshold = (scale * self.noise_level) / 4096.0;
        let raw = threshold <= energy;

        if raw {
            self.burst += 1;
            self.hangover += 1;
        } else {
            self.burst = (self.burst - 1).max(0);
        }
        if self.burst >= 2 {
            self.hangover = 6;
            if self.burst >= 3 {
                self.burst = 3;
            }
        }
        let active = if self.hangover != 0 {
            if self.burst == 0 {
                self.hangover -= 1;
            }
            true
        } else {
            raw
        };
        self.lags[0] = self.lags[2];
        self.lags[1] = self.lags[3];
        active
    }

    /// Records the open-loop lags of the current frame.
    pub(crate) fn set_lags(&mut self, lags: [usize; 2]) {
        self.lags[2] = lags[0];
        self.lags[3] = lags[1];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frexp_matches_the_definition() {
        assert_eq!(frexp(1024.0), (0.5, 11));
        assert_eq!(frexp(131071.0), (131071.0 / 131072.0, 17));
        assert_eq!(frexp(128.0), (0.5, 8));
        assert_eq!(frexp(1056.0), (1056.0 / 2048.0, 11));
    }

    #[test]
    fn first_frames_follow_the_reference_check_values() {
        let mut vad = Vad::default();
        let quiet = [0.0; FRAME_LEN];
        let decisions: Vec<bool> = (0..5).map(|_| vad.detect(&quiet, false)).collect();
        assert_eq!(decisions, [true, true, true, false, false]);
    }

    #[test]
    fn loud_frames_are_active() {
        let mut vad = Vad::default();
        let loud: [f64; FRAME_LEN] = std::array::from_fn(|n| 3000.0 * (n as f64 * 1.3).sin());
        for _ in 0..10 {
            assert!(vad.detect(&loud, false));
        }
    }
}
