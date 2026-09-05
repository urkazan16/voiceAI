# Privacy

Core pipeline is local:

- Audio
- STT
- LLM
- Dictionary
- Personalization
- History

Network is allowed only for user-initiated model download and optional app update. Both are labeled in the UI.

User data boundary: `~/Library/Application Support/LocalFlow/` except OS temp files.

Delete History removes SQLite history rows. Reset Personalization clears correction events, learned candidates, and accepted inferred preferences.
