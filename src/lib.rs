//! ITU-T G.723.1 dual rate speech coder at 6.3 and 5.3 kbit/s, with the Annex A silence
//! compression scheme and the Annex C channel coding scheme.
//!
//! The coder works on frames of 240 samples of 16-bit PCM at 8 kHz (30 ms).
//!
//! ```
//! use g7231::{Decoder, Encoder, FRAME_LEN, Rate};
//!
//! let mut encoder = Encoder::new(Rate::R63, true);
//! let mut decoder = Decoder::new(true);
//! let pcm: [i16; FRAME_LEN] = std::array::from_fn(|n| (2000.0 * (n as f64 * 0.1).sin()) as i16);
//! let frame = encoder.encode(&pcm);
//! assert_eq!(frame.as_bytes().len(), 24);
//! let speech = decoder.decode(frame.as_bytes(), false);
//! assert_eq!(speech.len(), FRAME_LEN);
//! ```

#![forbid(unsafe_code)]
#![allow(clippy::needless_range_loop)]

mod acelp;
mod bitstream;
pub mod channel;
mod cng;
mod concealment;
mod decoder;
mod encoder;
mod highpass;
mod lpc;
mod lsp;
mod mpmlq;
mod pitch;
mod postfilter;
#[allow(dead_code)]
mod tables;
mod vad;
mod weighting;

pub use decoder::Decoder;
pub use encoder::{Encoder, Frame};

/// Number of octets of the frame whose first octet is `first_octet`: 24 or 20 for speech at
/// 6.3 or 5.3 kbit/s, 4 for a SID frame, 1 for an untransmitted frame.
pub fn frame_len(first_octet: u8) -> usize {
    bitstream::frame_len(first_octet)
}

/// Samples per frame (30 ms at 8 kHz).
pub const FRAME_LEN: usize = 240;
const SUBFRAME_LEN: usize = 60;
const LPC_ORDER: usize = 10;
/// Length of the excitation memory: the largest pitch lag plus the predictor taps.
const EXC_MEMORY: usize = 145;
/// Energies at or below this value count as zero.
const MIN_ENERGY: f64 = 1.175_494_35e-38;

/// Coder bit rate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Rate {
    /// 6.3 kbit/s, multipulse excitation.
    #[default]
    R63,
    /// 5.3 kbit/s, algebraic code excitation.
    R53,
}

impl Rate {
    fn index(self) -> usize {
        match self {
            Rate::R63 => 0,
            Rate::R53 => 1,
        }
    }
}

/// Limits an excitation sample to the 16-bit range without rounding it.
fn clip(x: f64) -> f64 {
    if x < -32767.5 {
        -32768.0
    } else if x > 32766.5 {
        32767.0
    } else {
        x
    }
}

/// Dot product accumulated from 0.0 in index order.
fn dot(a: &[f64], b: &[f64]) -> f64 {
    let mut sum = 0.0;
    for (x, y) in a.iter().zip(b) {
        sum += x * y;
    }
    sum
}
