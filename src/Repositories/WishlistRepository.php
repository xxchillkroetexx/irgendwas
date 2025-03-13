<?php

namespace SecretSanta\Repositories;

use SecretSanta\Database\DataMapper;
use SecretSanta\Models\Wishlist;
use SecretSanta\Models\WishlistItem;
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

    public function findWithItemsByUserAndGroup(int $userId, int $groupId): ?Wishlist
    {
        $query = "
            SELECT 
                w.*, 
                wi.id as item_id,
                wi.title as item_title,
                wi.description as item_description,
                wi.link as item_link,
                wi.position as item_position,
                wi.created_at as item_created_at,
                wi.updated_at as item_updated_at
            FROM 
                wishlists w
            LEFT JOIN 
                wishlist_items wi ON w.id = wi.wishlist_id
            WHERE 
                w.user_id = :user_id 
                AND w.group_id = :group_id
            ORDER BY 
                wi.position ASC
        ";
        
        $stmt = $this->db->prepare($query);
        $stmt->execute([
            'user_id' => $userId,
            'group_id' => $groupId
        ]);
        
        $rows = $stmt->fetchAll();
        
        if (empty($rows)) {
            return null;
        }
        
        return $this->mapJoinedRowsToWishlist($rows);
    }

    public function findAllWithItems(array $criteria = [], array $orderBy = []): array
    {
        $query = "
            SELECT 
                w.*, 
                wi.id as item_id,
                wi.title as item_title,
                wi.description as item_description,
                wi.link as item_link,
                wi.position as item_position,
                wi.created_at as item_created_at,
                wi.updated_at as item_updated_at
            FROM 
                wishlists w
            LEFT JOIN 
                wishlist_items wi ON w.id = wi.wishlist_id
        ";
        
        // Add WHERE clause if criteria provided
        if (!empty($criteria)) {
            $query .= " WHERE ";
            $conditions = [];
            foreach (array_keys($criteria) as $key) {
                $conditions[] = "w.$key = :$key";
            }
            $query .= implode(' AND ', $conditions);
        }
        
        // Add ORDER BY clause
        if (!empty($orderBy)) {
            $query .= " ORDER BY ";
            $orders = [];
            foreach ($orderBy as $column => $direction) {
                $orders[] = "w.$column $direction";
            }
            $query .= implode(', ', $orders);
        }
        
        // Always order by wishlist ID and item position for consistent results
        $query .= empty($orderBy) ? " ORDER BY w.id, wi.position" : ", w.id, wi.position";
        
        $stmt = $this->db->prepare($query);
        
        // Bind parameters for criteria
        foreach ($criteria as $key => $value) {
            $stmt->bindValue(":$key", $value);
        }
        
        $stmt->execute();
        $rows = $stmt->fetchAll();
        
        return $this->mapJoinedRowsToWishlists($rows);
    }

    //Helper Function to support eager loading
    private function mapJoinedRowsToWishlist(array $rows): ?Wishlist
    {
        if (empty($rows)) {
            return null;
        }
        
        // First row contains the wishlist data
        $wishlistData = [
            'id' => $rows[0]['id'],
            'user_id' => $rows[0]['user_id'],
            'group_id' => $rows[0]['group_id'],
            'is_priority_ordered' => $rows[0]['is_priority_ordered'],
            'created_at' => $rows[0]['created_at'],
            'updated_at' => $rows[0]['updated_at']
        ];
        
        $wishlist = new Wishlist($wishlistData);
        $items = [];
        
        // Process each row to extract wishlist items
        foreach ($rows as $row) {
            // Skip rows without item ID (in case of LEFT JOIN with no items)
            if (empty($row['item_id'])) {
                continue;
            }
            
            $itemData = [
                'id' => $row['item_id'],
                'wishlist_id' => $wishlist->getId(),
                'title' => $row['item_title'],
                'description' => $row['item_description'],
                'link' => $row['item_link'],
                'position' => $row['item_position'],
                'created_at' => $row['item_created_at'],
                'updated_at' => $row['item_updated_at']
            ];
            
            $items[] = new WishlistItem($itemData);
        }
        
        $wishlist->setItems($items);
        return $wishlist;
    }

    // Helper Function for eager loading
    private function mapJoinedRowsToWishlists(array $rows): array
    {
        if (empty($rows)) {
            return [];
        }
        
        $wishlists = [];
        $wishlistItems = [];
        
        // Group rows by wishlist ID
        foreach ($rows as $row) {
            $wishlistId = $row['id'];
            
            // Initialize wishlist if not already created
            if (!isset($wishlists[$wishlistId])) {
                $wishlistData = [
                    'id' => $wishlistId,
                    'user_id' => $row['user_id'],
                    'group_id' => $row['group_id'],
                    'is_priority_ordered' => $row['is_priority_ordered'],
                    'created_at' => $row['created_at'],
                    'updated_at' => $row['updated_at']
                ];
                
                $wishlists[$wishlistId] = new Wishlist($wishlistData);
                $wishlistItems[$wishlistId] = [];
            }
            
            // Add item to the wishlist items array (if item exists)
            if (!empty($row['item_id'])) {
                $itemData = [
                    'id' => $row['item_id'],
                    'wishlist_id' => $wishlistId,
                    'title' => $row['item_title'],
                    'description' => $row['item_description'],
                    'link' => $row['item_link'],
                    'position' => $row['item_position'],
                    'created_at' => $row['item_created_at'],
                    'updated_at' => $row['item_updated_at']
                ];
                
                $wishlistItems[$wishlistId][] = new WishlistItem($itemData);
            }
        }
        
        // Set items on each wishlist
        foreach ($wishlists as $id => $wishlist) {
            $wishlist->setItems($wishlistItems[$id]);
        }
        
        return array_values($wishlists);
    }

}
