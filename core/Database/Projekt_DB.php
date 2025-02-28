<?php
namespace core\Database;

/**
 * Main database connection and query handler class
 */
class Projekt_DB {
    private static ?Projekt_DB $instance = null;
    private ?\PDO $connection = null;
    private ?string $lastInsertId = null;
    private int $transactionCount = 0;
    private array $queryLog = [];
    private bool $loggingEnabled = false;
    
    /**
     * Private constructor for singleton pattern
     */
    private function __construct() {
        $this->connect();
    }
    
    /**
     * Get singleton instance
     * @return Projekt_DB Database instance
     */
    public static function getInstance(): Projekt_DB {
        if (self::$instance === null) {
            self::$instance = new self();
        }
        return self::$instance;
    }
    
    /**
     * Connect to the database
     * @param array $overrideConfig Optional array to override default connection settings
     * @throws \Exception if connection fails
     */
    private function connect(array $overrideConfig = []): void {
        try {
            // Use override config or defaults
            $host = $overrideConfig['host'] ?? DB_HOST;
            $port = $overrideConfig['port'] ?? (defined('DB_PORT') ? DB_PORT : 3306);
            $name = $overrideConfig['name'] ?? DB_NAME;
            $user = $overrideConfig['user'] ?? DB_USER;
            $pass = $overrideConfig['pass'] ?? DB_PASS;
            $charset = $overrideConfig['charset'] ?? DB_CHARSET;
            
            // Include port number in DSN
            $dsn = "mysql:host=$host;port=$port;dbname=$name;charset=$charset";
            $options = [
                \PDO::ATTR_ERRMODE => \PDO::ERRMODE_EXCEPTION,
                \PDO::ATTR_DEFAULT_FETCH_MODE => \PDO::FETCH_ASSOC,
                \PDO::ATTR_EMULATE_PREPARES => false,
                \PDO::ATTR_PERSISTENT => true
            ];
            
            // Try to create connection
            $this->connection = new \PDO($dsn, $user, $pass, $options);
        } catch (\PDOException $e) {
            // Enhanced error message with more details
            $message = 'Database connection failed: ' . $e->getMessage();
            $message .= ' (Host: ' . $host . ', Database: ' . $name . ')';
            
            if (defined('DEBUG') && DEBUG) {
                echo '<div style="background: #f8d7da; color: #721c24; padding: 20px; margin: 20px; border-radius: 5px;">';
                echo '<h3>Database Connection Error</h3>';
                echo '<p>' . $message . '</p>';
                echo '<h4>Possible solutions:</h4>';
                echo '<ol>';
                echo '<li>Check if MySQL/MariaDB server is running</li>';
                echo '<li>Verify the database credentials in config.php</li>';
                echo '<li>Make sure the database "' . $name . '" exists</li>';
                echo '<li>Try using "127.0.0.1" instead of "localhost" or vice versa</li>';
                echo '</ol>';
                echo '</div>';
            }
            
            throw new \Exception($message);
        }
    }
    
    /**
     * Get the PDO connection
     * @return \PDO The PDO connection instance
     */
    public function getConnection(): \PDO {
        return $this->connection;
    }
    
    /**
     * Execute a query with parameters
     * 
     * @param string $sql The SQL query
     * @param array $params Parameters for the query
     * @return \PDOStatement|false Statement object or false on failure
     */
    public function execute(string $sql, array $params = []) {
        $startTime = microtime(true);
        try {
            $statement = $this->connection->prepare($sql);
            $statement->execute($params);
            $this->lastInsertId = $this->connection->lastInsertId();
            
            if ($this->loggingEnabled) {
                $this->logQuery($sql, $params, microtime(true) - $startTime);
            }
            
            return $statement;
        } catch (\PDOException $e) {
            $this->handleError($e, $sql, $params);
            return false;
        }
    }
    
    /**
     * Query and fetch all results
     * 
     * @param string $sql The SQL query
     * @param array $params Parameters for the query
     * @param int|null $fetchMode PDO fetch mode
     * @return array Results array
     */
    public function fetchAll(string $sql, array $params = [], ?int $fetchMode = null): array {
        $statement = $this->execute($sql, $params);
        if ($statement && $fetchMode) {
            return $statement->fetchAll($fetchMode);
        } elseif ($statement) {
            return $statement->fetchAll();
        }
        return [];
    }
    
    /**
     * Query and fetch a single row
     * 
     * @param string $sql The SQL query
     * @param array $params Parameters for the query
     * @param int|null $fetchMode PDO fetch mode
     * @return array|false Result array or false if no results
     */
    public function fetch(string $sql, array $params = [], ?int $fetchMode = null) {
        $statement = $this->execute($sql, $params);
        if ($statement && $fetchMode) {
            return $statement->fetch($fetchMode);
        } elseif ($statement) {
            return $statement->fetch();
        }
        return false;
    }
    
    /**
     * Query and fetch a single column value
     * 
     * @param string $sql The SQL query
     * @param array $params Parameters for the query
     * @param int $column Zero-indexed column number
     * @return mixed Column value or false if no results
     */
    public function fetchColumn(string $sql, array $params = [], int $column = 0) {
        $statement = $this->execute($sql, $params);
        if ($statement) {
            return $statement->fetchColumn($column);
        }
        return false;
    }
    
    /**
     * Get the last inserted ID
     * @return string|null The last insert ID or null
     */
    public function lastInsertId(): ?string {
        return $this->lastInsertId;
    }
    
    /**
     * Start a transaction with optional savepoint
     * 
     * @param string|null $savepoint Optional savepoint name
     * @return bool Success status
     */
    public function beginTransaction(?string $savepoint = null): bool {
        if ($savepoint) {
            return $this->execute("SAVEPOINT $savepoint") !== false;
        }
        
        if ($this->transactionCount === 0) {
            $success = $this->connection->beginTransaction();
        } else {
            $success = $this->execute("SAVEPOINT trans{$this->transactionCount}") !== false;
        }
        
        if ($success) {
            $this->transactionCount++;
        }
        
        return $success;
    }
    
    /**
     * Commit a transaction with optional savepoint
     * 
     * @param string|null $savepoint Optional savepoint name
     * @return bool Success status
     */
    public function commit(?string $savepoint = null): bool {
        if ($savepoint) {
            return $this->execute("RELEASE SAVEPOINT $savepoint") !== false;
        }
        
        if ($this->transactionCount === 1) {
            $success = $this->connection->commit();
            if ($success) {
                $this->transactionCount = 0;
            }
            return $success;
        } elseif ($this->transactionCount > 1) {
            $this->transactionCount--;
            return $this->execute("RELEASE SAVEPOINT trans" . ($this->transactionCount - 1)) !== false;
        }
        
        return false;
    }
    
    /**
     * Roll back a transaction with optional savepoint
     * 
     * @param string|null $savepoint Optional savepoint name
     * @return bool Success status
     */
    public function rollBack(?string $savepoint = null): bool {
        if ($savepoint) {
            return $this->execute("ROLLBACK TO SAVEPOINT $savepoint") !== false;
        }
        
        if ($this->transactionCount === 1) {
            $success = $this->connection->rollBack();
            if ($success) {
                $this->transactionCount = 0;
            }
            return $success;
        } elseif ($this->transactionCount > 1) {
            $this->transactionCount--;
            return $this->execute("ROLLBACK TO SAVEPOINT trans" . ($this->transactionCount - 1)) !== false;
        }
        
        return false;
    }
    
    /**
     * Create a query builder instance
     * 
     * @param string $table Table name
     * @return DB_Query Query builder object
     */
    public function table(string $table): DB_Query {
        return new DB_Query($this, $table);
    }
    
    /**
     * Handle database errors
     * 
     * @param \PDOException $e Exception object
     * @param string $sql SQL query that failed
     * @param array $params Query parameters
     */
    private function handleError(\PDOException $e, string $sql, array $params): void {
        $message = 'Database query error: ' . $e->getMessage();
        $context = [
            'sql' => $sql,
            'params' => $params,
            'error' => $e->getMessage(),
            'trace' => $e->getTraceAsString()
        ];
        
        if (defined('DEBUG') && DEBUG) {
            echo '<div style="background: #f8d7da; color: #721c24; padding: 20px; margin: 20px; border-radius: 5px;">';
            echo '<h3>Database Error</h3>';
            echo '<p>' . htmlspecialchars($message) . '</p>';
            echo '<h4>Query</h4>';
            echo '<pre>' . htmlspecialchars($sql) . '</pre>';
            echo '<h4>Parameters</h4>';
            echo '<pre>' . htmlspecialchars(print_r($params, true)) . '</pre>';
            echo '</div>';
        }
        
        // Log the error
        error_log($message . ' ' . json_encode($context, JSON_UNESCAPED_SLASHES));
    }
    
    /**
     * Enable or disable query logging
     * 
     * @param bool $enabled Whether to enable logging
     * @return Projekt_DB Self for chaining
     */
    public function enableLogging(bool $enabled = true): Projekt_DB {
        $this->loggingEnabled = $enabled;
        return $this;
    }
    
    /**
     * Get the query log
     * 
     * @return array Query log entries
     */
    public function getQueryLog(): array {
        return $this->queryLog;
    }
    
    /**
     * Log a database query
     * 
     * @param string $sql SQL query
     * @param array $params Query parameters
     * @param float $duration Query execution time in seconds
     */
    private function logQuery(string $sql, array $params, float $duration): void {
        $this->queryLog[] = [
            'sql' => $sql,
            'params' => $params,
            'duration' => $duration,
            'time' => date('Y-m-d H:i:s')
        ];
    }
    
    // Create database tables if they don't exist
    public function createTables() {
        $queries = [
            // Users table
            "CREATE TABLE IF NOT EXISTS users (
                id INT AUTO_INCREMENT PRIMARY KEY,
                email VARCHAR(255) NOT NULL UNIQUE,
                password VARCHAR(255) NOT NULL,
                first_name VARCHAR(100) NOT NULL,
                last_name VARCHAR(100) NOT NULL,
                reset_token VARCHAR(100) NULL,
                reset_token_expiry DATETIME NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
            )",
            
            // Groups table
            "CREATE TABLE IF NOT EXISTS `groups` (
                id INT AUTO_INCREMENT PRIMARY KEY,
                name VARCHAR(255) NOT NULL,
                description TEXT NULL,
                admin_id INT NOT NULL,
                join_deadline DATETIME NULL,
                draw_date DATETIME NULL,
                is_drawn BOOLEAN DEFAULT FALSE,
                custom_email_template TEXT NULL,
                wishlist_visibility ENUM('all', 'santa_only') DEFAULT 'all',
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
                FOREIGN KEY (admin_id) REFERENCES users(id) ON DELETE CASCADE
            )",
            
            // Group members table
            "CREATE TABLE IF NOT EXISTS group_members (
                id INT AUTO_INCREMENT PRIMARY KEY,
                group_id INT NOT NULL,
                user_id INT NOT NULL,
                invitation_token VARCHAR(100) NULL,
                invitation_email VARCHAR(255) NULL,
                status ENUM('invited', 'active', 'declined') DEFAULT 'invited',
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
                FOREIGN KEY (group_id) REFERENCES `groups`(id) ON DELETE CASCADE,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
                UNIQUE KEY (group_id, user_id)
            )",
            
            // Wishlists table
            "CREATE TABLE IF NOT EXISTS wishlists (
                id INT AUTO_INCREMENT PRIMARY KEY,
                user_id INT NOT NULL,
                group_id INT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (group_id) REFERENCES `groups`(id) ON DELETE CASCADE,
                UNIQUE KEY (user_id, group_id)
            )",
            
            // Wishlist items table
            "CREATE TABLE IF NOT EXISTS wishlist_items (
                id INT AUTO_INCREMENT PRIMARY KEY,
                wishlist_id INT NOT NULL,
                name VARCHAR(255) NOT NULL,
                description TEXT NULL,
                link VARCHAR(512) NULL,
                sort_order INT DEFAULT 0,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
                FOREIGN KEY (wishlist_id) REFERENCES wishlists(id) ON DELETE CASCADE
            )",
            
            // Assignments table
            "CREATE TABLE IF NOT EXISTS assignments (
                id INT AUTO_INCREMENT PRIMARY KEY,
                group_id INT NOT NULL,
                giver_id INT NOT NULL,
                receiver_id INT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
                FOREIGN KEY (group_id) REFERENCES `groups`(id) ON DELETE CASCADE,
                FOREIGN KEY (giver_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (receiver_id) REFERENCES users(id) ON DELETE CASCADE,
                UNIQUE KEY (group_id, giver_id),
                UNIQUE KEY (group_id, receiver_id)
            )",
            
            // Restrictions table
            "CREATE TABLE IF NOT EXISTS restrictions (
                id INT AUTO_INCREMENT PRIMARY KEY,
                group_id INT NOT NULL,
                user_id INT NOT NULL,
                restricted_user_id INT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
                FOREIGN KEY (group_id) REFERENCES `groups`(id) ON DELETE CASCADE,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (restricted_user_id) REFERENCES users(id) ON DELETE CASCADE,
                UNIQUE KEY (group_id, user_id, restricted_user_id)
            )"
        ];
        
        foreach ($queries as $query) {
            $this->execute($query);
        }
    }
}
