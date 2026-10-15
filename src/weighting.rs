//! Perceptual weighting (clause 2.8), open-loop pitch estimation (2.9), harmonic noise
//! shaping (2.11) and the combined filter of the subframe loop (2.12, 2.13, 2.19).

use crate::lsp::Lpc;
use crate::tables::{WEIGHTING_FILTER_POLE_FACTORS, WEIGHTING_FILTER_ZERO_FACTORS};
use crate::{EXC_MEMORY, LPC_ORDER, SUBFRAME_LEN, dot};

/// Formant-weighted speech: the history followed by the current frame.
pub(crate) const WEIGHTED_LEN: usize = EXC_MEMORY + crate::FRAME_LEN;
const MIN_LAG: usize = 18;
const MAX_OPEN_LOOP_LAG: usize = 142;

fn push(memory: &mut [f64; LPC_ORDER], value: f64) {
    memory.copy_within(0..LPC_ORDER - 1, 1);
    memory[0] = value;
}

/// Coefficients of the formant perceptual weighting filter W(z) of one subframe.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Weights {
    numerator: Lpc,
    denominator: Lpc,
}

impl Weights {
    pub(crate) fn new(a: &Lpc) -> Self {
        Self {
            numerator: std::array::from_fn(|j| a[j] * WEIGHTING_FILTER_ZERO_FACTORS[j]),
            denominator: std::array::from_fn(|j| a[j] * WEIGHTING_FILTER_POLE_FACTORS[j]),
        }
    }
}

/// The weighting filter applied to the whole frame, with its memory.
#[derive(Clone, Debug, Default)]
pub(crate) struct WeightingFilter {
    fir: [f64; LPC_ORDER],
    iir: [f64; LPC_ORDER],
}

impl WeightingFilter {
    pub(crate) fn apply(&mut self, w: &Weights, x: &mut [f64]) {
        for sample in x {
            let c = *sample;
            let mut y = c - dot(&w.numerator, &self.fir);
            push(&mut self.fir, c);
            y += dot(&w.denominator, &self.iir);
            push(&mut self.iir, y);
            *sample = y;
        }
    }
}

/// Open-loop pitch estimate of the 120 samples of `g` that start at `start` (equation 12).
pub(crate) fn open_loop_pitch(g: &[f64; WEIGHTED_LEN], start: usize) -> usize {
    let mut best_lag = MIN_LAG;
    let mut best_energy = 1.0;
    let mut best_corr2 = 0.0;
    let current = &g[start..start + 120];
    let first = &g[start - (MIN_LAG - 1)..start - (MIN_LAG - 1) + 120];
    let mut energy = dot(first, first);
    for j in MIN_LAG..=MAX_OPEN_LOOP_LAG {
        let leaving = g[start - j + 120];
        let entering = g[start - j];
        energy = (energy - leaving * leaving) + entering * entering;
        let c = dot(current, &g[start - j..start - j + 120]);
        let c2 = c * c;
        let d = c2 * best_energy - energy * best_corr2;
        if energy > 0.0
            && c > 0.0
            && ((d > 0.0 && j - best_lag < MIN_LAG) || d > 0.25 * c2 * best_energy)
        {
            best_lag = j;
            best_energy = energy;
            best_corr2 = c2;
        }
    }
    best_lag
}

/// The harmonic noise shaping filter P(z) = 1 - gain z^-lag of one subframe (equation 13).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct HarmonicFilter {
    pub lag: usize,
    pub gain: f64,
}

impl HarmonicFilter {
    /// Finds the filter of the subframe of `g` starting at `start`, around the open-loop lag.
    pub(crate) fn new(g: &[f64; WEIGHTED_LEN], start: usize, open_loop_lag: usize) -> Self {
        let current = &g[start..start + SUBFRAME_LEN];
        let total = dot(current, current);
        let mut accepted = None;
        let (mut best_energy, mut best_corr, mut best_corr2) = (1.0, 0.0, 0.0);
        for i in 0..7 {
            let lag = open_loop_lag - 3 + i;
            let past = &g[start - lag..start - lag + SUBFRAME_LEN];
            let c = dot(current, past);
            let e = dot(past, past);
            if e > 0.0 && c > 0.0 {
                let c2 = c * c;
                if c2 * best_energy > e * best_corr2 {
                    accepted = Some(lag);
                    best_energy = e;
                    best_corr = c;
                    best_corr2 = c2;
                }
            }
        }
        let Some(lag) = accepted else {
            return Self {
                lag: open_loop_lag,
                gain: 0.0,
            };
        };
        let gain = if best_corr2 > (best_energy * total) * 0.375 {
            let optimum = if best_corr > best_energy {
                1.0
            } else {
                best_corr / best_energy
            };
            0.3125 * optimum
        } else {
            0.0
        };
        Self { lag, gain }
    }

    /// The target w(n) = f(n) - gain f(n - lag) of the subframe of `g` starting at `start`.
    pub(crate) fn target(&self, g: &[f64; WEIGHTED_LEN], start: usize) -> [f64; SUBFRAME_LEN] {
        std::array::from_fn(|n| g[start + n] - self.gain * g[start + n - self.lag])
    }
}

/// The filters of one subframe: quantized synthesis, formant weighting and harmonic noise
/// shaping (equation 19).
pub(crate) struct Combined<'a> {
    pub quantized: &'a Lpc,
    pub weights: &'a Weights,
    pub harmonic: HarmonicFilter,
}

impl Combined<'_> {
    fn step(&self, y: &mut [f64; LPC_ORDER], z: &mut [f64; LPC_ORDER], x: f64) -> f64 {
        let ys = x + dot(self.quantized, y);
        let mut q = ys - dot(&self.weights.numerator, y);
        push(y, ys);
        q += dot(&self.weights.denominator, z);
        push(z, q);
        q
    }

    /// Impulse response h(n) of the combined filter.
    pub(crate) fn impulse_response(&self) -> [f64; SUBFRAME_LEN] {
        let (mut y, mut z) = ([0.0; LPC_ORDER], [0.0; LPC_ORDER]);
        let mut out = [0.0; SUBFRAME_LEN];
        for n in 0..SUBFRAME_LEN {
            out[n] = self.step(&mut y, &mut z, if n == 0 { 1.0 } else { 0.0 });
        }
        let mut h = [0.0; SUBFRAME_LEN];
        for n in 0..SUBFRAME_LEN {
            let delayed = if n >= self.harmonic.lag {
                out[n - self.harmonic.lag]
            } else {
                0.0
            };
            h[n] = out[n] - self.harmonic.gain * delayed;
        }
        h
    }
}

/// The memory of the combined filter between subframes.
#[derive(Clone, Debug)]
pub(crate) struct CombinedMemory {
    fir: [f64; LPC_ORDER],
    iir: [f64; LPC_ORDER],
    /// Outputs before the harmonic term, oldest first.
    ringing: [f64; EXC_MEMORY],
}

impl Default for CombinedMemory {
    fn default() -> Self {
        Self {
            fir: [0.0; LPC_ORDER],
            iir: [0.0; LPC_ORDER],
            ringing: [0.0; EXC_MEMORY],
        }
    }
}

impl CombinedMemory {
    /// Subtracts the zero input response of the combined filter from the target in place.
    pub(crate) fn subtract_ringing(&self, filter: &Combined, target: &mut [f64; SUBFRAME_LEN]) {
        let (mut y, mut z) = (self.fir, self.iir);
        let mut out = [0.0; EXC_MEMORY + SUBFRAME_LEN];
        out[..EXC_MEMORY].copy_from_slice(&self.ringing);
        for n in 0..SUBFRAME_LEN {
            let q = filter.step(&mut y, &mut z, 0.0);
            out[EXC_MEMORY + n] = q;
            let zero_input = q - filter.harmonic.gain * out[EXC_MEMORY + n - filter.harmonic.lag];
            target[n] -= zero_input;
        }
    }

    /// Runs the excitation of the subframe through the combined filter to update its memory.
    pub(crate) fn update(&mut self, filter: &Combined, excitation: &[f64; SUBFRAME_LEN]) {
        self.ringing.copy_within(SUBFRAME_LEN.., 0);
        for n in 0..SUBFRAME_LEN {
            let q = filter.step(&mut self.fir, &mut self.iir, excitation[n]);
            self.ringing[EXC_MEMORY - SUBFRAME_LEN + n] = q;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shaping(lag: usize, gain: f64) -> HarmonicFilter {
        HarmonicFilter { lag, gain }
    }

    fn periodic(period: usize) -> [f64; WEIGHTED_LEN] {
        std::array::from_fn(|i| {
            let phase = (i % period) as f64 / period as f64;
            1000.0 * (2.0 * std::f64::consts::PI * phase).sin() + 300.0 * (phase - 0.5)
        })
    }

    #[test]
    fn open_loop_pitch_finds_the_period() {
        let g = periodic(53);
        assert_eq!(open_loop_pitch(&g, EXC_MEMORY), 53);
        assert_eq!(open_loop_pitch(&g, EXC_MEMORY + 120), 53);
    }

    #[test]
    fn open_loop_pitch_prefers_the_fundamental() {
        let g = periodic(40);
        assert_eq!(open_loop_pitch(&g, EXC_MEMORY), 40);
    }

    #[test]
    fn harmonic_filter_locks_on_the_period() {
        let g = periodic(61);
        let hf = HarmonicFilter::new(&g, EXC_MEMORY + 60, 60);
        assert_eq!(hf.lag, 61);
        assert_eq!(hf.gain, 0.3125);
    }

    #[test]
    fn impulse_response_starts_at_one() {
        let a = [0.5, -0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let weights = Weights::new(&a);
        let filter = Combined {
            quantized: &a,
            weights: &weights,
            harmonic: shaping(20, 0.2),
        };
        let h = filter.impulse_response();
        assert_eq!(h[0], 1.0);
        let without = Combined {
            harmonic: shaping(20, 0.0),
            ..filter
        };
        let h0 = without.impulse_response();
        assert!((h[25] - (h0[25] - 0.2 * h0[5])).abs() < 1e-12);
    }

    #[test]
    fn ringing_is_the_continuation_of_the_last_excitation() {
        let a = [0.9, -0.3, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let weights = Weights::new(&a);
        let filter = Combined {
            quantized: &a,
            weights: &weights,
            harmonic: shaping(30, 0.0),
        };
        let mut memory = CombinedMemory::default();
        let mut pulse = [0.0; SUBFRAME_LEN];
        pulse[0] = 1.0;
        memory.update(&filter, &pulse);
        let h = filter.impulse_response();
        let mut target = [0.0; SUBFRAME_LEN];
        memory.subtract_ringing(&filter, &mut target);
        let mut full = [0.0; 2 * SUBFRAME_LEN];
        let (mut y, mut z) = ([0.0; LPC_ORDER], [0.0; LPC_ORDER]);
        for n in 0..2 * SUBFRAME_LEN {
            full[n] = filter.step(&mut y, &mut z, if n == 0 { 1.0 } else { 0.0 });
        }
        assert!((full[0] - h[0]).abs() < 1e-12);
        for n in 0..SUBFRAME_LEN {
            assert!((target[n] + full[SUBFRAME_LEN + n]).abs() < 1e-9);
        }
    }
}
