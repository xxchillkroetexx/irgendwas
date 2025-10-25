<?php

namespace SecretSanta\Repositories;

use SecretSanta\Database\DataMapper;
use SecretSanta\Models\WishlistItem;
use SecretSanta\Models\Wishlist;

/**
 * Repository class for handling WishlistItem data operations
 * 
 * Manages database interactions for wishlist item entities including CRUD operations
 * and position management for ordered lists.
 */
class WishlistItemRepository extends DataMapper
{
    /**
     * Database table name
     * 
     * @var string
     */
    protected string $table = 'wishlist_items';

    /**
     * Entity class name
     * 
     * @var string
     */
    protected string $entityClass = WishlistItem::class;

    /**
     * Available database columns
     * 
     * @var array
     */
    protected array $columns = [
        'id',
        'wishlist_id',
        'title',
        'description',
        'link',
        'created_at',
        'updated_at'
    ];

    /**
     * Find all items for a specific wishlist
     * 
     * Returns items sorted by position in ascending order.
     * 
     * @param int $wishlistId Wishlist ID to find items for
     * @return array Array of WishlistItem entities
     */
    public function findByWishlistId(int $wishlistId): array
    {
        return $this->findBy(['wishlist_id' => $wishlistId]);
    }

    /**
     * Load the wishlist relationship for an item
     * 
     * @param WishlistItem $item The item to load wishlist for
     * @return WishlistItem Item with wishlist loaded
     */
    public function loadWishlist(WishlistItem $item): WishlistItem
    {
        if ($item->getWishlistId() === null) {
            return $item;
        }

        $wishlistRepository = new WishlistRepository();
        $wishlist = $wishlistRepository->find($item->getWishlistId());

        return $item->setWishlist($wishlist);
    }

    /**
     * Create a new wishlist item
     * 
     * Automatically assigns the next available position number.
     * 
     * @param Wishlist $wishlist The wishlist to add item to
     * @param string $title Item title
     * @param string|null $description Optional item description
     * @param string|null $link Optional item link/URL
     * @return WishlistItem The created item entity
     */
    public function createItem(Wishlist $wishlist, string $title, ?string $description = null, ?string $link = null): WishlistItem
    {
        $item = new WishlistItem();
        $item->setWishlistId($wishlist->getId())
            ->setTitle($title)
            ->setDescription($description)
            ->setLink($link);

        return $this->save($item);
    }
}
