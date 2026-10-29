# Pipeline result export v1. Requires explicitly installed jsonlite.
pipeline_coefficient <- function(id, estimate, estimand, specification_id, sample_id,
                                 units, standard_error=NULL, confidence_interval=NULL,
                                 n=NULL, uncertainty_method=NULL) {
  stopifnot(all(is.finite(c(estimate, standard_error, confidence_interval))),
            length(estimate)==1, is.null(standard_error) || standard_error>=0)
  list(resultId=id, estimate=estimate, estimand=estimand,
       specificationId=specification_id, sampleId=sample_id, units=units,
       standardError=standard_error, confidenceInterval=confidence_interval,
       n=n, transformation=NULL, uncertaintyMethod=uncertainty_method,
       sourceExecutionId="", artifactLocator="")
}
pipeline_write_results <- function(path, results, convergence=NULL, diagnostics=list()) {
  stopifnot(length(results)>=1, length(results)<=1000,
            !anyDuplicated(vapply(results, function(r) r$resultId, character(1))))
  jsonlite::write_json(list(schema="research-results-v1", results=results,
    diagnostics=list(convergence=convergence, declared=diagnostics)), path,
    auto_unbox=TRUE, null="null", na="null", digits=NA, pretty=TRUE)
}
