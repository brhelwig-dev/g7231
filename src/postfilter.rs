//! Decoder postfilters: the pitch postfilter (3.6), the formant postfilter (3.8) and the gain
//! scaling unit (3.9).

use crate::tables::{
    PITCH_POSTFILTER_GAIN_WEIGHTS, POSTFILTER_POLE_FACTORS, POSTFILTER_ZERO_FACTORS,
};
use crate::{EXC_MEMORY, FRAME_LEN, LPC_ORDER, MIN_ENERGY, Rate, SUBFRAME_LEN, dot};

/// The pitch postfilter of one subframe: ppf(n) = scale e(n) + gain e(n + delay).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PitchPostfilter {
    pub delay: isize,
    pub gain: f64,
    pub scale: f64,
}

impl PitchPostfilter {
    /// Finds the postfilter of the subframe starting at `start` from the excitation `buf`,
    /// which holds the excitation memory followed by the frame.
    pub(crate) fn search(buf: &[f64], start: usize, pair_lag: usize, rate: Rate) -> Self {
        let base = EXC_MEMORY + start;
        let current = &buf[base..base + SUBFRAME_LEN];
        let shifted = |m: isize| {
            let from = (base as isize + m) as usize;
            &buf[from..from + SUBFRAME_LEN]
        };
        let corr = |m: isize| dot(current, shifted(m));
        let energy = |m: isize| dot(shifted(m), shifted(m));
        let lags = pair_lag.min(142) - 3..=pair_lag.min(142) + 3;

        let mut backward = None;
        let mut best = 0.0;
        for m in lags.clone() {
            let c = corr(-(m as isize));
            if c > best {
                best = c;
                backward = Some((-(m as isize), c));
            }
        }
        let mut forward = None;
        best = 0.0;
        for m in lags {
            if start + SUBFRAME_LEN + m > FRAME_LEN {
                break;
            }
            let c = corr(m as isize);
            if c > best {
                best = c;
                forward = Some((m as isize, c));
            }
        }

        let (delay, c, d) = match (backward, forward) {
            (None, None) => {
                return Self {
                    delay: 0,
                    gain: 0.0,
                    scale: 1.0,
                };
            }
            (Some((m, c)), None) | (None, Some((m, c))) => (m, c, energy(m)),
            (Some((mb, cb)), Some((mf, cf))) => {
                let db = energy(mb);
                let df = energy(mf);
                if df * cb * cb > db * cf * cf {
                    (mb, cb, db)
                } else {
                    (mf, cf, df)
                }
            }
        };
        let ten = energy(0);
        if c * c <= ten * d / 4.0 {
            return Self {
                delay,
                gain: 0.0,
                scale: 1.0,
            };
        }
        let weight = PITCH_POSTFILTER_GAIN_WEIGHTS[rate.index()];
        let g = if c >= d { weight } else { weight * (c / d) };
        let filtered_energy = ten + 2.0 * c * g + g * g * d;
        let scale = if filtered_energy.abs() < MIN_ENERGY {
            0.0
        } else {
            (ten / filtered_energy).sqrt()
        };
        Self {
            delay,
            gain: g * scale,
            scale,
        }
    }

    /// Filters the subframe starting at `start` of `buf` into `out`.
    pub(crate) fn apply(&self, buf: &[f64], start: usize, out: &mut [f64]) {
        let base = EXC_MEMORY + start;
        for (n, y) in out.iter_mut().enumerate().take(SUBFRAME_LEN) {
            let delayed = buf[(base as isize + n as isize + self.delay) as usize];
            *y = self.scale * buf[base + n] + self.gain * delayed;
        }
    }
}

/// The formant postfilter and the gain scaling unit, with the state they keep across
/// subframes and frames.
#[derive(Clone, Debug)]
pub(crate) struct FormantPostfilter {
    /// Last inputs, most recent first.
    fir_memory: [f64; LPC_ORDER],
    /// Last outputs of the pole-zero section, most recent first.
    iir_memory: [f64; LPC_ORDER],
    k1: f64,
    gain: f64,
}

impl Default for FormantPostfilter {
    fn default() -> Self {
        Self {
            fir_memory: [0.0; LPC_ORDER],
            iir_memory: [0.0; LPC_ORDER],
            k1: 0.0,
            gain: 1.0,
        }
    }
}

impl FormantPostfilter {
    /// Postfilters and rescales one subframe of synthesized speech in place, with the LPC
    /// coefficients a(1) to a(10) of that subframe.
    pub(crate) fn process(&mut self, a: &[f64; LPC_ORDER], sy: &mut [f64; SUBFRAME_LEN]) {
        let r0 = dot(sy, sy);
        let r1 = dot(&sy[..SUBFRAME_LEN - 1], &sy[1..]);
        let k = if r0 > MIN_ENERGY { r1 / r0 } else { 0.0 };
        self.k1 = 0.75 * self.k1 + 0.25 * k;
        let tilt = -0.25 * self.k1;
        let fir: [f64; LPC_ORDER] = std::array::from_fn(|j| a[j] * POSTFILTER_ZERO_FACTORS[j]);
        let iir: [f64; LPC_ORDER] = std::array::from_fn(|j| a[j] * POSTFILTER_POLE_FACTORS[j]);

        for x in sy.iter_mut() {
            let y = *x - dot(&fir, &self.fir_memory) + dot(&iir, &self.iir_memory);
            let previous = self.iir_memory[0];
            self.fir_memory.copy_within(0..LPC_ORDER - 1, 1);
            self.fir_memory[0] = *x;
            self.iir_memory.copy_within(0..LPC_ORDER - 1, 1);
            self.iir_memory[0] = y;
            *x = y + tilt * previous;
        }

        let r_post = dot(sy, sy);
        let step = if r_post > MIN_ENERGY {
            (r0 / r_post).sqrt() * 0.0625
        } else {
            0.0625
        };
        for x in sy.iter_mut() {
            self.gain = 0.9375 * self.gain + step;
            *x = (1.0625 * *x) * self.gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn periodic_buffer(period: usize) -> Vec<f64> {
        (0..EXC_MEMORY + FRAME_LEN)
            .map(|i| if i % period == 0 { 1000.0 } else { 10.0 })
            .collect()
    }

    #[test]
    fn pitch_postfilter_locks_on_the_period() {
        let buf = periodic_buffer(40);
        let pf = PitchPostfilter::search(&buf, 0, 41, Rate::R63);
        assert_eq!(pf.delay.abs(), 40);
        assert!(pf.gain > 0.0 && pf.scale < 1.0);
    }

    #[test]
    fn pitch_postfilter_keeps_forward_delays_inside_the_frame() {
        let buf = periodic_buffer(40);
        let pf = PitchPostfilter::search(&buf, 180, 40, Rate::R53);
        assert!(pf.delay < 0);
    }

    #[test]
    fn pitch_postfilter_is_off_for_noise_like_input() {
        let mut state = 0x2545_f491_u32;
        let buf: Vec<f64> = (0..EXC_MEMORY + FRAME_LEN)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                f64::from(state % 2001) - 1000.0
            })
            .collect();
        let pf = PitchPostfilter::search(&buf, 60, 100, Rate::R63);
        assert_eq!((pf.gain, pf.scale), (0.0, 1.0));
    }

    #[test]
    fn pitch_postfilter_preserves_energy() {
        let buf = periodic_buffer(50);
        let pf = PitchPostfilter::search(&buf, 120, 50, Rate::R53);
        let mut out = [0.0; SUBFRAME_LEN];
        pf.apply(&buf, 120, &mut out);
        let before: f64 = buf[EXC_MEMORY + 120..EXC_MEMORY + 180].iter().map(|x| x * x).sum();
        let after: f64 = out.iter().map(|x| x * x).sum();
        assert!((before - after).abs() < 1e-6 * before);
    }

    #[test]
    fn gain_scaling_tracks_the_input_energy() {
        let mut pf = FormantPostfilter::default();
        let a = [0.9, -0.2, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let mut energy = (0.0, 0.0);
        for frame in 0..50 {
            let mut sy: [f64; SUBFRAME_LEN] =
                std::array::from_fn(|n| 1000.0 * ((frame * 60 + n) as f64 * 0.3).sin());
            energy.0 = sy.iter().map(|x| x * x).sum::<f64>();
            pf.process(&a, &mut sy);
            energy.1 = sy.iter().map(|x| x * x).sum::<f64>();
        }
        let ratio = energy.1 / energy.0;
        assert!((0.8..1.25).contains(&ratio), "ratio {ratio}");
    }

    #[test]
    fn silence_stays_silent() {
        let mut pf = FormantPostfilter::default();
        let mut sy = [0.0; SUBFRAME_LEN];
        pf.process(&[0.5; LPC_ORDER], &mut sy);
        assert_eq!(sy, [0.0; SUBFRAME_LEN]);
    }
}
