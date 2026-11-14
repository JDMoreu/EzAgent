//! Small helpers of the swarm service.

pub(super) fn known_command(op: &str) -> bool {
    matches!(
        op,
        "next"
            | "done"
            | "fail"
            | "fix"
            | "add"
            | "say"
            | "ask"
            | "answer"
            | "inbox"
            | "lock"
            | "unlock"
            | "board"
            | "me"
            | "help"
            | "wait"
    )
}

/// URL-safe base64 without padding (`secrets.token_urlsafe`).
pub(super) fn base64_url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let first = u32::from(chunk[0]);
        let second = chunk.get(1).copied().map_or(0, u32::from);
        let third = chunk.get(2).copied().map_or(0, u32::from);
        let value = (first << 16) | (second << 8) | third;
        output.push(ALPHABET[((value >> 18) & 63) as usize] as char);
        output.push(ALPHABET[((value >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[((value >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[(value & 63) as usize] as char);
        }
    }
    output
}
