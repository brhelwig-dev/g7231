//! The five-tap pitch predictor (clauses 2.14 and 2.18) and the encoder's limit on its
//! gains ("taming").

use crate::bitstream::GainCode;
use crate::tables::{
    PITCH_GAIN_CODEBOOK_85, PITCH_GAIN_CODEBOOK_170, TAMING_GAIN_BOUNDS_85, TAMING_GAIN_BOUNDS_170,
};
use crate::{Rate, SUBFRAME_LEN, dot};

const TAPS: usize = 5;
const DELAYED_LEN: usize = SUBFRAME_LEN + TAPS - 1;

/// The pitch gain codebook serving a subframe pair with this lag: rows of five tap gains,
/// their negated squares and the negated products of tap pairs.
pub(crate) fn gain_codebook(rate: Rate, pair_lag: usize) -> &'static [[f64; 20]] {
    if GainCode::uses_small_codebook(rate, pair_lag) {
        &PITCH_GAIN_CODEBOOK_85
    } else {
        &PITCH_GAIN_CODEBOOK_170
    }
}

/// The excitation delayed by `lag`, two samples ahead of the subframe start, repeated with
/// period `lag` where it would reach into the subframe (equation 41.1). `past` ends at the
/// subframe start.
fn delayed_excitation(past: &[f64], lag: usize) -> [f64; DELAYED_LEN] {
    let end = past.len();
    let mut rz = [0.0; DELAYED_LEN];
    rz[0] = past[end - lag - 2];
    rz[1] = past[end - lag - 1];
    for i in 0..DELAYED_LEN - 2 {
        rz[2 + i] = past[end - lag + i % lag];
    }
    rz
}

/// The adaptive codebook contribution of a subframe (equation 41.2).
pub(crate) fn adaptive_vector(past: &[f64], lag: usize, row: &[f64; 20]) -> [f64; SUBFRAME_LEN] {
    let rz = delayed_excitation(past, lag);
    std::array::from_fn(|n| dot(&rz[n..n + TAPS], &row[..TAPS]))
}

/// Result of the closed-loop pitch search of one subframe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PitchChoice {
    /// For even subframes the new pair lag; for odd subframes the pair lag unchanged.
    pub pair_lag: usize,
    /// Lag offset code: the lag used is `pair_lag + offset - 1`.
    pub offset: usize,
    /// Row of the pitch gain codebook.
    pub gain_index: usize,
}

impl PitchChoice {
    pub(crate) fn lag(&self) -> usize {
        self.pair_lag + self.offset - 1
    }
}

/// Closed-loop search of lag and gains for subframe `k`. `lag_base` is the open-loop lag
/// for even subframes and the pair lag for odd ones.
#[allow(clippy::too_many_arguments)]
pub(crate) fn search(
    target: &[f64; SUBFRAME_LEN],
    h: &[f64; SUBFRAME_LEN],
    past: &[f64],
    k: usize,
    lag_base: usize,
    rate: Rate,
    taming: &Taming,
    sine_detected: bool,
) -> PitchChoice {
    let even = k % 2 == 0;
    let (base, candidates) = if even {
        (lag_base.clamp(19, 140), 3)
    } else {
        (lag_base, 4)
    };
    let level = taming.limit(base - 1, base + candidates - 2, sine_detected);
    let bound85 = (51 + 4 * level).min(85);
    let bound170 = (93 + 8 * level).min(170);

    let mut best = 0.0;
    let mut gain_index = 0;
    let mut offset = 1;
    for d in 0..candidates {
        let lag = base - 1 + d;
        let fv = filtered_taps(&delayed_excitation(past, lag), h);
        let cv = correlations(target, &fv);
        let codebook_lag = if even { lag } else { base };
        let (codebook, bound) = if GainCode::uses_small_codebook(rate, codebook_lag) {
            (&PITCH_GAIN_CODEBOOK_85[..], bound85)
        } else {
            (&PITCH_GAIN_CODEBOOK_170[..], bound170)
        };
        for (i, row) in codebook[..bound].iter().enumerate() {
            let score = dot(&cv, row);
            if score > best {
                best = score;
                gain_index = i;
                offset = d;
            }
        }
    }
    if even {
        PitchChoice {
            pair_lag: base - 1 + offset,
            offset: 1,
            gain_index,
        }
    } else {
        PitchChoice {
            pair_lag: base,
            offset,
            gain_index,
        }
    }
}

/// The five delayed excitation vectors filtered by h.
fn filtered_taps(rz: &[f64; DELAYED_LEN], h: &[f64; SUBFRAME_LEN]) -> [[f64; SUBFRAME_LEN]; TAPS] {
    let mut fv = [[0.0; SUBFRAME_LEN]; TAPS];
    for n in 0..SUBFRAME_LEN {
        let mut sum = 0.0;
        for j in 0..=n {
            sum += rz[TAPS - 1 + j] * h[n - j];
        }
        fv[TAPS - 1][n] = sum;
    }
    for m in (0..TAPS - 1).rev() {
        fv[m][0] = rz[m];
        for n in 1..SUBFRAME_LEN {
            fv[m][n] = rz[m] * h[n] + fv[m + 1][n - 1];
        }
    }
    fv
}

/// Correlations of the target with the filtered taps, their half energies and their
/// cross products, in the column order of the pitch gain codebooks.
fn correlations(target: &[f64; SUBFRAME_LEN], fv: &[[f64; SUBFRAME_LEN]; TAPS]) -> [f64; 20] {
    let mut cv = [0.0; 20];
    for m in 0..TAPS {
        cv[m] = dot(target, &fv[m]);
        cv[TAPS + m] = 0.5 * dot(&fv[m], &fv[m]);
    }
    let mut i = 2 * TAPS;
    for m in 1..TAPS {
        for q in 0..m {
            cv[i] = dot(&fv[m], &fv[q]);
            i += 1;
        }
    }
    cv
}

/// The target minus the adaptive contribution filtered by h (equation 20).
pub(crate) fn residual(
    target: &[f64; SUBFRAME_LEN],
    contribution: &[f64; SUBFRAME_LEN],
    h: &[f64; SUBFRAME_LEN],
) -> [f64; SUBFRAME_LEN] {
    std::array::from_fn(|n| {
        let mut r = target[n];
        for j in 0..=n {
            r -= contribution[j] * h[n - j];
        }
        r
    })
}

const INITIAL_ERROR: f64 = 0.00000381464;

/// Worst-case growth estimates of the excitation over five stretches of 30 samples, newest
/// first, which limit the pitch gains the encoder may choose.
#[derive(Clone, Debug)]
pub(crate) struct Taming {
    error: [f64; 5],
}

impl Default for Taming {
    fn default() -> Self {
        Self {
            error: [INITIAL_ERROR; 5],
        }
    }
}

impl Taming {
    /// The limit level for a search over lags `lag1` to `lag2`: 0 is the most restrictive.
    fn limit(&self, lag1: usize, lag2: usize, sine_detected: bool) -> usize {
        let zone2 = (lag2 + 2) / 30;
        let zone1 = (lag1 as isize - 61).max(1) as usize / 30;
        let mut worst = -1.0;
        for z in (zone1..=zone2).rev() {
            if self.error[z] > worst {
                worst = self.error[z];
            }
        }
        if worst > 128.0 || sine_detected {
            0
        } else {
            (128.0 - worst) as usize
        }
    }

    /// Updates the estimates after a subframe has been coded.
    pub(crate) fn update(&mut self, choice: &PitchChoice, rate: Rate) {
        let lag = choice.lag();
        let beta = if GainCode::uses_small_codebook(rate, choice.pair_lag) {
            TAMING_GAIN_BOUNDS_85[choice.gain_index]
        } else {
            TAMING_GAIN_BOUNDS_170[choice.gain_index]
        };
        let grow = |e: f64| e * beta + INITIAL_ERROR;
        let (mut w0, mut w1);
        if lag <= 30 {
            w0 = grow(self.error[0]);
            w1 = w0;
        } else {
            let zone = (lag * 1092) >> 15;
            if 30 * (zone + 1) != lag {
                if zone == 1 {
                    w0 = grow(self.error[0]);
                    w1 = grow(self.error[1]);
                    if w1 > w0 {
                        w0 = w1;
                    } else {
                        w1 = w0;
                    }
                } else {
                    let wt = grow(self.error[zone - 1]);
                    w0 = grow(self.error[zone - 2]);
                    if wt > w0 {
                        w0 = wt;
                    }
                    w1 = grow(self.error[zone]);
                    if wt > w1 {
                        w1 = wt;
                    }
                }
            } else {
                w0 = grow(self.error[zone - 1]);
                w1 = grow(self.error[zone]);
            }
        }
        self.error[4] = self.error[2];
        self.error[3] = self.error[1];
        self.error[2] = self.error[0];
        self.error[0] = w0.min(256.0);
        self.error[1] = w1.min(256.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delayed_excitation_repeats_short_lags() {
        let past: Vec<f64> = (0..145).map(f64::from).collect();
        let rz = delayed_excitation(&past, 20);
        assert_eq!(rz[0], 123.0);
        assert_eq!(rz[1], 124.0);
        assert_eq!(rz[2], 125.0);
        assert_eq!(rz[21], 144.0);
        assert_eq!(rz[22], 125.0);
        let long = delayed_excitation(&past, 100);
        assert!(long.windows(2).all(|w| w[1] == w[0] + 1.0));
    }

    #[test]
    fn single_tap_gain_scales_the_delayed_excitation() {
        let past: Vec<f64> = (0..145).map(|i| (i as f64 * 0.37).sin()).collect();
        let mut row = [0.0; 20];
        row[2] = 0.5;
        let u = adaptive_vector(&past, 80, &row);
        for n in 0..SUBFRAME_LEN {
            assert_eq!(u[n], 0.5 * past[145 - 80 + n]);
        }
    }

    #[test]
    fn filtered_taps_are_convolutions() {
        let rz: [f64; DELAYED_LEN] = std::array::from_fn(|i| (i as f64 * 0.7).cos());
        let h: [f64; SUBFRAME_LEN] = std::array::from_fn(|n| 0.8_f64.powi(n as i32));
        let fv = filtered_taps(&rz, &h);
        for m in 0..TAPS {
            for n in [0, 7, 59] {
                let direct: f64 = (0..=n).map(|j| rz[m + j] * h[n - j]).sum();
                assert!((fv[m][n] - direct).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn search_recovers_a_pure_delay() {
        let past: Vec<f64> = (0..145).map(|i| 1000.0 * (i as f64 * 0.21).sin()).collect();
        let mut h = [0.0; SUBFRAME_LEN];
        h[0] = 1.0;
        let target: [f64; SUBFRAME_LEN] = std::array::from_fn(|n| 0.6 * past[145 - 90 + n]);
        let choice = search(
            &target,
            &h,
            &past,
            0,
            90,
            Rate::R53,
            &Taming::default(),
            false,
        );
        assert_eq!(choice.lag(), 90);
        let row = &PITCH_GAIN_CODEBOOK_170[choice.gain_index];
        let u = adaptive_vector(&past, 90, row);
        let error: f64 = (0..SUBFRAME_LEN).map(|n| (target[n] - u[n]).powi(2)).sum();
        let energy: f64 = target.iter().map(|t| t * t).sum();
        assert!(error < 0.1 * energy);
    }

    #[test]
    fn taming_restricts_gains_after_growth() {
        let mut taming = Taming::default();
        assert!(taming.limit(17, 20, false) > 100);
        assert_eq!(taming.limit(17, 20, true), 0);
        let choice = PitchChoice {
            pair_lag: 20,
            offset: 1,
            gain_index: 84,
        };
        for _ in 0..40 {
            taming.update(&choice, Rate::R63);
        }
        assert_eq!(taming.limit(17, 20, false), 0);
    }
}
