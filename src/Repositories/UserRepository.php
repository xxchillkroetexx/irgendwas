<?php

namespace SecretSanta\Repositories;

use SecretSanta\Database\DataMapper;
use SecretSanta\Models\User;

/**
 * Repository class for handling User data operations
 * 
 * Manages database interactions for user entities including CRUD operations,
 * authentication, and password reset functionality.
 */
class UserRepository extends DataMapper
{
    /**
     * Database table name
     * 
     * @var string
     */
    protected string $table = 'users';

    /**
     * Entity class name
     * 
     * @var string
     */
    protected string $entityClass = User::class;

    /**
     * Available database columns
     * 
     * @var array
     */
    protected array $columns = [
        'id',
        'email',
        'password',
        'name',
        'created_at',
        'updated_at',
        'last_login',
        'reset_token',
        'reset_token_expires',
        'pending_email',
        'email_verification_token',
        'email_verification_expires'
    ];

    /**
     * Finds a user by email address
     * 
     * @param string $email Email address to search for
     * @return User|null User entity if found, null otherwise
     */
    public function findByEmail(string $email): ?User
    {
        $users = $this->findBy(['email' => $email]);
        return !empty($users) ? $users[0] : null;
    }

    /**
     * Finds a user by pending email address
     * 
     * @param string $email Pending email address to search for
     * @return User|null User entity if found, null otherwise
     */
    public function findByPendingEmail(string $email): ?User
    {
        $users = $this->findBy(['pending_email' => $email]);
        return !empty($users) ? $users[0] : null;
    }

    /**
     * Finds a user by email verification token
     * 
     * @param string $token Email verification token to search for
     * @return User|null User entity if found, null otherwise
     */
    public function findByEmailVerificationToken(string $token): ?User
    {
        $users = $this->findBy(['email_verification_token' => $token]);
        return !empty($users) ? $users[0] : null;
    }

    /**
     * Finds a user by password reset token
     * 
     * @param string $token Password reset token to search for
     * @return User|null User entity if found, null otherwise
     */
    public function findByResetToken(string $token): ?User
    {
        $users = $this->findBy(['reset_token' => $token]);
        return !empty($users) ? $users[0] : null;
    }

    /**
     * Loads all groups that a user belongs to
     * 
     * @param User $user The user to load groups for
     * @return User User with groups loaded
     */
    public function loadGroups(User $user): User
    {
        if ($user->getId() === null) {
            return $user;
        }

        $groupMemberRepository = new GroupMemberRepository();
        $groupRepository = new GroupRepository();

        $memberships = $groupMemberRepository->findBy(['user_id' => $user->getId()]);
        $groups = [];

        foreach ($memberships as $membership) {
            $group = $groupRepository->find($membership->getGroupId());
            if ($group) {
                $groups[] = $group;
            }
        }

        return $user->setGroups($groups);
    }

    /**
     * Loads all wishlists created by a user
     * 
     * @param User $user The user to load wishlists for
     * @return User User with wishlists loaded
     */
    public function loadWishlists(User $user): User
    {
        if ($user->getId() === null) {
            return $user;
        }

        $wishlistRepository = new WishlistRepository();
        $wishlists = $wishlistRepository->findBy(['user_id' => $user->getId()]);

        return $user->setWishlists($wishlists);
    }

    /**
     * Authenticates a user with email and password
     * 
     * Uses constant-time comparison to prevent timing attacks. Updates the last login
     * timestamp if authentication is successful.
     * 
     * @param string $email User's email address
     * @param string $password Password to verify
     * @return User|null Authenticated user or null if authentication fails
     */
    public function authenticateUser(string $email, string $password): ?User
    {
        $user = $this->findByEmail($email);

        $dummyHash = '$2y$10$nevergonnagiveyouuppppppp$';
        if (!$user) {
            // Perform dummy verification to mitigate timing attacks
            password_verify($password, $dummyHash);
            return null;
        }

        if (!password_verify($password, $user->getPassword())) {
            return null;
        }

        // Update last login time
        $user->setLastLogin(date('Y-m-d H:i:s'));
        $this->save($user);

        return $user;
    }

    /**
     * Creates a new user account
     * 
     * Securely hashes the password before storing it in the database.
     * 
     * @param string $email User's email address
     * @param string $name User's name
     * @param string $password User's password (will be hashed)
     * @return User The created user entity with ID
     */
    public function createUser(string $email, string $name, string $password): User
    {
        $user = new User();
        $user->setEmail($email)
            ->setName($name)
            ->setPassword(password_hash($password, PASSWORD_DEFAULT));

        return $this->save($user);
    }

    /**
     * Generates a secure password reset token for a user
     * 
     * Creates a random token with 24-hour expiration and saves it to the user record.
     * 
     * @param User $user User entity to generate token for
     * @return User Updated user entity with reset token information
     * @throws \Exception If token generation or saving fails
     */
    public function generateResetToken(User $user): User
    {
        // Generate a secure random token
        $token = bin2hex(random_bytes(32));

        // Set expiration to 24 hours from now
        $expires = date('Y-m-d H:i:s', strtotime('+24 hours'));

        // Update user with token and expiration
        $user->setResetToken($token)
            ->setResetTokenExpires($expires);

        try {
            $savedUser = $this->save($user);

            // Log success for debugging
            error_log("Reset token generated for user ID " . $user->getId() . ": " . substr($token, 0, 8) . "...");

            return $savedUser;
        } catch (\Exception $e) {
            error_log("Error generating reset token: " . $e->getMessage());
            throw $e; // Re-throw to be handled by caller
        }
    }

    /**
     * Resets a user's password using a valid token
     * 
     * Validates the token and its expiration before updating the password.
     * Clears the reset token after successful password change.
     * 
     * @param string $token Reset token to validate
     * @param string $newPassword New password to set (will be hashed)
     * @return bool True if password was successfully reset, false otherwise
     */
    public function resetPassword(string $token, string $newPassword): bool
    {
        $user = $this->findByResetToken($token);

        if (!$user) {
            error_log("Reset password failed: Token not found: " . substr($token, 0, 8) . "...");
            return false;
        }

        // Check if token has expired
        if ($user->getResetTokenExpires() === null || strtotime($user->getResetTokenExpires()) < time()) {
            error_log("Reset password failed: Token expired for user ID " . $user->getId());
            return false;
        }

        // Update user password and clear reset token
        $user->setPassword(password_hash($newPassword, PASSWORD_DEFAULT))
            ->setResetToken(null)
            ->setResetTokenExpires(null);

        try {
            $this->save($user);
            error_log("Password reset successful for user ID " . $user->getId());
            return true;
        } catch (\Exception $e) {
            error_log("Error saving user after password reset: " . $e->getMessage());
            return false;
        }
    }

    /**
     * Initiates email change by generating a verification token
     * 
     * @param User $user User entity
     * @param string $newEmail New email address to verify
     * @return User Updated user entity with verification token
     * @throws \Exception If token generation or saving fails
     */
    public function initiateEmailChange(User $user, string $newEmail): User
    {
        // Generate a secure random token
        $token = bin2hex(random_bytes(32));

        // Set expiration to 24 hours from now
        $expires = date('Y-m-d H:i:s', strtotime('+24 hours'));

        // Update user with pending email and verification token
        $user->setPendingEmail($newEmail)
            ->setEmailVerificationToken($token)
            ->setEmailVerificationExpires($expires);

        try {
            $savedUser = $this->save($user);
            error_log("Email verification token generated for user ID " . $user->getId());
            return $savedUser;
        } catch (\Exception $e) {
            error_log("Error generating email verification token: " . $e->getMessage());
            throw $e;
        }
    }

    /**
     * Verifies and applies email change using a valid token
     * 
     * @param string $token Email verification token
     * @return bool True if email was successfully changed, false otherwise
     */
    public function verifyEmailChange(string $token): bool
    {
        $user = $this->findByEmailVerificationToken($token);

        if (!$user) {
            error_log("Email verification failed: Token not found: " . substr($token, 0, 8) . "...");
            return false;
        }

        // Check if token has expired
        if ($user->getEmailVerificationExpires() === null || strtotime($user->getEmailVerificationExpires()) < time()) {
            error_log("Email verification failed: Token expired for user ID " . $user->getId());
            return false;
        }

        // Check if pending email is still available
        if (!$user->getPendingEmail()) {
            error_log("Email verification failed: No pending email for user ID " . $user->getId());
            return false;
        }

        // Check if the new email is already taken by another user
        $existingUser = $this->findByEmail($user->getPendingEmail());
        if ($existingUser && $existingUser->getId() !== $user->getId()) {
            error_log("Email verification failed: Email already taken for user ID " . $user->getId());
            return false;
        }

        // Apply the email change
        $user->setEmail($user->getPendingEmail())
            ->setPendingEmail(null)
            ->setEmailVerificationToken(null)
            ->setEmailVerificationExpires(null);

        try {
            $this->save($user);
            error_log("Email successfully changed for user ID " . $user->getId());
            return true;
        } catch (\Exception $e) {
            error_log("Error saving user after email verification: " . $e->getMessage());
            return false;
        }
    }
}
