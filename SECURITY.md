# Security Policy

## Supported versions

Security fixes are accepted for the latest `0.1.x` MVP line.

## Reporting

Email or open a private report describing:

- LocalFlow version and git SHA (Diagnostics)
- macOS version and architecture
- whether models were installed
- steps to reproduce
- impact (data leak, crash, injection, model bypass)

Do not file public issues for unreleased model checksum bypasses or paste-injection privilege issues.

## Scope

In scope:

- checksum bypass (`MODEL_CHECKSUM_MISMATCH` not enforced)
- unexpected network during recording / STT / LLM / dictionary / personalization / insertion
- execution of generated code
- writes outside `~/Library/Application Support/LocalFlow/`

Out of scope:

- model quality / hallucination
- third-party app paste targets refusing clipboard input
