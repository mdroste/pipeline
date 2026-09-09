//! Workflow executor entry points.
//!
//! `schedule` owns dependency readiness for both preview and runtime. `run`
//! owns mutable execution state; `parallel` and `sequential` dispatch through
//! `step_call`, whose provider calls remain supervised by `pipeline::call`.
//! Artifact selection, prompts, findings and checkpoint products have separate
//! owners within this feature. None share writable Workspace runtime state.

mod artifact_context;
mod checkpoints;
mod findings;
mod outputs;
mod parallel;
mod paths;
mod prompts;
mod run;
mod schedule;
mod sequential;
mod step_call;
mod units;

pub(crate) use paths::step_slug;
pub use run::{execute_steps, ExecutionResult};
pub use schedule::{dependents_of, execution_plan, input_processing_label, ExecutionPlanStage};

#[cfg(test)]
mod tests;
