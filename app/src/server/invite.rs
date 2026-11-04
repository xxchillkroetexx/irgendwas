//! Invitation codes (spec: groups, D10): 8 characters drawn uniformly with the OS CSPRNG
//! from a 32-symbol alphabet (40 bits, repeats allowed), retried on a UNIQUE collision.
//! The column is `utf8mb4_bin` (migration 0005), so a lookup is exact on the normalised
//! value.

use sqlx::{MySql, Transaction};

use crate::error::AppError;

/// The 32 symbols a code is drawn from.
pub const CODE_ALPHABET: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
/// Characters per code.
pub const CODE_LEN: usize = 8;
/// Tries on a UNIQUE collision before giving up with [`AppError::Internal`].
pub const MAX_CODE_ATTEMPTS: usize = 5;

/// 8 characters of [`CODE_ALPHABET`], uniformly from a CSPRNG.
pub fn generate_code() -> String {
    let alphabet = CODE_ALPHABET.as_bytes();
    let mut bytes = [0u8; CODE_LEN];
    getrandom::fill(&mut bytes).expect("the OS random number generator is available");
    // 256 is a multiple of 32: the low five bits of a uniform byte are uniform.
    bytes
        .iter()
        .map(|b| char::from(alphabet[usize::from(*b) % alphabet.len()]))
        .collect()
}

/// Trim, remove spaces and `-`, uppercase. Nothing else (no accent folding). One
/// implementation, shared with the enter-code form on the client.
pub use crate::groups_api::normalize_invitation_code as normalize_code;

fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db) if db.is_unique_violation())
}

/// Inserts a group (no dates, not drawn) with a code from `next_code` and the admin's
/// membership, in `tx`. On a UNIQUE collision of the code it asks `next_code` again, up to
/// [`MAX_CODE_ATTEMPTS`] calls in total, then returns [`AppError::Internal`]. Returns the
/// group id and its code.
pub async fn insert_group_with_code(
    tx: &mut Transaction<'_, MySql>,
    name: &str,
    admin_id: i32,
    next_code: &mut (dyn FnMut() -> String + Send),
) -> Result<(i32, String), AppError> {
    for _ in 0..MAX_CODE_ATTEMPTS {
        let code = next_code();
        let inserted = sqlx::query(
            "INSERT INTO `groups` (name, admin_id, invitation_code, is_drawn) VALUES (?, ?, ?, 0)",
        )
        .bind(name)
        .bind(admin_id)
        .bind(&code)
        .execute(&mut **tx)
        .await;
        match inserted {
            Ok(result) => {
                let group_id = i32::try_from(result.last_insert_id())
                    .map_err(|_| AppError::internal("group id does not fit i32"))?;
                sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES (?, ?)")
                    .bind(group_id)
                    .bind(admin_id)
                    .execute(&mut **tx)
                    .await?;
                return Ok((group_id, code));
            }
            // A failed statement does not end the transaction (InnoDB rolls back only the
            // statement), so the next try runs in the same `tx`.
            Err(err) if is_unique_violation(&err) => {
                tracing::warn!("invitation code collision on insert, retrying");
            }
            Err(err) => return Err(err.into()),
        }
    }
    Err(AppError::internal(
        "no unique invitation code after the maximum number of attempts",
    ))
}

/// Sets a fresh code on `group_id` with the same retry rule. Returns the new code.
pub async fn replace_code(
    tx: &mut Transaction<'_, MySql>,
    group_id: i32,
    next_code: &mut (dyn FnMut() -> String + Send),
) -> Result<String, AppError> {
    for _ in 0..MAX_CODE_ATTEMPTS {
        let code = next_code();
        let updated = sqlx::query("UPDATE `groups` SET invitation_code = ? WHERE id = ?")
            .bind(&code)
            .bind(group_id)
            .execute(&mut **tx)
            .await;
        match updated {
            Ok(_) => return Ok(code),
            Err(err) if is_unique_violation(&err) => {
                tracing::warn!(group_id, "invitation code collision on replace, retrying");
            }
            Err(err) => return Err(err.into()),
        }
    }
    Err(AppError::internal(
        "no unique invitation code after the maximum number of attempts",
    ))
}
