<?php

namespace SecretSanta\Models;

/**
 * Exclusion Rule Model
 * 
 * Represents a rule that prevents a specific user from being assigned as the gift receiver
 * for another specific user in a Secret Santa group. Used to handle constraints like
 * family members not giving gifts to each other.
 * 
 * @class ExclusionRule
 * @package SecretSanta\Models
 */
class ExclusionRule
{
    /**
     * @var ?int The unique identifier of the rule, null if not yet saved
     */
    private ?int $id = null;
    
    /**
     * @var int The ID of the group this rule applies to
     */
    private int $group_id;
    
    /**
     * @var int The ID of the user who should not give a gift
     */
    private int $user_id;
    
    /**
     * @var int The ID of the user who should not receive a gift from user_id
     */
    private int $excluded_user_id;
    
    /**
     * @var string Timestamp when the rule was created
     */
    private string $created_at;

    // Lazy-loaded relationships
    /**
     * @var ?Group The group this rule belongs to
     */
    private ?Group $group = null;
    
    /**
     * @var ?User The user who should not give a gift to the excluded user
     */
    private ?User $user = null;
    
    /**
     * @var ?User The user who should not receive a gift from the user
     */
    private ?User $excludedUser = null;

    /**
     * Constructor for the ExclusionRule model
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
        if (isset($data['user_id'])) $this->user_id = (int) $data['user_id'];
        if (isset($data['excluded_user_id'])) $this->excluded_user_id = (int) $data['excluded_user_id'];
        if (isset($data['created_at'])) $this->created_at = $data['created_at'];
    }

    /**
     * Get the rule ID
     * 
     * @return ?int The rule ID or null if not yet saved
     */
    public function getId(): ?int
    {
        return $this->id;
    }

    /**
     * Get the group ID
     * 
     * @return int The ID of the group this rule applies to
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
     * Get the user ID
     * 
     * @return int The ID of the user who should not give a gift to the excluded user
     */
    public function getUserId(): int
    {
        return $this->user_id;
    }

    /**
     * Set the user ID
     * 
     * @param int $user_id The new user ID
     * @return self For method chaining
     */
    public function setUserId(int $user_id): self
    {
        $this->user_id = $user_id;
        return $this;
    }

    /**
     * Get the excluded user ID
     * 
     * @return int The ID of the user who should not receive a gift from the user
     */
    public function getExcludedUserId(): int
    {
        return $this->excluded_user_id;
    }

    /**
     * Set the excluded user ID
     * 
     * @param int $excluded_user_id The new excluded user ID
     * @return self For method chaining
     */
    public function setExcludedUserId(int $excluded_user_id): self
    {
        $this->excluded_user_id = $excluded_user_id;
        return $this;
    }

    /**
     * Get the creation timestamp
     * 
     * @return string The timestamp when the rule was created
     */
    public function getCreatedAt(): string
    {
        return $this->created_at;
    }

    /**
     * Get the group object
     * 
     * @return ?Group The group this rule belongs to or null if not loaded
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
     * Get the user object
     * 
     * @return ?User The user who should not give a gift to the excluded user
     */
    public function getUser(): ?User
    {
        return $this->user;
    }

    /**
     * Set the user object
     * 
     * @param ?User $user The user object
     * @return self For method chaining
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
     * Get the excluded user object
     * 
     * @return ?User The user who should not receive a gift from the user
     */
    public function getExcludedUser(): ?User
    {
        return $this->excludedUser;
    }

    /**
     * Set the excluded user object
     * 
     * @param ?User $user The excluded user object
     * @return self For method chaining
     */
    public function setExcludedUser(?User $user): self
    {
        $this->excludedUser = $user;
        if ($user) {
            $this->excluded_user_id = $user->getId();
        }
        return $this;
    }

    /**
     * Convert the model to an array
     * 
     * @return array The rule data as an associative array
     */
    public function toArray(): array
    {
        return [
            'id' => $this->id,
            'group_id' => $this->group_id,
            'user_id' => $this->user_id,
            'excluded_user_id' => $this->excluded_user_id,
            'created_at' => $this->created_at ?? null
        ];
    }
}
