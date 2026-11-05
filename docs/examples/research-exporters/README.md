# Result exporters

These helpers write your analysis results in the `research-results-v1` format,
which Pipeline imports under **Analyze → Experiments** with **Import
research-results-v1 / v2**. There's one helper each for Python, R, Julia, and
Stata. `research-results-v1.example.json` shows a complete file.

## Use a helper

1. Call the helper at the end of your analysis script to write the results
   file.
2. In **Analyze → Saved analysis plans**, list the helper, your script, your
   data, and your lockfiles as inputs. Give scripts as paths relative to the
   project folder.
3. Run the plan. Pipeline writes `pipeline-parameters.json` into each new run
   folder and sets `PIPELINE_PARAMETERS_FILE` to its path. If you set a random
   seed, it's in `PIPELINE_RANDOM_SEED`. Your script has to read these values
   for them to take effect.
4. Import the results file from a run that finished successfully. Pipeline
   records which run and file the results came from.

Write a missing standard error or interval as `null`, not `0`. The Python, R,
and Julia helpers can also record convergence and diagnostics that you
declare. Pipeline shows these as your declarations and doesn't check them.

## Requirements

- Python uses only the standard library.
- R needs `jsonlite`.
- Julia needs `JSON3` in the project environment.
- Stata: run the helper through the Stata launcher in **Analyze → Command
  settings**, and end the do-file with `exit, clear`.

The helpers don't install packages. Test the R, Julia, and Stata helpers with
your own installation before you rely on them.

## Register a dataset without importing it

`dataset-version-v1.example.json` shows the format for describing a dataset
Pipeline doesn't read, such as one stored elsewhere. Replace the example
hash, version reference, dimensions, variables, and source details with your
own, then paste it into **Analyze → Data & samples → Import a data
dictionary**.

## Sharing

A project package (`.pwex`) includes raw data only if you turn on **Allow raw
data in sharing packages** under **Analyze → Data & samples**. A research
backup (`.pwrx`) is a private copy of your whole library, including captured
data, so don't use it to share with coauthors. See
[Back up and share](../../projects.md#back-up-and-share).
