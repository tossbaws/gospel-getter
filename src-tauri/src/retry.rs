//! Trying background work again, a few times, with a pause between tries:
//! for work that can lose a race (a lock) it would win a moment later, and
//! that nothing else will redo until the app restarts.

use std::future::Future;
use std::time::Duration;

/// Runs `attempt` until it succeeds, pausing `delays[i]` after its `i`-th
/// failure; when the pauses run out, the last try's error is the answer.
/// Each failure that's tried again is logged as a warning, naming `what`.
pub async fn with_retries<T, F, Fut>(
    what: &str,
    delays: &[Duration],
    mut attempt: F,
) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let tries = delays.len() + 1;
    for (failed, delay) in delays.iter().enumerate() {
        match attempt().await {
            Ok(value) => return Ok(value),
            Err(e) => {
                tracing::warn!(
                    "{what} failed (try {} of {tries}), trying again in {delay:?}: {e:#}",
                    failed + 1
                );
                tokio::time::sleep(*delay).await;
            }
        }
    }
    attempt().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;
    use std::cell::Cell;

    const NO_PAUSES: [Duration; 3] = [Duration::ZERO; 3];

    #[tokio::test]
    async fn a_success_is_not_tried_again() {
        let tries = Cell::new(0);
        let answer = with_retries("work", &NO_PAUSES, || {
            tries.set(tries.get() + 1);
            async { Ok::<_, anyhow::Error>(7) }
        })
        .await;
        assert_eq!(answer.unwrap(), 7);
        assert_eq!(tries.get(), 1);
    }

    #[tokio::test]
    async fn failures_are_tried_again_until_one_succeeds() {
        let tries = Cell::new(0);
        let answer = with_retries("work", &NO_PAUSES, || {
            tries.set(tries.get() + 1);
            let n = tries.get();
            async move {
                if n < 3 {
                    bail!("database is locked");
                }
                Ok(n)
            }
        })
        .await;
        assert_eq!(answer.unwrap(), 3);
        assert_eq!(tries.get(), 3);
    }

    #[tokio::test]
    async fn after_the_last_pause_the_last_error_is_the_answer() {
        let tries = Cell::new(0);
        let answer: anyhow::Result<()> = with_retries("work", &NO_PAUSES, || {
            tries.set(tries.get() + 1);
            let n = tries.get();
            async move { bail!("failure {n}") }
        })
        .await;
        assert_eq!(answer.unwrap_err().to_string(), "failure 4");
        assert_eq!(tries.get(), 4);
    }

    #[tokio::test]
    async fn it_pauses_between_tries() {
        let started = std::time::Instant::now();
        let _: anyhow::Result<()> = with_retries(
            "work",
            &[Duration::from_millis(30), Duration::from_millis(60)],
            || async { bail!("no") },
        )
        .await;
        assert!(started.elapsed() >= Duration::from_millis(90));
    }
}
