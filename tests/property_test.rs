use frg::domain::money::MoneyAmount;
use frg::domain::payment::PaymentStatus;
use proptest::prelude::*;

fn arb_status() -> impl Strategy<Value = PaymentStatus> {
    prop_oneof![
        Just(PaymentStatus::Pending),
        Just(PaymentStatus::Succeeded),
        Just(PaymentStatus::Failed),
        Just(PaymentStatus::Refunded),
    ]
}

proptest! {
    /// Terminal states (Succeeded, Failed, Refunded) can never transition to anything.
    #[test]
    fn terminal_states_reject_all_transitions(target in arb_status()) {
        use PaymentStatus::*;
        for terminal in [Succeeded, Failed, Refunded] {
            prop_assert!(!terminal.can_transition_to(&target));
        }
    }

    /// Any random sequence of transitions starting from Pending
    /// has at most 1 valid step — all reachable targets are terminal.
    #[test]
    fn random_walk_has_at_most_one_transition(
        steps in prop::collection::vec(arb_status(), 1..20)
    ) {
        let mut current = PaymentStatus::Pending;
        let mut transitions = 0u32;
        for next in &steps {
            if current.can_transition_to(next) {
                current = next.clone();
                transitions += 1;
            }
        }
        prop_assert!(transitions <= 1, "got {transitions} transitions in walk: {steps:?}");
    }

    /// as_str → try_from roundtrip is identity for any status.
    #[test]
    fn status_roundtrip(status in arb_status()) {
        let roundtripped = PaymentStatus::try_from(status.as_str()).unwrap();
        prop_assert_eq!(roundtripped, status);
    }

    /// MoneyAmount survives roundtrip through cents().
    #[test]
    fn money_amount_roundtrip(cents in 0u64..=i64::MAX as u64) {
        let amount = MoneyAmount::new(cents);
        prop_assert_eq!(amount.cents(), cents);
    }

    /// MoneyAmount::checked_add matches u64::checked_add — never silently overflows.
    #[test]
    fn money_add_never_silently_overflows(a in 0u64..=u64::MAX, b in 0u64..=u64::MAX) {
        let result = MoneyAmount::new(a).checked_add(MoneyAmount::new(b));
        match a.checked_add(b) {
            Some(expected) => prop_assert_eq!(result.unwrap().cents(), expected),
            None => prop_assert!(result.is_none()),
        }
    }
}
