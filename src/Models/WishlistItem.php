<?php

namespace SecretSanta\Models;

/**
 * Class WishlistItem
 * 
 * Represents an item in a user's wishlist
 */
class WishlistItem
{
    /**
     * @var int|null Unique identifier of the wishlist item
     */
    private ?int $id = null;
    
    /**
     * @var int ID of the wishlist this item belongs to
     */
    private int $wishlist_id;
    
    /**
     * @var string Title of the wishlist item
     */
    private string $title;
    
    /**
     * @var string|null Optional description of the wishlist item
     */
    private ?string $description = null;
    
    /**
     * @var string|null Optional URL link to the item
     */
    private ?string $link = null;
    
    /**
     * @var string Timestamp when the item was created
     */
    private string $created_at;
    
    /**
     * @var string Timestamp when the item was last updated
     */
    private string $updated_at;

    // Lazy-loaded relationships
    /**
     * @var Wishlist|null The wishlist this item belongs to
     */
    private ?Wishlist $wishlist = null;

    /**
     * Constructor for the WishlistItem class
     * 
     * @param array $data Optional data to hydrate the object with
     */
    public function __construct(array $data = [])
    {
        if (!empty($data)) {
            $this->hydrate($data);
        }
    }

    /**
     * Hydrates the object with the provided data
     * 
     * @param array $data Associative array of data to populate the object
     * @return void
     */
    public function hydrate(array $data): void
    {
        if (isset($data['id'])) $this->id = (int) $data['id'];
        if (isset($data['wishlist_id'])) $this->wishlist_id = (int) $data['wishlist_id'];
        if (isset($data['title'])) $this->title = $data['title'];
        if (isset($data['description'])) $this->description = $data['description'];
        if (isset($data['link'])) $this->link = $data['link'];
        if (isset($data['created_at'])) $this->created_at = $data['created_at'];
        if (isset($data['updated_at'])) $this->updated_at = $data['updated_at'];
    }

    /**
     * Gets the ID of the wishlist item
     * 
     * @return int|null The ID or null if not set
     */
    public function getId(): ?int
    {
        return $this->id;
    }

    /**
     * Gets the ID of the wishlist this item belongs to
     * 
     * @return int The wishlist ID
     */
    public function getWishlistId(): int
    {
        return $this->wishlist_id;
    }

    /**
     * Sets the ID of the wishlist this item belongs to
     * 
     * @param int $wishlist_id The wishlist ID
     * @return self
     */
    public function setWishlistId(int $wishlist_id): self
    {
        $this->wishlist_id = $wishlist_id;
        return $this;
    }

    /**
     * Gets the title of the wishlist item
     * 
     * @return string The title
     */
    public function getTitle(): string
    {
        return $this->title;
    }

    /**
     * Sets the title of the wishlist item
     * 
     * @param string $title The title
     * @return self
     */
    public function setTitle(string $title): self
    {
        $this->title = $title;
        return $this;
    }

    /**
     * Gets the description of the wishlist item
     * 
     * @return string|null The description or null if not set
     */
    public function getDescription(): ?string
    {
        return $this->description;
    }

    /**
     * Sets the description of the wishlist item
     * 
     * @param string|null $description The description
     * @return self
     */
    public function setDescription(?string $description): self
    {
        $this->description = $description;
        return $this;
    }

    /**
     * Gets the URL link to the wishlist item
     * 
     * @return string|null The URL link or null if not set
     */
    public function getLink(): ?string
    {
        return $this->link;
    }

    /**
     * Sets the URL link to the wishlist item
     * 
     * @param string|null $link The URL link
     * @return self
     */
    public function setLink(?string $link): self
    {
        $this->link = $link;
        return $this;
    }

    /**
     * Gets the timestamp when the item was created
     * 
     * @return string The created timestamp
     */
    public function getCreatedAt(): string
    {
        return $this->created_at;
    }

    /**
     * Gets the timestamp when the item was last updated
     * 
     * @return string The updated timestamp
     */
    public function getUpdatedAt(): string
    {
        return $this->updated_at;
    }

    /**
     * Gets the wishlist this item belongs to
     * 
     * @return Wishlist|null The wishlist or null if not set
     */
    public function getWishlist(): ?Wishlist
    {
        return $this->wishlist;
    }

    /**
     * Sets the wishlist this item belongs to
     * 
     * @param Wishlist|null $wishlist The wishlist
     * @return self
     */
    public function setWishlist(?Wishlist $wishlist): self
    {
        $this->wishlist = $wishlist;
        if ($wishlist) {
            $this->wishlist_id = $wishlist->getId();
        }
        return $this;
    }

    /**
     * Converts the object to an associative array
     * 
     * @return array The associative array representation of the object
     */
    public function toArray(): array
    {
        return [
            'id' => $this->id,
            'wishlist_id' => $this->wishlist_id,
            'title' => $this->title,
            'description' => $this->description,
            'link' => $this->link,
            'created_at' => $this->created_at ?? null,
            'updated_at' => $this->updated_at ?? null
        ];
    }
}
