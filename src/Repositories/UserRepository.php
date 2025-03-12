<?php

namespace SecretSanta\Repositories;

use SecretSanta\Database\DataMapper;
use SecretSanta\Models\User;

class UserRepository extends DataMapper
{
    protected string $table = 'users';
    protected string $entityClass = User::class;
    protected array $columns = [
        'id',
        'email',
        'password',
        'name',
        'created_at',
        'updated_at',
        'last_login',
        'reset_token',
        'reset_token_expires'
    ];

    public function findByEmail(string $email): ?User
    {
        $users = $this->findBy(['email' => $email]);
        return !empty($users) ? $users[0] : null;
    }

    public function findByResetToken(string $token): ?User
    {
        $users = $this->findBy(['reset_token' => $token]);
        return !empty($users) ? $users[0] : null;
    }

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

    public function loadWishlists(User $user): User
    {
        if ($user->getId() === null) {
            return $user;
        }

        $wishlistRepository = new WishlistRepository();
        $wishlists = $wishlistRepository->findBy(['user_id' => $user->getId()]);

        return $user->setWishlists($wishlists);
    }

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

    public function createUser(string $email, string $name, string $password): User
    {
        $user = new User();
        $user->setEmail($email)
            ->setName($name)
            ->setPassword(password_hash($password, PASSWORD_DEFAULT));

        return $this->save($user);
    }

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
}
