//! Frame erasure concealment of the decoder (clause 3.10.2).

use crate::{EXC_MEMORY, FRAME_LEN, dot};

/// Excitation memory followed by one frame.
pub(crate) const BUFFER_LEN: usize = EXC_MEMORY + FRAME_LEN;
/// Erased frames in a row after which the output is muted.
pub(crate) const MAX_ERASED: u8 = 3;

/// The 16-bit linear congruential generator s = 521 s + 259.
#[derive(Clone, Debug, Default)]
pub(crate) struct Random(u16);

impl Random {
    pub(crate) fn new(seed: u16) -> Self {
        Self(seed)
    }

    pub(crate) fn next(&mut self) -> i16 {
        self.0 = self.0.wrapping_mul(521).wrapping_add(259);
        self.0 as i16
    }
}

/// Classifies the last 120 samples of a good frame as voiced or unvoiced. Returns the lag
/// to regenerate a voiced frame with (0 for unvoiced) and the energy of those samples.
pub(crate) fn classify(buf: &[f64; BUFFER_LEN], lag: usize) -> (usize, f64) {
    let lag = lag.min(142);
    let start = EXC_MEMORY + FRAME_LEN / 2;
    let current = &buf[start..];
    let mut best = 0.0;
    let mut best_lag = lag;
    for m in lag - 3..=lag + 3 {
        let c = dot(current, &buf[start - m..BUFFER_LEN - m]);
        if c > best {
            best = c;
            best_lag = m;
        }
    }
    let energy = dot(current, current);
    let past = &buf[start - best_lag..BUFFER_LEN - best_lag];
    let past_energy = dot(past, past);
    let voiced = best > 0.0 && 0.125 * past_energy * energy - best * best < 0.0;
    (if voiced { best_lag } else { 0 }, energy)
}

/// Generates the excitation of an erased frame. `erased` counts the erased frames in a row
/// (1 to 3) and `gain` is the already attenuated gain for unvoiced frames.
pub(crate) fn regenerate(
    buf: &mut [f64; BUFFER_LEN],
    erased: u8,
    lag: usize,
    gain: f64,
    random: &mut Random,
) -> [f64; FRAME_LEN] {
    let mut out = [0.0; FRAME_LEN];
    if erased >= MAX_ERASED {
        *buf = [0.0; BUFFER_LEN];
    } else if lag != 0 {
        for n in EXC_MEMORY..BUFFER_LEN {
            buf[n] = buf[n - lag];
        }
        for n in 0..FRAME_LEN {
            buf[EXC_MEMORY + n] *= 0.75;
            out[n] = buf[EXC_MEMORY + n];
        }
    } else {
        for x in &mut out {
            *x = gain * f64::from(random.next()) / 32768.0;
        }
        *buf = [0.0; BUFFER_LEN];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_sequence_starts_as_specified() {
        let mut random = Random::default();
        assert_eq!((random.next(), random.next()), (259, 4126));
    }

    #[test]
    fn periodic_frames_are_voiced() {
        let buf: [f64; BUFFER_LEN] =
            std::array::from_fn(|i| if i % 50 == 0 { 1000.0 } else { -20.0 });
        assert_eq!(classify(&buf, 51).0, 50);
    }

    #[test]
    fn voiced_regeneration_repeats_and_attenuates() {
        let mut buf: [f64; BUFFER_LEN] = std::array::from_fn(|i| (i % 40) as f64);
        let out = regenerate(&mut buf, 1, 40, 0.0, &mut Random::default());
        for n in 0..FRAME_LEN {
            assert_eq!(out[n], 0.75 * ((EXC_MEMORY + n) % 40) as f64);
        }
        assert_eq!(&buf[EXC_MEMORY..], &out[..]);
    }

    #[test]
    fn unvoiced_regeneration_clears_the_memory() {
        let mut buf = [5.0; BUFFER_LEN];
        let out = regenerate(&mut buf, 2, 0, 100.0, &mut Random::default());
        assert_eq!(out[0], 100.0 * 259.0 / 32768.0);
        assert_eq!(buf, [0.0; BUFFER_LEN]);
    }

    #[test]
    fn third_erasure_mutes() {
        let mut buf = [5.0; BUFFER_LEN];
        let out = regenerate(&mut buf, 3, 40, 100.0, &mut Random::default());
        assert_eq!(out, [0.0; FRAME_LEN]);
        assert_eq!(buf, [0.0; BUFFER_LEN]);
    }
}
