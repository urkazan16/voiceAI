# Utterance journal

Each completed replica appends one UTF-8 JSON object followed by a newline to:

`~/Library/Application Support/LocalFlow/logs/utterances.jsonl`

(or `$LOCALFLOW_DATA_DIR/logs/utterances.jsonl`).

## Schema (version 1)

| Field                | Type    | Meaning                                        |
| -------------------- | ------- | ---------------------------------------------- |
| `schema`             | number  | Always `1`                                     |
| `id`                 | string  | History row id                                 |
| `ts`                 | string  | RFC 3339 UTC timestamp                         |
| `timezone`           | string  | Numeric offset of the local zone, e.g. `+0300` |
| `text`               | string  | Inserted / final text                          |
| `raw`                | string  | STT text before formatting                     |
| `application`        | string  | Frontmost app at capture                       |
| `profile`            | string  | Resolved profile name                          |
| `mode`               | string  | Pipeline mode                                  |
| `model`              | string  | Active STT model id                            |
| `processing_time_ms` | number  | Pipeline wall time                             |
| `duration_ms`        | number  | Audio length at 16 kHz                         |
| `word_count`         | number  | Whitespace-separated words in `text`           |
| `wpm`                | number  | `word_count * 60000 / duration_ms`             |
| `insert_method`      | string  | `clipboard` or `none`                          |
| `insert_ok`          | boolean | Insert succeeded                               |

History-off (`history_enabled: false`) skips both SQLite history and this journal.
