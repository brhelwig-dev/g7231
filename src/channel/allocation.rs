//! Distribution of the channel bits over the five protection classes (C.2.5.1).

use super::{CONFIG_BITS, CRC_BITS, MODES, TAIL_BITS};
use crate::tables::{
    CLASS_RECIPROCALS_HIGH_RATE, CLASS_RECIPROCALS_LOW_RATE, CLASS_SIZES_HIGH_RATE,
    CLASS_SIZES_LOW_RATE, CLASS_SIZES_SID, CLASS_WEIGHTS_HIGH_RATE, CLASS_WEIGHTS_LOW_RATE,
    CONVOLUTIONAL_PUNCTURING,
};

const CLASSES: usize = 5;
/// Code rate denominator of an unprotected class: rate 12/12.
const UNPROTECTED: usize = 12;
/// Reciprocals 2^20 / (size + 4) of class 0 when it takes all the redundancy.
const CLASS0_RECIPROCALS: [i64; 3] = [19785, 24386, 26887];
const SID: usize = 2;

/// Class sizes and code rates 12 / rates[i] of one configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Allocation {
    pub sizes: [usize; CLASSES],
    pub rates: [usize; CLASSES],
}

/// Channel bits per frame at the channel bit rate `rate`.
pub(super) fn channel_bits(rate: u16) -> usize {
    ((2 * i64::from(rate) * 31458) >> 21) as usize
}

fn initial_sizes(mode: usize) -> [usize; CLASSES] {
    let sizes = match mode {
        0 => CLASS_SIZES_HIGH_RATE,
        1 => CLASS_SIZES_LOW_RATE,
        _ => CLASS_SIZES_SID,
    };
    sizes.map(usize::from)
}

/// 12 (1 + E weight / size), from the stored negative weight, rounded to the nearest
/// integer when `round` is 1 and down when it is 0.
fn rate_of_class(e: usize, weight: i16, size: usize, reciprocal: i64, round: i64) -> usize {
    let s1 = -2 * e as i64 * i64::from(weight) + 65536 * size as i64;
    let t = (12 * s1) >> 12;
    let u = (2 * t * reciprocal) >> 24;
    ((u + round) >> 1) as usize
}

impl Allocation {
    /// The puncturing patterns of the three generators for class `k`.
    pub(super) fn patterns(&self, k: usize) -> [u16; 3] {
        CONVOLUTIONAL_PUNCTURING[self.rates[k] - UNPROTECTED]
    }

    /// The last protected class: the largest of classes 1 to 4 with a code, or 0.
    pub(super) fn last(&self) -> usize {
        (1..CLASSES)
            .rev()
            .find(|&i| self.rates[i] > UNPROTECTED)
            .unwrap_or(0)
    }

    /// Channel bits produced when classes 0 to `last` are coded, including the
    /// configuration bits.
    pub(super) fn count(&self, last: usize) -> usize {
        let mut n = 0;
        let mut phase = 0;
        for k in 0..=last {
            let patterns = self.patterns(k);
            for _ in 0..self.sizes[k] {
                n += patterns.iter().filter(|&&p| (p >> phase) & 1 == 1).count();
                phase = (phase + 1) % 12;
            }
        }
        n + self.sizes[last + 1..].iter().sum::<usize>() + CONFIG_BITS
    }

    /// Allocates the channel bits of a frame of source mode `mode` at channel bit rate
    /// `rate`, or returns `None` when the rate is too low for the mode.
    pub(super) fn new(mode: usize, rate: u16) -> Option<Self> {
        let budget = channel_bits(rate);
        let info_bits = MODES[mode].info_bits;
        let redundancy = budget.checked_sub(info_bits + CRC_BITS + CONFIG_BITS)?;
        let initial = initial_sizes(mode);
        let mut a = Allocation {
            sizes: initial,
            rates: [UNPROTECTED; CLASSES],
        };
        let limit = 2 * initial.iter().sum::<usize>() + TAIL_BITS;

        if redundancy == 0 {
            let tail_class = if mode < SID { 3 } else { 0 };
            a.sizes[tail_class] -= TAIL_BITS;
            a.unprotect_all();
            return Some(a);
        }
        if redundancy >= limit {
            a.rates = [36, 36, 36, 36, UNPROTECTED];
            a.sizes[3] += a.sizes[4];
            a.sizes[4] = 0;
            a.adjust(mode, budget, false);
            return Some(a);
        }

        let e4 = redundancy.saturating_sub(TAIL_BITS);
        if e4 <= initial[0] + 8 || mode == SID {
            let tail_class = if mode < SID { 3 } else { 0 };
            a.sizes[tail_class] -= TAIL_BITS;
            a.rates[0] = rate_of_class(e4, -32768, a.sizes[0] + 4, CLASS0_RECIPROCALS[mode], 0);
            if a.rates[0] == UNPROTECTED {
                a.unprotect_all();
            } else {
                a.sizes[0] += TAIL_BITS;
            }
        } else {
            let (weights, reciprocals) = match mode {
                0 => (CLASS_WEIGHTS_HIGH_RATE, CLASS_RECIPROCALS_HIGH_RATE),
                _ => (CLASS_WEIGHTS_LOW_RATE, CLASS_RECIPROCALS_LOW_RATE),
            };
            let class_rate = |i: usize, size: usize| {
                rate_of_class(e4, weights[i], size, i64::from(reciprocals[i]), 1)
            };
            a.rates[0] = class_rate(0, a.sizes[0]).clamp(24, 36);
            a.sizes[3] -= TAIL_BITS;
            a.sizes[0] += TAIL_BITS;
            for i in 1..4 {
                if a.count(i - 1) >= budget {
                    break;
                }
                a.rates[i] = class_rate(i, a.sizes[i]).clamp(18, 36).min(a.rates[0]);
                a.sizes[i - 1] -= TAIL_BITS;
                a.sizes[i] += TAIL_BITS;
            }
        }
        a.adjust(mode, budget, true);
        Some(a)
    }

    fn unprotect_all(&mut self) {
        let moved: usize = self.sizes[..4].iter().sum();
        self.sizes = [0, 0, 0, 0, self.sizes[4] + moved];
        self.rates = [UNPROTECTED; CLASSES];
    }

    /// Fits the allocation to the budget by moving bits between classes.
    fn adjust(&mut self, mode: usize, budget: usize, may_grow: bool) {
        let last = self.last();
        let used = self.count(last);
        if used > budget {
            if mode < SID {
                self.shrink(budget, last);
            } else {
                self.rates[0] -= 1;
                if self.rates[0] == UNPROTECTED {
                    self.sizes[0] -= TAIL_BITS;
                }
            }
        } else if used < budget && may_grow && self.rates[0] > UNPROTECTED && mode < SID {
            self.grow(budget, last);
        }
    }

    fn move_bit(&mut self, from: usize, to: usize) {
        self.sizes[from] -= 1;
        self.sizes[to] += 1;
    }

    fn shrink(&mut self, budget: usize, last: usize) {
        if self.rates[3] > 21 {
            loop {
                for i in [2, 1] {
                    if self.sizes[i] == 0 {
                        return;
                    }
                    self.move_bit(i, i + 1);
                    if self.count(last) <= budget {
                        return;
                    }
                }
            }
        }
        while self.sizes[last] > 0 {
            self.move_bit(last, last + 1);
            if self.count(last) <= budget {
                return;
            }
        }
    }

    fn grow(&mut self, budget: usize, last: usize) {
        let mut source = last + 1;
        let mut destination = last;
        loop {
            if self.sizes[source] == 0 {
                if source == last || last == 0 {
                    return;
                }
                source = last;
                destination = last - 1;
                if self.rates[source] == self.rates[destination] {
                    return;
                }
            }
            self.move_bit(source, destination);
            if self.count(last) >= budget {
                break;
            }
        }
        if self.count(last) > budget {
            self.shrink(budget, last);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(mode: usize, rate: u16, sizes: [usize; 5], rows: [usize; 4]) {
        let a = Allocation::new(mode, rate).unwrap();
        assert_eq!(a.sizes, sizes, "mode {mode} rate {rate}");
        let actual: [usize; 4] = std::array::from_fn(|i| a.rates[i] - UNPROTECTED);
        assert_eq!(actual, rows, "mode {mode} rate {rate}");
        assert!(a.count(a.last()) <= channel_bits(rate));
    }

    #[test]
    fn high_rate_check_values() {
        check(0, 7034, [0, 0, 0, 0, 198], [0, 0, 0, 0]);
        check(0, 8000, [59, 38, 46, 47, 12], [5, 0, 0, 0]);
        check(0, 10000, [49, 44, 22, 75, 12], [12, 7, 6, 0]);
        check(0, 12000, [49, 44, 46, 31, 32], [12, 11, 9, 8]);
        check(0, 13834, [49, 44, 46, 54, 9], [13, 13, 13, 11]);
        check(0, 18000, [49, 44, 46, 60, 3], [21, 21, 20, 17]);
        check(0, 20634, [49, 44, 46, 63, 0], [24, 24, 24, 24]);
    }

    #[test]
    fn low_rate_check_values() {
        check(1, 6000, [0, 0, 0, 0, 167], [0, 0, 0, 0]);
        check(1, 8000, [39, 35, 49, 40, 8], [12, 6, 0, 0]);
        check(1, 10000, [39, 40, 40, 23, 29], [12, 11, 8, 7]);
        check(1, 11767, [39, 40, 65, 27, 0], [12, 12, 12, 11]);
        check(1, 14000, [39, 40, 51, 41, 0], [17, 17, 17, 15]);
        check(1, 17534, [39, 40, 40, 52, 0], [24, 24, 24, 24]);
    }

    #[test]
    fn sid_check_values() {
        check(2, 1600, [0, 0, 0, 0, 35], [0, 0, 0, 0]);
        check(2, 2000, [39, 0, 0, 0, 0], [2, 0, 0, 0]);
        check(2, 2500, [39, 0, 0, 0, 0], [7, 0, 0, 0]);
        check(2, 2967, [39, 0, 0, 0, 0], [11, 0, 0, 0]);
        check(2, 4334, [39, 0, 0, 0, 0], [24, 24, 24, 24]);
    }

    #[test]
    fn every_rate_fits_its_budget() {
        for (mode, low, high) in [(0, 7034, 20634), (1, 6000, 17534), (2, 1600, 4334)] {
            for rate in low..=high {
                let a = Allocation::new(mode, rate).unwrap();
                assert!(
                    a.count(a.last()) <= channel_bits(rate),
                    "mode {mode} rate {rate}"
                );
            }
        }
        assert_eq!(Allocation::new(0, 7000), None);
    }
}
