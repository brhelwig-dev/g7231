//! DC removal filter H(z) = (1 - z^-1) / (1 - 127/128 z^-1) (clause 2.3).

use crate::FRAME_LEN;

#[derive(Clone, Debug, Default)]
pub(crate) struct HighPass {
    input: f64,
    output: f64,
}

impl HighPass {
    pub(crate) fn process(&mut self, s: &[i16; FRAME_LEN]) -> [f64; FRAME_LEN] {
        s.map(|sample| {
            let x = f64::from(sample);
            let d = x - self.input;
            self.input = x;
            self.output = d + self.output * 0.9921875;
            self.output
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_a_constant_offset() {
        let mut hp = HighPass::default();
        let mut out = [0.0; FRAME_LEN];
        for _ in 0..20 {
            out = hp.process(&[1000; FRAME_LEN]);
        }
        assert!(out.iter().all(|y| y.abs() < 1e-3));
    }

    #[test]
    fn passes_a_step_at_first() {
        let out = HighPass::default().process(&[1000; FRAME_LEN]);
        assert_eq!(out[0], 1000.0);
        assert_eq!(out[1], 992.1875);
    }
}
