//! Random numbers: a small seeded generator (mulberry32, the same one the
//! 1.2.x game used, so the same seed makes the same race tracks), and quick
//! helpers for the everyday kind.

/// A seeded random number generator. Same seed, same sequence.
#[derive(Clone, Debug)]
pub struct Seeded(u32);

impl Seeded {
    pub fn new(seed: f64) -> Self {
        // JavaScript's `seed >>> 0`: the number wrapped into 32 bits.
        Seeded(js_u32(seed))
    }

    /// The next number, 0 up to (not including) 1.
    pub fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }

    pub fn between(&mut self, min: f64, max: f64) -> f64 {
        min + (max - min) * self.next()
    }
}

/// JavaScript's `x >>> 0` for any number.
fn js_u32(x: f64) -> u32 {
    if !x.is_finite() {
        return 0;
    }
    let t = x.trunc();
    t.rem_euclid(4_294_967_296.0) as u32
}

/// A random number 0..1 from the thread's generator.
pub fn random() -> f64 {
    rand::random::<f64>()
}

/// A random number between `min` and `max`.
pub fn between(min: f64, max: f64) -> f64 {
    min + (max - min) * random()
}

/// -0.5..0.5
pub fn centered() -> f64 {
    random() - 0.5
}

/// A random item from a list.
pub fn pick<T: Clone>(list: &[T]) -> T {
    list[((random() * list.len() as f64) as usize).min(list.len() - 1)].clone()
}

/// A random whole number 0..n.
pub fn index(n: usize) -> usize {
    ((random() * n as f64) as usize).min(n.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_old_game() {
        // seededRandom(1) in the 1.2.x game starts 0.6270739405881613, 0.002735721180215478.
        let mut r = Seeded::new(1.0);
        assert!((r.next() - 0.627_073_940_588_161_3).abs() < 1e-15);
        assert!((r.next() - 0.002_735_721_180_215_478).abs() < 1e-15);
    }
}
