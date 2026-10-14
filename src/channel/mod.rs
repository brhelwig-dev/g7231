//! Annex C: scalable channel coding for transmission over error-prone channels.
//!
//! The channel encoder protects the bits of each G.723.1 frame according to their
//! sensitivity, with a CRC and a punctured convolutional code whose rate follows the
//! channel bit rate of the frame. The channel decoder reverses this and flags damaged
//! frames.

mod adaptation;
mod allocation;
mod convolutional;
mod crc;
mod decoder;
mod encoder;

pub use decoder::{ChannelDecoder, Received};
pub use encoder::ChannelEncoder;

use crate::tables::{BIT_ORDER_HIGH_RATE, BIT_ORDER_LOW_RATE, BIT_ORDER_SID, CONFIG_BLOCK_CODEWORDS};

/// The two octets sent for an untransmitted frame.
pub const UNTRANSMITTED_MARKER: [u8; 2] = [0xaf, 0x16];
/// Soft values per frame at the input of the channel decoder.
pub const RECORD_LEN: usize = 684;
const CONFIG_BITS: usize = 13;
const CRC_BITS: usize = 5;
const TAIL_BITS: usize = 4;

/// The channel bit rates in bit/s allowed for each source mode: high rate, low rate and SID
/// frames, each with the rate of channel configuration 0 and of configuration 1.
pub type AllowedRates = [[u16; 2]; 3];

/// The requested channel bit rate is below the least the source mode needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateTooLow;

impl std::fmt::Display for RateTooLow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("channel bit rate too low for the source mode")
    }
}

impl std::error::Error for RateTooLow {}

/// Properties of a source mode: 0 high rate, 1 low rate, 2 SID frame.
struct Mode {
    /// Bits of the ordered bitstream.
    info_bits: usize,
    /// Bits protected by the CRC.
    crc_window: usize,
    /// Octets of the frame the ordering table reads.
    adapted_octets: usize,
}

const MODES: [Mode; 3] = [
    Mode {
        info_bits: 193,
        crc_window: 44,
        adapted_octets: 25,
    },
    Mode {
        info_bits: 162,
        crc_window: 34,
        adapted_octets: 21,
    },
    Mode {
        info_bits: 30,
        crc_window: 30,
        adapted_octets: 4,
    },
];

/// The position in the ordered bitstream of bit `column` (most significant first) of
/// octet `row`, or -1.
fn order_entry(mode: usize, row: usize, column: usize) -> i16 {
    match mode {
        0 => BIT_ORDER_HIGH_RATE[row][column],
        1 => BIT_ORDER_LOW_RATE[row][column],
        _ => i16::from(BIT_ORDER_SID[row][column]),
    }
}

/// Bit `i` of a frame, counting from the least significant bit of the first octet.
fn bit(bytes: &[u8], i: usize) -> u8 {
    (bytes[i / 8] >> (i % 8)) & 1
}

/// The `width` bits of a frame from bit `start`, least significant first.
fn field(bytes: &[u8], start: usize, width: usize) -> u32 {
    (0..width).fold(0, |v, j| v | (u32::from(bit(bytes, start + j)) << j))
}

/// Appends `width` bits of `value`, least significant first.
fn push_field(bits: &mut Vec<u8>, value: u32, width: usize) {
    bits.extend((0..width).map(|j| ((value >> j) & 1) as u8));
}

fn to_octets(bits: &[u8], octets: usize) -> Vec<u8> {
    let mut out = vec![0; octets];
    for (i, &b) in bits.iter().enumerate() {
        out[i / 8] |= b << (i % 8);
    }
    out
}

/// The 13 configuration bits of configuration index `m`.
fn config_bits(m: usize) -> [u8; CONFIG_BITS] {
    CONFIG_BLOCK_CODEWORDS[m].map(|v| u8::from(v < 0))
}

/// Octets of a channel frame at channel bit rate `rate`.
fn frame_octets(rate: u16) -> usize {
    let rate = i64::from(rate);
    let mut octets = (2 * rate * 31457) >> 24;
    if (2 * octets * 17067) >> 7 < rate {
        octets += 1;
    }
    octets as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn octet_counts_follow_the_fixed_point_rule() {
        assert_eq!(frame_octets(7034), 27);
        assert_eq!(frame_octets(20634), 78);
        assert_eq!(frame_octets(1600), 6);
        assert_eq!(frame_octets(17067), 64);
        assert_eq!(frame_octets(17066), 64);
    }

    #[test]
    fn configuration_code_words_differ_in_seven_bits() {
        for a in 0..8 {
            for b in a + 1..8 {
                let d = (0..CONFIG_BITS)
                    .filter(|&n| config_bits(a)[n] != config_bits(b)[n])
                    .count();
                assert!(d >= 7);
            }
        }
        assert_eq!(config_bits(0), [0; CONFIG_BITS]);
    }

    #[test]
    fn ordering_tables_are_permutations() {
        for (m, mode) in MODES.iter().enumerate() {
            let mut seen = vec![false; mode.info_bits];
            for row in 0..mode.adapted_octets {
                for column in 0..8 {
                    let q = order_entry(m, row, column);
                    if q >= 0 {
                        assert!(!seen[q as usize]);
                        seen[q as usize] = true;
                    }
                }
            }
            assert!(seen.iter().all(|&s| s));
        }
    }
}
