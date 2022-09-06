# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Additions
- Introduced `CHANGELOG.md`
- Added progress bar indicator to Add to Queue buttons.

### Fixes
- Made backend use (and clean up) temp files in article extraction. Fixes bug where a failed extraction makes retry impossible due to "File already exists" error.
- Improved presentation of errors. Trafilatura errors now say "extraction error" rather than "error parsing trafilatura JSON output"