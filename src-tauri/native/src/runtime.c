#include "localflow_runtime.h"

#include <stdio.h>
#include <string.h>

int lf_runtime_id(char *buffer, int buffer_len) {
    const char *id = "localflow-native-stub/0.1.0";
    if (buffer == NULL || buffer_len <= 0) {
        return LF_ERR_RUNTIME;
    }
    snprintf(buffer, (size_t)buffer_len, "%s", id);
    return LF_OK;
}

int lf_whisper_transcribe(const char *model_path, const float *samples, int sample_count, char *out, int out_len) {
    (void)samples;
    (void)sample_count;
    if (model_path == NULL || model_path[0] == '\0') {
        return LF_ERR_NO_MODEL;
    }
    if (out == NULL || out_len <= 0) {
        return LF_ERR_RUNTIME;
    }
    /* Full whisper.cpp is linked only from the verified MIT subset in third_party/.
       The stub keeps the app self-contained and license-clean until that subset is built. */
    snprintf(out, (size_t)out_len, "");
    return LF_ERR_UNSUPPORTED;
}

int lf_llama_generate(const char *model_path, const char *prompt, char *out, int out_len) {
    (void)prompt;
    if (model_path == NULL || model_path[0] == '\0') {
        return LF_ERR_NO_MODEL;
    }
    if (out == NULL || out_len <= 0) {
        return LF_ERR_RUNTIME;
    }
    snprintf(out, (size_t)out_len, "");
    return LF_ERR_UNSUPPORTED;
}
