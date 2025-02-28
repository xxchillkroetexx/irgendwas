<?php

namespace SecretSanta\Models;

class GiftAssignment {
    private ?int $id = null;
    private int $group_id;
    private int $giver_id;
    private int $receiver_id;
    private string $created_at;
    private bool $notification_sent = false;
    
    // Lazy-loaded relationships
    private ?Group $group = null;
    private ?User $giver = null;
    private ?User $receiver = null;
    
    public function __construct(array $data = []) {
        if (!empty($data)) {
            $this->hydrate($data);
        }
    }
    
    public function hydrate(array $data): void {
        if (isset($data['id'])) $this->id = (int) $data['id'];
        if (isset($data['group_id'])) $this->group_id = (int) $data['group_id'];
        if (isset($data['giver_id'])) $this->giver_id = (int) $data['giver_id'];
        if (isset($data['receiver_id'])) $this->receiver_id = (int) $data['receiver_id'];
        if (isset($data['created_at'])) $this->created_at = $data['created_at'];
        if (isset($data['notification_sent'])) $this->notification_sent = (bool) $data['notification_sent'];
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
    
    public function getGiverId(): int {
        return $this->giver_id;
    }
    
    public function setGiverId(int $giver_id): self {
        $this->giver_id = $giver_id;
        return $this;
    }
    
    public function getReceiverId(): int {
        return $this->receiver_id;
    }
    
    public function setReceiverId(int $receiver_id): self {
        $this->receiver_id = $receiver_id;
        return $this;
    }
    
    public function getCreatedAt(): string {
        return $this->created_at;
    }
    
    public function isNotificationSent(): bool {
        return $this->notification_sent;
    }
    
    public function setNotificationSent(bool $notification_sent): self {
        $this->notification_sent = $notification_sent;
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
    
    public function getGiver(): ?User {
        return $this->giver;
    }
    
    public function setGiver(?User $giver): self {
        $this->giver = $giver;
        if ($giver) {
            $this->giver_id = $giver->getId();
        }
        return $this;
    }
    
    public function getReceiver(): ?User {
        return $this->receiver;
    }
    
    public function setReceiver(?User $receiver): self {
        $this->receiver = $receiver;
        if ($receiver) {
            $this->receiver_id = $receiver->getId();
        }
        return $this;
    }
    
    public function toArray(): array {
        return [
            'id' => $this->id,
            'group_id' => $this->group_id,
            'giver_id' => $this->giver_id,
            'receiver_id' => $this->receiver_id,
            'created_at' => $this->created_at ?? null,
            'notification_sent' => $this->notification_sent
        ];
    }
}