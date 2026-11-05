# LLMs as Research Instruments

Audit studies that use language models as instruments of research — annotators, simulated participants, data generators, or analysis assistants — where the conclusions are about something other than the model. State each role the model plays, the validation against human or ground-truth benchmarks, and the claims that depend on model output.

Examine annotation validity: agreement with human coders on a representative validation set, error correlation with the constructs under study, and prompt-induced drift across the corpus. Examine simulated-participant logic: what makes model responses evidence about humans, which population they are claimed to represent, and sensitivity to persona and prompt framing. Check contamination when the model has plausibly seen the study's materials, version pinning and pipeline reproducibility, and propagation of instrument error into downstream estimates. "The model agrees with humans" must be established for the cases that matter, not on average.

Request additional human validation or an error-propagation analysis only when a concrete failure of the instrument threatens the substantive conclusion.
