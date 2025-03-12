<?php

namespace SecretSanta\Repositories;

use SecretSanta\Database\DataMapper;
use SecretSanta\Models\Wishlist;
use SecretSanta\Models\User;
use SecretSanta\Models\Group;

class WishlistRepository extends DataMapper
{
    protected string $table = 'wishlists';
    protected string $entityClass = Wishlist::class;
    protected array $columns = [
        'id',
        'user_id',
        'group_id',
        'is_priority_ordered',
        'created_at',
        'updated_at'
    ];

    /**
     * Find a wishlist by user ID and group ID
     */
    public function findByUserAndGroup(int $userId, int $groupId): ?Wishlist
    {
        $wishlists = $this->findBy(['user_id' => $userId, 'group_id' => $groupId]);
        return !empty($wishlists) ? $wishlists[0] : null;
    }

    /**
     * Find all wishlists for a specific group
     */
    public function findByGroupId(int $groupId): array
    {
        return $this->findBy(['group_id' => $groupId]);
    }

    /**
     * Find all wishlists for a specific user
     */
    public function findByUserId(int $userId): array
    {
        return $this->findBy(['user_id' => $userId]);
    }

    /**
     * Load the user relationship for a wishlist
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
     */
    public function loadItems(Wishlist $wishlist): Wishlist
    {
        if ($wishlist->getId() === null) {
            return $wishlist;
        }

        $wishlistItemRepository = new WishlistItemRepository();
        $items = $wishlistItemRepository->findByWishlistId($wishlist->getId());

        // Sort items by position if priority ordered
        if ($wishlist->isPriorityOrdered()) {
            usort($items, function ($a, $b) {
                return $a->getPosition() - $b->getPosition();
            });
        }

        return $wishlist->setItems($items);
    }

    /**
     * Create or update a wishlist for a user in a group
     */
    public function createOrUpdateWishlist(int $userId, int $groupId, bool $isPriorityOrdered = false): Wishlist
    {
        $wishlist = $this->findByUserAndGroup($userId, $groupId);

        if (!$wishlist) {
            // Create new wishlist
            $wishlist = new Wishlist();
            $wishlist->setUserId($userId)
                ->setGroupId($groupId)
                ->setIsPriorityOrdered($isPriorityOrdered);
        } else {
            // Update existing wishlist
            $wishlist->setIsPriorityOrdered($isPriorityOrdered);
        }

        return $this->save($wishlist);
    }
}
