<?php

namespace SecretSanta\Models;

class ExclusionRule {
    private ?int $id = null;
    private int $group_id;
    private int $user_id;
    private int $excluded_user_id;
    private string $created_at;
    
    // Lazy-loaded relationships
    private ?Group $group = null;
    private ?User $user = null;
    private ?User $excludedUser = null;
    
    public function __construct(array $data = []) {
        if (!empty($data)) {
            $this->hydrate($data);
        }
    }
    
    public function hydrate(array $data): void {
        if (isset($data['id'])) $this->id = (int) $data['id'];
        if (isset($data['group_id'])) $this->group_id = (int) $data['group_id'];
        if (isset($data['user_id'])) $this->user_id = (int) $data['user_id'];
        if (isset($data['excluded_user_id'])) $this->excluded_user_id = (int) $data['excluded_user_id'];
        if (isset($data['created_at'])) $this->created_at = $data['created_at'];
    }
    
    public function getId(): ?int {
        return $this->id;
    }
    
    public function getGroupId(): int {
        return $this->group_id;
    }
    
    public function setGroupId(int $group_id): self {
        $this->group_id = $group_id;
        return $this;
    }
    
    public function getUserId(): int {
        return $this->user_id;
    }
    
    public function setUserId(int $user_id): self {
        $this->user_id = $user_id;
        return $this;
    }
    
    public function getExcludedUserId(): int {
        return $this->excluded_user_id;
    }
    
    public function setExcludedUserId(int $excluded_user_id): self {
        $this->excluded_user_id = $excluded_user_id;
        return $this;
    }
    
    public function getCreatedAt(): string {
        return $this->created_at;
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
    
    public function getExcludedUser(): ?User {
        return $this->excludedUser;
    }
    
    public function setExcludedUser(?User $user): self {
        $this->excludedUser = $user;
        if ($user) {
            $this->excluded_user_id = $user->getId();
        }
        return $this;
    }
    
    public function toArray(): array {
        return [
            'id' => $this->id,
            'group_id' => $this->group_id,
            'user_id' => $this->user_id,
            'excluded_user_id' => $this->excluded_user_id,
            'created_at' => $this->created_at ?? null
        ];
    }
}