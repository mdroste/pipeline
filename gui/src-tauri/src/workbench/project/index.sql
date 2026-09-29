SELECT w.id, w.name, w.root, w.missing_root_at, w.updated_at,
  (SELECT json_object('id', n.id, 'title', substr(n.body, 1, 600),
                      'updatedAt', n.updated_at, 'kind', n.kind)
   FROM research_notes n
   WHERE n.workspace_id = w.id AND n.state = 'accepted'
     AND (n.kind = 'question' OR n.pinned = 1 OR n.id IN (
       SELECT value FROM json_each(COALESCE(h.body_json, '{}'), '$.briefNoteIds')))
   ORDER BY (n.id IN (SELECT value FROM json_each(COALESCE(h.body_json, '{}'), '$.briefNoteIds'))) DESC,
            (n.kind = 'question') DESC, n.pinned DESC, n.updated_at DESC, n.id
   LIMIT 1),
  (SELECT json_object('id', s.id, 'title', substr(s.title, 1, 240),
                      'updatedAt', s.updated_at, 'kind', 'conversation')
   FROM sessions s WHERE s.workspace_id = w.id AND s.archived_at IS NULL
   ORDER BY s.updated_at DESC, s.id LIMIT 1),
  (SELECT json_object('id', a.id, 'title', a.title, 'updatedAt', a.updated_at, 'kind', a.kind)
   FROM (
     SELECT id, substr(title, 1, 240) AS title, updated_at, 'conversation' AS kind
       FROM sessions WHERE workspace_id = w.id AND archived_at IS NULL
     UNION ALL
     SELECT id, substr(title, 1, 240), updated_at, 'document'
       FROM papers WHERE workspace_id = w.id AND current_revision_id IS NOT NULL
     UNION ALL
     SELECT id, substr(body, 1, 240), updated_at, 'note'
       FROM research_notes WHERE workspace_id = w.id AND state = 'accepted'
     UNION ALL
     SELECT id, substr(json_extract(body_json, '$.objective'), 1, 240), updated_at, 'task'
       FROM project_records WHERE workspace_id = w.id AND kind = 'task'
   ) a ORDER BY a.updated_at DESC, a.id LIMIT 1),
  (SELECT json_object('id', t.id, 'title', substr(json_extract(t.body_json, '$.objective'), 1, 240),
                      'updatedAt', t.updated_at, 'kind', 'task')
   FROM project_records t WHERE t.workspace_id = w.id AND t.kind = 'task'
     AND json_extract(t.body_json, '$.status') NOT IN ('completed', 'rejected')
   ORDER BY (json_extract(t.body_json, '$.status') = 'deferred'), t.updated_at DESC, t.id LIMIT 1),
  (SELECT count(*) FROM research_notes n WHERE n.workspace_id = w.id AND n.state = 'proposed'),
  (SELECT count(*) FROM project_records a WHERE a.workspace_id = w.id AND a.kind = 'application'
     AND json_extract(a.body_json, '$.state') IN ('applying', 'recovery_required'))
FROM workspaces w
LEFT JOIN project_records h ON h.workspace_id = w.id AND h.kind = 'home' AND h.id = 'home_' || w.id
WHERE w.archived_at IS NULL
ORDER BY w.updated_at DESC, w.id
LIMIT ?1
