<?php

/**
 * Automatic Database Initialization Script
 * 
 * This script automatically initializes the database schema when the container starts.
 * It checks if tables exist and creates them if needed.
 */

define('APP_ROOT', '/var/www');

// Load autoloader
require_once APP_ROOT . '/vendor/autoload.php';
require_once APP_ROOT . '/src/Core/Autoloader.php';

$autoloader = new \SecretSanta\Core\Autoloader();
$autoloader->register();
$autoloader->addNamespace('SecretSanta', APP_ROOT . '/src');

try {
    echo "Connecting to database...\n";
    $db = \SecretSanta\Config\Database::getInstance();
    $connection = $db->getConnection();
    
    // Check if tables exist
    $stmt = $connection->query("SHOW TABLES");
    $tables = $stmt->fetchAll(PDO::FETCH_COLUMN);
    
    if (empty($tables)) {
        echo "Database is empty. Initializing schema...\n";
        $db->initialize();
        echo "Database schema created successfully!\n";
    } else {
        echo "Database already initialized. Tables found: " . count($tables) . "\n";
    }
    
    exit(0);
} catch (Exception $e) {
    echo "Error initializing database: " . $e->getMessage() . "\n";
    echo "You can manually initialize by visiting /setup.php\n";
    exit(1);
}
