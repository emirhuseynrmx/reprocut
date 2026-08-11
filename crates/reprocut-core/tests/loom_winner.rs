#![cfg(loom)]

use loom::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use reprocut_core::LowestWinner;

#[test]
fn lower_candidate_wins_every_explored_interleaving() {
    loom::model(|| {
        let winner = Arc::new(LowestWinner::new());
        let high = Arc::clone(&winner);
        let low = Arc::clone(&winner);

        let high_worker = loom::thread::spawn(move || high.claim(9));
        let low_worker = loom::thread::spawn(move || low.claim(3));

        high_worker.join().expect("high worker completes");
        low_worker.join().expect("low worker completes");
        assert_eq!(winner.load(), Some(3));
    });
}

#[test]
fn winner_claim_publishes_preceding_terminal_evidence() {
    loom::model(|| {
        let winner = Arc::new(LowestWinner::new());
        let terminal = Arc::new(AtomicBool::new(false));
        let worker_winner = Arc::clone(&winner);
        let worker_terminal = Arc::clone(&terminal);

        let worker = loom::thread::spawn(move || {
            worker_terminal.store(true, Ordering::Relaxed);
            worker_winner.claim(4);
        });
        worker.join().expect("worker completes");

        assert_eq!(winner.load(), Some(4));
        assert!(terminal.load(Ordering::Relaxed));
    });
}

#[test]
fn cancellation_never_promotes_a_higher_completed_candidate() {
    loom::model(|| {
        let winner = Arc::new(LowestWinner::new());
        let cancelled = Arc::new(AtomicBool::new(false));
        let high_winner = Arc::clone(&winner);
        let high_cancelled = Arc::clone(&cancelled);
        let low_winner = Arc::clone(&winner);
        let low_cancelled = Arc::clone(&cancelled);

        let high = loom::thread::spawn(move || {
            high_winner.claim(8);
            high_cancelled.store(true, Ordering::Release);
        });
        let low = loom::thread::spawn(move || {
            if low_cancelled.load(Ordering::Acquire) {
                low_winner.claim(2);
            } else {
                low_winner.claim(2);
                low_cancelled.store(true, Ordering::Release);
            }
        });

        high.join().expect("high worker completes");
        low.join().expect("low worker completes");
        assert!(cancelled.load(Ordering::Acquire));
        assert_eq!(winner.load(), Some(2));
    });
}
