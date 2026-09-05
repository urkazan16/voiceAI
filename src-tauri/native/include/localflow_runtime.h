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

int lf_runtime_id(char *buffer, int buffer_len);
int lf_whisper_transcribe(const char *model_path, const float *samples, int sample_count, char *out, int out_len);
int lf_llama_generate(const char *model_path, const char *prompt, char *out, int out_len);

#ifdef __cplusplus
}
#endif

#endif
