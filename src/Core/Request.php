<?php

namespace SecretSanta\Core;

class Request {
    private static ?self $instance = null;
    private array $queryParams;
    private array $postParams;
    private array $serverParams;
    private array $cookieParams;
    private array $files;
    
    private function __construct() {
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
    
    public static function getInstance(): self {
        if (self::$instance === null) {
            self::$instance = new self();
        }
        
        return self::$instance;
    }
    
    public function getQueryParam(string $key, $default = null) {
        return $this->queryParams[$key] ?? $default;
    }
    
    public function getPostParam(string $key, $default = null) {
        return $this->postParams[$key] ?? $default;
    }
    
    public function getServerParam(string $key, $default = null) {
        return $this->serverParams[$key] ?? $default;
    }
    
    public function getCookieParam(string $key, $default = null) {
        return $this->cookieParams[$key] ?? $default;
    }
    
    public function getFile(string $key) {
        return $this->files[$key] ?? null;
    }
    
    public function all(): array {
        return array_merge($this->queryParams, $this->postParams);
    }
    
    /**
     * Get a request parameter (post or query)
     */
    public function get(string $key, $default = null) {
        return $this->postParams[$key] ?? $this->queryParams[$key] ?? $default;
    }
    
    public function getMethod(): string {
        return $this->serverParams['REQUEST_METHOD'];
    }
    
    public function isMethod(string $method): bool {
        return $this->getMethod() === strtoupper($method);
    }
    
    public function getUri(): string {
        return $this->serverParams['REQUEST_URI'];
    }
    
    public function getHeader(string $name, $default = null) {
        $headerName = 'HTTP_' . strtoupper(str_replace('-', '_', $name));
        return $this->serverParams[$headerName] ?? $this->serverParams[$name] ?? $default;
    }
    
    public function isAjax(): bool {
        return $this->getHeader('X-Requested-With') === 'XMLHttpRequest';
    }
    
    public function validate(array $rules): array {
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