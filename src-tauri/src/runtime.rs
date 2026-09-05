use crate::error::{LfError, LfResult};
use std::ffi::CStr;

extern "C" {
    fn lf_runtime_id(buffer: *mut i8, buffer_len: i32) -> i32;
    fn lf_whisper_transcribe(
        model_path: *const i8,
        samples: *const f32,
        sample_count: i32,
        out: *mut i8,
        out_len: i32,
    ) -> i32;
    fn lf_llama_generate(
        model_path: *const i8,
        prompt: *const i8,
        out: *mut i8,
        out_len: i32,
    ) -> i32;
}

pub fn runtime_id() -> String {
    let mut buf = [0_i8; 128];
    let rc = unsafe { lf_runtime_id(buf.as_mut_ptr(), buf.len() as i32) };
    if rc != 0 {
        return "unknown".into();
    }
    unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

pub fn native_transcribe(model_path: &str, samples: &[f32]) -> LfResult<String> {
    call_native(model_path, |c_path, out, out_len| unsafe {
        lf_whisper_transcribe(c_path, samples.as_ptr(), samples.len() as i32, out, out_len)
    })
}

pub fn native_generate(model_path: &str, prompt: &str) -> LfResult<String> {
    let c_prompt =
        std::ffi::CString::new(prompt).map_err(|_| LfError::Other("prompt contains NUL".into()))?;
    call_native(model_path, |c_path, out, out_len| unsafe {
        lf_llama_generate(c_path, c_prompt.as_ptr(), out, out_len)
    })
}

fn call_native(
    model_path: &str,
    func: impl Fn(*const i8, *mut i8, i32) -> i32,
) -> LfResult<String> {
    let c_path = std::ffi::CString::new(model_path)
        .map_err(|_| LfError::Other("model path contains NUL".into()))?;
    let mut out = vec![0_i8; 16 * 1024];
    let rc = func(c_path.as_ptr(), out.as_mut_ptr(), out.len() as i32);
    match rc {
        0 => Ok(unsafe { CStr::from_ptr(out.as_ptr()) }
            .to_string_lossy()
            .into_owned()),
        1 => Err(LfError::ModelMissing(model_path.into())),
        2 => Err(LfError::ModelChecksumMismatch {
            expected: "native".into(),
            actual: "native".into(),
        }),
        3 => Err(LfError::ModelFormatInvalid(model_path.into())),
        5 => Err(LfError::RuntimeUnsupported(
            "native whisper/llama subset is not linked; install verified MIT runtime via scripts/build-native-runtime.sh".into(),
        )),
        _ => Err(LfError::RuntimeUnsupported(format!("native rc={rc}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_runtime_identifies_itself() {
        assert!(runtime_id().contains("localflow-native"));
    }
}
