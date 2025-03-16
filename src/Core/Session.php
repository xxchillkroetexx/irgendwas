<?php

namespace SecretSanta\Core;

class Session
{
    private static ?self $instance = null;
    private bool $started = false;
    private const INACTIVITY_LIMIT = 1200; // 20 minutes in seconds
    
    // Rate limiting constants
    private const MAX_LOGIN_ATTEMPTS = 5; // Maximum login attempts before lockout
    private const BASE_LOCKOUT_TIME = 300; // Base lockout time in seconds (5 minutes)
    private const LOCKOUT_MULTIPLIER = 2; // Each additional failed attempt doubles the lockout time

    // Rate limiting constants for registration
    private const MAX_REGISTRATION = 3; // Maximum registration per IP-address before lockout
    private const BASE_REGISTRATION_LOCKOUT_TIME = 600; // Base lockout time in seconds (10 minutes)
    private const REGISTRATION_LOCKOUT_MULTIPLIER = 3; // Each additional failed attempt triples the lockout time (higher grwoth rate than login lockout, as registration is less frequent)
        
    
    private function __construct()
    {
        // Don't start session in constructor - will do it on demand
    }

    public static function getInstance(): self
    {
        if (self::$instance === null) {
            self::$instance = new self();
        }

        return self::$instance;
    }

    private function start(): void
    {
        if (!$this->started && session_status() === PHP_SESSION_NONE && !headers_sent()) {
            session_start();
            $this->started = true;
            $this->checkInactivity();
        }
    }

    public function checkInactivity(): void
    {
        if (isset($_SESSION['last_activity']) && (time() - $_SESSION['last_activity']) > self::INACTIVITY_LIMIT) {
            $this->destroy();
        }
        $_SESSION['last_activity'] = time();
    }

    public function set(string $key, $value): void
    {
        $this->start();
        $_SESSION[$key] = $value;
    }

    public function get(string $key, $default = null)
    {
        $this->start();
        return $_SESSION[$key] ?? $default;
    }

    public function has(string $key): bool
    {
        $this->start();
        return isset($_SESSION[$key]);
    }

    public function remove(string $key): void
    {
        $this->start();
        if (isset($_SESSION[$key])) {
            unset($_SESSION[$key]);
        }
    }

    public function destroy(): void
    {
        $this->start();
        session_destroy();
        $_SESSION = [];
        $this->started = false;
    }

    public function setFlash(string $key, $value): void
    {
        $this->start();
        if (!isset($_SESSION['_flash'])) {
            $_SESSION['_flash'] = [];
        }

        $_SESSION['_flash'][$key] = $value;
    }

    public function getFlash(string $key, $default = null)
    {
        $this->start();
        if (!isset($_SESSION['_flash']) || !isset($_SESSION['_flash'][$key])) {
            return $default;
        }

        $value = $_SESSION['_flash'][$key];
        unset($_SESSION['_flash'][$key]);

        return $value;
    }

    public function hasFlash(string $key): bool
    {
        $this->start();
        return isset($_SESSION['_flash']) && isset($_SESSION['_flash'][$key]);
    }

    public function clearFlash(): void
    {
        $this->start();
        $_SESSION['_flash'] = [];
    }

    public function regenerate(): void
    {
        $this->start();
        session_regenerate_id(true);
    }

    public function getSessionId(): string
    {
        $this->start();
        return session_id();
    }

    public function flash(string $key, $value): void
    {
        $this->setFlash($key, $value);
    }

    public function getOldInput(string $key, $default = null)
    {
        $old = $this->getFlash('old') ?? [];
        return $old[$key] ?? $default;
    }

    /**
     * Increment the failed login attempts for a given identifier (email or IP)
     *
     * @param string $identifier The user identifier (email or IP)
     * @return int The current number of attempts
     */
    public function incrementLoginAttempts(string $identifier): int
    {
        $this->start();
        
        if (!isset($_SESSION['login_attempts'])) {
            $_SESSION['login_attempts'] = [];
        }
        
        if (!isset($_SESSION['login_attempts'][$identifier])) {
            $_SESSION['login_attempts'][$identifier] = [
                'attempts' => 0,
                'last_attempt' => 0,
                'lockout_until' => 0
            ];
        }
        
        $_SESSION['login_attempts'][$identifier]['attempts']++;
        $_SESSION['login_attempts'][$identifier]['last_attempt'] = time();
        
        // If attempts exceed threshold, set lockout timestamp
        if ($_SESSION['login_attempts'][$identifier]['attempts'] >= self::MAX_LOGIN_ATTEMPTS) {
            $lockoutTime = $this->calculateLockoutTime($_SESSION['login_attempts'][$identifier]['attempts']);
            $_SESSION['login_attempts'][$identifier]['lockout_until'] = time() + $lockoutTime;
        }
        
        return $_SESSION['login_attempts'][$identifier]['attempts'];
    }
    
    /**
     * Get the number of login attempts for a given identifier
     *
     * @param string $identifier The user identifier (email or IP)
     * @return int The number of attempts
     */
    public function getLoginAttempts(string $identifier): int
    {
        $this->start();
        
        if (!isset($_SESSION['login_attempts']) || !isset($_SESSION['login_attempts'][$identifier])) {
            return 0;
        }
        
        return $_SESSION['login_attempts'][$identifier]['attempts'];
    }
    
    /**
     * Reset login attempts counter for a given identifier
     *
     * @param string $identifier The user identifier (email or IP)
     */
    public function resetLoginAttempts(string $identifier): void
    {
        $this->start();
        
        if (isset($_SESSION['login_attempts']) && isset($_SESSION['login_attempts'][$identifier])) {
            unset($_SESSION['login_attempts'][$identifier]);
        }
    }
    
    /**
     * Check if login is locked for a given identifier
     *
     * @param string $identifier The user identifier (email or IP)
     * @return bool True if locked, false otherwise
     */
    public function isLoginLocked(string $identifier): bool
    {
        $this->start();
        
        if (!isset($_SESSION['login_attempts']) || !isset($_SESSION['login_attempts'][$identifier])) {
            return false;
        }
        
        $lockoutUntil = $_SESSION['login_attempts'][$identifier]['lockout_until'] ?? 0;
        
        // If lockout period has expired
        if ($lockoutUntil > 0 && $lockoutUntil <= time()) {
            // Keep the attempt count but remove the lockout
            $_SESSION['login_attempts'][$identifier]['lockout_until'] = 0;
            return false;
        }
        
        return $lockoutUntil > time();
    }
    
    /**
     * Get remaining lockout time in seconds
     *
     * @param string $identifier The user identifier (email or IP)
     * @return int Remaining lockout time in seconds, 0 if not locked
     */
    public function getRemainingLockoutTime(string $identifier): int
    {
        $this->start();
        
        if (!isset($_SESSION['login_attempts']) || 
            !isset($_SESSION['login_attempts'][$identifier]) ||
            !isset($_SESSION['login_attempts'][$identifier]['lockout_until'])) {
            return 0;
        }
        
        $lockoutUntil = $_SESSION['login_attempts'][$identifier]['lockout_until'];
        $remaining = $lockoutUntil - time();
        
        return $remaining > 0 ? $remaining : 0;
    }
    
    /**
     * Calculate lockout time based on number of attempts
     *
     * @param int $attempts The number of failed attempts
     * @return int Lockout time in seconds
     */
    private function calculateLockoutTime(int $attempts): int
    {
        $excessAttempts = $attempts - self::MAX_LOGIN_ATTEMPTS;
        
        if ($excessAttempts < 0) {
            return 0;
        }
        
        // Base lockout time for first violation
        if ($excessAttempts === 0) {
            return self::BASE_LOCKOUT_TIME;
        }
        
        // Increase lockout time exponentially with each additional attempt
        return self::BASE_LOCKOUT_TIME * pow(self::LOCKOUT_MULTIPLIER, $excessAttempts);
    }

    /**
     * Increment the registration attempts for a given IP address
     *
     * @param string $identifier The IP address of the user
     * @return int The current number of attempts
     */
    public function incrementRegistrationAttempts(string $identifier): int
    {
        $this->start();
        
        if (!isset($_SESSION['registration_attempts'])) {
            $_SESSION['registration_attempts'] = [];
        }
        
        if (!isset($_SESSION['registration_attempts'][$identifier])) {
            $_SESSION['registration_attempts'][$identifier] = [
                'attempts' => 0,
                'last_attempt' => 0,
                'lockout_until' => 0
            ];
        }
        
        $_SESSION['registration_attempts'][$identifier]['attempts']++;
        $_SESSION['registration_attempts'][$identifier]['last_attempt'] = time();
        
        // If attempts exceed threshold, set lockout timestamp
        if ($_SESSION['registration_attempts'][$identifier]['attempts'] >= self::MAX_REGISTRATION) {
            $lockoutTime = $this->calculateRegistrationLockoutTime($_SESSION['registration_attempts'][$identifier]['attempts']);
            $_SESSION['registration_attempts'][$identifier]['lockout_until'] = time() + $lockoutTime;
        }
        
        return $_SESSION['registration_attempts'][$identifier]['attempts'];
    }

    /**
     * Check if registration is locked for an IP address
     *
     * @param string $identifier The IP address of the user
     * @return bool True if locked, false otherwise
     */
    public function isRegistrationLocked(string $identifier): bool
    {
        $this->start();
        
        if (!isset($_SESSION['registration_attempts']) || !isset($_SESSION['registration_attempts'][$identifier])) {
            return false;
        }
        
        $lockoutUntil = $_SESSION['registration_attempts'][$identifier]['lockout_until'] ?? 0;
        
        // If lockout period has expired
        if ($lockoutUntil > 0 && $lockoutUntil <= time()) {
            // Keep the attempt count but remove the lockout
            $_SESSION['registration_attempts'][$identifier]['lockout_until'] = 0;
            return false;
        }
        
        return $lockoutUntil > time();
    }

    /**
     * Get remaining registration lockout time in seconds
     *
     * @param string $identifier The IP address of the user
     * @return int Remaining lockout time in seconds, 0 if not locked
     */
    public function getRemainingRegistrationLockoutTime(string $identifier): int
    {
        $this->start();
        
        if (!isset($_SESSION['registration_attempts']) || 
            !isset($_SESSION['registration_attempts'][$identifier]) ||
            !isset($_SESSION['registration_attempts'][$identifier]['lockout_until'])) {
            return 0;
        }
        
        $lockoutUntil = $_SESSION['registration_attempts'][$identifier]['lockout_until'];
        $remaining = $lockoutUntil - time();
        
        return $remaining > 0 ? $remaining : 0;
    }

    /**
     * Calculate registration lockout time based on number of attempts
     *
     * @param int $attempts The number of failed attempts
     * @return int Lockout time in seconds
     */
    private function calculateRegistrationLockoutTime(int $attempts): int
    {
        $excessAttempts = $attempts - self::MAX_REGISTRATION;
        
        if ($excessAttempts < 0) {
            return 0;
        }
        
        // Base lockout time for first violation
        if ($excessAttempts === 0) {
            return self::BASE_REGISTRATION_LOCKOUT_TIME;
        }
        
        // Increase lockout time exponentially with each additional attempt
        return self::BASE_REGISTRATION_LOCKOUT_TIME * pow(self::REGISTRATION_LOCKOUT_MULTIPLIER, $excessAttempts);
    }

    public function update(): void
    {
        $this->start();
        $_SESSION['last_activity'] = time();
    }
}