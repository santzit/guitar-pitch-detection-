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
**License:** Creative Commons Attribution Non-Commercial 3.0

The IDMT-SMT-Guitar dataset contains isolated guitar notes recorded across
several electric and acoustic guitars at various playing styles.

### Download steps
Register on the Fraunhofer IDMT website and download the ZIP archive, then:
```bash
unzip IDMT-SMT-Guitar_V2.zip -d tests/dataset/idmt_guitar
```

Expected layout:
```
tests/dataset/idmt_guitar/
  dataset1/
    acoustic_mic/
      *.wav
  dataset2/
    …
```

The tests look for WAV files matching `tests/dataset/idmt_guitar/**/*.wav`.

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
