//! Qualified Codex App Server integration boundary.

use crate::agent_runtime::codex::compatibility;
pub mod native_prompts;
mod probe;
mod process;
mod supervisor;
use crate::agent_runtime::codex::transport;
use crate::agent_runtime::codex::wire;

pub use compatibility::{
    detected_version, DetectedCodexVersion, EXPERIMENTAL_SCHEMA_SHA256, MINIMUM_CODEX_VERSION,
    SCHEMA_REFERENCE_CODEX_VERSION, STABLE_SCHEMA_SHA256,
};
pub use probe::{run_qualification_probe, QualificationProbeReport};
pub(crate) use supervisor::event_thread_id;
pub use supervisor::{
    supervisor_manager, AccountState, AccountStatus, AppServerSupervisor, LoginStart, ModelCatalog,
    RateLimitBucket, RateLimitSource, RateLimitWindow, RateLimits, ReasoningEffortOption,
    SideTurnRequest, StartThreadRequest, StartTurnRequest, SupervisorManager, SupervisorStatus,
    ThreadConnection, WorkspaceModel,
};
pub use transport::{AppServerClient, RequestError, DEFAULT_REQUEST_TIMEOUT};
pub use wire::{InitializeResult, NormalizedEvent};
