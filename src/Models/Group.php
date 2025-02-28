<?php

namespace SecretSanta\Models;

class Group {
    private ?int $id = null;
    private string $name;
    private ?string $description = null;
    private int $admin_id;
    private string $invitation_code;
    private ?string $registration_deadline = null;
    private ?string $draw_date = null;
    private bool $is_drawn = false;
    private string $created_at;
    private string $updated_at;
    
    // Lazy-loaded relationships
    private ?User $admin = null;
    private array $members = [];
    private array $assignments = [];
    private array $exclusionRules = [];
    
    public function __construct(array $data = []) {
        if (!empty($data)) {
            $this->hydrate($data);
        }
    }
    
    public function hydrate(array $data): void {
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
    
    public function getId(): ?int {
        return $this->id;
    }
    
    public function getName(): string {
        return $this->name;
    }
    
    public function setName(string $name): self {
        $this->name = $name;
        return $this;
    }
    
    public function getDescription(): ?string {
        return $this->description;
    }
    
    public function setDescription(?string $description): self {
        $this->description = $description;
        return $this;
    }
    
    public function getAdminId(): int {
        return $this->admin_id;
    }
    
    public function setAdminId(int $admin_id): self {
        $this->admin_id = $admin_id;
        return $this;
    }
    
    public function getInvitationCode(): string {
        return $this->invitation_code;
    }
    
    public function setInvitationCode(string $invitation_code): self {
        $this->invitation_code = $invitation_code;
        return $this;
    }
    
    public function getRegistrationDeadline(): ?string {
        return $this->registration_deadline;
    }
    
    public function setRegistrationDeadline(?string $registration_deadline): self {
        $this->registration_deadline = $registration_deadline;
        return $this;
    }
    
    public function getDrawDate(): ?string {
        return $this->draw_date;
    }
    
    public function setDrawDate(?string $draw_date): self {
        $this->draw_date = $draw_date;
        return $this;
    }
    
    public function isDrawn(): bool {
        return $this->is_drawn;
    }
    
    public function setIsDrawn(bool $is_drawn): self {
        $this->is_drawn = $is_drawn;
        return $this;
    }
    
    public function getCreatedAt(): string {
        return $this->created_at;
    }
    
    public function getUpdatedAt(): string {
        return $this->updated_at;
    }
    
    public function getAdmin(): ?User {
        return $this->admin;
    }
    
    public function setAdmin(?User $admin): self {
        $this->admin = $admin;
        if ($admin) {
            $this->admin_id = $admin->getId();
        }
        return $this;
    }
    
    public function getMembers(): array {
        return $this->members;
    }
    
    public function setMembers(array $members): self {
        $this->members = $members;
        return $this;
    }
    
    public function getAssignments(): array {
        return $this->assignments;
    }
    
    public function setAssignments(array $assignments): self {
        $this->assignments = $assignments;
        return $this;
    }
    
    public function getExclusionRules(): array {
        return $this->exclusionRules;
    }
    
    public function setExclusionRules(array $exclusionRules): self {
        $this->exclusionRules = $exclusionRules;
        return $this;
    }
    
    public function toArray(): array {
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