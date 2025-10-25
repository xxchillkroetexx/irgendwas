<?php

namespace SecretSanta\Models;

/**
 * Class GroupMember
 * 
 * Represents a user's membership in a group
 */
class GroupMember
{
    /**
     * @var int|null Unique identifier of the group membership
     */
    private ?int $id = null;
    
    /**
     * @var int ID of the group
     */
    private int $group_id;
    
    /**
     * @var int ID of the user
     */
    private int $user_id;
    
    /**
     * @var string Timestamp when the user joined the group
     */
    private string $joined_at;

    // Lazy-loaded relationships
    /**
     * @var User|null The user who is a member of the group
     */
    private ?User $user = null;
    
    /**
     * @var Group|null The group the user belongs to
     */
    private ?Group $group = null;

    /**
     * Constructor for the GroupMember class
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
        if (isset($data['group_id'])) $this->group_id = (int) $data['group_id'];
        if (isset($data['user_id'])) $this->user_id = (int) $data['user_id'];
        if (isset($data['joined_at'])) $this->joined_at = $data['joined_at'];
    }

    /**
     * Gets the ID of the group membership
     * 
     * @return int|null The ID of the group membership
     */
    public function getId(): ?int
    {
        return $this->id;
    }

    /**
     * Gets the ID of the group
     * 
     * @return int The ID of the group
     */
    public function getGroupId(): int
    {
        return $this->group_id;
    }

    /**
     * Sets the ID of the group
     * 
     * @param int $group_id The ID of the group
     * @return self
     */
    public function setGroupId(int $group_id): self
    {
        $this->group_id = $group_id;
        return $this;
    }

    /**
     * Gets the ID of the user
     * 
     * @return int The ID of the user
     */
    public function getUserId(): int
    {
        return $this->user_id;
    }

    /**
     * Sets the ID of the user
     * 
     * @param int $user_id The ID of the user
     * @return self
     */
    public function setUserId(int $user_id): self
    {
        $this->user_id = $user_id;
        return $this;
    }

    /**
     * Gets the timestamp when the user joined the group
     * 
     * @return string The timestamp when the user joined the group
     */
    public function getJoinedAt(): string
    {
        return $this->joined_at;
    }

    /**
     * Sets the timestamp when the user joined the group
     * 
     * @param string $joined_at The timestamp when the user joined the group
     * @return self
     */
    public function setJoinedAt(string $joined_at): self
    {
        $this->joined_at = $joined_at;
        return $this;
    }

    /**
     * Gets the user who is a member of the group
     * 
     * @return User|null The user who is a member of the group
     */
    public function getUser(): ?User
    {
        return $this->user;
    }

    /**
     * Sets the user who is a member of the group
     * 
     * @param User|null $user The user who is a member of the group
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
     * Gets the group the user belongs to
     * 
     * @return Group|null The group the user belongs to
     */
    public function getGroup(): ?Group
    {
        return $this->group;
    }

    /**
     * Sets the group the user belongs to
     * 
     * @param Group|null $group The group the user belongs to
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
     * Converts the object to an associative array
     * 
     * @return array The object as an associative array
     */
    public function toArray(): array
    {
        return [
            'id' => $this->id,
            'group_id' => $this->group_id,
            'user_id' => $this->user_id,
            'joined_at' => $this->joined_at
        ];
    }
}
