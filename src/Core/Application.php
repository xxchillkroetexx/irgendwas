<?php

namespace SecretSanta\Core;

use SecretSanta\Controllers\HomeController;

class Application {
    private Router $router;
    
    public function __construct() {
        $this->router = Router::getInstance();
        $this->setupRoutes();
    }
    
    public function run(): void {
        try {
            // Handle the current request
            $this->router->handle();
        } catch (\Exception $e) {
            // Log the error
            error_log("Application error: " . $e->getMessage());
            
            // Display error in development mode
            if (getenv('APP_DEBUG') === 'true') {
                echo '<h1>Application Error</h1>';
                echo '<p>' . $e->getMessage() . '</p>';
                echo '<h2>Stack Trace:</h2>';
                echo '<pre>' . $e->getTraceAsString() . '</pre>';
            } else {
                // Show a generic error in production
                echo '<h1>Application Error</h1>';
                echo '<p>An unexpected error occurred. Please try again later.</p>';
            }
        }
    }
    
    private function setupRoutes(): void {
        // Home routes - just the minimal routes for testing
        $this->router->get('/', [HomeController::class, 'index']);
        $this->router->get('/language/:locale', [HomeController::class, 'setLanguage']);
    }
}