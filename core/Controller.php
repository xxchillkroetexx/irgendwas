<?php
namespace core;

use models\User;

class Controller {
    // Render a view with the given data
    protected function view($view, $data = []) {
        // Extract data to make variables available in view
        if (!empty($data) && is_array($data)) {
            extract($data);
        }
        
        // Check if view exists
        $viewPath = ROOT_PATH . '/views/' . $view . '.php';
        if (!file_exists($viewPath)) {
            throw new \Exception("View {$view} not found.");
        }
        
        // Start output buffering
        ob_start();
        include $viewPath;
        $content = ob_get_clean();
        
        // Check if layout exists and include it
        $layoutPath = ROOT_PATH . '/views/layouts/main.php';
        if (file_exists($layoutPath)) {
            include $layoutPath;
        } else {
            echo $content;
        }
    }
    
    // Redirect to a specified URL
    protected function redirect($url) {
        header("Location: {$url}");
        exit;
    }
    
    // Get current authenticated user
    protected function currentUser() {
        if (isset($_SESSION['user_id'])) {
            $userModel = new User();
            return $userModel->findById($_SESSION['user_id']);
        }
        return null;
    }
    
    // Check if user is authenticated
    protected function isAuthenticated() {
        return isset($_SESSION['user_id']);
    }
    
    // Require authentication to access a page
    protected function requireAuth() {
        if (!$this->isAuthenticated()) {
            $_SESSION['redirect_after_login'] = $_SERVER['REQUEST_URI'];
            $this->redirect('/login');
        }
    }
    
    // Flash messages for user notification
    protected function flash($type, $message) {
        Flash::set($type, $message);
    }
    
    // Get flash messages
    protected function getFlash($type = null) {
        return Flash::get($type);
    }
    
    // Validate CSRF token
    protected function validateCSRF() {
        if (!isset($_POST['csrf_token']) || $_POST['csrf_token'] !== $_SESSION['csrf_token']) {
            $this->flash('danger', 'CSRF token verification failed');
            $this->redirect($_SERVER['HTTP_REFERER'] ?? '/');
        }
    }
    
    // Generate CSRF token
    protected function generateCSRF() {
        $_SESSION['csrf_token'] = bin2hex(random_bytes(32));
        return $_SESSION['csrf_token'];
    }
    
    // Sanitize input
    protected function sanitize($data) {
        if (is_array($data)) {
            foreach ($data as $key => $value) {
                $data[$key] = $this->sanitize($value);
            }
            return $data;
        }
        return htmlspecialchars($data, ENT_QUOTES, 'UTF-8');
    }
    
    // Return JSON response
    protected function json($data, $statusCode = 200) {
        http_response_code($statusCode);
        header('Content-Type: application/json');
        echo json_encode($data);
        exit;
    }
}
