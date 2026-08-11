#[cfg(test)]
mod winner_remote_contract {
    use std::{sync::Arc, thread};

    use super::reprocut_core::LowestWinner;

    #[test]
    fn concurrent_claimers_publish_the_lowest_identifier() {
        let winner = Arc::new(LowestWinner::new());
        let workers = [91, 17, 42, 3, 88, 11, 5, 29]
            .into_iter()
            .map(|candidate| {
                let worker_winner = Arc::clone(&winner);
                thread::spawn(move || worker_winner.claim(candidate))
            })
            .collect::<Vec<_>>();

        for worker in workers {
            worker.join().expect("worker completes without panic");
        }
        assert_eq!(winner.load(), Some(3));
    }
}
