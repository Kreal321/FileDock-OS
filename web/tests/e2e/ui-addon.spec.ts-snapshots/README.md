# Canonical Linux Phase14 baselines

Reviewed Ubuntu24.04 / Playwright1.64.0 / Chromium156.0.8078.4 revision1248 / local Inter4.1.
Pinned image digest and the image-by-image review/checksums are recorded in
`specs/001-core-os-foundation/assets/phase14-linux-review.md`.
Use default browser mode on Linux to compare; `FD_UI_CAPTURE_ONLY=1` produces supplemental
captures and does not establish or pass golden comparison. Do not rename Darwin images to Linux.
