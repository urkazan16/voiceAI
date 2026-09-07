//! Frontmost session lock. History and hotkeys are denied while locked.
pub fn screen_is_locked() -> bool {
    crate::platform::current().screen_is_locked()
}

#[cfg(test)]
mod tests {
    #[test]
    fn lock_query_does_not_panic() {
        let _ = super::screen_is_locked();
    }
}
