# Guitar Datasets

Place downloaded dataset files in this directory.
Tests in `tests/dataset_tests.rs` will skip gracefully if files are absent.

---

## GuitarSet
**URL:** https://guitarset.weebly.com/  
**Paper:** Xi et al., "GuitarSet: A Dataset for Guitar Transcription", ISMIR 2018  
**License:** Creative Commons Attribution 4.0

GuitarSet contains 360 audio recordings of guitar playing with detailed
annotations for pitch, timing, and playing techniques.

### Download steps
```bash
# Install the downloader (Python 3 required)
pip install mirdata

python3 - <<'EOF'
import mirdata
guitarset = mirdata.initialize('guitarset', data_home='tests/dataset/guitarset')
guitarset.download()
EOF
```

Expected layout after download:
```
tests/dataset/guitarset/
  audio/
    mic/
      00_BN1-129-Eb_comp_mic.wav
      …
  annotations/
    …
```

The tests look for WAV files matching `tests/dataset/guitarset/audio/mic/*.wav`.

---

## IDMT-SMT-Guitar
**URL:** https://www.idmt.fraunhofer.de/en/publications/datasets/guitar.html  
**Zenodo:** https://zenodo.org/record/7544110  
**License:** Creative Commons Attribution Non-Commercial 3.0

The IDMT-SMT-Guitar dataset contains electric and acoustic guitar recordings across
four subsets covering isolated note events, licks with playing techniques
(bending, slide, vibrato, harmonics, dead-notes), and chord/rhythm pieces.
All recordings are mono, 44100 Hz RIFF WAV.

**Representative samples** (matching the real dataset naming convention) are
already included in `tests/dataset/idmt_guitar/` and are used by the test suite.
To replace them with the full dataset (~10 GB):

### Download steps
1. Visit https://zenodo.org/record/7544110 and download the ZIP(s).
2. Extract into `tests/dataset/idmt_guitar/`:
   ```bash
   unzip IDMT-SMT-Guitar_V2.zip -d tests/dataset/idmt_guitar
   ```

Expected layout after extraction:
```
tests/dataset/idmt_guitar/
  dataset1/
    G1/
      lick_finger_normal.wav
      lick_pick_bending.wav
      …
    G2/  G3/
  dataset2/
    G1/
      normal/
        G1_normal_E2.wav  G1_normal_F2.wav  …
      muted/  harmonics/
    G2/
  dataset3/  dataset4/
```

The tests find all `.wav` files recursively under `tests/dataset/idmt_guitar/`.

---

## Running the dataset tests

```bash
# Default (no audio_input feature required — uses built-in WAV codec):
cargo test dataset

# With audio_input feature (rodio decoder, supports 24-bit / 32-bit float WAV):
cargo test --features audio_input dataset
```

Tests that find no matching dataset files emit a notice and pass immediately,
so the CI suite never fails due to missing data files.
