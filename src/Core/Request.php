<?php

namespace SecretSanta\Core;

/**
 * Request class handles HTTP requests and provides methods to access request data.
 * 
 * This singleton class encapsulates and provides access to request parameters,
 * headers, files, and other request information while also offering validation
 * capabilities.
 */
class Request
{
    /** @var self|null Singleton instance of the Request class */
    private static ?self $instance = null;
    
    /** @var array Query parameters from $_GET */
    private array $queryParams;
    
    /** @var array POST parameters from $_POST and JSON request body */
    private array $postParams;
    
    /** @var array Server parameters from $_SERVER */
    private array $serverParams;
    
    /** @var array Cookie parameters from $_COOKIE */
    private array $cookieParams;
    
    /** @var array Uploaded files from $_FILES */
    private array $files;

    /**
     * Private constructor to enforce singleton pattern.
     * 
     * Initializes the request data from PHP superglobals and
     * handles JSON request body if the content-type is application/json.
     */
    private function __construct()
    {
        $this->queryParams = $_GET;
        $this->postParams = $_POST;
        $this->serverParams = $_SERVER;
        $this->cookieParams = $_COOKIE;
        $this->files = $_FILES;

        // Check for JSON input in the request body
        $contentType = $this->getHeader('Content-Type');
        if ($contentType && strpos($contentType, 'application/json') !== false) {
            $json = file_get_contents('php://input');
            if ($json) {
                $data = json_decode($json, true);
                if ($data) {
                    $this->postParams = array_merge($this->postParams, $data);
                }
            }
        }
    }

    /**
     * Gets the singleton instance of the Request class.
     * 
     * @return self The Request instance
     */
    public static function getInstance(): self
    {
        if (self::$instance === null) {
            self::$instance = new self();
        }

        return self::$instance;
    }

    /**
     * Gets a specific query parameter.
     * 
     * @param string $key The parameter name
     * @param mixed $default Default value if parameter doesn't exist
     * @return mixed The parameter value or default if not found
     */
    public function getQueryParam(string $key, $default = null)
    {
        return $this->queryParams[$key] ?? $default;
    }

    /**
     * Gets a specific POST parameter.
     * 
     * @param string $key The parameter name
     * @param mixed $default Default value if parameter doesn't exist
     * @return mixed The parameter value or default if not found
     */
    public function getPostParam(string $key, $default = null)
    {
        return $this->postParams[$key] ?? $default;
    }

    /**
     * Gets a specific server parameter.
     * 
     * @param string $key The parameter name
     * @param mixed $default Default value if parameter doesn't exist
     * @return mixed The parameter value or default if not found
     */
    public function getServerParam(string $key, $default = null)
    {
        return $this->serverParams[$key] ?? $default;
    }

    /**
     * Gets a specific cookie parameter.
     * 
     * @param string $key The cookie name
     * @param mixed $default Default value if cookie doesn't exist
     * @return mixed The cookie value or default if not found
     */
    public function getCookieParam(string $key, $default = null)
    {
        return $this->cookieParams[$key] ?? $default;
    }

    /**
     * Gets uploaded file data.
     * 
     * @param string $key The file field name
     * @return array|null The file data or null if not found
     */
    public function getFile(string $key)
    {
        return $this->files[$key] ?? null;
    }

    /**
     * Gets all request parameters combining POST and GET.
     * 
     * @return array Combined request parameters
     */
    public function all(): array
    {
        return array_merge($this->queryParams, $this->postParams);
    }

    /**
     * Gets a parameter from either POST or GET data.
     * 
     * Checks POST data first, then falls back to GET data if not found.
     * 
     * @param string $key The parameter name
     * @param mixed $default Default value if parameter doesn't exist
     * @return mixed The parameter value or default if not found
     */
    public function get(string $key, $default = null)
    {
        return $this->postParams[$key] ?? $this->queryParams[$key] ?? $default;
    }

    /**
     * Gets the HTTP request method.
     * 
     * @return string The HTTP method (GET, POST, etc.)
     */
    public function getMethod(): string
    {
        return $this->serverParams['REQUEST_METHOD'];
    }

    /**
     * Checks if the request method matches the given method.
     * 
     * @param string $method The method to check against
     * @return bool True if methods match, false otherwise
     */
    public function isMethod(string $method): bool
    {
        return $this->getMethod() === strtoupper($method);
    }

    /**
     * Gets the request URI.
     * 
     * @return string The current request URI
     */
    public function getUri(): string
    {
        return $this->serverParams['REQUEST_URI'];
    }

    /**
     * Gets a specific HTTP header.
     * 
     * @param string $name The header name
     * @param mixed $default Default value if header doesn't exist
     * @return mixed The header value or default if not found
     */
    public function getHeader(string $name, $default = null)
    {
        $headerName = 'HTTP_' . strtoupper(str_replace('-', '_', $name));
        return $this->serverParams[$headerName] ?? $this->serverParams[$name] ?? $default;
    }

    /**
     * Checks if the request is an AJAX request.
     * 
     * @return bool True if it's an AJAX request, false otherwise
     */
    public function isAjax(): bool
    {
        return $this->getHeader('X-Requested-With') === 'XMLHttpRequest';
    }

    /**
     * Validates request data against a set of rules.
     * 
     * Rules can include: required, min:length, max:length, email, url, and same:field
     * 
     * @param array $rules Validation rules as field => rule string
     * @return array Validation errors indexed by field name
     */
    public function validate(array $rules): array
    {
        $errors = [];
        $data = $this->all();

        foreach ($rules as $field => $rule) {
            // Split rule into parts
            $ruleParts = explode('|', $rule);

            foreach ($ruleParts as $rulePart) {
                if ($rulePart === 'required') {
                    if (!isset($data[$field]) || $data[$field] === '') {
                        $errors[$field][] = "$field is required.";
                    }
                } elseif (strpos($rulePart, 'min:') === 0) {
                    $min = (int) substr($rulePart, 4);
                    if (isset($data[$field]) && strlen($data[$field]) < $min) {
                        $errors[$field][] = "$field must be at least $min characters.";
                    }
                } elseif (strpos($rulePart, 'max:') === 0) {
                    $max = (int) substr($rulePart, 4);
                    if (isset($data[$field]) && strlen($data[$field]) > $max) {
                        $errors[$field][] = "$field must not exceed $max characters.";
                    }
                } elseif ($rulePart === 'email') {
                    if (isset($data[$field]) && !filter_var($data[$field], FILTER_VALIDATE_EMAIL)) {
                        $errors[$field][] = "$field must be a valid email address.";
                    }
                } elseif ($rulePart === 'url') {
                    if (isset($data[$field]) && !filter_var($data[$field], FILTER_VALIDATE_URL)) {
                        $errors[$field][] = "$field must be a valid URL.";
                    }
                } elseif (strpos($rulePart, 'same:') === 0) {
                    $otherField = substr($rulePart, 5);
                    if (isset($data[$field]) && isset($data[$otherField]) && $data[$field] !== $data[$otherField]) {
                        $errors[$field][] = "$field must match $otherField.";
                    }
                }
            }
        }

        return $errors;
    }
}
