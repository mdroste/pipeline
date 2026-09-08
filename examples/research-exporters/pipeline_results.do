* Pipeline result export v1. Run only through /bin/zsh -lic 'oldstata ...'.
* This scalar helper writes one explicitly named coefficient. No log scraping.
capture program drop pipeline_coefficient
program define pipeline_coefficient
    version 15
    syntax using/, ID(string) ESTimate(real) ESTIMand(string) SPECification(string) SAMPLE(string) UNITS(string) [SE(real -1) N(real -1)]
    if missing(`estimate') | (`se' != -1 & (missing(`se') | `se' < 0)) {
        display as error "Finite estimate and nonnegative standard error required"
        exit 198
    }
    * Reject characters requiring JSON escaping; do not emit invalid JSON.
    foreach name in id estimand specification sample units {
        if strpos(`"``name''"', char(34)) | strpos(`"``name''"', char(92)) | strpos(`"``name''"', char(10)) {
            display as error "Labels cannot contain quotes, backslashes or newlines"
            exit 198
        }
    }
    local sejson "null"
    if `se' >= 0 local sejson : display %24.17g `se'
    local njson "null"
    if `n' >= 0 {
        if `n' != floor(`n') exit 198
        local njson : display %18.0f `n'
    }
    tempname out
    file open `out' using `"`using'"', write text replace
    file write `out' `"{"schema":"research-results-v1","results":[{"resultId":"`id'","estimate":' %24.17g (`estimate') `',"estimand":"`estimand'","specificationId":"`specification'","sampleId":"`sample'","units":"`units'","standardError":`sejson',"confidenceInterval":null,"n":`njson',"transformation":null,"uncertaintyMethod":null,"sourceExecutionId":"","artifactLocator":""}]}"' _n
    file close `out'
end
