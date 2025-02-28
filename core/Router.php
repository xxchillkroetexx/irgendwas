<?php
namespace core;

class Router {
    protected $routes = [
        'GET' => [],
        'POST' => []
    ];

    protected $params = [];
    
    // Register GET route
    public function get($uri, $controller) {
        $this->routes['GET'][$this->formatUri($uri)] = $controller;
    }
    
    // Register POST route
    public function post($uri, $controller) {
        $this->routes['POST'][$this->formatUri($uri)] = $controller;
    }
    
    // Format URI to standardize routes
    private function formatUri($uri) {
        // Remove trailing slash
        $uri = rtrim($uri, '/');
        
        // Ensure leading slash
        if (!empty($uri) && $uri[0] !== '/') {
            $uri = '/' . $uri;
        }
        
        return $uri;
    }
    
    // Get current URI
    public function getCurrentUri() {
        $uri = $_SERVER['REQUEST_URI'];
        
        // Remove query string
        $uri = strtok($uri, '?');
        
        // Remove trailing slash
        $uri = rtrim($uri, '/');
        
        // Ensure leading slash
        if (empty($uri)) {
            $uri = '/';
        }
        
        return $uri;
    }
    
    // Match current request to a route
    public function match($uri, $requestMethod) {
        // Check for exact match
        if (array_key_exists($uri, $this->routes[$requestMethod])) {
            return $this->routes[$requestMethod][$uri];
        }
        
        // Check for routes with parameters
        foreach ($this->routes[$requestMethod] as $route => $controller) {
            // Replace route parameters with regex pattern
            if (strpos($route, '{') !== false) {
                $pattern = preg_replace('/{[^}]+}/', '([^/]+)', $route);
                $pattern = '#^' . $pattern . '$#';
                
                if (preg_match($pattern, $uri, $matches)) {
                    // Extract parameter values
                    array_shift($matches);
                    $this->params = $matches;
                    
                    return $controller;
                }
            }
        }
        
        return false;
    }
    
    // Dispatch the request to the appropriate controller
    public function dispatch() {
        $uri = $this->getCurrentUri();
        $requestMethod = $_SERVER['REQUEST_METHOD'];
        
        $controller = $this->match($uri, $requestMethod);
        
        if ($controller) {
            return $this->loadController($controller);
        }
        
        // No route match found - show 404
        $this->notFound();
    }
    
    // Load controller and execute the method
    protected function loadController($controller) {
        // Split controller@method syntax
        list($controller, $method) = explode('@', $controller);
        
        // Add namespace to controller if it doesn't already have one
        if (strpos($controller, '\\') === false) {
            $controller = "controllers\\{$controller}";
        }
        
        // Check if controller exists
        if (!class_exists($controller)) {
            throw new \Exception("Controller {$controller} not found");
        }
        
        $controllerInstance = new $controller();
        
        // Check if method exists
        if (!method_exists($controllerInstance, $method)) {
            throw new \Exception("Method {$method} not found in controller {$controller}");
        }
        
        // Execute controller method with parameters
        return call_user_func_array([$controllerInstance, $method], $this->params ?? []);
    }
    
    // 404 Not Found handling
    protected function notFound() {
        http_response_code(404);
        $viewPath = ROOT_PATH . '/views/errors/404.php';
        
        if (file_exists($viewPath)) {
            // Extract $pageTitle for the layout
            $pageTitle = '404 Not Found';
            
            // Include the layout if it exists
            $layoutPath = ROOT_PATH . '/views/layouts/main.php';
            ob_start();
            include $viewPath;
            $content = ob_get_clean();
            
            if (file_exists($layoutPath)) {
                include $layoutPath;
            } else {
                echo $content;
            }
        } else {
            echo '<h1>404 Not Found</h1>';
            echo '<p>The requested page could not be found.</p>';
        }
        exit;
    }
}
