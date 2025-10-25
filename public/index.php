<?php

/**
 * Secret Santa Web Application
 * 
 * A web application for organizing Secret Santa gift exchanges. This is the main entry point
 * of the application that initializes autoloading, error reporting, and starts the application.
 * 
 * @package SecretSanta
 * @author  Kasimir Weilandt, Jannis Stahl, Andreas Wolf, Julian Gardeike, 2025
 * @version 1.0
 */

/**
 * Include the Composer autoloader to manage external dependencies
 * This loads all third-party packages defined in composer.json
 */
require_once __DIR__ . '/../vendor/autoload.php';

/**
 * Configure error handling based on environment
 * 
 * When APP_DEBUG is true, display all errors for development
 * Otherwise, hide errors for production environment
 */
if (getenv('APP_DEBUG') === 'true') {
    ini_set('display_errors', 1);
    error_reporting(E_ALL);
} else {
    ini_set('display_errors', 0);
    error_reporting(0);
}

/**
 * Define application root constant
 * This constant points to the parent directory of the public folder
 */
define('APP_ROOT', dirname(__DIR__));

/**
 * Load the custom autoloader class
 * This class handles the autoloading of application-specific classes
 */
require_once APP_ROOT . '/src/Core/Autoloader.php';

try {
    /**
     * Initialize and register the custom autoloader
     * This sets up PSR-4 style autoloading for the application namespace
     * 
     * @var \SecretSanta\Core\Autoloader $autoloader The autoloader instance
     */
    $autoloader = new \SecretSanta\Core\Autoloader();
    $autoloader->register();
    $autoloader->addNamespace('SecretSanta', APP_ROOT . '/src');

    /**
     * Bootstrap the application
     * Create an instance of the main Application class and run it
     * 
     * @var \SecretSanta\Core\Application $app The application instance
     */
    $app = new \SecretSanta\Core\Application();
    $app->run();
} catch (Throwable $e) {
    /**
     * Error handling
     * If an uncaught exception occurs, display the error information
     * This helps with debugging but should be disabled in production
     */
    echo '<h1>Application Error</h1>';
    echo '<p>' . $e->getMessage() . '</p>';
    echo '<h2>Stack Trace:</h2>';
    echo '<pre>' . $e->getTraceAsString() . '</pre>';
}
