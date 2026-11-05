-- Transcript pages are ordered by time within each binding. This index holds
-- everything the page query reads, so choosing a page touches no payloads.
CREATE INDEX transcript_items_page
    ON transcript_items(binding_id, created_at, id, provider_item_id);
