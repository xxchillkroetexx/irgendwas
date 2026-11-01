//! Input normalisation and validation shared by auth and account (spec: auth, "Common input
//! normalisation"). Pure functions, usable on server and client.

use crate::auth_api::{Field, FieldError, NAME_MAX, PASSWORD_MAX, PASSWORD_MIN};

/// Longest accepted email address, in characters.
pub const EMAIL_MAX: usize = 255;

/// Trimmed and lowercased: the only form in which emails are stored and compared.
pub fn normalize_email(raw: &str) -> String {
    raw.trim().to_lowercase()
}

/// Checks an already [normalised](normalize_email) email: at most 255 characters, exactly
/// one `@`, non-empty local part and domain, a `.` in the domain but not at either end, no
/// whitespace or control characters.
pub fn validate_email(email: &str) -> Result<(), FieldError> {
    if email.is_empty() {
        return Err(FieldError::EmailRequired);
    }
    if email.chars().count() > EMAIL_MAX
        || email.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(FieldError::EmailInvalid);
    }
    let mut parts = email.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(FieldError::EmailInvalid);
    };
    if local.is_empty()
        || domain.is_empty()
        || !domain.contains('.')
        || domain.starts_with('.')
        || domain.ends_with('.')
    {
        return Err(FieldError::EmailInvalid);
    }
    Ok(())
}

/// Trims `raw` and checks it: 1..=100 characters, no control characters.
pub fn validate_name(raw: &str) -> Result<String, FieldError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(FieldError::NameRequired);
    }
    if name.chars().any(char::is_control) {
        return Err(FieldError::NameInvalid);
    }
    if name.chars().count() > NAME_MAX as usize {
        return Err(FieldError::NameTooLong);
    }
    Ok(name.to_string())
}

/// Password policy (not trimmed): 8..=256 characters, and `confirm` byte-equal. Returns
/// every problem found.
pub fn password_errors(password: &str, confirm: &str) -> Vec<(Field, FieldError)> {
    let mut errors = Vec::new();
    let length = password.chars().count();
    if length < PASSWORD_MIN as usize {
        errors.push((Field::Password, FieldError::PasswordTooShort));
    } else if length > PASSWORD_MAX as usize {
        errors.push((Field::Password, FieldError::PasswordTooLong));
    }
    if password != confirm {
        errors.push((Field::PasswordConfirm, FieldError::PasswordsDontMatch));
    }
    errors
}
