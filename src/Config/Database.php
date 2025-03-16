<?php

namespace SecretSanta\Config;

/**
 * Database Connection Management Class
 * 
 * This class manages database connections using the Singleton pattern to ensure
 * only one database connection exists throughout the application. It also provides
 * functionality to initialize the database schema.
 * 
 * @package SecretSanta\Config
 * @version 1.0
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
     * PDO connection instance
     * 
     * @var \PDO|null
     */
    private ?\PDO $connection = null;

    /**
     * Database connection parameters
     * 
     * @var string
     */
    private string $host;
    private string $port;
    private string $database;
    private string $username;
    private string $password;

    /**
     * Private constructor to prevent direct instantiation
     * 
     * Loads database configuration from environment variables 
     * with fallback default values
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
     * Get the PDO connection to the database
     * 
     * Creates a new connection if one doesn't exist yet
     * 
     * @return \PDO The PDO connection object
     * @throws \Exception If connection fails
     */
    public function getConnection(): \PDO
    {
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

    /**
     * Initialize the database structure
     * 
     * Reads the SQL schema file and executes all SQL statements to create
     * the necessary database tables and structure
     * 
     * @throws \Exception If the schema file is missing or database initialization fails
     * @return void
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
            $connection->setAttribute(\PDO::ATTR_EMULATE_PREPARES, 0);

            // Execute each statement
            foreach ($statements as $statement) {
                $connection->exec($statement);
            }
        } catch (\PDOException $e) {
            throw new \Exception("Error initializing database: " . $e->getMessage());
        }
    }
}
