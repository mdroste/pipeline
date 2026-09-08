# Versioned result exporters

These helpers produce `research-results-v1` for **Analyses → Experiments**. Import only an adopted output from a successful execution. Pipeline attaches the execution ID and exact artifact locator itself; the exporter does not choose provenance. Missing uncertainty is JSON `null`, never zero. Python, R, and Julia support declared convergence and diagnostics in the retained export artifact; these declarations are not automatic validation.

Declare the helper, analysis script, data, and dependency lockfiles as profile inputs. Use relative script paths. **Analyses → Captured execution** captures those inputs, records a declared toolchain version and executable hash, and writes `pipeline-parameters.json` into every new run directory. `PIPELINE_PARAMETERS_FILE` names that file; `PIPELINE_RANDOM_SEED` carries an optional seed. The script must actually read and apply those values. A seed alone does not promise deterministic computation.

Python uses only the standard library. R needs `jsonlite`; Julia needs `JSON3` recorded in its environment. These helpers do not install packages. The Stata helper is a scalar export program; on this Mac load and invoke it only within `/bin/zsh -lic 'oldstata …'` and finish the do-file with `exit, clear`. No direct Stata binary or diagnostic is permitted. R, Julia, and Stata helpers require qualification with the researcher's installed toolchain before release claims.

Raw data is excluded from selective `.pwex` project exchange. `.pwrx` is a private whole-store recovery backup and contains captured local data; it is not a coauthor sharing format. Dataset dictionary and summary policy applies to curated search/context services. Arbitrary host commands still have host access.

For metadata-only data registration, `dataset-version-v1.example.json` shows the accepted dictionary shape. Replace its example hash, external version reference, dimensions, variable definitions and acquisition details with your exporter's actual declarations. Paste it into **Data & samples → Import an exporter dictionary**. Pipeline marks external diagnostics as declarations and does not read the external dataset.
