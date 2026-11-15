/** Native pages are selected by immutable id, then presented in recent order. */
export async function allProjectPages<
  T extends { id: string; updatedAt: string },
>(read: (afterId?: string) => Promise<T[]>) {
  const rows: T[] = [];
  let afterId: string | undefined;
  for (;;) {
    const page = await read(afterId);
    rows.push(...page);
    if (page.length < 500) break;
    const lastId: string = page.reduce(
      (max, row) => (row.id > max ? row.id : max),
      afterId ?? "",
    );
    if (lastId === afterId)
      throw new Error(
        "The project list did not advance. Refresh Projects to try again.",
      );
    afterId = lastId;
  }
  return rows.sort(
    (a, b) =>
      b.updatedAt.localeCompare(a.updatedAt) || a.id.localeCompare(b.id),
  );
}
