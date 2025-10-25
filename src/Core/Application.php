<?php

namespace SecretSanta\Core;

use SecretSanta\Controllers\HomeController;
use SecretSanta\Controllers\AuthController;
use SecretSanta\Controllers\UserController;
use SecretSanta\Controllers\GroupController;
use SecretSanta\Controllers\WishlistController;
use SecretSanta\Controllers\ExclusionController;

/**
 * Application Core Class
 * 
 * The main application class that bootstraps the Secret Santa application,
 * sets up routes, and handles incoming requests. It serves as the central
 * coordination point for the application.
 * 
 * @package SecretSanta\Core
 * @version 1.0
 */
class Application
{
    /**
     * Router instance for handling URL routing
     * 
     * @var Router
     */
    private Router $router;

    /**
     * Constructor - initializes the application components
     * 
     * Sets up internationalization, loads translation functions,
     * and configures all application routes
     */
    public function __construct()
    {
        // Initialize i18n
        I18n::getInstance();

        // Include global translation functions if not using composer autoload
        if (!function_exists('__')) {
            require_once APP_ROOT . '/src/Localization/functions.php';
        }

        $this->router = Router::getInstance();
        $this->setupRoutes();
    }

    /**
     * Run the application
     * 
     * Process the current request and generate a response.
     * Handles errors and exceptions that occur during request processing.
     * 
     * @return void
     */
    public function run(): void
    {
        try {
            // Handle the current request
            $this->router->handle();
        } catch (\Exception $e) {
            // Log the error
            error_log("Application error: " . $e->getMessage());

            // Display error in development mode
            if (getenv('APP_DEBUG') === 'true') {
                echo '<h1>' . __('application_error') . '</h1>';
                echo '<p>' . $e->getMessage() . '</p>';
                echo '<h2>' . __('stack_trace') . '</h2>';
                echo '<pre>' . $e->getTraceAsString() . '</pre>';
            } else {
                // Show a generic error in production
                echo '<h1>' . __('application_error') . '</h1>';
                echo '<p>' . __('generic_error_message') . '</p>';
            }
        }
    }

    /**
     * Configure all application routes
     * 
     * Defines the URL patterns and their corresponding controller actions.
     * Groups routes by functionality (home, auth, user, group, wishlist, exclusion).
     * 
     * @return void
     */
    private function setupRoutes(): void
    {
        // Home routes
        $this->router->get('/', [HomeController::class, 'index']);
        $this->router->get('/language/:locale', [HomeController::class, 'setLanguage']);

        // Authentication routes
        $this->router->get('/auth/login', [AuthController::class, 'showLogin']);
        $this->router->post('/auth/login', [AuthController::class, 'login']);
        $this->router->get('/auth/register', [AuthController::class, 'showRegister']);
        $this->router->post('/auth/register', [AuthController::class, 'register']);
        $this->router->get('/auth/logout', [AuthController::class, 'logout']);
        $this->router->get('/auth/forgot-password', [AuthController::class, 'showForgotPassword']);
        $this->router->post('/auth/forgot-password', [AuthController::class, 'forgotPassword']);
        $this->router->get('/auth/reset-password/:token', [AuthController::class, 'showResetPassword']);
        $this->router->post('/auth/reset-password', [AuthController::class, 'resetPassword']);

        // User routes
        $this->router->get('/user/dashboard', [UserController::class, 'dashboard']);
        $this->router->get('/user/profile', [UserController::class, 'showProfile']);
        $this->router->post('/user/profile', [UserController::class, 'updateProfile']);
        $this->router->get('/user/verify-email/:token', [UserController::class, 'verifyEmail']);

        // API routes
        $this->router->get('/api/user/:id', [UserController::class, 'apiGetUser']);

        // Group routes
        $this->router->get('/groups', [GroupController::class, 'index']);
        $this->router->get('/groups/create', [GroupController::class, 'create']);
        $this->router->post('/groups/create', [GroupController::class, 'store']);
        $this->router->get('/groups/join', [GroupController::class, 'showJoin']);
        $this->router->post('/groups/join', [GroupController::class, 'join']);
        $this->router->get('/groups/:id', [GroupController::class, 'show']);
        $this->router->get('/groups/:id/edit', [GroupController::class, 'edit']);
        $this->router->post('/groups/:id/edit', [GroupController::class, 'update']);
        $this->router->get('/groups/:id/delete', [GroupController::class, 'delete']);
        $this->router->get('/groups/:id/leave', [GroupController::class, 'leave']);
        $this->router->get('/groups/:id/draw', [GroupController::class, 'draw']);
        $this->router->get('/groups/:id/regenerate-invitation', [GroupController::class, 'generateInvitationLink']);

        // Wishlist routes
        $this->router->get('/wishlist/view/:userId/:groupId', [WishlistController::class, 'view']);
        $this->router->get('/wishlist/edit/:groupId', [WishlistController::class, 'edit']);
        $this->router->post('/wishlist/:groupId/settings', [WishlistController::class, 'updateSettings']);
        $this->router->post('/wishlist/:groupId/item/add', [WishlistController::class, 'addItem']);
        $this->router->post('/wishlist/item/:itemId/update', [WishlistController::class, 'updateItem']);
        $this->router->get('/wishlist/item/:itemId/delete', [WishlistController::class, 'deleteItem']);
        $this->router->post('/wishlist/:groupId/priority', [WishlistController::class, 'updatePriority']);
        $this->router->get('/wishlist/:groupId/delete', [WishlistController::class, 'deleteWishlist']);

        // Exclusion routes
        $this->router->get('/exclusions/:groupId', [ExclusionController::class, 'index']);
        $this->router->post('/exclusions/:groupId/add', [ExclusionController::class, 'add']);
        $this->router->get('/exclusions/:groupId/remove/:excludedUserId', [ExclusionController::class, 'remove']);
    }
}
