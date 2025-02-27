<?php
namespace core;

use core\Database\Projekt_DB;

class App {
    private static $instance = null;
    private $router;
    private $db;
    
    public function __construct() {
        // Initialize session
        $this->initSession();
        
        // Create router instance
        $this->router = new Router();
        
        // Initialize database connection
        $this->db = Projekt_DB::getInstance();
        
        // Ensure database tables exist
        $this->ensureDatabaseTables();
        
        // Register routes
        $this->registerRoutes();
        
        // Set singleton instance
        self::$instance = $this;
    }
    
    public static function getInstance() {
        if (self::$instance === null) {
            self::$instance = new self();
        }
        return self::$instance;
    }
    
    private function initSession() {
        // Configure session settings
        ini_set('session.gc_maxlifetime', SESSION_LIFETIME);
        ini_set('session.cookie_lifetime', SESSION_LIFETIME);
        
        session_set_cookie_params([
            'lifetime' => SESSION_LIFETIME,
            'path' => '/',
            'domain' => '',
            'secure' => SESSION_SECURE,
            'httponly' => SESSION_HTTP_ONLY,
            'samesite' => 'Lax'
        ]);
        
        // Start the session
        if (session_status() === PHP_SESSION_NONE) {
            session_start();
        }
    }
    
    private function registerRoutes() {
        // Home routes
        $this->router->get('/', 'HomeController@index');
        
        // Authentication routes
        $this->router->get('/login', 'AuthController@showLogin');
        $this->router->post('/login', 'AuthController@login');
        $this->router->get('/register', 'AuthController@showRegister');
        $this->router->post('/register', 'AuthController@register');
        $this->router->get('/logout', 'AuthController@logout');
        $this->router->get('/forgot-password', 'AuthController@showForgotPassword');
        $this->router->post('/forgot-password', 'AuthController@forgotPassword');
        $this->router->get('/reset-password/{token}', 'AuthController@showResetPassword');
        $this->router->post('/reset-password', 'AuthController@resetPassword');
        
        // Group routes
        $this->router->get('/groups', 'GroupController@index');
        $this->router->get('/groups/new', 'GroupController@create');
        $this->router->post('/groups', 'GroupController@store');
        $this->router->get('/groups/{id}', 'GroupController@show');
        $this->router->get('/groups/{id}/edit', 'GroupController@edit');
        $this->router->post('/groups/{id}', 'GroupController@update');
        $this->router->post('/groups/{id}/delete', 'GroupController@delete');
        $this->router->post('/groups/{id}/draw', 'GroupController@draw');
        $this->router->post('/groups/{id}/redraw', 'GroupController@redraw');
        $this->router->post('/groups/{id}/invite', 'GroupController@sendInvites');
        
        // Wishlist routes
        $this->router->get('/groups/{groupId}/wishlist', 'WishlistController@show');
        $this->router->post('/groups/{groupId}/wishlist', 'WishlistController@update');
        $this->router->post('/groups/{groupId}/wishlist/item', 'WishlistController@addItem');
        $this->router->post('/groups/{groupId}/wishlist/item/{itemId}/delete', 'WishlistController@deleteItem');
        $this->router->post('/groups/{groupId}/wishlist/item/{itemId}/order', 'WishlistController@reorderItem');
        
        // User profile routes
        $this->router->get('/profile', 'ProfileController@show');
        $this->router->post('/profile', 'ProfileController@update');
        
        // Admin routes
        $this->router->get('/admin/users', 'AdminController@users');
        $this->router->get('/admin/groups', 'AdminController@groups');
    }
    
    public function run() {
        try {
            // Handle the current request
            $this->router->dispatch();
        } catch (\Exception $e) {
            // Log the error and display error page
            $this->handleException($e);
        }
    }
    
    private function handleException(\Exception $e) {
        if (DEBUG) {
            // Display detailed error info in development mode
            echo '<h1>Error</h1>';
            echo '<p>' . $e->getMessage() . '</p>';
            echo '<pre>' . $e->getTraceAsString() . '</pre>';
        } else {
            // Display generic error in production
            include ROOT_PATH . '/views/errors/500.php';
        }
    }
    
    public function getRouter() {
        return $this->router;
    }
    
    public function getDB() {
        return $this->db;
    }
    
    // Ensure all required database tables exist
    private function ensureDatabaseTables() {
        try {
            // Check if the users table exists by querying it
            $this->db->execute("SHOW TABLES LIKE 'users'");
            $result = $this->db->fetch("SHOW TABLES LIKE 'users'");
            
            if (!$result) {
                // Tables don't exist, create them
                $this->db->createTables();
                
                if (DEBUG) {
                    echo "<div style='position: fixed; top: 0; right: 0; background: #d4edda; border: 1px solid #c3e6cb; padding: 10px; margin: 10px; border-radius: 5px; z-index: 9999;'>";
                    echo "Database tables were automatically created.";
                    echo "</div>";
                }
            }
        } catch (\Exception $e) {
            // If there's an error, log it but continue
            error_log('Failed to check/create database tables: ' . $e->getMessage());
        }
    }
}
