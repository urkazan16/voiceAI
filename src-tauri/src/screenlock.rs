//! Frontmost session lock. History and hotkeys are denied while locked.
pub fn screen_is_locked() -> bool {
    #[cfg(target_os = "macos")]
    {
        unsafe { lf_screen_is_locked() != 0 }
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(target_os = "macos")]
extern "C" {
    fn lf_screen_is_locked() -> i32;
}

#[cfg(test)]
mod tests {
    #[test]
    fn lock_query_does_not_panic() {
        let _ = super::screen_is_locked();
    }
}
