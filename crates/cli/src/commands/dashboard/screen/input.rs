use craft_core::Result;

/// Displays an in-place single-line input prompt with full readline editing support.
pub fn run_input_prompt(
    header_title: &str,
    prompt_label: &str,
    default_val: Option<&str>,
) -> Result<Option<String>> {
    let mut modal = super::modals::InputModal::new(header_title, prompt_label);
    if let Some(def) = default_val {
        modal = modal.with_default(def);
    }
    match modal.run()? {
        super::modals::InputOutcome::Submitted(val) => Ok(Some(val)),
        super::modals::InputOutcome::Cancelled => Ok(None),
    }
}

/// Displays an in-place single-line password input prompt where characters are masked as `*`.
#[allow(dead_code)]
pub fn run_password_prompt(
    header_title: &str,
    prompt_label: &str,
) -> Result<Option<String>> {
    let modal = super::modals::InputModal::new(header_title, prompt_label).with_password(true);
    match modal.run()? {
        super::modals::InputOutcome::Submitted(val) => Ok(Some(val)),
        super::modals::InputOutcome::Cancelled => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_readline_word_deletion_logic() {
        let mut buf = "hello world test".to_string();
        let mut pos = buf.len();
        // simulate ctrl+w
        let before = &buf[..pos];
        let trimmed = before.trim_end();
        let word_start = trimmed.rfind(' ').map(|idx| idx + 1).unwrap_or(0);
        let after = buf[pos..].to_string();
        buf = format!("{}{}", &buf[..word_start], after);
        pos = word_start;

        assert_eq!(buf, "hello world ");
        assert_eq!(pos, 12);
    }
}
