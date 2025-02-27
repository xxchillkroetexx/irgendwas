<?php
// Database initialization script
define('ROOT_PATH', __DIR__);
require_once ROOT_PATH . '/config/config.php';

// Display all errors for easier debugging
ini_set('display_errors', 1);
ini_set('display_startup_errors', 1);
error_reporting(E_ALL);

echo "<h1>Secret Santa Database Initialization</h1>";

// Autoload classes
spl_autoload_register(function ($class) {
    $classPath = ROOT_PATH . '/' . str_replace('\\', '/', $class) . '.php';
    if (file_exists($classPath)) {
        require_once $classPath;
    }
});

try {
    echo "<h2>Database Connection</h2>";
    
    // Get database connection
    $db = \core\Database\Projekt_DB::getInstance();
    echo "<p style='color: green;'>✓ Connected to database server</p>";
    
    echo "<h2>Creating Tables</h2>";
    
    // Create all the necessary tables
    $db->createTables();
    echo "<p style='color: green;'>✓ Database tables created successfully</p>";
    
    echo "<h2>Creating Test User</h2>";
    
    // Create a test user
    $userModel = new models\User();
    
    // Check if test user already exists
    $testUser = $userModel->findByEmail('test@example.com');
    
    if ($testUser) {
        echo "<p>Test user already exists.</p>";
    } else {
        $testUser = $userModel->create([
            'email' => 'test@example.com',
            'password' => 'password123',
            'first_name' => 'Test',
            'last_name' => 'User'
        ]);
        
        if ($testUser) {
            echo "<p style='color: green;'>✓ Test user created successfully!</p>";
            echo "<p>Login with:</p>";
            echo "<ul>";
            echo "<li>Email: <strong>test@example.com</strong></li>";
            echo "<li>Password: <strong>password123</strong></li>";
            echo "</ul>";
        } else {
            echo "<p style='color: red;'>✗ Failed to create test user</p>";
        }
    }
    
    echo "<h2>All Done!</h2>";
    echo "<p>Database setup complete. <a href='/'>Go to homepage</a></p>";
    
} catch (\Exception $e) {
    echo "<div style='color: red; background: #ffeeee; padding: 10px; margin: 10px; border: 1px solid red;'>";
    echo "<h3>Error:</h3>";
    echo "<p>{$e->getMessage()}</p>";
    echo "</div>";
    
    echo "<h3>Debugging Information:</h3>";
    echo "<pre>";
    echo $e->getTraceAsString();
    echo "</pre>";
}
