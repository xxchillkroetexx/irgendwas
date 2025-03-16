<?php

namespace SecretSanta\Models;

/**
 * Class Wishlist
 * 
 * Represents a user's wishlist within a specific group
 */
class Wishlist
{
    /**
     * @var int|null Unique identifier of the wishlist
     */
    private ?int $id = null;
    
    /**
     * @var int ID of the user who owns this wishlist
     */
    private int $user_id;
    
    /**
     * @var int ID of the group this wishlist belongs to
     */
    private int $group_id;
    
    /**
     * @var bool Whether the items in this wishlist are ordered by priority
     */
    private bool $is_priority_ordered = false;
    
    /**
     * @var string Timestamp when the wishlist was created
     */
    private string $created_at;
    
    /**
     * @var string Timestamp when the wishlist was last updated
     */
    private string $updated_at;

    // Lazy-loaded relationships
    /**
     * @var User|null The user who owns this wishlist
     */
    private ?User $user = null;
    
    /**
     * @var Group|null The group this wishlist belongs to
     */
    private ?Group $group = null;
    
    /**
     * @var array Items in this wishlist
     */
    private array $items = [];

    /**
     * Constructor for the Wishlist class
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
        if (isset($data['user_id'])) $this->user_id = (int) $data['user_id'];
        if (isset($data['group_id'])) $this->group_id = (int) $data['group_id'];
        if (isset($data['is_priority_ordered'])) $this->is_priority_ordered = (bool) $data['is_priority_ordered'];
        if (isset($data['created_at'])) $this->created_at = $data['created_at'];
        if (isset($data['updated_at'])) $this->updated_at = $data['updated_at'];
    }

    /**
     * Gets the unique identifier of the wishlist
     * 
     * @return int|null
     */
    public function getId(): ?int
    {
        return $this->id;
    }

    /**
     * Gets the ID of the user who owns this wishlist
     * 
     * @return int
     */
    public function getUserId(): int
    {
        return $this->user_id;
    }

    /**
     * Sets the ID of the user who owns this wishlist
     * 
     * @param int $user_id
     * @return self
     */
    public function setUserId(int $user_id): self
    {
        $this->user_id = $user_id;
        return $this;
    }

    /**
     * Gets the ID of the group this wishlist belongs to
     * 
     * @return int
     */
    public function getGroupId(): int
    {
        return $this->group_id;
    }

    /**
     * Sets the ID of the group this wishlist belongs to
     * 
     * @param int $group_id
     * @return self
     */
    public function setGroupId(int $group_id): self
    {
        $this->group_id = $group_id;
        return $this;
    }

    /**
     * Checks if the items in this wishlist are ordered by priority
     * 
     * @return bool
     */
    public function isPriorityOrdered(): bool
    {
        return $this->is_priority_ordered;
    }

    /**
     * Sets whether the items in this wishlist are ordered by priority
     * 
     * @param bool $is_priority_ordered
     * @return self
     */
    public function setIsPriorityOrdered(bool $is_priority_ordered): self
    {
        $this->is_priority_ordered = $is_priority_ordered;
        return $this;
    }

    /**
     * Gets the timestamp when the wishlist was created
     * 
     * @return string
     */
    public function getCreatedAt(): string
    {
        return $this->created_at;
    }

    /**
     * Gets the timestamp when the wishlist was last updated
     * 
     * @return string
     */
    public function getUpdatedAt(): string
    {
        return $this->updated_at;
    }

    /**
     * Gets the user who owns this wishlist
     * 
     * @return User|null
     */
    public function getUser(): ?User
    {
        return $this->user;
    }

    /**
     * Sets the user who owns this wishlist
     * 
     * @param User|null $user
     * @return self
     */
    public function setUser(?User $user): self
    {
        $this->user = $user;
        if ($user) {
            $this->user_id = $user->getId();
        }
        return $this;
    }

    /**
     * Gets the group this wishlist belongs to
     * 
     * @return Group|null
     */
    public function getGroup(): ?Group
    {
        return $this->group;
    }

    /**
     * Sets the group this wishlist belongs to
     * 
     * @param Group|null $group
     * @return self
     */
    public function setGroup(?Group $group): self
    {
        $this->group = $group;
        if ($group) {
            $this->group_id = $group->getId();
        }
        return $this;
    }

    /**
     * Gets the items in this wishlist
     * 
     * @return array
     */
    public function getItems(): array
    {
        return $this->items;
    }

    /**
     * Sets the items in this wishlist
     * 
     * @param array $items
     * @return self
     */
    public function setItems(array $items): self
    {
        $this->items = $items;
        return $this;
    }

    /**
     * Converts the wishlist object to an associative array
     * 
     * @return array
     */
    public function toArray(): array
    {
        return [
            'id' => $this->id,
            'user_id' => $this->user_id,
            'group_id' => $this->group_id,
            'is_priority_ordered' => $this->is_priority_ordered,
            'created_at' => $this->created_at ?? null,
            'updated_at' => $this->updated_at ?? null
        ];
    }
}
