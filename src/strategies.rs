use rand::{rngs::StdRng, Rng, SeedableRng};
use std::time::Duration;

/// Defines the exponential backoff strategy for retry operations.
pub struct ExpBackoffStrategy {
    min: Duration,
    max: Option<Duration>,
    factor: f64,
    jitter: f64,
    seed: Option<u64>,
}

impl ExpBackoffStrategy {
    /// Creates a new `ExpBackoffStrategy` with specified minimum duration, growth factor, and jitter.
    pub fn new(min: Duration, factor: f64, jitter: f64) -> Self {
        Self {
            min,
            max: None,
            factor,
            jitter,
            seed: None,
        }
    }

    /// Sets the maximum wait duration for the strategy.
    pub fn with_max(mut self, max: Duration) -> Self {
        self.max = Some(max);
        self
    }

    /// Sets the seed for random jitter generation.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }
}

impl Default for ExpBackoffStrategy {
    fn default() -> Self {
        Self {
            min: Duration::from_secs(4),
            max: Some(Duration::from_secs(60)),
            factor: 2.0,
            jitter: 0.05,
            seed: None,
        }
    }
}

impl IntoIterator for ExpBackoffStrategy {
    type Item = Duration;
    type IntoIter = ExpBackoffIter;

    fn into_iter(self) -> Self::IntoIter {
        let init = self.min.as_secs_f64();
        let rng = match self.seed {
            Some(seed) => StdRng::seed_from_u64(seed),
            None => StdRng::from_entropy(),
        };

        ExpBackoffIter {
            strategy: self,
            init,
            pow: 0,
            rng,
        }
    }
}

/// Iterator for generating exponential backoff durations.
pub struct ExpBackoffIter {
    strategy: ExpBackoffStrategy,
    init: f64,
    pow: u32,
    rng: StdRng,
}

pub trait ResetIterExt {
    fn reset(&mut self);
}

impl ResetIterExt for ExpBackoffIter {
    /// Resets the iterator to its initial state.
    fn reset(&mut self) {
        self.pow = 0;
    }
}

impl Iterator for ExpBackoffIter {
    type Item = Duration;

    fn next(&mut self) -> Option<Self::Item> {
        let base = self.init * self.strategy.factor.powf(self.pow as f64);
        let jitter = base * self.strategy.jitter * (self.rng.gen::<f64>() * 2. - 1.);
        let current = Duration::from_secs_f64(base + jitter);
        self.pow += 1;

        Some(match self.strategy.max {
            Some(max) => max.min(current),
            None => current,
        })
    }
}

pub trait DurationIteratorExt: Iterator<Item = Duration> + ResetIterExt + Send + Sync {}
pub type DurationIterator = Box<dyn DurationIteratorExt>;

impl DurationIteratorExt for ExpBackoffIter {}

#[cfg(test)]
mod test {
    use crate::strategies::{ExpBackoffStrategy, ResetIterExt};

    #[tokio::test]
    async fn test_reset() {
        let strategy = ExpBackoffStrategy::default();
        let mut iter = strategy.into_iter();

        // Simulate some retry operations
        for _ in 0..3 {
            println!("{:?}", iter.next());
        }

        // Reset the iterator
        iter.reset();

        // Retry operations again from the start
        for _ in 0..3 {
            println!("{:?}", iter.next());
        }
    }
}
