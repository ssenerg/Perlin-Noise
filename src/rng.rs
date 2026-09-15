//! Minimal deterministic pseudo random number generator.
//!
//! `SplitMix64` is used instead of an external crate so that the same seed
//! always produces the same lattice, independently of the platform and of any
//! dependency version.

pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform value in `[0, 1)`.
    pub fn next_f64(&mut self) -> f64 {
        // 53 significant bits, the most a f64 can hold without rounding.
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Standard normal value, via the Box-Muller transform.
    pub fn next_normal(&mut self) -> f64 {
        // `1.0 - next_f64()` lands in (0, 1], keeping the logarithm finite.
        let u1 = 1.0 - self.next_f64();
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }

    /// Vector of length `n` drawn uniformly from the unit hypersphere.
    ///
    /// Normalising a vector of independent normals is the standard way to get a
    /// direction with no bias towards the corners of the hypercube.
    pub fn next_unit_vector(&mut self, n: usize) -> Vec<f64> {
        loop {
            let v: Vec<f64> = (0..n).map(|_| self.next_normal()).collect();
            let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
            if norm.is_finite() && norm > 1e-12 {
                return v.into_iter().map(|x| x / norm).collect();
            }
        }
    }
}
