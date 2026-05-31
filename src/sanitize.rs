//! Input sanitization for user-provided strings.
//! Prevents control character injection, path traversal, and excessive lengths.
//! Called at input boundaries before storage.

/// Sanitize an alarm label: strip control characters, truncate to 100 chars.
pub fn sanitize_label(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(100)
        .collect()
}

/// Sanitize a filename: reject path traversal, null bytes, and excessive length.
/// Returns Ok(sanitized) or Err with reason.
pub fn sanitize_filename(input: &str) -> Result<String, &'static str> {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return Err("Filename cannot be empty");
    }
    if trimmed.len() > 255 {
        return Err("Filename too long (max 255 chars)");
    }
    if trimmed.contains("..") {
        return Err("Filename cannot contain '..'");
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err("Filename cannot contain path separators");
    }
    if trimmed.contains('\0') {
        return Err("Filename cannot contain null bytes");
    }

    Ok(trimmed.to_string())
}

/// Sanitize a challenge reference value (barcode/QR): strip control chars, truncate to 2048.
pub fn sanitize_reference_value(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_control())
        .take(2048)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_strips_control_chars() {
        let input = "Hello\x00World\x01\x02";
        assert_eq!(sanitize_label(input), "HelloWorld");
    }

    #[test]
    fn label_preserves_newline() {
        let input = "Line 1\nLine 2";
        assert_eq!(sanitize_label(input), "Line 1\nLine 2");
    }

    #[test]
    fn label_truncates_at_100() {
        let input = "a".repeat(150);
        assert_eq!(sanitize_label(&input).len(), 100);
    }

    #[test]
    fn label_normal_input_unchanged() {
        let input = "Wake up for gym";
        assert_eq!(sanitize_label(input), "Wake up for gym");
    }

    #[test]
    fn filename_rejects_empty() {
        assert!(sanitize_filename("").is_err());
        assert!(sanitize_filename("   ").is_err());
    }

    #[test]
    fn filename_rejects_path_traversal() {
        assert!(sanitize_filename("../../../etc/passwd").is_err());
        assert!(sanitize_filename("..\\windows\\system32").is_err());
        assert!(sanitize_filename("foo/../bar").is_err());
    }

    #[test]
    fn filename_rejects_separators() {
        assert!(sanitize_filename("path/file.mp3").is_err());
        assert!(sanitize_filename("path\\file.mp3").is_err());
    }

    #[test]
    fn filename_rejects_null_byte() {
        assert!(sanitize_filename("file\x00.mp3").is_err());
    }

    #[test]
    fn filename_rejects_too_long() {
        let long = "a".repeat(256);
        assert!(sanitize_filename(&long).is_err());
    }

    #[test]
    fn filename_accepts_valid() {
        assert_eq!(
            sanitize_filename("my-sound-file.mp3").unwrap(),
            "my-sound-file.mp3"
        );
        assert_eq!(sanitize_filename("  trimmed.ogg  ").unwrap(), "trimmed.ogg");
    }

    #[test]
    fn reference_strips_control_chars() {
        let input = "barcode\x00value\x01here";
        assert_eq!(sanitize_reference_value(input), "barcodevaluehere");
    }

    #[test]
    fn reference_truncates_at_2048() {
        let input = "x".repeat(3000);
        assert_eq!(sanitize_reference_value(&input).len(), 2048);
    }

    #[test]
    fn reference_normal_input_unchanged() {
        let input = "1234567890123";
        assert_eq!(sanitize_reference_value(input), "1234567890123");
    }
}
