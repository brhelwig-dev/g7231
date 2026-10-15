//! The CRC of the most sensitive bits (C.2.4).

use super::CRC_BITS;

/// The five parity bits p(0) to p(4) of `bits`.
pub(super) fn parity(bits: &[u8]) -> [u8; CRC_BITS] {
    let mut register = 0u8;
    for &b in bits {
        let carry = (b ^ register) & 1;
        register >>= 1;
        if carry == 1 {
            register ^= 18;
        }
    }
    std::array::from_fn(|j| (register >> (CRC_BITS - 1 - j)) & 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_reference_example() {
        let bits: Vec<u8> = "01010100011111111010000011011001011000010101"
            .bytes()
            .map(|c| c - b'0')
            .collect();
        assert_eq!(parity(&bits), [1, 0, 0, 1, 0]);
    }

    #[test]
    fn detects_single_bit_errors() {
        let bits = vec![0; 44];
        for i in 0..44 {
            let mut damaged = bits.clone();
            damaged[i] = 1;
            assert_ne!(parity(&damaged), parity(&bits));
        }
    }
}
