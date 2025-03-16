<?php

namespace SecretSanta\Models;

/**
 * Group Model
 * 
 * Represents a Secret Santa group with members, admins, and assignment information.
 * Manages the group data, relationships, and state needed for the Secret Santa process.
 * 
 * @class Group
 * @package SecretSanta\Models
 */
class Group
{
    /**
     * @var ?int The unique identifier of the group, null if not yet saved
     */
    private ?int $id = null;
    
    /**
     * @var string The name of the Secret Santa group
     */
    private string $name;
    
    /**
     * @var ?string An optional description of the group
     */
    private ?string $description = null;
    
    /**
     * @var int The user ID of the group administrator
     */
    private int $admin_id;
    
    /**
     * @var string Unique code used to invite members to the group
     */
    private string $invitation_code;
    
    /**
     * @var ?string The deadline for registration in ISO date format
     */
    private ?string $registration_deadline = null;
    
    /**
     * @var ?string The date when names will be drawn in ISO date format
     */
    private ?string $draw_date = null;
    
    /**
     * @var bool Indicates whether the drawing has been completed
     */
    private bool $is_drawn = false;
    
    /**
     * @var ?string Timestamp when the group was created
     */
    private ?string $created_at = null;
    
    /**
     * @var ?string Timestamp when the group was last updated
     */
    private ?string $updated_at = null;

    // Lazy-loaded relationships
    /**
     * @var ?User The admin user object, lazy-loaded when needed
     */
    private ?User $admin = null;
    
    /**
     * @var array List of User objects who are members of this group
     */
    private array $members = [];
    
    /**
     * @var array List of GiftAssignment objects for this group
     */
    private array $assignments = [];
    
    /**
     * @var array List of ExclusionRule objects for this group
     */
    private array $exclusionRules = [];

    /**
     * Constructor for the Group model
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
        if (isset($data['name'])) $this->name = $data['name'];
        if (isset($data['description'])) $this->description = $data['description'];
        if (isset($data['admin_id'])) $this->admin_id = (int) $data['admin_id'];
        if (isset($data['invitation_code'])) $this->invitation_code = $data['invitation_code'];
        if (isset($data['registration_deadline'])) $this->registration_deadline = $data['registration_deadline'];
        if (isset($data['draw_date'])) $this->draw_date = $data['draw_date'];
        if (isset($data['is_drawn'])) $this->is_drawn = (bool) $data['is_drawn'];
        if (isset($data['created_at'])) $this->created_at = $data['created_at'];
        if (isset($data['updated_at'])) $this->updated_at = $data['updated_at'];
    }

    /**
     * Get the group ID
     * 
     * @return ?int The group ID or null if not yet saved
     */
    public function getId(): ?int
    {
        return $this->id;
    }

    /**
     * Get the group name
     * 
     * @return string The name of the group
     */
    public function getName(): string
    {
        return $this->name;
    }

    /**
     * Set the group name
     * 
     * @param string $name The new name for the group
     * @return self For method chaining
     */
    public function setName(string $name): self
    {
        $this->name = $name;
        return $this;
    }

    /**
     * Get the group description
     * 
     * @return ?string The description or null if not set
     */
    public function getDescription(): ?string
    {
        return $this->description;
    }

    /**
     * Set the group description
     * 
     * @param ?string $description The new description for the group
     * @return self For method chaining
     */
    public function setDescription(?string $description): self
    {
        $this->description = $description;
        return $this;
    }

    /**
     * Get the admin user ID
     * 
     * @return int The admin user ID
     */
    public function getAdminId(): int
    {
        return $this->admin_id;
    }

    /**
     * Set the admin user ID
     * 
     * @param int $admin_id The new admin user ID
     * @return self For method chaining
     */
    public function setAdminId(int $admin_id): self
    {
        $this->admin_id = $admin_id;
        return $this;
    }

    /**
     * Get the invitation code for the group
     * 
     * @return string The invitation code
     */
    public function getInvitationCode(): string
    {
        return $this->invitation_code;
    }

    /**
     * Set the invitation code for the group
     * 
     * @param string $invitation_code The new invitation code
     * @return self For method chaining
     */
    public function setInvitationCode(string $invitation_code): self
    {
        $this->invitation_code = $invitation_code;
        return $this;
    }

    /**
     * Get the registration deadline
     * 
     * @return ?string The registration deadline in ISO date format or null if not set
     */
    public function getRegistrationDeadline(): ?string
    {
        return $this->registration_deadline;
    }

    /**
     * Set the registration deadline
     * 
     * @param ?string $registration_deadline The new registration deadline in ISO date format
     * @return self For method chaining
     */
    public function setRegistrationDeadline(?string $registration_deadline): self
    {
        $this->registration_deadline = $registration_deadline;
        return $this;
    }

    /**
     * Get the draw date
     * 
     * @return ?string The draw date in ISO date format or null if not set
     */
    public function getDrawDate(): ?string
    {
        return $this->draw_date;
    }

    /**
     * Set the draw date
     * 
     * @param ?string $draw_date The new draw date in ISO date format
     * @return self For method chaining
     */
    public function setDrawDate(?string $draw_date): self
    {
        $this->draw_date = $draw_date;
        return $this;
    }

    /**
     * Check if the Secret Santa assignments have been drawn
     * 
     * @return bool True if assignments have been made, false otherwise
     */
    public function isDrawn(): bool
    {
        return $this->is_drawn;
    }

    /**
     * Set the drawn state of the group
     * 
     * @param bool $is_drawn Whether the group has been drawn
     * @return self For method chaining
     */
    public function setIsDrawn(bool $is_drawn): self
    {
        $this->is_drawn = $is_drawn;
        return $this;
    }

    /**
     * Get the creation timestamp
     * 
     * @return ?string The timestamp when the group was created
     */
    public function getCreatedAt(): ?string
    {
        return $this->created_at;
    }

    /**
     * Get the last update timestamp
     * 
     * @return ?string The timestamp when the group was last updated
     */
    public function getUpdatedAt(): ?string
    {
        return $this->updated_at;
    }

    /**
     * Get the admin user object
     * 
     * @return ?User The admin user or null if not loaded
     */
    public function getAdmin(): ?User
    {
        return $this->admin;
    }

    /**
     * Set the admin user object
     * 
     * @param ?User $admin The admin user object
     * @return self For method chaining
     */
    public function setAdmin(?User $admin): self
    {
        $this->admin = $admin;
        if ($admin) {
            $this->admin_id = $admin->getId();
        }
        return $this;
    }

    /**
     * Get all members of the group
     * 
     * @return array List of User objects who are members of this group
     */
    public function getMembers(): array
    {
        return $this->members;
    }

    /**
     * Set the members of the group
     * 
     * @param array $members List of User objects
     * @return self For method chaining
     */
    public function setMembers(array $members): self
    {
        $this->members = $members;
        return $this;
    }

    /**
     * Get all gift assignments for the group
     * 
     * @return array List of GiftAssignment objects
     */
    public function getAssignments(): array
    {
        return $this->assignments;
    }

    /**
     * Set the gift assignments for the group
     * 
     * @param array $assignments List of GiftAssignment objects
     * @return self For method chaining
     */
    public function setAssignments(array $assignments): self
    {
        $this->assignments = $assignments;
        return $this;
    }

    /**
     * Get all exclusion rules for the group
     * 
     * @return array List of ExclusionRule objects
     */
    public function getExclusionRules(): array
    {
        return $this->exclusionRules;
    }

    /**
     * Set the exclusion rules for the group
     * 
     * @param array $exclusionRules List of ExclusionRule objects
     * @return self For method chaining
     */
    public function setExclusionRules(array $exclusionRules): self
    {
        $this->exclusionRules = $exclusionRules;
        return $this;
    }

    /**
     * Convert the model to an array
     * 
     * @return array The group data as an associative array
     */
    public function toArray(): array
    {
        return [
            'id' => $this->id,
            'name' => $this->name,
            'description' => $this->description,
            'admin_id' => $this->admin_id,
            'invitation_code' => $this->invitation_code,
            'registration_deadline' => $this->registration_deadline,
            'draw_date' => $this->draw_date,
            'is_drawn' => $this->is_drawn,
            'created_at' => $this->created_at ?? null,
            'updated_at' => $this->updated_at ?? null
        ];
    }
}
