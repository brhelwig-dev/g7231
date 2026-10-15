//! High rate fixed codebook: multipulse maximum likelihood quantization (clauses 2.15 and
//! 2.17).

use crate::tables::{
    FIXED_GAIN_LEVELS, MPMLQ_CODEBOOK_SIZES, MPMLQ_COMBINATORIAL_TABLE, MPMLQ_PULSE_COUNTS,
};
use crate::{SUBFRAME_LEN, dot};

/// Pitch lags below this are "short": the pulse train and the 85-entry gain codebook apply.
pub(crate) const SHORT_LAG: usize = 58;
const GRID_SLOTS: usize = 30;
const MAX_PULSES: usize = 6;

/// The coded fixed excitation of one subframe, at either rate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FixedCode {
    /// The excitation vector, scaled by the quantized gain.
    pub vector: [f64; SUBFRAME_LEN],
    pub position: u32,
    pub signs: u8,
    /// Index into FIXED_GAIN_LEVELS.
    pub gain_index: usize,
    pub grid: u8,
    pub train: bool,
}

/// Adds copies of `s` delayed by every multiple of `period` below the subframe length.
pub(crate) fn pulse_train(s: &[f64; SUBFRAME_LEN], period: usize) -> [f64; SUBFRAME_LEN] {
    let mut y = *s;
    for shift in (period..SUBFRAME_LEN).step_by(period) {
        for n in shift..SUBFRAME_LEN {
            y[n] += s[n - shift];
        }
    }
    y
}

#[derive(Clone, Copy)]
struct Candidate {
    error: f64,
    grid: usize,
    gain_index: usize,
    train: bool,
    positions: [usize; MAX_PULSES],
    amplitudes: [f64; MAX_PULSES],
}

/// Searches the high rate codebook for subframe `k` with target `r` and impulse response
/// `h`. `pair_lag` is the transmitted lag of the subframe pair.
pub(crate) fn search(
    r: &[f64; SUBFRAME_LEN],
    h: &[f64; SUBFRAME_LEN],
    k: usize,
    pair_lag: usize,
) -> FixedCode {
    let pulses = usize::from(MPMLQ_PULSE_COUNTS[k]);
    let mut best = Candidate {
        error: -99999999.9,
        grid: 0,
        gain_index: 0,
        train: false,
        positions: [0; MAX_PULSES],
        amplitudes: [0.0; MAX_PULSES],
    };
    find_best(r, h, pulses, SUBFRAME_LEN, &mut best);
    if pair_lag < SHORT_LAG {
        find_best(r, h, pulses, pair_lag, &mut best);
    }

    let mut vector = [0.0; SUBFRAME_LEN];
    for j in 0..pulses {
        vector[best.positions[j]] = best.amplitudes[j];
    }
    let mut row = MAX_PULSES - pulses;
    let mut position = 0;
    let mut signs = 0;
    for slot in 0..GRID_SLOTS {
        let x = vector[best.grid + 2 * slot];
        if x == 0.0 {
            position += MPMLQ_COMBINATORIAL_TABLE[row][slot];
        } else {
            signs = 2 * signs + u8::from(x < 0.0);
            row += 1;
            if row == MAX_PULSES {
                break;
            }
        }
    }
    if best.train {
        vector = pulse_train(&vector, pair_lag);
    }
    FixedCode {
        vector,
        position,
        signs,
        gain_index: best.gain_index,
        grid: best.grid as u8,
        train: best.train,
    }
}

fn find_best(
    r: &[f64; SUBFRAME_LEN],
    h: &[f64; SUBFRAME_LEN],
    pulses: usize,
    period: usize,
    best: &mut Candidate,
) {
    let train = period < SHORT_LAG;
    let g = if train { pulse_train(h, period) } else { *h };
    let c: [f64; SUBFRAME_LEN] = std::array::from_fn(|i| dot(&g[..SUBFRAME_LEN - i], &g[i..]));
    let e: [f64; SUBFRAME_LEN] = std::array::from_fn(|i| dot(&r[i..], &g[..SUBFRAME_LEN - i]));

    for grid in 0..2 {
        let slots = (0..GRID_SLOTS).map(|j| grid + 2 * j);
        let mut largest = 0.0;
        let mut first = grid;
        for p in slots.clone() {
            if e[p].abs() >= largest {
                largest = e[p].abs();
                first = p;
            }
        }
        let mut nearest = 32767.0;
        let mut estimate = 22;
        for q in (2..=22).rev() {
            let distance = (FIXED_GAIN_LEVELS[q] * c[0] - largest).abs();
            if distance < nearest {
                nearest = distance;
                estimate = q;
            }
        }

        for gain_index in estimate - 2..=estimate + 1 {
            let amplitude = FIXED_GAIN_LEVELS[gain_index];
            let signed = |x: f64| if x >= 0.0 { amplitude } else { -amplitude };
            let mut w = e;
            let mut occupied = [false; SUBFRAME_LEN];
            let mut positions = [0; MAX_PULSES];
            let mut amplitudes = [0.0; MAX_PULSES];
            positions[0] = first;
            amplitudes[0] = signed(w[first]);
            occupied[first] = true;
            for j in 1..pulses {
                let mut strongest = -32768.0;
                let mut chosen = grid;
                for p in slots.clone() {
                    if occupied[p] {
                        continue;
                    }
                    w[p] -= amplitudes[j - 1] * c[p.abs_diff(positions[j - 1])];
                    if w[p].abs() > strongest {
                        strongest = w[p].abs();
                        chosen = p;
                    }
                }
                positions[j] = chosen;
                amplitudes[j] = signed(w[chosen]);
                occupied[chosen] = true;
            }

            let mut u = [0.0; SUBFRAME_LEN];
            for j in 0..pulses {
                u[positions[j]] = amplitudes[j];
            }
            let s: [f64; SUBFRAME_LEN] = std::array::from_fn(|l| {
                let mut sum = 0.0;
                for j in 0..=l {
                    sum += u[j] * g[l - j];
                }
                sum
            });
            let error = 2.0 * dot(r, &s) - dot(&s, &s);
            if error > best.error {
                *best = Candidate {
                    error,
                    grid,
                    gain_index,
                    train,
                    positions,
                    amplitudes,
                };
            }
        }
    }
}

/// Decodes the high rate fixed excitation of subframe `k`.
pub(crate) fn decode(
    position: u32,
    signs: u8,
    gain_index: usize,
    grid: u8,
    train: bool,
    k: usize,
    pair_lag: usize,
) -> [f64; SUBFRAME_LEN] {
    let mut v = [0.0; SUBFRAME_LEN];
    if position >= MPMLQ_CODEBOOK_SIZES[k] {
        return v;
    }
    let level = FIXED_GAIN_LEVELS[gain_index];
    let mut row = MAX_PULSES - usize::from(MPMLQ_PULSE_COUNTS[k]);
    let mut rest = i64::from(position);
    for slot in 0..GRID_SLOTS {
        let count = i64::from(MPMLQ_COMBINATORIAL_TABLE[row][slot]);
        rest -= count;
        if rest < 0 {
            rest += count;
            row += 1;
            let negative = signs & (1 << (MAX_PULSES - row)) != 0;
            v[usize::from(grid) + 2 * slot] = if negative { -level } else { level };
            if row == MAX_PULSES {
                break;
            }
        }
    }
    if train { pulse_train(&v, pair_lag) } else { v }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convolve(x: &[f64; SUBFRAME_LEN], h: &[f64; SUBFRAME_LEN]) -> [f64; SUBFRAME_LEN] {
        let mut y = [0.0; SUBFRAME_LEN];
        for n in 0..SUBFRAME_LEN {
            for j in 0..=n {
                y[n] += x[j] * h[n - j];
            }
        }
        y
    }

    fn impulse_response() -> [f64; SUBFRAME_LEN] {
        std::array::from_fn(|n| 0.85_f64.powi(n as i32) * (n as f64 * 0.4).cos())
    }

    #[test]
    fn pulse_train_repeats_the_vector() {
        let mut s = [0.0; SUBFRAME_LEN];
        s[3] = 1.0;
        s[5] = -2.0;
        let y = pulse_train(&s, 25);
        assert_eq!((y[3], y[28], y[53]), (1.0, 1.0, 1.0));
        assert_eq!((y[5], y[30], y[55]), (-2.0, -2.0, -2.0));
        assert_eq!(y.iter().filter(|x| **x != 0.0).count(), 6);
    }

    #[test]
    fn positions_and_signs_round_trip() {
        let h = impulse_response();
        for k in 0..4 {
            let r: [f64; SUBFRAME_LEN] =
                std::array::from_fn(|n| 3000.0 * ((n * (k + 3)) as f64 * 0.9).sin());
            let code = search(&r, &h, k, 100);
            assert!(code.position < MPMLQ_CODEBOOK_SIZES[k]);
            let decoded = decode(
                code.position,
                code.signs,
                code.gain_index,
                code.grid,
                code.train,
                k,
                100,
            );
            assert_eq!(decoded, code.vector);
            let pulses = decoded.iter().filter(|x| **x != 0.0).count();
            assert_eq!(pulses, usize::from(MPMLQ_PULSE_COUNTS[k]));
        }
    }

    #[test]
    fn short_lag_codes_round_trip() {
        let h = impulse_response();
        let mut periodic = [0.0; SUBFRAME_LEN];
        for n in (4..SUBFRAME_LEN).step_by(20) {
            periodic[n] = 5000.0;
        }
        let r = convolve(&periodic, &h);
        let c = search(&r, &h, 1, 20);
        let decoded = decode(c.position, c.signs, c.gain_index, c.grid, c.train, 1, 20);
        assert_eq!(decoded, c.vector);
    }

    #[test]
    fn decoded_pulse_trains_repeat_the_pulses() {
        let plain = decode(1234, 0b10110, 7, 1, false, 3, 21);
        assert_eq!(
            decode(1234, 0b10110, 7, 1, true, 3, 21),
            pulse_train(&plain, 21)
        );
    }

    #[test]
    fn search_reduces_the_error() {
        let h = impulse_response();
        let r: [f64; SUBFRAME_LEN] = std::array::from_fn(|n| 2000.0 * (n as f64 * 0.31).cos());
        let code = search(&r, &h, 0, 120);
        let synthesized = convolve(&code.vector, &h);
        let error: f64 = (0..SUBFRAME_LEN)
            .map(|n| (r[n] - synthesized[n]).powi(2))
            .sum();
        assert!(error < dot(&r, &r));
    }

    #[test]
    fn out_of_range_positions_decode_to_silence() {
        let v = decode(MPMLQ_CODEBOOK_SIZES[1], 0, 5, 0, false, 1, 100);
        assert_eq!(v, [0.0; SUBFRAME_LEN]);
    }
}
