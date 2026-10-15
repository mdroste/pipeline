//! Content identity for project exchange objects.
use super::{hex, sort_json};
use serde_json::Value;

/// Content fingerprint that ignores volatile bookkeeping so identical research
/// content compares equal across stores.
pub fn fingerprint(value: &Value) -> String {
    let mut v = value.clone();
    fn strip(v: &mut Value) {
        match v {
            Value::Object(o) => {
                for key in [
                    "workspaceId",
                    "updatedAt",
                    "createdAt",
                    "revision",
                    "textReference",
                    "storageReference",
                    "cwd",
                    "sessionId",
                    "profileId",
                    "artifactId",
                    "compiledArtifactId",
                    "entrypoint",
                    "paperId",
                    // Rewritten by every import: evidence freshness and
                    // unpackaged-output markers never describe content.
                    "freshness",
                    "staleReason",
                    "unavailable",
                ] {
                    o.remove(key);
                }
                o.values_mut().for_each(strip);
            }
            Value::Array(a) => a.iter_mut().for_each(strip),
            _ => {}
        }
    }
    strip(&mut v);
    sort_json(&mut v);
    hex(v.to_string().as_bytes())
}
