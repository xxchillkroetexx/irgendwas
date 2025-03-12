<?php

namespace SecretSanta\Models;

class User
{
    private ?int $id = null;
    private string $email;
    private string $password;
    private string $name;
    private string $created_at;
    private string $updated_at;
    private ?string $last_login = null;
    private ?string $reset_token = null;
    private ?string $reset_token_expires = null;

    // Lazy-loaded relationships
    private array $groups = [];
    private array $wishlists = [];

    public function __construct(array $data = [])
    {
        if (!empty($data)) {
            $this->hydrate($data);
        }
    }

    public function hydrate(array $data): void
    {
        if (isset($data['id'])) $this->id = (int) $data['id'];
        if (isset($data['email'])) $this->email = $data['email'];
        if (isset($data['password'])) $this->password = $data['password'];
        if (isset($data['name'])) $this->name = $data['name'];
        if (isset($data['created_at'])) $this->created_at = $data['created_at'];
        if (isset($data['updated_at'])) $this->updated_at = $data['updated_at'];
        if (isset($data['last_login'])) $this->last_login = $data['last_login'];
        if (isset($data['reset_token'])) $this->reset_token = $data['reset_token'];
        if (isset($data['reset_token_expires'])) $this->reset_token_expires = $data['reset_token_expires'];
    }

    public function getId(): ?int
    {
        return $this->id;
    }

    public function getEmail(): string
    {
        return $this->email;
    }

    public function setEmail(string $email): self
    {
        $this->email = $email;
        return $this;
    }

    public function getPassword(): string
    {
        return $this->password;
    }

    public function setPassword(string $password): self
    {
        $this->password = $password;
        return $this;
    }

    public function getName(): string
    {
        return $this->name;
    }

    public function setName(string $name): self
    {
        $this->name = $name;
        return $this;
    }

    public function getCreatedAt(): string
    {
        return $this->created_at;
    }

    public function getUpdatedAt(): string
    {
        return $this->updated_at;
    }

    public function getLastLogin(): ?string
    {
        return $this->last_login;
    }

    public function setLastLogin(?string $last_login): self
    {
        $this->last_login = $last_login;
        return $this;
    }

    public function getResetToken(): ?string
    {
        return $this->reset_token;
    }

    public function setResetToken(?string $reset_token): self
    {
        $this->reset_token = $reset_token;
        return $this;
    }

    public function getResetTokenExpires(): ?string
    {
        return $this->reset_token_expires;
    }

    public function setResetTokenExpires(?string $reset_token_expires): self
    {
        $this->reset_token_expires = $reset_token_expires;
        return $this;
    }

    public function setGroups(array $groups): self
    {
        $this->groups = $groups;
        return $this;
    }

    public function getGroups(): array
    {
        return $this->groups;
    }

    public function setWishlists(array $wishlists): self
    {
        $this->wishlists = $wishlists;
        return $this;
    }

    public function getWishlists(): array
    {
        return $this->wishlists;
    }

    public function toArray(): array
    {
        return [
            'id' => $this->id,
            'email' => $this->email,
            'password' => $this->password,
            'name' => $this->name,
            'created_at' => $this->created_at ?? null,
            'updated_at' => $this->updated_at ?? null,
            'last_login' => $this->last_login,
            'reset_token' => $this->reset_token,
            'reset_token_expires' => $this->reset_token_expires
        ];
    }
}
