//! Low rate fixed codebook: algebraic code excitation (clauses 2.16 and 2.17).

use crate::mpmlq::{FixedCode, SHORT_LAG};
use crate::tables::{ACELP_PITCH_TAP_GAINS, ACELP_PITCH_TAP_OFFSETS, FIXED_GAIN_LEVELS};
use crate::{MIN_ENERGY, SUBFRAME_LEN, dot};

/// Positions 0 to 63: four pulses on interleaved grids of eight, the last slot of pulses 2
/// and 3 falling outside the subframe.
const EXTENDED_LEN: usize = 64;
const PULSES: usize = 4;
const SLOTS: usize = 8;
/// Innermost search loops allowed per subframe, before the carry-over.
pub(crate) const SEARCH_BUDGET: i32 = 120;

/// The pitch-synchronous filter 1 / (1 - beta z^-period) of a subframe, when it applies.
fn pitch_sync(lag: usize, gain_index: usize) -> Option<(usize, f64)> {
    let period = lag as isize + isize::from(ACELP_PITCH_TAP_OFFSETS[gain_index]);
    if period < SHORT_LAG as isize {
        Some((period as usize, ACELP_PITCH_TAP_GAINS[gain_index]))
    } else {
        None
    }
}

fn apply_pitch_sync(v: &mut [f64; SUBFRAME_LEN], filter: Option<(usize, f64)>) {
    if let Some((period, beta)) = filter {
        for n in period..SUBFRAME_LEN {
            v[n] += beta * v[n - period];
        }
    }
}

/// Phi(p, q) of equation 29, summed in pairs of terms.
fn covariance(h: &[f64; SUBFRAME_LEN], p: usize, q: usize) -> f64 {
    let (p, q) = (p.min(q), p.max(q));
    if q >= SUBFRAME_LEN {
        return 0.0;
    }
    let d = q - p;
    let mut sum = 0.0;
    for u in (0..SUBFRAME_LEN - q).step_by(2) {
        sum += h[u] * h[u + d] + h[u + 1] * h[u + 1 + d];
    }
    sum
}

/// Searches the low rate codebook. `lag` is the adaptive codebook lag of the subframe and
/// `pitch_gain` its pitch gain index. `carry` is the unused search budget of the earlier
/// subframes of the frame.
pub(crate) fn search(
    r: &[f64; SUBFRAME_LEN],
    h: &[f64; SUBFRAME_LEN],
    lag: usize,
    pitch_gain: usize,
    carry: &mut i32,
) -> FixedCode {
    let filter = pitch_sync(lag, pitch_gain);
    let mut h = *h;
    apply_pitch_sync(&mut h, filter);

    let mut d = [0.0; EXTENDED_LEN];
    for i in 0..SUBFRAME_LEN {
        d[i] = dot(&r[i..], &h[..SUBFRAME_LEN - i]);
    }
    let mut pair_sign = [1.0; EXTENDED_LEN / 2];
    for j in 0..SUBFRAME_LEN / 2 {
        if d[2 * j] + d[2 * j + 1] < 0.0 {
            pair_sign[j] = -1.0;
        }
        d[2 * j] *= pair_sign[j];
        d[2 * j + 1] *= pair_sign[j];
    }
    let sign = |p: usize| pair_sign[p / 2];

    let position = |k: usize, x: usize| SLOTS * x + 2 * k;
    let mut diagonal = [[0.0; SLOTS]; PULSES];
    for k in 0..PULSES {
        for x in 0..SLOTS {
            let p = position(k, x);
            diagonal[k][x] = covariance(&h, p, p);
        }
    }
    let mut folded = [[[[0.0; SLOTS]; SLOTS]; PULSES]; PULSES];
    for a in 0..PULSES {
        for b in a + 1..PULSES {
            for xa in 0..SLOTS {
                for xb in 0..SLOTS {
                    let (pa, pb) = (position(a, xa), position(b, xb));
                    folded[a][b][xa][xb] = 2.0 * sign(pa) * sign(pb) * covariance(&h, pa, pb);
                }
            }
        }
    }

    let mut threshold = f64::NEG_INFINITY;
    for shift in 0..2 {
        let largest = |k: usize| {
            let mut m = d[shift + position(k, 0)];
            for x in 1..SLOTS {
                if d[shift + position(k, x)] > m {
                    m = d[shift + position(k, x)];
                }
            }
            m
        };
        let maximum = largest(0) + largest(1) + largest(2);
        let mut average = 0.0;
        for x in 0..SLOTS {
            let i = shift + SLOTS * x;
            average += (d[i + 4] + d[i + 2]) + d[i];
        }
        average *= 0.125;
        let t = average + (maximum - average) * 0.5;
        if shift == 0 || t > threshold {
            threshold = t;
        }
    }

    let mut budget = SEARCH_BUDGET + *carry;
    let mut best_corr2 = 0.0;
    let mut best_energy = 1.0;
    let mut best_slots = [0; PULSES];
    let mut best_shift = 0;
    'search: for x0 in 0..SLOTS {
        let p0 = position(0, x0);
        let (a0, a0s) = (d[p0], d[p0 + 1]);
        let e0 = diagonal[0][x0];
        for x1 in 0..SLOTS {
            let p1 = position(1, x1);
            let (a1, a1s) = (a0 + d[p1], a0s + d[p1 + 1]);
            let e1 = (e0 + diagonal[1][x1]) + folded[0][1][x0][x1];
            for x2 in 0..SLOTS {
                let p2 = position(2, x2);
                let (a2, a2s) = (a1 + d[p2], a1s + d[p2 + 1]);
                let mut e2 = e1 + diagonal[2][x2];
                e2 += folded[0][2][x0][x2];
                e2 += folded[1][2][x1][x2];
                let (a2, shift) = if a2s > a2 { (a2s, 1) } else { (a2, 0) };
                if a2 <= threshold {
                    continue;
                }
                for x3 in 0..SLOTS {
                    let a3 = a2 + d[position(3, x3) + shift];
                    let mut e3 = e2 + diagonal[3][x3];
                    e3 += folded[0][3][x0][x3];
                    e3 += folded[1][3][x1][x3];
                    e3 += folded[2][3][x2][x3];
                    let q = a3 * a3;
                    if q * best_energy > best_corr2 * e3 {
                        best_corr2 = q;
                        best_energy = e3;
                        best_slots = [x0, x1, x2, x3];
                        best_shift = shift;
                    }
                }
                budget -= 1;
                if budget <= 0 {
                    break 'search;
                }
            }
        }
    }
    *carry = budget;

    let mut u = [0.0; SUBFRAME_LEN];
    let mut y = [0.0; SUBFRAME_LEN];
    let mut signs = 0;
    let mut position_index = 0;
    for k in 0..PULSES {
        let p = position(k, best_slots[k]);
        let q = p + best_shift;
        let positive = sign(p) > 0.0;
        signs |= u8::from(positive) << k;
        position_index |= ((q / SLOTS) as u32) << (3 * k);
        if q < SUBFRAME_LEN {
            u[q] = sign(p);
            for n in q..SUBFRAME_LEN {
                if positive {
                    y[n] += h[n - q];
                } else {
                    y[n] -= h[n - q];
                }
            }
        }
    }

    let gain_index = quantize_gain(r, &y);
    let level = FIXED_GAIN_LEVELS[gain_index];
    let mut vector = u.map(|x| level * x);
    apply_pitch_sync(&mut vector, filter);
    FixedCode {
        vector,
        position: position_index,
        signs,
        gain_index,
        grid: best_shift as u8,
        train: false,
    }
}

fn quantize_gain(r: &[f64; SUBFRAME_LEN], y: &[f64; SUBFRAME_LEN]) -> usize {
    let xy = dot(r, y);
    if xy <= 0.0 {
        return 0;
    }
    let yy = dot(y, y);
    let gain = if yy > MIN_ENERGY { xy / yy } else { 0.0 };
    let mut best = 0;
    let mut nearest = (gain - FIXED_GAIN_LEVELS[0]).abs();
    for (q, level) in FIXED_GAIN_LEVELS.iter().enumerate().skip(1) {
        let distance = (gain - level).abs();
        if distance < nearest {
            nearest = distance;
            best = q;
        }
    }
    best
}

/// Decodes the low rate fixed excitation of a subframe with adaptive codebook lag `lag` and
/// pitch gain index `pitch_gain`.
pub(crate) fn decode(
    position: u32,
    signs: u8,
    gain_index: usize,
    grid: u8,
    lag: usize,
    pitch_gain: usize,
) -> [f64; SUBFRAME_LEN] {
    let level = FIXED_GAIN_LEVELS[gain_index];
    let mut v = [0.0; EXTENDED_LEN];
    for k in 0..PULSES {
        let slot = ((position >> (3 * k)) & 7) as usize;
        let q = SLOTS * slot + usize::from(grid) + 2 * k;
        v[q] = if signs & (1 << k) != 0 { level } else { -level };
    }
    let mut out: [f64; SUBFRAME_LEN] = std::array::from_fn(|n| v[n]);
    apply_pitch_sync(&mut out, pitch_sync(lag, pitch_gain));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse_response() -> [f64; SUBFRAME_LEN] {
        std::array::from_fn(|n| 0.8_f64.powi(n as i32) * (n as f64 * 0.5).cos())
    }

    fn convolve(x: &[f64; SUBFRAME_LEN], h: &[f64; SUBFRAME_LEN]) -> [f64; SUBFRAME_LEN] {
        let mut y = [0.0; SUBFRAME_LEN];
        for n in 0..SUBFRAME_LEN {
            for j in 0..=n {
                y[n] += x[j] * h[n - j];
            }
        }
        y
    }

    /// An entry of the pitch gain codebook that adds no pitch-synchronous filter.
    fn plain_gain_index() -> usize {
        ACELP_PITCH_TAP_OFFSETS.iter().position(|&o| o == 60).unwrap()
    }

    #[test]
    fn finds_the_pulses_of_a_codeword() {
        let h = impulse_response();
        let mut pulses = [0.0; SUBFRAME_LEN];
        pulses[9] = 1000.0;
        pulses[19] = -1000.0;
        pulses[37] = 1000.0;
        pulses[55] = -1000.0;
        let r = convolve(&pulses, &h);
        let mut carry = SEARCH_BUDGET;
        let plain = plain_gain_index();
        let code = search(&r, &h, 100, plain, &mut carry);
        assert_eq!(code.grid, 1);
        assert_eq!(code.signs, 0b0101);
        assert_eq!(code.position, 1 | (2 << 3) | (4 << 6) | (6 << 9));
        let decoded = decode(code.position, code.signs, code.gain_index, 1, 100, plain);
        assert_eq!(decoded, code.vector);
    }

    #[test]
    fn pitch_synchronous_codes_round_trip() {
        let h = impulse_response();
        let r: [f64; SUBFRAME_LEN] = std::array::from_fn(|n| 800.0 * (n as f64 * 0.7).sin());
        let gain = (0..170).find(|&i| ACELP_PITCH_TAP_OFFSETS[i] != 60).unwrap();
        let mut carry = SEARCH_BUDGET;
        let code = search(&r, &h, 30, gain, &mut carry);
        let decoded = decode(code.position, code.signs, code.gain_index, code.grid, 30, gain);
        assert_eq!(decoded, code.vector);
    }

    #[test]
    fn budget_carries_over_and_runs_out() {
        let h = impulse_response();
        let r: [f64; SUBFRAME_LEN] = std::array::from_fn(|n| ((n * 37) % 11) as f64 - 5.0);
        let mut carry = 0;
        search(&r, &h, 100, plain_gain_index(), &mut carry);
        assert!((0..=SEARCH_BUDGET).contains(&carry));
    }

    #[test]
    fn covariance_is_symmetric_and_vanishes_outside() {
        let h = impulse_response();
        assert_eq!(covariance(&h, 8, 18), covariance(&h, 18, 8));
        assert_eq!(covariance(&h, 4, 60), 0.0);
        let direct: f64 = (18..SUBFRAME_LEN).map(|n| h[n - 8] * h[n - 18]).sum();
        assert!((covariance(&h, 8, 18) - direct).abs() < 1e-12);
    }
}
