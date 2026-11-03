//! Moving a finished install into place. On Windows, security software and the
//! search indexer hold files that were just written, and a rename fails until
//! they let go, so it is retried for as long as npm's file library would.

use std::path::Path;
use std::time::{Duration, Instant};

const RETRY_FOR: Duration = Duration::from_secs(30);
const RETRY_EVERY: Duration = Duration::from_millis(250);

// Windows error codes: access denied, sharing violation, lock violation.
fn held_by_another_program(error: &std::io::Error, windows: bool) -> bool {
    windows && matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

async fn retry_while_held(
    mut rename: impl FnMut() -> std::io::Result<()>,
    windows: bool,
    retry_for: Duration,
    retry_every: Duration,
) -> std::io::Result<()> {
    let started = Instant::now();
    loop {
        match rename() {
            Err(error)
                if held_by_another_program(&error, windows) && started.elapsed() < retry_for =>
            {
                tokio::time::sleep(retry_every).await;
            }
            result => return result,
        }
    }
}

pub(crate) async fn rename_when_free(from: &Path, to: &Path) -> std::io::Result<()> {
    let rename = || std::fs::rename(from, to);
    retry_while_held(rename, cfg!(windows), RETRY_FOR, RETRY_EVERY).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANT: Duration = Duration::ZERO;
    const MOMENT: Duration = Duration::from_secs(5);

    fn failing(code: i32, failures: usize) -> impl FnMut() -> std::io::Result<()> {
        let mut left = failures;
        move || {
            if left == 0 {
                return Ok(());
            }
            left -= 1;
            Err(std::io::Error::from_raw_os_error(code))
        }
    }

    #[tokio::test]
    async fn a_held_folder_is_renamed_once_it_is_released() {
        for code in [5, 32, 33] {
            let result = retry_while_held(failing(code, 3), true, MOMENT, INSTANT).await;
            assert!(result.is_ok(), "{code}");
        }
    }

    #[tokio::test]
    async fn a_folder_held_past_the_limit_reports_the_error() {
        let result = retry_while_held(failing(5, usize::MAX), true, INSTANT, INSTANT).await;
        assert_eq!(result.unwrap_err().raw_os_error(), Some(5));
    }

    #[tokio::test]
    async fn other_failures_are_not_retried() {
        let mut attempts = 0;
        let rename = || {
            attempts += 1;
            Err(std::io::Error::from_raw_os_error(3))
        };
        assert!(retry_while_held(rename, true, MOMENT, INSTANT)
            .await
            .is_err());
        assert_eq!(attempts, 1);
    }

    #[tokio::test]
    async fn these_codes_mean_nothing_elsewhere() {
        assert!(retry_while_held(failing(5, 1), false, MOMENT, INSTANT)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn a_free_folder_moves_on_the_first_try() {
        let root = tempfile::tempdir().unwrap();
        let (from, to) = (root.path().join("stage"), root.path().join("target"));
        std::fs::create_dir(&from).unwrap();
        rename_when_free(&from, &to).await.unwrap();
        assert!(to.is_dir() && !from.exists());
    }
}
