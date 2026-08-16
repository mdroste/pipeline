# Replication-Package Audit

Audit the paper's computational transparency: whether the supplied or described data-and-code package would let a competent reader reproduce the headline results. Reconstruct the package-to-paper pipeline — data sources and access conditions, file and script entry points, processing stages, execution order, estimation or analysis code, and the mapping from outputs to the reported tables and figures. Treat algorithmic correctness, numerical behavior, dependency sensitivity, and software failure handling as research-software questions unless they directly break package traceability.

Examine whether each headline exhibit is traceable to a described procedure; undocumented steps the results depend on, such as exclusions, recodes, tuning constants, and seeds; data-availability claims against the described access reality; version and dependency information where results plausibly depend on them; and inconsistencies between the paper's description and the package structure it references. Where artifacts are provided in the reviewable material, verify them by reading rather than assuming.

Report only gaps that block or materially endanger reproduction of central results. Do not demand engineering polish, and do not fault restricted data when the paper is honest about access and provides what it can.
