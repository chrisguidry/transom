//! How often each token may be woken: a token bucket per token, in memory.
//! Anyone may ask the gateway to wake any token, so the limit is what
//! keeps a leaked token from draining a phone's battery.
//!
//! A bucket holds `BURST` wake-ups and gains one every `EVERY`. A wake-up
//! syncs everything that changed, so a server that asks more often than
//! this loses nothing the next wake-up does not bring.

use std::collections::HashMap;
use std::fmt::Write;
use std::time::{Duration, Instant};

use aws_lc_rs::digest::{SHA256, digest};

/// The wake-ups a token may have at once.
const BURST: u32 = 5;

/// How long a token waits for each wake-up after its burst: four a minute.
const EVERY: Duration = Duration::from_secs(15);

/// How many buckets the limiter keeps before dropping the full ones. A
/// full bucket is the same as none, so dropping it changes no answer.
const PRUNE_ABOVE: usize = 10_000;

/// A short, stable name for `token`, for logs and for the limiter's keys,
/// from which the token cannot be recovered.
pub(crate) fn fingerprint(token: &str) -> String {
    let mut name = String::new();
    for byte in &digest(&SHA256, token.as_bytes()).as_ref()[..6] {
        let _ = write!(name, "{byte:02x}");
    }
    name
}

pub(crate) struct Limiter {
    /// When each token's bucket is next full, which says how many wake-ups
    /// it holds without a timer to refill it.
    full_at: HashMap<String, Instant>,
    prune_above: usize,
}

impl Limiter {
    pub(crate) fn new() -> Self {
        Self::pruning_above(PRUNE_ABOVE)
    }

    fn pruning_above(prune_above: usize) -> Self {
        Self {
            full_at: HashMap::new(),
            prune_above,
        }
    }

    /// Takes one wake-up from the bucket of the token named `fingerprint`
    /// at `now`, or says how long until the bucket has one.
    pub(crate) fn take(&mut self, fingerprint: &str, now: Instant) -> Result<(), Duration> {
        let window = EVERY * BURST;
        let full_at = self
            .full_at
            .get(fingerprint)
            .copied()
            .unwrap_or(now)
            .max(now);
        // Taking one more pushes the time the bucket is full one step
        // later. A bucket that would be full more than a burst from now
        // has nothing left.
        let after = full_at + EVERY;
        if after > now + window {
            return Err(after - (now + window));
        }
        self.full_at.insert(fingerprint.to_owned(), after);
        if self.full_at.len() > self.prune_above {
            self.full_at.retain(|_, full_at| *full_at > now);
        }
        Ok(())
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.full_at.len()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{BURST, EVERY, Limiter, fingerprint};

    /// How many of `count` wake-ups at `now` the limiter lets through.
    fn taken(limiter: &mut Limiter, token: &str, count: u32, now: Instant) -> u32 {
        (0..count)
            .map(|_| u32::from(limiter.take(token, now).is_ok()))
            .sum()
    }

    #[test]
    fn a_token_has_a_burst() {
        let mut limiter = Limiter::new();
        assert_eq!(taken(&mut limiter, "a", 10, Instant::now()), BURST);
    }

    #[test]
    fn a_refused_wake_up_says_how_long_to_wait() {
        let mut limiter = Limiter::new();
        let now = Instant::now();
        taken(&mut limiter, "a", BURST, now);
        assert_eq!(limiter.take("a", now), Err(EVERY));
        assert_eq!(
            limiter.take("a", now + Duration::from_secs(5)),
            Err(Duration::from_secs(10))
        );
    }

    #[test]
    fn a_token_gains_one_wake_up_each_step() {
        let mut limiter = Limiter::new();
        let now = Instant::now();
        taken(&mut limiter, "a", BURST, now);
        assert_eq!(taken(&mut limiter, "a", 3, now + EVERY), 1);
    }

    #[test]
    fn a_token_left_alone_has_its_burst_again() {
        let mut limiter = Limiter::new();
        let now = Instant::now();
        taken(&mut limiter, "a", BURST, now);
        assert_eq!(taken(&mut limiter, "a", 10, now + EVERY * BURST), BURST);
    }

    #[test]
    fn each_token_has_a_bucket_of_its_own() {
        let mut limiter = Limiter::new();
        let now = Instant::now();
        taken(&mut limiter, "a", BURST, now);
        assert_eq!(taken(&mut limiter, "b", 1, now), 1);
    }

    #[test]
    fn full_buckets_are_dropped_past_the_limit() {
        let mut limiter = Limiter::pruning_above(2);
        let now = Instant::now();
        limiter.take("a", now).unwrap();
        limiter.take("b", now).unwrap();
        limiter.take("c", now + EVERY).unwrap();
        assert_eq!(limiter.len(), 1);
    }

    #[test]
    fn a_fingerprint_is_short_and_stable() {
        assert_eq!(fingerprint("a-token"), fingerprint("a-token"));
        assert_eq!(fingerprint("a-token").len(), 12);
        assert_ne!(fingerprint("a-token"), fingerprint("another"));
    }
}
