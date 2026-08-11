#[cfg(test)]
mod termination_contract {
    use super::reprocut_core::{ContainmentMechanism, ExecutionObservation, TerminationReason};

    #[test]
    fn legacy_constructor_produces_a_portable_reason() {
        let timeout = ExecutionObservation::new(None, None, Vec::new(), Vec::new(), true, false);
        assert_eq!(timeout.termination(), TerminationReason::TimedOut);
        assert_eq!(timeout.containment(), ContainmentMechanism::DirectChild);
    }

    #[test]
    fn contained_observation_records_the_active_mechanism() {
        let observation = ExecutionObservation::new_contained(
            TerminationReason::ExitCode(7),
            Vec::new(),
            b"failure".to_vec(),
            false,
            ContainmentMechanism::PosixProcessGroup,
        );
        assert_eq!(observation.exit_code(), Some(7));
        assert_eq!(
            observation.containment(),
            ContainmentMechanism::PosixProcessGroup
        );
    }
}
