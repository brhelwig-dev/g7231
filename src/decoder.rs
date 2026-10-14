//! The speech decoder (clause 3).

use crate::bitstream::{self, ActiveFrame, Params};
use crate::cng::{self, SEED};
use crate::concealment::{self, BUFFER_LEN, MAX_ERASED, Random};
use crate::lsp::{self, Lpc, Lsp};
use crate::pitch;
use crate::postfilter::{FormantPostfilter, PitchPostfilter};
use crate::tables::{FIXED_GAIN_LEVELS, LSP_DC_VECTOR};
use crate::{EXC_MEMORY, FRAME_LEN, LPC_ORDER, Rate, SUBFRAME_LEN, acelp, clip, dot, mpmlq};

/// G.723.1 speech decoder.
#[derive(Clone, Debug)]
pub struct Decoder {
    postfilter: bool,
    previous_lsp: Lsp,
    excitation: [f64; EXC_MEMORY],
    synthesis: [f64; LPC_ORDER],
    formant: FormantPostfilter,
    erased_frames: u8,
    concealment_gain: f64,
    concealment_lag: usize,
    random: Random,
    previous_active: bool,
    /// The SID gain, or after a good active frame the energy of its last 120 excitation
    /// samples.
    sid_gain: f64,
    sid_lsp: Lsp,
    noise_gain: f64,
    noise_random: Random,
}

impl Decoder {
    /// Creates a decoder. The postfilter improves the perceived quality and is part of
    /// every conformance test.
    pub fn new(postfilter: bool) -> Self {
        Self {
            postfilter,
            previous_lsp: LSP_DC_VECTOR,
            excitation: [0.0; EXC_MEMORY],
            synthesis: [0.0; LPC_ORDER],
            formant: FormantPostfilter::default(),
            erased_frames: 0,
            concealment_gain: 0.0,
            concealment_lag: 0,
            random: Random::default(),
            previous_active: true,
            sid_gain: 0.0,
            sid_lsp: LSP_DC_VECTOR,
            noise_gain: 0.0,
            noise_random: Random::new(SEED),
        }
    }

    /// Restores the initial state.
    pub fn reset(&mut self) {
        *self = Self::new(self.postfilter);
    }

    /// Decodes one frame into 240 samples. `erased` marks a frame lost in transmission; its
    /// content is then ignored. A frame shorter than its type requires counts as erased.
    pub fn decode(&mut self, frame: &[u8], erased: bool) -> [i16; FRAME_LEN] {
        let params = match frame.first() {
            Some(&first) if !erased && frame.len() >= bitstream::frame_len(first) => {
                bitstream::unpack(&frame[..bitstream::frame_len(first)])
            }
            _ => None,
        };
        let active = match params {
            Some(Params::Active(_)) => true,
            None => self.previous_active,
            Some(_) => false,
        };
        let (excitation, lpc) = match params {
            Some(Params::Active(f)) => self.active(Some(&f)),
            _ if active => self.active(None),
            _ => self.comfort_noise(params),
        };
        self.previous_active = active;

        let mut out = [0; FRAME_LEN];
        for k in 0..4 {
            let mut block = [0.0; SUBFRAME_LEN];
            for n in 0..SUBFRAME_LEN {
                let sy = excitation[SUBFRAME_LEN * k + n] + dot(&lpc[k], &self.synthesis);
                self.synthesis.copy_within(0..LPC_ORDER - 1, 1);
                self.synthesis[0] = sy;
                block[n] = sy;
            }
            if self.postfilter {
                self.formant.process(&lpc[k], &mut block);
            }
            for n in 0..SUBFRAME_LEN {
                out[SUBFRAME_LEN * k + n] = to_pcm(block[n]);
            }
        }
        out
    }

    /// Decodes the excitation and LPC sets of an active frame, or conceals an erased one.
    fn active(&mut self, frame: Option<&ActiveFrame>) -> ([f64; FRAME_LEN], [Lpc; 4]) {
        self.erased_frames = match frame {
            Some(_) => 0,
            None => (self.erased_frames + 1).min(MAX_ERASED),
        };
        let index = frame.map_or([0; 3], |f| f.lsp_index);
        let lsp = lsp::dequantize(index, &self.previous_lsp, frame.is_none());
        let lpc = lsp::interpolate(&self.previous_lsp, &lsp);
        self.previous_lsp = lsp;

        let mut buf = [0.0; BUFFER_LEN];
        buf[..EXC_MEMORY].copy_from_slice(&self.excitation);
        let excitation = match frame {
            Some(f) => {
                let gain2 = f.gain_code(2).fixed;
                let gain3 = f.gain_code(3).fixed;
                self.concealment_gain = FIXED_GAIN_LEVELS[(gain2 + gain3) / 2];
                self.sid_lsp = lsp;
                self.good_excitation(f, &mut buf)
            }
            None => {
                self.concealment_gain *= 0.75;
                concealment::regenerate(
                    &mut buf,
                    self.erased_frames,
                    self.concealment_lag,
                    self.concealment_gain,
                    &mut self.random,
                )
            }
        };
        self.excitation.copy_from_slice(&buf[FRAME_LEN..]);
        self.noise_random = Random::new(SEED);
        (excitation, lpc)
    }

    /// Generates comfort noise for a SID frame, an untransmitted frame, or an erased frame
    /// during a speech pause (Annex A.5).
    fn comfort_noise(&mut self, params: Option<Params>) -> ([f64; FRAME_LEN], [Lpc; 4]) {
        if let Some(Params::Sid {
            lsp_index,
            gain_index,
        }) = params
        {
            self.sid_gain = cng::decode_gain(gain_index);
            self.sid_lsp = lsp::dequantize(lsp_index, &self.previous_lsp, false);
        } else if self.previous_active {
            self.sid_gain = cng::decode_gain(cng::excitation_gain_index(self.sid_gain));
        }
        self.noise_gain = if self.previous_active {
            self.sid_gain
        } else {
            0.875 * self.noise_gain + 0.125 * self.sid_gain
        };
        let noise = cng::excitation(
            self.noise_gain,
            &mut self.excitation,
            &mut self.noise_random,
        );
        let lpc = lsp::interpolate(&self.previous_lsp, &self.sid_lsp);
        self.previous_lsp = self.sid_lsp;
        (noise.excitation, lpc)
    }

    fn good_excitation(
        &mut self,
        f: &ActiveFrame,
        buf: &mut [f64; BUFFER_LEN],
    ) -> [f64; FRAME_LEN] {
        for k in 0..4 {
            let start = EXC_MEMORY + SUBFRAME_LEN * k;
            let gain = f.gain_code(k);
            let lag = f.subframe_lag(k);
            let fixed = match f.rate {
                Rate::R63 => mpmlq::decode(
                    f.positions[k],
                    f.signs[k],
                    gain.fixed,
                    f.grids[k],
                    gain.train,
                    k,
                    f.pair_lag(k),
                ),
                Rate::R53 => acelp::decode(
                    f.positions[k],
                    f.signs[k],
                    gain.fixed,
                    f.grids[k],
                    lag,
                    gain.pitch,
                ),
            };
            let row = &pitch::gain_codebook(f.rate, f.pair_lag(k))[gain.pitch];
            let adaptive = pitch::adaptive_vector(&buf[..start], lag, row);
            for n in 0..SUBFRAME_LEN {
                buf[start + n] = fixed[n] + adaptive[n];
            }
        }

        let unclipped: [f64; FRAME_LEN] = std::array::from_fn(|n| buf[EXC_MEMORY + n]);
        (self.concealment_lag, self.sid_gain) = concealment::classify(buf, f.lags[1]);
        let postfilters: [PitchPostfilter; 4] = std::array::from_fn(|k| {
            PitchPostfilter::search(buf, SUBFRAME_LEN * k, f.pair_lag(k), f.rate)
        });
        for x in &mut buf[EXC_MEMORY..] {
            *x = clip(*x);
        }
        if !self.postfilter {
            return unclipped;
        }
        let mut out = [0.0; FRAME_LEN];
        for (k, pf) in postfilters.iter().enumerate() {
            let start = SUBFRAME_LEN * k;
            pf.apply(buf, start, &mut out[start..start + SUBFRAME_LEN]);
        }
        out
    }
}

/// Rounds to the nearest 16-bit sample, halves away from zero, saturating.
fn to_pcm(x: f64) -> i16 {
    if x < -32767.5 {
        i16::MIN
    } else if x > 32766.5 {
        i16::MAX
    } else if x < 0.0 {
        (x - 0.5) as i16
    } else {
        (x + 0.5) as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_is_half_away_from_zero() {
        assert_eq!([2.5, -2.5, 2.49, -0.4].map(to_pcm), [3, -3, 2, 0]);
        let limits = [40000.0, -40000.0, 32766.5, -32767.5].map(to_pcm);
        assert_eq!(limits, [32767, -32768, 32767, -32768]);
    }

    #[test]
    fn erased_frames_fade_to_silence() {
        let mut decoder = Decoder::new(true);
        for _ in 0..3 {
            decoder.decode(&[], true);
        }
        assert_eq!(decoder.decode(&[], true), [0; FRAME_LEN]);
    }
}
