<?php

namespace SecretSanta\Models;

/**
 * Class User
 * 
 * Represents a user in the Secret Santa application
 */
class User
{
    /**
     * @var int|null Unique identifier of the user
     */
    private ?int $id = null;

    /**
     * @var string User's email address (used for login)
     */
    private string $email;

    /**
     * @var string User's hashed password
     */
    private string $password;

    /**
     * @var string User's display name
     */
    private string $name;

    /**
     * @var string Timestamp when the user account was created
     */
    private string $created_at;

    /**
     * @var string Timestamp when the user account was last updated
     */
    private string $updated_at;

    /**
     * @var string|null Timestamp of the user's last login
     */
    private ?string $last_login = null;

    /**
     * @var string|null Token for password reset
     */
    private ?string $reset_token = null;

    /**
     * @var string|null Expiration timestamp for the reset token
     */
    private ?string $reset_token_expires = null;

    // Lazy-loaded relationships
    /**
     * @var array Groups the user belongs to
     */
    private array $groups = [];

    /**
     * @var array Wishlists belonging to the user
     */
    private array $wishlists = [];

    /**
     * Constructor for the User class
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
        if (isset($data['email'])) $this->email = $data['email'];
        if (isset($data['password'])) $this->password = $data['password'];
        if (isset($data['name'])) $this->name = $data['name'];
        if (isset($data['created_at'])) $this->created_at = $data['created_at'];
        if (isset($data['updated_at'])) $this->updated_at = $data['updated_at'];
        if (isset($data['last_login'])) $this->last_login = $data['last_login'];
        if (isset($data['reset_token'])) $this->reset_token = $data['reset_token'];
        if (isset($data['reset_token_expires'])) $this->reset_token_expires = $data['reset_token_expires'];
    }

    /**
     * Gets the user's ID
     * 
     * @return int|null
     */
    public function getId(): ?int
    {
        return $this->id;
    }

    /**
     * Gets the user's email
     * 
     * @return string
     */
    public function getEmail(): string
    {
        return $this->email;
    }

    /**
     * Sets the user's email
     * 
     * @param string $email
     * @return self
     */
    public function setEmail(string $email): self
    {
        $this->email = $email;
        return $this;
    }

    /**
     * Gets the user's password
     * 
     * @return string
     */
    public function getPassword(): string
    {
        return $this->password;
    }

    /**
     * Sets the user's password
     * 
     * @param string $password
     * @return self
     */
    public function setPassword(string $password): self
    {
        $this->password = $password;
        return $this;
    }

    /**
     * Gets the user's name
     * 
     * @return string
     */
    public function getName(): string
    {
        return $this->name;
    }

    /**
     * Sets the user's name
     * 
     * @param string $name
     * @return self
     */
    public function setName(string $name): self
    {
        $this->name = $name;
        return $this;
    }

    /**
     * Gets the timestamp when the user account was created
     * 
     * @return string
     */
    public function getCreatedAt(): string
    {
        return $this->created_at;
    }

    /**
     * Gets the timestamp when the user account was last updated
     * 
     * @return string
     */
    public function getUpdatedAt(): string
    {
        return $this->updated_at;
    }

    /**
     * Gets the timestamp of the user's last login
     * 
     * @return string|null
     */
    public function getLastLogin(): ?string
    {
        return $this->last_login;
    }

    /**
     * Sets the timestamp of the user's last login
     * 
     * @param string|null $last_login
     * @return self
     */
    public function setLastLogin(?string $last_login): self
    {
        $this->last_login = $last_login;
        return $this;
    }

    /**
     * Gets the token for password reset
     * 
     * @return string|null
     */
    public function getResetToken(): ?string
    {
        return $this->reset_token;
    }

    /**
     * Sets the token for password reset
     * 
     * @param string|null $reset_token
     * @return self
     */
    public function setResetToken(?string $reset_token): self
    {
        $this->reset_token = $reset_token;
        return $this;
    }

    /**
     * Gets the expiration timestamp for the reset token
     * 
     * @return string|null
     */
    public function getResetTokenExpires(): ?string
    {
        return $this->reset_token_expires;
    }

    /**
     * Sets the expiration timestamp for the reset token
     * 
     * @param string|null $reset_token_expires
     * @return self
     */
    public function setResetTokenExpires(?string $reset_token_expires): self
    {
        $this->reset_token_expires = $reset_token_expires;
        return $this;
    }

    /**
     * Sets the groups the user belongs to
     * 
     * @param array $groups
     * @return self
     */
    public function setGroups(array $groups): self
    {
        $this->groups = $groups;
        return $this;
    }

    /**
     * Gets the groups the user belongs to
     * 
     * @return array
     */
    public function getGroups(): array
    {
        return $this->groups;
    }

    /**
     * Sets the wishlists belonging to the user
     * 
     * @param array $wishlists
     * @return self
     */
    public function setWishlists(array $wishlists): self
    {
        $this->wishlists = $wishlists;
        return $this;
    }

    /**
     * Gets the wishlists belonging to the user
     * 
     * @return array
     */
    public function getWishlists(): array
    {
        return $this->wishlists;
    }

    /**
     * Converts the user object to an associative array
     * 
     * @return array
     */
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
