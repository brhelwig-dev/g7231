//! The speech encoder (clause 2).

use crate::bitstream::{self, ActiveFrame, GainCode, MAX_FRAME_BYTES, Params};
use crate::cng::{self, SEED};
use crate::concealment::Random;
use crate::highpass::HighPass;
use crate::lpc::{self, ANALYSIS_LEN, Autocorrelation, SineDetector, durbin};
use crate::lsp::{self, Lsp};
use crate::pitch::{self, Taming};
use crate::tables::LSP_DC_VECTOR;
use crate::vad::Vad;
use crate::weighting::{
    Combined, CombinedMemory, HarmonicFilter, WEIGHTED_LEN, WeightingFilter, Weights,
    open_loop_pitch,
};
use crate::{EXC_MEMORY, FRAME_LEN, LPC_ORDER, Rate, SUBFRAME_LEN, acelp, clip, mpmlq};

const LOOKAHEAD: usize = ANALYSIS_LEN - FRAME_LEN;

/// One coded frame: 24, 20, 4 or 1 octets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    bytes: [u8; MAX_FRAME_BYTES],
    len: usize,
}

impl Frame {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// G.723.1 speech encoder.
#[derive(Clone, Debug)]
pub struct Encoder {
    rate: Rate,
    high_pass: HighPass,
    /// The last 120 high-pass filtered samples.
    input_history: [f64; LOOKAHEAD],
    previous_lsp: Lsp,
    weighted_history: [f64; EXC_MEMORY],
    weighting: WeightingFilter,
    memory: CombinedMemory,
    excitation: [f64; EXC_MEMORY],
    sine: SineDetector,
    taming: Taming,
    silence: Option<Silence>,
}

/// The state of the silence compression scheme of Annex A.
#[derive(Clone, Debug)]
struct Silence {
    vad: Vad,
    /// Autocorrelation of the current frame and the three before it.
    autocorrelation: [Autocorrelation; 4],
    /// Prediction error energies of the inactive frames since the last SID frame, newest
    /// first.
    energies: [f64; 3],
    energy_count: usize,
    reference_gain: u8,
    /// Reference function of the filter described by the last SID frame.
    reference_function: Autocorrelation,
    sid_gain: f64,
    sid_lsp: Lsp,
    current_gain: f64,
    previous_active: bool,
    random: Random,
}

impl Default for Silence {
    fn default() -> Self {
        Self {
            vad: Vad::default(),
            autocorrelation: [[0.0; LPC_ORDER + 1]; 4],
            energies: [0.0; 3],
            energy_count: 0,
            reference_gain: 0,
            reference_function: [0.0; LPC_ORDER + 1],
            sid_gain: 0.0,
            sid_lsp: LSP_DC_VECTOR,
            current_gain: 0.0,
            previous_active: true,
            random: Random::new(SEED),
        }
    }
}

impl Encoder {
    /// Creates an encoder. With `vad` set, the encoder detects speech pauses and sends SID
    /// frames and untransmitted frames during them (Annex A).
    pub fn new(rate: Rate, vad: bool) -> Self {
        Self {
            rate,
            high_pass: HighPass::default(),
            input_history: [0.0; LOOKAHEAD],
            previous_lsp: LSP_DC_VECTOR,
            weighted_history: [0.0; EXC_MEMORY],
            weighting: WeightingFilter::default(),
            memory: CombinedMemory::default(),
            excitation: [0.0; EXC_MEMORY],
            sine: SineDetector::default(),
            taming: Taming::default(),
            silence: vad.then(Silence::default),
        }
    }

    /// Selects the rate of the following frames.
    pub fn set_rate(&mut self, rate: Rate) {
        self.rate = rate;
    }

    /// Restores the initial state, keeping the rate and the use of silence compression.
    pub fn reset(&mut self) {
        *self = Self::new(self.rate, self.silence.is_some());
    }

    /// Encodes one frame of 240 samples. The frame is delayed by 7.5 ms of look-ahead.
    pub fn encode(&mut self, pcm: &[i16; FRAME_LEN]) -> Frame {
        let x = self.high_pass.process(pcm);
        let mut b = [0.0; ANALYSIS_LEN];
        b[..LOOKAHEAD].copy_from_slice(&self.input_history);
        b[LOOKAHEAD..].copy_from_slice(&x);
        let analysis = lpc::analyse(&b, &mut self.sine);
        let unquantized = lsp::from_lpc(&analysis.lpc[3], &self.previous_lsp);
        let active = match &mut self.silence {
            Some(silence) => {
                silence.autocorrelation.copy_within(0..3, 1);
                let mut sum = [0.0; LPC_ORDER + 1];
                for r in &analysis.autocorrelation {
                    for j in 0..=LPC_ORDER {
                        sum[j] += r[j];
                    }
                }
                silence.autocorrelation[0] = sum;
                silence.vad.detect(&x, self.sine.detected())
            }
            None => true,
        };
        self.input_history.copy_from_slice(&b[FRAME_LEN..]);

        let weights = analysis.lpc.map(|a| Weights::new(&a));
        let mut g = [0.0; WEIGHTED_LEN];
        g[..EXC_MEMORY].copy_from_slice(&self.weighted_history);
        g[EXC_MEMORY..].copy_from_slice(&b[SUBFRAME_LEN..SUBFRAME_LEN + FRAME_LEN]);
        for (k, w) in weights.iter().enumerate() {
            let start = EXC_MEMORY + SUBFRAME_LEN * k;
            self.weighting.apply(w, &mut g[start..start + SUBFRAME_LEN]);
        }
        let open_loop = [EXC_MEMORY, EXC_MEMORY + 2 * SUBFRAME_LEN].map(|s| open_loop_pitch(&g, s));
        self.weighted_history.copy_from_slice(&g[FRAME_LEN..]);

        let params = if active {
            let frame = self.active_frame(&unquantized, &weights, &g, open_loop);
            if let Some(silence) = &mut self.silence {
                silence.previous_active = true;
                silence.random = Random::new(SEED);
            }
            Params::Active(frame)
        } else {
            self.inactive_frame(&weights)
        };
        if let Some(silence) = &mut self.silence {
            silence.vad.set_lags(open_loop);
        }

        let mut bytes = [0; MAX_FRAME_BYTES];
        let len = bitstream::pack(&params, &mut bytes);
        Frame { bytes, len }
    }

    fn active_frame(
        &mut self,
        unquantized: &Lsp,
        weights: &[Weights; 4],
        g: &[f64; WEIGHTED_LEN],
        open_loop: [usize; 2],
    ) -> ActiveFrame {
        let lsp_index = lsp::quantize(unquantized, &self.previous_lsp);
        let harmonic: [HarmonicFilter; 4] = std::array::from_fn(|k| {
            HarmonicFilter::new(g, EXC_MEMORY + SUBFRAME_LEN * k, open_loop[k / 2])
        });
        let targets: [[f64; SUBFRAME_LEN]; 4] =
            std::array::from_fn(|k| harmonic[k].target(g, EXC_MEMORY + SUBFRAME_LEN * k));

        let quantized_lsp = lsp::dequantize(lsp_index, &self.previous_lsp, false);
        let quantized = lsp::interpolate(&self.previous_lsp, &quantized_lsp);
        self.previous_lsp = quantized_lsp;

        let mut frame = ActiveFrame {
            rate: self.rate,
            lsp_index,
            ..ActiveFrame::default()
        };
        let mut acelp_carry = acelp::SEARCH_BUDGET;
        for k in 0..4 {
            let filter = Combined {
                quantized: &quantized[k],
                weights: &weights[k],
                harmonic: harmonic[k],
            };
            let h = filter.impulse_response();
            let mut target = targets[k];
            self.memory.subtract_ringing(&filter, &mut target);

            let lag_base = if k % 2 == 0 {
                open_loop[k / 2]
            } else {
                frame.lags[k / 2]
            };
            let choice = pitch::search(
                &target,
                &h,
                &self.excitation,
                k,
                lag_base,
                self.rate,
                &self.taming,
                self.sine.detected(),
            );
            if k % 2 == 0 {
                frame.lags[k / 2] = choice.pair_lag;
            } else {
                frame.lag_offsets[k / 2] = choice.offset as u8;
            }
            let row = &pitch::gain_codebook(self.rate, choice.pair_lag)[choice.gain_index];
            let adaptive = pitch::adaptive_vector(&self.excitation, choice.lag(), row);
            let residual = pitch::residual(&target, &adaptive, &h);
            let fixed = match self.rate {
                Rate::R63 => mpmlq::search(&residual, &h, k, choice.pair_lag),
                Rate::R53 => acelp::search(
                    &residual,
                    &h,
                    choice.lag(),
                    choice.gain_index,
                    &mut acelp_carry,
                ),
            };

            let excitation: [f64; SUBFRAME_LEN] =
                std::array::from_fn(|n| fixed.vector[n] + adaptive[n]);
            self.excitation.copy_within(SUBFRAME_LEN.., 0);
            for n in 0..SUBFRAME_LEN {
                self.excitation[EXC_MEMORY - SUBFRAME_LEN + n] = clip(excitation[n]);
            }
            self.taming.update(&choice, self.rate);
            self.memory.update(&filter, &excitation);

            frame.gains[k] = GainCode {
                pitch: choice.gain_index,
                fixed: fixed.gain_index,
                train: fixed.train,
            }
            .encode();
            frame.grids[k] = fixed.grid;
            frame.positions[k] = fixed.position;
            frame.signs[k] = fixed.signs;
        }
        frame
    }

    /// Codes a frame without speech as a SID frame or leaves it untransmitted, and brings
    /// the encoder to the state the decoder reaches with comfort noise (Annex A.4).
    fn inactive_frame(&mut self, weights: &[Weights; 4]) -> Params {
        let Some(silence) = &mut self.silence else {
            unreachable!("only the voice activity detector declares frames inactive");
        };
        silence.energies.copy_within(0..2, 1);
        let current = durbin(&silence.autocorrelation[0]);
        silence.energies[0] = current.energy;
        let mut sid = silence.previous_active;
        silence.energy_count = if sid {
            1
        } else {
            (silence.energy_count + 1).min(3)
        };
        let gain_index = cng::energy_gain_index(&silence.energies[..silence.energy_count]);
        sid = sid
            || cng::differs(
                &silence.reference_function,
                &silence.autocorrelation[0],
                current.energy,
            )
            || gain_index.abs_diff(silence.reference_gain) > 3;

        let params = if sid {
            let mut past = [0.0; LPC_ORDER + 1];
            for j in 0..=LPC_ORDER {
                past[j] = silence.autocorrelation[1][j]
                    + silence.autocorrelation[2][j]
                    + silence.autocorrelation[3][j];
            }
            let average = durbin(&past).lpc;
            if silence.vad.adapting() {
                silence.vad.noise_filter = average;
            }
            silence.reference_function = cng::reference_function(&average);
            let filter = if cng::differs(
                &silence.reference_function,
                &silence.autocorrelation[0],
                current.energy,
            ) {
                silence.reference_function = cng::reference_function(&current.lpc);
                current.lpc
            } else {
                average
            };
            let sid_lsp = lsp::from_lpc(&filter, &self.previous_lsp);
            let lsp_index = lsp::quantize(&sid_lsp, &self.previous_lsp);
            silence.sid_lsp = lsp::dequantize(lsp_index, &self.previous_lsp, false);
            silence.reference_gain = gain_index;
            silence.sid_gain = cng::decode_gain(gain_index);
            Params::Sid {
                lsp_index,
                gain_index,
            }
        } else {
            Params::Untransmitted
        };

        silence.current_gain = if silence.previous_active {
            silence.sid_gain
        } else {
            0.875 * silence.current_gain + 0.125 * silence.sid_gain
        };
        let noise = cng::excitation(
            silence.current_gain,
            &mut self.excitation,
            &mut silence.random,
        );
        let quantized = lsp::interpolate(&self.previous_lsp, &silence.sid_lsp);
        self.previous_lsp = silence.sid_lsp;
        silence.previous_active = false;

        for k in 0..4 {
            self.taming.update(&noise.pitch[k], self.rate);
            let filter = Combined {
                quantized: &quantized[k],
                weights: &weights[k],
                harmonic: HarmonicFilter::default(),
            };
            let start = SUBFRAME_LEN * k;
            let excitation: [f64; SUBFRAME_LEN] =
                std::array::from_fn(|n| noise.excitation[start + n]);
            self.memory.update(&filter, &excitation);
        }
        params
    }
}
