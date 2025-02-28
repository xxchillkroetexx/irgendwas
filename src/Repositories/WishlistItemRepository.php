<?php

namespace SecretSanta\Repositories;

use SecretSanta\Database\DataMapper;
use SecretSanta\Models\WishlistItem;
use SecretSanta\Models\Wishlist;

class WishlistItemRepository extends DataMapper {
    protected string $table = 'wishlist_items';
    protected string $entityClass = WishlistItem::class;
    protected array $columns = [
        'id', 'wishlist_id', 'title', 'description', 'link', 'position', 'created_at', 'updated_at'
    ];
    
    /**
     * Find all items for a specific wishlist
     */
    public function findByWishlistId(int $wishlistId): array {
        return $this->findBy(['wishlist_id' => $wishlistId], ['position' => 'ASC']);
    }
    
    /**
     * Load the wishlist relationship for an item
     */
    public function loadWishlist(WishlistItem $item): WishlistItem {
        if ($item->getWishlistId() === null) {
            return $item;
        }
        
        $wishlistRepository = new WishlistRepository();
        $wishlist = $wishlistRepository->find($item->getWishlistId());
        
        return $item->setWishlist($wishlist);
    }
    
    /**
     * Create a new wishlist item
     */
    public function createItem(Wishlist $wishlist, string $title, ?string $description = null, ?string $link = null): WishlistItem {
        // Find the highest position number for this wishlist
        $items = $this->findByWishlistId($wishlist->getId());
        $maxPosition = 0;
        
        foreach ($items as $item) {
            if ($item->getPosition() > $maxPosition) {
                $maxPosition = $item->getPosition();
            }
        }
        
        // Create new item with the next position number
        $item = new WishlistItem();
        $item->setWishlistId($wishlist->getId())
             ->setTitle($title)
             ->setDescription($description)
             ->setLink($link)
             ->setPosition($maxPosition + 1);
        
        return $this->save($item);
    }
    
    /**
     * Update item positions for priority ordering
     */
    public function updatePositions(array $itemPositions): bool {
        $this->beginTransaction();
        
        try {
            foreach ($itemPositions as $itemId => $position) {
                $item = $this->find($itemId);
                if ($item) {
                    $item->setPosition($position);
                    $this->save($item);
                }
            }
            
            $this->commit();
            return true;
        } catch (\Exception $e) {
            $this->rollback();
            return false;
        }
    }
    
    /**
     * Delete all items for a wishlist
     */
    public function deleteAllForWishlist(int $wishlistId): bool {
        $items = $this->findByWishlistId($wishlistId);
        
        $this->beginTransaction();
        
        try {
            foreach ($items as $item) {
                $this->delete($item);
            }
            
            $this->commit();
            return true;
        } catch (\Exception $e) {
            $this->rollback();
            return false;
        }
    }
}