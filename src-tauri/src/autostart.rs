use crate::error::LfResult;

pub fn apply(enabled: bool) -> LfResult<()> {
    crate::platform::current().set_autostart(enabled)
}

#[cfg(test)]
mod tests {
    #[test]
    fn disable_is_idempotent() {
        super::apply(false).unwrap();
    }

    #[test]
    fn disable_when_agent_missing_is_silent_ok() {
        super::apply(false).unwrap();
        super::apply(false).unwrap();
    }
}
