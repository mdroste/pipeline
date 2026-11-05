# Pipeline result export v1. Declare JSON3 in Project.toml/Manifest.toml.
using JSON3
function pipeline_coefficient(id, estimate; estimand, specification_id, sample_id,
                              units, standard_error=nothing, confidence_interval=nothing,
                              n=nothing, uncertainty_method=nothing)
    isfinite(estimate) || error("Estimate must be finite")
    isnothing(standard_error) || (isfinite(standard_error) && standard_error >= 0) || error("Invalid standard error")
    (; resultId=id, estimate, estimand, specificationId=specification_id, sampleId=sample_id,
       units, standardError=standard_error, confidenceInterval=confidence_interval, n,
       transformation=nothing, uncertaintyMethod=uncertainty_method,
       sourceExecutionId="", artifactLocator="")
end
function pipeline_write_results(path, results; convergence=nothing, diagnostics=Dict())
    1 <= length(results) <= 1000 || error("Export 1–1000 results")
    length(unique(r.resultId for r in results)) == length(results) || error("Duplicate result IDs")
    open(path,"w") do io
        JSON3.write(io, (;schema="research-results-v1",results,diagnostics=(;convergence,declared=diagnostics)))
    end
end
