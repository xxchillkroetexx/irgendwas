<?php

namespace SecretSanta\Models;

class Wishlist {
    private ?int $id = null;
    private int $user_id;
    private int $group_id;
    private bool $is_priority_ordered = false;
    private string $created_at;
    private string $updated_at;
    
    // Lazy-loaded relationships
    private ?User $user = null;
    private ?Group $group = null;
    private array $items = [];
    
    public function __construct(array $data = []) {
        if (!empty($data)) {
            $this->hydrate($data);
        }
    }
    
    public function hydrate(array $data): void {
        if (isset($data['id'])) $this->id = (int) $data['id'];
        if (isset($data['user_id'])) $this->user_id = (int) $data['user_id'];
        if (isset($data['group_id'])) $this->group_id = (int) $data['group_id'];
        if (isset($data['is_priority_ordered'])) $this->is_priority_ordered = (bool) $data['is_priority_ordered'];
        if (isset($data['created_at'])) $this->created_at = $data['created_at'];
        if (isset($data['updated_at'])) $this->updated_at = $data['updated_at'];
    }
    
    public function getId(): ?int {
        return $this->id;
    }
    
    public function getUserId(): int {
        return $this->user_id;
    }
    
    public function setUserId(int $user_id): self {
        $this->user_id = $user_id;
        return $this;
    }
    
    public function getGroupId(): int {
        return $this->group_id;
    }
    
    public function setGroupId(int $group_id): self {
        $this->group_id = $group_id;
        return $this;
    }
    
    public function isPriorityOrdered(): bool {
        return $this->is_priority_ordered;
    }
    
    public function setIsPriorityOrdered(bool $is_priority_ordered): self {
        $this->is_priority_ordered = $is_priority_ordered;
        return $this;
    }
    
    public function getCreatedAt(): string {
        return $this->created_at;
    }
    
    public function getUpdatedAt(): string {
        return $this->updated_at;
    }
    
    public function getUser(): ?User {
        return $this->user;
    }
    
    public function setUser(?User $user): self {
        $this->user = $user;
        if ($user) {
            $this->user_id = $user->getId();
        }
        return $this;
    }
    
    public function getGroup(): ?Group {
        return $this->group;
    }
    
    public function setGroup(?Group $group): self {
        $this->group = $group;
        if ($group) {
            $this->group_id = $group->getId();
        }
        return $this;
    }
    
    public function getItems(): array {
        return $this->items;
    }
    
    public function setItems(array $items): self {
        $this->items = $items;
        return $this;
    }
    
    public function toArray(): array {
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