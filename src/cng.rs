//! Comfort noise generation (Annex A.3 to A.5): SID gain coding, the filter distance test and
//! the random excitation shared by the encoder and the decoder.

use crate::concealment::Random;
use crate::lpc::Autocorrelation;
use crate::lsp::Lpc;
use crate::pitch::PitchChoice;
use crate::tables::{
    MPMLQ_PULSE_COUNTS, PITCH_GAIN_CODEBOOK_170, SID_GAIN_ENERGY_SCALES, SID_GAIN_SEGMENT_BASES,
    SID_GAIN_SEGMENT_LIMITS,
};
use crate::{EXC_MEMORY, FRAME_LEN, LPC_ORDER, SUBFRAME_LEN, clip, dot};

/// Seed of the comfort noise generator at the start of every inactive period.
pub(crate) const SEED: u16 = 12345;
/// Number of values the random lag of the second half frame is drawn from. The 5.2 release
/// of the reference draws from 19; the conformance vectors were generated with 21.
const SECOND_LAG_CHOICES: u32 = 21;
const LAG_OFFSETS: [usize; 4] = [1, 0, 1, 3];
const BLOCK_LEN: usize = 2 * SUBFRAME_LEN;
const BLOCK_PULSES: usize = 11;

impl Random {
    /// A random integer from 0 to `n` - 1.
    fn draw(&mut self, n: u32) -> usize {
        let v = (self.next() as u16 & 0x7fff) as u32;
        ((v * n) >> 15) as usize
    }
}

/// Quantizes a scaled energy to a 6-bit SID gain index (A.4.3).
pub(crate) fn quantize_gain(x: f64) -> u8 {
    if x >= SID_GAIN_SEGMENT_LIMITS[2] {
        return 63;
    }
    let (segment, depth) = if x >= SID_GAIN_SEGMENT_LIMITS[1] {
        (2, 4)
    } else if x >= SID_GAIN_SEGMENT_LIMITS[0] {
        (1, 3)
    } else {
        (0, 3)
    };
    let level = |j: i32| SID_GAIN_SEGMENT_BASES[segment] + f64::from(j) * f64::from(2 << segment);
    let mut j = 1 << depth;
    let mut step = j / 2;
    for _ in 0..depth {
        if x >= level(j) * level(j) {
            j += step;
        } else {
            j -= step;
        }
        step /= 2;
    }
    let y = level(j) * level(j) - x;
    let chosen = if y <= 0.0 {
        let z = x - level(j + 1) * level(j + 1);
        if y > z { j } else { j + 1 }
    } else {
        let z = x - level(j - 1) * level(j - 1);
        if y < z { j } else { j - 1 }
    };
    (16 * segment as i32 + chosen) as u8
}

/// The comfort noise amplitude of a SID gain index.
pub(crate) fn decode_gain(index: u8) -> f64 {
    let segment = usize::from(index >> 4).min(2);
    let j = i32::from(index) - 16 * segment as i32;
    SID_GAIN_SEGMENT_BASES[segment] + f64::from(j) * f64::from(2 << segment)
}

/// The SID gain index of the mean of `energies` frame energies.
pub(crate) fn energy_gain_index(energies: &[f64]) -> u8 {
    let mut sum = 0.0;
    for e in energies {
        sum += e;
    }
    quantize_gain(SID_GAIN_ENERGY_SCALES[energies.len()] * sum)
}

/// The SID gain index of the energy of the last half frame of excitation of an active frame,
/// used when the first SID frame of an inactive period is lost.
pub(crate) fn excitation_gain_index(energy: f64) -> u8 {
    quantize_gain(SID_GAIN_ENERGY_SCALES[0] * energy)
}

/// The autocorrelation of the impulse response of the inverse filter A(z) (equation A-11).
pub(crate) fn reference_function(a: &Lpc) -> Autocorrelation {
    let mut rc = [0.0; LPC_ORDER + 1];
    rc[0] = 1.0 + dot(a, a);
    for j in 1..=LPC_ORDER {
        rc[j] = 2.0 * (-a[j - 1] + dot(&a[..LPC_ORDER - j], &a[j..]));
    }
    rc
}

/// Whether the spectrum `r` with prediction error `energy` differs from the filter whose
/// reference function is `rc` (equation A-10).
pub(crate) fn differs(rc: &Autocorrelation, r: &Autocorrelation, energy: f64) -> bool {
    dot(rc, r) > 1.2136 * energy
}

/// One frame of comfort noise excitation with the random lags and gains it used.
pub(crate) struct ComfortNoise {
    pub excitation: [f64; FRAME_LEN],
    pub pitch: [PitchChoice; 4],
}

/// Generates a frame of comfort noise excitation with amplitude `gain`, updating the
/// excitation memory (A.4.5).
pub(crate) fn excitation(
    gain: f64,
    memory: &mut [f64; EXC_MEMORY],
    random: &mut Random,
) -> ComfortNoise {
    let lags = [123 + random.draw(21), 123 + random.draw(SECOND_LAG_CHOICES)];
    let gains: [usize; 4] = std::array::from_fn(|_| 1 + random.draw(50));
    let words = [random.draw(8192), random.draw(8192)];
    let mut positions = [[0; BLOCK_PULSES]; 2];
    for s in 0..4 {
        let (block, second) = (s / 2, s % 2);
        let offset = if second == 0 {
            words[block] & 1
        } else {
            SUBFRAME_LEN + ((words[block] >> 1) & 1)
        };
        let first_pulse = if second == 0 { 0 } else { 6 };
        let mut slots: [usize; 30] = std::array::from_fn(|i| i);
        let mut remaining = 30;
        for p in 0..usize::from(MPMLQ_PULSE_COUNTS[s]) {
            let j = random.draw(remaining as u32);
            positions[block][first_pulse + p] = 2 * slots[j] + offset;
            remaining -= 1;
            slots[j] = slots[remaining];
        }
    }

    let pitch: [PitchChoice; 4] = std::array::from_fn(|s| PitchChoice {
        pair_lag: lags[s / 2],
        offset: LAG_OFFSETS[s],
        gain_index: gains[s],
    });
    let mut excitation = [0.0; FRAME_LEN];
    for block in 0..2 {
        let mut u = [0.0; BLOCK_LEN];
        for second in 0..2 {
            let s = 2 * block + second;
            let lag = pitch[s].lag();
            let row = &PITCH_GAIN_CODEBOOK_170[gains[s]];
            for m in 0..SUBFRAME_LEN {
                let start = EXC_MEMORY - 2 + SUBFRAME_LEN * second - lag + m;
                u[SUBFRAME_LEN * second + m] = dot(&row[..5], &memory[start..start + 5]);
            }
        }
        let sign = |k: usize| {
            if (words[block] >> (2 + k)) & 1 == 1 {
                1.0
            } else {
                -1.0
            }
        };
        let energy = dot(&u, &u);
        let mut cross = 0.0;
        for k in 0..BLOCK_PULSES {
            cross += u[positions[block][k]] * sign(k);
        }
        let inverse = 1.0 / 11.0;
        let b0 = cross * inverse;
        let c = (energy - (gain * gain) * 120.0) * inverse;
        let delta = b0 * b0 - c;
        let mut x = if delta <= 0.0 {
            -b0
        } else {
            let q = delta.sqrt();
            let (x1, x2) = (-b0 + q, b0 + q);
            if x2.abs() < x1.abs() { -x2 } else { x1 }
        };
        x = x.clamp(-5000.0, 5000.0);
        let mut e = u;
        for k in 0..BLOCK_PULSES {
            e[positions[block][k]] += x * sign(k);
        }
        let e = e.map(clip);
        memory.copy_within(BLOCK_LEN.., 0);
        memory[EXC_MEMORY - BLOCK_LEN..].copy_from_slice(&e);
        excitation[BLOCK_LEN * block..BLOCK_LEN * (block + 1)].copy_from_slice(&e);
    }
    ComfortNoise { excitation, pitch }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_quantizer_check_values() {
        let cases = [
            (2.7314, 1),
            (1100.0, 16),
            (1400.0, 17),
            (1500.0, 18),
            (2.0, 0),
            (3.0, 1),
            (9216.0, 32),
            (115617.0, 63),
        ];
        for (x, index) in cases {
            assert_eq!(quantize_gain(x), index, "{x}");
        }
    }

    #[test]
    fn gain_levels_join_without_gaps() {
        assert_eq!(
            [0, 15, 16, 31, 32, 63].map(decode_gain),
            [0.0, 30.0, 32.0, 92.0, 96.0, 344.0]
        );
    }

    #[test]
    fn random_draws_follow_the_reference_check_values() {
        let mut random = Random::new(SEED);
        assert_eq!(random.draw(21), 6);
        let mut memory = [0.0; EXC_MEMORY];
        let mut random = Random::new(SEED);
        let noise = excitation(100.0, &mut memory, &mut random);
        assert_eq!(noise.pitch.map(|p| p.pair_lag), [129, 129, 137, 137]);
        assert_eq!(noise.pitch.map(|p| p.gain_index), [34, 31, 18, 35]);
    }

    #[test]
    fn comfort_noise_has_the_requested_energy() {
        let mut memory = [0.0; EXC_MEMORY];
        let mut random = Random::new(SEED);
        let noise = excitation(100.0, &mut memory, &mut random);
        let energy = dot(
            &noise.excitation[..BLOCK_LEN],
            &noise.excitation[..BLOCK_LEN],
        );
        assert!((energy - 120.0 * 100.0 * 100.0).abs() < 1e-6 * energy);
        assert_eq!(
            &memory[EXC_MEMORY - BLOCK_LEN..],
            &noise.excitation[BLOCK_LEN..]
        );
    }

    #[test]
    fn identical_spectra_do_not_differ() {
        let a = [0.5, -0.2, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let rc = reference_function(&a);
        let mut r = [0.0; LPC_ORDER + 1];
        r[0] = 1.0;
        assert!(!differs(&rc, &r, rc[0]));
        assert!(differs(&rc, &r, 0.5 * rc[0]));
    }
}
