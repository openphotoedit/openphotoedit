# PSD corpus

Real Photoshop (and some GIMP, Clip Studio, SAI and ag-psd written) files used by
`crates/editor-psd/tests/corpus.rs`. 269 files, about 40 MB.

| Folder | Source | Licence | Snapshot |
|---|---|---|---|
| `psd-tools/` | [psd-tools](https://github.com/psd-tools/psd-tools) `tests/psd_files/` — the root files, `layers/`, `layers-minimal/`, `blend-modes/` (without the CMYK and gray sheets), `adjustments/` (RGB cases and `adjustment_*`), `colormodes/`, `transparency/`, `effects/`, `masks/2.psd`, `group-clipping/group-clipping.psd`, `issues/`, `gradients/noise-gradient-rgb.psd`, `colorprofiles/`, and a dozen `.psb` twins | MIT, © 2019 Kota Yamaguchi (repository licence) | commit `5d2c957c85ba0d9c76b67ab2cf84d02f50f81fe7` |
| `ag-psd/` | [ag-psd](https://github.com/Agamnentzar/ag-psd) `test/read/<case>/src.psd`, renamed to `<case>.psd` | MIT, © 2016 Agamnentzar (repository licence) | commit `387049670cb89b88fb8fe1b7c01aeacf98dd2e3b` |
| `oracle/` | Generated here by `crates/editor-psd/scripts/oracle.py` (psd-tools 1.19.0) | Same as this repository | — |

Notes on provenance:

- The files are distributed in those repositories as test fixtures under the repository licences above; neither
  repository states a separate origin for individual files. Some ag-psd cases came from issue reports.
- Files whose pictures looked like third-party work were left out on purpose: three small pixel-art files resembling
  textures from a commercial game (`psd-tools/third-party-psds/cactus_top.psd`, `ag-psd/lantern.psd`, `ag-psd/cat.psd`),
  a signed illustration (`ag-psd/smart-object.psd`) and a stock-looking wildlife photograph (`ag-psd/16bits.psd`).
- The remaining ag-psd drawings (for example `32bits.psd`, `smart-object-png.psd`, `nested.psd`) look like the ag-psd
  author's own test art; that is an inference, not something the repository states.
- Several psd-tools fixtures contain photographs or clip-art of unstated origin (the nebula in `adjustments/*`, the
  rubber duck in `blend-modes/*`, the rabbit in `background-red-opacity-80.psd` and `mask-index.psd`, the flowers in
  `fill_adjustments.psd` and `patterns.psd`). They are kept as the psd-tools project distributes them, for testing only;
  replace them if that is not acceptable for your use.

## Regenerating the oracle

```sh
python3 -m venv target/psd/venv && target/psd/venv/bin/pip install psd-tools
target/psd/venv/bin/python crates/editor-psd/scripts/oracle.py --corpus testdata/psd testdata/psd/oracle --force
```

## The fidelity report

```sh
PSD_FIDELITY_REPORT=1 CARGO_TARGET_DIR=target/psd cargo test -p editor-psd --test corpus -- --nocapture
```

rewrites [`FIDELITY.md`](FIDELITY.md). Without the variable the test still runs every check and writes the report to
`target/psd/roundtrip/FIDELITY.md`.
