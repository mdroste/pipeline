//! Ledger owned by the Workspace research service.

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRecord {
    pub id: String,
    pub workspace_id: String,
    pub paper_id: Option<String>,
    pub workflow_state: String,
    pub version_id: String,
    pub version: i64,
    pub claim: String,
    pub kind: String,
    pub origin: String,
    pub paper_locator: Option<Value>,
    pub dependency_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposeClaimRequest {
    pub workspace_id: String,
    pub paper_id: Option<String>,
    pub claim: String,
    pub kind: String,
    pub origin: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetClaimStateRequest {
    pub claim_id: String,
    pub state: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRecord {
    pub id: String,
    pub workspace_id: String,
    pub claim_version_id: String,
    pub target_type: String,
    pub target_id: String,
    pub locator: Option<Value>,
    pub relation: String,
    pub assessment: String,
    pub assessor: String,
    pub dependency_hash: Option<String>,
    pub freshness: String,
    pub stale_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposeEvidenceRequest {
    pub workspace_id: String,
    pub claim_version_id: String,
    pub target_type: String,
    pub target_id: String,
    pub locator: Option<Value>,
    pub relation: String,
    pub assessor: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmEvidenceRequest {
    pub evidence_id: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchLedger {
    pub claims: Vec<ClaimRecord>,
    pub evidence: Vec<EvidenceRecord>,
    pub unsupported_accepted_claims: Vec<String>,
    pub stale_claims: Vec<String>,
}

fn claim_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClaimRecord> {
    let locator: Option<String> = row.get(9)?;
    Ok(ClaimRecord {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        paper_id: row.get(2)?,
        workflow_state: row.get(3)?,
        version_id: row.get(4)?,
        version: row.get(5)?,
        claim: row.get(6)?,
        kind: row.get(7)?,
        origin: row.get(8)?,
        paper_locator: locator.and_then(|value| serde_json::from_str(&value).ok()),
        dependency_hash: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn evidence_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EvidenceRecord> {
    let locator: Option<String> = row.get(5)?;
    Ok(EvidenceRecord {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        claim_version_id: row.get(2)?,
        target_type: row.get(3)?,
        target_id: row.get(4)?,
        locator: locator.and_then(|value| serde_json::from_str(&value).ok()),
        relation: row.get(6)?,
        assessment: row.get(7)?,
        assessor: row.get(8)?,
        dependency_hash: row.get(9)?,
        freshness: row.get(10)?,
        stale_reason: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

pub fn propose_claim(
    store: &Store,
    request: ProposeClaimRequest,
    model_origin: bool,
) -> WorkbenchResult<ClaimRecord> {
    validate_id("workspace id", &request.workspace_id)?;
    let claim_text = validate_text("Claim", &request.claim, 128 * 1024)?;
    let kind = validate_text("Claim kind", &request.kind, 100)?;
    let origin = validate_text("Claim origin", &request.origin, 500)?;
    let claim_id = new_id("claim")?;
    let version_id = new_id("claimv")?;
    let timestamp = now();
    let mut connection = open_connection(store)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| WorkbenchError::storage("Failed to start claim proposal", error))?;
    transaction.execute("INSERT INTO claims (id, workspace_id, paper_id, current_version_id, workflow_state, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'proposed', ?5, ?5)", params![claim_id, request.workspace_id, request.paper_id, version_id, timestamp]).map_err(|error| WorkbenchError::storage("Failed to create claim", error))?;
    transaction.execute("INSERT INTO claim_versions (id, claim_id, version, claim_text, kind, origin, created_at) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6)", params![version_id, claim_id, claim_text, kind, origin, timestamp]).map_err(|error| WorkbenchError::storage("Failed to create claim version", error))?;
    append_change(
        &transaction,
        &request.operation_id,
        "claim",
        &claim_id,
        (Some(&request.workspace_id), None),
        "proposed",
        &json!({"versionId":version_id,"modelOrigin":model_origin}),
    )?;
    transaction
        .commit()
        .map_err(|error| WorkbenchError::storage("Failed to commit claim proposal", error))?;
    get_claim(store, &request.workspace_id, &claim_id)
}

fn get_claim(store: &Store, workspace_id: &str, claim_id: &str) -> WorkbenchResult<ClaimRecord> {
    open_connection(store)?.query_row("SELECT c.id, c.workspace_id, c.paper_id, c.workflow_state, v.id, v.version, v.claim_text, v.kind, v.origin, v.paper_locator_json, v.dependency_hash, c.created_at, c.updated_at FROM claims c JOIN claim_versions v ON v.id = c.current_version_id WHERE c.id = ?1 AND c.workspace_id = ?2", params![claim_id, workspace_id], claim_from_row).optional().map_err(|error| WorkbenchError::storage("Failed to read claim", error))?.ok_or_else(|| WorkbenchError::invalid("Claim was not found in this workspace"))
}

pub fn set_claim_state(
    store: &Store,
    request: SetClaimStateRequest,
) -> WorkbenchResult<ClaimRecord> {
    validate_id("claim id", &request.claim_id)?;
    if !matches!(
        request.state.as_str(),
        "proposed" | "under_review" | "accepted" | "retired"
    ) {
        return Err(WorkbenchError::invalid("Unknown claim workflow state"));
    }
    let mut connection = open_connection(store)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| WorkbenchError::storage("Failed to start claim state update", error))?;
    let workspace_id: String = transaction
        .query_row(
            "SELECT workspace_id FROM claims WHERE id = ?1",
            [&request.claim_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to inspect claim", error))?
        .ok_or_else(|| WorkbenchError::invalid("Claim was not found"))?;
    transaction
        .execute(
            "UPDATE claims SET workflow_state = ?2, updated_at = ?3 WHERE id = ?1",
            params![request.claim_id, request.state, now()],
        )
        .map_err(|error| WorkbenchError::storage("Failed to update claim state", error))?;
    append_change(
        &transaction,
        &request.operation_id,
        "claim",
        &request.claim_id,
        (Some(&workspace_id), None),
        "state_changed",
        &json!({"state":request.state,"actor":"user"}),
    )?;
    transaction
        .commit()
        .map_err(|error| WorkbenchError::storage("Failed to commit claim state", error))?;
    get_claim(store, &workspace_id, &request.claim_id)
}

fn target_hash(
    connection: &Connection,
    workspace_id: &str,
    target_type: &str,
    target_id: &str,
) -> WorkbenchResult<Option<String>> {
    let query = match target_type {
        "paper_revision" => "SELECT pr.content_hash FROM paper_revisions pr JOIN papers p ON p.id = pr.paper_id WHERE pr.id = ?1 AND p.workspace_id = ?2",
        "source_version" => "SELECT sv.content_hash FROM source_versions sv JOIN sources s ON s.id = sv.source_id WHERE sv.id = ?1 AND s.workspace_id = ?2",
        "execution" => "SELECT dependency_hash FROM research_executions WHERE id = ?1 AND workspace_id = ?2",
        "artifact" => "SELECT content_hash FROM artifacts WHERE id = ?1 AND workspace_id = ?2",
        "derivation" => return Ok(None),
        _ => return Err(WorkbenchError::invalid("Unknown evidence target type")),
    };
    let found = connection
        .query_row(query, params![target_id, workspace_id], |row| {
            row.get::<_, Option<String>>(0)
        })
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to validate evidence target", error))?;
    found.ok_or_else(|| WorkbenchError::invalid("Evidence target was not found in this workspace"))
}

pub fn propose_evidence(
    store: &Store,
    request: ProposeEvidenceRequest,
    model_origin: bool,
) -> WorkbenchResult<EvidenceRecord> {
    validate_id("workspace id", &request.workspace_id)?;
    validate_id("claim version id", &request.claim_version_id)?;
    provider_identifier("evidence target id", &request.target_id)?;
    if !matches!(
        request.relation.as_str(),
        "supports" | "contradicts" | "qualifies"
    ) {
        return Err(WorkbenchError::invalid("Unknown evidence relation"));
    }
    let assessor = validate_text("Evidence assessor", &request.assessor, 500)?;
    let locator = request
        .locator
        .map(|value| json_object("Evidence locator", value, 64 * 1024))
        .transpose()?
        .map(|(_, encoded)| encoded);
    let mut connection = open_connection(store)?;
    let dependency_hash = target_hash(
        &connection,
        &request.workspace_id,
        &request.target_type,
        &request.target_id,
    )?;
    let claim_exists: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM claim_versions v JOIN claims c ON c.id = v.claim_id WHERE v.id = ?1 AND c.workspace_id = ?2)", params![request.claim_version_id, request.workspace_id], |row| row.get(0)).map_err(|error| WorkbenchError::storage("Failed to validate evidence claim", error))?;
    if !claim_exists {
        return Err(WorkbenchError::invalid(
            "Claim version was not found in this workspace",
        ));
    }
    let id = new_id("evidence")?;
    let timestamp = now();
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| WorkbenchError::storage("Failed to start evidence proposal", error))?;
    let assessment = if model_origin {
        "model_assessed"
    } else {
        "not_checked"
    };
    let freshness = if dependency_hash.is_some() {
        "current"
    } else {
        "unknown"
    };
    transaction.execute("INSERT INTO evidence_links (id, workspace_id, claim_version_id, target_type, target_id, locator_json, relation, assessment, assessor, dependency_hash, freshness, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)", params![id, request.workspace_id, request.claim_version_id, request.target_type, request.target_id, locator, request.relation, assessment, assessor, dependency_hash, freshness, timestamp]).map_err(|error| WorkbenchError::storage("Failed to create evidence proposal", error))?;
    append_change(
        &transaction,
        &request.operation_id,
        "evidence_link",
        &id,
        (Some(&request.workspace_id), None),
        "proposed",
        &json!({"assessment":assessment,"modelOrigin":model_origin}),
    )?;
    transaction
        .commit()
        .map_err(|error| WorkbenchError::storage("Failed to commit evidence proposal", error))?;
    get_evidence(store, &request.workspace_id, &id)
}

pub(super) fn get_evidence(
    store: &Store,
    workspace_id: &str,
    evidence_id: &str,
) -> WorkbenchResult<EvidenceRecord> {
    open_connection(store)?.query_row("SELECT id, workspace_id, claim_version_id, target_type, target_id, locator_json, relation, assessment, assessor, dependency_hash, freshness, stale_reason, created_at, updated_at FROM evidence_links WHERE id = ?1 AND workspace_id = ?2", params![evidence_id, workspace_id], evidence_from_row).optional().map_err(|error| WorkbenchError::storage("Failed to read evidence", error))?.ok_or_else(|| WorkbenchError::invalid("Evidence link was not found in this workspace"))
}

pub fn confirm_evidence(
    store: &Store,
    request: ConfirmEvidenceRequest,
) -> WorkbenchResult<EvidenceRecord> {
    validate_id("evidence id", &request.evidence_id)?;
    let mut connection = open_connection(store)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| WorkbenchError::storage("Failed to start evidence confirmation", error))?;
    let workspace_id: String = transaction
        .query_row(
            "SELECT workspace_id FROM evidence_links WHERE id = ?1",
            [&request.evidence_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to inspect evidence", error))?
        .ok_or_else(|| WorkbenchError::invalid("Evidence link was not found"))?;
    transaction.execute("UPDATE evidence_links SET assessment = 'human_confirmed', assessor = 'user', updated_at = ?2 WHERE id = ?1", params![request.evidence_id, now()]).map_err(|error| WorkbenchError::storage("Failed to confirm evidence", error))?;
    append_change(
        &transaction,
        &request.operation_id,
        "evidence_link",
        &request.evidence_id,
        (Some(&workspace_id), None),
        "human_confirmed",
        &json!({"actor":"user"}),
    )?;
    transaction.commit().map_err(|error| {
        WorkbenchError::storage("Failed to commit evidence confirmation", error)
    })?;
    get_evidence(store, &workspace_id, &request.evidence_id)
}

pub fn research_ledger(store: &Store, workspace_id: &str) -> WorkbenchResult<ResearchLedger> {
    validate_id("workspace id", workspace_id)?;
    let connection = open_connection(store)?;
    let mut claims_statement = connection.prepare("SELECT c.id, c.workspace_id, c.paper_id, c.workflow_state, v.id, v.version, v.claim_text, v.kind, v.origin, v.paper_locator_json, v.dependency_hash, c.created_at, c.updated_at FROM claims c JOIN claim_versions v ON v.id = c.current_version_id WHERE c.workspace_id = ?1 AND c.workflow_state <> 'retired' ORDER BY c.updated_at DESC LIMIT 500").map_err(|error| WorkbenchError::storage("Failed to prepare claim ledger", error))?;
    let claims = claims_statement
        .query_map([workspace_id], claim_from_row)
        .map_err(|error| WorkbenchError::storage("Failed to list claims", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode claims", error))?;
    let mut evidence_statement = connection.prepare("SELECT id, workspace_id, claim_version_id, target_type, target_id, locator_json, relation, assessment, assessor, dependency_hash, freshness, stale_reason, created_at, updated_at FROM evidence_links WHERE workspace_id = ?1 ORDER BY updated_at DESC LIMIT 1000").map_err(|error| WorkbenchError::storage("Failed to prepare evidence ledger", error))?;
    let evidence = evidence_statement
        .query_map([workspace_id], evidence_from_row)
        .map_err(|error| WorkbenchError::storage("Failed to list evidence", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode evidence", error))?;
    let unsupported_accepted_claims = claims
        .iter()
        .filter(|claim| {
            claim.workflow_state == "accepted"
                && !evidence.iter().any(|item| {
                    item.claim_version_id == claim.version_id
                        && item.relation == "supports"
                        && item.freshness == "current"
                })
        })
        .map(|claim| claim.id.clone())
        .collect();
    let stale_claims = claims
        .iter()
        .filter(|claim| {
            evidence
                .iter()
                .any(|item| item.claim_version_id == claim.version_id && item.freshness == "stale")
        })
        .map(|claim| claim.id.clone())
        .collect();
    Ok(ResearchLedger {
        claims,
        evidence,
        unsupported_accepted_claims,
        stale_claims,
    })
}
