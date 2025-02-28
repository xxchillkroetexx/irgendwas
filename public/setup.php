<?php
/**
 * Secret Santa Web Application - Setup Script
 * 
 * This script initializes the database schema.
 */

// Display all errors
ini_set('display_errors', 1);
error_reporting(E_ALL);

// Define the application root path
define('APP_ROOT', dirname(__DIR__));

// Output basic HTML
echo '<!DOCTYPE html>
<html>
<head>
    <title>Secret Santa Setup</title>
    <style>
        body {
            font-family: Arial, sans-serif;
            max-width: 800px;
            margin: 20px auto;
            padding: 20px;
            line-height: 1.6;
        }
        .success {
            color: green;
            padding: 10px;
            border: 1px solid green;
            margin: 10px 0;
            border-radius: 5px;
            background-color: #f0fff0;
        }
        .error {
            color: red;
            padding: 10px;
            border: 1px solid red;
            margin: 10px 0;
            border-radius: 5px;
            background-color: #fff0f0;
        }
        pre {
            background-color: #f5f5f5;
            padding: 10px;
            overflow: auto;
            border-radius: 3px;
        }
    </style>
</head>
<body>
    <h1>Secret Santa Setup</h1>';

try {
    echo '<h2>Loading autoloader...</h2>';
    // Load the autoloader
    $autoloaderPath = APP_ROOT . '/src/Core/Autoloader.php';
    if (!file_exists($autoloaderPath)) {
        throw new Exception("Autoloader file not found at: $autoloaderPath");
    }
    
    require_once $autoloaderPath;
    echo '<p>Autoloader found and loaded.</p>';

    // Register the autoloader
    echo '<h2>Registering autoloader...</h2>';
    $autoloader = new \SecretSanta\Core\Autoloader();
    $autoloader->register();
    $autoloader->addNamespace('SecretSanta', APP_ROOT . '/src');
    echo '<p>Autoloader registered.</p>';

    // Check for schema file
    echo '<h2>Checking schema file...</h2>';
    $schemaPath = APP_ROOT . '/src/Config/schema.sql';
    if (!file_exists($schemaPath)) {
        throw new Exception("Schema file not found at: $schemaPath");
    }
    echo '<p>Schema file found.</p>';
    
    // Connect to database
    echo '<h2>Connecting to database...</h2>';
    $db = \SecretSanta\Config\Database::getInstance();
    $connection = $db->getConnection();
    echo '<p>Successfully connected to database.</p>';
    
    // Initialize the database
    echo '<h2>Initializing database schema...</h2>';
    $db->initialize();
    
    echo '<div class="success">Database schema created successfully!</div>';
    
    echo '<p>You can now <a href="/">return to the homepage</a> and register an account.</p>';
} catch (Exception $e) {
    echo '<div class="error">Error: ' . $e->getMessage() . '</div>';
    echo '<h3>Stack Trace:</h3>';
    echo '<pre>' . $e->getTraceAsString() . '</pre>';
    echo '<p>Please check your database configuration and try again.</p>';
}

echo '</body></html>';