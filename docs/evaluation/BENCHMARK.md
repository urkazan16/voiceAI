# Benchmark report

Numbers below are the **report format**. They are not measured results.

```text
Hardware: (fill)
OS: macOS
Build: (git SHA)
Whisper model: Whisper Small
LLM model: Qwen3-4B-Instruct-2507
Quantization: GGUF Q4_K_M
Corpus version: tests/corpus 0.1.0

STT results: (WER, RTF, durations 5/10/30/60s)
LLM results: (TTFT, tok/s, memory peak)
E2E latency: P50/P95/P99 per mode
Reliability: n/100 successful cycles
```

Example layout (not a guarantee):

```text
Audio: 10 sec
STT: 4.2 sec
LLM: 2.1 sec
Total: 6.6 sec
RTF: 0.42
Success: 99/100
```

Latency targets: hotkey P95 ≤ 150 ms, recording start P95 ≤ 300 ms, insertion P95 ≤ 200 ms.
