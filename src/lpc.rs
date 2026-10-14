//! LPC analysis (clause 2.4): windowing, autocorrelation, Levinson-Durbin recursion and the
//! sine detector.

use crate::lsp::Lpc;
use crate::tables::{BINOMIAL_WINDOW, HAMMING_WINDOW};
use crate::{LPC_ORDER, SUBFRAME_LEN, dot};

const WINDOW_LEN: usize = 180;
/// Samples of high-pass filtered speech that one frame's analysis looks at.
pub(crate) const ANALYSIS_LEN: usize = 360;

pub(crate) type Autocorrelation = [f64; LPC_ORDER + 1];

/// Windowed autocorrelation of the 180 samples starting at `x`, with the white noise
/// correction and the binomial lag window applied.
pub(crate) fn autocorrelation(x: &[f64]) -> Autocorrelation {
    let v: [f64; WINDOW_LEN] = std::array::from_fn(|n| x[n] * HAMMING_WINDOW[n]);
    let mut r = [0.0; LPC_ORDER + 1];
    r[0] = dot(&v, &v) / 32400.0;
    r[0] *= 1.0009765625;
    if r[0] != 0.0 {
        for j in 1..=LPC_ORDER {
            r[j] = (dot(&v[..WINDOW_LEN - j], &v[j..]) / 32400.0) * BINOMIAL_WINDOW[j - 1];
        }
    }
    r
}

/// The result of the Levinson-Durbin recursion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Prediction {
    /// Predictor coefficients a(1) to a(10).
    pub lpc: Lpc,
    /// What the sine detector looks at: the second reflection coefficient with its sign
    /// changed, or 0.99 when the recursion stops early.
    pub pk2: f64,
    /// The prediction error energy where the recursion ended.
    pub energy: f64,
}

/// Levinson-Durbin recursion.
pub(crate) fn durbin(r: &Autocorrelation) -> Prediction {
    let mut a = [0.0; LPC_ORDER];
    let mut energy = r[0];
    let mut pk2 = 0.0;
    for i in 1..=LPC_ORDER {
        let mut t = r[i];
        for j in 1..i {
            t -= a[j - 1] * r[i - j];
        }
        if t.abs() >= energy {
            pk2 = 0.99;
            break;
        }
        let k = t / energy;
        a[i - 1] = k;
        energy -= t * k;
        if i == 2 {
            pk2 = -k;
        }
        let old = a;
        for j in 1..i {
            a[j - 1] -= k * old[i - j - 1];
        }
    }
    Prediction {
        lpc: a,
        pk2,
        energy,
    }
}

/// Tracks how many recent subframes look like a pure tone.
#[derive(Clone, Debug, Default)]
pub(crate) struct SineDetector(u16);

impl SineDetector {
    fn push(&mut self, pk2: f64) {
        self.0 = (self.0 << 1) | u16::from(pk2 > 0.95);
    }

    fn end_frame(&mut self) {
        self.0 &= 0x7fff;
        if self.0.count_ones() >= 14 {
            self.0 |= 0x8000;
        }
    }

    pub(crate) fn detected(&self) -> bool {
        self.0 & 0x8000 != 0
    }
}

/// The LPC analysis of one frame.
pub(crate) struct Analysis {
    /// Unquantized LPC set of each subframe.
    pub lpc: [Lpc; 4],
    /// Autocorrelation of each subframe.
    pub autocorrelation: [Autocorrelation; 4],
}

/// Analyses the four subframes of a frame. `b` is the previous 120 filtered samples
/// followed by the 240 of the current frame.
pub(crate) fn analyse(b: &[f64; ANALYSIS_LEN], sine: &mut SineDetector) -> Analysis {
    let mut lpc = [[0.0; LPC_ORDER]; 4];
    let mut autocorrelations = [[0.0; LPC_ORDER + 1]; 4];
    for k in 0..4 {
        autocorrelations[k] = autocorrelation(&b[SUBFRAME_LEN * k..]);
        let prediction = durbin(&autocorrelations[k]);
        lpc[k] = prediction.lpc;
        sine.push(prediction.pk2);
    }
    sine.end_frame();
    Analysis {
        lpc,
        autocorrelation: autocorrelations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durbin_solves_a_first_order_process() {
        let rho: f64 = 0.8;
        let r: Autocorrelation = std::array::from_fn(|j| rho.powi(j as i32));
        let p = durbin(&r);
        assert!((p.lpc[0] - rho).abs() < 1e-12);
        assert!(p.lpc[1..].iter().all(|c| c.abs() < 1e-12));
        assert!(p.pk2.abs() < 1e-12);
        assert!((p.energy - (1.0 - rho * rho)).abs() < 1e-12);
    }

    #[test]
    fn durbin_solves_a_second_order_process() {
        let (a1, a2) = (1.2, -0.5);
        let mut r = [0.0; LPC_ORDER + 1];
        r[0] = 1.0;
        r[1] = a1 / (1.0 - a2);
        for j in 2..=LPC_ORDER {
            r[j] = a1 * r[j - 1] + a2 * r[j - 2];
        }
        let p = durbin(&r);
        assert!((p.lpc[0] - a1).abs() < 1e-9 && (p.lpc[1] - a2).abs() < 1e-9, "{p:?}");
        assert!((p.pk2 - 0.5).abs() < 1e-9);
    }

    #[test]
    fn silence_gives_zero_coefficients() {
        let r = autocorrelation(&[0.0; WINDOW_LEN]);
        let p = durbin(&r);
        assert_eq!((p.lpc, p.pk2, p.energy), ([0.0; LPC_ORDER], 0.99, 0.0));
    }

    #[test]
    fn sine_detector_needs_fourteen_of_fifteen_subframes() {
        let mut sine = SineDetector::default();
        for frame in 0..4 {
            for _ in 0..4 {
                sine.push(0.99);
            }
            sine.end_frame();
            assert_eq!(sine.detected(), frame >= 3);
        }
        sine.push(0.0);
        sine.push(0.0);
        sine.push(0.99);
        sine.push(0.99);
        sine.end_frame();
        assert!(!sine.detected());
    }
}
