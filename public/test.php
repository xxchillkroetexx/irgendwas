<?php
// Display all errors for debugging
ini_set('display_errors', 1);
error_reporting(E_ALL);

// Define the application root path
define('APP_ROOT', dirname(__DIR__));

echo "<h1>PHP Diagnostic Information</h1>";
echo "<h2>Current Directory Structure</h2>";
echo "<pre>";

// Check src directory contents
$srcPath = APP_ROOT . '/src';
echo "Checking directory: {$srcPath}\n";
if (is_dir($srcPath)) {
    $srcContents = scandir($srcPath);
    echo "Contents of {$srcPath}:\n";
    print_r($srcContents);
    
    // Check Core directory
    $corePath = $srcPath . '/Core';
    echo "\nChecking directory: {$corePath}\n";
    if (is_dir($corePath)) {
        $coreContents = scandir($corePath);
        echo "Contents of {$corePath}:\n";
        print_r($coreContents);
        
        // Check Autoloader.php
        $autoloaderPath = $corePath . '/Autoloader.php';
        echo "\nChecking file: {$autoloaderPath}\n";
        if (file_exists($autoloaderPath)) {
            echo "File exists: Yes\n";
            echo "File size: " . filesize($autoloaderPath) . " bytes\n";
            echo "File readable: " . (is_readable($autoloaderPath) ? 'Yes' : 'No') . "\n";
            echo "File content sample:\n";
            echo htmlspecialchars(substr(file_get_contents($autoloaderPath), 0, 200)) . "...\n";
        } else {
            echo "File exists: No\n";
        }
    } else {
        echo "Directory not found: {$corePath}\n";
    }
} else {
    echo "Directory not found: {$srcPath}\n";
}

echo "</pre>";

// Check PHP version and extensions
echo "<h2>PHP Environment</h2>";
echo "<pre>";
echo "PHP Version: " . phpversion() . "\n";
echo "Loaded Extensions:\n";
print_r(get_loaded_extensions());
echo "</pre>";

// Check file permissions
echo "<h2>File Permissions</h2>";
echo "<pre>";
echo "Current user: " . exec('whoami') . "\n";
echo "Public directory permissions: " . substr(sprintf('%o', fileperms(APP_ROOT . '/public')), -4) . "\n";
echo "Src directory permissions: " . substr(sprintf('%o', fileperms(APP_ROOT . '/src')), -4) . "\n";
if (is_dir($corePath)) {
    echo "Core directory permissions: " . substr(sprintf('%o', fileperms($corePath)), -4) . "\n";
}
echo "</pre>";

// Try to manually include the Autoloader
echo "<h2>Autoloader Test</h2>";
echo "<pre>";
if (file_exists(APP_ROOT . '/src/Core/Autoloader.php')) {
    echo "Including Autoloader.php...\n";
    try {
        require_once APP_ROOT . '/src/Core/Autoloader.php';
        echo "Autoloader included successfully.\n";
        
        if (class_exists('\SecretSanta\Core\Autoloader')) {
            echo "Autoloader class exists in the expected namespace.\n";
        } else {
            echo "Autoloader class not found in the \SecretSanta\Core namespace.\n";
        }
        
    } catch (Exception $e) {
        echo "Error including Autoloader.php: " . $e->getMessage() . "\n";
    }
} else {
    echo "Autoloader.php not found\n";
}
echo "</pre>";

// Testing environment variables
echo "<h2>Environment Variables</h2>";
echo "<pre>";
echo "APP_DEBUG: " . (getenv('APP_DEBUG') ? getenv('APP_DEBUG') : 'Not set') . "\n";
echo "APP_URL: " . (getenv('APP_URL') ? getenv('APP_URL') : 'Not set') . "\n";
echo "DB_HOST: " . (getenv('DB_HOST') ? getenv('DB_HOST') : 'Not set') . "\n";
echo "</pre>";

phpinfo();