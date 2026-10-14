//! Line spectral pairs: conversion from and to LPC coefficients, quantization, inverse
//! quantization and interpolation (clauses 2.5 to 2.7 and 3.10.1).
//!
//! LSP frequencies are in units of pi/256 radian (15.625 Hz).

use crate::{LPC_ORDER, dot};
use crate::tables::{
    BANDWIDTH_EXPANSION_FACTORS, LSP_BAND0_CODEBOOK, LSP_BAND1_CODEBOOK, LSP_BAND2_CODEBOOK,
    LSP_COSINE_TABLE, LSP_DC_VECTOR, LSP_INTERPOLATION_WEIGHTS, LSP_POLYNOMIAL_SCALES,
};

pub(crate) type Lsp = [f64; LPC_ORDER];
pub(crate) type Lpc = [f64; LPC_ORDER];

/// Converts the LPC set of the last subframe to LSP frequencies, after a small bandwidth
/// expansion. Returns `previous` when fewer than ten roots are found.
pub(crate) fn from_lpc(a: &Lpc, previous: &Lsp) -> Lsp {
    let v: [f64; LPC_ORDER] = std::array::from_fn(|j| a[j] * BANDWIDTH_EXPANSION_FACTORS[j]);
    let mut g = [0.0; 12];
    g[0] = 1.0;
    g[1] = 1.0;
    for m in 0..5 {
        g[2 * m + 2] = (-g[2 * m] - v[m]) - v[9 - m];
        g[2 * m + 3] = (g[2 * m + 1] - v[m]) + v[9 - m];
    }
    g[10] *= 0.5;
    g[11] *= 0.5;
    let evaluate = |q: usize, odd: usize| {
        let mut sum = 0.0;
        for j in 0..6 {
            sum += g[10 - 2 * j + odd] * LSP_COSINE_TABLE[(q * j) % 512];
        }
        sum
    };

    let mut roots = [0.0; LPC_ORDER];
    let mut found = 0;
    let mut odd = 0;
    let mut prev = evaluate(0, odd);
    for q in 1..256 {
        let mut cur = evaluate(q, odd);
        if cur * prev < 0.0 {
            let (ap, ac) = (prev.abs(), cur.abs());
            roots[found] = (q - 1) as f64 + ap / (ap + ac);
            found += 1;
            if found == LPC_ORDER {
                break;
            }
            odd ^= 1;
            cur = evaluate(q, odd);
        }
        prev = cur;
    }
    if found < LPC_ORDER { *previous } else { roots }
}

/// Quantizes an LSP vector with the predictive split vector quantizer and returns the three
/// codebook indices.
pub(crate) fn quantize(lsp: &Lsp, previous: &Lsp) -> [u8; 3] {
    let mut weight = [0.0; LPC_ORDER];
    weight[0] = 1.0 / (lsp[1] - lsp[0]);
    weight[9] = 1.0 / (lsp[9] - lsp[8]);
    for j in 1..9 {
        let mut m = lsp[j + 1] - lsp[j];
        let m2 = lsp[j] - lsp[j - 1];
        if m2 < m {
            m = m2;
        }
        weight[j] = if m > 0.0 { 1.0 / m } else { 1.0 };
    }
    let target: [f64; LPC_ORDER] = std::array::from_fn(|j| {
        (lsp[j] - LSP_DC_VECTOR[j]) - 0.375 * (previous[j] - LSP_DC_VECTOR[j])
    });
    [
        search_band(&LSP_BAND0_CODEBOOK, &target[0..3], &weight[0..3]),
        search_band(&LSP_BAND1_CODEBOOK, &target[3..6], &weight[3..6]),
        search_band(&LSP_BAND2_CODEBOOK, &target[6..10], &weight[6..10]),
    ]
}

fn search_band<const N: usize>(
    codebook: &[[f64; N]; 256],
    target: &[f64],
    weight: &[f64],
) -> u8 {
    let mut best = -1.0;
    let mut best_index = 0;
    for (l, entry) in codebook.iter().enumerate() {
        let tw: [f64; N] = std::array::from_fn(|d| weight[d] * entry[d]);
        let score = 2.0 * dot(target, &tw) - dot(entry, &tw);
        if score > best {
            best = score;
            best_index = l;
        }
    }
    best_index as u8
}

/// Decodes the three codebook indices into an LSP vector. An erased frame ignores the
/// indices and uses the stronger prediction and wider minimum separation of 3.10.1.
pub(crate) fn dequantize(index: [u8; 3], previous: &Lsp, erased: bool) -> Lsp {
    let (index, separation, predictor) = if erased {
        ([0; 3], 4.0, 0.71875)
    } else {
        (index, 2.0, 0.375)
    };
    let mut e = [0.0; LPC_ORDER];
    e[0..3].copy_from_slice(&LSP_BAND0_CODEBOOK[usize::from(index[0])]);
    e[3..6].copy_from_slice(&LSP_BAND1_CODEBOOK[usize::from(index[1])]);
    e[6..10].copy_from_slice(&LSP_BAND2_CODEBOOK[usize::from(index[2])]);
    let mut p: Lsp = std::array::from_fn(|j| {
        (e[j] + (previous[j] - LSP_DC_VECTOR[j]) * predictor) + LSP_DC_VECTOR[j]
    });

    for _ in 0..10 {
        if p[0] < 3.0 {
            p[0] = 3.0;
        }
        if p[9] > 252.0 {
            p[9] = 252.0;
        }
        for j in 1..LPC_ORDER {
            let mut d = (separation + p[j - 1]) - p[j];
            if d > 0.0 {
                d *= 0.5;
                p[j - 1] -= d;
                p[j] += d;
            }
        }
        if (1..LPC_ORDER).all(|j| p[j] - p[j - 1] >= separation - 0.03125) {
            return p;
        }
    }
    *previous
}

/// Interpolates between the previous and the current LSP vector for each subframe
/// (equation 8) and converts the results to LPC sets.
pub(crate) fn interpolate(previous: &Lsp, current: &Lsp) -> [Lpc; 4] {
    std::array::from_fn(|k| {
        let w = LSP_INTERPOLATION_WEIGHTS[k];
        let u: Lsp = std::array::from_fn(|j| (1.0 - w) * previous[j] + w * current[j]);
        to_lpc(&u)
    })
}

/// Converts an LSP vector to an LPC set in predictor form.
pub(crate) fn to_lpc(lsp: &Lsp) -> Lpc {
    let x: [f64; LPC_ORDER] = std::array::from_fn(|j| {
        let fl = lsp[j] as usize;
        let c0 = LSP_COSINE_TABLE[fl];
        -(c0 + (LSP_COSINE_TABLE[fl + 1] - c0) * (lsp[j] - fl as f64))
    });
    let p = polynomial(x[0], x[2], [x[4], x[6], x[8]]);
    let q = polynomial(x[1], x[3], [x[5], x[7], x[9]]);
    let mut a = [0.0; LPC_ORDER];
    for i in 0..5 {
        a[i] = (((-p[i] - p[i + 1]) + q[i]) - q[i + 1]) * 8.0;
        a[9 - i] = (((-p[i] - p[i + 1]) - q[i]) + q[i + 1]) * 8.0;
    }
    a
}

fn polynomial(x1: f64, x3: f64, rest: [f64; 3]) -> [f64; 6] {
    let mut p = [0.0; 6];
    p[0] = 0.5;
    p[1] = x1 + x3;
    p[2] = 1.0 + 2.0 * x1 * x3;
    for (round, xp) in rest.into_iter().enumerate() {
        let r = round + 2;
        p[r + 1] = p[r - 1] + p[r] * xp;
        for j in (2..=r).rev() {
            p[j] = p[j - 1] * xp + 0.5 * (p[j] + p[j - 2]);
        }
        p[0] *= 0.5;
        p[1] = (p[1] + xp * LSP_POLYNOMIAL_SCALES[round]) * 0.5;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn lsp_to_lpc_and_back() {
        let a = to_lpc(&LSP_DC_VECTOR);
        let unexpanded: Lpc = std::array::from_fn(|j| a[j] / BANDWIDTH_EXPANSION_FACTORS[j]);
        let lsp = from_lpc(&unexpanded, &[0.0; LPC_ORDER]);
        for j in 0..LPC_ORDER {
            assert!((lsp[j] - LSP_DC_VECTOR[j]).abs() < 0.15, "{lsp:?}");
        }
    }

    #[test]
    fn flat_spectrum_has_evenly_spread_lsps() {
        let a = to_lpc(&std::array::from_fn(|j| 256.0 * (j + 1) as f64 / 11.0));
        assert!(a.iter().all(|c| c.abs() < 1e-2), "{a:?}");
    }

    #[test]
    fn failed_root_search_falls_back() {
        let previous = LSP_DC_VECTOR;
        let lsp = from_lpc(&[f64::NAN; LPC_ORDER], &previous);
        assert_eq!(lsp, previous);
    }

    #[test]
    fn quantized_lsps_decode_close_to_the_input() {
        let previous = LSP_DC_VECTOR;
        let offset = |j: usize| 3.0 * (j as f64 - 4.5).sin();
        let target: Lsp = std::array::from_fn(|j| LSP_DC_VECTOR[j] + offset(j));
        let index = quantize(&target, &previous);
        let decoded = dequantize(index, &previous, false);
        for j in 0..LPC_ORDER {
            assert!((decoded[j] - target[j]).abs() < 4.0, "{decoded:?}");
        }
    }

    #[test]
    fn zero_index_reproduces_the_prediction() {
        let decoded = dequantize([0; 3], &LSP_DC_VECTOR, false);
        assert_eq!(decoded, LSP_DC_VECTOR);
    }

    #[test]
    fn dequantized_lsps_are_separated() {
        let mut previous = LSP_DC_VECTOR;
        previous[4] = previous[3] + 0.5;
        for (erased, gap) in [(false, 2.0), (true, 4.0)] {
            let decoded = dequantize([17, 99, 201], &previous, erased);
            let separated = decoded.windows(2).all(|w| w[1] - w[0] >= gap - 0.03125);
            assert!(separated || decoded == previous, "{decoded:?}");
        }
    }

    #[test]
    fn interpolation_ends_at_the_current_vector() {
        let current: Lsp = std::array::from_fn(|j| LSP_DC_VECTOR[j] + 2.0);
        let sets = interpolate(&LSP_DC_VECTOR, &current);
        assert_eq!(sets[3], to_lpc(&current));
    }
}
