<?php

namespace SecretSanta\Core;

class Session
{
    private static ?self $instance = null;
    private bool $started = false;

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
        }
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
}
