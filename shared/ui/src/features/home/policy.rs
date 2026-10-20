//! Bounded, allocation-conscious code-entry transforms.
use brews_domain::games::GameCode;

pub(super) const CODE_LENGTH: usize = 8;

pub(super) fn normalize_code(value: &str) -> Result<String, &'static str> {
    GameCode::from_lookup(value)
        .map(|code| code.as_str().to_owned())
        .map_err(|_| "Enter all eight letters and numbers. Spaces are only allowed around a pasted code.")
}

pub(super) fn insert_code(
    current: &[String; CODE_LENGTH],
    index: usize,
    value: &str,
) -> Result<([String; CODE_LENGTH], usize), &'static str> {
    let value = value.trim_matches(' ');
    if index >= CODE_LENGTH || !value.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err("Use letters A–Z and numbers 0–9 only.");
    }
    let start = if value.len() == CODE_LENGTH { 0 } else { index };
    if value.len() > CODE_LENGTH - start {
        return Err("That code is too long. Paste exactly eight letters and numbers.");
    }
    let mut next = current.clone();
    if value.is_empty() {
        next[index].clear();
        return Ok((next, index));
    }
    for (offset, byte) in value.bytes().enumerate() {
        next[start + offset] = char::from(byte.to_ascii_uppercase()).to_string();
    }
    Ok((next, (start + value.len()).min(CODE_LENGTH - 1)))
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "Fixed test fixtures.")]
mod tests {
    use super::*;

    #[test]
    fn normalization_matches_backend_without_internal_space_or_unicode_folding() {
        assert_eq!(normalize_code("  ab12cd34  "), Ok("AB12CD34".into()));
        for invalid in [
            "AB12 CD3",
            "AB12CD345",
            "ＡB12CD34",
            "\tAB12CD34",
            "AB12CD3",
        ] {
            assert!(normalize_code(invalid).is_err());
        }
    }

    #[test]
    fn full_paste_replaces_all_boxes_and_overflow_never_truncates() {
        let empty = std::array::from_fn(|_| String::new());
        let (filled, focus) = insert_code(&empty, 6, " ab12cd34 ").expect("valid code");
        assert_eq!(filled.concat(), "AB12CD34");
        assert_eq!(focus, 7);
        assert!(insert_code(&filled, 7, "xy").is_err());
        assert!(insert_code(&filled, 0, "AB12CD345").is_err());
        assert_eq!(filled.concat(), "AB12CD34");
    }

    #[test]
    fn individual_edits_and_deletion_preserve_other_boxes() {
        let empty = std::array::from_fn(|_| String::new());
        let (filled, _) = insert_code(&empty, 2, "a").expect("valid edit");
        assert_eq!(filled[2], "A");
        let (cleared, focus) = insert_code(&filled, 2, "").expect("delete");
        assert_eq!(cleared, empty);
        assert_eq!(focus, 2);
        assert!(insert_code(&empty, 0, "é").is_err());
    }
}
