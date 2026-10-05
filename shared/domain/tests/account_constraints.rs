use brews_domain::accounts::{
    PASSWORD_MAX_LEN, PASSWORD_MIN_LEN, USERNAME_MAX_LEN, USERNAME_MIN_LEN, validate_password,
    validate_username,
};

#[test]
fn named_length_limits_match_the_approved_policy() {
    assert_eq!((PASSWORD_MIN_LEN, PASSWORD_MAX_LEN), (10, 50));
    assert_eq!((USERNAME_MIN_LEN, USERNAME_MAX_LEN), (10, 50));
}

#[test]
fn validators_use_the_declared_length_boundaries() {
    for (minimum, maximum, validate) in [
        (
            PASSWORD_MIN_LEN,
            PASSWORD_MAX_LEN,
            validate_password as fn(&str) -> _,
        ),
        (USERNAME_MIN_LEN, USERNAME_MAX_LEN, |value: &str| {
            validate_username(value).map(|_| ())
        }),
    ] {
        assert!(validate(&"x".repeat(minimum - 1)).is_err());
        assert!(validate(&"x".repeat(minimum)).is_ok());
        assert!(validate(&"x".repeat(maximum)).is_ok());
        assert!(validate(&"x".repeat(maximum + 1)).is_err());
    }
}
