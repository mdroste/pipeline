# Sensors and Signal Processing

Audit the path from the underlying phenomenon to the reported signal-derived quantity. State the sensor, sampling rate and range, calibration, synchronization, preprocessing, filter or transform, detection or feature rule, aggregation, and ground-truth validation.

Examine drift, hysteresis, saturation, quantization, aliasing, missing samples, timing error, cross-talk, environmental sensitivity, device and operator variation, baseline correction, filter phase and edge effects, window choices, signal-to-noise ratio, and whether tuning used evaluation data. Check that preprocessing does not remove or manufacture the frequency, transient, or event being interpreted; that repeated measurements are not treated as independent units; and that performance is reported across devices, conditions, and relevant failure regimes.

Each issue should identify the affected measurement or detection claim and propose a calibration trace, raw-signal comparison, synthetic-signal test, alternate filter, synchronization audit, device holdout, uncertainty calculation, or narrower operating range.
