<?php
namespace core;

class Flash {
    // Get flash messages
    public static function get($type = null) {
        if ($type !== null) {
            $message = $_SESSION['flash'][$type] ?? null;
            unset($_SESSION['flash'][$type]);
            return $message;
        }
        
        $flash = $_SESSION['flash'] ?? [];
        unset($_SESSION['flash']);
        return $flash;
    }
    
    // Set flash message
    public static function set($type, $message) {
        $_SESSION['flash'][$type] = $message;
    }
}
