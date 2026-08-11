#![cfg(loom)]

use loom::sync::Arc;
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
