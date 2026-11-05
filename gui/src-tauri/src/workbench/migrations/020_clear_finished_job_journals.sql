-- A finished job's output lives on its receipt, so its finalization journal is a
-- second copy that nothing reads. A run with an unknown outcome keeps its journal
-- for reconciliation, and so does a queued or running one: a crash after the
-- journal was written leaves it that way until it is marked unknown.
UPDATE execution_jobs SET finalization_json=NULL
WHERE finalization_json IS NOT NULL
  AND execution_id IN (
    SELECT id FROM research_executions
    WHERE outcome IN ('completed','failed','interrupted','timed_out')
  );
