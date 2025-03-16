<?php

namespace SecretSanta\Core;

use SecretSanta\Models\User;
use SecretSanta\Repositories\UserRepository;

/**
 * Auth class handles user authentication, registration, and authorization.
 *
 * This singleton class provides methods for user login, logout, registration,
 * and password reset functionality.
 */
class Auth
{
    /** @var self|null Singleton instance of the Auth class */
    private static ?self $instance = null;
    
    /** @var Session The session instance */
    private Session $session;
    
    /** @var User|null Currently authenticated user */
    private ?User $user = null;

    /**
     * Private constructor to enforce singleton pattern.
     * 
     * Initializes the session instance.
     */
    private function __construct()
    {
        $this->session = Session::getInstance();
    }

    /**
     * Gets the singleton instance of the Auth class.
     * 
     * @return self The Auth instance
     */
    public static function getInstance(): self
    {
        if (self::$instance === null) {
            self::$instance = new self();
        }

        return self::$instance;
    }

    /**
     * Attempts to log in a user with the given credentials.
     * 
     * @param string $email User's email address
     * @param string $password User's password
     * @return bool True if login was successful, false otherwise
     */
    public function login(string $email, string $password): bool
    {
        $userRepository = new UserRepository();
        $user = $userRepository->authenticateUser($email, $password);

        if (!$user) {
            return false;
        }

        // Store last login time in session flash to show next time
        if ($user->getLastLogin() !== null && $user->getLastLogin() !== '') {
            $this->session->setFlash('last_login', $user->getLastLogin());
        }

        $this->session->set('user_id', $user->getId());
        $this->session->regenerate();
        $this->user = $user;

        return true;
    }

    /**
     * Logs out the current user.
     * 
     * Removes user from session and regenerates session ID.
     */
    public function logout(): void
    {
        $this->session->remove('user_id');
        $this->session->regenerate();
        $this->user = null;
    }

    /**
     * Registers a new user.
     * 
     * @param string $email User's email address
     * @param string $name User's name
     * @param string $password User's password
     * @return User|null The created user or null if registration failed
     */
    public function register(string $email, string $name, string $password): ?User
    {
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

    /**
     * Checks if a user is currently authenticated.
     * 
     * @return bool True if a user is logged in, false otherwise
     */
    public function check(): bool
    {
        if ($this->session->has('user_id')) {
            $this->session->set('last_activity', time());
            return true;
        }
        return false;
    }

    /**
     * Gets the currently authenticated user.
     * 
     * @return User|null The current user or null if not authenticated
     */
    public function user(): ?User
    {
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

    /**
     * Gets the ID of the currently authenticated user.
     * 
     * @return int|null The user ID or null if not authenticated
     */
    public function userId(): ?int
    {
        return $this->session->get('user_id');
    }

    /**
     * Initiates a password reset process for a user.
     * 
     * Generates a reset token and sends a password reset email.
     * 
     * @param string $email The email address of the user
     * @return bool True if the reset process was initiated, false otherwise
     */
    public function requestPasswordReset(string $email): bool
    {
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

    /**
     * Resets a user's password using a reset token.
     * 
     * @param string $token The password reset token
     * @param string $newPassword The new password
     * @return bool True if the password was successfully reset, false otherwise
     */
    public function resetPassword(string $token, string $newPassword): bool
    {
        $userRepository = new UserRepository();
        return $userRepository->resetPassword($token, $newPassword);
    }
}
