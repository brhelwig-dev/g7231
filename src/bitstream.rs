//! Frame packing and unpacking (clause 4, Tables 5 and 6, and Table A.1).

use crate::Rate;
use crate::tables::PACKED_FRAME_BYTES;

/// Octets of a SID frame.
pub(crate) const SID_BYTES: usize = 4;
/// Octets of an untransmitted frame.
pub(crate) const UNTRANSMITTED_BYTES: usize = 1;
/// Octets of the longest frame.
pub(crate) const MAX_FRAME_BYTES: usize = 24;

/// Highest valid value of the 7-bit lag fields ACL0 and ACL2.
const MAX_LAG_FIELD: u32 = 123;
/// Smallest pitch lag.
pub(crate) const MIN_LAG: usize = 18;

/// Number of octets of a frame, from the frame type code in the two low bits of its first
/// octet.
pub(crate) fn frame_len(first_octet: u8) -> usize {
    match first_octet & 3 {
        0 => usize::from(PACKED_FRAME_BYTES[0]),
        1 => usize::from(PACKED_FRAME_BYTES[1]),
        2 => SID_BYTES,
        _ => UNTRANSMITTED_BYTES,
    }
}

/// The parameters of one active frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ActiveFrame {
    pub rate: Rate,
    /// The three 8-bit LSP codebook indices, sub-vectors 0 to 2.
    pub lsp_index: [u8; 3],
    /// Lag of subframes 0 and 1 (lag A) and of subframes 2 and 3 (lag B), 18 to 141.
    pub lags: [usize; 2],
    /// Lag offset codes of subframes 1 and 3, 0 to 3, meaning offsets -1 to +2.
    pub lag_offsets: [u8; 2],
    /// The 12-bit GAIN field of each subframe.
    pub gains: [u16; 4],
    pub grids: [u8; 4],
    /// The full pulse position index of each subframe.
    pub positions: [u32; 4],
    /// The PSIG field of each subframe.
    pub signs: [u8; 4],
}

impl ActiveFrame {
    /// The transmitted lag of the pair that subframe `s` belongs to.
    pub(crate) fn pair_lag(&self, s: usize) -> usize {
        self.lags[s / 2]
    }

    /// The adaptive codebook lag of subframe `s`.
    pub(crate) fn subframe_lag(&self, s: usize) -> usize {
        let code = if s % 2 == 1 {
            usize::from(self.lag_offsets[s / 2])
        } else {
            1
        };
        self.pair_lag(s) + code - 1
    }

    pub(crate) fn gain_code(&self, s: usize) -> GainCode {
        GainCode::decode(self.gains[s], self.rate, self.pair_lag(s))
    }
}

/// The contents of a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Params {
    Active(ActiveFrame),
    Sid { lsp_index: [u8; 3], gain_index: u8 },
    Untransmitted,
}

/// The parts of a 12-bit GAIN field (equations 36, 39 and 40).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GainCode {
    /// Index into the pitch gain codebook.
    pub pitch: usize,
    /// Index into FIXED_GAIN_LEVELS.
    pub fixed: usize,
    /// The pulse train flag of the high rate.
    pub train: bool,
}

impl GainCode {
    /// Whether the 85-entry pitch gain codebook serves a subframe pair with this lag.
    pub(crate) fn uses_small_codebook(rate: Rate, pair_lag: usize) -> bool {
        rate == Rate::R63 && pair_lag < 58
    }

    /// Size of the pitch gain codebook serving a subframe pair with this lag.
    pub(crate) fn codebook_size(rate: Rate, pair_lag: usize) -> usize {
        if Self::uses_small_codebook(rate, pair_lag) {
            85
        } else {
            170
        }
    }

    pub(crate) fn decode(field: u16, rate: Rate, pair_lag: usize) -> Self {
        let field = usize::from(field);
        if Self::uses_small_codebook(rate, pair_lag) {
            let rest = field % 2048;
            Self {
                pitch: rest / 24,
                fixed: rest % 24,
                train: field >= 2048,
            }
        } else {
            Self {
                pitch: field / 24,
                fixed: field % 24,
                train: false,
            }
        }
    }

    pub(crate) fn encode(self) -> u16 {
        let field = self.pitch * 24 + self.fixed + if self.train { 2048 } else { 0 };
        field as u16
    }
}

struct BitReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl BitReader<'_> {
    fn read(&mut self, width: u32) -> u32 {
        let mut value = 0;
        for j in 0..width {
            let bit = (self.bytes[self.pos / 8] >> (self.pos % 8)) & 1;
            value |= u32::from(bit) << j;
            self.pos += 1;
        }
        value
    }
}

struct BitWriter<'a> {
    out: &'a mut [u8],
    pos: usize,
}

impl BitWriter<'_> {
    fn write(&mut self, value: u32, width: u32) {
        for j in 0..width {
            let bit = ((value >> j) & 1) as u8;
            self.out[self.pos / 8] |= bit << (self.pos % 8);
            self.pos += 1;
        }
    }
}

fn lsp_id(index: [u8; 3]) -> u32 {
    (u32::from(index[0]) << 16) | (u32::from(index[1]) << 8) | u32::from(index[2])
}

fn lsp_index(id: u32) -> [u8; 3] {
    [(id >> 16) as u8, (id >> 8) as u8, id as u8]
}

/// Divisors of the high-order parts of the high rate position indices.
const POSITION_SPLIT: [u32; 4] = [1 << 16, 1 << 14, 1 << 16, 1 << 14];
/// Widths of the low-order parts of the high rate position indices.
const POSITION_LOW_BITS: [u32; 4] = [16, 14, 16, 14];
/// Widths of the high rate PSIG fields.
const HIGH_RATE_SIGN_BITS: [u32; 4] = [6, 5, 6, 5];

/// Unpacks a frame whose length is `frame_len` of its first octet. Returns `None` when a
/// lag or gain field holds a forbidden value; the decoder then treats the frame as erased.
pub(crate) fn unpack(bytes: &[u8]) -> Option<Params> {
    let mut r = BitReader { bytes, pos: 0 };
    let code = r.read(2);
    if code == 3 {
        return Some(Params::Untransmitted);
    }
    let lsp_index = lsp_index(r.read(24));
    if code == 2 {
        let gain_index = r.read(6) as u8;
        return Some(Params::Sid {
            lsp_index,
            gain_index,
        });
    }
    let rate = if code == 0 { Rate::R63 } else { Rate::R53 };
    let mut f = ActiveFrame {
        rate,
        lsp_index,
        ..ActiveFrame::default()
    };
    for pair in 0..2 {
        let lag = r.read(7);
        if lag > MAX_LAG_FIELD {
            return None;
        }
        f.lags[pair] = lag as usize + MIN_LAG;
        f.lag_offsets[pair] = r.read(2) as u8;
    }
    for s in 0..4 {
        f.gains[s] = r.read(12) as u16;
        let bound = GainCode::codebook_size(rate, f.pair_lag(s));
        if f.gain_code(s).pitch >= bound {
            return None;
        }
    }
    for grid in &mut f.grids {
        *grid = r.read(1) as u8;
    }
    match rate {
        Rate::R63 => {
            r.read(1);
            let msb = r.read(13);
            let (q1, q2) = (msb / 90, msb % 90);
            let high = [q1 / 9, q1 % 9, q2 / 9, q2 % 9];
            for s in 0..4 {
                f.positions[s] = high[s] * POSITION_SPLIT[s] + r.read(POSITION_LOW_BITS[s]);
            }
            for s in 0..4 {
                f.signs[s] = r.read(HIGH_RATE_SIGN_BITS[s]) as u8;
            }
        }
        Rate::R53 => {
            for position in &mut f.positions {
                *position = r.read(12);
            }
            for sign in &mut f.signs {
                *sign = r.read(4) as u8;
            }
        }
    }
    Some(Params::Active(f))
}

/// Packs a frame into `out` and returns its length in octets.
pub(crate) fn pack(params: &Params, out: &mut [u8; MAX_FRAME_BYTES]) -> usize {
    *out = [0; MAX_FRAME_BYTES];
    let mut w = BitWriter { out, pos: 0 };
    match params {
        Params::Untransmitted => {
            w.write(3, 2);
            UNTRANSMITTED_BYTES
        }
        Params::Sid {
            lsp_index,
            gain_index,
        } => {
            w.write(2, 2);
            w.write(lsp_id(*lsp_index), 24);
            w.write(u32::from(*gain_index), 6);
            SID_BYTES
        }
        Params::Active(f) => {
            let code = match f.rate {
                Rate::R63 => 0,
                Rate::R53 => 1,
            };
            w.write(code, 2);
            w.write(lsp_id(f.lsp_index), 24);
            for pair in 0..2 {
                w.write((f.lags[pair] - MIN_LAG) as u32, 7);
                w.write(u32::from(f.lag_offsets[pair]), 2);
            }
            for gain in f.gains {
                w.write(u32::from(gain), 12);
            }
            for grid in f.grids {
                w.write(u32::from(grid), 1);
            }
            match f.rate {
                Rate::R63 => {
                    let high: [u32; 4] =
                        std::array::from_fn(|s| f.positions[s] / POSITION_SPLIT[s]);
                    w.write(0, 1);
                    w.write((high[0] * 9 + high[1]) * 90 + high[2] * 9 + high[3], 13);
                    for s in 0..4 {
                        w.write(f.positions[s] % POSITION_SPLIT[s], POSITION_LOW_BITS[s]);
                    }
                    for s in 0..4 {
                        w.write(u32::from(f.signs[s]), HIGH_RATE_SIGN_BITS[s]);
                    }
                }
                Rate::R53 => {
                    for position in f.positions {
                        w.write(position, 12);
                    }
                    for sign in f.signs {
                        w.write(u32::from(sign), 4);
                    }
                }
            }
            frame_len(code as u8)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn high_rate_frame() -> ActiveFrame {
        ActiveFrame {
            rate: Rate::R63,
            lsp_index: [0x12, 0xab, 0xff],
            lags: [57, 141],
            lag_offsets: [0, 3],
            gains: [2048 + 84 * 24 + 23, 5, 169 * 24 + 23, 1000],
            grids: [1, 0, 1, 1],
            positions: [593_774, 142_505, 65_536 * 4 + 7, 16_384 * 8 + 1],
            signs: [0x3f, 0x15, 0x01, 0x10],
        }
    }

    fn low_rate_frame() -> ActiveFrame {
        ActiveFrame {
            rate: Rate::R53,
            lsp_index: [0, 1, 2],
            lags: [18, 100],
            lag_offsets: [1, 2],
            gains: [0, 4079, 1234, 24],
            grids: [0, 1, 0, 1],
            positions: [0, 4095, 2049, 77],
            signs: [0xf, 0, 0x5, 0xa],
        }
    }

    fn round_trip(params: Params) {
        let mut out = [0; MAX_FRAME_BYTES];
        let len = pack(&params, &mut out);
        assert_eq!(len, frame_len(out[0]));
        assert_eq!(unpack(&out[..len]), Some(params));
    }

    #[test]
    fn active_frames_round_trip() {
        round_trip(Params::Active(high_rate_frame()));
        round_trip(Params::Active(low_rate_frame()));
    }

    #[test]
    fn sid_and_untransmitted_frames_round_trip() {
        round_trip(Params::Sid {
            lsp_index: [3, 200, 77],
            gain_index: 63,
        });
        round_trip(Params::Untransmitted);
    }

    #[test]
    fn frame_lengths_follow_the_type_code() {
        assert_eq!(frame_len(0), 24);
        assert_eq!(frame_len(1), 20);
        assert_eq!(frame_len(2), 4);
        assert_eq!(frame_len(0xff), 1);
    }

    #[test]
    fn first_octets_follow_table_5() {
        let mut out = [0; MAX_FRAME_BYTES];
        let mut f = high_rate_frame();
        f.lsp_index = [0, 0, 0b10_1010];
        pack(&Params::Active(f), &mut out);
        assert_eq!(out[0], 0b1010_1000);
        assert_eq!(out[3] & 3, 0);
    }

    #[test]
    fn forbidden_lags_and_gains_are_rejected() {
        let mut out = [0; MAX_FRAME_BYTES];
        let mut f = low_rate_frame();
        pack(&Params::Active(f), &mut out);
        out[3] |= 0xfc;
        out[4] |= 1;
        assert_eq!(unpack(&out[..20]), None);

        f.gains[2] = 4080;
        pack(&Params::Active(f), &mut out);
        assert_eq!(unpack(&out[..20]), None);

        let mut f = high_rate_frame();
        f.gains[0] = 2048 + 85 * 24;
        pack(&Params::Active(f), &mut out);
        assert_eq!(unpack(&out[..24]), None);
    }

    #[test]
    fn gain_codes_split_by_codebook() {
        let small = GainCode::decode(2048 + 30 * 24 + 7, Rate::R63, 40);
        assert_eq!(
            small,
            GainCode {
                pitch: 30,
                fixed: 7,
                train: true
            }
        );
        assert_eq!(small.encode(), 2048 + 30 * 24 + 7);
        let large = GainCode::decode(150 * 24 + 3, Rate::R63, 58);
        assert_eq!(
            large,
            GainCode {
                pitch: 150,
                fixed: 3,
                train: false
            }
        );
    }

    #[test]
    fn subframe_lags_apply_the_offset_codes() {
        let f = high_rate_frame();
        assert_eq!([0, 1, 2, 3].map(|s| f.subframe_lag(s)), [57, 56, 141, 143]);
    }
}
