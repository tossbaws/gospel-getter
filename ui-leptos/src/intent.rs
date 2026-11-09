//! Whose an answer is, once it's back: a small rule for the work the page
//! starts for something the reader did (a click, a key, a change) and
//! finishes after an `await`, by which time the reader may have done
//! something newer that the late answer mustn't undo.
//!
//! Each such operation keeps one [`Generation`], in whatever already holds
//! its state (a module's thread-local `State`, a component's
//! `StoredValue`). The reader's intent [`claim`](Generation::claim)s it and
//! keeps the [`Claim`]; anything newer that takes over moves the generation
//! on, either by claiming it in turn (a newer search, a newer save) or with
//! [`cancel`](Generation::cancel) when nothing newer is asked for (Escape,
//! the panel closing, a tab switched). When the answer comes back it acts
//! only if the generation still [`holds`](Generation::holds) its claim; a
//! superseded answer is dropped without a word.
//!
//! A counter, not a lock: nothing waits, and nothing is cancelled on the
//! backend. Free of the DOM, so the rule is tested on the host. The
//! reader's own `load_token` predates this and works the same way; the
//! answers that outlive it (an Edit waiting for its jump, Compare's own
//! fetch) sit beside it with their own generations.

/// One operation's intents: which of them, if any, an answer still belongs
/// to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Generation(u64);

/// What an intent holds while its answer is on the way: the generation it
/// claimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use = "an answer is checked against its claim with `Generation::holds`"]
pub struct Claim(u64);

impl Generation {
    /// A new intent, which supersedes every earlier one: its answer is the
    /// only one that still belongs.
    pub fn claim(&mut self) -> Claim {
        self.0 = self.0.wrapping_add(1);
        Claim(self.0)
    }

    /// Something newer happened that no answer belongs to (Escape, a close,
    /// a navigation): every claim so far is superseded.
    pub fn cancel(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }

    /// Whether `claim` is still the newest: nothing has claimed or
    /// cancelled since.
    pub fn holds(&self, claim: Claim) -> bool {
        self.0 == claim.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_newest_claim_holds() {
        let mut generation = Generation::default();
        let first = generation.claim();
        assert!(generation.holds(first));
        let second = generation.claim();
        assert!(!generation.holds(first), "a newer intent supersedes it");
        assert!(generation.holds(second));
    }

    #[test]
    fn a_cancel_supersedes_every_claim_without_making_one() {
        let mut generation = Generation::default();
        let claim = generation.claim();
        generation.cancel();
        assert!(!generation.holds(claim));
        // Claimed again after, the new one holds.
        let again = generation.claim();
        assert!(generation.holds(again));
        assert!(!generation.holds(claim));
    }

    #[test]
    fn each_operation_keeps_its_own_generation() {
        let (mut edit, mut compare) = (Generation::default(), Generation::default());
        let editing = edit.claim();
        let comparing = compare.claim();
        compare.cancel();
        assert!(
            edit.holds(editing),
            "another operation's cancel isn't this one's"
        );
        assert!(!compare.holds(comparing));
    }

    #[test]
    fn it_wraps_rather_than_overflowing() {
        let mut generation = Generation(u64::MAX);
        let claim = generation.claim();
        assert!(generation.holds(claim));
        assert!(!generation.holds(Claim(u64::MAX)));
    }
}
