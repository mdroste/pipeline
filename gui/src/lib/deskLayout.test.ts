import { beforeEach, expect, it } from 'vitest';
import { loadDeskLayout, saveDeskLayout } from './deskLayout';
beforeEach(() => localStorage.clear());
it('restores the exact revision, comparison and subpage independently for each project', () => {
  const layout = { ...loadDeskLayout('a'), tab: 'data', revisionId: 'old-revision', pinnedRevision: 'older-revision', assistantWidth: 47, object: { kind: 'result', id: 'result-one', revision: 'old-hash', start: 10, end: 30 } };
  saveDeskLayout('a', layout);
  expect(loadDeskLayout('a')).toEqual(layout);
  expect(loadDeskLayout('b').revisionId).toBeNull();
});
it('contains malformed preferences to the layout instead of breaking the desk', () => {
  localStorage.setItem('pipeline.desk.a', JSON.stringify({ tab: 'removed', object: { id: 2 }, assistantWidth: 200 }));
  expect(loadDeskLayout('a')).toMatchObject({ tab: 'overview', object: null, assistantWidth: 60 });
});
