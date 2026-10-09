# License exceptions (manual review)

| Component                                  | License                   | Decision                                                                                                                                       |
| ------------------------------------------ | ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| unicode-ident and other `unicode-*` crates | Unicode-3.0               | Allowed as data-file license                                                                                                                   |
| bytemuck                                   | Zlib OR Apache-2.0 OR MIT | Allowed                                                                                                                                        |
| ring                                       | MIT AND ISC AND OpenSSL   | Allowed; OpenSSL is permissive                                                                                                                 |
| cssparser / Servo HTML stack used by Tauri | MPL-2.0                   | Allowed: file-level weak copyleft. Modified MPL files must remain MPL.                                                                         |
| Symphonia 0.6.1 crates                     | MPL-2.0                   | Pending release review for the optional `audio-symphonia-opus` prototype; the Servo/cssparser decision does not cover this separate component. |
| webpki-roots                               | CDLA-Permissive-2.0       | Allowed: permissive data license for TLS roots                                                                                                 |
