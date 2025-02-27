<?php
// Entry point for the Secret Santa application
define('ROOT_PATH', dirname(__DIR__));
require_once ROOT_PATH . '/config/config.php';

// Autoload classes
spl_autoload_register(function ($class) {
    $classPath = ROOT_PATH . '/' . str_replace('\\', '/', $class) . '.php';
    if (file_exists($classPath)) {
        require_once $classPath;
    }
});

// Initialize the application
$app = new \core\App();
$app->run();
?>