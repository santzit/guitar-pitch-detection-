/*
 * C-compatible header for the cycfi/q pitch-detector wrapper.
 *
 * These are the only functions exposed across the Rust–C++ FFI boundary.
 * All Q internals stay on the C++ side; Rust only sees opaque pointers and
 * primitive types.
 */
#pragma once

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/** Opaque handle to a heap-allocated q::pitch_detector instance. */
struct QPitchDetector;
struct QBandpassFilter;

/**
 * Allocate and initialise a Q pitch detector.
 *
 * @param min_freq_hz   Lowest frequency of interest (Hz).  Guitar low-E ≈ 82.4 Hz.
 * @param max_freq_hz   Highest frequency of interest (Hz). Guitar high-E fret 22 ≈ 1318.5 Hz.
 * @param sample_rate   Audio sample rate in Hz (e.g. 44100, 48000).
 * @param hysteresis_db Suppresses spurious pitch shifts.  Negative value, e.g. –45.0 dB.
 * @return Pointer to the new detector, or NULL on allocation failure.
 */
struct QPitchDetector* q_pd_create(
    float min_freq_hz,
    float max_freq_hz,
    float sample_rate,
    float hysteresis_db
);

/** Free a detector created by q_pd_create.  Safe to call with NULL. */
void q_pd_destroy(struct QPitchDetector* pd);

/**
 * Feed one mono audio sample (f32, normalised −1 … 1) to the detector.
 *
 * @return 1 when Q has completed a new pitch analysis period (a fresh
 *         frequency / periodicity value is now available via the getters),
 *         0 otherwise.
 */
int q_pd_process(struct QPitchDetector* pd, float sample);

/** Most recently detected frequency in Hz.  Returns 0.0 if nothing detected yet. */
float q_pd_get_frequency(struct QPitchDetector* pd);

/**
 * Periodicity confidence of the last detected pitch.
 *
 * Range [0, 1].  Values above ~0.8 indicate a strongly periodic (tonal) signal.
 * Use this as a confidence score for the detected pitch.
 */
float q_pd_get_periodicity(struct QPitchDetector* pd);

/** Reset internal state.  Call between songs or after a long pause. */
void q_pd_reset(struct QPitchDetector* pd);

/**
 * Allocate and initialise a Q constant-skirt-gain band-pass filter.
 *
 * @param center_freq_hz Center frequency in Hz.
 * @param sample_rate    Audio sample rate in Hz.
 * @param q_factor       Quality factor (> 0). Higher = narrower band.
 * @return Pointer to the new filter, or NULL on allocation failure.
 */
struct QBandpassFilter* q_bp_create(
    float center_freq_hz,
    float sample_rate,
    float q_factor
);

/** Free a filter created by q_bp_create. Safe to call with NULL. */
void q_bp_destroy(struct QBandpassFilter* bp);

/** Reconfigure the filter center frequency / Q for the same sample rate. */
void q_bp_config(struct QBandpassFilter* bp, float center_freq_hz, float sample_rate, float q_factor);

/** Feed one mono sample through the filter and return the filtered sample. */
float q_bp_process(struct QBandpassFilter* bp, float sample);

/** Reset filter state (delay buffers). */
void q_bp_reset(struct QBandpassFilter* bp);

#ifdef __cplusplus
} /* extern "C" */
#endif
