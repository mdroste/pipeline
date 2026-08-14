# Formal Privacy and Disclosure

Audit formal privacy claims — differential privacy and related definitions, or disclosure-limitation guarantees — and the utility claims made under them. State the privacy definition and parameters, the unit protected, the mechanism, and the composition accounting across all releases and analyses.

Examine the end-to-end privacy budget, including hyperparameter tuning, retries, and auxiliary releases the accounting omits; the threat model's match to the guarantee, including what the adversary observes and adaptivity; implementation gaps between the analyzed mechanism and the described system, such as sampling assumptions and finite-precision issues; the meaningfulness of the parameters at the claimed granularity — user versus record level; and utility evaluations run at the privacy level actually deployed rather than a favorable one. Check that empirical attack results are not presented as guarantees, nor guarantees as immunity to all inference.

Request a corrected accounting, tighter analysis, or utility-at-parity comparison only when a concrete gap threatens either the guarantee or the claimed utility.
