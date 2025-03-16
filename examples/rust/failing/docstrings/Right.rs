/// Escapes `"` or `\` with `\` for use in AppleScript text
fn esc_quote(s: &str) -> Cow<'_, str> {
    if s.contains(&['"', '\\']) {
        let mut r = String::with_capacity(s.len());
        let chars = s.chars();
        for c in chars {
            match c {
                '"' | '\\' => {
                    r.push('\\');
                    r.push(c);
                } // escapes quote/escape char
                _ => {
                    r.push(c);
                } // no escape required
            }
        }
        Cow::Owned(r)
    } else {
        Cow::Borrowed(s)
    }
}
