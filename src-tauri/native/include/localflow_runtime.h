#ifndef LOCALFLOW_RUNTIME_H
#define LOCALFLOW_RUNTIME_H

#ifdef __cplusplus
extern "C" {
#endif

#define LF_OK 0
#define LF_ERR_NO_MODEL 1
#define LF_ERR_CHECKSUM 2
#define LF_ERR_FORMAT 3
#define LF_ERR_RUNTIME 4
#define LF_ERR_UNSUPPORTED 5
#define LF_ERR_PERMISSION 6

int lf_macos_transcribe(const char *wav_path, char *out, int out_len);
int lf_screen_is_locked(void);

#ifdef __cplusplus
}
#endif

#endif
