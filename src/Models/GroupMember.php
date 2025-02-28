<?php

namespace SecretSanta\Models;

class GroupMember {
    private ?int $id = null;
    private int $group_id;
    private int $user_id;
    private string $joined_at;
    
    // Lazy-loaded relationships
    private ?User $user = null;
    private ?Group $group = null;
    
    public function __construct(array $data = []) {
        if (!empty($data)) {
            $this->hydrate($data);
        }
    }
    
    public function hydrate(array $data): void {
        if (isset($data['id'])) $this->id = (int) $data['id'];
        if (isset($data['group_id'])) $this->group_id = (int) $data['group_id'];
        if (isset($data['user_id'])) $this->user_id = (int) $data['user_id'];
        if (isset($data['joined_at'])) $this->joined_at = $data['joined_at'];
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
    
    public function getJoinedAt(): string {
        return $this->joined_at;
    }
    
    public function setJoinedAt(string $joined_at): self {
        $this->joined_at = $joined_at;
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
    
    public function toArray(): array {
        return [
            'id' => $this->id,
            'group_id' => $this->group_id,
            'user_id' => $this->user_id,
            'joined_at' => $this->joined_at
        ];
    }
}