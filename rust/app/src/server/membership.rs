//! Membership data rules (spec: groups, D5).

use sqlx::{MySql, Transaction};

use crate::error::AppError;

/// Deletes everything a departing member owns in `group_id`: the `group_members` row, the
/// `wishlists` row (items cascade), every `exclusion_rules` row where the user is
/// `user_id` or `excluded_user_id`, and any `gift_assignments` row where the user is giver
/// or receiver. Touches nothing of other users or other groups. Later slices extend it.
///
/// The caller holds the group row lock (`SELECT … FOR UPDATE` on `groups`, plan: "group row
/// first") and commits or rolls back `tx`.
pub async fn remove_member_data(
    tx: &mut Transaction<'_, MySql>,
    group_id: i32,
    user_id: i32,
) -> Result<(), AppError> {
    sqlx::query(
        "DELETE FROM gift_assignments WHERE group_id = ? AND (giver_id = ? OR receiver_id = ?)",
    )
    .bind(group_id)
    .bind(user_id)
    .bind(user_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "DELETE FROM exclusion_rules WHERE group_id = ? AND (user_id = ? OR excluded_user_id = ?)",
    )
    .bind(group_id)
    .bind(user_id)
    .bind(user_id)
    .execute(&mut **tx)
    .await?;
    // wishlist_items go with their wishlist (ON DELETE CASCADE).
    sqlx::query("DELETE FROM wishlists WHERE group_id = ? AND user_id = ?")
        .bind(group_id)
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM group_members WHERE group_id = ? AND user_id = ?")
        .bind(group_id)
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
