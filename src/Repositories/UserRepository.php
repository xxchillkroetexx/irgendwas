<?php

namespace SecretSanta\Repositories;

use SecretSanta\Database\DataMapper;
use SecretSanta\Models\User;

class UserRepository extends DataMapper {
    protected string $table = 'users';
    protected string $entityClass = User::class;
    protected array $columns = [
        'id', 'email', 'password', 'name', 'created_at', 'updated_at', 
        'last_login', 'reset_token', 'reset_token_expires'
    ];
    
    public function findByEmail(string $email): ?User {
        $users = $this->findBy(['email' => $email]);
        return !empty($users) ? $users[0] : null;
    }
    
    public function findByResetToken(string $token): ?User {
        $users = $this->findBy(['reset_token' => $token]);
        return !empty($users) ? $users[0] : null;
    }
    
    public function loadGroups(User $user): User {
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
    
    public function loadWishlists(User $user): User {
        if ($user->getId() === null) {
            return $user;
        }
        
        $wishlistRepository = new WishlistRepository();
        $wishlists = $wishlistRepository->findBy(['user_id' => $user->getId()]);
        
        return $user->setWishlists($wishlists);
    }
    
    public function authenticateUser(string $email, string $password): ?User {
        $user = $this->findByEmail($email);
        
        if (!$user) {
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
    
    public function createUser(string $email, string $name, string $password): User {
        $user = new User();
        $user->setEmail($email)
             ->setName($name)
             ->setPassword(password_hash($password, PASSWORD_DEFAULT));
        
        return $this->save($user);
    }
    
    public function generateResetToken(User $user): User {
        $token = bin2hex(random_bytes(32));
        $expires = date('Y-m-d H:i:s', strtotime('+24 hours'));
        
        $user->setResetToken($token)
             ->setResetTokenExpires($expires);
        
        return $this->save($user);
    }
    
    public function resetPassword(string $token, string $newPassword): bool {
        $user = $this->findByResetToken($token);
        
        if (!$user) {
            return false;
        }
        
        if (strtotime($user->getResetTokenExpires()) < time()) {
            return false;
        }
        
        $user->setPassword(password_hash($newPassword, PASSWORD_DEFAULT))
             ->setResetToken(null)
             ->setResetTokenExpires(null);
        
        $this->save($user);
        
        return true;
    }
}