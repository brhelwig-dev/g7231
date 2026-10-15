//! The channel adaptation layer (C.2.1 to C.2.3, C.3.2): gain decompression, LPC
//! reordering and the ordering of bits by sensitivity, and their inverses.

use super::{MODES, field, order_entry, push_field, to_octets};
use crate::tables::{
    LSP_REORDER_FORWARD_0, LSP_REORDER_FORWARD_1, LSP_REORDER_FORWARD_2, LSP_REORDER_INVERSE_0,
    LSP_REORDER_INVERSE_1, LSP_REORDER_INVERSE_2,
};

const SID: usize = 2;
const MAX_LAG_INDEX: u32 = 123;
/// Bits of the packed frame from the grid bits on, which the adaptation copies.
const HIGH_RATE_TAIL: (usize, usize) = (97, 95);
const LOW_RATE_TAIL: (usize, usize) = (96, 64);
const ADAPTED_TAIL_START: usize = 100;

fn reorder(lpc: u32, tables: [&[u8; 256]; 3]) -> u32 {
    (0..3).fold(0, |v, i| {
        v | (u32::from(tables[i][((lpc >> (8 * i)) & 0xff) as usize]) << (8 * i))
    })
}

fn uses_small_codebook(mode: usize, lag_index: u32) -> bool {
    mode == 0 && lag_index + 18 < 58
}

fn tail(mode: usize) -> (usize, usize) {
    if mode == 0 {
        HIGH_RATE_TAIL
    } else {
        LOW_RATE_TAIL
    }
}

/// Bits of the frame whose bit ordering table applies: the channel adapted frame of an
/// active frame, the packed frame of a SID frame.
fn adapt(frame: &[u8], mode: usize) -> Vec<u8> {
    let octets = MODES[mode].adapted_octets;
    if mode == SID {
        return (0..8 * octets).map(|i| super::bit(frame, i)).collect();
    }
    let mut bits = Vec::with_capacity(8 * octets);
    push_field(&mut bits, mode as u32, 2);
    let lpc = field(frame, 2, 24);
    let forward = [
        &LSP_REORDER_FORWARD_0,
        &LSP_REORDER_FORWARD_1,
        &LSP_REORDER_FORWARD_2,
    ];
    push_field(&mut bits, reorder(lpc, forward), 24);
    for (start, width) in [(26, 7), (33, 2), (35, 7), (42, 2)] {
        push_field(&mut bits, field(frame, start, width), width);
    }
    let lags = [field(frame, 26, 7), field(frame, 35, 7)];
    let gains: [u32; 4] = std::array::from_fn(|i| field(frame, 44 + 12 * i, 12));
    let mut fixed = [0; 4];
    for i in 0..4 {
        let (train, g) = if uses_small_codebook(mode, lags[i / 2]) {
            (gains[i] >> 11, gains[i] & 2047)
        } else {
            (0, gains[i])
        };
        push_field(&mut bits, g / 24 + 128 * train, 8);
        fixed[i] = g % 24;
    }
    for f in fixed {
        push_field(&mut bits, f, 5);
    }
    for i in 92..96 {
        bits.push(super::bit(frame, i));
    }
    let (start, len) = tail(mode);
    bits.extend((start..start + len).map(|i| super::bit(frame, i)));
    bits.resize(8 * octets, 0);
    bits
}

/// The ordered bitstream of a frame of source mode `mode`, most sensitive bit first.
pub(super) fn order(frame: &[u8], mode: usize) -> Vec<u8> {
    let adapted = adapt(frame, mode);
    let mut ordered = vec![0; MODES[mode].info_bits];
    for row in 0..MODES[mode].adapted_octets {
        for column in 0..8 {
            let q = order_entry(mode, row, column);
            if q >= 0 {
                ordered[q as usize] = adapted[8 * row + 7 - column];
            }
        }
    }
    ordered
}

/// The last valid values of the decoded parameters, which replace forbidden ones.
#[derive(Clone, Debug, Default)]
pub(super) struct LastValid {
    lags: [u32; 2],
    pitch_gain: u32,
    train: u32,
    fixed_gain: u32,
}

/// Rebuilds the packed frame from an ordered bitstream. Returns the frame and whether a
/// forbidden index had to be replaced.
pub(super) fn restore(ordered: &[u8], mode: usize, last: &mut LastValid) -> (Vec<u8>, bool) {
    let octets = MODES[mode].adapted_octets;
    let mut adapted = vec![0; 8 * octets];
    for row in 0..octets {
        for column in 0..8 {
            let q = order_entry(mode, row, column);
            if q >= 0 {
                adapted[8 * row + 7 - column] = ordered[q as usize];
            }
        }
    }
    adapted[0] = (mode & 1) as u8;
    adapted[1] = (mode >> 1) as u8;
    let adapted = to_octets(&adapted, octets);
    if mode == SID {
        return (adapted, false);
    }

    let mut errors = 0;
    let mut bits = Vec::with_capacity(8 * 24);
    push_field(&mut bits, mode as u32, 2);
    let inverse = [
        &LSP_REORDER_INVERSE_0,
        &LSP_REORDER_INVERSE_1,
        &LSP_REORDER_INVERSE_2,
    ];
    push_field(&mut bits, reorder(field(&adapted, 2, 24), inverse), 24);
    let mut lags = [field(&adapted, 26, 7), field(&adapted, 35, 7)];
    let mut forbidden_lag = [false; 2];
    for pair in 0..2 {
        if lags[pair] > MAX_LAG_INDEX {
            lags[pair] = last.lags[pair];
            forbidden_lag[pair] = true;
            errors += pair + 1;
        } else {
            last.lags[pair] = lags[pair];
        }
    }
    push_field(&mut bits, lags[0], 7);
    push_field(&mut bits, field(&adapted, 33, 2), 2);
    push_field(&mut bits, lags[1], 7);
    push_field(&mut bits, field(&adapted, 42, 2), 2);
    for i in 0..4 {
        let small = uses_small_codebook(mode, lags[i / 2]);
        let raw = field(&adapted, 44 + 8 * i, 8);
        let (mut train, mut g) = if small {
            (raw >> 7, raw & 127)
        } else {
            (0, raw)
        };
        let bound = if small { 85 } else { 170 };
        if g >= bound || forbidden_lag[i / 2] {
            (g, train) = (last.pitch_gain, last.train);
            errors += 1;
        } else {
            (last.pitch_gain, last.train) = (g, train);
        }
        let mut fixed = field(&adapted, 76 + 5 * i, 5);
        if fixed >= 24 {
            fixed = last.fixed_gain;
            errors += 1;
        } else {
            last.fixed_gain = fixed;
        }
        let small_train = if small { 2048 * train } else { 0 };
        push_field(&mut bits, 24 * g + fixed + small_train, 12);
    }
    for i in 96..100 {
        bits.push(super::bit(&adapted, i));
    }
    if mode == 0 {
        bits.push(0);
    }
    let len = tail(mode).1;
    bits.extend((ADAPTED_TAIL_START..ADAPTED_TAIL_START + len).map(|i| super::bit(&adapted, i)));
    let octets = if mode == 0 { 24 } else { 20 };
    (to_octets(&bits, octets), errors > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reordering_tables_invert() {
        let forward = [
            &LSP_REORDER_FORWARD_0,
            &LSP_REORDER_FORWARD_1,
            &LSP_REORDER_FORWARD_2,
        ];
        let inverse = [
            &LSP_REORDER_INVERSE_0,
            &LSP_REORDER_INVERSE_1,
            &LSP_REORDER_INVERSE_2,
        ];
        for lpc in [0, 0x123456, 0xffffff, 0xc884b1] {
            assert_eq!(reorder(reorder(lpc, forward), inverse), lpc);
        }
    }

    #[test]
    fn matches_the_reference_example() {
        let mut frame = [0; 24];
        frame[..6].copy_from_slice(&[0x54, 0xbe, 0xf8, 0x08, 0xfe, 0x0a]);
        let adapted = to_octets(&adapt(&frame, 0), 25);
        assert_eq!(&adapted[..6], &[0x90, 0x9f, 0x3e, 0x0b, 0xfe, 0x0a]);
    }
}
