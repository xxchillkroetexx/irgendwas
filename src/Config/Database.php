<?php

namespace SecretSanta\Config;

class Database {
    private static ?self $instance = null;
    private ?\PDO $connection = null;
    
    private string $host;
    private string $port;
    private string $database;
    private string $username;
    private string $password;
    
    private function __construct() {
        $this->host = getenv('DB_HOST') ?: 'localhost';
        $this->port = getenv('DB_PORT') ?: '3306';
        $this->database = getenv('DB_DATABASE') ?: 'irgendwas_db';
        $this->username = getenv('DB_USERNAME') ?: 'irgendjemand';
        $this->password = getenv('DB_PASSWORD') ?: 'irgendeinpasswort';
    }
    
    public static function getInstance(): self {
        if (self::$instance === null) {
            self::$instance = new self();
        }
        
        return self::$instance;
    }
    
    public function getConnection(): \PDO {
        if ($this->connection === null) {
            try {
                $dsn = "mysql:host={$this->host};port={$this->port};dbname={$this->database};charset=utf8mb4";
                $options = [
                    \PDO::ATTR_ERRMODE => \PDO::ERRMODE_EXCEPTION,
                    \PDO::ATTR_DEFAULT_FETCH_MODE => \PDO::FETCH_ASSOC,
                    \PDO::ATTR_EMULATE_PREPARES => false,
                ];
                
                $this->connection = new \PDO($dsn, $this->username, $this->password, $options);
            } catch (\PDOException $e) {
                throw new \Exception("Database connection failed: " . $e->getMessage());
            }
        }
        
        return $this->connection;
    }
    
    public function initialize(): void {
        $schema = file_get_contents(__DIR__ . '/schema.sql');
        $this->getConnection()->exec($schema);
    }
}