/*
 * C++ implementation of the q_wrapper FFI layer.
 *
 * Wraps cycfi::q::pitch_detector in a plain-C API so that Rust can link to
 * it without requiring a C++ ABI on the Rust side.
 *
 * cycfi/q is MIT licensed: https://github.com/cycfi/q
 */

#include "q_wrapper.hpp"

#include <q/pitch/pitch_detector.hpp>
#include <q/fx/biquad.hpp>
#include <q/support/frequency.hpp>
#include <q/support/decibel.hpp>
#include <q/support/unit.hpp>

using namespace cycfi::q;

/* ── Internal C++ struct ──────────────────────────────────────────────────── */

struct QPitchDetector {
    pitch_detector detector;

    QPitchDetector(float min_freq, float max_freq, float sps, float hysteresis_db)
        : detector{
              frequency{ static_cast<double>(min_freq)  },
              frequency{ static_cast<double>(max_freq)  },
              sps,
              /* decibel(double) constructor is deleted in newer Q; use direct_unit
                 to bypass the deleted conversion and pass the raw dB value.      */
              decibel{ static_cast<double>(hysteresis_db), direct_unit }
          }
    {}
};

struct QBandpassFilter {
    bandpass_csg filter;

    QBandpassFilter(float center_freq_hz, float sample_rate, float q_factor)
        : filter{frequency{ static_cast<double>(center_freq_hz) }, sample_rate, static_cast<double>(q_factor)}
    {}

    void config(float center_freq_hz, float sample_rate, float q_factor) {
        filter.config(frequency{ static_cast<double>(center_freq_hz) }, sample_rate, static_cast<double>(q_factor));
    }

    void reset() {
        filter.x1 = 0.0f;
        filter.x2 = 0.0f;
        filter.y1 = 0.0f;
        filter.y2 = 0.0f;
    }
};

/* ── C API implementation ─────────────────────────────────────────────────── */

extern "C" {

QPitchDetector* q_pd_create(
    float min_freq_hz,
    float max_freq_hz,
    float sample_rate,
    float hysteresis_db)
{
    try {
        return new QPitchDetector(min_freq_hz, max_freq_hz, sample_rate, hysteresis_db);
    } catch (...) {
        return nullptr;
    }
}

void q_pd_destroy(QPitchDetector* pd)
{
    delete pd;   /* delete nullptr is a no-op in C++, so no null check needed */
}

int q_pd_process(QPitchDetector* pd, float sample)
{
    if (!pd) return 0;
    try {
        return pd->detector(sample) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

float q_pd_get_frequency(QPitchDetector* pd)
{
    if (!pd) return 0.0f;
    return pd->detector.get_frequency();
}

float q_pd_get_periodicity(QPitchDetector* pd)
{
    if (!pd) return 0.0f;
    return pd->detector.periodicity();
}

void q_pd_reset(QPitchDetector* pd)
{
    if (!pd) return;
    pd->detector.reset();
}

QBandpassFilter* q_bp_create(
    float center_freq_hz,
    float sample_rate,
    float q_factor)
{
    try {
        return new QBandpassFilter(center_freq_hz, sample_rate, q_factor);
    } catch (...) {
        return nullptr;
    }
}

void q_bp_destroy(QBandpassFilter* bp)
{
    delete bp;
}

void q_bp_config(QBandpassFilter* bp, float center_freq_hz, float sample_rate, float q_factor)
{
    if (!bp) return;
    bp->config(center_freq_hz, sample_rate, q_factor);
}

float q_bp_process(QBandpassFilter* bp, float sample)
{
    if (!bp) return 0.0f;
    try {
        return bp->filter(sample);
    } catch (...) {
        return 0.0f;
    }
}

void q_bp_reset(QBandpassFilter* bp)
{
    if (!bp) return;
    bp->reset();
}

} /* extern "C" */
