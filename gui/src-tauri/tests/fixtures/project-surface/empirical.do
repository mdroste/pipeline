clear all
set more off
set obs 20
generate double x = _n
generate double y = 1 + 2*x
regress y x
assert abs(_b[x] - 2) < 1e-10
file open result using "estimate.txt", write replace
file write result "slope=2" _n
file close result
exit, clear
