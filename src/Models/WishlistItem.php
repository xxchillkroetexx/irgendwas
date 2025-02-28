<?php

namespace SecretSanta\Models;

class WishlistItem {
    private ?int $id = null;
    private int $wishlist_id;
    private string $title;
    private ?string $description = null;
    private ?string $link = null;
    private int $position = 0;
    private string $created_at;
    private string $updated_at;
    
    // Lazy-loaded relationships
    private ?Wishlist $wishlist = null;
    
    public function __construct(array $data = []) {
        if (!empty($data)) {
            $this->hydrate($data);
        }
    }
    
    public function hydrate(array $data): void {
        if (isset($data['id'])) $this->id = (int) $data['id'];
        if (isset($data['wishlist_id'])) $this->wishlist_id = (int) $data['wishlist_id'];
        if (isset($data['title'])) $this->title = $data['title'];
        if (isset($data['description'])) $this->description = $data['description'];
        if (isset($data['link'])) $this->link = $data['link'];
        if (isset($data['position'])) $this->position = (int) $data['position'];
        if (isset($data['created_at'])) $this->created_at = $data['created_at'];
        if (isset($data['updated_at'])) $this->updated_at = $data['updated_at'];
    }
    
    public function getId(): ?int {
        return $this->id;
    }
    
    public function getWishlistId(): int {
        return $this->wishlist_id;
    }
    
    public function setWishlistId(int $wishlist_id): self {
        $this->wishlist_id = $wishlist_id;
        return $this;
    }
    
    public function getTitle(): string {
        return $this->title;
    }
    
    public function setTitle(string $title): self {
        $this->title = $title;
        return $this;
    }
    
    public function getDescription(): ?string {
        return $this->description;
    }
    
    public function setDescription(?string $description): self {
        $this->description = $description;
        return $this;
    }
    
    public function getLink(): ?string {
        return $this->link;
    }
    
    public function setLink(?string $link): self {
        $this->link = $link;
        return $this;
    }
    
    public function getPosition(): int {
        return $this->position;
    }
    
    public function setPosition(int $position): self {
        $this->position = $position;
        return $this;
    }
    
    public function getCreatedAt(): string {
        return $this->created_at;
    }
    
    public function getUpdatedAt(): string {
        return $this->updated_at;
    }
    
    public function getWishlist(): ?Wishlist {
        return $this->wishlist;
    }
    
    public function setWishlist(?Wishlist $wishlist): self {
        $this->wishlist = $wishlist;
        if ($wishlist) {
            $this->wishlist_id = $wishlist->getId();
        }
        return $this;
    }
    
    public function toArray(): array {
        return [
            'id' => $this->id,
            'wishlist_id' => $this->wishlist_id,
            'title' => $this->title,
            'description' => $this->description,
            'link' => $this->link,
            'position' => $this->position,
            'created_at' => $this->created_at ?? null,
            'updated_at' => $this->updated_at ?? null
        ];
    }
}