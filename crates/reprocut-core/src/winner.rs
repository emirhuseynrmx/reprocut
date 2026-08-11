#[cfg(loom)]
use loom::sync::atomic::{AtomicUsize, Ordering};
#[cfg(not(loom))]
use std::sync::atomic::{AtomicUsize, Ordering};

const EMPTY: usize = usize::MAX;

/// Lock-free ownership of the numerically lowest completed candidate.
///
/// Candidate identifiers must be lower than `usize::MAX`, which is reserved as
/// the empty sentinel. A successful claim publishes preceding worker writes;
/// an acquiring reader that observes it can safely consume those writes.
#[derive(Debug)]
pub struct LowestWinner {
    value: AtomicUsize,
}

impl LowestWinner {
    /// Creates an empty winner slot.
    #[must_use]
    pub fn new() -> Self {
        Self {
            value: AtomicUsize::new(EMPTY),
        }
    }

    /// Installs `candidate` only when it is lower than the current winner.
    ///
    /// Returns `true` when this call changed the published value.
    pub fn claim(&self, candidate: usize) -> bool {
        if candidate == EMPTY {
            return false;
        }

        let mut current = self.value.load(Ordering::Acquire);
        while candidate < current {
            match self.value.compare_exchange_weak(
                current,
                candidate,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(observed) => current = observed,
            }
        }
        false
    }

    /// Loads the current winner, or `None` before the first valid claim.
    #[must_use]
    pub fn load(&self) -> Option<usize> {
        match self.value.load(Ordering::Acquire) {
            EMPTY => None,
            winner => Some(winner),
        }
    }
}

impl Default for LowestWinner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::LowestWinner;

    #[test]
    fn only_strictly_lower_valid_candidates_replace_the_winner() {
        let winner = LowestWinner::new();
        assert_eq!(winner.load(), None);
        assert!(!winner.claim(usize::MAX));
        assert!(winner.claim(9));
        assert!(!winner.claim(12));
        assert!(winner.claim(3));
        assert!(!winner.claim(3));
        assert_eq!(winner.load(), Some(3));
    }
}
