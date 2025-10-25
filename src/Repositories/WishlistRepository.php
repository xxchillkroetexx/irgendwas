<?php

namespace SecretSanta\Repositories;

use SecretSanta\Database\DataMapper;
use SecretSanta\Models\Wishlist;
use SecretSanta\Models\User;
use SecretSanta\Models\Group;

/**
 * Repository class for handling Wishlist data operations
 * 
 * Manages database interactions for wishlist entities including CRUD operations
 * and relationship management with users and groups.
 */
class WishlistRepository extends DataMapper
{
    /**
     * Database table name
     * 
     * @var string
     */
    protected string $table = 'wishlists';

    /**
     * Entity class name
     * 
     * @var string
     */
    protected string $entityClass = Wishlist::class;

    /**
     * Available database columns
     * 
     * @var array
     */
    protected array $columns = [
        'id',
        'user_id',
        'group_id',
        'created_at',
        'updated_at'
    ];

    /**
     * Find a wishlist by user ID and group ID
     * 
     * @param int $userId User ID to search for
     * @param int $groupId Group ID to search for
     * @return Wishlist|null Wishlist entity if found, null otherwise
     */
    public function findByUserAndGroup(int $userId, int $groupId): ?Wishlist
    {
        $wishlists = $this->findBy(['user_id' => $userId, 'group_id' => $groupId]);
        return !empty($wishlists) ? $wishlists[0] : null;
    }

    /**
     * Find all wishlists for a specific group
     * 
     * @param int $groupId Group ID to search for
     * @return array Array of Wishlist entities
     */
    public function findByGroupId(int $groupId): array
    {
        return $this->findBy(['group_id' => $groupId]);
    }

    /**
     * Find all wishlists for a specific user
     * 
     * @param int $userId User ID to search for
     * @return array Array of Wishlist entities
     */
    public function findByUserId(int $userId): array
    {
        return $this->findBy(['user_id' => $userId]);
    }

    /**
     * Load the user relationship for a wishlist
     * 
     * @param Wishlist $wishlist The wishlist to load user for
     * @return Wishlist Wishlist with user loaded
     */
    public function loadUser(Wishlist $wishlist): Wishlist
    {
        if ($wishlist->getUserId() === null) {
            return $wishlist;
        }

        $userRepository = new UserRepository();
        $user = $userRepository->find($wishlist->getUserId());

        return $wishlist->setUser($user);
    }

    /**
     * Load the group relationship for a wishlist
     * 
     * @param Wishlist $wishlist The wishlist to load group for
     * @return Wishlist Wishlist with group loaded
     */
    public function loadGroup(Wishlist $wishlist): Wishlist
    {
        if ($wishlist->getGroupId() === null) {
            return $wishlist;
        }

        $groupRepository = new GroupRepository();
        $group = $groupRepository->find($wishlist->getGroupId());

        return $wishlist->setGroup($group);
    }

    /**
     * Load the wishlist items for a wishlist
     * 
     * @param Wishlist $wishlist The wishlist to load items for
     * @return Wishlist Wishlist with items loaded
     */
    public function loadItems(Wishlist $wishlist): Wishlist
    {
        if ($wishlist->getId() === null) {
            return $wishlist;
        }

        $wishlistItemRepository = new WishlistItemRepository();
        $items = $wishlistItemRepository->findByWishlistId($wishlist->getId());

        return $wishlist->setItems($items);
    }

    /**
     * Create or update a wishlist for a user in a group
     * 
     * If a wishlist for the given user and group already exists, it will be updated.
     * Otherwise, a new wishlist will be created.
     * 
     * @param int $userId User ID to create/update wishlist for
     * @param int $groupId Group ID to create/update wishlist for
     * @return Wishlist The created or updated wishlist entity
     */
    public function createOrUpdateWishlist(int $userId, int $groupId): Wishlist
    {
        $wishlist = $this->findByUserAndGroup($userId, $groupId);

        if (!$wishlist) {
            // Create new wishlist
            $wishlist = new Wishlist();
            $wishlist->setUserId($userId)
                ->setGroupId($groupId);
        }

        return $this->save($wishlist);
    }
}
