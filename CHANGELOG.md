# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Fixes
- Fixed bug where a `/` in the article title would cause a file creation error. Triggered by [this](http://strangehorizons.com/non-fiction/writing-realizing-disability-power/) article.

## [0.2.0] - 2022-09-12

### Additions
- Introduced `CHANGELOG.md`
- Added progress bar indicator to Add to Queue buttons.
- Implemented TTS rate limiting on the backend. CLI flag is `--max-chars-per-min`.
- Instituted size limit on title length. 300 UTF-16 code units.

### Fixes
- Made backend use (and clean up) temp files in article extraction. Fixes bug where a failed extraction makes retry impossible due to "File already exists" error.
- Improved presentation of errors. Trafilatura errors now say "extraction error" rather than "error parsing trafilatura JSON output"
