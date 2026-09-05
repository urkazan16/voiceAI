#!/usr/bin/env bash
# Block 0 gate: environment, CLI, optional TextEdit paste, optional say→Whisper WER.
# Never pastes into the frontmost app unless it is TextEdit.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
RUN=(bash scripts/run-with-toolchain.sh cargo run --manifest-path src-tauri/Cargo.toml --quiet --)
OUT="${BLOCK0_OUT:-$ROOT/docs/evaluation/BLOCK0.md}"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/lf-block0.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

lf() { "${RUN[@]}" "$@"; }

wer() {
  python3 - "$@" <<'PY'
import sys
ref = sys.argv[1].split()
hyp = sys.argv[2].split()
if not ref:
    print("1.0" if hyp else "0.0")
    raise SystemExit
prev = list(range(len(hyp) + 1))
for i, w in enumerate(ref, 1):
    cur = [i]
    for j, x in enumerate(hyp, 1):
        cur.append(min(prev[j] + 1, cur[-1] + 1, prev[j - 1] + (w != x)))
    prev = cur
print(f"{prev[-1] / len(ref):.4f}")
PY
}

{
  echo "# Block 0 measurement"
  echo
  echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo
  echo "## A-17 environment"
  echo '```'
  sw_vers 2>/dev/null || true
  echo "uname: $(uname -m) $(uname -s) $(uname -r)"
  echo '```'
  echo
  echo "## CLI check (T-16, B-13, C-06, AG-07 query)"
  echo '```'
  lf check || true
  echo '```'
  echo
  echo "## Devices (C-01)"
  echo '```'
  lf devices || echo "devices: FAIL"
  echo '```'
  echo
  echo "## K paste smoke (ClipboardInjector → TextEdit only)"
  if [[ "$(uname -s)" == Darwin ]]; then
    set +e
    python3 - <<'PY'
import subprocess, sys
try:
    subprocess.run(
        ["osascript", "-e", 'tell application "TextEdit" to get name'],
        timeout=6,
        check=False,
        capture_output=True,
    )
except subprocess.TimeoutExpired:
    print("- **SKIP** TextEdit did not respond in 6s (grant Automation to the terminal / Cursor)")
    sys.exit(0)
open_doc = """
tell application "TextEdit"
  activate
  make new document with properties {text:""}
end tell
"""
try:
    subprocess.run(["osascript", "-e", open_doc], timeout=8, check=True, capture_output=True)
except Exception as exc:
    print(f"- **SKIP** could not open TextEdit document: {exc}")
    sys.exit(0)
print("- TextEdit document opened")
sys.exit(20)
PY
    st=$?
    set -e
    if [[ "$st" == "20" ]]; then
      TOKEN="$(lf paste-smoke 2>/dev/null | tail -n 1 || true)"
      GOT="$(python3 - <<'PY'
import subprocess
try:
    r = subprocess.run(
        ["osascript", "-e", 'tell application "TextEdit" to get text of document 1'],
        timeout=6, capture_output=True, text=True)
    print((r.stdout or "").strip())
except Exception:
    print("")
PY
)"
      python3 - <<'PY' >/dev/null 2>&1 || true
import subprocess
try:
    subprocess.run(
        ["osascript", "-e", 'tell application "TextEdit" to close document 1 saving no'],
        timeout=6, capture_output=True)
except Exception:
    pass
PY
      echo "- token: \`$TOKEN\`"
      echo "- document: \`$GOT\`"
      if [[ -n "$TOKEN" && "$GOT" == *"$TOKEN"* ]]; then
        echo "- K-16/K-17 (TextEdit via LocalFlow injector): **PASS**"
      else
        echo "- K-16/K-17 (TextEdit): **FAIL or SKIP** (token not in document; Accessibility?)"
      fi
    fi
    echo "- K-21 2000-char paste into Telegram/Safari/Cursor: **not run** (would steal focus)"
  else
    echo "- skipped (not macOS)"
  fi
  echo
  echo "## P-01 / P-02 network during check"
  BEFORE="$(lsof -nP -iTCP -sTCP:ESTABLISHED 2>/dev/null | awk 'NR>1 && $9 !~ /127.0.0.1/ {c++} END {print c+0}')"
  lf check >/dev/null || true
  AFTER="$(lsof -nP -iTCP -sTCP:ESTABLISHED 2>/dev/null | awk 'NR>1 && $9 !~ /127.0.0.1/ {c++} END {print c+0}')"
  echo "- established TCP (non-loopback) before check: $BEFORE"
  echo "- after check: $AFTER"
  echo "- P-01 (checker): no download; not a full record→insert airplane-mode run"
  echo "- P-02: socket count is not byte volume; packet capture still **human**"
  echo
  echo "## F-01 say → transcribe (needs Whisper on disk + Russian voice)"
  WHISPER_LINE="$(lf check 2>/dev/null | awk -F': ' '/^whisper_ready:/{print $2}' || true)"
  VOICE="$(say -v '?' 2>/dev/null | awk 'tolower($0) ~ /ru_/ {print $1; exit}')"
  if [[ "$WHISPER_LINE" == "no" || -z "$WHISPER_LINE" ]]; then
    echo "- **SKIP** Whisper model not installed (download in Model Manager)"
  elif [[ -z "$VOICE" ]]; then
    echo "- **SKIP** no Russian \`say\` voice (install in System Settings → Accessibility → Spoken Content)"
  else
    echo "- voice: $VOICE  model: $WHISPER_LINE"
    PHRASES=(
      "Привет это проверка диктовки"
      "Открой файл настроек"
      "Собери проект без сети"
      "Вставь текст в заметки"
      "Один два три четыре пять"
    )
    echo
    echo "| Reference | Hypothesis | WER |"
    echo "| --- | --- | --- |"
    SUM=0
    N=0
    for phrase in "${PHRASES[@]}"; do
      AIFF="$TMP/p.aiff"
      WAV="$TMP/p.wav"
      say -v "$VOICE" -o "$AIFF" "$phrase" || true
      if [[ -f "$AIFF" ]]; then
        afconvert -f WAVE -d LEI16@16000 "$AIFF" "$WAV" 2>/dev/null || cp "$AIFF" "$WAV"
      fi
      python3 - "$WAV" <<'PY' || true
import sys, wave, os
path = sys.argv[1]
if not os.path.isfile(path):
    raise SystemExit(1)
with wave.open(path, "rb") as src:
    params = src.getparams()
    data = src.readframes(src.getnframes())
    rate = src.getframerate()
    width = src.getsampwidth()
    ch = src.getnchannels()
need = int(rate * 1.25)
have = len(data) // (width * ch)
if have < need:
    data = data + (b"\x00" * ((need - have) * width * ch))
    with wave.open(path, "wb") as dst:
        dst.setparams(params)
        dst.writeframes(data)
PY
      if [[ ! -f "$WAV" ]]; then
        echo "| $phrase | _no audio_ | 1.0000 |"
        continue
      fi
      HYP="$(lf transcribe --no-postprocess --language ru "$WAV" 2>/tmp/lf-block0-whisper.log | tail -n 1 || true)"
      W="$(wer "$phrase" "$HYP")"
      echo "| $phrase | $HYP | $W |"
      SUM="$(python3 -c "print($SUM + $W)")"
      N=$((N + 1))
    done
    AVG="$(python3 -c "print(round($SUM / $N, 4) if $N else 1)")"
    echo
    echo "- mean WER on ${N} TTS phrases: **$AVG** (TTS≠mic; F-01 still needs live speech)"
  fi
  echo
  echo "## Not run here (need a person at the Mac)"
  echo
  echo "| ID | Why |"
  echo "| --- | --- |"
  echo "| D-07 | fullscreen of another app |"
  echo "| D-08 | global hotkey while LocalFlow unfocused — start \`npm run tauri dev\` |"
  echo "| K-16…K-21 | Telegram, Safari, Cursor |"
  echo "| AM-05 | tail of a live utterance |"
  echo "| AM-12 | Bluetooth headset |"
  echo "| P-01/P-02 full | airplane mode + packet capture of record→insert |"
  echo "| F-01 live | microphone, not \`say\` |"
  echo "| A-17 CI | this host is recorded above; GitHub Actions is still macos-13 |"
} >"$OUT"

echo "Wrote $OUT"
cat "$OUT"
