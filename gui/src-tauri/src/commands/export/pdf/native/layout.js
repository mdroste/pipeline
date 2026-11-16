// Measure with the same fonts and column widths used by native printing.
// WebKit does not reliably repeat table headers, so long tables become
// page-sized continuations with an explicit copy of their column headings.
window.pipelineNativePdfState = "loading";
window.pipelineReportReady
  .then(async () => {
    for (const display of document.querySelectorAll(".katex-display")) {
      const math = display.querySelector(".katex");
      if (math && math.scrollWidth > display.clientWidth && display.clientWidth > 0) {
        math.style.fontSize = `${parseFloat(getComputedStyle(math).fontSize) * display.clientWidth / math.scrollWidth}px`;
      }
    }
    await document.fonts.ready;
    const pageHeight = (11 - 0.72 - 0.7) * 96;
    for (const table of document.querySelectorAll("table")) {
      const head = table.tHead;
      const body = table.tBodies[0];
      if (!head || !body || !head.rows[0]) continue;
      const style = getComputedStyle(table);
      // Leave room for a section heading before the first continuation.
      const limit = pageHeight - parseFloat(style.marginTop) - parseFloat(style.marginBottom) - 96;
      if (table.getBoundingClientRect().height <= limit) continue;

      const columns = document.createElement("colgroup");
      const width = table.getBoundingClientRect().width;
      for (const cell of head.rows[0].cells) {
        const column = document.createElement("col");
        column.style.width = `${100 * cell.getBoundingClientRect().width / width}%`;
        columns.append(column);
      }
      const headerHeight = head.getBoundingClientRect().height;
      const rows = Array.from(body.rows, row => ({ row, height: row.getBoundingClientRect().height }));
      let part, partBody, group, height;
      for (const entry of rows) {
        if (!part || (partBody.rows.length && height + entry.height > limit)) {
          part = table.cloneNode(false);
          part.style.tableLayout = "fixed";
          // WebKitGTK can ignore break rules on the table itself. An ordinary
          // block keeps each measured continuation and its header together.
          group = document.createElement("div");
          group.className = "pipeline-table-continuation";
          group.style.pageBreakInside = "avoid";
          if (height !== undefined) group.style.pageBreakBefore = "always";
          part.append(columns.cloneNode(true), head.cloneNode(true));
          partBody = document.createElement("tbody");
          part.append(partBody);
          group.append(part);
          table.before(group);
          height = headerHeight;
        }
        if (headerHeight + entry.height > limit) {
          group.style.pageBreakInside = "auto";
          entry.row.style.pageBreakInside = "auto";
        }
        partBody.append(entry.row);
        height += entry.height;
      }
      table.remove();
    }
    // A flex issue heading can ignore break-after in WebKit. Keep its first
    // paragraph (or a short table) in the same ordinary block when it fits.
    for (const heading of document.querySelectorAll(".report-body .comment-header,.report-body h1,.report-body h2,.report-body h3")) {
      const next = heading.nextElementSibling;
      if (!next || !next.matches("p,table,.pipeline-table-continuation,.pipeline-display-equation")) continue;
      if (next.getBoundingClientRect().bottom - heading.getBoundingClientRect().top > pageHeight - 24) continue;
      const group = document.createElement("div");
      group.style.pageBreakInside = "avoid";
      heading.before(group);
      group.append(heading, next);
    }
    document.body.getBoundingClientRect();
    window.pipelineNativePdfState = "ready";
  })
  .catch(error => {
    window.pipelineNativePdfState = `error:${String(error).slice(0, 500)}`;
  });
