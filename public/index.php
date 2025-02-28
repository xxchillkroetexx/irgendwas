<?php
/**
 * Secret Santa Web Application
 * 
 * A web application for organizing Secret Santa gift exchanges
 */

// Display all errors for debugging
ini_set('display_errors', 1);
error_reporting(E_ALL);

// Define the application root path
define('APP_ROOT', dirname(__DIR__));

// Load the autoloader
require_once APP_ROOT . '/src/Core/Autoloader.php';

try {
    // Register the autoloader
    $autoloader = new \SecretSanta\Core\Autoloader();
    $autoloader->register();
    $autoloader->addNamespace('SecretSanta', APP_ROOT . '/src');

    // Start the application
    $app = new \SecretSanta\Core\Application();
    $app->run();
} catch (Throwable $e) {
    // Display the error message
    echo '<h1>Application Error</h1>';
    echo '<p>' . $e->getMessage() . '</p>';
    echo '<h2>Stack Trace:</h2>';
    echo '<pre>' . $e->getTraceAsString() . '</pre>';
}