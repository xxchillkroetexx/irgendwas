<?php
// Display all errors
ini_set('display_errors', 1);
error_reporting(E_ALL);

echo '<h1>PHP Debug Information</h1>';

// Check file paths and existence
$appRoot = dirname(__DIR__);
echo '<h2>File Paths and Existence</h2>';
echo '<pre>';
echo "App Root: {$appRoot}\n";

// Check src directory
$srcDir = $appRoot . '/src';
echo "src directory exists: " . (is_dir($srcDir) ? 'Yes' : 'No') . "\n";
if (is_dir($srcDir)) {
    echo "src directory readable: " . (is_readable($srcDir) ? 'Yes' : 'No') . "\n";
    echo "src directory contents: " . implode(', ', scandir($srcDir)) . "\n";
    
    // Check Core directory
    $coreDir = $srcDir . '/Core';
    echo "\nCore directory exists: " . (is_dir($coreDir) ? 'Yes' : 'No') . "\n";
    if (is_dir($coreDir)) {
        echo "Core directory readable: " . (is_readable($coreDir) ? 'Yes' : 'No') . "\n";
        echo "Core directory contents: " . implode(', ', scandir($coreDir)) . "\n";
        
        // Check Autoloader.php
        $autoloaderFile = $coreDir . '/Autoloader.php';
        echo "\nAutoloader.php exists: " . (file_exists($autoloaderFile) ? 'Yes' : 'No') . "\n";
        if (file_exists($autoloaderFile)) {
            echo "Autoloader.php readable: " . (is_readable($autoloaderFile) ? 'Yes' : 'No') . "\n";
            echo "Autoloader.php size: " . filesize($autoloaderFile) . " bytes\n";
            
            // Try to include the Autoloader
            echo "\nAttempting to include Autoloader.php...\n";
            try {
                include_once $autoloaderFile;
                echo "Successfully included Autoloader.php\n";
                
                // Check if the class is available
                if (class_exists('\\SecretSanta\\Core\\Autoloader')) {
                    echo "Autoloader class exists\n";
                    
                    // Try to instantiate the Autoloader
                    try {
                        $autoloader = new \SecretSanta\Core\Autoloader();
                        echo "Successfully instantiated Autoloader\n";
                    } catch (Throwable $e) {
                        echo "Error instantiating Autoloader: " . $e->getMessage() . "\n";
                    }
                } else {
                    echo "Autoloader class does not exist\n";
                }
            } catch (Throwable $e) {
                echo "Error including Autoloader.php: " . $e->getMessage() . "\n";
            }
        }
    }
}

echo '</pre>';

// Display current working directory and file permissions
echo '<h2>Working Directory and Permissions</h2>';
echo '<pre>';
echo "Current working directory: " . getcwd() . "\n";
echo "Document root: " . $_SERVER['DOCUMENT_ROOT'] . "\n";
echo "Script filename: " . $_SERVER['SCRIPT_FILENAME'] . "\n";
echo '</pre>';

// Check environment variables
echo '<h2>Environment Variables</h2>';
echo '<pre>';
foreach ($_ENV as $key => $value) {
    echo "$key: $value\n";
}
echo '</pre>';

// Check PHP information
echo '<h2>PHP Information</h2>';
echo '<pre>';
echo "PHP version: " . phpversion() . "\n";
echo "Loaded extensions: " . implode(', ', get_loaded_extensions()) . "\n";
echo '</pre>';

// Display server information
echo '<h2>Server Information</h2>';
echo '<pre>';
foreach ($_SERVER as $key => $value) {
    if (!is_array($value)) {
        echo "$key: $value\n";
    }
}
echo '</pre>';