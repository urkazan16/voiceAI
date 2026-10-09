# File transcription cancellation contract

Decision date: 2026-10-09  
Status: accepted scope for the Ogg/Opus backend

## Contract

The one-second cancellation requirement applies to:

- audio validation, Opus packet decoding, downmixing and resampling;
- queued and running Whisper file recognition.

Both paths receive the file task's independent atomic token. Whisper also
passes it to the native abort callback. Cancellation must return
`AUDIO_CANCELLED` and must not cancel dictation or another file task.

For T-One file recognition, GigaAM and Parakeet, cancellation is
**result-suppression**, not a sub-second native abort guarantee. The request is
stored immediately and checked before and after inference. No transcript is
accepted after cancellation, but the dedicated engine worker may remain busy
until its current synchronous sherpa-onnx call returns.

Live T-One dictation remains more responsive because it checks its session
token between audio chunks and decode steps. That does not change the narrower
file-import guarantee above.

## Rationale and future change

The currently used sherpa-onnx offline API exposes a synchronous `decode` call
and no safe abort callback. Killing its thread would risk native state and is
not an acceptable cancellation mechanism. T-One's file path similarly enters
synchronous decode work; checks between calls cannot establish a hard bound on
the duration of one native call.

If a future runtime exposes a documented interruptible API, LocalFlow may move
an engine to `CooperativeAbort` after adding a test that requests cancellation
during real inference and observes completion within one second. Until then,
new native engines default to `ResultSuppression`.
