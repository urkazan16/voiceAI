//! Identifies the shipped inference stack. STT is whisper-rs; there is no C stub runtime.

use crate::error::{LfError, LfResult};

pub fn runtime_id() -> String {
    format!("whisper-rs/{}", whisper_rs_version())
}

fn whisper_rs_version() -> &'static str {
    "0.13.2"
}

pub fn native_generate(_model_path: &str, _prompt: &str) -> LfResult<String> {
    Err(LfError::RuntimeUnsupported(
        "llama.cpp is not linked; professional/code modes use on-device formatting".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_id_is_whisper_rs_not_a_stub() {
        let id = runtime_id();
        assert!(id.starts_with("whisper-rs/"), "{id}");
        assert!(!id.contains("stub"), "{id}");
        assert!(!id.contains("localflow-native"), "{id}");
    }

    #[test]
    fn llama_generate_is_unsupported_without_c_ffi() {
        let err = native_generate("/tmp/model.gguf", "hello").unwrap_err();
        assert_eq!(err.code(), "RUNTIME_UNSUPPORTED");
        assert!(!err.to_string().contains("build-native-runtime"));
    }
}
