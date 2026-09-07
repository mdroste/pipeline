* Configure ONLY with /bin/zsh -lic and oldstata on this Mac.
clear all
set obs 6
generate double x = _n
generate double y = 1 + 2*x + cond(mod(_n,2),-.1,.1)
regress y x
local b = strtrim(string(_b[x], "%21.15f"))
local se = strtrim(string(_se[x], "%21.15f"))
local count = e(N)
file open results using "results.json", write replace text
file write results `"{"schema":"research-results-v1","results":[{"resultId":"slope","estimand":"OLS slope of y on x","specificationId":"ols-intercept","sampleId":"six-observations","estimate":`b',"standardError":`se',"confidenceInterval":null,"n":`count',"units":"y units per x unit","transformation":null,"uncertaintyMethod":"homoskedastic OLS standard error"}]}"' _n
file close results
exit, clear
