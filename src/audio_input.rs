//! Unified audio input for the guitar pitch detector.
//!
//! Both audio paths go through the **cpal** ecosystem:
//!
//! | Source                         | Backend                                    |
//! |--------------------------------|--------------------------------------------|
//! | WAV / OGG / MP3 / FLAC file    | [`rodio::Decoder`] (symphonia, built on cpal) |
//! | Live USB / mic (Real Tone Cable)| [`cpal`] directly                          |
//!
//! Enable the `audio_input` Cargo feature to compile this module.
//!
//! # Real Tone Cable
//! The Rocksmith Real Tone Cable is a standard USB audio device.  It appears
//! in the device list as a name containing "Rocksmith" or "USB Guitar Adapter".
//! Use [`list_input_devices`] to enumerate devices and [`LiveCapture::open`]
//! with a name fragment to select it.

#[cfg(not(feature = "audio_input"))]
compile_error!(
    "audio_input.rs requires the `audio_input` Cargo feature. \
     Add `features = [\"audio_input\"]` to your dependency declaration."
);

// ── Imports ───────────────────────────────────────────────────────────────────

use rodio::{Decoder, Source};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use dasp_sample::Sample as DaspSample;

use std::collections::VecDeque;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::{Arc, Mutex};

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors that can occur when opening or reading an audio source.
#[derive(Debug)]
pub enum AudioInputError {
    Io(std::io::Error),
    Decode(rodio::decoder::DecoderError),
    Device(String),
}

impl std::fmt::Display for AudioInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioInputError::Io(e) => write!(f, "I/O error: {e}"),
            AudioInputError::Decode(e) => write!(f, "Decode error: {e}"),
            AudioInputError::Device(s) => write!(f, "Device error: {s}"),
        }
    }
}

impl std::error::Error for AudioInputError {}

impl From<std::io::Error> for AudioInputError {
    fn from(e: std::io::Error) -> Self {
        AudioInputError::Io(e)
    }
}

impl From<rodio::decoder::DecoderError> for AudioInputError {
    fn from(e: rodio::decoder::DecoderError) -> Self {
        AudioInputError::Decode(e)
    }
}

// ── FileReader ────────────────────────────────────────────────────────────────

/// Decode an audio file into mono f32 samples using **rodio** (which is built
/// on **cpal**).
///
/// Supported formats (via symphonia): WAV (16-bit, 24-bit, 32-bit float),
/// OGG/Vorbis, MP3, FLAC.  Stereo and multi-channel files are downmixed to
/// mono by averaging channels.
///
/// # Example
/// ```no_run
/// # #[cfg(feature = "audio_input")]
/// # {
/// use guitar_pitch_detection::audio_input::FileReader;
/// use guitar_pitch_detection::GuitarPitchDetector;
///
/// let mut reader = FileReader::open("recording.wav").unwrap();
/// let mut detector = GuitarPitchDetector::new(reader.sample_rate(), 512);
///
/// while let Some(frame) = reader.next_frame(512) {
///     let result = detector.process(&frame);
///     for note in &result.notes {
///         println!("{} {:.1} Hz", note.name, note.frequency);
///     }
/// }
/// # }
/// ```
pub struct FileReader {
    samples: Vec<f32>,
    sample_rate: u32,
    position: usize,
}

impl FileReader {
    /// Open and fully decode an audio file.
    ///
    /// The entire file is decoded into memory upfront so that frames can be
    /// served at the exact rate the detector expects.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AudioInputError> {
        let file = BufReader::new(File::open(path)?);
        let decoder = Decoder::new(file)?;

        let sample_rate = decoder.sample_rate();
        let channels = decoder.channels() as usize;

        // Collect all samples as f32 (rodio normalises to i16 internally;
        // convert_samples() uses dasp's sample conversion under the hood).
        let raw: Vec<f32> = decoder.convert_samples::<f32>().collect();

        // Downmix to mono.
        let samples: Vec<f32> = if channels <= 1 {
            raw
        } else {
            raw.chunks(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                .collect()
        };

        Ok(Self {
            samples,
            sample_rate,
            position: 0,
        })
    }

    /// Sample rate of the decoded audio in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Total number of mono f32 samples in the file.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// `true` when all samples have been consumed.
    pub fn is_empty(&self) -> bool {
        self.position >= self.samples.len()
    }

    /// Borrow all decoded samples as a slice.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// Read the next `frame_size` mono samples, advancing the internal cursor.
    ///
    /// Returns `None` when the file is exhausted.  The last frame may contain
    /// fewer than `frame_size` samples if the file length is not a multiple.
    pub fn next_frame(&mut self, frame_size: usize) -> Option<Vec<f32>> {
        if self.position >= self.samples.len() {
            return None;
        }
        let end = (self.position + frame_size).min(self.samples.len());
        let frame = self.samples[self.position..end].to_vec();
        self.position = end;
        Some(frame)
    }

    /// Reset the cursor to the beginning of the file.
    pub fn rewind(&mut self) {
        self.position = 0;
    }
}

// ── Device listing ────────────────────────────────────────────────────────────

/// Return the names of all audio input devices available on the system.
///
/// The Rocksmith **Real Tone Cable** typically appears as:
/// * Windows: `"Headset Microphone (Rocksmith USB Guitar Adapter)"`
/// * macOS:   `"Rocksmith USB Guitar Adapter"`
/// * Linux:   `"USB Audio Device"` (or similar)
///
/// Use a name fragment with [`LiveCapture::open`] to select it.
pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devs| devs.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

// ── LiveCapture ───────────────────────────────────────────────────────────────

/// Real-time audio capture from a system input device via **cpal**.
///
/// Internally, **cpal** is the same library that **rodio** uses for hardware
/// access, so both [`FileReader`] and `LiveCapture` share the same underlying
/// audio stack.
///
/// The captured samples are stored in a lock-protected ring-buffer (up to
/// 4 seconds) and consumed frame-by-frame through [`read_frame`](Self::read_frame).
///
/// # Real Tone Cable
/// ```no_run
/// # #[cfg(feature = "audio_input")]
/// # {
/// use guitar_pitch_detection::audio_input::{list_input_devices, LiveCapture};
///
/// println!("Available devices: {:?}", list_input_devices());
///
/// // Open the Rocksmith cable (partial name match, case-insensitive).
/// let capture = LiveCapture::open(Some("Rocksmith")).unwrap();
/// # }
/// ```
pub struct LiveCapture {
    _stream: cpal::Stream, // keeps the cpal stream alive
    buffer: Arc<Mutex<VecDeque<f32>>>,
    sample_rate: u32,
}

impl LiveCapture {
    /// Open the system default audio input device.
    pub fn new() -> Result<Self, AudioInputError> {
        Self::open(None)
    }

    /// Open an input device whose name contains `name_fragment` (case-insensitive).
    ///
    /// Pass `None` to use the system default device.
    pub fn open(name_fragment: Option<&str>) -> Result<Self, AudioInputError> {
        let host = cpal::default_host();

        let device = match name_fragment {
            None => host
                .default_input_device()
                .ok_or_else(|| AudioInputError::Device("no default input device".into()))?,
            Some(frag) => {
                let frag_lo = frag.to_lowercase();
                host.input_devices()
                    .map_err(|e| AudioInputError::Device(e.to_string()))?
                    .find(|d| {
                        d.name()
                            .map(|n| n.to_lowercase().contains(&frag_lo))
                            .unwrap_or(false)
                    })
                    .ok_or_else(|| {
                        AudioInputError::Device(format!(
                            "no input device whose name contains '{frag}'"
                        ))
                    })?
            }
        };

        let supported = device
            .default_input_config()
            .map_err(|e| AudioInputError::Device(e.to_string()))?;

        let sample_rate = supported.sample_rate().0;
        let config: cpal::StreamConfig = supported.clone().into();

        let buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let buf_w = Arc::clone(&buffer);
        let max_buf = sample_rate as usize * 4; // 4-second ring-buffer

        let err_fn = |e: cpal::StreamError| eprintln!("[audio_input] stream error: {e}");

        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    push_samples(data, &buf_w, max_buf);
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let cvt: Vec<f32> = data.iter().map(|&s| s.to_sample::<f32>()).collect();
                    push_samples(&cvt, &buf_w, max_buf);
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::I32 => device.build_input_stream(
                &config,
                move |data: &[i32], _: &cpal::InputCallbackInfo| {
                    let cvt: Vec<f32> = data.iter().map(|&s| s.to_sample::<f32>()).collect();
                    push_samples(&cvt, &buf_w, max_buf);
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_input_stream(
                &config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    let cvt: Vec<f32> = data.iter().map(|&s| s.to_sample::<f32>()).collect();
                    push_samples(&cvt, &buf_w, max_buf);
                },
                err_fn,
                None,
            ),
            fmt => {
                return Err(AudioInputError::Device(format!(
                    "unsupported sample format: {fmt:?}"
                )))
            }
        }
        .map_err(|e| AudioInputError::Device(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioInputError::Device(e.to_string()))?;

        Ok(Self {
            _stream: stream,
            buffer,
            sample_rate,
        })
    }

    /// Sample rate of the capture stream in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Read up to `frame_size` samples from the internal ring-buffer.
    ///
    /// Returns a `Vec<f32>` of exactly `frame_size` samples.  If fewer have
    /// arrived since the last call, the returned frame is zero-padded.
    pub fn read_frame(&self, frame_size: usize) -> Vec<f32> {
        let mut guard = self.buffer.lock().unwrap();
        let available = guard.len().min(frame_size);
        let mut frame: Vec<f32> = guard.drain(..available).collect();
        frame.resize(frame_size, 0.0);
        frame
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn push_samples(data: &[f32], buffer: &Arc<Mutex<VecDeque<f32>>>, max: usize) {
    let mut guard = buffer.lock().unwrap();
    for &s in data {
        guard.push_back(s);
    }
    // Drop oldest samples when the consumer is slower than the producer.
    while guard.len() > max {
        guard.pop_front();
    }
}
