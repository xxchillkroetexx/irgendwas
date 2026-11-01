//! `require_member` / `require_admin` against `group_members` and `groups.admin_id`.
//! Groups spec D2: non-members get `NotFound` (like a missing group); a member calling an
//! admin-only check gets `Forbidden`.

use app::AppError;
use app::server::authz::{require_admin, require_member};
use app::test_support::{add_member, create_group, create_user};
use sqlx::MySqlPool;

#[sqlx::test(migrations = "../migrations")]
async fn membership_and_admin_checks(pool: MySqlPool) {
    let admin = create_user(&pool, "Admin", "admin@example.com").await;
    let member = create_user(&pool, "Member", "member@example.com").await;
    let outsider = create_user(&pool, "Outsider", "outsider@example.com").await;
    let group = create_group(&pool, admin, "Family").await;
    add_member(&pool, group, member).await;

    // Admin: member and admin.
    let access = require_member(&pool, admin, group).await.unwrap();
    assert!(access.is_admin && access.is_member);
    assert_eq!(
        require_admin(&pool, admin, group).await.unwrap().group_id,
        group
    );

    // Member: member, not admin.
    let access = require_member(&pool, member, group).await.unwrap();
    assert!(access.is_member && !access.is_admin);
    assert_eq!(
        require_admin(&pool, member, group).await,
        Err(AppError::Forbidden)
    );

    // Outsider: indistinguishable from a missing group (no existence leak).
    assert_eq!(
        require_member(&pool, outsider, group).await,
        Err(AppError::NotFound)
    );
    assert_eq!(
        require_admin(&pool, outsider, group).await,
        Err(AppError::NotFound)
    );

    // Unknown group: not found, for everyone.
    assert_eq!(
        require_member(&pool, admin, group + 1000).await,
        Err(AppError::NotFound)
    );
    assert_eq!(
        require_admin(&pool, admin, group + 1000).await,
        Err(AppError::NotFound)
    );

    // Membership of another group does not leak into this one.
    let other = create_group(&pool, outsider, "Friends").await;
    assert_eq!(
        require_member(&pool, member, other).await,
        Err(AppError::NotFound)
    );
    assert_eq!(
        require_admin(&pool, member, other).await,
        Err(AppError::NotFound)
    );

    // Works inside a transaction too.
    let mut tx = pool.begin().await.unwrap();
    assert!(require_member(&mut *tx, member, group).await.is_ok());
    tx.rollback().await.unwrap();
}
