//! Conformance tests of the ITU-T G.723.1 Annex B test suite (floating-point
//! implementations). They need the test vectors of the ITU-T G.723.1 (05/2006) electronic
//! attachment: set G7231_VECTORS to its extracted `Software` directory and run
//! `cargo test --release -- --ignored`.

use std::path::PathBuf;

use g7231::{Decoder, Encoder, FRAME_LEN, Rate, frame_len};

const FRAMES_PER_GROUP: usize = 10;

fn read(name: &str) -> Vec<u8> {
    let root = std::env::var("G7231_VECTORS")
        .expect("G7231_VECTORS must name the Software directory of the ITU-T package");
    let path = PathBuf::from(root)
        .join("G.723.1-Annex-B-Software-Release.2LC/tv")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn pcm(bytes: &[u8]) -> Vec<i16> {
    bytes
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect()
}

fn frames(stream: &[u8]) -> Vec<&[u8]> {
    let mut frames = Vec::new();
    let mut rest = stream;
    while let Some(&first) = rest.first() {
        let (frame, tail) = rest.split_at(frame_len(first).min(rest.len()));
        frames.push(frame);
        rest = tail;
    }
    frames
}

fn decode(stream: &str, erasures: Option<&str>) -> Vec<i16> {
    let stream = read(stream);
    let erasures = erasures.map(|name| pcm(&read(name))).unwrap_or_default();
    let mut decoder = Decoder::new(true);
    let mut out = Vec::new();
    for (i, frame) in frames(&stream).into_iter().enumerate() {
        let erased = erasures.get(i).or(erasures.last()).is_some_and(|&w| w != 0);
        out.extend_from_slice(&decoder.decode(frame, erased));
    }
    out
}

fn snr(reference: &[i16], output: &[i16]) -> f64 {
    assert_eq!(reference.len(), output.len(), "output length");
    let mut energy = 0.0;
    let mut error = 0.0;
    for (&x, &y) in reference.iter().zip(output) {
        let (x, y) = (f64::from(x), f64::from(y));
        energy += x * x;
        error += (x - y) * (x - y);
    }
    if error == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (energy / error).log10()
    }
}

/// Decodes each stream and checks its SNR against the reference output. Each case is the
/// stream, the erasure file, the reference output and the threshold in dB.
fn check_decoder(cases: &[(&str, Option<&str>, &str, f64)]) {
    let mut failures = Vec::new();
    for &(stream, erasures, reference, threshold) in cases {
        let snr = snr(&pcm(&read(reference)), &decode(stream, erasures));
        println!("{stream} {erasures:?}: {snr:.2} dB (threshold {threshold})");
        if snr < threshold {
            failures.push(format!("{stream}: {snr:.2} dB < {threshold}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

fn encode(input: &str, rate: Rate, vad: bool) -> Vec<u8> {
    let samples = pcm(&read(input));
    let mut encoder = Encoder::new(rate, vad);
    let mut out = Vec::new();
    for (i, chunk) in samples.chunks_exact(FRAME_LEN).enumerate() {
        if i > 0 && i % FRAMES_PER_GROUP == 0 {
            encoder.reset();
        }
        let frame: &[i16; FRAME_LEN] = chunk.try_into().unwrap();
        out.extend_from_slice(encoder.encode(frame).as_bytes());
    }
    out
}

/// Encodes with a reset every 10 frames and requires that at least 95 percent of the
/// 10-frame groups match the reference bit for bit.
fn check_encoder(input: &str, rate: Rate, vad: bool, reference: &str) {
    let expected = read(reference);
    let output = encode(input, rate, vad);
    let expected: Vec<_> = frames(&expected)
        .chunks(FRAMES_PER_GROUP)
        .map(<[_]>::concat)
        .collect();
    let output: Vec<_> = frames(&output)
        .chunks(FRAMES_PER_GROUP)
        .map(<[_]>::concat)
        .collect();
    assert_eq!(expected.len(), output.len(), "number of frame groups");
    let matching = expected.iter().zip(&output).filter(|(e, o)| e == o).count();
    let percent = 100.0 * matching as f64 / expected.len() as f64;
    println!("{input}: {percent:.1} % of {} groups match", expected.len());
    assert!(
        percent >= 95.0,
        "{input}: only {percent:.1} % of the groups match"
    );
}

#[test]
#[ignore = "needs the ITU-T test vectors"]
fn encoder_5_3_without_vad() {
    check_encoder("TST53C.BIN", Rate::R53, false, "TST53C.C53");
}

#[test]
#[ignore = "needs the ITU-T test vectors"]
fn encoder_6_3_without_vad() {
    check_encoder("TST63C.BIN", Rate::R63, false, "TST63C.C63");
}

#[test]
#[ignore = "needs the ITU-T test vectors"]
fn encoder_5_3_with_vad() {
    check_encoder("TST53V.BIN", Rate::R53, true, "TST53V.C53");
}

#[test]
#[ignore = "needs the ITU-T test vectors"]
fn encoder_6_3_with_vad() {
    check_encoder("TST63V.BIN", Rate::R63, true, "TST63V.C63");
}

#[test]
#[ignore = "needs the ITU-T test vectors"]
fn decoder_5_3_without_cng() {
    check_decoder(&[
        ("DM29.C5H", Some("DM29.BFI"), "DM29.B5P", 63.0),
        ("DM29.C5H", None, "DM29.R5P", 63.0),
        ("DMBIS.C5H", None, "DMBIS.R5P", 62.0),
        ("IN1.C5H", None, "IN1.R5P", 75.0),
        ("IN2.C5H", None, "IN2.R5P", 80.0),
        ("IN3.C5H", None, "IN3.R5P", 58.0),
        ("IN4.C5H", None, "IN4.R5P", 60.0),
        ("IN5.C5H", None, "IN5.R5P", 64.0),
        ("IN6.C5H", None, "IN6.R5P", 77.0),
        ("INEQC53.C5H", None, "INEQC53.R5P", 59.0),
        ("OVERC53H.C5H", None, "OVERC53H.R5P", 84.0),
        ("PATHC53.C5H", None, "PATHC53.R5P", 51.0),
        ("READ7.C5H", None, "READ7.R5P", 58.0),
        ("TEST16.C5H", None, "TEST16.R5P", 61.0),
    ]);
}

#[test]
#[ignore = "needs the ITU-T test vectors"]
fn decoder_6_3_without_cng() {
    check_decoder(&[
        ("CODEC63.C6H", None, "CODEC63.R6P", 59.0),
        ("DM29.C6H", Some("DM29.BFI"), "DM29.B6P", 63.0),
        ("DM29.C6H", None, "DM29.R6P", 63.0),
        ("DMBIS.C6H", None, "DMBIS.R6P", 63.0),
        ("IN1.C6H", None, "IN1.R6P", 77.0),
        ("IN2.C6H", None, "IN2.R6P", 81.0),
        ("IN3.C6H", None, "IN3.R6P", 58.0),
        ("IN4.C6H", None, "IN4.R6P", 60.0),
        ("IN5.C6H", None, "IN5.R6P", 64.0),
        ("IN6.C6H", None, "IN6.R6P", 75.0),
        ("OVERC63.C6H", None, "OVERC63.R6P", 77.0),
        ("PATHC63H.C6H", None, "PATHC63H.R6P", 61.0),
        ("READ7.C6H", None, "READ7.R6P", 58.0),
        ("TEST16.C6H", None, "TEST16.R6P", 61.0),
    ]);
}

#[test]
#[ignore = "needs the ITU-T test vectors"]
fn decoder_mixed_rate_without_cng() {
    check_decoder(&[("TEST16.CMH", Some("TEST16.BFI"), "TEST16.BMP", 60.0)]);
}

#[test]
#[ignore = "needs the ITU-T test vectors"]
fn decoder_5_3_with_cng() {
    check_decoder(&[
        ("DM29.C5V", None, "DM29.R5V", 63.0),
        ("DMBIS.C5V", None, "DMBIS.R5V", 62.0),
        ("IN5.C5V", None, "IN5.R5V", 64.0),
        ("PATHC53.C5V", None, "PATHC53.R5V", 51.0),
        ("READ7.C5V", None, "READ7.R5V", 58.0),
        ("TEST16.C5V", None, "TEST16.R5V", 60.0),
        ("VAD6353.C5V", None, "VAD6353.R5V", 57.0),
    ]);
}

#[test]
#[ignore = "needs the ITU-T test vectors"]
fn decoder_6_3_with_cng() {
    check_decoder(&[
        ("DM29.C6V", None, "DM29.R6V", 63.0),
        ("DMBIS.C6V", None, "DMBIS.R6V", 62.0),
        ("IN5.C6V", None, "IN5.R6V", 64.0),
        ("PATHC63H.C6V", None, "PATHC63H.R6V", 61.0),
        ("READ7.C6V", None, "READ7.R6V", 58.0),
        ("TEST16.C6V", None, "TEST16.R6V", 60.0),
        ("VAD6353.C6V", None, "VAD6353.R6V", 57.0),
    ]);
}

#[test]
#[ignore = "needs the ITU-T test vectors"]
fn decoder_mixed_rate_with_cng() {
    check_decoder(&[("VAD6353.CMV", Some("VAD6353.BFI"), "VAD6353.BMV", 57.0)]);
}
