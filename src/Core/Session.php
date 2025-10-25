<?php

namespace SecretSanta\Core;

/**
 * Session management class implementing Singleton pattern
 * 
 * Provides methods to manage PHP sessions, including flash messages
 * and form input persistence.
 */
class Session
{
    /**
     * @var self|null Singleton instance
     */
    private static ?self $instance = null;

    /**
     * @var bool Whether the session has been started
     */
    private bool $started = false;
    private int $inactivityLimit;

    /**
     * Private constructor to enforce singleton pattern
     */
    private function __construct()
    {
        $this->inactivityLimit = getenv('SESSION_INACTIVITY_LIMIT') ?: 1200;
        // Don't start session in constructor - will do it on demand
    }

    /**
     * Get the singleton instance
     * 
     * @return self The session instance
     */
    public static function getInstance(): self
    {
        if (self::$instance === null) {
            self::$instance = new self();
        }

        return self::$instance;
    }

    /**
     * Start the session if not already started
     * 
     * @return void
     */
    private function start(): void
    {
        if (!$this->started && session_status() === PHP_SESSION_NONE && !headers_sent()) {
            session_start();
            $this->started = true;
        }
    }

    /**
     * Check for inactivity and destroy the session if inactive
     * 
     * @return void
     */
    public function checkInactivity(): void
    {
        if (isset($_SESSION['last_activity']) && (time() - $_SESSION['last_activity']) > $this->inactivityLimit) {
            $this->destroy();
        }
        $_SESSION['last_activity'] = time();
    }

    /**
     * Set a session value
     * 
     * @param string $key Session key
     * @param mixed $value Value to store
     * @return void
     */
    public function set(string $key, $value): void
    {
        $this->start();
        $_SESSION[$key] = $value;
    }

    /**
     * Get a session value
     * 
     * @param string $key Session key
     * @param mixed $default Default value if key not found
     * @return mixed Session value or default
     */
    public function get(string $key, $default = null)
    {
        $this->start();
        return $_SESSION[$key] ?? $default;
    }

    /**
     * Check if a session key exists
     * 
     * @param string $key Session key
     * @return bool True if key exists
     */
    public function has(string $key): bool
    {
        $this->start();
        return isset($_SESSION[$key]);
    }

    /**
     * Remove a session value
     * 
     * @param string $key Session key
     * @return void
     */
    public function remove(string $key): void
    {
        $this->start();
        if (isset($_SESSION[$key])) {
            unset($_SESSION[$key]);
        }
    }

    /**
     * Destroy the current session
     * 
     * @return void
     */
    public function destroy(): void
    {
        $this->start();
        session_destroy();
        $_SESSION = [];
        $this->started = false;
    }

    /**
     * Set a flash message (available only for the next request)
     * 
     * @param string $key Flash key
     * @param mixed $value Flash value
     * @return void
     */
    public function setFlash(string $key, $value): void
    {
        $this->start();
        if (!isset($_SESSION['_flash'])) {
            $_SESSION['_flash'] = [];
        }

        $_SESSION['_flash'][$key] = $value;
    }

    /**
     * Get and remove a flash message
     * 
     * @param string $key Flash key
     * @param mixed $default Default value if key not found
     * @return mixed Flash value or default
     */
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

    /**
     * Check if a flash message exists
     * 
     * @param string $key Flash key
     * @return bool True if flash key exists
     */
    public function hasFlash(string $key): bool
    {
        $this->start();
        return isset($_SESSION['_flash']) && isset($_SESSION['_flash'][$key]);
    }

    /**
     * Clear all flash messages
     * 
     * @return void
     */
    public function clearFlash(): void
    {
        $this->start();
        $_SESSION['_flash'] = [];
    }

    /**
     * Regenerate session ID for security
     * 
     * @return void
     */
    public function regenerate(): void
    {
        $this->start();
        session_regenerate_id(true);
    }

    /**
     * Get current session ID
     * 
     * @return string Session ID
     */
    public function getSessionId(): string
    {
        $this->start();
        return session_id();
    }

    /**
     * Alias for setFlash
     * 
     * @param string $key Flash key
     * @param mixed $value Flash value
     * @return void
     */
    public function flash(string $key, $value): void
    {
        $this->setFlash($key, $value);
    }

    /**
     * Get old form input value from flash data
     * 
     * @param string $key Input field name
     * @param mixed $default Default value if not found
     * @return mixed Input value or default
     */
    public function getOldInput(string $key, $default = null)
    {
        $old = $this->getFlash('old') ?? [];
        return $old[$key] ?? $default;
    }

    public function update(): void
    {
        $this->start();
        $_SESSION['last_activity'] = time();
    }
}
