<?php

namespace SecretSanta\Config;

/**
 * Database configuration and connection management class
 * 
 * This class implements the Singleton pattern to provide a single point of access
 * to the database connection throughout the application.
 */
class Database
{
    /**
     * Singleton instance of the Database class
     * 
     * @var self|null
     */
    private static ?self $instance = null;
    
    /**
     * The mysqli connection instance
     * 
     * @var \mysqli|null
     */
    private ?\mysqli $connection = null;

    /**
     * Database host
     * 
     * @var string
     */
    private string $host;
    
    /**
     * Database port
     * 
     * @var string
     */
    private string $port;
    
    /**
     * Database name
     * 
     * @var string
     */
    private string $database;
    
    /**
     * Database username
     * 
     * @var string
     */
    private string $username;
    
    /**
     * Database password
     * 
     * @var string
     */
    private string $password;

    /**
     * Private constructor to prevent direct instantiation
     * 
     * Loads database configuration from environment variables, with fallback to default values.
     */
    private function __construct()
    {
        $this->host = getenv('DB_HOST') ?: 'localhost';
        $this->port = getenv('DB_PORT') ?: '3306';
        $this->database = getenv('DB_DATABASE') ?: 'irgendwas_db';
        $this->username = getenv('DB_USERNAME') ?: 'irgendjemand';
        $this->password = getenv('DB_PASSWORD') ?: 'irgendeinpasswort';
    }

    /**
     * Get the singleton instance of the Database class
     * 
     * @return self The Database instance
     */
    public static function getInstance(): self
    {
        if (self::$instance === null) {
            self::$instance = new self();
        }

        return self::$instance;
    }

    /**
     * Get the mysqli connection
     * 
     * Creates a new connection if one doesn't exist already.
     * 
     * @return \mysqli The mysqli connection instance
     * @throws \Exception If the connection fails
     */
    public function getConnection(): \mysqli
    {
        if ($this->connection === null) {
            try {
                // Create mysqli connection
                $this->connection = new \mysqli(
                    $this->host,
                    $this->username,
                    $this->password,
                    $this->database,
                    (int)$this->port
                );

                // Check for connection errors
                if ($this->connection->connect_error) {
                    throw new \Exception("Database connection failed: " . $this->connection->connect_error);
                }

                // Set charset
                $this->connection->set_charset('utf8mb4');

                // Set error reporting mode
                mysqli_report(MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT);
            } catch (\Exception $e) {
                throw new \Exception("Database connection failed: " . $e->getMessage());
            }
        }

        return $this->connection;
    }

    /**
     * Initialize the database schema
     * 
     * Reads SQL statements from schema file and executes them to set up
     * or update the database structure.
     * 
     * @throws \Exception If the schema file is not found or if any SQL statement fails
     */
    public function initialize(): void
    {
        // Get the schema SQL content
        $schemaPath = __DIR__ . '/schema.sql';

        if (!file_exists($schemaPath)) {
            throw new \Exception("Schema file not found at: $schemaPath");
        }

        $schema = file_get_contents($schemaPath);

        if (!$schema) {
            throw new \Exception("Failed to read schema file");
        }

        // Split the SQL by semicolons to execute statements individually
        $statements = array_filter(
            array_map('trim', explode(';', $schema)),
            function ($statement) {
                return !empty($statement);
            }
        );

        try {
            $connection = $this->getConnection();

            // Execute each statement
            foreach ($statements as $statement) {
                if (!$connection->query($statement)) {
                    throw new \Exception("Error executing statement: " . $connection->error);
                }
            }
        } catch (\Exception $e) {
            throw new \Exception("Error initializing database: " . $e->getMessage());
        }
    }
}
