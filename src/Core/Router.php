<?php

namespace SecretSanta\Core;

/**
 * Router class implementing Singleton pattern
 * 
 * Handles routing HTTP requests to appropriate handlers
 * and supports different HTTP methods and route parameters.
 */
class Router
{
    /**
     * @var self|null Singleton instance
     */
    private static ?self $instance = null;
    
    /**
     * @var array Array of registered routes
     */
    private array $routes = [];

    /**
     * Private constructor to enforce singleton pattern
     */
    private function __construct() {}

    /**
     * Get the singleton instance
     * 
     * @return self The router instance
     */
    public static function getInstance(): self
    {
        if (self::$instance === null) {
            self::$instance = new self();
        }

        return self::$instance;
    }

    /**
     * Add a new route with specified HTTP method
     * 
     * @param string $method HTTP method (GET, POST, etc.)
     * @param string $path Route path with optional parameters
     * @param mixed $handler Route handler (callable or [controller, method])
     * @return self For method chaining
     */
    public function addRoute(string $method, string $path, $handler): self
    {
        $this->routes[] = [
            'method' => $method,
            'path' => $path,
            'handler' => $handler
        ];

        return $this;
    }

    /**
     * Add a GET route
     * 
     * @param string $path Route path
     * @param mixed $handler Route handler
     * @return self For method chaining
     */
    public function get(string $path, $handler): self
    {
        return $this->addRoute('GET', $path, $handler);
    }

    /**
     * Add a POST route
     * 
     * @param string $path Route path
     * @param mixed $handler Route handler
     * @return self For method chaining
     */
    public function post(string $path, $handler): self
    {
        return $this->addRoute('POST', $path, $handler);
    }

    /**
     * Add a PUT route
     * 
     * @param string $path Route path
     * @param mixed $handler Route handler
     * @return self For method chaining
     */
    public function put(string $path, $handler): self
    {
        return $this->addRoute('PUT', $path, $handler);
    }

    /**
     * Add a DELETE route
     * 
     * @param string $path Route path
     * @param mixed $handler Route handler
     * @return self For method chaining
     */
    public function delete(string $path, $handler): self
    {
        return $this->addRoute('DELETE', $path, $handler);
    }

    /**
     * Handle the current request by matching against registered routes
     * 
     * @return void
     */
    public function handle(): void
    {
        $method = $_SERVER['REQUEST_METHOD'];
        $uri = parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH);

        if ($uri !== '/') {
            $uri = rtrim($uri, '/');
        }

        foreach ($this->routes as $route) {
            if ($route['method'] !== $method) {
                continue;
            }

            $pattern = $this->convertRouteToRegex($route['path']);

            if (preg_match($pattern, $uri, $matches)) {
                array_shift($matches); // Remove the full match
                $handler = $route['handler'];

                // Process the handler
                try {
                    if (is_callable($handler)) {
                        // It's already a callable, so use it directly
                        $response = call_user_func_array($handler, $matches);
                    } elseif (is_array($handler) && count($handler) === 2) {
                        // It's an array in the form [ControllerClass, methodName]
                        $controllerClass = $handler[0];
                        $method = $handler[1];

                        if (class_exists($controllerClass)) {
                            $controller = new $controllerClass();
                            if (method_exists($controller, $method)) {
                                $response = call_user_func_array([$controller, $method], $matches);
                            } else {
                                throw new \Exception("Method {$method} not found in controller {$controllerClass}");
                            }
                        } else {
                            throw new \Exception("Controller class {$controllerClass} not found");
                        }
                    } else {
                        throw new \Exception("Invalid route handler format");
                    }

                    // Process the response
                    if ($response === null) {
                        return;
                    }

                    if (is_string($response)) {
                        echo $response;
                    } elseif (is_array($response) || is_object($response)) {
                        header('Content-Type: application/json');
                        echo json_encode($response);
                    }

                    return;
                } catch (\Exception $e) {
                    // Log the error
                    error_log("Router error: " . $e->getMessage());

                    // Return 500 error
                    http_response_code(500);
                    echo "500 Internal Server Error: " . $e->getMessage();
                    return;
                }
            }
        }

        // No route found, return 404
        http_response_code(404);
        echo '404 Not Found';
    }

    /**
     * Convert a route path to a regular expression pattern
     * 
     * @param string $route Route path with parameters (e.g. /users/:id)
     * @return string Regular expression pattern
     */
    private function convertRouteToRegex(string $route): string
    {
        // Convert route parameters to regex patterns
        // e.g. /users/:id -> /users/([^/]+)
        $pattern = preg_replace('/:([^\/]+)/', '([^/]+)', $route);
        $pattern = '#^' . $pattern . '$#';

        return $pattern;
    }

    /**
     * Redirect to another URL
     * 
     * @param string $url Target URL
     * @return void
     */
    public function redirect(string $url): void
    {
        header("Location: $url");
        exit;
    }
}
