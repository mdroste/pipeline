//! Product adapters retain distinct runtimes behind one durable action envelope.
use crate::orchestration::store::{err, Result};
use serde_json::Value;
use std::path::Path;

mod execute;
mod prepare;
mod preview;
mod review;
mod snapshots;

pub use execute::execute;
pub use prepare::{available, prepare, task_binding};
pub use preview::output_preview;
pub(super) use preview::{failure_message, oversized_reply, terminal_failure};
#[cfg(test)]
pub(crate) use review::release_adopted;
pub(crate) use review::{journaled_review, normalize_review};
pub use review::{reconcile_abandoned_reviews, reconcile_adopted_reviews, recover_review};
pub use snapshots::{artifact_path, snapshot};

pub fn atomic_json(path: &Path, value: &Value) -> Result<()> {
    super::durable::publish(path, &serde_json::to_vec(value).map_err(err)?)
}
