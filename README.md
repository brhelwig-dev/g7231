# g7231

A Rust implementation of ITU-T Recommendation G.723.1, the dual rate speech coder for
multimedia communications at 6.3 and 5.3 kbit/s.

- Encoder and decoder at both rates, with rate changes from frame to frame.
- Annex A: voice activity detection, SID frames and comfort noise generation.
- Frame erasure concealment.
- Annex C: scalable channel coding (channel encoder and decoder).
- 64-bit floating-point arithmetic, as specified in Annex B. No dependencies and no unsafe
  code.

## Usage

```rust
use g7231::{Decoder, Encoder, FRAME_LEN, Rate};

let mut encoder = Encoder::new(Rate::R63, true);
let mut decoder = Decoder::new(true);

let pcm = [0i16; FRAME_LEN]; // 30 ms of 16-bit PCM at 8 kHz
let frame = encoder.encode(&pcm); // 24, 20, 4 or 1 octets
let speech = decoder.decode(frame.as_bytes(), false);
```

`Encoder::new` takes the rate and whether to use the silence compression of Annex A.
`Decoder::new` takes whether to use the postfilter. Pass `true` as the second argument of
`Decoder::decode` for a frame lost in transmission. `g7231::frame_len` gives the length of
a frame from its first octet, for splitting a stream of frames.

The `channel` module holds the Annex C channel encoder and decoder.

## Conformance

The Annex B floating-point test suite and the Annex C test sequences of the ITU-T G.723.1
(05/2006) electronic attachment run in CI:

- Every decoder test stream, with and without comfort noise, frame erasures and rate
  changes, decodes to output identical to the reference.
- The encoder output, with and without silence compression, matches the reference in all
  10-frame groups. The suite requires 95 percent.
- The Annex C channel encoder reproduces all three test sequences octet for octet.

The 5.2 release of the reference software draws the random lag of the second half of a
comfort noise frame from 19 values. The test vectors in the electronic attachment were
generated with 21 values, and this implementation uses 21 so that it reproduces them.

To run the tests locally:

```sh
curl -L -o g7231.zip 'https://www.itu.int/rec/dologin_pub.asp?lang=e&id=T-REC-G.723.1-200605-I!!SOFT-ZST-E&type=items'
unzip -q g7231.zip 'Software/*' -d itu
G7231_VECTORS=$PWD/itu/Software cargo test --release -- --ignored
```

## Clean room

The implementation was written from the Recommendation and from functional specifications
in [`docs/spec`](docs/spec), without reading the ITU-T reference software.
[`docs/provenance.md`](docs/provenance.md) describes the process.

## License

MIT. See [LICENSE](LICENSE).
