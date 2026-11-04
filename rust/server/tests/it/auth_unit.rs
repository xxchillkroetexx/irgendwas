//! auth slice: pure unit tests (`safe_next`, error-to-key mapping). No database, no HTTP.

use app::auth_api::{AuthError, FieldError};
use app::paths::{safe_next, safe_return_path};

// ---------------------------------------------------------------------- AC7

#[test]
fn ac7_safe_next_rejects_open_redirects_and_auth_loops() {
    let long = format!("/{}", "a".repeat(599));
    let rejected = [
        "",
        "groups",
        "//evil.example/x",
        "/\\evil.example",
        "https://evil.example",
        "http://evil.example/x",
        "javascript:alert(1)",
        "/\r\nSet-Cookie:x",
        "/a\nb",
        "/a\tb",
        "/a b",
        "/a\\b",
        "/api",
        "/api/login",
        "/auth",
        "/auth/",
        "/auth/login",
        "/auth/register/confirm/abc",
        "/auth/reset-password/abc",
        long.as_str(),
    ];
    for raw in rejected {
        assert_eq!(safe_next(raw), None, "{raw:?} must be rejected");
    }
}

#[test]
fn ac7_safe_next_returns_exactly_the_validated_string() {
    let accepted = [
        "/",
        "/user/dashboard",
        "/user/dashboard?x=1",
        "/groups/3?tab=wishlist",
        "/join/ABCDEFGH",
        "/authors",
        "/apiary",
        "/a%2Fb",
    ];
    for raw in accepted {
        assert_eq!(safe_next(raw).as_deref(), Some(raw), "{raw:?}");
    }
    let max = format!("/{}", "a".repeat(511));
    assert_eq!(max.len(), 512);
    assert_eq!(safe_next(&max).as_deref(), Some(max.as_str()));
}

#[test]
fn safe_next_is_safe_return_path_minus_the_auth_pages() {
    let samples = [
        "/",
        "/x",
        "/auth",
        "/auth/x",
        "/authx",
        "/api",
        "/api/x",
        "//x",
        "/\\x",
        "/ x",
        "x",
        "",
        "/groups",
        "/groups?a=b&c=d",
        "/a\u{7f}b",
        "/é",
        "/%0d%0a",
    ];
    for raw in samples {
        let expected = safe_return_path(raw).filter(|p| p != "/auth" && !p.starts_with("/auth/"));
        assert_eq!(safe_next(raw), expected, "{raw:?}");
    }
}

// --------------------------------------------------------------- error keys

#[test]
fn error_variants_map_to_the_keys_of_the_spec_table() {
    let table: [(AuthError, &str); 8] = [
        (
            AuthError::MissingCredentials,
            "flash.error.email_password_required",
        ),
        (
            AuthError::InvalidCredentials,
            "flash.error.invalid_credentials",
        ),
        (
            AuthError::TooManyAttempts { minutes: 3 },
            "flash.error.too_many_attempts",
        ),
        (AuthError::TooManyRequests, "flash.error.too_many_requests"),
        (
            AuthError::InvalidRegistrationToken,
            "flash.error.registration_link_invalid",
        ),
        (
            AuthError::InvalidResetToken,
            "flash.error.reset_token_expired",
        ),
        (AuthError::Unauthorized, "auth.login.required"),
        // AC45: no internal detail ever reaches the client, the UI shows the generic text.
        (AuthError::Internal, "error.generic.message"),
    ];
    for (error, key) in table {
        assert_eq!(error.message_key(), key, "{error:?}");
    }
}

#[test]
fn field_errors_map_to_the_keys_of_the_spec_table() {
    let table = [
        (FieldError::EmailRequired, "validation.email_required"),
        (FieldError::EmailInvalid, "validation.email_invalid"),
        (FieldError::NameRequired, "validation.name_required"),
        (FieldError::NameTooLong, "validation.name_too_long"),
        (FieldError::NameInvalid, "validation.name_invalid"),
        (
            FieldError::PasswordTooShort,
            "flash.error.password_too_short",
        ),
        (FieldError::PasswordTooLong, "validation.password_too_long"),
        (
            FieldError::PasswordsDontMatch,
            "flash.error.passwords_dont_match",
        ),
    ];
    for (error, key) in table {
        assert_eq!(error.message_key(), key, "{error:?}");
    }
}

#[test]
fn auth_error_survives_the_wire_format() {
    let samples = [
        AuthError::Internal,
        AuthError::TooManyAttempts { minutes: 7 },
        AuthError::Validation(vec![(app::auth_api::Field::Name, FieldError::NameInvalid)]),
    ];
    for error in samples {
        let json = serde_json::to_string(&error).unwrap();
        assert_eq!(serde_json::from_str::<AuthError>(&json).unwrap(), error);
    }
}
