<?php

namespace SecretSanta\Models;

/**
 * Gift Assignment Model
 * 
 * Represents the relationship between a gift giver and receiver in a Secret Santa group.
 * Tracks who gives a gift to whom and the notification status.
 * 
 * @class GiftAssignment
 * @package SecretSanta\Models
 */
class GiftAssignment
{
    /**
     * @var ?int The unique identifier of the assignment, null if not yet saved
     */
    private ?int $id = null;
    
    /**
     * @var int The ID of the group this assignment belongs to
     */
    private int $group_id;
    
    /**
     * @var int The user ID of the gift giver
     */
    private int $giver_id;
    
    /**
     * @var int The user ID of the gift receiver
     */
    private int $receiver_id;
    
    /**
     * @var string Timestamp when the assignment was created
     */
    private string $created_at;
    
    /**
     * @var bool Whether a notification has been sent to the giver
     */
    private bool $notification_sent = false;

    // Lazy-loaded relationships
    /**
     * @var ?Group The group this assignment belongs to
     */
    private ?Group $group = null;
    
    /**
     * @var ?User The user who is giving the gift
     */
    private ?User $giver = null;
    
    /**
     * @var ?User The user who is receiving the gift
     */
    private ?User $receiver = null;

    /**
     * Constructor for the GiftAssignment model
     * 
     * @param array $data Initial data to populate the model
     */
    public function __construct(array $data = [])
    {
        if (!empty($data)) {
            $this->hydrate($data);
        }
    }

    /**
     * Populates the model with data from an array
     * 
     * @param array $data The data to populate the model with
     * @return void
     */
    public function hydrate(array $data): void
    {
        if (isset($data['id'])) $this->id = (int) $data['id'];
        if (isset($data['group_id'])) $this->group_id = (int) $data['group_id'];
        if (isset($data['giver_id'])) $this->giver_id = (int) $data['giver_id'];
        if (isset($data['receiver_id'])) $this->receiver_id = (int) $data['receiver_id'];
        if (isset($data['created_at'])) $this->created_at = $data['created_at'];
        if (isset($data['notification_sent'])) $this->notification_sent = (bool) $data['notification_sent'];
    }

    /**
     * Get the assignment ID
     * 
     * @return ?int The assignment ID or null if not yet saved
     */
    public function getId(): ?int
    {
        return $this->id;
    }

    /**
     * Get the group ID
     * 
     * @return int The ID of the group this assignment belongs to
     */
    public function getGroupId(): int
    {
        return $this->group_id;
    }

    /**
     * Set the group ID
     * 
     * @param int $group_id The new group ID
     * @return self For method chaining
     */
    public function setGroupId(int $group_id): self
    {
        $this->group_id = $group_id;
        return $this;
    }

    /**
     * Get the giver user ID
     * 
     * @return int The ID of the user giving the gift
     */
    public function getGiverId(): int
    {
        return $this->giver_id;
    }

    /**
     * Set the giver user ID
     * 
     * @param int $giver_id The new giver user ID
     * @return self For method chaining
     */
    public function setGiverId(int $giver_id): self
    {
        $this->giver_id = $giver_id;
        return $this;
    }

    /**
     * Get the receiver user ID
     * 
     * @return int The ID of the user receiving the gift
     */
    public function getReceiverId(): int
    {
        return $this->receiver_id;
    }

    /**
     * Set the receiver user ID
     * 
     * @param int $receiver_id The new receiver user ID
     * @return self For method chaining
     */
    public function setReceiverId(int $receiver_id): self
    {
        $this->receiver_id = $receiver_id;
        return $this;
    }

    /**
     * Get the creation timestamp
     * 
     * @return string The timestamp when the assignment was created
     */
    public function getCreatedAt(): string
    {
        return $this->created_at;
    }

    /**
     * Check if notification has been sent to the giver
     * 
     * @return bool True if notification has been sent, false otherwise
     */
    public function isNotificationSent(): bool
    {
        return $this->notification_sent;
    }

    /**
     * Set the notification status
     * 
     * @param bool $notification_sent Whether the notification has been sent
     * @return self For method chaining
     */
    public function setNotificationSent(bool $notification_sent): self
    {
        $this->notification_sent = $notification_sent;
        return $this;
    }

    /**
     * Get the group object
     * 
     * @return ?Group The group this assignment belongs to or null if not loaded
     */
    public function getGroup(): ?Group
    {
        return $this->group;
    }

    /**
     * Set the group object
     * 
     * @param ?Group $group The group object
     * @return self For method chaining
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
     * Get the giver user object
     * 
     * @return ?User The user giving the gift or null if not loaded
     */
    public function getGiver(): ?User
    {
        return $this->giver;
    }

    /**
     * Set the giver user object
     * 
     * @param ?User $giver The user giving the gift
     * @return self For method chaining
     */
    public function setGiver(?User $giver): self
    {
        $this->giver = $giver;
        if ($giver) {
            $this->giver_id = $giver->getId();
        }
        return $this;
    }

    /**
     * Get the receiver user object
     * 
     * @return ?User The user receiving the gift or null if not loaded
     */
    public function getReceiver(): ?User
    {
        return $this->receiver;
    }

    /**
     * Set the receiver user object
     * 
     * @param ?User $receiver The user receiving the gift
     * @return self For method chaining
     */
    public function setReceiver(?User $receiver): self
    {
        $this->receiver = $receiver;
        if ($receiver) {
            $this->receiver_id = $receiver->getId();
        }
        return $this;
    }

    /**
     * Convert the model to an array
     * 
     * @return array The assignment data as an associative array
     */
    public function toArray(): array
    {
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
