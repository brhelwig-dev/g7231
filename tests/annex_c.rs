//! Conformance tests of the ITU-T G.723.1 Annex C channel encoder. They need the test
//! sequences of the ITU-T G.723.1 (05/2006) electronic attachment: set G7231_VECTORS to its
//! extracted `Software` directory and run `cargo test --release -- --ignored`.

use std::path::PathBuf;

use g7231::channel::{AllowedRates, ChannelDecoder, ChannelEncoder, RECORD_LEN};
use g7231::frame_len;

fn read(name: &str) -> Vec<u8> {
    let root = std::env::var("G7231_VECTORS")
        .expect("G7231_VECTORS must name the Software directory of the ITU-T package");
    let path = PathBuf::from(root)
        .join("G.723.1-Annex-C-Software-1996-11(unchanged)")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Reads a rate configuration file: pairs of rates for the low rate, the high rate and SID
/// frames, each first value after any comment lines and the second on the next line.
fn allowed_rates(name: &str) -> AllowedRates {
    let text = String::from_utf8(read(name)).unwrap();
    let mut lines = text.lines();
    let mut value = |skip_comments: bool| -> u16 {
        let line = lines
            .by_ref()
            .find(|line| !(skip_comments && line.starts_with('#')))
            .unwrap();
        line.trim().parse().unwrap()
    };
    let mut pairs = [[0; 2]; 3];
    for mode in [1, 0, 2] {
        pairs[mode] = [value(true), value(false)];
    }
    pairs
}

fn frames(stream: &[u8]) -> Vec<&[u8]> {
    let mut frames = Vec::new();
    let mut rest = stream;
    while let Some(&first) = rest.first() {
        let (frame, tail) = rest.split_at(frame_len(first));
        frames.push(frame);
        rest = tail;
    }
    frames
}

fn encode(mut encoder: ChannelEncoder) -> Vec<u8> {
    let input = read("INFOBIT.TCO");
    let mut rates = read("CHANRATE.REQ")
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect::<Vec<_>>()
        .into_iter();
    let mut out = Vec::new();
    for frame in frames(&input) {
        let rate = if frame[0] & 3 == 3 {
            0
        } else {
            rates.next().unwrap()
        };
        out.extend(encoder.encode(frame, rate).unwrap());
    }
    out
}

fn check(encoder: ChannelEncoder, reference: &str) {
    let expected = read(reference);
    let output = encode(encoder);
    assert_eq!(output.len(), expected.len(), "{reference}: length");
    let first = output.iter().zip(&expected).position(|(a, b)| a != b);
    assert_eq!(first, None, "{reference}: first differing octet");
}

#[test]
#[ignore = "needs the ITU-T test vectors"]
fn channel_encoder_bitrate0() {
    check(
        ChannelEncoder::new(allowed_rates("BITRATE0.CFG")),
        "CHANBIT0.TX",
    );
}

#[test]
#[ignore = "needs the ITU-T test vectors"]
fn channel_encoder_bitrate1() {
    check(
        ChannelEncoder::new(allowed_rates("BITRATE1.CFG")),
        "CHANBIT1.TX",
    );
}

#[test]
#[ignore = "needs the ITU-T test vectors"]
fn channel_encoder_test_mode() {
    check(ChannelEncoder::test_mode(), "CHANBIT2.TX");
}

#[test]
#[ignore = "needs the ITU-T test vectors"]
fn channel_decoder_round_trip() {
    for config in ["BITRATE0.CFG", "BITRATE1.CFG"] {
        let allowed = allowed_rates(config);
        let mut encoder = ChannelEncoder::new(allowed);
        let mut decoder = ChannelDecoder::new(allowed);
        let input = read("INFOBIT.TCO");
        let mut rates = read("CHANRATE.REQ")
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect::<Vec<_>>()
            .into_iter();
        for frame in frames(&input) {
            let coded = encoder.encode(frame, rates.next().unwrap()).unwrap();
            let mut soft = [0i8; RECORD_LEN];
            for (i, s) in soft.iter_mut().enumerate().take(8 * coded.len()) {
                *s = if (coded[i / 8] >> (i % 8)) & 1 == 1 {
                    -127
                } else {
                    127
                };
            }
            let received = decoder.decode(&soft);
            assert!(!received.bfi && !received.fii);
            assert_eq!(received.frame, frame, "{config}");
        }
    }
}
