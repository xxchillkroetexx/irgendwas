<?php
namespace models;

use core\Database\Projekt_DB;

class Wishlist {
    private $db;
    
    // Wishlist properties
    private $id;
    private $userId;
    private $groupId;
    private $createdAt;
    private $updatedAt;
    private $items = [];
    
    public function __construct($data = null) {
        $this->db = Projekt_DB::getInstance();
        
        if ($data) {
            $this->mapData($data);
        }
    }
    
    // Map data from array or object to properties
    private function mapData($data) {
        $data = (array) $data;
        
        $this->id = $data['id'] ?? null;
        $this->userId = $data['user_id'] ?? null;
        $this->groupId = $data['group_id'] ?? null;
        $this->createdAt = $data['created_at'] ?? null;
        $this->updatedAt = $data['updated_at'] ?? null;
    }
    
    // Find wishlist by ID
    public function findById($id) {
        $wishlist = $this->db->table('wishlists')
            ->where('id', $id)
            ->first();
            
        if ($wishlist) {
            $instance = new self($wishlist);
            $instance->loadItems();
            return $instance;
        }
        
        return null;
    }
    
    // Find wishlist by user ID and group ID
    public function findByUserAndGroup($userId, $groupId) {
        $wishlist = $this->db->table('wishlists')
            ->where('user_id', $userId)
            ->where('group_id', $groupId)
            ->first();
            
        if ($wishlist) {
            $instance = new self($wishlist);
            $instance->loadItems();
            return $instance;
        }
        
        return null;
    }
    
    // Create a new wishlist
    public function create($userId, $groupId) {
        // Check if wishlist already exists
        $existing = $this->findByUserAndGroup($userId, $groupId);
        if ($existing) {
            return $existing;
        }
        
        $wishlistId = $this->db->table('wishlists')->insert([
            'user_id' => $userId,
            'group_id' => $groupId
        ]);
        
        return $this->findById($wishlistId);
    }
    
    // Load wishlist items from database
    public function loadItems() {
        if (!$this->id) {
            return [];
        }
        
        $this->items = $this->db->table('wishlist_items')
            ->where('wishlist_id', $this->id)
            ->orderBy('sort_order', 'ASC')
            ->get();
            
        return $this->items;
    }
    
    // Add item to wishlist
    public function addItem($name, $description = null, $link = null) {
        if (!$this->id) {
            return false;
        }
        
        // Get the highest sort order
        $maxOrder = $this->db->table('wishlist_items')
            ->select('MAX(sort_order) as max_order')
            ->where('wishlist_id', $this->id)
            ->fetchColumn();
            
        $nextOrder = $maxOrder ? $maxOrder + 10 : 10;
        
        $itemId = $this->db->table('wishlist_items')->insert([
            'wishlist_id' => $this->id,
            'name' => $name,
            'description' => $description,
            'link' => $link,
            'sort_order' => $nextOrder
        ]);
        
        // Reload items
        $this->loadItems();
        
        return $itemId;
    }
    
    // Update wishlist item
    public function updateItem($itemId, $data) {
        if (!$this->id) {
            return false;
        }
        
        // Check if item belongs to this wishlist
        $item = $this->db->table('wishlist_items')
            ->where('id', $itemId)
            ->where('wishlist_id', $this->id)
            ->first();
            
        if (!$item) {
            return false;
        }
        
        $result = $this->db->table('wishlist_items')
            ->where('id', $itemId)
            ->update($data);
            
        // Reload items
        $this->loadItems();
        
        return $result;
    }
    
    // Delete wishlist item
    public function deleteItem($itemId) {
        if (!$this->id) {
            return false;
        }
        
        // Check if item belongs to this wishlist
        $item = $this->db->table('wishlist_items')
            ->where('id', $itemId)
            ->where('wishlist_id', $this->id)
            ->first();
            
        if (!$item) {
            return false;
        }
        
        $result = $this->db->table('wishlist_items')
            ->where('id', $itemId)
            ->delete();
            
        // Reload items
        $this->loadItems();
        
        return $result;
    }
    
    // Reorder wishlist items
    public function reorderItems($itemOrder) {
        if (!$this->id) {
            return false;
        }
        
        $this->db->beginTransaction();
        
        try {
            foreach ($itemOrder as $index => $itemId) {
                $sortOrder = ($index + 1) * 10;
                $this->db->table('wishlist_items')
                    ->where('id', $itemId)
                    ->where('wishlist_id', $this->id)
                    ->update(['sort_order' => $sortOrder]);
            }
            
            $this->db->commit();
            $this->loadItems();
            return true;
        } catch (\Exception $e) {
            $this->db->rollBack();
            return false;
        }
    }
    
    // Delete the entire wishlist
    public function delete() {
        if (!$this->id) {
            return false;
        }
        
        // Items will be deleted by foreign key constraint
        return $this->db->table('wishlists')
            ->where('id', $this->id)
            ->delete();
    }
    
    // Check if wishlist belongs to user
    public function belongsToUser($userId) {
        return $this->userId == $userId;
    }
    
    // Check if wishlist is visible to a given user
    public function isVisibleTo($userId) {
        // If wishlist belongs to the user, they can see it
        if ($this->belongsToUser($userId)) {
            return true;
        }
        
        // Get the group
        $groupModel = new Group();
        $group = $groupModel->findById($this->groupId);
        
        if (!$group) {
            return false;
        }
        
        // Admin can always see wishlists
        if ($group->isAdmin($userId)) {
            return true;
        }
        
        // If wishlist visibility is set to all group members
        if ($group->getWishlistVisibility() === 'all' && $group->isMember($userId)) {
            return true;
        }
        
        // Check if this user is assigned to gift to the wishlist owner
        $assignment = $group->getUserAssignment($userId);
        if ($assignment && $assignment->getId() == $this->userId) {
            return true;
        }
        
        return false;
    }
    
    // Getters
    public function getId() {
        return $this->id;
    }
    
    public function getUserId() {
        return $this->userId;
    }
    
    public function getGroupId() {
        return $this->groupId;
    }
    
    public function getItems() {
        return $this->items;
    }
    
    public function getUser() {
        $userModel = new User();
        return $userModel->findById($this->userId);
    }
    
    public function getGroup() {
        $groupModel = new Group();
        return $groupModel->findById($this->groupId);
    }
    
    public function getCreatedAt() {
        return $this->createdAt;
    }
    
    public function getUpdatedAt() {
        return $this->updatedAt;
    }
}
