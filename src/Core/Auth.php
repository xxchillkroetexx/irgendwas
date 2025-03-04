<?php

namespace SecretSanta\Core;

use SecretSanta\Models\User;
use SecretSanta\Repositories\UserRepository;

class Auth {
    private static ?self $instance = null;
    private Session $session;
    private ?User $user = null;
    
    private function __construct() {
        $this->session = Session::getInstance();
    }
    
    public static function getInstance(): self {
        if (self::$instance === null) {
            self::$instance = new self();
        }
        
        return self::$instance;
    }
    
    public function login(string $email, string $password): bool {
        $userRepository = new UserRepository();
        $user = $userRepository->authenticateUser($email, $password);
        
        if (!$user) {
            return false;
        }
        
        $this->session->regenerate();
        $this->session->set('user_id', $user->getId());
        $this->user = $user;
        
        return true;
    }
    
    public function logout(): void {
        $this->session->remove('user_id');
        $this->session->regenerate();
        $this->user = null;
    }
    
    public function register(string $email, string $name, string $password): ?User {
        $userRepository = new UserRepository();
        
        // Check if user already exists
        if ($userRepository->findByEmail($email)) {
            return null;
        }
        
        $user = $userRepository->createUser($email, $name, $password);
        
        if (!$user) {
            return null;
        }
        
        $this->session->regenerate();
        $this->user = $user;
        
        return $user;
    }
    
    public function check(): bool {
        return $this->session->has('user_id');
    }
    
    public function user(): ?User {
        if ($this->user !== null) {
            return $this->user;
        }
        
        if (!$this->check()) {
            return null;
        }
        
        $userRepository = new UserRepository();
        $this->user = $userRepository->find($this->session->get('user_id'));
        
        return $this->user;
    }
    
    public function userId(): ?int {
        return $this->session->get('user_id');
    }
    
    public function requestPasswordReset(string $email): bool {
        $userRepository = new UserRepository();
        $user = $userRepository->findByEmail($email);
        
        if (!$user) {
            return false;
        }
        
        // Generate reset token
        $user = $userRepository->generateResetToken($user);
        
        // Send email with reset token
        $emailService = new \SecretSanta\Services\EmailService();
        return $emailService->sendPasswordReset($user, $user->getResetToken());
    }
    
    public function resetPassword(string $token, string $newPassword): bool {
        $userRepository = new UserRepository();
        return $userRepository->resetPassword($token, $newPassword);
    }
}