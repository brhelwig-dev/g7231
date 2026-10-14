//! The channel decoder (C.3). Beyond what the Recommendation makes mandatory it follows the
//! example of the reference: maximum correlation decoding of the configuration bits and a
//! hard-output Viterbi decoder.

use super::adaptation::{LastValid, restore};
use super::allocation::Allocation;
use super::convolutional::{decode, exchange_trellis_end};
use super::crc::parity;
use super::{AllowedRates, CONFIG_BITS, CRC_BITS, MODES, RECORD_LEN};
use crate::tables::CONFIG_BLOCK_CODEWORDS;

/// Correlation margin above which a change of configuration is believed.
const MIN_CONFIDENCE: i64 = 1000;

/// A frame recovered by the channel decoder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Received {
    /// The packed G.723.1 frame.
    pub frame: Vec<u8>,
    /// Bad frame indication: errors remain among the most sensitive bits. The speech
    /// decoder should treat the frame as erased.
    pub bfi: bool,
    /// Erroneous frame indication: errors remain among the less sensitive bits. Detecting
    /// them needs reliability values from the Viterbi decoder, which this decoder does not
    /// produce, so it sets the flag only together with `bfi` for a frame it could not
    /// accept.
    pub efi: bool,
    /// Forbidden index indication: a forbidden lag or gain index was replaced.
    pub fii: bool,
}

/// The Annex C channel decoder.
#[derive(Clone, Debug)]
pub struct ChannelDecoder {
    allowed: AllowedRates,
    accepted: usize,
    allocation: Option<(usize, Allocation)>,
    previous: Option<Received>,
    last_valid: LastValid,
}

impl ChannelDecoder {
    pub fn new(allowed: AllowedRates) -> Self {
        Self {
            allowed,
            accepted: 0,
            allocation: None,
            previous: None,
            last_valid: LastValid::default(),
        }
    }

    /// Decodes one frame from its soft values: one per channel bit, positive for 0 and
    /// negative for 1, with 0 for bits not received. A record of zeros is an untransmitted
    /// frame.
    pub fn decode(&mut self, soft: &[i8; RECORD_LEN]) -> Received {
        if soft.iter().all(|&s| s == 0) {
            return Received {
                frame: vec![3],
                bfi: false,
                efi: false,
                fii: false,
            };
        }

        let correlations: Vec<i64> = CONFIG_BLOCK_CODEWORDS
            .iter()
            .map(|row| (0..CONFIG_BITS).map(|n| i64::from(soft[n]) * i64::from(row[n])).sum())
            .collect();
        let mut index = 0;
        for (m, &d) in correlations.iter().enumerate() {
            if d > correlations[index] {
                index = m;
            }
        }
        let second = (0..correlations.len())
            .filter(|&m| m != index)
            .map(|m| correlations[m])
            .max()
            .unwrap_or(i64::MIN);
        let confidence = ((correlations[index] - second) / 8).min(32767);
        if index != self.accepted {
            if confidence > MIN_CONFIDENCE && index % 4 != 3 {
                self.accepted = index;
            } else {
                return match &self.previous {
                    Some(previous) => Received {
                        bfi: true,
                        efi: true,
                        fii: false,
                        ..previous.clone()
                    },
                    None => Received {
                        frame: vec![3],
                        bfi: true,
                        efi: true,
                        fii: false,
                    },
                };
            }
        }

        let (mode, configuration) = (index % 4, index / 4);
        let allocation = match self.allocation {
            Some((previous, allocation)) if previous == index => allocation,
            _ => {
                let rate = self.allowed[mode][configuration];
                let allocation =
                    Allocation::new(mode, rate).expect("configured rates suffice for each mode");
                self.allocation = Some((index, allocation));
                allocation
            }
        };

        let window = MODES[mode].crc_window;
        let mut bs = decode(soft, &allocation, MODES[mode].info_bits + CRC_BITS);
        exchange_trellis_end(&mut bs, &allocation, window);
        let bfi = parity(&bs[..window])[..] != bs[window..window + CRC_BITS];
        let mut ordered = bs[..window].to_vec();
        ordered.extend_from_slice(&bs[window + CRC_BITS..]);
        let (frame, fii) = restore(&ordered, mode, &mut self.last_valid);
        let received = Received {
            frame,
            bfi,
            efi: false,
            fii,
        };
        self.previous = Some(received.clone());
        received
    }
}
