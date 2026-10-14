//! The channel encoder (C.2).

use super::adaptation::order;
use super::allocation::Allocation;
use super::convolutional::{encode, exchange_trellis_end};
use super::crc::parity;
use super::{
    AllowedRates, MODES, RateTooLow, UNTRANSMITTED_MARKER, config_bits, frame_octets, to_octets,
};

/// Highest channel bit rate each source mode can use.
const MAX_RATES: [u16; 3] = [20634, 17534, 4334];

/// The Annex C channel encoder.
#[derive(Clone, Debug)]
pub struct ChannelEncoder {
    /// `None` in the test mode, which codes every frame at its requested rate.
    allowed: Option<AllowedRates>,
    /// The configuration index of the previous frame and its allocation.
    allocation: Option<(usize, Allocation)>,
}

impl ChannelEncoder {
    /// Creates an encoder that maps each requested rate to the next lower of the two rates
    /// allowed for the source mode of the frame.
    pub fn new(allowed: AllowedRates) -> Self {
        Self {
            allowed: Some(allowed),
            allocation: None,
        }
    }

    /// Creates an encoder in the test mode of the conformance sequences: every frame is coded
    /// at its requested rate, limited to the highest useful rate, with channel
    /// configuration 0. This is not an operating mode.
    pub fn test_mode() -> Self {
        Self {
            allowed: None,
            allocation: None,
        }
    }

    /// Codes one G.723.1 frame for a channel of `requested` bit/s. An untransmitted frame
    /// becomes the two octets [`UNTRANSMITTED_MARKER`] whatever the rate.
    ///
    /// [`UNTRANSMITTED_MARKER`]: super::UNTRANSMITTED_MARKER
    pub fn encode(&mut self, frame: &[u8], requested: u16) -> Result<Vec<u8>, RateTooLow> {
        let mode = usize::from(frame[0] & 3);
        if mode == 3 {
            return Ok(UNTRANSMITTED_MARKER.to_vec());
        }
        let (configuration, rate) = match &self.allowed {
            Some(allowed) => {
                let [low, high] = allowed[mode];
                if requested >= high {
                    (1, high)
                } else if requested >= low {
                    (0, low)
                } else {
                    return Err(RateTooLow);
                }
            }
            None => (0, requested.min(MAX_RATES[mode])),
        };
        let index = 4 * configuration + mode;
        let allocation = match self.allocation {
            Some((previous, allocation)) if previous == index => allocation,
            _ => {
                let allocation = Allocation::new(mode, rate)?;
                self.allocation = Some((index, allocation));
                allocation
            }
        };

        let window = MODES[mode].crc_window;
        let ordered = order(frame, mode);
        let mut bs = ordered[..window].to_vec();
        bs.extend_from_slice(&parity(&ordered[..window]));
        bs.extend_from_slice(&ordered[window..]);
        exchange_trellis_end(&mut bs, &allocation, window);

        let mut channel = config_bits(index).to_vec();
        channel.extend(encode(&bs, &allocation));
        let octets = frame_octets(rate);
        assert!(channel.len() <= 8 * octets, "allocation exceeds the frame");
        Ok(to_octets(&channel, octets))
    }
}
