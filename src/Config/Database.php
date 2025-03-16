<?php

namespace SecretSanta\Config;

class Database
{
    private static ?self $instance = null;
    private ?\mysqli $connection = null;

    private string $host;
    private string $port;
    private string $database;
    private string $username;
    private string $password;

    private function __construct()
    {
        $this->host = getenv('DB_HOST') ?: 'localhost';
        $this->port = getenv('DB_PORT') ?: '3306';
        $this->database = getenv('DB_DATABASE') ?: 'irgendwas_db';
        $this->username = getenv('DB_USERNAME') ?: 'irgendjemand';
        $this->password = getenv('DB_PASSWORD') ?: 'irgendeinpasswort';
    }

    public static function getInstance(): self
    {
        if (self::$instance === null) {
            self::$instance = new self();
        }

        return self::$instance;
    }

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
