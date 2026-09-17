use craft_core::Result;

/// Displays an in-place modal dialog box without leaving the alternate screen.
/// Supports scrolling for messages with many lines.
pub fn show_modal_message<S: AsRef<str>>(title: &str, lines: &[S], is_error: bool) -> Result<()> {
    let modal = super::modals::InfoModal::new(title)
        .with_lines(lines.iter().map(|s| s.as_ref().to_string()))
        .is_error(is_error);
    modal.run()?;
    Ok(())
}

/// Renders in-place progress information pinned at top of alternate screen.
pub fn print_in_place_status<S: AsRef<str>>(title: &str, lines: &[S]) -> Result<()> {
    super::modals::render_boxed_status(title, lines)?;
    Ok(())
}
