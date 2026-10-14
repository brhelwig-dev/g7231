//! The punctured convolutional code (C.2.5.2, C.2.5.3) and a Viterbi decoder for it.

use super::allocation::Allocation;
use super::{CONFIG_BITS, TAIL_BITS};
use crate::tables::SOFT_DECISION_CODEWORDS;

const STATES: usize = 16;

/// Exchanges ten bits near the end of the protected part with ten earlier ones, so that
/// sensitive bits profit from the known end state of the trellis. The exchange is its own
/// inverse.
pub(super) fn exchange_trellis_end<T>(bs: &mut [T], allocation: &Allocation, crc_window: usize) {
    let protected = allocation.sizes[..=allocation.last()].iter().sum::<usize>();
    if protected < crc_window + 25 + TAIL_BITS {
        return;
    }
    let end = protected - TAIL_BITS - 1;
    for n in 0..10 {
        bs.swap(end - n, end - 19 + n);
    }
}

/// The class of each coded input position, tail bits included.
fn classes(allocation: &Allocation) -> Vec<usize> {
    let last = allocation.last();
    (0..=last)
        .flat_map(|k| std::iter::repeat_n(k, allocation.sizes[k]))
        .collect()
}

/// The outputs of the three generators for the register `x(n), x(n-1), ..., x(n-4)`.
fn outputs(x: [u8; 5]) -> [u8; 3] {
    [
        x[0] ^ x[1] ^ x[4],
        x[0] ^ x[2] ^ x[3] ^ x[4],
        x[0] ^ x[1] ^ x[2] ^ x[4],
    ]
}

/// Codes the bits `bs`: the protected part with the punctured code, then the unprotected
/// bits unchanged.
pub(super) fn encode(bs: &[u8], allocation: &Allocation) -> Vec<u8> {
    if allocation.rates[0] == 12 {
        return bs.to_vec();
    }
    let classes = classes(allocation);
    let coded = classes.len() - TAIL_BITS;
    let mut out = Vec::new();
    let mut register = [0u8; 5];
    for (n, &k) in classes.iter().enumerate() {
        register.copy_within(0..4, 1);
        register[0] = if n < coded { bs[n] } else { 0 };
        let patterns = allocation.patterns(k);
        let phase = n % 12;
        for (j, u) in outputs(register).into_iter().enumerate() {
            if (patterns[j] >> phase) & 1 == 1 {
                out.push(u);
            }
        }
    }
    out.extend_from_slice(&bs[coded..]);
    out
}

/// Decodes `count` bits from the soft values that follow the configuration bits.
pub(super) fn decode(soft: &[i8], allocation: &Allocation, count: usize) -> Vec<u8> {
    let hard = |s: i8| u8::from(s < 0);
    let soft = &soft[CONFIG_BITS..];
    if allocation.rates[0] == 12 {
        return soft[..count].iter().map(|&s| hard(s)).collect();
    }
    let classes = classes(allocation);
    let coded = classes.len() - TAIL_BITS;

    let mut metrics = [-5000i16; STATES];
    metrics[0] = 0;
    let mut choices = Vec::with_capacity(classes.len());
    let mut next = 0;
    for (n, &k) in classes.iter().enumerate() {
        let patterns = allocation.patterns(k);
        let z: [i32; 3] = std::array::from_fn(|j| {
            if (patterns[j] >> (n % 12)) & 1 == 1 {
                next += 1;
                2 * i32::from(soft[next - 1])
            } else {
                0
            }
        });
        let mut updated = [0i16; STATES];
        let mut choice = [false; STATES];
        for state in 0..STATES {
            let b = state & 1;
            let candidate = |oldest: usize| {
                let previous = (state >> 1) | (oldest << 3);
                let j = 2 * (previous % 8) + ((b + previous / 8) % 2);
                let branch: i32 = (0..3)
                    .map(|g| 2 * i32::from(SOFT_DECISION_CODEWORDS[j][g]) * z[g])
                    .sum();
                metrics[previous].saturating_add((branch >> 16) as i16)
            };
            let (m0, m1) = (candidate(0), candidate(1));
            choice[state] = m1 > m0;
            updated[state] = m0.max(m1);
        }
        metrics = updated;
        choices.push(choice);
    }

    let mut bits = vec![0; classes.len()];
    let mut state = 0;
    for n in (0..classes.len()).rev() {
        bits[n] = (state & 1) as u8;
        if choices[n][state] {
            state += STATES;
        }
        state >>= 1;
    }
    bits.truncate(coded);
    bits.extend(soft[next..next + count - coded].iter().map(|&s| hard(s)));
    bits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn soft(bits: &[u8]) -> Vec<i8> {
        let mut s = vec![127; CONFIG_BITS];
        s.extend(bits.iter().map(|&b| if b == 1 { -127 } else { 127 }));
        s
    }

    fn pseudo_random(len: usize) -> Vec<u8> {
        let mut state = 12345u32;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(1103515245).wrapping_add(12345);
                ((state >> 16) & 1) as u8
            })
            .collect()
    }

    #[test]
    fn decoding_inverts_encoding() {
        for (mode, rate) in [(0, 8000), (0, 12000), (0, 20634), (1, 10000), (2, 2500)] {
            let a = Allocation::new(mode, rate).unwrap();
            let len = a.sizes.iter().sum::<usize>() - TAIL_BITS;
            let bs = pseudo_random(len);
            let coded = encode(&bs, &a);
            assert_eq!(coded.len(), a.count(a.last()) - CONFIG_BITS);
            assert_eq!(
                decode(&soft(&coded), &a, len),
                bs,
                "mode {mode} rate {rate}"
            );
        }
    }

    #[test]
    fn corrects_isolated_errors() {
        let a = Allocation::new(0, 20634).unwrap();
        let len = a.sizes.iter().sum::<usize>() - TAIL_BITS;
        let bs = pseudo_random(len);
        let mut received = soft(&encode(&bs, &a));
        for i in (CONFIG_BITS + 5..received.len()).step_by(40) {
            received[i] = -received[i];
        }
        assert_eq!(decode(&received, &a, len), bs);
    }

    #[test]
    fn unprotected_frames_pass_through() {
        let a = Allocation::new(1, 6000).unwrap();
        let bs = pseudo_random(167);
        assert_eq!(encode(&bs, &a), bs);
        assert_eq!(decode(&soft(&bs), &a, 167), bs);
    }

    #[test]
    fn trellis_exchange_is_an_involution() {
        let a = Allocation::new(0, 13834).unwrap();
        let original: Vec<usize> = (0..198).collect();
        let mut bs = original.clone();
        exchange_trellis_end(&mut bs, &a, 44);
        assert_ne!(bs, original);
        exchange_trellis_end(&mut bs, &a, 44);
        assert_eq!(bs, original);
    }
}
