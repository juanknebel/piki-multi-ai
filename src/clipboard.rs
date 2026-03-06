use arboard::Clipboard;

pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    Clipboard::new()?.set_text(text)?;
    Ok(())
}

pub fn paste_from_clipboard() -> anyhow::Result<String> {
    Ok(Clipboard::new()?.get_text()?)
}
