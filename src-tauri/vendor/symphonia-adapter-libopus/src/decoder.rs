// Derived from symphonia-adapter-libopus 0.3.0 under Apache-2.0.
use std::ffi::c_int;

use log::{error, warn};
use symphonia_core::errors::{Error, Result};

fn error_code_to_str(code: c_int) -> &'static str {
    match code {
        opusic_sys::OPUS_BAD_ARG => "One or more invalid/out of range arguments.",
        opusic_sys::OPUS_BUFFER_TOO_SMALL => "The output buffer is too small.",
        opusic_sys::OPUS_INTERNAL_ERROR => "An internal error was detected.",
        opusic_sys::OPUS_INVALID_PACKET => "The compressed data is corrupted.",
        opusic_sys::OPUS_UNIMPLEMENTED => "The request is unsupported.",
        opusic_sys::OPUS_INVALID_STATE => "The decoder state is invalid.",
        opusic_sys::OPUS_ALLOC_FAIL => "Memory allocation failed.",
        _ => "unknown libopus error",
    }
}

#[derive(Debug)]
pub(crate) struct Decoder {
    ptr: *mut opusic_sys::OpusDecoder,
    channels: u32,
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: libopus returned this pointer from opus_decoder_create and this
        // instance owns it until drop.
        unsafe { opusic_sys::opus_decoder_destroy(self.ptr) };
    }
}

// libopus decoder instances are not used concurrently; Symphonia moves the
// decoder between worker contexts behind exclusive mutable access.
unsafe impl Send for Decoder {}
unsafe impl Sync for Decoder {}

impl Decoder {
    pub(crate) fn new(sample_rate: u32, channels: u32) -> Result<Self> {
        let mut error = 0;
        // SAFETY: arguments are validated by the caller and libopus writes only
        // the error integer supplied here.
        let ptr = unsafe {
            opusic_sys::opus_decoder_create(sample_rate as i32, channels as c_int, &mut error)
        };
        if error != opusic_sys::OPUS_OK || ptr.is_null() {
            error!("decoder creation failed with {error}: {}", error_code_to_str(error));
            return Err(Error::DecodeError("opus: error creating decoder"));
        }
        Ok(Self { ptr, channels })
    }

    pub(crate) fn decode(&mut self, input: &[u8], output: &mut [f32]) -> Result<usize> {
        let ptr = if input.is_empty() { std::ptr::null() } else { input.as_ptr() };
        // SAFETY: output is writable for its full length and libopus is given
        // the per-channel capacity. The decoder pointer is exclusively held.
        let decoded = unsafe {
            opusic_sys::opus_decode_float(
                self.ptr,
                ptr,
                checked_len(input)?,
                output.as_mut_ptr(),
                checked_len(output)? / self.channels as c_int,
                0,
            )
        };
        if decoded < 0 {
            warn!("decode failed with {decoded}: {}", error_code_to_str(decoded));
            return Err(Error::DecodeError("opus: decode failed"));
        }
        Ok(decoded as usize)
    }

    pub(crate) fn reset(&mut self) {
        // SAFETY: the pointer is a live decoder owned by this instance.
        let result = unsafe { opusic_sys::opus_decoder_ctl(self.ptr, opusic_sys::OPUS_RESET_STATE) };
        if result != opusic_sys::OPUS_OK {
            warn!("reset failed with {result}: {}", error_code_to_str(result));
        }
    }
}

fn checked_len<T>(slice: &[T]) -> Result<c_int> {
    c_int::try_from(slice.len()).map_err(|_| {
        error!("buffer length out of range: {}", slice.len());
        Error::DecodeError("opus: buffer length out of range")
    })
}
